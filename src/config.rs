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

pub fn state_dir() -> Result<PathBuf, anyhow::Error> {
    Ok(std::env::var_os("XDG_STATE_HOME")
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .or_else(|| std::env::home_dir().map(|h| h.join(".local").join("state")))
        .context("locating state directory with XDG_STATE_HOME or HOME/.local/state")?
        .join("blinools"))
}

pub fn setup_dirs() -> Result<(), anyhow::Error> {
    // /run/user/<UID>/blinools
    let runtime = runtime_dir();
    std::fs::create_dir_all(&runtime).context("creating runtime directory")?;
    std::fs::set_permissions(runtime, std::fs::Permissions::from_mode(0o700))?;

    // HOME/.local/state/blinools/
    let state = state_dir()?;
    std::fs::create_dir_all(&state).context("creating state directory")?;
    std::fs::set_permissions(state, std::fs::Permissions::from_mode(0o700))?;

    Ok(())
}

pub fn parse_config(config_file: &str) -> Result<Option<Config>, anyhow::Error> {
    let mut config: Config = config::Config::builder()
        // HOME/.config/blinools/blinools.toml
        .add_source(config::File::from(config_dir().join("blinools.toml")).required(false))
        .add_source(config::File::with_name(config_file).required(false))
        .build()
        .context("reading config file")?
        .try_deserialize()
        .context("parsing config file")?;

    config.validate().context("validating config file")?;
    // TODO: I don't like doing this manually
    if let Some(ref mut sandbox) = config.sandbox {
        sandbox.kernel = make_path_absolute(&sandbox.kernel)?;
        sandbox.rootfs = make_path_absolute(&sandbox.rootfs)?;
        if let Some(ref mut shares) = sandbox.shares {
            for share in shares.iter_mut() {
                share.host_dir = make_path_absolute(&share.host_dir)?;
            }
        }
    }
    Ok(Some(config))
}

fn make_path_absolute(path: &Path) -> Result<PathBuf, anyhow::Error> {
    std::fs::canonicalize(path).context("trying to transform path to absolute path")
}

fn config_dir() -> PathBuf {
    std::env::var_os("XDG_CONFIG_HOME")
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .or_else(|| std::env::home_dir().map(|h| h.join(".config")))
        .unwrap_or_default()
        .join("blinools")
}
