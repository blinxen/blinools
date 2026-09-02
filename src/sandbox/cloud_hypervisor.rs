use std::io::{Read, Write};
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};

use anyhow::Context;
use serde::Deserialize;
use tabled::{Table, Tabled};

use crate::sandbox::config::RootfsType;
use crate::sandbox::fs::FsMount;
use crate::sandbox::{create_qcow2_overlay, create_socket_path};

pub const SOCKET_NAME: &str = "cloud-hypervisor.sock";

pub struct CloudHypervisorVmConfig<'sandbox> {
    pub name: &'sandbox str,
    pub binary: &'sandbox Path,
    pub kernel: &'sandbox Path,
    pub rootfs: &'sandbox Path,
    pub rootfs_type: RootfsType,
    pub reset_overlay: bool,
    pub network_socket: &'sandbox Path,
    pub cmdline: String,
    pub memory_mb: u64,
    pub cpus: u8,
    pub mounts: &'sandbox Vec<FsMount>,
}
pub struct CloudHypervisor {
    socket_path: PathBuf,
    handle: Child,
}

impl CloudHypervisor {
    pub fn block_until_vm_shutsdown(&mut self) -> Result<std::process::ExitStatus, std::io::Error> {
        self.handle.wait()
    }
}

impl Drop for CloudHypervisor {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.socket_path);
        let _ = std::fs::remove_file(self.socket_path.with_added_extension("lock"));
    }
}

pub fn create_vm(cfg: &CloudHypervisorVmConfig) -> Result<CloudHypervisor, anyhow::Error> {
    let mut mounts: Vec<String> = Vec::new();
    let mut cmdline = format!(
        "console=hvc0 root=/dev/vda rw systemd.hostname={} ",
        cfg.name
    );
    cmdline.push_str(&cfg.cmdline);

    for mount in cfg.mounts {
        mounts.push("--fs".into());
        mounts.push(format!(
            "tag={},socket={},num_queues=1,queue_size=512",
            mount.tag,
            mount.socket_path.display()
        ));
        cmdline.push_str(" systemd.mount-extra=");
        cmdline.push_str(&mount.tag);
        cmdline.push_str(":/mnt/");
        cmdline.push_str(&mount.tag);
        cmdline.push_str(":virtiofs:");
        if mount.read_only {
            cmdline.push_str("ro");
        } else {
            cmdline.push_str("rw");
        }
    }

    let socket_path = create_socket_path(cfg.name, SOCKET_NAME);
    let handle = Command::new(cfg.binary)
        .arg("--api-socket")
        .arg(&socket_path)
        .arg("--kernel")
        .arg(cfg.kernel)
        .arg("--landlock")
        .arg("--landlock-rules")
        .arg(format!("path={},access=r", cfg.rootfs.display()))
        .arg("--disk")
        .arg(format!(
            "path={},image_type=qcow2,backing_files=on",
            create_qcow2_overlay(cfg)?.display()
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
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .spawn()
        .context("spawning cloud-hypervisor")?;

    Ok(CloudHypervisor {
        socket_path,
        handle,
    })
}

pub fn shutdown_vm(api_socket_path: &Path) -> Result<(), anyhow::Error> {
    if can_connect_to_socket(api_socket_path) {
        let response = api(api_socket_path, "PUT", "vmm.shutdown", None)
            .context("requesting cloud hypervisor to shut down the sandbox")?;

        if !response.success() {
            return Err(anyhow::anyhow!(
                "shutting down the sandbox was not successful"
            ));
        }
    }

    Ok(())
}

#[derive(Debug, Deserialize)]
pub struct ChInfo {
    pub state: String,
}

#[derive(Tabled)]
pub struct SandboxInfo {
    pub name: String,
    pub state: String,
}

pub fn list_vms(vm_base_dir: &Path) -> Result<(), anyhow::Error> {
    let entries =
        std::fs::read_dir(vm_base_dir).context("crawling runtime directory for sandboxes")?;
    let mut sandbox_infos: Vec<SandboxInfo> =
        Vec::with_capacity(entries.size_hint().1.unwrap_or(0));
    for entry in entries {
        let sandbox_name = match entry {
            Ok(entry) => entry.file_name().display().to_string(),
            _ => continue,
        };
        let socket_path = vm_base_dir.join(&sandbox_name).join(SOCKET_NAME);
        if !can_connect_to_socket(&socket_path) {
            sandbox_infos.push(SandboxInfo {
                name: sandbox_name,
                state: String::from("Stopped"),
            });
            continue;
        }
        let response = api(&socket_path, "GET", "vm.info", None)
            .context("requesting cloud hypervisor for information about the sandbox")?;
        if response.success()
            && let Ok(ch_info) = serde_json::from_str::<ChInfo>(&response.body)
        {
            sandbox_infos.push(SandboxInfo {
                name: sandbox_name,
                state: ch_info.state,
            });
        } else {
            sandbox_infos.push(SandboxInfo {
                name: sandbox_name,
                state: String::from("Unknown"),
            });
        }
    }
    println!("{}", Table::new(sandbox_infos));
    Ok(())
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

pub fn can_connect_to_socket(socket_path: &Path) -> bool {
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
    let mut stream =
        UnixStream::connect(socket_path).context("connecting to cloud hypervisor socket")?;
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
        // Read until there are no bytes left or we have received the full body (according to
        // content-length)
        let count = stream
            .read(&mut bytes)
            .context("reading response bytes from cloud hypervisor")?;
        if count == 0 {
            break;
        }
        response.extend_from_slice(&bytes[..count]);

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
