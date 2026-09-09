use std::{
    os::unix::fs::DirBuilderExt,
    path::{Path, PathBuf},
};

use anyhow::Context;
use garde::Validate;
use serde::Deserialize;

use crate::sandbox::{self, config::FsShare};

const CONFIG_FILE_NAME: &str = "blinools.toml";

#[derive(Deserialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct Config {
    #[garde[dive]]
    pub sandbox: Option<sandbox::config::Config>,
}

pub fn runtime_dir() -> PathBuf {
    std::env::var_os("XDG_RUNTIME_DIR")
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            let uid = unsafe { libc::getuid() };
            PathBuf::from("/run/user").join(uid.to_string())
        })
        .join("blinools")
}

pub fn state_dir() -> Result<PathBuf, anyhow::Error> {
    Ok(std::env::var_os("XDG_STATE_HOME")
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .or_else(|| std::env::home_dir().map(|h| h.join(".local").join("state")))
        .context("locating state directory with XDG_STATE_HOME or HOME/.local/state")?
        .join("blinools"))
}

fn config_dir() -> Option<PathBuf> {
    std::env::var_os("XDG_CONFIG_HOME")
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .or_else(|| std::env::home_dir().map(|h| h.join(".config")))
        .map(|p| p.join("blinools"))
}

pub fn setup_dirs() -> Result<(), anyhow::Error> {
    create_dir(&runtime_dir()).context("creating runtime directory")?;
    create_dir(&state_dir()?).context("creating state directory")?;

    Ok(())
}

pub fn create_dir(path: &Path) -> Result<(), std::io::Error> {
    std::fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(path)
}

pub fn parse_config(config_file: Option<&String>) -> Result<Config, anyhow::Error> {
    parse_config_with(
        config_dir().map(|p| p.join(CONFIG_FILE_NAME)).as_ref(),
        project_file(config_file.map(String::as_str), Path::new(CONFIG_FILE_NAME)),
    )
}

fn project_file<'a>(cli_config_file: Option<&'a str>, local_file: &'a Path) -> Option<&'a str> {
    match cli_config_file {
        Some(config_file) => Some(config_file),
        None => local_file.to_str().filter(|_| local_file.exists()),
    }
}

fn parse_config_with(
    global_file: Option<&PathBuf>,
    project_file: Option<&str>,
) -> Result<Config, anyhow::Error> {
    let mut builder = config::Config::builder();

    if let Some(global_file) = global_file {
        builder = builder.add_source(config::File::from(global_file.clone()).required(false));
    }

    if let Some(project_file) = project_file {
        builder = builder.add_source(config::File::with_name(project_file).required(true));
    }

    let mut config: Config = builder
        .build()
        .context("reading config file")?
        .try_deserialize()
        .context("parsing config file")?;

    if let (Some(sandbox), Some(global_file), Some(project_file)) =
        (config.sandbox.as_mut(), global_file, project_file)
    {
        sandbox.shares = Some(resolve_shares(
            sandbox.inherit_shares,
            global_file,
            project_file,
        )?);
    }

    config.validate().context("validating config file")?;
    Ok(config)
}

fn resolve_shares(
    inherit_shares: bool,
    global_file: &Path,
    project_file: &str,
) -> Result<Vec<FsShare>, anyhow::Error> {
    let overrides = shares_from(config::File::with_name(project_file).required(true))?;
    let inherited = if inherit_shares {
        shares_from(config::File::from(global_file.to_path_buf()).required(false))?
    } else {
        None
    };

    Ok(sandbox::config::merge_by_name(
        inherited.as_ref(),
        overrides.unwrap_or_default(),
    ))
}

