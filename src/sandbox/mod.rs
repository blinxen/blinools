mod cgroup;
mod console;
mod fs;
mod lock;
mod network;
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
        cgroup::CGroup,
        config::{CliOverrides, FsShare},
        console::{Console, ConsoleExit},
        fs::FsMount,
        hypervisor::VmConfig,
        lock::SandboxLock,
        name::Name,
    },
};

// On Linux we can't have a bigger path length
const MAX_SOCKET_PATH_LENGTH: usize = 107;

#[derive(Subcommand)]
pub enum Command {
    /// List all sandboxes and their state.
    Ps,
    /// Create and start a sandbox.
    ///
    /// Stays attached to the console until the guest shuts down.
    /// Reuse a stopped sandbox's rootfs overlay unless --recreate is passed.
    #[command(
        after_long_help = "Examples:\n  blinools sandbox create work\n  blinools sandbox create work --share .\n  blinools sandbox create work --share source:.:ro\n  blinools sandbox create work --recreate --delete-after-shutdown"
    )]
    Create {
        /// Add a filesystem share.
        ///
        /// This option can be repeated. Supported forms are PATH, PATH:rw, PATH:ro,
        /// NAME:PATH:rw and NAME:PATH:ro. Shares are read-write by default.
        /// Unnamed shares take their name from the directory. Paths are relative
        /// to the working directory and cannot contain colons. This option replaces
        /// all settings of a configured share with the same name.
        #[arg(short = 's', long = "share", value_parser = config::parse_share)]
        shares: Vec<FsShare>,
        /// Specify the sandbox name.
        ///
        /// This overrides the name from the configuration. Fails if a sandbox with
        /// this name is already running.
        #[arg(add = ArgValueCompleter::new(complete_sandbox_name))]
        name: Option<Name>,
        /// Recreate the sandbox from the configured rootfs.
        ///
        /// This discards the existing rootfs overlay and all changes stored in it.
        #[arg(long = "recreate", default_value_t = false)]
        recreate: bool,
        /// Delete the sandbox after it shuts down.
        #[arg(long = "delete-after-shutdown", default_value_t = false)]
        delete_after_shutdown: bool,
        /// Mount all shares read-only, overriding the configuration.
        #[arg(long = "read-only", default_value_t = false)]
        read_only: bool,
        /// Disable hidden-path masking.
        ///
        /// Other read-only restrictions still apply.
        #[arg(long = "no-hidden", default_value_t = false)]
        no_hidden: bool,
        /// Expose hidden paths as read-only instead of masking them.
        #[arg(long = "hidden-as-read-only", default_value_t = false)]
        hidden_as_read_only: bool,
    },
    /// Stop a sandbox through its hypervisor.
    ///
    /// To let the guest operating system shut down cleanly, run poweroff inside
    /// the guest. Does nothing when the hypervisor control socket is unavailable.
    #[command(
        after_long_help = "Examples:\n  blinools sandbox shutdown work\n  blinools sandbox shutdown work --force"
    )]
    Shutdown {
        /// Specify the sandbox name.
        #[arg(add = ArgValueCompleter::new(complete_sandbox_name))]
        name: Name,
        /// Ignore guest shutdown failures reported by Cloud Hypervisor.
        ///
        /// The command still fails if the virtual machine monitor cannot shut down.
        /// This flag does not change QEMU shutdown.
        #[arg(short = 'f', long = "force", default_value_t = false)]
        force: bool,
    },
    /// Delete a sandbox and all of its saved rootfs changes.
    ///
    /// Removes only runtime files and the rootfs overlay.
    /// The base image and files in host shares are not touched.
    /// Deleting a running sandbox fails unless --force is supplied.
    #[command(
        after_long_help = "Examples:\n  blinools sandbox delete work\n  blinools sandbox delete work --force"
    )]
    Delete {
        /// Specify the sandbox name.
        #[arg(add = ArgValueCompleter::new(complete_sandbox_name))]
        name: Name,
        /// Shut down a running sandbox before deleting it.
        #[arg(short = 'f', long = "force", default_value_t = false)]
        force: bool,
    },
    /// Delete all stopped sandboxes and their saved rootfs changes.
    ///
    /// Asks for confirmation before deleting saved state.
    Prune,
}

