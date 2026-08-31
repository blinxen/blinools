use std::path::{Path, PathBuf};

use garde::Validate;
use serde::Deserialize;

#[derive(Deserialize, Default)]
pub enum RootfsType {
    Raw,
    #[default]
    QCOW2,
}

#[derive(Deserialize, Validate)]
pub struct Config {
    #[garde(custom(path_exists))]
    pub kernel: PathBuf,
    #[garde(custom(validate_kernel_cmdline))]
    pub kernel_cmdline: Option<String>,
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
    #[garde(alphanumeric)]
    pub name: String,
    #[garde(skip)]
    #[serde(default)]
    pub read_only: bool,
}

#[derive(Deserialize, Validate)]
pub struct ChConfig {
    #[garde(custom(path_exists_optional))]
    pub cloud_hypervisor_binary: Option<PathBuf>,
    #[garde(custom(path_exists_optional))]
    pub ch_remote_binary: Option<PathBuf>,
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

fn validate_kernel_cmdline(value: &Option<String>, _ctx: &()) -> garde::Result {
    let Some(value) = value else { return Ok(()) };

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
    let (name, rest) = s
        .split_once(':')
        .ok_or_else(|| format!("invalid share `{s}`, expected TAG:PATH:(ro|rw)"))?;

    let (path, mode) = rest
        .rsplit_once(':')
        .ok_or_else(|| format!("invalid share `{s}`, expected TAG:PATH:(ro|rw)"))?;

    let read_only = match mode {
        "ro" => true,
        "rw" => false,
        other => return Err(format!("invalid mode `{other}`, expected `ro` or `rw`")),
    };

    let share = FsShare {
        host_dir: PathBuf::from(path),
        name: name.to_owned(),
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
