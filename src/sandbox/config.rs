use garde::Validate;
use serde::{Deserialize, Deserializer};
use std::{
    fmt,
    path::{Path, PathBuf},
};

use crate::sandbox::name::Name;

#[derive(Deserialize, Default)]
#[serde(deny_unknown_fields)]
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
#[serde(deny_unknown_fields)]
pub struct Config {
    #[garde(skip)]
    #[serde(default = "default_sandbox_name")]
    pub name: Name,
    #[serde(deserialize_with = "deserialize_absolute_path")]
    #[garde(custom(path_exists))]
    pub kernel: PathBuf,
    #[garde(custom(validate_kernel_cmdline))]
    #[serde(default)]
    pub kernel_cmdline: String,
    #[garde(custom(path_exists))]
    #[serde(deserialize_with = "deserialize_absolute_path")]
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
    #[garde(skip)]
    #[serde(default = "default_guest_uid_gid")]
    pub sandbox_user_uid: u32,
    #[garde(skip)]
    #[serde(default = "default_guest_uid_gid")]
    pub sandbox_user_gid: u32,
    #[garde(inner(inner(ip)))]
    pub dns: Option<Vec<String>>,
    #[garde(dive)]
    pub cloud_hypervisor: Option<BinaryConfig>,
    #[garde(dive)]
    pub passt: Option<BinaryConfig>,
    #[garde(dive)]
    pub pasta: Option<BinaryConfig>,
    #[garde(dive)]
    pub virtiofsd: Option<BinaryConfig>,
}

#[derive(Clone, Debug, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct FsShare {
    #[garde(custom(path_exists))]
    #[serde(deserialize_with = "deserialize_absolute_path")]
    pub host_dir: PathBuf,
    #[garde(skip)]
    pub name: Name,
    #[garde(skip)]
    #[serde(default)]
    // For the whole share
    pub read_only: bool,
}

#[derive(Deserialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct BinaryConfig {
    #[garde(custom(path_exists_optional))]
    pub binary: Option<PathBuf>,
}

fn default_sandbox_name() -> Name {
    std::env::current_dir()
        .ok()
        .and_then(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .and_then(Name::sanitize)
        })
        .unwrap_or_else(Name::randomized)
}

fn default_guest_uid_gid() -> u32 {
    1000
}

fn deserialize_absolute_path<'de, D>(deserializer: D) -> Result<PathBuf, D::Error>
where
    D: Deserializer<'de>,
{
    let raw = PathBuf::deserialize(deserializer)?;
    std::fs::canonicalize(&raw).map_err(serde::de::Error::custom)
}

fn deserialize_absolute_paths<'de, D>(deserializer: D) -> Result<Vec<PathBuf>, D::Error>
where
    D: Deserializer<'de>,
{
    let raw = Vec::<PathBuf>::deserialize(deserializer)?;
    raw.into_iter()
        .map(|p| std::fs::canonicalize(&p).map_err(serde::de::Error::custom))
        .collect()
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
        Some(name) => Name::new(name).map_err(|error| format!("invalid share {error}"))?,
        None => host_dir
            .file_name()
            .and_then(|name| name.to_str())
            .and_then(Name::sanitize)
            .unwrap_or_else(Name::randomized),
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
        assert!(path_exists(dir.path(), &()).is_ok());
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

    #[test]
    fn parse_share_rejects_names_that_are_not_path_components() {
        let dir = tempdir().unwrap();
        let path = dir.path().display();
        assert!(parse_share(&format!("../../etc:{path}:rw")).is_err());
        assert!(parse_share(&format!("a init=-bin-sh:{path}:rw")).is_err());
    }

    #[test]
    fn parse_share_accepts_every_documented_form() {
        let dir = tempdir().unwrap();
        let path = dir.path().display().to_string();

        let share = parse_share(&path).unwrap();
        assert!(!share.read_only);

        let share = parse_share(&format!("{path}:ro")).unwrap();
        assert!(share.read_only);

        let share = parse_share(&format!("data:{path}:rw")).unwrap();
        assert_eq!(share.name.as_str(), "data");
        assert!(!share.read_only);
    }
}
