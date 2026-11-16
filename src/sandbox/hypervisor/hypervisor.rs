use std::fmt;
use std::os::unix::net::UnixStream;
use std::path::Path;

use anyhow::Context;

use super::backend::Backend;
use super::event_monitor::EventMonitor;
use super::{BootOptions, State, Vm, VmConfig, create_qcow2_overlay, guest_cmdline};
use crate::sandbox::events;
use crate::sandbox::name::Name;

pub struct Hypervisor {
    pub(super) backend: Box<dyn Backend>,
}

impl fmt::Display for Hypervisor {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str(self.backend.name())
    }
}

impl Hypervisor {
    pub fn boot(self, config: VmConfig<'_>) -> anyhow::Result<Vm> {
        let (parent_stream, monitor_stream) =
            UnixStream::pair().context("creating hypervisor event socket")?;
        let boot = BootOptions {
            overlay: create_qcow2_overlay(&config)?,
            cmdline: guest_cmdline(&config),
        };
        let mut process = self.backend.configure(&config, &boot)?;
        process
            .eventfd(monitor_stream.into())
            .stdin(config.console.stdin)
            .stdout(config.console.stdout)
            .stderr(config.console.stderr)
            .spawn()
            .context("spawning hypervisor")?;

        let (sender, events) = events::channel().context("creating VM event channel")?;
        let monitor = EventMonitor::start(parent_stream, sender, self.backend)?;
        Ok(Vm {
            monitor,
            events,
            process,
        })
    }

    pub fn is_running(&self, sandbox_runtime_dir: &Path) -> bool {
        self.backend.is_running(sandbox_runtime_dir)
    }

    pub fn shutdown(&self, sandbox_runtime_dir: &Path, force: bool) -> anyhow::Result<()> {
        self.backend.shutdown(sandbox_runtime_dir, force)
    }

    pub fn state(&self, sandbox_runtime_dir: &Path) -> State {
        self.backend.state(sandbox_runtime_dir)
    }

    pub fn has_fs_hotplug_support(&self) -> bool {
        self.backend.has_fs_hotplug_support()
    }

    pub fn attach_fs(
        &self,
        sandbox_name: &Name,
        sandbox_runtime_dir: &Path,
        tag: &Name,
        socket_path: &Path,
    ) -> anyhow::Result<()> {
        self.backend
            .attach_fs(sandbox_name, sandbox_runtime_dir, tag, socket_path)
    }

    pub fn detach_fs(
        &self,
        sandbox_name: &Name,
        sandbox_runtime_dir: &Path,
        tag: &Name,
    ) -> anyhow::Result<()> {
        self.backend
            .detach_fs(sandbox_name, sandbox_runtime_dir, tag)
    }
}
