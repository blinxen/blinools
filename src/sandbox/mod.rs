mod cloud_hypervisor;
mod fs;
mod passt;

pub mod config;

use std::{collections::HashMap, path::PathBuf, process::Child};

use anyhow::Context;
use clap::Subcommand;

use crate::sandbox::{config::FsShare, fs::FsMount};

#[derive(Subcommand)]
pub enum Command {
    /// Create a sandbox in the current workding directory
    Create {
        /// Add a filesystem share: TAG:PATH:(ro|rw). Can be passed multiple times.
        #[arg(short = 's', long = "share", value_name = "TAG:PATH:(ro|rw)", value_parser = config::parse_share)]
        shares: Vec<FsShare>,
    },
}

pub fn handle(command: Command, config: config::Config) -> Result<(), anyhow::Error> {
    match command {
        Command::Create { shares } => {
            let passt_network =
                passt::PasstNetwork::new(config.passt.as_ref(), config.dns.as_ref())?;
            let mut mounts = Vec::new();
            for share in merge_shares(config.shares.as_ref(), shares) {
                mounts.push(FsMount::spawn(config.virtiofsd.as_ref(), &share)?);
            }
            // TODO: Handle error properly here
            let mut child =
                create_and_start_vm(config, passt_network.socket_path().clone(), &mounts)?;

            child.wait().context("waiting on sandbox to exit")?;
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
    config: config::Config,
    network_socket: PathBuf,
    mounts: &Vec<FsMount>,
) -> Result<Child, anyhow::Error> {
    let mut binary_path = PathBuf::from("cloud-hypervisor");
    if let Some(cloud_hypervisor) = config.cloud_hypervisor
        && let Some(binary) = cloud_hypervisor.binary
    {
        binary_path = binary.to_path_buf();
    }
    let ch_vmm = cloud_hypervisor::create_vm(cloud_hypervisor::VmConfig {
        binary: binary_path,
        kernel: config.kernel,
        rootfs: config.rootfs,
        network_socket,
        cmdline: config.kernel_cmdline,
        memory_mb: config.memory_mb,
        cpus: config.cpus,
        mounts,
    })?;

    Ok(ch_vmm)
}
