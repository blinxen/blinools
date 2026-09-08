use std::{
    os::unix::fs::DirBuilderExt,
    path::{Path, PathBuf},
};

use anyhow::Context;
use garde::Validate;
use serde::Deserialize;

use crate::sandbox;

#[derive(Deserialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct Config {
    #[garde[dive]]
    pub sandbox: Option<sandbox::config::Config>,
}

pub fn runtime_dir() -> PathBuf {
    std::env::var_os("XDG_RUNTIME_DIR")
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            let uid = unsafe { libc::getuid() };
            PathBuf::from("/run/user").join(uid.to_string())
        })
        .join("blinools")
}

pub fn state_dir() -> Result<PathBuf, anyhow::Error> {
    Ok(std::env::var_os("XDG_STATE_HOME")
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .or_else(|| std::env::home_dir().map(|h| h.join(".local").join("state")))
        .context("locating state directory with XDG_STATE_HOME or HOME/.local/state")?
        .join("blinools"))
}

pub fn setup_dirs() -> Result<(), anyhow::Error> {
    create_dir(&runtime_dir()).context("creating runtime directory")?;
    create_dir(&state_dir()?).context("creating state directory")?;

    Ok(())
}

pub fn create_dir(path: &Path) -> Result<(), std::io::Error> {
    std::fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(path)
}

pub fn parse_config(config_file: Option<&String>) -> Result<Config, anyhow::Error> {
    let config: Config = config::Config::builder()
        .add_source(config::File::from(config_dir().join("blinools.toml")).required(false))
        .add_source(config::File::with_name(config_file.unwrap_or(&String::new())).required(config_file.is_some()))
        .build()
        .context("reading config file")?
        .try_deserialize()
        .context("parsing config file")?;

    config.validate().context("validating config file")?;
    Ok(config)
}

fn config_dir() -> PathBuf {
    std::env::var_os("XDG_CONFIG_HOME")
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .or_else(|| std::env::home_dir().map(|h| h.join(".config")))
        .unwrap_or_default()
        .join("blinools")
}
