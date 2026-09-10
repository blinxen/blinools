use std::io::{Read, Write};
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus};
use std::time::Duration;

use anyhow::Context;
use imago::format::drivers::FormatDriverInstance;
use imago::qcow2::Qcow2;
use imago::{FormatCreateBuilder, Storage, qcow2::Qcow2CreateBuilder};
use serde::Deserialize;

use crate::config::state_dir;
use crate::sandbox::config::RootfsType;
use crate::sandbox::hypervisor::{Hypervisor, State, Vm, VmConfig};
use crate::sandbox::process::{die_with_parent, kill_child_and_cleanup, remove_stale_socket};
use crate::sandbox::{socket_path, socket_path_in};

const API_TIMEOUT: Duration = Duration::from_secs(5);
const MAX_API_RESPONSE_LENGTH: usize = 64 * 1024;
pub const SOCKET_NAME: &str = "cloud-hypervisor";

pub struct CloudHypervisor {
    binary: PathBuf,
}

impl CloudHypervisor {
    pub fn new(binary: PathBuf) -> Self {
        CloudHypervisor { binary }
    }
}

impl Hypervisor for CloudHypervisor {
    fn boot(&self, cfg: VmConfig) -> Result<Box<dyn Vm>, anyhow::Error> {
        let mut mounts: Vec<String> = Vec::new();
        let mut cmdline = format!(
            "console=hvc0 root=/dev/vda rw systemd.hostname={} ",
            cfg.name
        );
        cmdline.push_str(cfg.cmdline);

        for mount in cfg.mounts {
            mounts.push("--fs".into());
            mounts.push(format!(
                "tag={},socket={},num_queues=1,queue_size=512",
                mount.tag,
                mount.socket_path.display()
            ));
            cmdline.push_str(" systemd.mount-extra=");
            cmdline.push_str(mount.tag.as_str());
            cmdline.push_str(":/mnt/");
            cmdline.push_str(mount.tag.as_str());
            cmdline.push_str(":virtiofs:");
            if mount.read_only {
                cmdline.push_str("ro");
            } else {
                cmdline.push_str("rw");
            }
        }

        let api_socket = socket_path(cfg.name, SOCKET_NAME);
        remove_stale_socket(&api_socket);
        remove_stale_socket(&api_socket.with_added_extension("lock"));
        let mut command = Command::new(&self.binary);
        command
            .arg("--api-socket")
            .arg(&api_socket)
            .arg("--log-file")
            .arg(api_socket.with_extension("log"))
            .arg("--kernel")
            .arg(cfg.kernel)
            .arg("--landlock")
            .arg("--landlock-rules")
            .arg(format!("path={},access=r", cfg.rootfs.display()))
            .arg("--disk")
            .arg(format!(
                "path={},image_type=qcow2,backing_files=on",
                create_qcow2_overlay(&cfg)?.display()
            ))
            .args(mounts)
            .arg("--cmdline")
            .arg(cmdline)
            .arg("--net")
            .arg(format!(
                "vhost_user=true,socket={}",
                cfg.network_socket.display()
            ))
            .arg("--cpus")
            .arg(format!("boot={}", cfg.cpus))
            .arg("--memory")
            .arg(format!("size={}M,shared=on", cfg.memory_mb))
            .arg("--serial")
            .arg("off")
            .arg("--console")
            .arg("tty")
            .stdin(cfg.console.stdin)
            .stdout(cfg.console.stdout)
            .stderr(cfg.console.stderr);

        if log::log_enabled!(log::Level::Trace) {
            command.arg("-vvv");
        } else if log::log_enabled!(log::Level::Debug) {
            command.arg("-vv");
        } else if log::log_enabled!(log::Level::Info) {
            command.arg("-v");
        };

        die_with_parent(&mut command);

        let handle = command.spawn().context("spawning cloud-hypervisor")?;

        Ok(Box::new(CloudHypervisorVm {
            socket_path: api_socket,
            handle,
        }))
    }

    fn is_running(&self, sandbox_runtime_dir: &Path) -> bool {
        can_connect_to_socket(&socket_path_in(sandbox_runtime_dir, SOCKET_NAME))
    }

    fn shutdown(&self, sandbox_runtime_dir: &Path) -> Result<(), anyhow::Error> {
        let api_socket_path = socket_path_in(sandbox_runtime_dir, SOCKET_NAME);
        if can_connect_to_socket(&api_socket_path) {
            let response = api(&api_socket_path, "PUT", "vmm.shutdown", None)
                .context("requesting cloud hypervisor to shut down the sandbox")?;

            if !response.success() {
                return Err(anyhow::anyhow!(
                    "shutting down the sandbox was not successful"
                ));
            }
        }

        Ok(())
    }

