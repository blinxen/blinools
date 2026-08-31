use std::{os::unix::fs::PermissionsExt, path::PathBuf};

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
    let config: Config = config::Config::builder()
        // HOME/.config/blinools/blinools.toml
        .add_source(
            config::File::from(config_dir().join("blinools").join("blinools.toml")).required(false),
        )
        .add_source(config::File::with_name(config_file))
        .build()
        .context("reading config file")?
        .try_deserialize()
        .context("parsing config file")?;

    config.validate().context("validating config file")?;
    Ok(Some(config))
}

fn config_dir() -> PathBuf {
    std::env::var_os("XDG_CONFIG_HOME")
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .or_else(|| std::env::home_dir().map(|h| h.join(".config")))
        .unwrap_or_else(|| PathBuf::from(".config"))
}
