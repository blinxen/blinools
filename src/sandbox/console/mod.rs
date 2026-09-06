mod filter;
mod queue;
mod signals;

use std::io::{self, IsTerminal};
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd, RawFd};
use std::process::Stdio;
use std::sync::atomic::{AtomicBool, Ordering};

use anyhow::Context;

use crate::sandbox::console::queue::FilteredQueue;
use crate::sandbox::console::signals::Signals;

static WINDOW_RESIZED: AtomicBool = AtomicBool::new(false);
static TERMINATED: AtomicBool = AtomicBool::new(false);

/// Stop reading from a source once this much output for the other side is still queued
const CONSOLE_BUFFER_LIMIT: usize = 256 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConsoleExit {
    GuestGone,
    Terminated,
}

pub struct ConsoleIo {
    pub stdin: Stdio,
    pub stdout: Stdio,
    pub stderr: Stdio,
}

// The idea here is that we attach to the sandbox pty and filter problematic
// escape characters
// TODO: Store termio settings and reapply after exit
pub struct Console {
    master: OwnedFd,
    slave: Option<OwnedFd>,
    raw_mode: Option<RawMode>,
    non_blocking: Vec<NonBlockingFd>,
    signals_mask: Option<Signals>,
}

impl Console {
    pub fn new() -> Result<Self, anyhow::Error> {
        let mut master: libc::c_int = -1;
        let mut slave: libc::c_int = -1;
        let result = unsafe {
            libc::openpty(
                &mut master,
                &mut slave,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
            )
        };
        if result != 0 {
            return Err(io::Error::last_os_error())
                .context("allocating a pty for the guest console");
        }

        let console = Console {
            master: unsafe { OwnedFd::from_raw_fd(master) },
            slave: Some(unsafe { OwnedFd::from_raw_fd(slave) }),
            raw_mode: None,
            non_blocking: Vec::new(),
            signals_mask: None,
        };

        make_raw(console.slave.as_ref().map(|s| s.as_raw_fd()))?;
        console.refresh_window_size();

        Ok(console)
    }

    fn refresh_window_size(&self) {
        unsafe {
            let mut size: libc::winsize = std::mem::zeroed();
            if libc::ioctl(std::io::stdin().as_raw_fd(), libc::TIOCGWINSZ, &mut size) != 0 {
                return;
            }
            libc::ioctl(self.master.as_raw_fd(), libc::TIOCSWINSZ, &size);
        }
    }

    pub fn take_slave(&mut self) -> Result<ConsoleIo, anyhow::Error> {
        let slave = self
            .slave
            .take()
            .context("guest console was already handed out")?;

        Ok(ConsoleIo {
            stdin: Stdio::from(slave.try_clone()?),
            stdout: Stdio::from(slave.try_clone()?),
            stderr: Stdio::from(slave),
        })
    }

    pub fn read_until_terminated(&mut self) -> Result<ConsoleExit, anyhow::Error> {
        // Make sure signals are handled properly
        self.signals_mask = Some(Signals::install_handlers_and_mask()?);
        // Put stdin in raw mode so we can let the guest do whatever he wants
        // Not quite, we have some filtering
        if std::io::stdin().is_terminal() {
            self.raw_mode = Some(RawMode::enable(std::io::stdin().as_raw_fd())?);
        }
        // Until we are done, neither master nor stdout should block
        self.non_blocking
            .push(NonBlockingFd::new(self.master.as_raw_fd())?);
        self.non_blocking
            .push(NonBlockingFd::new(std::io::stdout().as_raw_fd())?);

        let original_mask = self
            .signals_mask
            .as_ref()
            .context("signal mask was not installed")?
            .original();

        // Buffer used for reading
        let mut buffer = [0u8; 8192];

        // Bytes read from one side that the other side has not accepted yet
        let mut to_master = FilteredQueue::filtered();
        let mut to_guest = FilteredQueue::unfiltered();
        let mut stdin_open = true;
        let mut guest_gone = false;

        // The loop reads / polls through stdin, stdout and master and then:
        // * forwards stdin to guest
        // * forwards stdout to master
        loop {
            if TERMINATED.load(Ordering::Relaxed) {
                return Ok(ConsoleExit::Terminated);
            }
            if WINDOW_RESIZED.swap(false, Ordering::Relaxed) {
                self.refresh_window_size();
            }

            // Everything the guest wrote has reached the terminal nothing left to forward
            if guest_gone && to_master.is_empty() {
                return Ok(ConsoleExit::GuestGone);
            }

            let stdin_events = if stdin_open && to_guest.len() < CONSOLE_BUFFER_LIMIT {
                // we can queue more data for guest
                libc::POLLIN
            } else {
                0
            };

            let stdout_events = if to_master.is_empty() {
                0
            } else {
                // we can read more data for master
                libc::POLLOUT
            };

            let mut master_events = 0;
            if !guest_gone && to_master.len() < CONSOLE_BUFFER_LIMIT {
                master_events |= libc::POLLIN;
            }
            if !to_guest.is_empty() {
                master_events |= libc::POLLOUT;
            }

            let mut poll_fds = [
                pollfd(std::io::stdin().as_raw_fd(), stdin_events),
                pollfd(self.master.as_raw_fd(), master_events),
                pollfd(std::io::stdout().as_raw_fd(), stdout_events),
            ];

            let ready = unsafe {
                libc::ppoll(
                    poll_fds.as_mut_ptr(),
                    poll_fds.len() as libc::nfds_t,
                    // TODO: Do we need a timeout? Probably not
                    std::ptr::null(),
                    &original_mask,
                )
            };
            if ready < 0 {
                let error = io::Error::last_os_error();
                if error.kind() == io::ErrorKind::Interrupted {
                    continue;
                }
                return Err(error).context("waiting for guest console activity");
            }

            // Read whatever the user typed and append it to the to_guest outgoing buffer
            if poll_fds[0].revents != 0 {
                match read(std::io::stdin().as_raw_fd(), &mut buffer) {
                    Ok(0) | Err(_) => stdin_open = false,
                    Ok(count) => to_guest.push(&buffer[..count]),
                }
            }

            // Master has room to write into, drain guest as much as possible into master
            if poll_fds[1].revents & libc::POLLOUT != 0 {
                to_guest
                    .drain_into(self.master.as_raw_fd())
                    .context("forwarding input to the guest console")?;
            }

            // Master has no room to write into, so read into master instead and at the same time
            // check if guest is still there
            if poll_fds[1].revents & (libc::POLLIN | libc::POLLHUP | libc::POLLERR) != 0 {
                match read(self.master.as_raw_fd(), &mut buffer) {
                    // nothing was read -> guest is gone
                    Ok(0) | Err(_) => guest_gone = true,
                    // something was read, send to master but filter the bytes first
                    Ok(count) => to_master.filter_in(&buffer[..count]),
                }
            }

            // Flush buffered guest output out to the terminal
            if poll_fds[2].revents != 0 {
                to_master
                    .drain_into(std::io::stdout().as_raw_fd())
                    .context("writing guest console output")?;
            }
        }
    }
}

