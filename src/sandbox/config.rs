use garde::Validate;
use serde::{Deserialize, Deserializer};
use std::{
    collections::HashMap,
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
    #[garde(custom(file_exists))]
    pub kernel: PathBuf,
    #[garde(custom(validate_kernel_cmdline))]
    #[serde(default)]
    pub kernel_cmdline: String,
    #[garde(custom(file_exists))]
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
    #[serde(default)]
    pub inherit_shares: bool,
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
    pub virtiofsd: Option<BinaryConfig>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FsShare {
    #[serde(deserialize_with = "deserialize_absolute_path")]
    pub host_dir: PathBuf,
    pub name: Name,
    #[serde(default)]
    // For the whole share
    pub read_only: bool,
    #[serde(default)]
    #[serde(deserialize_with = "deserialize_absolute_paths")]
    // Meant as just subpaths in the share, so only part of the share should be read only
    pub read_only_paths: Vec<PathBuf>,
    #[serde(default)]
    #[serde(deserialize_with = "deserialize_absolute_paths")]
    // Same as the read only paths but here we actually make the paths hidden so the sandbox
    // can't see
    pub hidden_paths: Vec<PathBuf>,
}

impl garde::Validate for FsShare {
    type Context = ();

    fn validate_into(
        &self,
        ctx: &Self::Context,
        parent: &mut dyn FnMut() -> garde::Path,
        report: &mut garde::Report,
    ) {
        if let Err(e) = dir_exists(&self.host_dir, ctx) {
            report.append(parent().join("host_dir"), e);
            return;
        }

        for (field, paths) in [
            ("read_only_paths", &self.read_only_paths),
            ("hidden_paths", &self.hidden_paths),
        ] {
            for (i, path) in paths.iter().enumerate() {
                if !path.starts_with(&self.host_dir) || path == &self.host_dir {
                    report.append(
                        parent().join(field).join(i),
                        garde::Error::new(format!(
                            "{} is not inside host_dir {}",
                            path.display(),
                            self.host_dir.display(),
                        )),
                    );
                }
            }
        }
    }
}

#[derive(Deserialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct BinaryConfig {
    #[garde(custom(file_exists_optional))]
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

fn file_exists(value: &Path, _ctx: &()) -> garde::Result {
    if value.is_file() {
        Ok(())
    } else {
        Err(garde::Error::new(format!(
            "Path `{}` does not exist or is not a file",
            value.display()
        )))
    }
}

fn dir_exists(value: &Path, _ctx: &()) -> garde::Result {
    if value.is_dir() {
        Ok(())
    } else {
        Err(garde::Error::new(format!(
            "Path `{}` does not exist or is not a directory",
            value.display()
        )))
    }
}

fn file_exists_optional(value: &Option<PathBuf>, ctx: &()) -> garde::Result {
    let Some(value) = value else { return Ok(()) };

    file_exists(value, ctx)
}

pub fn merge_by_name(base: Option<&Vec<FsShare>>, overrides: Vec<FsShare>) -> Vec<FsShare> {
    // TODO: Should probably warn about dangerous shares
    let mut shares: HashMap<Name, FsShare> = base
        .into_iter()
        .flatten()
        .map(|s| (s.name.clone(), s.clone()))
        .collect();

    for override_share in overrides {
        shares.insert(override_share.name.clone(), override_share);
    }

    let mut shares: Vec<FsShare> = shares.into_values().collect();
    shares.sort_by(|a, b| a.name.cmp(&b.name));

    shares
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
        read_only_paths: Vec::new(),
        hidden_paths: Vec::new(),
    };
    share.validate().map_err(|e| e.to_string())?;
    Ok(share)
}

#[cfg(test)]
mod tests {
    use super::*;
    use garde::Validate;
    use std::fs;
    use tempfile::tempdir;

    fn named_share(name: &str, dir: &str, read_only: bool) -> FsShare {
        FsShare {
            host_dir: PathBuf::from(dir),
            name: Name::new(name).unwrap(),
            read_only,
            read_only_paths: Vec::new(),
            hidden_paths: Vec::new(),
        }
    }

    fn share(
        host_dir: PathBuf,
        read_only_paths: Vec<PathBuf>,
        hidden_paths: Vec<PathBuf>,
    ) -> FsShare {
        FsShare {
            host_dir,
            name: Name::new("test-share").expect("valid name"),
            read_only: false,
            read_only_paths,
            hidden_paths,
        }
    }

    #[test]
    fn dir_exists_only_accepts_existing_dirs() {
        let dir = tempdir().unwrap();
        let file = dir.path().join("file");
        std::fs::File::create(&file).unwrap();
        assert!(dir_exists(dir.path(), &()).is_ok());
        assert!(dir_exists(&Path::new("i-dont-exist"), &()).is_err());
        assert!(dir_exists(&file, &()).is_err());
    }