pub fn handle(command: Command, config: Option<config::Config>) -> Result<(), anyhow::Error> {
    match command {
        Command::Ps => list_sandboxes(config.as_ref())?,
        Command::Create {
            shares,
            name,
            recreate,
            delete_after_shutdown,
            read_only,
            no_hidden,
            hidden_as_read_only,
        } => {
            create_sandbox(
                config.context("the configuration has no `sandbox` section")?,
                shares,
                name,
                recreate,
                delete_after_shutdown,
                CliOverrides {
                    read_only,
                    no_hidden,
                    hidden_as_read_only,
                },
            )?;
        }
        Command::Shutdown { name, force } => {
            let sandbox_runtime_dir = runtime_dir().join(&name);
            hypervisor::for_sandbox(config.as_ref(), &sandbox_runtime_dir)
                .shutdown(&sandbox_runtime_dir, force)?;
        }
        Command::Delete { name, force } => {
            delete_sandbox(config.as_ref(), &name, force)?;
        }
        Command::Prune => prune_sandboxes(config.as_ref())?,
    };

    Ok(())
}

fn create_sandbox(
    mut config: config::Config,
    shares: Vec<FsShare>,
    name: Option<Name>,
    recreate: bool,
    delete_after_shutdown: bool,
    overrides: CliOverrides,
) -> Result<(), anyhow::Error> {
    if let Some(name) = name {
        config.name = name;
    }
    ensure_unique_sandbox_name(Some(&config), &config.name)?;
    let hypervisor = hypervisor::new(Some(&config));
    let lock = SandboxLock::try_acquire(&config.name)?
        .context("failed to acquire lock, a sandbox with the same name is already running")?;
    // Create various directories that we need later
    create_dir(&runtime_dir().join(&config.name))
        .context("creating runtime directory for sandbox")?;
    create_dir(&state_dir()?.join(&config.name)).context("creating state directory for sandbox")?;
    let shares = config::merge_by_name(config.shares.as_ref(), shares);
    validate_socket_path_lengths(&config.name, &shares)?;

    {
        let cgroup = CGroup::create(config.name.as_str(), config.cpus as f64, config.memory_mb);
        let mut network = None;
        if config.network.mode != config::NetworkMode::None {
            network = Some(network::create(&config, cgroup.as_ref())?);
        }
        let mut mounts = Vec::new();
        for share in &shares {
            mounts.push(FsMount::spawn(&config, share, cgroup.as_ref(), &overrides)?);
        }

        let mut console = Console::new()?;
        let cfg = VmConfig {
            name: &config.name,
            cgroup: cgroup.as_ref(),
            kernel: &config.kernel,
            rootfs: &config.rootfs,
            rootfs_type: &config.rootfs_type,
            reset_overlay: recreate,
            network_socket: network.as_ref().map(|p| p.socket_path()),
            cmdline: &config.kernel_cmdline,
            memory_mb: config.memory_mb,
            cpus: config.cpus as f64,
            mounts: &mounts,
            console: console.take_slave()?,
        };
        let mut vm = hypervisor.boot(cfg)?;
        match console.read_until_terminated()? {
            ConsoleExit::GuestGone => {
                vm.wait().context("waiting for sandbox to exit")?;
            }
            ConsoleExit::Terminated => drop(vm),
        }
    }

    // Drop must happen here because delete will try to acquire the lock too
    drop(lock);
    if delete_after_shutdown {
        delete_sandbox(Some(&config), &config.name, true)?;
    }

    Ok(())
}

