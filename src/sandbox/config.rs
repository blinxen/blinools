use std::path::{Path, PathBuf};

use garde::Validate;
use gix::hashtable::hash_set::HashSet;
use serde::Deserialize;

#[derive(Deserialize, Validate)]
pub struct Config {
    #[garde(custom(path_exists))]
    pub kernel: PathBuf,
    #[garde(skip)]
    pub kernel_cmdline: String,
    #[garde(custom(path_exists))]
    pub rootfs: PathBuf,
    #[garde(range(min = 512, max = 131072))]
    pub memory_mb: u64,
    #[garde(range(min = 1, max = 255))]
    pub cpus: u8,
    #[garde(custom(validate_shares))]
    pub shares: Option<Vec<FsShare>>,
    #[garde(ip)]
    pub dns: Option<String>,
    #[garde(dive)]
    pub cloud_hypervisor: Option<ChConfig>,
    #[garde(dive)]
    pub passt: Option<PasstConfig>,
    #[garde(dive)]
    pub virtiofsd: Option<VirtiofsdConfig>,
}

#[derive(Deserialize, Validate)]
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
    pub binary: Option<PathBuf>,
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

fn validate_shares(value: &Option<Vec<FsShare>>, _ctx: &()) -> garde::Result {
    let Some(shares) = value else { return Ok(()) };

    for share in shares {
        share
            .validate()
            .map_err(|e| garde::Error::new(e.to_string()))?;
    }

    let mut seen = HashSet::new();
    for share in shares {
        if !seen.insert(&share.name) {
            return Err(garde::Error::new(format!(
                "duplicate share name `{}`",
                share.name
            )));
        }
    }

    Ok(())
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

    #[test]
    fn validate_shares_accepts_none_or_empty() {
        assert!(validate_shares(&None, &()).is_ok());
        assert!(validate_shares(&Some(Vec::new()), &()).is_ok());
    }

    #[test]
    fn validate_shares_rejects_duplicate_name() {
        let share = FsShare {
            host_dir: PathBuf::from("/"),
            name: String::from("name"),
            read_only: false,
        };
        let share2 = FsShare {
            host_dir: PathBuf::from("/"),
            name: String::from("name"),
            read_only: false,
        };
        let result = validate_shares(&Some(vec![share, share2]), &());
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .eq("duplicate share name `name`")
        );
    }
}
