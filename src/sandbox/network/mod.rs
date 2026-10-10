mod passt;
mod pasta;

use std::{path::Path, time::Duration};

use anyhow::Context;

use crate::sandbox::{
    cgroup::CGroup,
    config::Config,
    process::{wait_for_pid_file, wait_for_socket},
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
    passt.wait_for_namespace_creation()?;
    let pasta = pasta::Pasta::new(passt.pid(), config, cgroup)?;
    // Wait until pasta is initializing
    if let Err(error) = wait_for_pid_file(pasta.pid_file(), pasta.pid(), Duration::from_secs(10)) {
        return Err(error).context("waiting for pasta");
    }
    passt.pasta_launched()?;

    // Wait until the vhost-user socket is available
    if let Err(error) = wait_for_socket(passt.socket_path(), passt.pid(), Duration::from_secs(10)) {
        return Err(error).context("waiting for sandbox network");
    }

    Ok(Network {
        passt,
        _pasta: pasta,
    })
}