fn delete_sandbox(
    config: Option<&config::Config>,
    name: &Name,
    force: bool,
) -> Result<(), anyhow::Error> {
    let sandbox_runtime_dir = runtime_dir().join(name);
    let sandbox_state_dir = state_dir()?.join(name);
    let hypervisor = hypervisor::for_sandbox(config, &sandbox_runtime_dir);

    let lock = SandboxLock::try_acquire(name)?;
    if !force && (lock.is_none() || hypervisor.is_running(&sandbox_runtime_dir)) {
        anyhow::bail!(
            "can't delete a running sandbox, either use --force or shut the sandbox down and then try again"
        );
    }

    hypervisor.shutdown(&sandbox_runtime_dir, force)?;
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

fn list_sandboxes(config: Option<&config::Config>) -> Result<(), anyhow::Error> {
    let base_dir = runtime_dir();
    let sandbox_infos: Vec<SandboxInfo> = existing_sandbox_names()
        .into_iter()
        .map(|name| {
            let sandbox_runtime_dir = base_dir.join(&name);
            SandboxInfo {
                state: hypervisor::for_sandbox(config, &sandbox_runtime_dir)
                    .state(&sandbox_runtime_dir)
                    .to_string(),
                name,
            }
        })
        .collect();

    println!("{}", Table::new(sandbox_infos));

    Ok(())
}

fn prune_sandboxes(config: Option<&config::Config>) -> Result<(), anyhow::Error> {
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
            && let sandbox_runtime_dir = runtime_dir().join(&name)
            && !hypervisor::for_sandbox(config, &sandbox_runtime_dir)
                .is_running(&sandbox_runtime_dir)
        {
            if let Err(err) = delete_sandbox(config, &name, false) {
                println!("{err}");
                log::warn!("could not delete {name}: {err}");
            }
        } else {
            log::warn!("unexpected invalid sandbox name: {sandbox}");
        }
    }

    Ok(())
}

fn ensure_unique_sandbox_name(
    config: Option<&config::Config>,
    name: &Name,
) -> Result<(), anyhow::Error> {
    let sandbox_runtime_dir = runtime_dir().join(name);
    if hypervisor::for_sandbox(config, &sandbox_runtime_dir).is_running(&sandbox_runtime_dir) {
        anyhow::bail!("a sandbox with the same name already exists");
    }
    Ok(())
}

fn existing_sandbox_names() -> Vec<String> {
    sandbox_names_in([Some(runtime_dir()), state_dir().ok()])
}

fn sandbox_names_in(dirs: impl IntoIterator<Item = Option<PathBuf>>) -> Vec<String> {
    let mut names: Vec<String> = dirs
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
    let mut paths = vec![socket_path(name, "passt")];
    for share in shares {
        paths.push(socket_path(name, &format!("vfsd-{}", share.name)));
    }

    for path in hypervisor::socket_paths_to_validate() {
        paths.push(socket_path(name, path));
    }

    for path in paths {
        let length = path.as_os_str().len();
        if length > MAX_SOCKET_PATH_LENGTH {
            anyhow::bail!(
                "socket path `{}` is {length} bytes, which is over the {MAX_SOCKET_PATH_LENGTH} \
                 byte limit for unix sockets, use a shorter sandbox or share name",
                path.display()
            );
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
            git_action: None,
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
    fn sandbox_names_in_still_lists_runtime_when_state_dir_is_unresolvable() {
        // `state_dir()` can't be forced to fail through the environment (`home_dir` falls back to
        // the passwd entry), so the unresolvable case is passed in directly.
        // This is also the cause of why we split existing_sandbox_names into two methods

        let tmp = tempfile::tempdir().unwrap();
        let xdg_runtime = tmp.path().join("runtime");
        std::fs::create_dir_all(xdg_runtime.join("only-runtime")).unwrap();

        assert_eq!(
            sandbox_names_in([Some(xdg_runtime), None]),
            vec!["only-runtime".to_string()]
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
