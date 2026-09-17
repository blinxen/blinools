use std::fs;
use std::os::unix::process::CommandExt;
use std::path::Path;
use std::path::PathBuf;
use std::process::Command;

use crate::sandbox::hypervisor::VmConfig;

pub struct CGroup {
    path: PathBuf,
}

impl CGroup {
    pub fn create(cgroup: &Path, cfg: &VmConfig) -> Option<Self> {
        if !cgroup.exists() {
            log::warn!("could not create cgroup");
            return None;
        }
        // cpu
        // matches the kernel default (100ms)
        let cpu_period = 100_000u64;
        let cpu_quota = (cpu_period as f64 * cfg.cpus) as u64;
        fs::write(cgroup.join("cpu.max"), format!("{cpu_quota} {cpu_period}")).ok()?;
        // memory
        let memory_bytes = cfg.memory_mb * 1024 * 1024;
        fs::write(
            cgroup.join("memory.high"),
            (memory_bytes * 9 / 10).to_string(),
        )
        .ok()?;
        fs::write(cgroup.join("memory.max"), memory_bytes.to_string()).ok()?;
        fs::write(cgroup.join("memory.swap.max"), "0").ok()?;
        // pids
        fs::write(cgroup.join("pids.max"), "1024").ok()?;

        Some(Self {
            path: cgroup.to_path_buf(),
        })
    }

    pub fn enter(&self, command: &mut Command) {
        // Enter cgroup before launching command
        let procs_path = self.path.join("cgroup.procs");
        unsafe {
            command.pre_exec(move || {
                fs::write(&procs_path, std::process::id().to_string())?;
                Ok(())
            });
        }
    }
}

impl Drop for CGroup {
    fn drop(&mut self) {
        if self.path.exists() {
            let _ = fs::remove_dir(&self.path);
        }
    }
}
