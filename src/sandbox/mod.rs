mod console;
mod fs;
mod lock;
mod passt;
mod process;

pub mod config;
pub mod hypervisor;
pub mod name;

use std::{
    ffi::OsStr,
    io::Write,
    path::{Path, PathBuf},
};

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
    /// Delete all stopped sandboxes
    Prune,
}

pub fn handle(command: Command, config: Option<config::Config>) -> Result<(), anyhow::Error> {
    let hypervisor = hypervisor::new(config.as_ref());

    match command {
        Command::Ps => list_sandboxes(hypervisor.as_ref())?,
        Command::Create {
            shares,
            name,
            recreate,
            delete_after_shutdown,
        } => {
            create_sandbox(
                config.context("the configuration has no `sandbox` section")?,
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
        Command::Prune => prune_sandboxes(hypervisor.as_ref())?,
    };

    Ok(())
}

fn create_sandbox(
    mut config: config::Config,
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

    let shares = config::merge_by_name(config.shares.as_ref(), shares);
    validate_socket_path_lengths(&config.name, &shares)?;

    {
        let mut passt_network = None;
        if config.network == config::Network::Lan {
            passt_network = Some(passt::PasstNetwork::new(&config)?);
        }
        let mut mounts = Vec::new();
        for share in &shares {
            mounts.push(FsMount::spawn(&config, share)?);
        }

        let mut console = Console::new()?;
        let mut vm = hypervisor.boot(VmConfig {
            name: &config.name,
            kernel: &config.kernel,
            rootfs: &config.rootfs,
            rootfs_type: &config.rootfs_type,
            reset_overlay: recreate,
            network_socket: passt_network.as_ref().map(|p| p.socket_path()),
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

fn prune_sandboxes(hypervisor: &dyn Hypervisor) -> Result<(), anyhow::Error> {
    println!("This command will delete ALL stopped sandboxes including their state.");
    print!("Are you sure you want to continue? [y/N] ");
    let _ = std::io::stdout().flush();
    let mut input = String::new();
    std::io::stdin()
        .read_line(&mut input)
        .context("reading user input")?;
    if input.to_lowercase().trim() != "y" {
        return Ok(());
    }

    for sandbox in existing_sandbox_names() {
        if let Some(name) = Name::sanitize(&sandbox)
            && !hypervisor.is_running(&runtime_dir().join(&name))
        {
            if let Err(err) = delete_sandbox(hypervisor, &name, false) {
                println!("{err}");
                log::warn!("could not delete {name}: {err}");
            }
        } else {
            log::warn!("unexpected invalid sandbox name: {sandbox}");
        }
    }

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

#[cfg(test)]
mod tests {
    use super::*;
    use serial_test::serial;
    use std::ffi::{OsStr, OsString};
    use std::os::unix::ffi::OsStrExt;

    fn with_env<F: FnOnce()>(vars: &[(&str, Option<&str>)], f: F) {
        let saved: Vec<(&str, Option<OsString>)> = vars
            .iter()
            .map(|(k, _)| (*k, std::env::var_os(k)))
            .collect();

        for (k, v) in vars {
            match v {
                Some(val) => unsafe { std::env::set_var(k, val) },
                None => unsafe { std::env::remove_var(k) },
            }
        }

        f();

        for (k, v) in saved {
            match v {
                Some(val) => unsafe { std::env::set_var(k, val) },
                None => unsafe { std::env::remove_var(k) },
            }
        }
    }

    fn name(s: &str) -> Name {
        Name::new(s).expect("valid sandbox name")
    }

    fn share(name: &str) -> FsShare {
        FsShare {
            host_dir: PathBuf::new(),
            name: Name::new(name).unwrap(),
            read_only: false,
            read_only_paths: Vec::new(),
            hidden_paths: Vec::new(),
        }
    }

    #[test]
    fn socket_path_in_joins_socket_name_with_sock_suffix() {
        let dir = Path::new("/run/user/1000/blinools/mybox");
        assert_eq!(
            socket_path_in(dir, "passt"),
            PathBuf::from("/run/user/1000/blinools/mybox/passt.sock")
        );
        assert_eq!(
            socket_path_in(dir, "vfsd-myshare"),
            PathBuf::from("/run/user/1000/blinools/mybox/vfsd-myshare.sock")
        );
    }

    #[test]
    #[serial]
    fn existing_sandbox_names_sorts_and_dedupes_across_runtime_and_state() {
        let tmp = tempfile::tempdir().unwrap();
        let xdg_runtime = tmp.path().join("runtime");
        let xdg_state = tmp.path().join("state");

        with_env(
            &[
                ("XDG_RUNTIME_DIR", Some(xdg_runtime.to_str().unwrap())),
                ("XDG_STATE_HOME", Some(xdg_state.to_str().unwrap())),
            ],
            || {
                let runtime_base = runtime_dir();
                let state_base = state_dir().unwrap();

                std::fs::create_dir_all(runtime_base.join("zeta")).unwrap();
                std::fs::create_dir_all(runtime_base.join("alpha")).unwrap();
                std::fs::create_dir_all(state_base.join("alpha")).unwrap();
                std::fs::create_dir_all(state_base.join("beta")).unwrap();

                assert_eq!(
                    existing_sandbox_names(),
                    vec!["alpha".to_string(), "beta".to_string(), "zeta".to_string()]
                );
            },
        );
    }

    #[test]
    #[serial]
    fn existing_sandbox_names_ignores_non_directory_entries() {
        let tmp = tempfile::tempdir().unwrap();
        let xdg_runtime = tmp.path().join("runtime");

        with_env(
            &[
                ("XDG_RUNTIME_DIR", Some(xdg_runtime.to_str().unwrap())),
                (
                    "XDG_STATE_HOME",
                    Some(tmp.path().join("no-such-state").to_str().unwrap()),
                ),
            ],
            || {
                let runtime_base = runtime_dir();
                std::fs::create_dir_all(runtime_base.join("real-sandbox")).unwrap();
                std::fs::write(runtime_base.join("stray-file"), b"not a sandbox").unwrap();

                assert_eq!(existing_sandbox_names(), vec!["real-sandbox".to_string()]);
            },
        );
    }

    #[test]
    #[serial]
    fn existing_sandbox_names_empty_when_neither_dir_exists() {
        let tmp = tempfile::tempdir().unwrap();

        with_env(
            &[
                (
                    "XDG_RUNTIME_DIR",
                    Some(tmp.path().join("no-runtime").to_str().unwrap()),
                ),
                (
                    "XDG_STATE_HOME",
                    Some(tmp.path().join("no-state").to_str().unwrap()),
                ),
            ],
            || {
                assert!(existing_sandbox_names().is_empty());
            },
        );
    }

    #[test]
    #[serial]
    fn existing_sandbox_names_still_lists_runtime_when_state_dir_unresolvable() {
        let tmp = tempfile::tempdir().unwrap();
        let xdg_runtime = tmp.path().join("runtime");

        with_env(
            &[
                ("XDG_RUNTIME_DIR", Some(xdg_runtime.to_str().unwrap())),
                ("XDG_STATE_HOME", None),
                ("HOME", None),
            ],
            || {
                std::fs::create_dir_all(runtime_dir().join("only-runtime")).unwrap();
                assert_eq!(existing_sandbox_names(), vec!["only-runtime".to_string()]);
            },
        );
    }

    #[test]
    #[serial]
    fn complete_sandbox_name_filters_by_prefix() {
        let tmp = tempfile::tempdir().unwrap();
        let xdg_runtime = tmp.path().join("runtime");

        with_env(
            &[
                ("XDG_RUNTIME_DIR", Some(xdg_runtime.to_str().unwrap())),
                (
                    "XDG_STATE_HOME",
                    Some(tmp.path().join("no-such-state").to_str().unwrap()),
                ),
            ],
            || {
                let base = runtime_dir();
                std::fs::create_dir_all(base.join("web-1")).unwrap();
                std::fs::create_dir_all(base.join("web-2")).unwrap();
                std::fs::create_dir_all(base.join("db")).unwrap();

                let candidates = complete_sandbox_name(OsStr::new("web"));
                let mut values: Vec<String> = candidates
                    .iter()
                    .map(|c| c.get_value().to_string_lossy().into_owned())
                    .collect();
                values.sort();

                assert_eq!(values, vec!["web-1".to_string(), "web-2".to_string()]);
            },
        );
    }

    #[test]
    fn complete_sandbox_name_empty_for_non_utf8_input() {
        // Not valid UTF-8: "fo\xFFo"
        let invalid = OsStr::from_bytes(&[0x66, 0x6f, 0xff, 0x6f]);
        assert!(complete_sandbox_name(invalid).is_empty());
    }

    #[test]
    #[serial]
    fn complete_sandbox_name_empty_when_no_names_match() {
        let tmp = tempfile::tempdir().unwrap();
        let xdg_runtime = tmp.path().join("runtime");

        with_env(
            &[
                ("XDG_RUNTIME_DIR", Some(xdg_runtime.to_str().unwrap())),
                (
                    "XDG_STATE_HOME",
                    Some(tmp.path().join("no-such-state").to_str().unwrap()),
                ),
            ],
            || {
                std::fs::create_dir_all(runtime_dir().join("db")).unwrap();
                assert!(complete_sandbox_name(OsStr::new("web")).is_empty());
            },
        );
    }

    #[test]
    #[serial]
    fn validate_socket_path_lengths_ok_for_short_names() {
        with_env(&[("XDG_RUNTIME_DIR", Some("/tmp/xrd"))], || {
            let n = name("sb");
            assert!(validate_socket_path_lengths(&n, &[]).is_ok());
        });
    }

    #[test]
    #[serial]
    fn validate_socket_path_lengths_errors_when_sandbox_name_too_long() {
        with_env(
            &[("XDG_RUNTIME_DIR", Some(&"x".repeat(MAX_SOCKET_PATH_LENGTH)))],
            || {
                let n = name("x");
                assert!(validate_socket_path_lengths(&n, &[]).is_err());
            },
        );
    }

    #[test]
    #[serial]
    fn validate_socket_path_lengths_checks_per_share_socket_names() {
        with_env(
            &[("XDG_RUNTIME_DIR", Some(&"x".repeat(MAX_SOCKET_PATH_LENGTH)))],
            || {
                let n = name("b");
                let long_share = share("x");
                assert!(validate_socket_path_lengths(&n, &[long_share]).is_err());
            },
        );
    }
}