    #[test]
    fn file_exists_only_accepts_existing_files() {
        let dir = tempdir().unwrap();
        let file = dir.path().join("file");
        std::fs::File::create(&file).unwrap();
        assert!(file_exists(dir.path(), &()).is_err());
        assert!(file_exists(&Path::new("i-dont-exist"), &()).is_err());
        assert!(file_exists(&file, &()).is_ok());
    }

    #[test]
    fn file_exists_optional_accepts_none() {
        assert!(file_exists_optional(&None, &()).is_ok());
    }

    #[test]
    fn blacklisted_kernel_parameters_are_rejected() {
        assert!(validate_kernel_cmdline("foo console= bar", &()).is_err());
        assert!(validate_kernel_cmdline("console= bar", &()).is_err());
        assert!(validate_kernel_cmdline("console=bar", &()).is_err());
        assert!(validate_kernel_cmdline("foo console=", &()).is_err());
        assert!(validate_kernel_cmdline("console", &()).is_ok());

        assert!(validate_kernel_cmdline("foo root= bar", &()).is_err());
        assert!(validate_kernel_cmdline("root= bar", &()).is_err());
        assert!(validate_kernel_cmdline("root=bar", &()).is_err());
        assert!(validate_kernel_cmdline("foo root=", &()).is_err());
        assert!(validate_kernel_cmdline("root", &()).is_ok());
    }

    #[test]
    fn parse_share_rejects_names_that_are_not_path_components() {
        let dir = tempdir().unwrap();
        let path = dir.path().display();
        assert!(parse_share(&format!("../../etc:{path}:rw")).is_err());
        assert!(parse_share(&format!("a init=-bin-sh:{path}:rw")).is_err());
    }