impl Drop for Console {
    fn drop(&mut self) {
        // Blocking writes again, so the reset below cannot be lost to a full buffer
        self.non_blocking.clear();
        self.signals_mask.take();

        self.raw_mode.take();
    }
}

/// Puts a terminal into raw mode and restores the previous settings when dropped.
struct RawMode {
    fd: RawFd,
    original: libc::termios,
}

impl RawMode {
    fn enable(fd: RawFd) -> Result<Self, anyhow::Error> {
        let mut original: libc::termios = unsafe { std::mem::zeroed() };
        if unsafe { libc::tcgetattr(fd, &mut original) } != 0 {
            return Err(io::Error::last_os_error()).context("reading terminal settings");
        }

        make_raw(Some(fd))?;

        Ok(RawMode { fd, original })
    }
}

impl Drop for RawMode {
    fn drop(&mut self) {
        unsafe { libc::tcsetattr(self.fd, libc::TCSANOW, &self.original) };
    }
}

struct NonBlockingFd {
    fd: RawFd,
    original: libc::c_int,
}

impl NonBlockingFd {
    fn new(fd: RawFd) -> Result<Self, anyhow::Error> {
        let original = unsafe { libc::fcntl(fd, libc::F_GETFL) };
        if original < 0 {
            return Err(io::Error::last_os_error()).context("reading file descriptor flags");
        }
        if unsafe { libc::fcntl(fd, libc::F_SETFL, original | libc::O_NONBLOCK) } < 0 {
            return Err(io::Error::last_os_error())
                .context("switching a file descriptor to non-blocking mode");
        }

        Ok(NonBlockingFd { fd, original })
    }
}

impl Drop for NonBlockingFd {
    fn drop(&mut self) {
        unsafe { libc::fcntl(self.fd, libc::F_SETFL, self.original) };
    }
}

fn pollfd(fd: RawFd, events: libc::c_short) -> libc::pollfd {
    libc::pollfd {
        // skip a negative descriptor
        // direction is switched off
        fd: if events == 0 { -1 } else { fd },
        events,
        revents: 0,
    }
}

fn make_raw(fd: Option<RawFd>) -> Result<(), anyhow::Error> {
    if let Some(fd) = fd {
        let mut settings: libc::termios = unsafe { std::mem::zeroed() };
        unsafe {
            if libc::tcgetattr(fd, &mut settings) != 0 {
                return Err(io::Error::last_os_error()).context("reading terminal settings");
            }
            libc::cfmakeraw(&mut settings);
            if libc::tcsetattr(fd, libc::TCSANOW, &settings) != 0 {
                return Err(io::Error::last_os_error())
                    .context("switching the terminal to raw mode");
            }
        }
    }

    Ok(())
}

fn read(fd: RawFd, buffer: &mut [u8]) -> io::Result<usize> {
    let count = unsafe { libc::read(fd, buffer.as_mut_ptr().cast(), buffer.len()) };
    if count < 0 {
        return Err(io::Error::last_os_error());
    }

    Ok(count as usize)
}
