use std::os::fd::RawFd;

use crate::sandbox::console::filter::AnsiFilter;

pub struct FilteredQueue {
    buffer: Vec<u8>,
    offset: usize,
    filter: Option<AnsiFilter>,
}

impl FilteredQueue {
    pub fn unfiltered() -> Self {
        Self {
            buffer: Vec::new(),
            offset: 0,
            filter: None,
        }
    }

    pub fn filtered() -> Self {
        Self {
            buffer: Vec::new(),
            offset: 0,
            filter: Some(AnsiFilter::new()),
        }
    }

    pub fn len(&self) -> usize {
        self.buffer.len() - self.offset
    }

    pub fn is_empty(&self) -> bool {
        self.offset >= self.buffer.len()
    }

    pub fn push(&mut self, bytes: &[u8]) {
        self.compact();
        self.buffer.extend_from_slice(bytes);
    }

    pub fn filter_in(&mut self, bytes: &[u8]) {
        self.compact();
        match &mut self.filter {
            Some(f) => f.filter(bytes, &mut self.buffer),
            None => self.buffer.extend_from_slice(bytes),
        }
    }

    pub fn compact(&mut self) {
        if self.offset == 0 {
            return;
        }
        self.buffer.drain(..self.offset);
        self.offset = 0;
    }

    pub fn drain_into(&mut self, fd: RawFd) -> std::io::Result<()> {
        while !self.is_empty() {
            let pending = &self.buffer[self.offset..];
            let written = unsafe { libc::write(fd, pending.as_ptr().cast(), pending.len()) };
            if written < 0 {
                let error = std::io::Error::last_os_error();
                return match error.kind() {
                    std::io::ErrorKind::Interrupted => continue,
                    std::io::ErrorKind::WouldBlock => Ok(()),
                    _ => Err(error),
                };
            }
            self.offset += written as usize;
        }
        self.compact();

        Ok(())
    }
}
