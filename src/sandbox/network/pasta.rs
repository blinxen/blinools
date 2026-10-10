use std::{
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
};

use anyhow::Context;

use crate::sandbox::{cgroup::CGroup, config::Config};
use crate::sandbox::{
    process::{die_with_parent, kill_child_and_cleanup},
    socket_path,
};

pub struct Pasta {
    handle: Child,
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
        let mut binary_path = PathBuf::from("pasta");
        if let Some(cfg) = &config.pasta
            && let Some(binary) = &cfg.binary
        {
            binary_path = binary.to_path_buf();
        }
        let mut dns_config: Vec<&str> = Vec::new();
        if let Some(dns) = &config.dns {
            for d in dns {
                dns_config.push("--dns");
                dns_config.push(d);
            }
        }

        let mut cmd = Command::new(binary_path);
        cmd.args([
            "--log-file",
            &socket_path.with_extension("log").display().to_string(),
            "--pid",
            &pid_path.display().to_string(),
            "--foreground",
            "-4",
            "--no-icmp",
            "--no-map-gw",
            "--map-host-loopback",
            "none",
            "--map-guest-addr",
            "none",
            "-t",
            "none",
            "-u",
            "none",
            "-T",
            "none",
            "-U",
            "none",
            "--address",
            "10.200.0.2",
            "--netmask",
            "24",
            "--gateway",
            "10.200.0.1",
            "--no-splice",
            "--config-net",
            "--no-netns-quit",
        ])
        .args(dns_config)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());

        if log::log_enabled!(log::Level::Debug) {
            cmd.arg("--debug");
        }

        if log::log_enabled!(log::Level::Trace) {
            cmd.arg("--trace");
        }
        cmd.arg(passt_pid.to_string());

        die_with_parent(&mut cmd);
        if let Some(cgroup) = cgroup {
            cgroup.enter(&mut cmd);
        }

        log::debug!("Starting command: {:?}", cmd);
        let handle = cmd.spawn().context("spawing passt")?;

        Ok(Pasta { handle, pid_path })
    }

    pub fn pid(&self) -> u32 {
        self.handle.id()
    }

    pub fn pid_file(&self) -> &Path {
        &self.pid_path
    }
}

impl Drop for Pasta {
    fn drop(&mut self) {
        kill_child_and_cleanup(self.handle.id(), &[&self.pid_path]);
    }
}
