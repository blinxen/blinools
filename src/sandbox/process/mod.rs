mod pach_messages;

use std::convert::Infallible;
use std::ffi::{CString, OsStr, OsString};
use std::fs::File;
use std::os::fd::{AsRawFd, OwnedFd};
use std::os::unix::ffi::OsStrExt;
use std::os::unix::net::UnixStream;
use std::os::unix::process::ExitStatusExt;
use std::panic::{self, AssertUnwindSafe};
use std::path::{Path, PathBuf};
use std::process::ExitStatus;
use std::time::{Duration, Instant};
use std::{fmt, io};

use anyhow::Context;

use crate::sandbox::cgroup::CGroup;
use crate::sandbox::config::PROCESS_TERMINATE_TIMEOUT;
use crate::sandbox::process::pach_messages::PachMessage;

const SOCKET_READ_WRITE_TIMEOUT: Duration = Duration::from_secs(5);
const POLL_INTERVAL: Duration = Duration::from_millis(10);
const TERMINATE_POLL_INTERVAL: Duration = Duration::from_millis(50);

#[cfg(not(test))]
const WAIT_TIMEOUT: Duration = Duration::from_secs(30);
#[cfg(test)]
const WAIT_TIMEOUT: Duration = Duration::from_millis(200);

// https://github.com/torvalds/linux/blob/master/include/uapi/linux/sched.h#L38
const CLONE_INTO_CGROUP: u64 = 1 << 33;

type Hook = Box<dyn FnOnce() -> Result<(), anyhow::Error>>;

pub struct Process {
    binary: PathBuf,
    args: Vec<OsString>,
    cgroup: Option<PathBuf>,
    stdin: Option<OwnedFd>,
    stdout: Option<OwnedFd>,
    stderr: Option<OwnedFd>,
    // when set to false then we dont create any namespaces
    namespaces: bool,
    // Single hook for now, maybe this can become an array later
    pre_exec: Option<Hook>,
    // File to delete after the process is dropped
    files_to_remove: Vec<PathBuf>,
    pid: Option<libc::pid_t>,
    status: Option<ExitStatus>,
    // Parent-Child channel used to implement the separation between isolation and exec methods
    // This is mainly used for pasta where we need to create the namespace then let pasta start
    // and only then run passt.
    // This channel is not the same parent and child
    pach: Option<UnixStream>,
    // Needed for cleanup since we need to differentiate between parent and child
    // pid can't be used here because in the case of a failed exec, we don't had pid set
    is_child: bool,
}

impl Process {
    pub fn new(binary: impl AsRef<Path>, cgroup: Option<&CGroup>) -> Self {
        Process {
            binary: binary.as_ref().to_path_buf(),
            args: Vec::new(),
            cgroup: cgroup.map(|cgroup| cgroup.path().to_path_buf()),
            stdin: None,
            stdout: None,
            stderr: None,
            namespaces: true,
            pre_exec: None,
            files_to_remove: Vec::new(),
            pid: None,
            status: None,
            pach: None,
            is_child: false,
        }
    }

    pub fn arg(&mut self, arg: impl AsRef<OsStr>) -> &mut Self {
        self.args.push(arg.as_ref().to_os_string());
        self
    }

