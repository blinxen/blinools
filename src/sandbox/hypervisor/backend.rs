use std::path::Path;

use super::{BootOptions, State, VmConfig};
use crate::sandbox::name::Name;
use crate::sandbox::process::Process;

pub(super) trait Backend {
    fn name(&self) -> &'static str;

    fn configure(&self, config: &VmConfig<'_>, boot: &BootOptions) -> anyhow::Result<Process>;

    fn has_fs_hotplug_support(&self) -> bool;

    fn is_running(&self, sandbox_runtime_dir: &Path) -> bool;

    fn shutdown(&self, sandbox_runtime_dir: &Path, force: bool) -> Result<(), anyhow::Error>;

    fn state(&self, sandbox_runtime_dir: &Path) -> State;

    fn attach_fs(
        &self,
        sandbox_name: &Name,
        sandbox_runtime_dir: &Path,
        tag: &Name,
        socket_path: &Path,
    ) -> Result<(), anyhow::Error>;

    fn detach_fs(
        &self,
        sandbox_name: &Name,
        sandbox_runtime_dir: &Path,
        tag: &Name,
    ) -> Result<(), anyhow::Error>;
}
