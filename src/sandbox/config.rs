use std::path::PathBuf;

use serde::Deserialize;

#[derive(Deserialize)]
pub struct Config {
    pub kernel: PathBuf,
    pub kernel_cmdline: String,
    pub rootfs: PathBuf,
    pub memory_mb: u64,
    pub cpus: u8,
    pub shares: Option<Vec<FsShare>>,
    pub dns: Option<String>,
    pub cloud_hypervisor: Option<ChConfig>,
    pub passt: Option<PasstConfig>,
    pub virtiofsd: Option<VirtiofsdConfig>,
}

#[derive(Deserialize)]
pub struct FsShare {
    pub host_dir: PathBuf,
    pub name: String,
    pub read_only: bool,
}

#[derive(Deserialize)]
pub struct ChConfig {
    pub binary: Option<PathBuf>,
}

#[derive(Deserialize)]
pub struct PasstConfig {
    pub binary: Option<PathBuf>,
}

#[derive(Deserialize)]
pub struct VirtiofsdConfig {
    pub binary: Option<PathBuf>,
}
