use std::io;
use std::os::unix::process::CommandExt;
use std::path::Path;
use std::process::{Child, Command};
use std::time::{Duration, Instant};

use anyhow::Context;
use wait_timeout::ChildExt;

const SOCKET_POLL_INTERVAL: Duration = Duration::from_millis(10);

pub fn die_with_parent(command: &mut Command) {
    let parent = std::process::id() as libc::pid_t;

    unsafe {
        command.pre_exec(move || {
            if libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGTERM) != 0 {
                return Err(io::Error::last_os_error());
            }
            if libc::getppid() != parent {
                libc::_exit(1);
            }

            Ok(())
        });
    }
}

pub fn remove_stale_socket(socket_path: &Path) {
    let _ = std::fs::remove_file(socket_path);
}

pub fn wait_for_socket(
    socket_path: &Path,
    child: &mut Child,
    timeout: Duration,
) -> Result<(), anyhow::Error> {
    let deadline = Instant::now() + timeout;

    while Instant::now() < deadline {
        if socket_path.exists() {
            return Ok(());
        }
        if let Some(status) = child.try_wait().context("checking on a helper process")? {
            return Err(anyhow::anyhow!(
                "helper process exited with {status} before creating {}",
                socket_path.display()
            ));
        }
        std::thread::sleep(SOCKET_POLL_INTERVAL);
    }

    Err(anyhow::anyhow!(
        "timed out waiting for {} to appear",
        socket_path.display()
    ))
}

pub fn kill_child_and_cleanup(child: &mut Child, files_to_remove: &[&Path]) {
    if !matches!(child.try_wait(), Ok(Some(_))) {
        unsafe {
            let _ = libc::kill(child.id() as i32, libc::SIGTERM);
        }
        match child.wait_timeout(Duration::from_secs(3)) {
            Ok(Some(_)) => {}
            _ => {
                let _ = child.kill();
                let _ = child.wait();
            }
        }
    }

    for file in files_to_remove {
        let _ = std::fs::remove_file(file);
    }
}
