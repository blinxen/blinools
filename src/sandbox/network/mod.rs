mod firewall;
mod passt;
mod pasta;

use std::{path::Path, time::Duration};

use anyhow::Context;

use crate::sandbox::{cgroup::CGroup, config::Config};

const DNS_FORWARD_ADDRESS: &str = "10.200.0.3";

pub(super) fn dns_args(config: &Config) -> Vec<String> {
    config
        .dns
        .iter()
        .flatten()
        .flat_map(|dns| [String::from("--dns"), dns.clone()])
        .collect()
}

pub(super) fn common_args() -> Vec<&'static str> {
    vec![
        "--foreground",
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
    ]
}

const PASTA_STARTUP_TIMEOUT: Duration = Duration::from_secs(10);
const PASST_STARTUP_TIMEOUT: Duration = Duration::from_secs(10);

pub struct Network {
    passt: passt::Passt,
    // Keep it here so that it is dropped together with pasta
    _pasta: pasta::Pasta,
}

impl Network {
    pub fn socket_path(&self) -> &Path {
        self.passt.socket_path()
    }
}

pub fn create(config: &Config, cgroup: Option<&CGroup>) -> Result<Network, anyhow::Error> {
    let mut passt = passt::Passt::new(config, cgroup)?;
    let mut pasta = pasta::Pasta::new(passt.pid(), config, cgroup)?;
    // Wait until pasta is initialized
    pasta
        .wait_for_pid_file(PASTA_STARTUP_TIMEOUT)
        .context("waiting for pasta")?;

    // Wait until the vhost-user socket is available
    passt.exec()?;
    passt
        .wait_for_socket(PASST_STARTUP_TIMEOUT)
        .context("waiting for sandbox network")?;

    Ok(Network {
        passt,
        _pasta: pasta,
    })
}
