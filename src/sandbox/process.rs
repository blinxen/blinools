use std::os::unix::net::UnixStream;
use std::os::unix::process::CommandExt;
use std::path::Path;
use std::process::Command;
use std::time::{Duration, Instant};
use std::{io, thread};

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
    pid: u32,
    timeout: Duration,
) -> Result<(), anyhow::Error> {
    let deadline = Instant::now() + timeout;

    while Instant::now() < deadline {
        if socket_path.exists() && UnixStream::connect(socket_path).is_ok() {
            return Ok(());
        }
        if !is_process_running(pid) {
            return Err(anyhow::anyhow!(
                "helper process exited before creating {}",
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

pub fn wait_for_pid_file(
    pid_path: &Path,
    pid: u32,
    timeout: Duration,
) -> Result<(), anyhow::Error> {
    let deadline = Instant::now() + timeout;

    while Instant::now() < deadline {
        if pid_path.exists() {
            return Ok(());
        }
        if !is_process_running(pid) {
            return Err(anyhow::anyhow!(
                "helper process exited before creating {}",
                pid_path.display()
            ));
        }
        std::thread::sleep(SOCKET_POLL_INTERVAL);
    }

    Err(anyhow::anyhow!(
        "timed out waiting for {} to appear",
        pid_path.display()
    ))
}

pub fn kill_child_and_cleanup(pid: u32, files_to_remove: &[&Path]) {
    if let Ok(pid) = libc::pid_t::try_from(pid)
        && pid > 0
    {
        let deadline = Instant::now() + Duration::from_secs(5);
        let mut term_sent = false;

        loop {
            let result = unsafe { libc::waitpid(pid, std::ptr::null_mut(), libc::WNOHANG) };

            if result == pid {
                break;
            }
            if result == -1 {
                if io::Error::last_os_error().raw_os_error() == Some(libc::EINTR) {
                    continue;
                }
                break;
            }

            if !term_sent {
                unsafe {
                    libc::kill(pid, libc::SIGTERM);
                }
                term_sent = true;
            }

            if Instant::now() >= deadline {
                unsafe {
                    libc::kill(pid, libc::SIGKILL);
                }

                loop {
                    let result = unsafe { libc::waitpid(pid, std::ptr::null_mut(), 0) };

                    if result != -1
                        || io::Error::last_os_error().raw_os_error() != Some(libc::EINTR)
                    {
                        break;
                    }
                }
                break;
            }

            thread::sleep(Duration::from_millis(50));
        }
    }

    for file in files_to_remove {
        let _ = std::fs::remove_file(file);
    }
}

pub fn unshare_command(command: &mut Command) {
    // TODO: This needs to do a lot more for sandboxing
    unsafe {
        command.pre_exec(unshare);
    }
}

pub fn unshare() -> Result<(), std::io::Error> {
    unsafe {
        let (uid, gid) = (libc::getuid(), libc::getgid());
        if libc::unshare(libc::CLONE_NEWUSER | libc::CLONE_NEWNS | libc::CLONE_NEWNET) != 0 {
            return Err(std::io::Error::last_os_error());
        }
        std::fs::write("/proc/self/setgroups", "deny")?;
        std::fs::write("/proc/self/uid_map", format!("0 {uid} 1"))?;
        std::fs::write("/proc/self/gid_map", format!("0 {gid} 1"))?;
        // Make sure mounts don't leak outside
        let res = libc::mount(
            std::ptr::null(),
            c"/".as_ptr(),
            std::ptr::null(),
            libc::MS_REC | libc::MS_PRIVATE,
            std::ptr::null(),
        );
        if res != 0 {
            return Err(std::io::Error::last_os_error());
        }
    }
    Ok(())
}

fn is_process_running(pid: u32) -> bool {
    let Ok(pid) = libc::pid_t::try_from(pid) else {
        return false;
    };
    if pid <= 0 {
        return false;
    }

    unsafe { libc::kill(pid, 0) == 0 }
}
