use std::{
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    time::Duration,
};

use anyhow::Context;

use crate::sandbox::process::{die_with_parent, kill_child_and_cleanup, wait_for_socket};
use crate::sandbox::{config::Config, unique_socket_path};

const SOCKET_TIMEOUT: Duration = Duration::from_secs(10);

pub struct PasstNetwork {
    handle: Child,
    socket_path: PathBuf,
}

impl PasstNetwork {
    pub fn new(config: &Config) -> Result<PasstNetwork, anyhow::Error> {
        let socket_path = unique_socket_path(&config.name, "passt");
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

        let mut command = Command::new(binary_path);
        // TODO: Probably want to enter a network namespace before starting passt
        command
            .args([
                "--vhost-user",
                "--socket",
                &socket_path.display().to_string(),
                "--repair-path",
                "none",
                "--foreground",
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
                "10.200.0.2/24",
                "--gateway",
                "10.200.0.1",
            ])
            .args(dns_config)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        die_with_parent(&mut command);

        let mut handle = command.spawn().context("spawing passt")?;
        if let Err(error) = wait_for_socket(&socket_path, &mut handle, SOCKET_TIMEOUT) {
            kill_child_and_cleanup(&mut handle, &[&socket_path]);
            return Err(error).context("starting the sandbox network");
        }

        Ok(PasstNetwork {
            handle,
            socket_path,
        })
    }

    pub fn socket_path(&self) -> &Path {
        &self.socket_path
    }
}

impl Drop for PasstNetwork {
    fn drop(&mut self) {
        kill_child_and_cleanup(&mut self.handle, &[&self.socket_path]);
    }
}
