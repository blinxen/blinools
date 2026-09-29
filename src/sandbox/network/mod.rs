mod passt;

use std::path::Path;

use crate::sandbox::{cgroup::CGroup, config::Config};

pub struct Network {
    passt: passt::Passt,
}

impl Network {
    pub fn socket_path(&self) -> &Path {
        &self.passt.socket_path()
    }
}

pub fn create(config: &Config, cgroup: Option<&CGroup>) -> Result<Network, anyhow::Error> {
    Ok(Network {
        passt: passt::Passt::new(config, cgroup)?,
    })
}