    #[test]
    fn parse_share_rejects_invalid_shares() {
        let dir = tempdir().unwrap();
        let path = dir.path().display();
        assert!(parse_share(&format!("{path}:")).is_err());
        assert!(parse_share(&format!("{path}:r")).is_err());
        assert!(parse_share(&format!("{path}:o")).is_err());
        assert!(parse_share(&format!("{path}:ro:")).is_err());
        assert!(parse_share(&format!("{path}:rw:")).is_err());
        assert!(parse_share(&format!("{path}:rw:data")).is_err());
        assert!(parse_share(&format!("data:{path}:rw:data")).is_err());
        assert!(parse_share(&format!("data:{path}rw")).is_err());
        assert!(parse_share(&format!("data{path}rw")).is_err());
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

    #[test]
    fn overriding_shares_win_by_name() {
        let base = vec![named_share("data", "/from/config", true)];
        let merged = merge_by_name(Some(&base), vec![named_share("data", "/from/cli", false)]);

        assert_eq!(merged.len(), 1);
        assert_eq!(merged[0].host_dir, PathBuf::from("/from/cli"));
        assert!(!merged[0].read_only);
    }

    #[test]
    fn merged_shares_are_ordered_deterministically() {
        let base = vec![
            named_share("zulu", "/z", false),
            named_share("alpha", "/a", false),
            named_share("mike", "/m", false),
        ];
        let merged = merge_by_name(Some(&base), vec![named_share("bravo", "/b", false)]);
        let names: Vec<&str> = merged.iter().map(|s| s.name.as_str()).collect();

        assert_eq!(names, ["alpha", "bravo", "mike", "zulu"]);
    }

    #[test]
    fn merging_without_a_base_keeps_the_overrides() {
        let merged = merge_by_name(None, vec![named_share("data", "/from/cli", false)]);

        assert_eq!(merged.len(), 1);
        assert_eq!(merged[0].name.as_str(), "data");
    }

    #[test]
    fn fs_share_valid_with_no_subpaths() {
        let tmp = tempfile::tempdir().unwrap();
        let host_dir = tmp.path().canonicalize().unwrap();

        let s = share(host_dir, vec![], vec![]);
        assert!(s.validate().is_ok());
    }

    #[test]
    fn fs_share_valid_with_nested_subpaths() {
        let tmp = tempfile::tempdir().unwrap();
        let host_dir = tmp.path().canonicalize().unwrap();
        let ro = host_dir.join("configs");
        let hidden = host_dir.join("secrets");
        fs::create_dir(&ro).unwrap();
        fs::create_dir(&hidden).unwrap();

        let s = share(host_dir, vec![ro], vec![hidden]);
        assert!(s.validate().is_ok());
    }

    #[test]
    fn fs_share_valid_with_deeply_nested_subpath() {
        let tmp = tempfile::tempdir().unwrap();
        let host_dir = tmp.path().canonicalize().unwrap();
        let deep = host_dir.join("a").join("b").join("c");
        fs::create_dir_all(&deep).unwrap();

        let s = share(host_dir, vec![deep], vec![]);
        assert!(s.validate().is_ok());
    }

    #[test]
    fn fs_share_rejects_missing_host_dir() {
        let tmp = tempfile::tempdir().unwrap();
        let missing = tmp.path().join("does-not-exist");

        let s = share(missing, vec![], vec![]);
        let report = s.validate().unwrap_err();
        assert!(garde::select!(report, host_dir).next().is_some());
    }

    #[test]
    fn fs_share_rejects_read_only_path_outside_host_dir() {
        let base = tempfile::tempdir().unwrap();
        let host_dir = base.path().join("share");
        let outside = base.path().join("elsewhere");
        fs::create_dir(&host_dir).unwrap();
        fs::create_dir(&outside).unwrap();
        let host_dir = host_dir.canonicalize().unwrap();
        let outside = outside.canonicalize().unwrap();

        let s = share(host_dir, vec![outside], vec![]);
        let report = s.validate().unwrap_err();
        assert!(garde::select!(report, read_only_paths[0]).next().is_some());
    }

    #[test]
    fn fs_share_rejects_hidden_path_outside_host_dir() {
        let base = tempfile::tempdir().unwrap();
        let host_dir = base.path().join("share");
        let outside = base.path().join("elsewhere");
        fs::create_dir(&host_dir).unwrap();
        fs::create_dir(&outside).unwrap();
        let host_dir = host_dir.canonicalize().unwrap();
        let outside = outside.canonicalize().unwrap();

        let s = share(host_dir, vec![], vec![outside]);
        let report = s.validate().unwrap_err();
        assert!(garde::select!(report, hidden_paths[0]).next().is_some());
    }

    #[test]
    fn fs_share_rejects_sibling_dir_with_shared_prefix() {
        // Regression test for the "starts_with must be component-wise"
        // requirement: /base/share2 must NOT pass as inside /base/share.
        let base = tempfile::tempdir().unwrap();
        let host_dir = base.path().join("share");
        let sibling = base.path().join("share2");
        fs::create_dir(&host_dir).unwrap();
        fs::create_dir(&sibling).unwrap();
        let host_dir = host_dir.canonicalize().unwrap();
        let sibling = sibling.canonicalize().unwrap();

        let s = share(host_dir, vec![sibling], vec![]);
        let report = s.validate().unwrap_err();
        assert!(garde::select!(report, read_only_paths[0]).next().is_some());
    }

    #[test]
    fn fs_share_rejects_host_dir_itself_as_hidden_path() {
        let tmp = tempfile::tempdir().unwrap();
        let host_dir = tmp.path().canonicalize().unwrap();

        // Listing host_dir itself as a "hidden path" would hide the whole
        // share via a subpath entry instead of an explicit share-level flag.
        let s = share(host_dir.clone(), vec![], vec![host_dir]);
        let report = s.validate().unwrap_err();
        assert!(garde::select!(report, hidden_paths[0]).next().is_some());
    }

    #[test]
    fn fs_share_reports_one_error_per_bad_path() {
        let base = tempfile::tempdir().unwrap();
        let host_dir = base.path().join("share");
        let outside_a = base.path().join("outside-a");
        let outside_b = base.path().join("outside-b");
        fs::create_dir(&host_dir).unwrap();
        fs::create_dir(&outside_a).unwrap();
        fs::create_dir(&outside_b).unwrap();
        let host_dir = host_dir.canonicalize().unwrap();
        let outside_a = outside_a.canonicalize().unwrap();
        let outside_b = outside_b.canonicalize().unwrap();

        let s = share(host_dir, vec![outside_a], vec![outside_b]);
        let report = s.validate().unwrap_err();

        assert_eq!(report.iter().count(), 2);
        assert!(garde::select!(report, read_only_paths[0]).next().is_some());
        assert!(garde::select!(report, hidden_paths[0]).next().is_some());
    }

    #[test]
    fn fs_share_errors_nest_correctly_through_a_vec_of_shares() {
        let base = tempfile::tempdir().unwrap();
        let good_host = base.path().join("good");
        let bad_host = base.path().join("bad");
        let outside = base.path().join("outside");
        fs::create_dir(&good_host).unwrap();
        fs::create_dir(&bad_host).unwrap();
        fs::create_dir(&outside).unwrap();

        let good = share(good_host.canonicalize().unwrap(), vec![], vec![]);
        let bad = share(
            bad_host.canonicalize().unwrap(),
            vec![outside.canonicalize().unwrap()],
            vec![],
        );

        let shares = vec![good, bad];
        let report = shares.validate().unwrap_err();

        // index 0 (the good share) contributed nothing; index 1's bad
        // subpath shows up nested under its own index, same shape `dive`
        // on Config's `shares` field would produce.
        assert!(
            garde::select!(report, [1].read_only_paths[0])
                .next()
                .is_some()
        );
    }
}
