use std::{
    path::{Path, PathBuf},
    time::Duration,
};

use anyhow::Context;

use crate::sandbox::{
    cgroup::CGroup,
    config,
    config::Config,
    network::firewall,
    process::{Process, remove_stale_socket},
    socket_path,
};

pub struct Passt {
    pid: u32,
    socket_path: PathBuf,
    process: Process,
}

impl Passt {
    /// Leaves passt isolated, `exec` has to be called once pasta is attached to its namespaces
    pub fn new(config: &Config, cgroup: Option<&CGroup>) -> Result<Passt, anyhow::Error> {
        let socket_path = socket_path(&config.name, "passt");
        remove_stale_socket(&socket_path);

        let mut binary_path = PathBuf::from("passt");
        if let Some(cfg) = &config.passt
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

        let mut process = Process::new(binary_path, cgroup);
        process
            .args([
                "--vhost-user",
                "--socket",
                &socket_path.display().to_string(),
                "--log-file",
                &socket_path.with_extension("log").display().to_string(),
                "--repair-path",
                "none",
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
                "--address",
                "10.200.0.2",
                "--netmask",
                "24",
                "--gateway",
                "10.200.0.1",
            ])
            .args(dns_config);

        if log::log_enabled!(log::Level::Debug) {
            process.arg("--debug");
        }

        if log::log_enabled!(log::Level::Trace) {
            process.arg("--trace");
        }

        process.remove_on_drop(&socket_path);
        process.isolate().context("starting passt")?;
        let pid = process.pid().context("passt has no pid")?;

        if config.network == config::Network::Restricted {
            process.pre_exec(firewall::install);
        }

        Ok(Passt {
            pid,
            socket_path,
            process,
        })
    }

    pub fn socket_path(&self) -> &Path {
        &self.socket_path
    }

    pub fn pid(&self) -> u32 {
        self.pid
    }

    pub fn exec(&mut self) -> Result<(), anyhow::Error> {
        self.process.exec().context("starting passt")
    }

    pub fn wait_for_socket(&mut self, timeout: Duration) -> Result<(), anyhow::Error> {
        self.process.wait_for_socket(&self.socket_path, timeout)
    }
}
