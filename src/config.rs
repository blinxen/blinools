use std::{
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
};

use anyhow::Context;
use garde::Validate;
use serde::Deserialize;

use crate::sandbox;

#[derive(Deserialize, Validate)]
pub struct Config {
    #[garde[dive]]
    pub sandbox: Option<sandbox::config::Config>,
}

pub fn runtime_dir() -> PathBuf {
    let uid = unsafe { libc::getuid() };
    PathBuf::from("/run/user")
        .join(uid.to_string())
        .join("blinools")
}

pub fn setup_runtime_dir() -> Result<(), anyhow::Error> {
    let dir = runtime_dir();
    std::fs::create_dir_all(&dir).context("creating runtime directory")?;

    std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700))?;

    Ok(())
}

pub fn parse_config(config_file: &str) -> Result<Option<Config>, anyhow::Error> {
    let path = Path::new(config_file);
    if !path.exists() {
        return Ok(None);
    }

    let content = std::fs::read_to_string(path).context("reading config file")?;
    let config: Config = toml::from_str(&content).context("parsing config file")?;
    config.validate().context("validating config file")?;
    Ok(Some(config))
}
