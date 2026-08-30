mod cloud_hypervisor;
mod fs;
mod passt;

pub mod config;

use std::{
    collections::HashMap,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    process::Child,
};

use anyhow::Context;
use clap::Subcommand;
use wait_timeout::ChildExt;

use crate::{
    config::runtime_dir,
    sandbox::{config::FsShare, fs::FsMount},
};

#[derive(Subcommand)]
pub enum Command {
    /// Create a sandbox
    Create {
        /// Add a filesystem share: TAG:PATH:(ro|rw). Can be passed multiple times.
        #[arg(short = 's', long = "share", value_name = "TAG:PATH:(ro|rw)", value_parser = config::parse_share)]
        shares: Vec<FsShare>,
        /// Sandbox name
        name: String,
    },
    /// Shutdown a sandbox
    Shutdown {
        /// Sandbox name
        name: String,
    },
}

pub fn handle(command: Command, config: config::Config) -> Result<(), anyhow::Error> {
    match command {
        Command::Create { shares, name } => {
            setup_runtime_dir_for_sandbox(&name)?;
            let passt_network =
                passt::PasstNetwork::new(&name, config.passt.as_ref(), config.dns.as_ref())?;
            let mut mounts = Vec::new();
            for share in merge_shares(config.shares.as_ref(), shares) {
                mounts.push(FsMount::spawn(&name, config.virtiofsd.as_ref(), &share)?);
            }
            // TODO: Handle error properly here
            let mut child =
                create_and_start_vm(&name, config, passt_network.socket_path().clone(), &mounts)?;

            child.wait().context("waiting on sandbox to exit")?;
        }
        Command::Shutdown { name } => {
            // At this stage, the runtime dir must exist otherwise the vm was not created properly
            let sandbox_runtime_dir = runtime_dir().join(name);
            if sandbox_runtime_dir.exists() {
                shutdown_vm(config.cloud_hypervisor, sandbox_runtime_dir)?;
            }
        }
    };

    Ok(())
}

fn merge_shares(config_shares: Option<&Vec<FsShare>>, cli_shares: Vec<FsShare>) -> Vec<FsShare> {
    let mut shares: HashMap<String, FsShare> = config_shares
        .unwrap_or(&Vec::new())
        .iter()
        .map(|s| (s.name.clone(), s.clone()))
        .collect();

    for cli_share in cli_shares {
        shares.insert(cli_share.name.clone(), cli_share);
    }

    shares.into_values().collect()
}

pub fn create_and_start_vm(
    sandbox_name: &str,
    config: config::Config,
    network_socket: PathBuf,
    mounts: &Vec<FsMount>,
) -> Result<Child, anyhow::Error> {
    let mut binary_path = PathBuf::from("cloud-hypervisor");
    if let Some(cloud_hypervisor) = config.cloud_hypervisor
        && let Some(binary) = cloud_hypervisor.cloud_hypervisor_binary
    {
        binary_path = binary.to_path_buf();
    }
    let ch_vmm = cloud_hypervisor::create_vm(cloud_hypervisor::VmConfig {
        name: sandbox_name,
        binary: &binary_path,
        kernel: &config.kernel,
        rootfs: &config.rootfs,
        network_socket: &network_socket,
        cmdline: config.kernel_cmdline,
        memory_mb: config.memory_mb,
        cpus: config.cpus,
        mounts,
    })?;

    Ok(ch_vmm)
}

fn shutdown_vm(
    config: Option<config::ChConfig>,
    sandbox_runtime_dir: PathBuf,
) -> Result<(), anyhow::Error> {
    let mut binary_path = PathBuf::from("ch-remote");
    if let Some(config) = config
        && let Some(binary) = config.ch_remote_binary
    {
        binary_path = binary.to_path_buf();
    }
    cloud_hypervisor::shutdown_vm(
        &binary_path,
        &sandbox_runtime_dir.join(cloud_hypervisor::SOCKET_NAME),
    )?;
    Ok(())
}

pub fn create_socket_path(sandbox_name: &str, socket_name: &str) -> PathBuf {
    let socket_path = runtime_dir().join(sandbox_name).join(socket_name);
    if socket_path.exists() {
        // TODO: Should probably log this somewhere
        let _ = std::fs::remove_file(&socket_path);
    }

    socket_path
}

pub fn kill_child_and_socket_with_timeout(child: &mut Child, socket_path: &Path) {
    unsafe {
        let _ = libc::kill(child.id() as i32, libc::SIGTERM);
    }
    match child.wait_timeout(std::time::Duration::from_secs(3)) {
        Ok(Some(_)) => {}
        _ => {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
    // This makes sure that the socket file is properly removed (happens when child did not
    // gracefully shutdown)
    let _ = std::fs::remove_file(socket_path);
}

pub fn setup_runtime_dir_for_sandbox(name: &str) -> Result<(), anyhow::Error> {
    let dir = runtime_dir().join(name);
    std::fs::create_dir_all(&dir).context("creating runtime directory for sandbox")?;

    std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700))?;

    Ok(())
}