    fn state(&self, sandbox_runtime_dir: &Path) -> State {
        let api_socket_path = socket_path_in(sandbox_runtime_dir, SOCKET_NAME);
        if !can_connect_to_socket(&api_socket_path) {
            return State::Stopped;
        }

        match api(&api_socket_path, "GET", "vm.info", None) {
            Ok(response) if response.success() => {
                match serde_json::from_str::<ChInfo>(&response.body) {
                    Ok(info) => State::Running(info.state),
                    Err(_) => State::Unknown,
                }
            }
            _ => State::Unknown,
        }
    }
}

pub struct CloudHypervisorVm {
    socket_path: PathBuf,
    handle: Child,
}

impl Vm for CloudHypervisorVm {
    fn wait(&mut self) -> Result<ExitStatus, std::io::Error> {
        self.handle.wait()
    }

    fn terminate(&mut self) {
        kill_child_and_cleanup(
            &mut self.handle,
            &[
                &self.socket_path,
                &self.socket_path.with_added_extension("lock"),
            ],
        );
    }
}

impl Drop for CloudHypervisorVm {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.socket_path);
        let _ = std::fs::remove_file(self.socket_path.with_added_extension("lock"));
    }
}

fn create_qcow2_overlay(cfg: &VmConfig) -> Result<PathBuf, anyhow::Error> {
    let qcow2_path = state_dir()?
        .join(cfg.name)
        .join("backing_file")
        .with_extension(RootfsType::QCOW2.to_string());

    if qcow2_path.exists() && !cfg.reset_overlay {
        ensure_overlay_backs_onto_rootfs(&qcow2_path, cfg)?;
        return Ok(qcow2_path);
    }

    let rootfs_size = rootfs_virtual_size(cfg.rootfs, cfg.rootfs_type)?;
    let image_file = imago::file::File::create_open(
        imago::StorageCreateOptions::new()
            .filename(&qcow2_path)
            .overwrite(true),
    )
    .context("creating qcow2 overlay file")?;

    Qcow2CreateBuilder::<imago::file::File>::new(image_file)
        .size(rootfs_size)
        .backing(
            cfg.rootfs.display().to_string(),
            cfg.rootfs_type.to_string(),
        )
        .create()
        .context("formatting qcow2 image")?;

    Ok(qcow2_path)
}

fn rootfs_virtual_size(rootfs: &Path, rootfs_type: &RootfsType) -> Result<u64, anyhow::Error> {
    match rootfs_type {
        RootfsType::Raw => Ok(std::fs::metadata(rootfs)
            .context("calculating overlay size from rootfs")?
            .len()),
        RootfsType::QCOW2 => Ok(open_qcow2(rootfs)?.size()),
    }
}

fn ensure_overlay_backs_onto_rootfs(
    qcow2_path: &Path,
    cfg: &VmConfig,
) -> Result<(), anyhow::Error> {
    let overlay = open_qcow2(qcow2_path)?;
    let recorded = (
        overlay.implicit_backing_file().map(String::as_str),
        overlay.implicit_backing_format().map(String::as_str),
    );
    let rootfs = cfg.rootfs.display().to_string();
    let rootfs_type = cfg.rootfs_type.to_string();

    if recorded != (Some(rootfs.as_str()), Some(rootfs_type.as_str())) {
        return Err(anyhow::anyhow!(
            "the overlay of sandbox `{}` was created from `{}` ({}), but the configured rootfs is \
             `{rootfs}` ({rootfs_type}). Pass --recreate to build a new overlay, which discards \
             everything the sandbox has written so far.",
            cfg.name,
            recorded.0.unwrap_or("no backing file"),
            recorded.1.unwrap_or("unknown format"),
        ));
    }

    Ok(())
}

fn open_qcow2(path: &Path) -> Result<Qcow2<imago::file::File>, anyhow::Error> {
    Qcow2::open_path(path, false).with_context(|| format!("opening qcow2 image {}", path.display()))
}

#[derive(Debug, Deserialize)]
struct ChInfo {
    pub state: String,
}

#[derive(Debug)]
struct ApiResponse {
    pub status_code: u16,
    pub body: String,
}

impl ApiResponse {
    pub fn success(&self) -> bool {
        (200..300).contains(&self.status_code)
    }
}

fn can_connect_to_socket(socket_path: &Path) -> bool {
    socket_path.exists() && UnixStream::connect(socket_path).is_ok()
}

