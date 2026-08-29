use std::path::PathBuf;
use std::process::{Child, Command, Stdio};

use anyhow::Context;
use wait_timeout::ChildExt;

use crate::config::runtime_dir;
use crate::sandbox::config::{FsShare, VirtiofsdConfig};

#[derive(Debug)]
pub struct FsMount {
    pub tag: String,
    pub socket_path: PathBuf,
    pub read_only: bool,
    handle: Child,
}

impl FsMount {
    pub fn spawn(cfg: Option<&VirtiofsdConfig>, share: &FsShare) -> Result<Self, anyhow::Error> {
        let socket_path =
            runtime_dir().join(format!("vfsd-{}-{}.sock", share.name, std::process::id()));
        let _ = std::fs::remove_file(&socket_path);

        let mut binary_path = PathBuf::from("virtiofsd");
        if let Some(cfg) = cfg
            && let Some(binary) = &cfg.binary
        {
            binary_path = binary.to_path_buf();
        }
        let mut cmd = Command::new(binary_path);
        cmd.arg("--socket-path")
            .arg(&socket_path)
            .arg("--shared-dir")
            .arg(share.host_dir.clone())
            .arg("--sandbox")
            .arg("namespace")
            .arg("--cache")
            .arg("never")
            .arg("--tag")
            .arg(&share.name);
        if share.read_only {
            cmd.arg("--readonly");
        }
        cmd.stdin(Stdio::null());
        cmd.stdout(Stdio::null());
        cmd.stderr(Stdio::null());

        let child = cmd.spawn().context("spawning virtiofsd")?;
        // TODO: Maybe wait some time for socket to be created here
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
        unsafe {
            let _ = libc::kill(self.handle.id() as i32, libc::SIGTERM);
        }
        match self.handle.wait_timeout(std::time::Duration::from_secs(3)) {
            Ok(Some(_)) => {}
            _ => {
                let _ = self.handle.kill();
                let _ = self.handle.wait();
            }
        }
        // This makes sure that the socket file is properly removed (happens when virtiofsd did not
        // gracefully shutdown)
        let _ = std::fs::remove_file(&self.socket_path);
    }
}