fn shares_from<S>(source: S) -> Result<Option<Vec<FsShare>>, anyhow::Error>
where
    S: config::Source + Send + Sync + 'static,
{
    match config::Config::builder()
        .add_source(source)
        .build()
        .context("reading config file")?
        .get::<Vec<FsShare>>("sandbox.shares")
    {
        Ok(shares) => Ok(Some(shares)),
        Err(config::ConfigError::NotFound(_)) => Ok(None),
        Err(error) => Err(anyhow::Error::new(error).context("parsing shares")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serial_test::serial;
    use std::{ffi::OsString, fs, os::unix::fs::PermissionsExt};
    use tempfile::tempdir;

    fn base_config(dir: &Path) -> String {
        let kernel = dir.join("kernel");
        let rootfs = dir.join("rootfs.img");
        fs::write(&kernel, b"kernel").unwrap();
        fs::write(&rootfs, b"rootfs").unwrap();

        format!(
            "[sandbox]\nkernel = \"{}\"\nrootfs = \"{}\"\nmemory_mb = 512\ncpus = 1\n",
            kernel.display(),
            rootfs.display(),
        )
    }

    fn write_config(dir: &Path, name: &str, body: &str) -> PathBuf {
        let path = dir.join(name);
        fs::write(&path, body).unwrap();

        path
    }

    fn share_dir(root: &Path, name: &str) -> PathBuf {
        let path = root.join(name);
        fs::create_dir(&path).unwrap();

        path.canonicalize().unwrap()
    }

    fn share_entry(name: &str, host_dir: &Path) -> String {
        format!(
            "shares = [{{ name = \"{name}\", host_dir = \"{}\" }}]\n",
            host_dir.display(),
        )
    }

    fn shares(config: &Config) -> Vec<(String, PathBuf)> {
        config
            .sandbox
            .as_ref()
            .unwrap()
            .shares
            .iter()
            .flatten()
            .map(|share| (share.name.as_str().to_owned(), share.host_dir.clone()))
            .collect()
    }

    #[test]
    fn the_local_config_file_is_used_when_it_exists() {
        let tmp = tempdir().unwrap();
        let local = tmp.path().join(CONFIG_FILE_NAME);
        fs::write(&local, "").unwrap();

        assert_eq!(project_file(None, &local), local.to_str());
    }

    #[test]
    fn a_missing_local_config_file_is_skipped() {
        let tmp = tempdir().unwrap();
        let local = tmp.path().join(CONFIG_FILE_NAME);

        assert_eq!(project_file(None, &local), None);
    }

    #[test]
    fn the_config_flag_replaces_the_local_config_file() {
        let tmp = tempdir().unwrap();
        let local = tmp.path().join(CONFIG_FILE_NAME);
        fs::write(&local, "").unwrap();

        assert_eq!(
            project_file(Some("/somewhere/else.toml"), &local),
            Some("/somewhere/else.toml")
        );
    }

    #[test]
    fn global_shares_apply_without_a_config_file() {
        let tmp = tempdir().unwrap();
        let data = share_dir(tmp.path(), "data");
        let global = write_config(
            tmp.path(),
            "global.toml",
            &format!("{}{}", base_config(tmp.path()), share_entry("data", &data)),
        );

        let config = parse_config_with(Some(&global), None).unwrap();

        assert_eq!(shares(&config), [(String::from("data"), data)]);
    }

    #[test]
    fn global_shares_apply_without_a_config_file_even_when_inheritance_is_on() {
        let tmp = tempdir().unwrap();
        let data = share_dir(tmp.path(), "data");
        let global = write_config(
            tmp.path(),
            "global.toml",
            &format!(
                "{}inherit_shares = true\n{}",
                base_config(tmp.path()),
                share_entry("data", &data),
            ),
        );

        let config = parse_config_with(Some(&global), None).unwrap();

        assert_eq!(shares(&config), [(String::from("data"), data)]);
    }

    // This is only valid since the only config is the sandbox config
    #[test]
    fn config_must_not_be_built_when_global_file_and_config_file_are_not_valid() {
        assert!(parse_config_with(None, None).unwrap().sandbox.is_none());
        assert!(
            parse_config_with(Some(&PathBuf::new()), None)
                .unwrap()
                .sandbox
                .is_none()
        );
        assert!(parse_config_with(Some(&PathBuf::new()), Some("")).is_err());
        assert!(parse_config_with(Some(&PathBuf::new()), Some("i dont exist")).is_err());
    }

    #[test]
    fn config_file_shares_replace_global_shares_by_default() {
        let tmp = tempdir().unwrap();
        let global_dir = share_dir(tmp.path(), "from-global");
        let project_dir = share_dir(tmp.path(), "from-project");
        let base = base_config(tmp.path());
        let global = write_config(
            tmp.path(),
            "global.toml",
            &format!("{base}{}", share_entry("global", &global_dir)),
        );
        let project = write_config(
            tmp.path(),
            "project.toml",
            &format!("{base}{}", share_entry("project", &project_dir)),
        );

        let config =
            parse_config_with(Some(&global), Some(&project.display().to_string())).unwrap();

        assert_eq!(shares(&config), [(String::from("project"), project_dir)]);
    }

    #[test]
    fn config_file_without_shares_drops_global_shares() {
        let tmp = tempdir().unwrap();
        let global_dir = share_dir(tmp.path(), "from-global");
        let base = base_config(tmp.path());
        let global = write_config(
            tmp.path(),
            "global.toml",
            &format!("{base}{}", share_entry("global", &global_dir)),
        );
        let project = write_config(tmp.path(), "project.toml", &base);

        let config =
            parse_config_with(Some(&global), Some(&project.display().to_string())).unwrap();

        assert!(shares(&config).is_empty());
    }

    #[test]
    fn config_file_without_a_sandbox_section_drops_global_shares() {
        let tmp = tempdir().unwrap();
        let global_dir = share_dir(tmp.path(), "from-global");
        let global = write_config(
            tmp.path(),
            "global.toml",
            &format!(
                "{}{}",
                base_config(tmp.path()),
                share_entry("global", &global_dir),
            ),
        );
        let project = write_config(tmp.path(), "project.toml", "");

        let config =
            parse_config_with(Some(&global), Some(&project.display().to_string())).unwrap();

        assert!(shares(&config).is_empty());
    }

    #[test]
    fn inherited_shares_are_merged_with_the_config_file_shares() {
        let tmp = tempdir().unwrap();
        let global_dir = share_dir(tmp.path(), "from-global");
        let project_dir = share_dir(tmp.path(), "from-project");
        let base = base_config(tmp.path());
        let global = write_config(
            tmp.path(),
            "global.toml",
            &format!("{base}{}", share_entry("global", &global_dir)),
        );
        let project = write_config(
            tmp.path(),
            "project.toml",
            &format!(
                "{base}inherit_shares = true\n{}",
                share_entry("project", &project_dir),
            ),
        );

        let config =
            parse_config_with(Some(&global), Some(&project.display().to_string())).unwrap();

        assert_eq!(
            shares(&config),
            [
                (String::from("global"), global_dir),
                (String::from("project"), project_dir),
            ]
        );
    }

    #[test]
    fn config_file_shares_win_over_inherited_shares_with_the_same_name() {
        let tmp = tempdir().unwrap();
        let global_dir = share_dir(tmp.path(), "from-global");
        let project_dir = share_dir(tmp.path(), "from-project");
        let base = base_config(tmp.path());
        let global = write_config(
            tmp.path(),
            "global.toml",
            &format!("{base}{}", share_entry("data", &global_dir)),
        );
        let project = write_config(
            tmp.path(),
            "project.toml",
            &format!(
                "{base}inherit_shares = true\n{}",
                share_entry("data", &project_dir),
            ),
        );

        let config =
            parse_config_with(Some(&global), Some(&project.display().to_string())).unwrap();

        assert_eq!(shares(&config), [(String::from("data"), project_dir)]);
    }

    #[test]
    fn inherit_shares_can_be_set_in_the_global_config() {
        let tmp = tempdir().unwrap();
        let global_dir = share_dir(tmp.path(), "from-global");
        let project_dir = share_dir(tmp.path(), "from-project");
        let base = base_config(tmp.path());
        let global = write_config(
            tmp.path(),
            "global.toml",
            &format!(
                "{base}inherit_shares = true\n{}",
                share_entry("global", &global_dir),
            ),
        );
        let project = write_config(
            tmp.path(),
            "project.toml",
            &format!("{base}{}", share_entry("project", &project_dir)),
        );

        let config =
            parse_config_with(Some(&global), Some(&project.display().to_string())).unwrap();

        assert_eq!(
            shares(&config),
            [
                (String::from("global"), global_dir),
                (String::from("project"), project_dir),
            ]
        );
    }

    #[test]
    fn config_file_inherit_shares_overrides_the_global_one() {
        let tmp = tempdir().unwrap();
        let global_dir = share_dir(tmp.path(), "from-global");
        let project_dir = share_dir(tmp.path(), "from-project");
        let base = base_config(tmp.path());
        let global = write_config(
            tmp.path(),
            "global.toml",
            &format!(
                "{base}inherit_shares = true\n{}",
                share_entry("global", &global_dir),
            ),
        );
        let project = write_config(
            tmp.path(),
            "project.toml",
            &format!(
                "{base}inherit_shares = false\n{}",
                share_entry("project", &project_dir),
            ),
        );

        let config =
            parse_config_with(Some(&global), Some(&project.display().to_string())).unwrap();

        assert_eq!(shares(&config), [(String::from("project"), project_dir)]);
    }

    #[test]
    fn global_shares_are_not_read_when_inheritance_is_off() {
        let tmp = tempdir().unwrap();
        let missing = tmp.path().join("deleted");
        let project_dir = share_dir(tmp.path(), "from-project");
        let base = base_config(tmp.path());
        let global = write_config(
            tmp.path(),
            "global.toml",
            &format!("{base}{}", share_entry("global", &missing)),
        );
        let project = write_config(
            tmp.path(),
            "project.toml",
            &format!("{base}{}", share_entry("project", &project_dir)),
        );

        let config =
            parse_config_with(Some(&global), Some(&project.display().to_string())).unwrap();

        assert_eq!(shares(&config), [(String::from("project"), project_dir)]);
    }

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

    fn mode_of(path: &Path) -> u32 {
        std::fs::metadata(path).unwrap().permissions().mode() & 0o777
    }

    #[test]
    #[serial]
    fn runtime_dir_uses_xdg_runtime_dir_when_set() {
        with_env(&[("XDG_RUNTIME_DIR", Some("/tmp/xrd"))], || {
            assert_eq!(runtime_dir(), PathBuf::from("/tmp/xrd/blinools"));
        });
    }

    #[test]
    #[serial]
    fn runtime_dir_falls_back_when_var_empty() {
        with_env(&[("XDG_RUNTIME_DIR", Some(""))], || {
            let uid = unsafe { libc::getuid() };
            assert_eq!(
                runtime_dir(),
                PathBuf::from(format!("/run/user/{uid}/blinools"))
            );
        });
    }

    #[test]
    #[serial]
    fn runtime_dir_falls_back_when_var_unset() {
        with_env(&[("XDG_RUNTIME_DIR", None)], || {
            let uid = unsafe { libc::getuid() };
            assert_eq!(
                runtime_dir(),
                PathBuf::from(format!("/run/user/{uid}/blinools"))
            );
        });
    }

    #[test]
    #[serial]
    fn state_dir_uses_xdg_state_home_when_set() {
        with_env(
            &[
                ("XDG_STATE_HOME", Some("/tmp/xsh")),
                ("HOME", Some("/tmp/home")),
            ],
            || {
                assert_eq!(state_dir().unwrap(), PathBuf::from("/tmp/xsh/blinools"));
            },
        );
    }

    #[test]
    #[serial]
    fn state_dir_falls_back_to_home_when_var_empty() {
        with_env(
            &[("XDG_STATE_HOME", Some("")), ("HOME", Some("/tmp/home"))],
            || {
                assert_eq!(
                    state_dir().unwrap(),
                    PathBuf::from("/tmp/home/.local/state/blinools")
                );
            },
        );
    }

    #[test]
    #[serial]
    fn state_dir_falls_back_to_home_when_var_unset() {
        with_env(
            &[("XDG_STATE_HOME", None), ("HOME", Some("/tmp/home"))],
            || {
                assert_eq!(
                    state_dir().unwrap(),
                    PathBuf::from("/tmp/home/.local/state/blinools")
                );
            },
        );
    }

    #[test]
    #[serial]
    fn state_dir_still_returns_when_nothing_available() {
        with_env(&[("XDG_STATE_HOME", None), ("HOME", None)], || {
            assert!(state_dir().is_ok());
        });
    }

    #[test]
    #[serial]
    fn config_dir_uses_xdg_config_home_when_set() {
        with_env(
            &[
                ("XDG_CONFIG_HOME", Some("/tmp/xch")),
                ("HOME", Some("/tmp/home")),
            ],
            || {
                assert_eq!(config_dir(), Some(PathBuf::from("/tmp/xch/blinools")));
            },
        );
    }

    #[test]
    #[serial]
    fn config_dir_falls_back_to_home_when_var_empty() {
        with_env(
            &[("XDG_CONFIG_HOME", Some("")), ("HOME", Some("/tmp/home"))],
            || {
                assert_eq!(
                    config_dir(),
                    Some(PathBuf::from("/tmp/home/.config/blinools"))
                );
            },
        );
    }

    #[test]
    #[serial]
    fn config_dir_falls_back_to_home_when_var_unset() {
        with_env(
            &[("XDG_CONFIG_HOME", None), ("HOME", Some("/tmp/home"))],
            || {
                assert_eq!(
                    config_dir(),
                    Some(PathBuf::from("/tmp/home/.config/blinools"))
                );
            },
        );
    }

    #[test]
    #[serial]
    fn config_dir_still_returns_when_nothing_available() {
        with_env(&[("XDG_CONFIG_HOME", None), ("HOME", None)], || {
            assert!(config_dir().is_some());
        });
    }

    #[test]
    fn create_dir_creates_nested_path() {
        let tmp = tempfile::tempdir().unwrap();
        let target = tmp.path().join("a/b/c");

        create_dir(&target).unwrap();

        assert!(target.is_dir());
        assert_eq!(mode_of(&target), 0o700);
    }

    #[test]
    fn create_dir_is_idempotent() {
        let tmp = tempfile::tempdir().unwrap();
        let target = tmp.path().join("again");

        create_dir(&target).unwrap();
        create_dir(&target).unwrap();

        assert!(target.is_dir());
    }

    #[test]
    fn create_dir_errors_if_path_component_is_a_file() {
        let tmp = tempfile::tempdir().unwrap();
        let blocker = tmp.path().join("blocker");
        std::fs::write(&blocker, b"not a dir").unwrap();

        let target = blocker.join("child");

        assert!(create_dir(&target).is_err());
    }

    #[test]
    #[serial]
    fn setup_dirs_creates_runtime_and_state_with_0700() {
        let tmp = tempfile::tempdir().unwrap();
        let runtime = tmp.path().join("runtime");
        let state_home = tmp.path().join("state");

        with_env(
            &[
                ("XDG_RUNTIME_DIR", Some(runtime.to_str().unwrap())),
                ("XDG_STATE_HOME", Some(state_home.to_str().unwrap())),
            ],
            || {
                let runtime_target = runtime.join("blinools");
                let state_target = state_home.join("blinools");

                setup_dirs().unwrap();
                assert!(runtime_target.is_dir());
                assert!(state_target.is_dir());
                assert_eq!(mode_of(&runtime_target), 0o700);
                assert_eq!(mode_of(&state_target), 0o700);
            },
        );
    }

    #[test]
    #[serial]
    fn setup_dirs_is_idempotent() {
        let tmp = tempfile::tempdir().unwrap();
        let runtime = tmp.path().join("runtime");
        let state_home = tmp.path().join("state");

        with_env(
            &[
                ("XDG_RUNTIME_DIR", Some(runtime.to_str().unwrap())),
                ("XDG_STATE_HOME", Some(state_home.to_str().unwrap())),
            ],
            || {
                let runtime_target = runtime.join("blinools");
                let state_target = state_home.join("blinools");

                setup_dirs().unwrap();
                assert!(runtime_target.is_dir());
                assert!(state_target.is_dir());
                assert_eq!(mode_of(&runtime_target), 0o700);
                assert_eq!(mode_of(&state_target), 0o700);

                setup_dirs().unwrap();
                assert!(runtime_target.is_dir());
                assert!(state_target.is_dir());
                assert_eq!(mode_of(&runtime_target), 0o700);
                assert_eq!(mode_of(&state_target), 0o700);
            },
        );
    }

    #[test]
    #[serial]
    fn setup_dirs_does_not_create_config_dir() {
        let tmp = tempfile::tempdir().unwrap();
        let runtime = tmp.path().join("runtime");
        let state_home = tmp.path().join("state");
        let config_home = tmp.path().join("config");

        with_env(
            &[
                ("XDG_RUNTIME_DIR", Some(runtime.to_str().unwrap())),
                ("XDG_STATE_HOME", Some(state_home.to_str().unwrap())),
                ("XDG_CONFIG_HOME", Some(config_home.to_str().unwrap())),
            ],
            || {
                setup_dirs().unwrap();
                assert!(!config_home.join("blinools").exists());
            },
        );
    }
}
