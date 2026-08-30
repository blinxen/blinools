use std::path::Path;
use std::process::{Child, Command, Stdio};

use anyhow::Context;

use crate::sandbox::create_socket_path;
use crate::sandbox::fs::FsMount;

pub struct VmConfig<'sandbox> {
    pub name: &'sandbox str,
    pub binary: &'sandbox Path,
    pub kernel: &'sandbox Path,
    pub rootfs: &'sandbox Path,
    pub network_socket: &'sandbox Path,
    pub cmdline: String,
    pub memory_mb: u64,
    pub cpus: u8,
    pub mounts: &'sandbox Vec<FsMount>,
}

pub fn create_vm(cfg: VmConfig) -> Result<Child, anyhow::Error> {
    let mut mounts: Vec<String> = Vec::new();
    let mut cmdline = String::from("console=hvc0 root=/dev/vda ");
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

    // TODO: Control whether the args are good enough here
    // We probably also want to expose a socket here
    let handle = Command::new(cfg.binary)
        .arg("--api-socket")
        .arg(create_socket_path(cfg.name, "cloud-hypervisor.sock"))
        .arg("--kernel")
        .arg(cfg.kernel)
        .arg("--disk")
        .arg(format!("path={},image_type=raw", cfg.rootfs.display()))
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