// See
// https://github.com/cloud-hypervisor/cloud-hypervisor/blob/main/vmm/src/api/openapi/cloud-hypervisor.yaml
fn api(
    socket_path: &Path,
    method: &str,
    path: &str,
    body: Option<&str>,
) -> Result<ApiResponse, anyhow::Error> {
    api_with_timeout(socket_path, method, path, body, API_TIMEOUT)
}

fn api_with_timeout(
    socket_path: &Path,
    method: &str,
    path: &str,
    body: Option<&str>,
    timeout: Duration,
) -> Result<ApiResponse, anyhow::Error> {
    let mut stream =
        UnixStream::connect(socket_path).context("connecting to cloud hypervisor socket")?;
    stream
        .set_read_timeout(Some(timeout))
        .context("setting a read timeout on the cloud hypervisor socket")?;
    stream
        .set_write_timeout(Some(timeout))
        .context("setting a write timeout on the cloud hypervisor socket")?;
    // Request
    let mut request =
        format!("{method} /api/v1/{path} HTTP/1.1\r\nHost: localhost\r\nAccept: */*\r\n");
    if let Some(body) = body {
        request.push_str(&format!("Content-Length: {}\r\n", body.len()));
    }
    request.push_str("\r\n");
    if let Some(body) = body {
        request.push_str(body);
    }
    stream
        .write_all(request.as_bytes())
        .context("sending request to cloud hypervisor")?;
    stream
        .flush()
        .context("flushing request to cloud hypervisor")?;
    // Response
    let mut response = Vec::new();
    let mut body = String::new();
    let mut status_code = 0;
    loop {
        let mut bytes = vec![0; 256];
        // Read until there are no bytes left or we have received the full body
        // (according to content-length)
        let count = stream
            .read(&mut bytes)
            .context("reading response bytes from cloud hypervisor")?;
        if count == 0 {
            break;
        }
        response.extend_from_slice(&bytes[..count]);
        if response.len() > MAX_API_RESPONSE_LENGTH {
            return Err(anyhow::anyhow!(
                "cloud hypervisor answered with more than {MAX_API_RESPONSE_LENGTH} bytes"
            ));
        }

        // To parse the body we need content-length header
        // header parsing starts when we see \r\n\r\n since HTTP 1.1 defines that as the separator
        // between headers and body
        if let Some(header_end) = response.windows(4).position(|window| window == b"\r\n\r\n") {
            let body_offset = header_end + 4;

            let headers = std::str::from_utf8(&response[..header_end])
                .context("parsing HTTP headers as UTF-8")?;

            // Status code is not a header but we include the first line into the headers array
            // (not 100% correct but we don't care here)
            status_code = get_status_code(headers)?;
            if status_code == 204 {
                // No body available for this status code
                break;
            }

            let content_length = get_header(headers, "Content-Length")
                .context("looking for Content-Length header")?
                .trim()
                .parse::<usize>()
                .context("parsing Content-Length header")?;

            if response.len() >= body_offset + content_length {
                body =
                    String::from_utf8(response[body_offset..body_offset + content_length].to_vec())
                        .context("parsing HTTP body as valid UTF-8")?;
                break;
            }
        }
    }

    Ok(ApiResponse { status_code, body })
}

fn get_header<'a>(response: &'a str, header: &'a str) -> Option<&'a str> {
    response.lines().find_map(|line| {
        let (name, value) = line.split_once(':')?;

        if name.eq_ignore_ascii_case(header) {
            Some(value.trim())
        } else {
            None
        }
    })
}

fn get_status_code(headers: &str) -> Result<u16, anyhow::Error> {
    headers
        .lines()
        .next()
        .unwrap_or_default()
        .split_whitespace()
        .nth(1)
        .context("parsing response for status code")?
        .parse::<u16>()
        .context("parsing response for status code")
}

#[cfg(test)]
mod tests {
    use std::os::unix::net::UnixListener;
    use std::time::Instant;

    use crate::sandbox::socket_path_in;

    use super::*;
    use tempfile::tempdir;

    const VIRTUAL_SIZE: u64 = 5 * 1024 * 1024 * 1024;
    const TEST_TIMEOUT: Duration = Duration::from_millis(200);

    fn serve(socket_path: &Path, answer: fn(UnixStream)) {
        let listener = UnixListener::bind(socket_path).unwrap();
        std::thread::spawn(move || {
            while let Ok((stream, _)) = listener.accept() {
                answer(stream);
            }
        });
    }

