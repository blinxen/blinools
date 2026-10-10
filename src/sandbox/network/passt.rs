use std::{
    io::{Read, Write},
    os::unix::{net::UnixStream, process::CommandExt},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    time::Duration,
};

use anyhow::Context;

use crate::sandbox::{
    cgroup::CGroup,
    config::Config,
    process::{kill_child_and_cleanup, remove_stale_socket, unshare},
    socket_path,
};

pub struct Passt {
    pid: u32,
    socket_path: PathBuf,
    channel: UnixStream,
}

impl Passt {
    pub fn new(config: &Config, cgroup: Option<&CGroup>) -> Result<Passt, anyhow::Error> {
        let socket_path = socket_path(&config.name, "passt");
        remove_stale_socket(&socket_path);

        let (parent, child) = UnixStream::pair().context("creating passt startup channel")?;
        parent
            .set_read_timeout(Some(Duration::from_secs(5)))
            .context("setting namespace notification timeout")?;

        match unsafe { libc::fork() } {
            -1 => Err(std::io::Error::last_os_error()).context("forking passt"),
            0 => {
                drop(parent);
                if let Err(error) = spawn(&socket_path, config, cgroup, child) {
                    eprintln!("passt startup failed: {error:#}");
                    unsafe { libc::_exit(1) };
                }

                unsafe { libc::_exit(0) };
            }
            pid => {
                drop(child);

                Ok(Passt {
                    pid: pid as u32,
                    socket_path,
                    channel: parent,
                })
            }
        }
    }

    pub fn socket_path(&self) -> &Path {
        &self.socket_path
    }

    pub fn pid(&self) -> u32 {
        self.pid
    }

    pub fn wait_for_namespace_creation(&mut self) -> Result<(), anyhow::Error> {
        self.channel
            .read_exact(&mut [0u8; 1])
            .context("waiting for namespace creation")
    }

    pub fn pasta_launched(&mut self) -> Result<(), anyhow::Error> {
        self.channel
            .write_all(&[1])
            .context("notifying child that pasta is ready")
    }
}

impl Drop for Passt {
    fn drop(&mut self) {
        kill_child_and_cleanup(self.pid, &[&self.socket_path]);
    }
}

fn spawn(
    socket_path: &Path,
    config: &Config,
    cgroup: Option<&CGroup>,
    mut channel: UnixStream,
) -> Result<(), anyhow::Error> {
    unshare()?;

    channel
        .write_all(&[1])
        .context("notifying parent that namespace is ready")?;

    channel
        .read_exact(&mut [0u8; 1])
        .context("waiting for pasta initialization")?;
    drop(channel);

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

    let mut cmd = Command::new(binary_path);
    cmd.args([
        "--vhost-user",
        "--socket",
        &socket_path.display().to_string(),
        "--log-file",
        &socket_path.with_extension("log").display().to_string(),
        "--repair-path",
        "none",
        "--foreground",
        "-4",
        "--no-icmp",
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
        "10.200.0.2",
        "--netmask",
        "24",
        "--gateway",
        "10.200.0.1",
    ])
    .args(dns_config)
    .stdin(Stdio::null())
    .stdout(Stdio::null())
    .stderr(Stdio::null());

    if log::log_enabled!(log::Level::Debug) {
        cmd.arg("--debug");
    }

    if log::log_enabled!(log::Level::Trace) {
        cmd.arg("--trace");
    }

    if let Some(cgroup) = cgroup {
        cgroup.enter(&mut cmd);
    }
    log::debug!("Starting command: {:?}", cmd);
    Err(cmd.exec()).context("starting passt")
}
