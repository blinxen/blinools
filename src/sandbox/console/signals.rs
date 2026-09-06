use anyhow::Context;

use crate::sandbox::console::{TERMINATED, WINDOW_RESIZED};
use std::sync::atomic::Ordering;

// Makes sure program does not terminate since default handlers like to do that
pub struct Signals {
    original: libc::sigset_t,
}

const SIGNALS: [libc::c_int; 5] = [
    libc::SIGWINCH,
    libc::SIGTERM,
    libc::SIGHUP,
    libc::SIGINT,
    libc::SIGQUIT,
];

impl Signals {
    pub fn install_handlers_and_mask() -> Result<Self, anyhow::Error> {
        for signal in SIGNALS {
            let mut action: libc::sigaction = unsafe { std::mem::zeroed() };
            action.sa_sigaction = handle_signal as *const () as usize;
            action.sa_flags = 0;
            unsafe {
                libc::sigemptyset(&mut action.sa_mask);
                if libc::sigaction(signal, &action, std::ptr::null_mut()) != 0 {
                    return Err(std::io::Error::last_os_error())
                        .context("installing a signal handler");
                }
            }
        }
        unsafe {
            let mut signal_set: libc::sigset_t = std::mem::zeroed();
            let mut original: libc::sigset_t = std::mem::zeroed();
            libc::sigemptyset(&mut signal_set);
            for signal in SIGNALS {
                libc::sigaddset(&mut signal_set, signal);
            }
            if libc::pthread_sigmask(libc::SIG_BLOCK, &signal_set, &mut original) != 0 {
                return Err(std::io::Error::last_os_error()).context("blocking signals");
            }

            Ok(Signals { original })
        }
    }

    pub fn original(&self) -> libc::sigset_t {
        self.original
    }
}

impl Drop for Signals {
    fn drop(&mut self) {
        unsafe { libc::pthread_sigmask(libc::SIG_SETMASK, &self.original, std::ptr::null_mut()) };
    }
}

extern "C" fn handle_signal(signal: libc::c_int) {
    if signal == libc::SIGWINCH {
        WINDOW_RESIZED.store(true, Ordering::Relaxed);
    } else {
        TERMINATED.store(true, Ordering::Relaxed);
    }
}