    fn answer_vm_info(mut stream: UnixStream) {
        let mut request = Vec::new();
        let mut byte = [0u8; 1];
        while !request.ends_with(b"\r\n\r\n") {
            match stream.read(&mut byte) {
                Ok(0) | Err(_) => return,
                Ok(_) => request.extend_from_slice(&byte),
            }
        }

        let body = r#"{"state":"Running"}"#;
        let _ = stream.write_all(
            format!(
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\n\r\n{body}",
                body.len()
            )
            .as_bytes(),
        );
    }

    #[test]
    fn the_state_is_read_from_the_socket_boot_creates() {
        let dir = tempdir().unwrap();
        serve(&socket_path_in(dir.path(), SOCKET_NAME), answer_vm_info);
        let hypervisor = CloudHypervisor::new(PathBuf::from("cloud-hypervisor"));

        assert!(hypervisor.is_running(dir.path()));
        assert_eq!(hypervisor.state(dir.path()).to_string(), "Running");
    }

    #[test]
    fn a_sandbox_without_a_socket_is_stopped() {
        let dir = tempdir().unwrap();
        let hypervisor = CloudHypervisor::new(PathBuf::from("cloud-hypervisor"));

        assert!(!hypervisor.is_running(dir.path()));
        assert!(matches!(hypervisor.state(dir.path()), State::Stopped));
    }

    fn create_qcow2(path: &Path, size: u64, backing: Option<(&str, &str)>) {
        let file = imago::file::File::create_open(
            imago::StorageCreateOptions::new()
                .filename(path)
                .overwrite(true),
        )
        .unwrap();
        let mut builder = Qcow2CreateBuilder::<imago::file::File>::new(file).size(size);
        if let Some((name, format)) = backing {
            builder = builder.backing(name.to_string(), format.to_string());
        }
        builder.create().unwrap();
    }

    #[test]
    fn a_qcow2_rootfs_is_sized_from_its_virtual_size() {
        let dir = tempdir().unwrap();
        let rootfs = dir.path().join("rootfs.qcow2");
        create_qcow2(&rootfs, VIRTUAL_SIZE, None);

        assert!(std::fs::metadata(&rootfs).unwrap().len() < VIRTUAL_SIZE);
        assert_eq!(
            rootfs_virtual_size(&rootfs, &RootfsType::QCOW2).unwrap(),
            VIRTUAL_SIZE
        );
    }

    #[test]
    fn a_raw_rootfs_is_sized_from_its_file_length() {
        let dir = tempdir().unwrap();
        let rootfs = dir.path().join("rootfs.img");
        std::fs::write(&rootfs, [0u8; 512]).unwrap();

        assert_eq!(rootfs_virtual_size(&rootfs, &RootfsType::Raw).unwrap(), 512);
    }

    #[test]
    fn an_overlay_reports_the_rootfs_it_was_created_from() {
        let dir = tempdir().unwrap();
        let overlay = dir.path().join("backing_file.qcow2");
        create_qcow2(
            &overlay,
            VIRTUAL_SIZE,
            Some(("/images/fedora.img", &RootfsType::Raw.to_string())),
        );

        let opened = open_qcow2(&overlay).unwrap();

        assert_eq!(
            opened.implicit_backing_file().map(String::as_str),
            Some("/images/fedora.img")
        );
        assert_eq!(
            opened.implicit_backing_format().map(String::as_str),
            Some(RootfsType::Raw.to_string().as_str())
        );
    }

    #[test]
    fn a_vmm_that_never_answers_times_out() {
        let dir = tempdir().unwrap();
        let socket_path = dir.path().join("ch.sock");
        serve(&socket_path, |stream| {
            std::thread::sleep(Duration::from_secs(30));
            drop(stream);
        });

        let started = Instant::now();
        let error = api_with_timeout(&socket_path, "GET", "vm.info", None, TEST_TIMEOUT)
            .expect_err("reading from a silent VMM has to fail");

        assert!(started.elapsed() < Duration::from_secs(5), "{error}");
    }

    #[test]
    fn an_endless_response_is_cut_off() {
        let dir = tempdir().unwrap();
        let socket_path = dir.path().join("ch.sock");
        serve(&socket_path, |mut stream| {
            let _ = stream.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 999999999\r\n\r\n");
            while stream.write_all(&[b'x'; 4096]).is_ok() {}
        });

        let error = api_with_timeout(&socket_path, "GET", "vm.info", None, TEST_TIMEOUT)
            .expect_err("an endless response has to fail");

        assert!(
            error
                .to_string()
                .contains(&MAX_API_RESPONSE_LENGTH.to_string()),
            "{error}"
        );
    }
}
