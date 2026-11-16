use std::io::BufReader;
use std::net::Shutdown;
use std::os::unix::net::UnixStream;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::JoinHandle;
use std::time::Duration;

use anyhow::Context;
use serde_json::Value;

use super::backend::Backend;
use crate::sandbox::events::EventSender;

pub(super) struct EventMonitor {
    reader: UnixStream,
    stopping: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl EventMonitor {
    pub(super) fn start(
        parent_stream: UnixStream,
        sender: EventSender,
        mut backend: Box<dyn Backend>,
    ) -> anyhow::Result<Self> {
        // We have our wakeup mechanism, no need for timeout here.
        parent_stream.set_read_timeout(None)?;
        parent_stream.set_write_timeout(Some(Duration::from_secs(5)))?;
        let stopping = Arc::new(AtomicBool::new(false));
        let mut stream_t = parent_stream.try_clone()?;
        let input_t = BufReader::new(parent_stream.try_clone()?);
        let stopping_t = Arc::clone(&stopping);
        let thread = std::thread::Builder::new()
            .name("hypervisor-events".into())
            .spawn(move || {
                let main_loop = (|| -> anyhow::Result<()> {
                    let messages =
                        serde_json::Deserializer::from_reader(input_t).into_iter::<Value>();
                    for message in messages {
                        if stopping_t.load(Ordering::Relaxed) {
                            return Ok(());
                        }
                        let message = message.context("decoding hypervisor event")?;
                        if let Some(event) = backend.handle_event(message, &mut stream_t)? {
                            // This is currently only needed because we have the qemu handshake we need
                            // to do.
                            // TODO: Improve this somehow, maybe move the handshake out?
                            sender.send(Ok(event))?;
                        }
                    }
                    Ok(())
                })();
                if let Err(error) = main_loop
                    && !stopping_t.load(Ordering::Relaxed)
                {
                    let _ = sender.send(Err(error));
                }
            })
            .context("starting hypervisor event monitor")?;
        Ok(Self {
            reader: parent_stream,
            stopping,
            thread: Some(thread),
        })
    }

    pub(super) fn stop(&mut self) {
        if let Some(thread) = self.thread.take() {
            self.stopping.store(true, Ordering::Relaxed);
            let _ = self.reader.shutdown(Shutdown::Both);
            let _ = thread.join();
        }
    }
}

impl Drop for EventMonitor {
    fn drop(&mut self) {
        self.stop();
    }
}
