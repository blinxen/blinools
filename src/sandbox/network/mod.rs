mod firewall;
mod passt;
mod pasta;

use std::path::Path;

use anyhow::Context;

use crate::sandbox::{
    cgroup::CGroup,
    config::{Config, PASST_STARTUP_TIMEOUT, PASTA_STARTUP_TIMEOUT},
};

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
