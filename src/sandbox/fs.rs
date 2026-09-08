use std::ffi::{CStr, CString};
use std::os::unix::ffi::OsStrExt;
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::Duration;

use anyhow::Context;

use crate::sandbox::config::{Config, FsShare};
use crate::sandbox::name::Name;
use crate::sandbox::process::{die_with_parent, kill_child_and_cleanup, wait_for_socket};
use crate::sandbox::unique_socket_path;

const ALL_POSSIBLE_UIDS: u32 = u32::MAX;

#[derive(Debug)]
pub struct FsMount {
    pub tag: Name,
    pub socket_path: PathBuf,
    pub read_only: bool,
    handle: Child,
}

impl FsMount {
    pub fn spawn(config: &Config, share: &FsShare) -> Result<Self, anyhow::Error> {
        let socket_path = unique_socket_path(&config.name, &format!("vfsd-{}", share.name));
        let mut binary_path = PathBuf::from("virtiofsd");
        if let Some(cfg) = &config.virtiofsd
            && let Some(binary) = &cfg.binary
        {
            binary_path = binary.to_path_buf();
        }

        let mut cmd = Command::new(binary_path);
        cmd.arg("--socket-path")
            .arg(&socket_path)
            .arg("--shared-dir")
            .arg(&share.host_dir)
            .arg("--sandbox")
            .arg("namespace")
            .arg("--cache")
            .arg("never")
            .arg("--tag")
            .arg(share.name.as_str())
            // Looks like weird mappings but this way we make sure the guest cannot set weird UID /
            // GID that get pushed back to the HOST
            // Also we map 0 to all UIDS because we always create a new user namespace and in that
            // namespace we are uid / gid 0
            // Changing this here breaks creating files
            .arg("--translate-uid")
            .arg(format!("squash-guest:0:0:{ALL_POSSIBLE_UIDS}"))
            .arg("--translate-gid")
            .arg(format!("squash-guest:0:0:{ALL_POSSIBLE_UIDS}"))
            .arg("--translate-uid")
            .arg(format!(
                "squash-host:0:{}:{ALL_POSSIBLE_UIDS}",
                config.sandbox_user_uid
            ))
            .arg("--translate-gid")
            .arg(format!(
                "squash-host:0:{}:{ALL_POSSIBLE_UIDS}",
                config.sandbox_user_gid
            ));
        if share.read_only {
            cmd.arg("--readonly");
        }
        cmd.stdin(Stdio::null());
        cmd.stdout(Stdio::null());
        cmd.stderr(Stdio::null());
        die_with_parent(&mut cmd);

        unsafe {
            let s = share.clone();
            cmd.pre_exec(move || isolate_share(&s));
        }

        let mut child = cmd.spawn().context("spawning virtiofsd")?;
        if let Err(error) = wait_for_socket(&socket_path, &mut child, Duration::from_secs(10)) {
            kill_child_and_cleanup(&mut child, &[&socket_path]);
            return Err(error).with_context(|| format!("sharing `{}`", share.host_dir.display()));
        }

        Ok(FsMount {
            tag: share.name.clone(),
            socket_path,
            handle: child,
            read_only: share.read_only,
        })
    }
}

impl Drop for FsMount {
    fn drop(&mut self) {
        kill_child_and_cleanup(&mut self.handle, &[&self.socket_path]);
    }
}

// Create a user and mount namespace to hide / mark subpaths as read_only
// virtiofsd does pivot_namespace so we don't have to
fn isolate_share(share: &FsShare) -> Result<(), std::io::Error> {
    let (uid, gid) = unsafe { (libc::getuid(), libc::getgid()) };
    unsafe {
        // TODO: Think of a way to also create PID namespace
        if libc::unshare(libc::CLONE_NEWUSER | libc::CLONE_NEWNS | libc::CLONE_NEWNET) != 0 {
            Err(std::io::Error::last_os_error())?;
        }
    };
    std::fs::write("/proc/self/setgroups", "deny")?;
    std::fs::write("/proc/self/uid_map", format!("0 {} 1", uid))?;
    std::fs::write("/proc/self/gid_map", format!("0 {} 1", gid))?;

    // Make sure mounts don't leak outside
    mount(None, c"/", None, libc::MS_REC | libc::MS_PRIVATE, None)?;

    if share.read_only {
        mount_read_only(&share.host_dir, &share.host_dir)?;
    } else {
        for path in &share.read_only_paths {
            mount_read_only(path, path)?;
        }
    }

    for path in &share.hidden_paths {
        if std::fs::symlink_metadata(path)?.is_dir() {
            hide_dir(path)?;
        } else {
            hide_file(path)?;
        }
    }

    Ok(())
}

fn hide_dir(path: &Path) -> std::io::Result<()> {
    let c_path = cpath(path)?;
    let fstype = CString::new("tmpfs").expect("unreachable cstring");
    let data = CString::new("size=0").expect("unreachable cstring");

    mount(
        None,
        &c_path,
        Some(&fstype),
        libc::MS_RDONLY as libc::c_ulong,
        Some(&data),
    )?;

    Ok(())
}

fn hide_file(path: &Path) -> std::io::Result<()> {
    let empty = std::env::temp_dir().join(format!(".empty-{}", std::process::id()));
    std::fs::File::create(&empty)?;

    mount_read_only(&empty, path)?;

    Ok(())
}

fn mount_read_only(src: &Path, dst: &Path) -> std::io::Result<()> {
    let c_src = cpath(src)?;
    let c_dst = cpath(dst)?;

    // Bind mount
    mount(
        Some(&c_src),
        &c_dst,
        None,
        (libc::MS_BIND | libc::MS_REC) as libc::c_ulong,
        None,
    )?;

    // Remount as read-only because the first mount ignore this flag
    mount(
        None,
        &c_dst,
        None,
        (libc::MS_BIND
            | libc::MS_REMOUNT
            | libc::MS_RDONLY
            | libc::MS_NOSUID
            | libc::MS_NODEV
            | libc::MS_NOEXEC) as libc::c_ulong,
        None,
    )?;

    Ok(())
}

fn mount(
    source: Option<&CStr>,
    target: &CStr,
    filesystem: Option<&CStr>,
    flags: libc::c_ulong,
    data: Option<&CStr>,
) -> std::io::Result<()> {
    let pointer = |value: Option<&CStr>| value.map_or(std::ptr::null(), CStr::as_ptr);

    let res = unsafe {
        libc::mount(
            pointer(source),
            target.as_ptr(),
            pointer(filesystem),
            flags,
            pointer(data).cast(),
        )
    };

    if res != 0 {
        return Err(std::io::Error::last_os_error());
    }
    Ok(())
}

fn cpath(p: &Path) -> std::io::Result<CString> {
    CString::new(p.as_os_str().as_bytes())
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidInput, e))
}
