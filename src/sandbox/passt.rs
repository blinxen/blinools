use std::{
    path::PathBuf,
    process::{Child, Command, Stdio},
};

use anyhow::Context;
use wait_timeout::ChildExt;

use crate::{config::runtime_dir, sandbox::config::PasstConfig};

pub struct PasstNetwork {
    handle: Child,
    socket_path: PathBuf,
}

// TODO: Think about firewall here and how we can put another defense line
impl PasstNetwork {
    pub fn new(cfg: Option<&PasstConfig>, dns: Option<&String>) -> Result<PasstNetwork, anyhow::Error> {
        let socket_path = runtime_dir().join(format!("passt-{}.sock", std::process::id()));
        let mut binary_path = PathBuf::from("passt");
        if let Some(cfg) = cfg
            && let Some(binary) = &cfg.binary
        {
            binary_path = binary.to_path_buf();
        }
        let mut dns_config: Vec<&str> = Vec::new();
        if let Some(dns) = dns {
            dns_config.push("--dns");
            dns_config.push(dns);
        }
        // TODO: socket needs to be cleanup
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
            // TODO: probably want to log this
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
        unsafe {
            let _ = libc::kill(self.handle.id() as i32, libc::SIGTERM);
        }
        match self.handle.wait_timeout(std::time::Duration::from_secs(3)) {
            Ok(Some(_)) => {}
            _ => {
                let _ = self.handle.kill();
                let _ = self.handle.wait();
            }
        }
        // This makes sure that the socket file is properly removed (happens when virtiofsd did not
        // gracefully shutdown)
        let _ = std::fs::remove_file(&self.socket_path);
    }
}
