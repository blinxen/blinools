use std::fs;
use std::path::{Path, PathBuf};

use crate::config::create_dir;
use crate::sandbox::name::Name;

#[derive(Debug)]
pub struct CGroup {
    path: PathBuf,
}

impl CGroup {
    pub fn create(name: &str, cpus: f64, memory_mb: u64) -> Option<Self> {
        let base = maybe_init_base_group()?;
        let path = base.join(name);
        if create_dir(&path).is_err() {
            log::warn!("could not create cgroup");
            return None;
        }
        std::fs::write(
            base.join("cgroup.subtree_control"),
            String::from("+cpu +memory +pids"),
        )
        .ok()?;
        // cpu
        // matches the kernel default (100ms)
        let cpu_period = 100_000u64;
        let cpu_quota = (cpu_period as f64 * cpus) as u64;
        fs::write(path.join("cpu.max"), format!("{cpu_quota} {cpu_period}")).ok()?;
        // memory
        // 256MB overhead for qemu etc.
        let memory_bytes = memory_mb * 1024 * 1024 + (256 * 1024 * 1024);
        fs::write(path.join("memory.max"), memory_bytes.to_string()).ok()?;
        fs::write(path.join("memory.swap.max"), "0").ok()?;
        // pids
        fs::write(path.join("pids.max"), "1024").ok()?;

        Some(Self { path })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn from_existing(name: &Name) -> Option<Self> {
        let base = maybe_init_base_group()?;
        let path = base.join(name);
        if path.exists() {
            Some(Self { path })
        } else {
            None
        }
    }
}

fn maybe_init_base_group() -> Option<PathBuf> {
    let uid = unsafe { libc::getuid() };
    let path = PathBuf::from(&format!(
        "/sys/fs/cgroup/user.slice/user-{uid}.slice/user@{uid}.service/blinools.slice",
        uid = uid
    ));

    if !path.exists() {
        create_dir(&path).ok()?;
    }

    Some(path)
}

impl Drop for CGroup {
    fn drop(&mut self) {
        if self.path.exists() {
            let _ = fs::remove_dir(&self.path);
        }
    }
}
