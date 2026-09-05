use std::path::PathBuf;
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

        let (host_uid, host_gid) = unsafe { (libc::getuid(), libc::getgid()) };

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
            .arg("--translate-uid")
            .arg(format!("squash-guest:0:{host_uid}:{ALL_POSSIBLE_UIDS}"))
            .arg("--translate-gid")
            .arg(format!("squash-guest:0:{host_gid}:{ALL_POSSIBLE_UIDS}"))
            .arg("--translate-uid")
            .arg(format!("squash-host:0:{}:{ALL_POSSIBLE_UIDS}", config.sandbox_user_uid))
            .arg("--translate-gid")
            .arg(format!("squash-host:0:{}:{ALL_POSSIBLE_UIDS}", config.sandbox_user_gid));
        if share.read_only {
            cmd.arg("--readonly");
        }
        cmd.stdin(Stdio::null());
        cmd.stdout(Stdio::null());
        cmd.stderr(Stdio::null());
        die_with_parent(&mut cmd);

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