    pub fn args<I, S>(&mut self, args: I) -> &mut Self
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        for arg in args {
            self.arg(arg);
        }
        self
    }

    pub fn stdin(&mut self, fd: OwnedFd) -> &mut Self {
        self.stdin = Some(fd);
        self
    }

    pub fn stdout(&mut self, fd: OwnedFd) -> &mut Self {
        self.stdout = Some(fd);
        self
    }

    pub fn stderr(&mut self, fd: OwnedFd) -> &mut Self {
        self.stderr = Some(fd);
        self
    }

    /// Don't craate any namespace, should only be used by pasta
    pub fn no_namespaces(&mut self) -> &mut Self {
        self.namespaces = false;
        self
    }

    pub fn pre_exec(
        &mut self,
        hook: impl FnOnce() -> Result<(), anyhow::Error> + 'static,
    ) -> &mut Self {
        self.pre_exec = Some(Box::new(hook));
        self
    }

    pub fn remove_on_drop(&mut self, path: impl AsRef<Path>) -> &mut Self {
        self.files_to_remove.push(path.as_ref().to_path_buf());
        self
    }

    pub fn pid(&self) -> Option<u32> {
        self.pid.map(|pid| pid as u32)
    }

    pub fn spawn(&mut self) -> Result<(), anyhow::Error> {
        self.isolate()?;
        self.exec()
    }

    /// Fork and create namespaces but don't exec yet
    pub fn isolate(&mut self) -> Result<(), anyhow::Error> {
        if self.pid.is_some() {
            anyhow::bail!("{} was already started", self.binary.display());
        }

        let cgroup = self
            .cgroup
            .as_ref()
            .map(File::open)
            .transpose()
            .context("opening cgroup")?;

        let (uid, gid) = unsafe { (libc::getuid(), libc::getgid()) };

        let mut clone_args: libc::clone_args = unsafe { std::mem::zeroed() };
        clone_args.exit_signal = libc::SIGCHLD as u64;
        if self.namespaces {
            clone_args.flags |= (libc::CLONE_NEWUSER
                | libc::CLONE_NEWNS
                | libc::CLONE_NEWNET
                | libc::CLONE_NEWPID
                | libc::CLONE_NEWIPC
                | libc::CLONE_NEWUTS
                | libc::CLONE_NEWTIME) as u64;
        }
        if let Some(cgroup) = &cgroup {
            clone_args.flags |= CLONE_INTO_CGROUP;
            clone_args.cgroup = cgroup.as_raw_fd() as u64;
        }

        let (mut parent_sock, child_sock) =
            UnixStream::pair().context("create parent-child channel")?;
        parent_sock.set_read_timeout(Some(SOCKET_READ_WRITE_TIMEOUT))?;
        parent_sock.set_write_timeout(Some(SOCKET_READ_WRITE_TIMEOUT))?;
        child_sock.set_read_timeout(Some(SOCKET_READ_WRITE_TIMEOUT))?;
        child_sock.set_write_timeout(Some(SOCKET_READ_WRITE_TIMEOUT))?;
        log::debug!(
            "Starting command: {} {:?}",
            self.binary.display(),
            self.args
        );
        let pid = unsafe {
            libc::syscall(
                libc::SYS_clone3,
                &mut clone_args as *mut libc::clone_args,
                size_of::<libc::clone_args>(),
            )
        };

        // sockets must be dropped here because otherwise each process holds the other ends socket
        // which means EOF is not possible anymore if one exits.
        // The EOF is important in the case of stopping a blocking read call which we use to wait
        // for a signal.
        match pid {
            -1 => Err(io::Error::last_os_error())
                .with_context(|| format!("starting {}", self.binary.display())),
            0 => {
                self.is_child = true;
                drop(parent_sock);
                self.pach = Some(child_sock);
                let message = match panic::catch_unwind(AssertUnwindSafe(|| self.child(uid, gid))) {
                    Ok(Err(error)) => format!("{error:#}"),
                    Err(_) => format!("{} panicked before exec", self.binary.display()),
                };
                if let Some(pach) = self.pach.as_mut() {
                    PachMessage::Failed(message).send(pach)?;
                }
                unsafe { libc::_exit(127) }
            }
            pid => {
                drop(child_sock);
                // Drop the referenced to the fds since the parent does not need them
                self.stdin.take();
                self.stdout.take();
                self.stderr.take();
                self.pid = Some(pid as libc::pid_t);

                let result = match PachMessage::recv(&mut parent_sock) {
                    Ok(PachMessage::IsolationDone) => Ok(()),
                    Ok(PachMessage::Failed(message)) => Err(self.child_failure(&message)),
                    Err(error) if error.kind() != io::ErrorKind::UnexpectedEof => Err(self
                        .child_failure(&format!(
                            "{} did not report back after isolating: {error}",
                            self.binary.display()
                        ))),
                    _ => Err(self.child_failure("")),
                };

                self.pach = Some(parent_sock);

                result
            }
        }
    }

    pub fn exec(&mut self) -> Result<(), anyhow::Error> {
        let Some(mut pach) = self.pach.take() else {
            anyhow::bail!("{} is not isolated yet", self.binary.display());
        };

        PachMessage::ContinueExec.send(&mut pach)?;
        match PachMessage::recv(&mut pach) {
            Ok(PachMessage::Failed(message)) => Err(self.child_failure(&message)),
            _ => Ok(()),
        }
    }

    pub fn wait_for_file(&mut self, path: &Path, timeout: Duration) -> Result<(), anyhow::Error> {
        self.wait_until(path, timeout, Path::exists)
    }

    pub fn wait_for_socket(&mut self, path: &Path, timeout: Duration) -> Result<(), anyhow::Error> {
        self.wait_until(path, timeout, |path| UnixStream::connect(path).is_ok())
    }

    fn wait_until(
        &mut self,
        path: &Path,
        timeout: Duration,
        ready: impl Fn(&Path) -> bool,
    ) -> Result<(), anyhow::Error> {
        let deadline = Instant::now() + timeout;

        while Instant::now() < deadline {
            if ready(path) {
                return Ok(());
            }
            if let Some(status) = self
                .try_wait()
                .with_context(|| format!("checking on {}", self.binary.display()))?
            {
                anyhow::bail!(
                    "{} exited with {status} before creating {}",
                    self.binary.display(),
                    path.display()
                );
            }
            std::thread::sleep(POLL_INTERVAL);
        }

        Err(anyhow::anyhow!(
            "timed out waiting for {} to appear",
            path.display()
        ))
    }

    pub fn wait(&mut self) -> io::Result<ExitStatus> {
        let deadline = Instant::now() + WAIT_TIMEOUT;
        loop {
            if let Some(status) = self.try_wait()? {
                return Ok(status);
            }
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return Err(io::Error::new(
                    io::ErrorKind::TimedOut,
                    "timed out waiting for process",
                ));
            }
            std::thread::sleep(POLL_INTERVAL);
        }
    }

    fn try_wait(&mut self) -> io::Result<Option<ExitStatus>> {
        if let Some(status) = self.status {
            return Ok(Some(status));
        }
        let Some(pid) = self.pid else {
            return Err(io::Error::other("process was not started"));
        };

        let mut raw = 0;
        loop {
            match unsafe { libc::waitpid(pid, &mut raw, libc::WNOHANG) } {
                0 => return Ok(None), // still running
                -1 => {
                    let error = io::Error::last_os_error();
                    if error.kind() != io::ErrorKind::Interrupted {
                        return Err(error);
                    }
                }
                _ => {
                    let status = ExitStatus::from_raw(raw);
                    self.status = Some(status);
                    return Ok(Some(status));
                }
            }
        }
    }

    fn child_failure(&mut self, message: &str) -> anyhow::Error {
        self.pach.take();

        if !message.is_empty() {
            return anyhow::anyhow!("{}", message);
        }

        match self.wait() {
            Ok(status) => {
                anyhow::anyhow!("{} exited with {status} before exec", self.binary.display())
            }
            Err(error) => anyhow::anyhow!(error)
                .context(format!("{} exited before exec", self.binary.display())),
        }
    }

    fn child(&mut self, uid: libc::uid_t, gid: libc::gid_t) -> Result<Infallible, anyhow::Error> {
        unsafe {
            let mut signals: libc::sigset_t = std::mem::zeroed();
            libc::sigemptyset(&mut signals);
            libc::sigprocmask(libc::SIG_SETMASK, &signals, std::ptr::null_mut());
            // Broken pipes should crash the child
            libc::signal(libc::SIGPIPE, libc::SIG_DFL);
            // When the parent dies (in this case it is blinools) then we should make sure
            // that the child is terminated as well.
            // TODO: Is it OK to use SIGKILL?
            if libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGKILL) != 0 {
                return Err(io::Error::last_os_error()).context("setting parent death signal");
            }
        }

        if self.namespaces {
            std::fs::write("/proc/self/setgroups", "deny").context("writing setgroups")?;
            std::fs::write("/proc/self/uid_map", format!("0 {uid} 1"))
                .context("writing uid_map")?;
            std::fs::write("/proc/self/gid_map", format!("0 {gid} 1"))
                .context("writing gid_map")?;
            // Make sure mounts don't leak outside
            mount(None, c"/", None, libc::MS_REC | libc::MS_PRIVATE)
                .context("making mounts private")?;
            mount(
                Some(c"proc"),
                c"/proc",
                Some(c"proc"),
                libc::MS_NOSUID | libc::MS_NODEV | libc::MS_NOEXEC,
            )
            .context("mounting /proc")?;
            // Hide cgroup root
            if unsafe { libc::unshare(libc::CLONE_NEWCGROUP) } != 0 {
                return Err(io::Error::last_os_error()).context("unsharing cgroup namespace");
            }
        }

        // Some issue happend with the notifications, better to stop everything
        let mut early_exit = true;
        if let Some(pach) = self.pach.as_mut() {
            PachMessage::IsolationDone.send(pach)?;
            if let Ok(PachMessage::ContinueExec) = PachMessage::recv(pach) {
                early_exit = false;
            }
        }

        if early_exit {
            unsafe { libc::_exit(1) };
        }

        if let Some(hook) = self.pre_exec.take() {
            hook().context("pre_exec")?;
        }

        let dev_null = File::options()
            .read(true)
            .write(true)
            .open("/dev/null")
            .context("opening /dev/null")?;
        let stdin = self
            .stdin
            .as_ref()
            .map_or(dev_null.as_raw_fd(), AsRawFd::as_raw_fd);
        let stdout = self
            .stdout
            .as_ref()
            .map_or(dev_null.as_raw_fd(), AsRawFd::as_raw_fd);
        let stderr = self
            .stderr
            .as_ref()
            .map_or(dev_null.as_raw_fd(), AsRawFd::as_raw_fd);
        for (fd, target) in [(stdin, 0), (stdout, 1), (stderr, 2)] {
            dup2(fd, target).with_context(|| format!("redirecting fd {target}"))?;
        }

        let argv: Vec<CString> = std::iter::once(self.binary.as_os_str())
            .chain(self.args.iter().map(OsString::as_os_str))
            .map(|s| CString::new(s.as_bytes()).context("argument contains a null byte"))
            .collect::<Result<_, _>>()?;
        let mut argv_ptrs: Vec<*const libc::c_char> = argv.iter().map(|s| s.as_ptr()).collect();
        // We need a null terminated array for execvp
        argv_ptrs.push(std::ptr::null());
        unsafe { libc::execvp(argv_ptrs[0], argv_ptrs.as_ptr()) };
        Err(io::Error::last_os_error()).with_context(|| format!("exec {}", self.binary.display()))
    }
}

