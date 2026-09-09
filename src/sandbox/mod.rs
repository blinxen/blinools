mod console;
mod fs;
mod lock;
mod passt;
mod process;

pub mod config;
pub mod hypervisor;
pub mod name;

use std::{collections::HashMap, ffi::OsStr, path::PathBuf};

use anyhow::Context;
use clap::Subcommand;
use clap_complete::engine::{ArgValueCompleter, CompletionCandidate};
use tabled::{Table, Tabled};

use crate::{
    config::{create_dir, runtime_dir, state_dir},
    sandbox::{
        config::FsShare,
        console::{Console, ConsoleExit},
        fs::FsMount,
        hypervisor::{Hypervisor, VmConfig, cloud_hypervisor},
        lock::SandboxLock,
        name::Name,
    },
};

// On Linux we can't have a bigger path length
const MAX_SOCKET_PATH_LENGTH: usize = 107;

#[derive(Subcommand)]
pub enum Command {
    /// List all sandboxes
    Ps,
    /// Create and start a sandbox
    Create {
        /// Add a filesystem share. Can be passed multiple times.
        ///
        /// The following formats are accepted:
        ///
        /// PATH
        /// PATH:(rw|ro)
        /// NAME:PATH:(rw|ro)
        #[arg(short = 's', long = "share", value_parser = config::parse_share)]
        shares: Vec<FsShare>,
        /// Sandbox name
        #[arg(add = ArgValueCompleter::new(complete_sandbox_name))]
        name: Option<Name>,
        /// Recreate VM with the configured rootfs
        ///
        /// This will forcefully recreate the overlay containing all changes done since the last
        /// creation
        #[arg(long = "recreate", default_value_t = false)]
        recreate: bool,
        /// When set then the sandbox will be deleted once it is shutdown
        #[arg(long = "delete-after-shutdown", default_value_t = false)]
        delete_after_shutdown: bool,
    },
    /// Shutdown a sandbox
    Shutdown {
        /// Sandbox name
        #[arg(add = ArgValueCompleter::new(complete_sandbox_name))]
        name: Name,
    },
    /// Delete a sandbox
    Delete {
        /// Sandbox name
        #[arg(add = ArgValueCompleter::new(complete_sandbox_name))]
        name: Name,
        /// Forces a shutdown before deleting
        #[arg(short = 'f', long = "force", default_value_t = false)]
        force: bool,
    },
}

pub fn handle(command: Command, mut config: config::Config) -> Result<(), anyhow::Error> {
    let hypervisor = hypervisor::new(&config);

    match command {
        Command::Ps => {
            list_sandboxes(hypervisor.as_ref())?;
        }
        Command::Create {
            shares,
            name,
            recreate,
            delete_after_shutdown,
        } => {
            create_sandbox(
                &mut config,
                hypervisor,
                shares,
                name,
                recreate,
                delete_after_shutdown,
            )?;
        }
        Command::Shutdown { name } => {
            hypervisor.shutdown(&runtime_dir().join(&name))?;
        }
        Command::Delete { name, force } => {
            delete_sandbox(hypervisor.as_ref(), &name, force)?;
        }
    };

    Ok(())
}

fn create_sandbox(
    config: &mut config::Config,
    hypervisor: Box<dyn Hypervisor>,
    shares: Vec<FsShare>,
    name: Option<Name>,
    recreate: bool,
    delete_after_shutdown: bool,
) -> Result<(), anyhow::Error> {
    if let Some(name) = name {
        config.name = name;
    }
    ensure_unique_name(hypervisor.as_ref(), &config.name)?;
    let lock = SandboxLock::try_acquire(&config.name)?
        .context("failed to acquire lock, a sandbox with the same name is already running")?;
    create_dir(&runtime_dir().join(&config.name))
        .context("creating runtime directory for sandbox")?;
    create_dir(&state_dir()?.join(&config.name)).context("creating state directory for sandbox")?;

    let shares = merge_shares(config.shares.as_ref(), shares);
    validate_socket_path_lengths(&config.name, &shares)?;

    {
        let passt_network = passt::PasstNetwork::new(config)?;
        let mut mounts = Vec::new();
        for share in &shares {
            mounts.push(FsMount::spawn(config, share)?);
        }

        let mut console = Console::new()?;
        let mut vm = hypervisor.boot(VmConfig {
            name: &config.name,
            kernel: &config.kernel,
            rootfs: &config.rootfs,
            rootfs_type: &config.rootfs_type,
            reset_overlay: recreate,
            network_socket: passt_network.socket_path(),
            cmdline: &config.kernel_cmdline,
            memory_mb: config.memory_mb,
            cpus: config.cpus,
            mounts: &mounts,
            console: console.take_slave()?,
        })?;

        match console.read_until_terminated()? {
            ConsoleExit::GuestGone => {
                vm.wait().context("waiting for sandbox to exit")?;
            }
            ConsoleExit::Terminated => vm.terminate(),
        }
    }

    // Drop must happen here because delete will try to acquire the lock too
    drop(lock);
    if delete_after_shutdown {
        delete_sandbox(hypervisor.as_ref(), &config.name, true)?;
    }

    Ok(())
}

