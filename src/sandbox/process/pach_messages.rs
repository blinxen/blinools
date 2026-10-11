use serde::{Deserialize, Serialize};
use std::io::{self, Read, Write};
use std::os::unix::net::UnixStream;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum PachMessage {
    IsolationDone,
    ContinueExec,
    Failed(String),
}

const MAX_MSG: usize = 64 * 1024;
const MAX_STRING_IN_MESSAGE: usize = MAX_MSG - 16;

impl PachMessage {
    pub fn send(&self, s: &mut UnixStream) -> io::Result<()> {
        let bytes = match self {
            PachMessage::Failed(text) if text.len() > MAX_STRING_IN_MESSAGE => {
                let end = text.floor_char_boundary(MAX_STRING_IN_MESSAGE);
                postcard::to_stdvec(&PachMessage::Failed(text[..end].to_owned()))
            }
            other => postcard::to_stdvec(other),
        }
        .map_err(io::Error::other)?;

        let mut msg = Vec::with_capacity(4 + bytes.len());
        // A message always starts with the length
        msg.extend_from_slice(&(bytes.len() as u32).to_le_bytes());
        msg.extend_from_slice(&bytes);
        s.write_all(&msg)
    }

    pub fn recv(s: &mut UnixStream) -> io::Result<Self> {
        let mut len = [0u8; 4];
        s.read_exact(&mut len)?;
        let len = u32::from_le_bytes(len) as usize;
        if len > MAX_MSG {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "message too large",
            ));
        }
        let mut buf = vec![0u8; len];
        s.read_exact(&mut buf)?;
        postcard::from_bytes(&buf).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
    }
}
