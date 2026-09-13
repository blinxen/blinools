use std::io::{Read, Write};
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus};
use std::time::Duration;

use anyhow::Context;
use serde::Deserialize;

use crate::sandbox::config::Config;
use crate::sandbox::hypervisor::{
    Hypervisor, State, Vm, VmConfig, can_connect_to_socket, create_qcow2_overlay, guest_cmdline,
};
use crate::sandbox::process::{die_with_parent, kill_child_and_cleanup, remove_stale_socket};
use crate::sandbox::{socket_path, socket_path_in};

const API_TIMEOUT: Duration = Duration::from_secs(5);
const MAX_API_RESPONSE_LENGTH: usize = 64 * 1024;
pub const SOCKET_NAME: &str = "cloud-hypervisor";

pub struct CloudHypervisor {
    binary: PathBuf,
}

impl CloudHypervisor {
    pub fn new(config: Option<&Config>) -> Self {
        let mut binary = PathBuf::from("cloud-hypervisor");
        if let Some(config) = config
            && let Some(cloud_hypervisor) = config.cloud_hypervisor.as_ref()
            && let Some(configured) = cloud_hypervisor.binary.as_ref()
        {
            binary = configured.to_path_buf();
        }
        CloudHypervisor { binary }
    }
}

impl Hypervisor for CloudHypervisor {
    fn boot(&self, cfg: VmConfig) -> Result<Box<dyn Vm>, anyhow::Error> {
        let mut mounts: Vec<String> = Vec::new();
        let cmdline = guest_cmdline(&cfg);

        for mount in cfg.mounts {
            mounts.push("--fs".into());
            mounts.push(format!(
                "tag={},socket={},num_queues=1,queue_size=512",
                mount.tag,
                mount.socket_path.display()
            ));
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

        if let Some(network_socket) = cfg.network_socket {
            command.arg("--net").arg(format!(
                "vhost_user=true,socket={}",
                network_socket.display()
            ));
        }

        if log::log_enabled!(log::Level::Trace) {
            command.arg("-vvv");
        } else if log::log_enabled!(log::Level::Debug) {
            command.arg("-vv");
        } else if log::log_enabled!(log::Level::Info) {
            command.arg("-v");
        };

        die_with_parent(&mut command);

        log::debug!("Starting command: {:?}", command);
        let handle = command.spawn().context("spawning cloud-hypervisor")?;

        Ok(Box::new(CloudHypervisorVm {
            socket_path: api_socket,
            handle,
        }))
    }

    fn is_running(&self, sandbox_runtime_dir: &Path) -> bool {
        can_connect_to_socket(&socket_path_in(sandbox_runtime_dir, SOCKET_NAME))
    }

    fn shutdown(&self, sandbox_runtime_dir: &Path, force: bool) -> Result<(), anyhow::Error> {
        let api_socket_path = socket_path_in(sandbox_runtime_dir, SOCKET_NAME);
        if can_connect_to_socket(&api_socket_path) {
            let vm_response = api(&api_socket_path, "PUT", "vm.shutdown", None)
                .context("requesting cloud hypervisor to shut down the sandbox")?;

            if !force && !vm_response.success() {
                return Err(anyhow::anyhow!(
                    "shutting down the sandbox was not successful"
                ));
            }

            let vmm_response = api(&api_socket_path, "PUT", "vmm.shutdown", None)
                .context("requesting cloud hypervisor to shut down the sandbox")?;

            if !vmm_response.success() {
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
                    Ok(info) => State::Running(info.state.to_uppercase()),
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
        let hypervisor = CloudHypervisor::new(None);

        assert!(hypervisor.is_running(dir.path()));
        assert_eq!(hypervisor.state(dir.path()).to_string(), "Running");
    }

    #[test]
    fn a_sandbox_without_a_socket_is_stopped() {
        let dir = tempdir().unwrap();
        let hypervisor = CloudHypervisor::new(None);

        assert!(!hypervisor.is_running(dir.path()));
        assert!(matches!(hypervisor.state(dir.path()), State::Stopped));
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