fn delete_sandbox(
    hypervisor: &dyn Hypervisor,
    name: &Name,
    force: bool,
) -> Result<(), anyhow::Error> {
    let sandbox_runtime_dir = runtime_dir().join(name);
    let sandbox_state_dir = state_dir()?.join(name);

    let lock = SandboxLock::try_acquire(name)?;
    if !force && (lock.is_none() || hypervisor.is_running(&sandbox_runtime_dir)) {
        return Err(anyhow::anyhow!(
            "can't delete a running sandbox, either use --force or shut the sandbox down and then try again"
        ));
    }

    hypervisor.shutdown(&sandbox_runtime_dir)?;
    if sandbox_runtime_dir.exists() {
        std::fs::remove_dir_all(sandbox_runtime_dir)
            .context("cleaning up sandbox runtime directory")?;
    }
    if sandbox_state_dir.exists() {
        std::fs::remove_dir_all(sandbox_state_dir)
            .context("cleaning up sandbox state directory")?;
    }

    Ok(())
}

#[derive(Tabled)]
pub struct SandboxInfo {
    pub name: String,
    pub state: String,
}

fn list_sandboxes(hypervisor: &dyn Hypervisor) -> Result<(), anyhow::Error> {
    let base_dir = runtime_dir();
    let sandbox_infos: Vec<SandboxInfo> = existing_sandbox_names()
        .into_iter()
        .map(|name| SandboxInfo {
            state: hypervisor.state(&base_dir.join(&name)).to_string(),
            name,
        })
        .collect();

    println!("{}", Table::new(sandbox_infos));

    Ok(())
}

fn ensure_unique_name(hypervisor: &dyn Hypervisor, name: &Name) -> Result<(), anyhow::Error> {
    if hypervisor.is_running(&runtime_dir().join(name)) {
        return Err(anyhow::anyhow!(
            "a sandbox with the same name already exists"
        ));
    }
    Ok(())
}

fn merge_shares(config_shares: Option<&Vec<FsShare>>, cli_shares: Vec<FsShare>) -> Vec<FsShare> {
    // TODO: Should probably warn about dangerous shares
    let mut shares: HashMap<Name, FsShare> = config_shares
        .into_iter()
        .flatten()
        .map(|s| (s.name.clone(), s.clone()))
        .collect();

    for cli_share in cli_shares {
        shares.insert(cli_share.name.clone(), cli_share);
    }

    let mut shares: Vec<FsShare> = shares.into_values().collect();
    shares.sort_by(|a, b| a.name.cmp(&b.name));

    shares
}

fn existing_sandbox_names() -> Vec<String> {
    let mut names: Vec<String> = [Some(runtime_dir()), state_dir().ok()]
        .into_iter()
        .flatten()
        .filter_map(|dir| std::fs::read_dir(dir).ok())
        .flatten()
        .filter_map(|entry| entry.ok())
        .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_dir()))
        .filter_map(|entry| entry.file_name().into_string().ok())
        .collect();

    names.sort();
    names.dedup();

    names
}

fn complete_sandbox_name(current: &OsStr) -> Vec<CompletionCandidate> {
    let Some(current) = current.to_str() else {
        return Vec::new();
    };

    existing_sandbox_names()
        .into_iter()
        .filter(|name| name.starts_with(current))
        .map(CompletionCandidate::new)
        .collect()
}

fn validate_socket_path_lengths(name: &Name, shares: &[FsShare]) -> Result<(), anyhow::Error> {
    let mut paths = vec![
        socket_path(name, cloud_hypervisor::SOCKET_NAME),
        socket_path(name, "passt"),
    ];
    for share in shares {
        paths.push(socket_path(name, &format!("vfsd-{}", share.name)));
    }

    for path in paths {
        let length = path.as_os_str().len();
        if length > MAX_SOCKET_PATH_LENGTH {
            return Err(anyhow::anyhow!(
                "socket path `{}` is {length} bytes, which is over the {MAX_SOCKET_PATH_LENGTH} \
                 byte limit for unix sockets, use a shorter sandbox or share name",
                path.display()
            ));
        }
    }

    Ok(())
}

pub fn socket_path(sandbox_name: &Name, socket: &str) -> PathBuf {
    socket_path_in(&runtime_dir().join(sandbox_name), socket)
}

pub fn socket_path_in(sandbox_runtime_dir: &Path, socket: &str) -> PathBuf {
    sandbox_runtime_dir.join(format!("{socket}.sock"))
}
