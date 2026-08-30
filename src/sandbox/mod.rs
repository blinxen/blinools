mod cloud_hypervisor;
pub mod config;
mod fs;
mod passt;

use std::{
    io,
    path::PathBuf,
    process::{Child, ExitStatus},
};

use anyhow::Context;
use clap::Subcommand;

use crate::sandbox::fs::FsMount;

#[derive(Subcommand)]
pub enum Command {
    /// Create a sandbox in the current workding directory
    Create,
}

pub fn handle(command: Command, config: config::Config) -> Result<(), anyhow::Error> {
    match command {
        Command::Create => {
            let passt_network =
                passt::PasstNetwork::new(config.passt.as_ref(), config.dns.as_ref())?;
            let mut mounts = Vec::new();
            if let Some(shares) = &config.shares {
                for share in shares {
                    mounts.push(FsMount::spawn(config.virtiofsd.as_ref(), share)?);
                }
            }
            // TODO: Handle error properly here
            let mut sandbox =
                Sandbox::provision(config, passt_network.socket_path().clone(), &mounts)?;

            sandbox
                .wait_for_exit()
                .context("waiting on sandbox to exit")?;
        }
    };

    Ok(())
}

struct Sandbox {
    vmm: Child,
}

impl Sandbox {
    pub fn provision(
        config: config::Config,
        network_socket: PathBuf,
        mounts: &Vec<FsMount>,
    ) -> Result<Sandbox, anyhow::Error> {
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

        Ok(Sandbox { vmm: ch_vmm })
    }

    pub fn wait_for_exit(&mut self) -> Result<ExitStatus, io::Error> {
        self.vmm.wait()
    }
}
