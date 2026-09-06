use std::fs::{File, OpenOptions};
use std::os::fd::AsRawFd;
use std::os::unix::fs::OpenOptionsExt;
use std::path::Path;

use anyhow::Context;

use crate::config::runtime_dir;
use crate::sandbox::name::Name;

// `flock` when its holder dies, so a crashed run cannot leave a name taken forever.
pub struct SandboxLock(File);

impl SandboxLock {
    pub fn try_acquire(name: &Name) -> Result<Option<Self>, anyhow::Error> {
        Self::try_acquire_at(&runtime_dir().join(format!("{name}.lock")))
    }

    // Result says wether a error occured for whatever reason and the option
    // says whether locking was successful or not (lock already held by another process)
    fn try_acquire_at(path: &Path) -> Result<Option<Self>, anyhow::Error> {
        let file = OpenOptions::new()
            .create(true)
            .truncate(false)
            .write(true)
            .mode(0o600)
            .open(path)
            .with_context(|| format!("opening lock file {}", path.display()))?;

        if unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } != 0 {
            let error = std::io::Error::last_os_error();
            if error.kind() == std::io::ErrorKind::WouldBlock {
                return Ok(None);
            }

            return Err(error).with_context(|| format!("locking {}", path.display()));
        }

        Ok(Some(SandboxLock(file)))
    }
}

impl Drop for SandboxLock {
    fn drop(&mut self) {
        unsafe {
            libc::flock(self.0.as_raw_fd(), libc::LOCK_UN);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn a_second_holder_is_turned_away_until_the_first_one_is_gone() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("sandbox.lock");

        let held = SandboxLock::try_acquire_at(&path).unwrap();
        assert!(held.is_some());
        assert!(SandboxLock::try_acquire_at(&path).unwrap().is_none());

        drop(held);
        assert!(SandboxLock::try_acquire_at(&path).unwrap().is_some());
    }
}
