use std::io;
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd, RawFd};
use std::sync::{Arc, mpsc};

use serde_json::Value;

#[derive(Debug, PartialEq)]
pub enum VmEvent {
    KernelPanic,
    Paused,
    Resumed,
    Shutdown,
    Other(Value),
}

pub struct EventSender {
    sender: mpsc::Sender<anyhow::Result<VmEvent>>,
    wakeup: Arc<OwnedFd>,
}

pub struct EventReceiver {
    receiver: mpsc::Receiver<anyhow::Result<VmEvent>>,
    wakeup: Arc<OwnedFd>,
}

pub fn channel() -> io::Result<(EventSender, EventReceiver)> {
    let eventfd = unsafe { libc::eventfd(0, libc::EFD_CLOEXEC | libc::EFD_NONBLOCK) };
    if eventfd == -1 {
        return Err(io::Error::last_os_error());
    }
    let wakeup = Arc::new(unsafe { OwnedFd::from_raw_fd(eventfd) });
    let (sender, receiver) = mpsc::channel();
    Ok((
        EventSender {
            sender,
            wakeup: Arc::clone(&wakeup),
        },
        EventReceiver { receiver, wakeup },
    ))
}

impl EventSender {
    pub fn send(&self, event: anyhow::Result<VmEvent>) -> io::Result<()> {
        self.sender
            .send(event)
            .map_err(|_| io::Error::new(io::ErrorKind::BrokenPipe, "VM event receiver closed"))?;
        self.wake()
    }

    fn wake(&self) -> io::Result<()> {
        let value = 1u64;
        loop {
            if unsafe { libc::write(self.wakeup.as_raw_fd(), (&value as *const u64).cast(), 8) }
                >= 0
            {
                return Ok(());
            }
            let error = io::Error::last_os_error();
            match error.kind() {
                io::ErrorKind::Interrupted => continue,
                io::ErrorKind::WouldBlock => return Ok(()),
                _ => return Err(error),
            }
        }
    }
}

impl Drop for EventSender {
    fn drop(&mut self) {
        let _ = self.wake();
    }
}

impl EventReceiver {
    pub fn clear_wakeup(&self) -> io::Result<()> {
        let mut value = 0u64;
        loop {
            if unsafe { libc::read(self.wakeup.as_raw_fd(), (&mut value as *mut u64).cast(), 8) }
                >= 0
            {
                return Ok(());
            }
            let error = io::Error::last_os_error();
            match error.kind() {
                io::ErrorKind::Interrupted => continue,
                io::ErrorKind::WouldBlock => return Ok(()),
                _ => return Err(error),
            }
        }
    }

    pub fn try_recv(&self) -> Result<anyhow::Result<VmEvent>, mpsc::TryRecvError> {
        self.receiver.try_recv()
    }
}

impl AsRawFd for EventReceiver {
    fn as_raw_fd(&self) -> RawFd {
        self.wakeup.as_raw_fd()
    }
}
