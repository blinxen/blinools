use std::path::PathBuf;
use std::process::{Child, Command, Stdio};

use anyhow::Context;

use crate::sandbox::config::{Config, FsShare};
use crate::sandbox::{create_socket_path, kill_child_and_socket_with_timeout};

#[derive(Debug)]
pub struct FsMount {
    pub tag: String,
    pub socket_path: PathBuf,
    pub read_only: bool,
    handle: Child,
}

impl FsMount {
    pub fn spawn(config: &Config, share: &FsShare) -> Result<Self, anyhow::Error> {
        let socket_path = create_socket_path(&config.name, &format!("vfsd-{}.sock", share.name));
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
        kill_child_and_socket_with_timeout(&mut self.handle, &self.socket_path);
    }
}
