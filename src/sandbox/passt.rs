use std::{
    path::PathBuf,
    process::{Child, Command, Stdio},
};

use anyhow::Context;

use crate::sandbox::{config::Config, create_socket_path, kill_child_and_socket_with_timeout};

pub struct PasstNetwork {
    handle: Child,
    socket_path: PathBuf,
}

// TODO: Think about firewall here and how we can put another defense line
impl PasstNetwork {
    pub fn new(config: &Config) -> Result<PasstNetwork, anyhow::Error> {
        let socket_path = create_socket_path(&config.name, "passt.sock");
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
        let handle = Command::new(binary_path)
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
            .stderr(Stdio::null())
            .spawn()
            .context("spawing passt")?;

        Ok(PasstNetwork {
            handle,
            socket_path,
        })
    }

    pub fn socket_path(&self) -> &PathBuf {
        &self.socket_path
    }
}

impl Drop for PasstNetwork {
    fn drop(&mut self) {
        kill_child_and_socket_with_timeout(&mut self.handle, &self.socket_path);
    }
}
