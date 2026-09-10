pub mod cloud_hypervisor;

use std::fmt;
use std::path::{Path, PathBuf};
use std::process::ExitStatus;

use crate::sandbox::config::{Config, RootfsType};
use crate::sandbox::console::ConsoleIo;
use crate::sandbox::fs::FsMount;
use crate::sandbox::hypervisor::cloud_hypervisor::CloudHypervisor;
use crate::sandbox::name::Name;

pub struct VmConfig<'sandbox> {
    pub name: &'sandbox Name,
    pub kernel: &'sandbox Path,
    pub rootfs: &'sandbox Path,
    pub rootfs_type: &'sandbox RootfsType,
    pub reset_overlay: bool,
    pub network_socket: &'sandbox Path,
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

    fn shutdown(&self, sandbox_runtime_dir: &Path) -> Result<(), anyhow::Error>;

    fn state(&self, sandbox_runtime_dir: &Path) -> State;
}

pub fn new(config: Option<&Config>) -> Box<dyn Hypervisor> {
    let mut binary = PathBuf::from("cloud-hypervisor");
    if let Some(config) = config
        && let Some(cloud_hypervisor) = config.cloud_hypervisor.as_ref()
        && let Some(configured) = cloud_hypervisor.binary.as_ref()
    {
        binary = configured.to_path_buf();
    }

    Box::new(CloudHypervisor::new(binary))
}