impl fmt::Debug for Process {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_struct("Process")
            .field("binary", &self.binary)
            .field("args", &self.args)
            .field("pid", &self.pid)
            .finish_non_exhaustive()
    }
}

impl Drop for Process {
    fn drop(&mut self) {
        if self.is_child {
            return;
        }

        if let Some(pid) = self.pid
            && matches!(self.try_wait(), Ok(None))
        {
            unsafe { libc::kill(pid, libc::SIGTERM) };
            let deadline = Instant::now() + PROCESS_TERMINATE_TIMEOUT;
            while matches!(self.try_wait(), Ok(None)) && Instant::now() < deadline {
                std::thread::sleep(TERMINATE_POLL_INTERVAL);
            }

            if matches!(self.try_wait(), Ok(None)) {
                unsafe { libc::kill(pid, libc::SIGKILL) };
                if let Err(e) = self.wait() {
                    log::warn!("failed to reap pid {pid} after SIGKILL: {e}");
                }
            }
        }

        for file in &self.files_to_remove {
            let _ = std::fs::remove_file(file);
        }
    }
}

pub fn remove_stale_socket(socket_path: &Path) {
    let _ = std::fs::remove_file(socket_path);
}

fn dup2(fd: libc::c_int, target: libc::c_int) -> io::Result<()> {
    let result = if fd == target {
        // clear closexec since we need the fds when exec'n
        unsafe { libc::fcntl(fd, libc::F_SETFD, 0) }
    } else {
        unsafe { libc::dup2(fd, target) }
    };
    if result == -1 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

fn mount(
    source: Option<&std::ffi::CStr>,
    target: &std::ffi::CStr,
    filesystem: Option<&std::ffi::CStr>,
    flags: libc::c_ulong,
) -> io::Result<()> {
    let pointer = |value: Option<&std::ffi::CStr>| value.map_or(std::ptr::null(), |v| v.as_ptr());
    let result = unsafe {
        libc::mount(
            pointer(source),
            target.as_ptr(),
            pointer(filesystem),
            flags,
            std::ptr::null(),
        )
    };
    if result != 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn sh(script: &str) -> Process {
        let mut process = Process::new("sh", None);
        process.arg("-c").arg(script);
        process
    }

    #[test]
    fn the_exit_status_is_reported() {
        let mut process = sh("exit 3");
        process.spawn().unwrap();

        assert_eq!(process.wait().unwrap().code(), Some(3));
    }

    #[test]
    fn a_missing_binary_fails_spawn() {
        let error = Process::new("blinools-does-not-exist", None)
            .spawn()
            .expect_err("exec has to fail");

        assert!(format!("{error:#}").contains("exec blinools-does-not-exist"));
    }

    #[test]
    fn a_failing_pre_exec_hook_fails_spawn() {
        let error = sh("true")
            .pre_exec(|| Err(anyhow::anyhow!("hook broke")))
            .spawn()
            .expect_err("the hook has to fail");

        assert!(format!("{error:#}").contains("hook broke"));
    }

    #[test]
    fn the_process_is_pid_one_in_its_namespace() {
        let mut process = sh(&format!(
            "test $$ -eq 1 && test ! -e /proc/{}",
            std::process::id()
        ));
        process.spawn().unwrap();

        assert!(process.wait().unwrap().success());
    }

    #[test]
    fn no_namespaces_keeps_the_host_pid_namespace() {
        let mut process = sh("test $$ -ne 1");
        process.no_namespaces().spawn().unwrap();

        assert!(process.wait().unwrap().success());
    }

    #[test]
    fn stdout_is_redirected() {
        let dir = tempdir().unwrap();
        let output = dir.path().join("out");
        let mut process = sh("echo hello");
        process.stdout(File::create(&output).unwrap().into());
        process.spawn().unwrap();
        process.wait().unwrap();

        assert_eq!(std::fs::read_to_string(output).unwrap(), "hello\n");
    }

    #[test]
    fn an_isolated_process_waits_for_exec() {
        let dir = tempdir().unwrap();
        let marker = dir.path().join("marker");
        let mut process = sh(&format!("touch {}", marker.display()));
        process.isolate().unwrap();
        std::thread::sleep(Duration::from_millis(100));
        assert!(!marker.exists());

        process.exec().unwrap();
        process
            .wait_for_file(&marker, Duration::from_secs(2))
            .unwrap();
    }

    #[test]
    fn waiting_fails_when_the_process_exits_early() {
        let dir = tempdir().unwrap();
        let mut process = sh("exit 4");
        process.spawn().unwrap();

        let error = process
            .wait_for_file(&dir.path().join("never"), Duration::from_secs(5))
            .expect_err("the process exited");
        assert!(error.to_string().contains("exit status: 4"), "{error}");
    }

    #[test]
    fn waiting_should_timeout() {
        let mut process = sh("sleep 30");
        process.no_namespaces().spawn().unwrap();

        let err = process.wait().unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::TimedOut);
        // Timing out must not have reaped or killed it.
        assert!(matches!(process.try_wait(), Ok(None)));
    }

    #[test]
    fn drop_kills_the_process_and_removes_files() {
        let dir = tempdir().unwrap();
        let should_get_deleted = dir.path().join("socket");
        std::fs::write(&should_get_deleted, "").unwrap();
        let mut process = sh("sleep 30");
        process.no_namespaces().remove_on_drop(&should_get_deleted);
        process.spawn().unwrap();
        let pid = process.pid().unwrap() as libc::pid_t;

        let drop_started = Instant::now();
        drop(process);

        assert!(drop_started.elapsed() < PROCESS_TERMINATE_TIMEOUT);
        assert!(!should_get_deleted.exists());
        assert_ne!(unsafe { libc::kill(pid, 0) }, 0);
    }
}
