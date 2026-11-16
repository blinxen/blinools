use std::{path::PathBuf, time::Duration};

use anyhow::Context;

use crate::sandbox::{cgroup::CGroup, config::Config};
use crate::sandbox::{process::Process, socket_path};

pub struct Pasta {
    process: Process,
    pid_path: PathBuf,
}

impl Pasta {
    pub fn new(
        passt_pid: u32,
        config: &Config,
        cgroup: Option<&CGroup>,
    ) -> Result<Pasta, anyhow::Error> {
        let socket_path = socket_path(&config.name, "pasta");
        let pid_path = socket_path.with_extension("pid");

        let mut process = Process::new("pasta", config.pasta.as_ref(), cgroup);
        process
            .args([
                "--log-file",
                &socket_path.with_extension("log").display().to_string(),
                "--pid",
                &pid_path.display().to_string(),
                "-T",
                "none",
                "-U",
                "none",
                "--no-splice",
                "--config-net",
                "--no-netns-quit",
            ])
            .args(super::common_args())
            .args(super::dns_args(config));

        if log::log_enabled!(log::Level::Debug) {
            process.arg("--debug");
        }

        if log::log_enabled!(log::Level::Trace) {
            process.arg("--trace");
        }
        process.arg(passt_pid.to_string());

        // pasta has to stay in the host namespaces to reach the network and join passt
        process.no_namespaces();
        process.prepare_runtime_file(&pid_path)?;
        process.spawn().context("spawning pasta")?;

        Ok(Pasta { process, pid_path })
    }

    pub fn wait_for_pid_file(&mut self, timeout: Duration) -> Result<(), anyhow::Error> {
        self.process.wait_for_file(&self.pid_path, timeout)
    }
}
