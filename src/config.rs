use std::{os::unix::fs::PermissionsExt, path::PathBuf};

use anyhow::Context;
use serde::Deserialize;

use crate::sandbox;

#[derive(Deserialize)]
pub struct Config {
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
