mod cloud_hypervisor;
mod qemu;

use std::fmt;
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::process::ExitStatus;

use anyhow::Context;
use imago::format::drivers::FormatDriverInstance;
use imago::qcow2::{Qcow2, Qcow2CreateBuilder};
use imago::{FormatCreateBuilder, Storage};

use crate::config::state_dir;
use crate::sandbox::config::{self, Config, RootfsType};
use crate::sandbox::console::ConsoleIo;
use crate::sandbox::fs::FsMount;
use crate::sandbox::hypervisor::cloud_hypervisor::CloudHypervisor;
use crate::sandbox::hypervisor::qemu::Qemu;
use crate::sandbox::name::Name;

pub struct VmConfig<'sandbox> {
    pub name: &'sandbox Name,
    pub kernel: &'sandbox Path,
    pub rootfs: &'sandbox Path,
    pub rootfs_type: &'sandbox RootfsType,
    pub reset_overlay: bool,
    pub network_socket: Option<&'sandbox Path>,
    pub cmdline: &'sandbox str,
    pub memory_mb: u64,
    pub cpus: u8,
    pub mounts: &'sandbox [FsMount],
    pub console: ConsoleIo,
}

pub enum State {
    Running(String),
    Stopped,
    Unknown,
}

impl fmt::Display for State {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            State::Running(state) => f.write_str(state),
            State::Stopped => f.write_str("Stopped"),
            State::Unknown => f.write_str("Unknown"),
        }
    }
}

pub trait Vm {
    fn wait(&mut self) -> Result<ExitStatus, std::io::Error>;
    fn terminate(&mut self);
}

pub trait Hypervisor {
    fn boot(&self, config: VmConfig) -> Result<Box<dyn Vm>, anyhow::Error>;

    fn is_running(&self, sandbox_runtime_dir: &Path) -> bool;

    fn shutdown(&self, sandbox_runtime_dir: &Path, force: bool) -> Result<(), anyhow::Error>;

    fn state(&self, sandbox_runtime_dir: &Path) -> State;
}

pub fn new(config: Option<&Config>) -> Box<dyn Hypervisor> {
    if let Some(config) = config
        && config.hypervisor == config::Hypervisor::Qemu
    {
        Box::new(Qemu::new(Some(config)))
    } else {
        Box::new(CloudHypervisor::new(config))
    }
}

pub fn for_sandbox(config: Option<&Config>, sandbox_runtime_dir: &Path) -> Box<dyn Hypervisor> {
    let candidates: [Box<dyn Hypervisor>; 2] = [
        Box::new(CloudHypervisor::new(config)),
        Box::new(Qemu::new(config)),
    ];

    for candidate in candidates {
        if candidate.is_running(sandbox_runtime_dir) {
            return candidate;
        }
    }

    new(config)
}

pub fn socket_paths_to_validate() -> Vec<&'static str> {
    vec![cloud_hypervisor::SOCKET_NAME, qemu::SOCKET_NAME]
}

fn guest_cmdline(cfg: &VmConfig) -> String {
    let mut cmdline = format!(
        "console=hvc0 root=/dev/vda rw systemd.hostname={} ",
        cfg.name
    );
    cmdline.push_str(cfg.cmdline);

    for mount in cfg.mounts {
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

    cmdline
}

fn can_connect_to_socket(socket_path: &Path) -> bool {
    socket_path.exists() && UnixStream::connect(socket_path).is_ok()
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

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    const VIRTUAL_SIZE: u64 = 5 * 1024 * 1024 * 1024;

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
}
