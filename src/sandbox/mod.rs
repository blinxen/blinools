mod cloud_hypervisor;
mod fs;
mod passt;

pub mod config;

use std::{
    collections::HashMap,
    os::unix::fs::{MetadataExt, PermissionsExt},
    path::{Path, PathBuf},
    process::Child,
};

use anyhow::Context;
use clap::Subcommand;
use imago::{FormatCreateBuilder, Storage, qcow2::Qcow2CreateBuilder};
use wait_timeout::ChildExt;

use crate::{
    config::{runtime_dir, state_dir},
    sandbox::{
        cloud_hypervisor::{CloudHypervisor, CloudHypervisorVmConfig},
        config::{FsShare, RootfsType},
        fs::FsMount,
    },
};

#[derive(Subcommand)]
pub enum Command {
    /// List all sandboxes
    Ps,
    /// Create and start a sandbox
    Create {
        /// Add a filesystem share: TAG:PATH:(ro|rw). Can be passed multiple times.
        #[arg(short = 's', long = "share", value_name = "TAG:PATH:(ro|rw)", value_parser = config::parse_share)]
        shares: Vec<FsShare>,
        /// Sandbox name
        name: String,
        /// Recreate VM with the configured rootfs
        ///
        /// This will forcefully recreate the overlay containing all changes done since the last
        /// creation
        #[arg(long = "recreate", default_value_t = false)]
        recreate: bool,
    },
    /// Shutdown a sandbox
    Shutdown {
        /// Sandbox name
        name: String,
    },
    /// Delete a sandbox
    Delete {
        /// Sandbox name
        name: String,
        /// Forces a shutdown before deleting
        #[arg(short = 'f', long = "force", default_value_t = false)]
        force: bool,
    },
}

pub fn handle(command: Command, config: config::Config) -> Result<(), anyhow::Error> {
    match command {
        Command::Ps => {
            list_vms()?;
        }
        Command::Create {
            shares,
            name,
            recreate,
        } => {
            ensure_unique_name(&name)?;
            setup_dirs_for_sandbox(&name)?;
            let passt_network =
                passt::PasstNetwork::new(&name, config.passt.as_ref(), config.dns.as_ref())?;
            let mut mounts = Vec::new();
            for share in merge_shares(config.shares.as_ref(), shares) {
                mounts.push(FsMount::spawn(&name, config.virtiofsd.as_ref(), &share)?);
            }
            let mut vmm = create_and_start_vm(
                &name,
                config,
                passt_network.socket_path().clone(),
                &mounts,
                recreate,
            )?;

            vmm.block_until_vm_shutsdown()
                .context("waiting for sandbox to exit")?;
        }
        Command::Shutdown { name } => {
            shutdown_vm(&runtime_dir().join(name))?;
        }
        Command::Delete { name, force } => {
            let sandbox_runtime_dir = runtime_dir().join(&name);
            let sandbox_state_dir = state_dir()?.join(&name);

            if !force
                && cloud_hypervisor::can_connect_to_socket(
                    &sandbox_runtime_dir.join(cloud_hypervisor::SOCKET_NAME),
                )
            {
                eprintln!(
                    "can't delete a running sandbox, either use --force or shut the sandbox down and then try again"
                );
                return Ok(());
            }

            shutdown_vm(&sandbox_runtime_dir)?;
            std::fs::remove_dir_all(sandbox_runtime_dir)
                .context("cleaning up sandbox runtime directory")?;
            std::fs::remove_dir_all(sandbox_state_dir)
                .context("cleaning up sandbox state directory")?;
        }
    };

    Ok(())
}

fn ensure_unique_name(name: &str) -> Result<(), anyhow::Error> {
    if cloud_hypervisor::can_connect_to_socket(
        &runtime_dir().join(name).join(cloud_hypervisor::SOCKET_NAME),
    ) {
        return Err(anyhow::anyhow!(
            "a sandbox with the same name already exists"
        ));
    }
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

fn list_vms() -> Result<(), anyhow::Error> {
    cloud_hypervisor::list_vms(&runtime_dir())?;
    Ok(())
}

pub fn create_and_start_vm(
    sandbox_name: &str,
    config: config::Config,
    network_socket: PathBuf,
    mounts: &Vec<FsMount>,
    reset_overlay: bool,
) -> Result<CloudHypervisor, anyhow::Error> {
    let mut binary_path = PathBuf::from("cloud-hypervisor");
    if let Some(cloud_hypervisor) = config.cloud_hypervisor
        && let Some(binary) = cloud_hypervisor.cloud_hypervisor_binary
    {
        binary_path = binary.to_path_buf();
    }
    let ch_vmm = cloud_hypervisor::create_vm(&cloud_hypervisor::CloudHypervisorVmConfig {
        name: sandbox_name,
        binary: &binary_path,
        kernel: &config.kernel,
        rootfs: &config.rootfs,
        rootfs_type: config.rootfs_type,
        reset_overlay,
        network_socket: &network_socket,
        cmdline: config.kernel_cmdline.unwrap_or_default(),
        memory_mb: config.memory_mb,
        cpus: config.cpus,
        mounts,
    })
    .context("creating sandbox")?;

    Ok(ch_vmm)
}

fn shutdown_vm(sandbox_runtime_dir: &Path) -> Result<(), anyhow::Error> {
    cloud_hypervisor::shutdown_vm(&sandbox_runtime_dir.join(cloud_hypervisor::SOCKET_NAME))?;
    Ok(())
}

pub fn create_socket_path(sandbox_name: &str, socket_name: &str) -> PathBuf {
    runtime_dir().join(sandbox_name).join(socket_name)
}

pub fn create_qcow2_overlay(cfg: &CloudHypervisorVmConfig) -> Result<PathBuf, anyhow::Error> {
    let qcow2_path = state_dir()?
        .join(cfg.name)
        .join("backing_file")
        .with_extension(RootfsType::QCOW2.to_string());

    if qcow2_path.exists() && !cfg.reset_overlay {
        return Ok(qcow2_path);
    }

    let rootfs_size = std::fs::metadata(cfg.rootfs)
        .context("calculating overlay size from rootfs")?
        .size();
    let image_file = imago::file::File::create_open(
        imago::StorageCreateOptions::new()
            .filename(&qcow2_path)
            .overwrite(true),
    )
    .context("creating qcow2 overlay file")?;

    Qcow2CreateBuilder::<imago::file::File>::new(image_file)
        .size(rootfs_size)
        .backing(
            cfg.rootfs.display().to_string(),
            cfg.rootfs_type.to_string(),
        )
        .create()
        .context("formatting qcow2 image")?;

    Ok(qcow2_path)
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
    let _ = std::fs::remove_file(socket_path.with_added_extension("pid"));
}

pub fn setup_dirs_for_sandbox(name: &str) -> Result<(), anyhow::Error> {
    let runtime = runtime_dir().join(name);
    std::fs::create_dir_all(&runtime).context("creating runtime directory for sandbox")?;
    std::fs::set_permissions(runtime, std::fs::Permissions::from_mode(0o700))?;

    let state = state_dir()?.join(name);
    std::fs::create_dir_all(&state).context("creating state directory for sandbox")?;
    std::fs::set_permissions(state, std::fs::Permissions::from_mode(0o700))?;

    Ok(())
}
