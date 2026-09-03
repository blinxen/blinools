use garde::Validate;
use rand::distr::{Alphanumeric, SampleString};
use serde::Deserialize;
use std::{
    fmt,
    path::{Path, PathBuf},
};

#[derive(Deserialize, Default)]
pub enum RootfsType {
    #[default]
    Raw,
    QCOW2,
}

impl fmt::Display for RootfsType {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            RootfsType::Raw => write!(f, "raw"),
            RootfsType::QCOW2 => write!(f, "qcow2"),
        }
    }
}

#[derive(Deserialize, Validate)]
pub struct Config {
    #[garde(skip)]
    #[serde(default = "default_sandbox_name")]
    pub name: String,
    #[garde(custom(path_exists))]
    pub kernel: PathBuf,
    #[garde(custom(validate_kernel_cmdline))]
    #[serde(default)]
    pub kernel_cmdline: String,
    #[garde(custom(path_exists))]
    pub rootfs: PathBuf,
    #[garde(skip)]
    #[serde(default)]
    pub rootfs_type: RootfsType,
    #[garde(range(min = 512, max = 131072))]
    pub memory_mb: u64,
    #[garde(range(min = 1, max = 255))]
    pub cpus: u8,
    #[garde(dive)]
    pub shares: Option<Vec<FsShare>>,
    #[garde(inner(inner(ip)))]
    pub dns: Option<Vec<String>>,
    #[garde(dive)]
    pub cloud_hypervisor: Option<ChConfig>,
    #[garde(dive)]
    pub passt: Option<PasstConfig>,
    #[garde(dive)]
    pub virtiofsd: Option<VirtiofsdConfig>,
}

#[derive(Clone, Debug, Deserialize, Validate)]
pub struct FsShare {
    #[garde(custom(path_exists))]
    pub host_dir: PathBuf,
    #[garde(ascii)]
    pub name: String,
    #[garde(skip)]
    #[serde(default)]
    pub read_only: bool,
}

#[derive(Deserialize, Validate)]
pub struct ChConfig {
    #[garde(custom(path_exists_optional))]
    pub cloud_hypervisor_binary: Option<PathBuf>,
}

#[derive(Deserialize, Validate)]
pub struct PasstConfig {
    #[garde(custom(path_exists_optional))]
    pub binary: Option<PathBuf>,
}

#[derive(Deserialize, Validate)]
pub struct VirtiofsdConfig {
    #[garde(custom(path_exists_optional))]
    pub binary: Option<PathBuf>,
}

fn default_sandbox_name() -> String {
    std::env::current_dir()
        .ok()
        .and_then(|p| p.file_name().and_then(|s| s.to_str()).map(String::from))
        .unwrap_or(Alphanumeric.sample_string(&mut rand::rng(), 16))
}

fn validate_kernel_cmdline(value: &str, _ctx: &()) -> garde::Result {
    if value.contains("console=") {
        return Err(garde::Error::new(
            "Kernel command line parameters must not configure `console`. `console` is hardcoded to `hvc0` and cannot be changed.",
        ));
    }

    if value.contains("root=") {
        return Err(garde::Error::new(
            "Kernel command line parameters must not configure `root`. `root` is hardcoded to `/dev/vda` and cannot be changed.",
        ));
    }

    Ok(())
}

fn path_exists(value: &Path, _ctx: &()) -> garde::Result {
    if value.exists() {
        Ok(())
    } else {
        Err(garde::Error::new(format!(
            "Path `{}` does not exist",
            value.display()
        )))
    }
}

fn path_exists_optional(value: &Option<PathBuf>, _ctx: &()) -> garde::Result {
    let Some(value) = value else { return Ok(()) };

    if value.exists() {
        Ok(())
    } else {
        Err(garde::Error::new(format!(
            "Path `{}` does not exist",
            value.display()
        )))
    }
}

// Used only by clap parser
pub fn parse_share(s: &str) -> Result<FsShare, String> {
    let usage =
        || format!("invalid share `{s}`, expected PATH, PATH:(ro|rw), or NAME:PATH:(ro|rw)");

    let parts: Vec<&str> = s.split(':').collect();
    let (name, path, mode): (Option<&str>, &str, Option<&str>) = match parts.as_slice() {
        [path] => (None, path, None),
        [path, mode] => (None, path, Some(mode)),
        [name, path, mode] => (Some(name), path, Some(mode)),
        _ => return Err(usage()),
    };

    let read_only = match mode {
        None => false,
        Some("ro") => true,
        Some("rw") => false,
        Some(other) => return Err(format!("invalid mode `{other}`, expected `ro` or `rw`")),
    };

    let host_dir =
        std::fs::canonicalize(path).map_err(|_| String::from("could not make path absolute"))?;
    let name = match name {
        Some(n) => n.to_owned(),
        None => host_dir
            .file_name()
            .and_then(|s| s.to_str())
            .map(String::from)
            .unwrap_or_else(|| Alphanumeric.sample_string(&mut rand::rng(), 16)),
    };

    let share = FsShare {
        host_dir,
        name,
        read_only,
    };
    share.validate().map_err(|e| e.to_string())?;
    Ok(share)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn path_exists_accepts_real_dir() {
        let dir = tempdir().unwrap();
        assert!(path_exists(&dir.path().to_path_buf(), &()).is_ok());
        assert!(path_exists_optional(&Some(dir.path().to_path_buf()), &()).is_ok());
    }

    #[test]
    fn path_exists_rejects_missing_dir() {
        let missing = PathBuf::from("/definitely/not/a/real/path/xyz");
        let result = path_exists(&missing, &());
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("does not exist"));
        let result2 = path_exists_optional(&Some(missing), &());
        assert!(result2.is_err());
        assert!(result2.unwrap_err().to_string().contains("does not exist"));
    }

    #[test]
    fn path_exists_optional_accepts_none() {
        assert!(path_exists_optional(&None, &()).is_ok());
    }
}
