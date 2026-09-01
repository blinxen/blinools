use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};

use anyhow::Context;
use serde::Deserialize;
use tabled::{Table, Tabled};

use crate::config::runtime_dir;
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

pub fn shutdown_vm(binary_path: &Path, api_socket_path: &Path) -> Result<(), anyhow::Error> {
    if api_socket_path.exists() {
        let output = Command::new(binary_path)
            .arg("--api-socket")
            .arg(api_socket_path)
            .arg("shutdown-vmm")
            .output()
            .context("shutting down the sandbox")?;

        if !output.status.success() {
            return Err(anyhow::anyhow!(
                "shutting down the sandbox was not successful"
            ));
        }
    } else {
        eprintln!("Sandbox with the given name does not exist");
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

pub fn list_vms(binary_path: &Path) -> Result<(), anyhow::Error> {
    let entries =
        std::fs::read_dir(runtime_dir()).context("crawling runtime directory for sandboxes")?;
    let mut sandbox_infos: Vec<SandboxInfo> =
        Vec::with_capacity(entries.size_hint().1.unwrap_or(0));
    for entry in entries {
        let sandbox_name = match entry {
            Ok(entry) => entry.file_name(),
            _ => continue,
        };
        let socket_path = runtime_dir().join(&sandbox_name).join(SOCKET_NAME);
        if !socket_path.exists() {
            sandbox_infos.push(SandboxInfo {
                name: sandbox_name.to_string_lossy().to_string(),
                state: String::from("stopped"),
            });
            continue;
        }
        let output = Command::new(binary_path)
            .arg("--api-socket")
            .arg(socket_path)
            .arg("info")
            .output()
            .context("getting coud hypervisor info on sandbox")?;
        if let Ok(ch_info) = serde_json::from_slice::<ChInfo>(&output.stdout) {
            sandbox_infos.push(SandboxInfo {
                name: sandbox_name.to_string_lossy().to_string(),
                state: ch_info.state,
            });
        } else {
            sandbox_infos.push(SandboxInfo {
                name: sandbox_name.to_string_lossy().to_string(),
                state: String::from("unknown"),
            });
        }
    }
    println!("{}", Table::new(sandbox_infos));
    Ok(())
}
