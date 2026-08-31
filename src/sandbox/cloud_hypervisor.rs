use std::path::Path;
use std::process::{Child, Command, Stdio};

use anyhow::Context;
use serde::Deserialize;
use tabled::{Table, Tabled};

use crate::config::runtime_dir;
use crate::sandbox::config::RootfsType;
use crate::sandbox::create_socket_path;
use crate::sandbox::fs::FsMount;

pub const SOCKET_NAME: &str = "cloud-hypervisor.sock";

pub struct VmConfig<'sandbox> {
    pub name: &'sandbox str,
    pub binary: &'sandbox Path,
    pub kernel: &'sandbox Path,
    pub rootfs: &'sandbox Path,
    pub rootfs_type: RootfsType,
    pub network_socket: &'sandbox Path,
    pub cmdline: String,
    pub memory_mb: u64,
    pub cpus: u8,
    pub mounts: &'sandbox Vec<FsMount>,
}

pub fn create_vm(cfg: VmConfig) -> Result<Child, anyhow::Error> {
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

    let disk = match cfg.rootfs_type {
        RootfsType::RAW => format!(
            "path={},image_type=raw",
            cfg.rootfs.display()
        ),
        RootfsType::QCOW2 => format!(
            "path={},image_type=qcow2,backing_files=on",
            cfg.rootfs.display()
        ),
    };

    // TODO: Control whether the args are good enough here
    // We probably also want to expose a socket here
    let handle = Command::new(cfg.binary)
        .arg("--api-socket")
        .arg(create_socket_path(cfg.name, SOCKET_NAME))
        .arg("--kernel")
        .arg(cfg.kernel)
        .arg("--disk")
        .arg(disk)
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

    Ok(handle)
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
        let output = Command::new(binary_path)
            .arg("--api-socket")
            .arg(runtime_dir().join(&sandbox_name).join(SOCKET_NAME))
            .arg("info")
            .output()
            .context("getting coud hypervisor info on sandbox")?;
        let ch_info: ChInfo = serde_json::from_slice(&output.stdout).context("")?;
        sandbox_infos.push(SandboxInfo {
            name: sandbox_name.to_string_lossy().to_string(),
            state: ch_info.state,
        });
    }
    println!("{}", Table::new(sandbox_infos));
    Ok(())
}
