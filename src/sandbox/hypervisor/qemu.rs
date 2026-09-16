use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus};
use std::time::Duration;

use anyhow::Context;
use serde::Deserialize;
use serde_json::{Value, json};

use crate::sandbox::config::Config;
use crate::sandbox::hypervisor::{
    Hypervisor, State, Vm, VmConfig, can_connect_to_socket, create_qcow2_overlay, guest_cmdline,
};
use crate::sandbox::process::{die_with_parent, kill_child_and_cleanup, remove_stale_socket};
use crate::sandbox::{socket_path, socket_path_in};

const QMP_TIMEOUT: Duration = Duration::from_secs(5);
const MEMORY_BACKEND: &str = "guest-memory";
pub const SOCKET_NAME: &str = "qemu-qmp";

pub struct Qemu {
    binary: PathBuf,
}

impl Qemu {
    pub fn new(config: Option<&Config>) -> Self {
        let mut binary = PathBuf::from("qemu-system-x86_64");
        if let Some(config) = config
            && let Some(qemu) = config.qemu.as_ref()
            && let Some(configured) = qemu.binary.as_ref()
        {
            binary = configured.to_path_buf();
        }
        Qemu { binary }
    }
}

struct QemuVm {
    socket_path: PathBuf,
    handle: Child,
}

#[derive(Deserialize)]
struct QmpStatus {
    status: String,
}

impl Hypervisor for Qemu {
    fn boot(&self, cfg: VmConfig) -> Result<Box<dyn Vm>, anyhow::Error> {
        let mut mount_args: Vec<String> = Vec::new();
        let cmdline = guest_cmdline(&cfg);

        for mount in cfg.mounts {
            mount_args.push("-chardev".into());
            mount_args.push(format!(
                "socket,id=char-{tag},path={sock}",
                tag = mount.tag,
                sock = mount.socket_path.display()
            ));
            mount_args.push("-device".into());
            mount_args.push(format!(
                "vhost-user-fs-device,queue-size=512,chardev=char-{tag},tag={tag}",
                tag = mount.tag
            ));
        }

        let api_socket = socket_path(cfg.name, SOCKET_NAME);
        remove_stale_socket(&api_socket);

        let overlay = create_qcow2_overlay(&cfg)?;

        let mut command = Command::new(&self.binary);
        command
            .arg("-machine")
            .arg(format!(
                "microvm,x-option-roms=off,pit=off,pic=off,isa-serial=off,rtc=off,\
                 memory-backend={MEMORY_BACKEND}"
            ))
            .arg("-object")
            .arg(format!(
                "memory-backend-memfd,id={MEMORY_BACKEND},size={}M,share=on",
                cfg.memory_mb
            ))
            .arg("-enable-kvm")
            .arg("-cpu")
            .arg("host")
            .arg("-m")
            .arg(format!("{}m", cfg.memory_mb))
            .arg("-smp")
            .arg(cfg.cpus.to_string())
            .arg("-kernel")
            .arg(cfg.kernel)
            .arg("-append")
            .arg(cmdline)
            .arg("-serial")
            .arg("stdio")
            .arg("-nodefaults")
            .arg("-no-reboot")
            .arg("-no-user-config")
            .arg("-nographic")
            .arg("-chardev")
            // "signal=off" keeps qemu from eating Ctrl-C
            // the guest is supposed to see it
            .arg("stdio,id=virtiocon0,signal=off")
            .arg("-device")
            .arg("virtio-serial-device")
            .arg("-device")
            .arg("virtconsole,chardev=virtiocon0")
            .arg("-drive")
            .arg(format!(
                "id=root,file={},format=qcow2,if=none",
                overlay.display()
            ))
            .arg("-device")
            .arg("virtio-blk-device,drive=root")
            .args(mount_args)
            .arg("-qmp")
            .arg(format!("unix:{},server=on,wait=off", api_socket.display()))
            .arg("-D")
            .arg(api_socket.with_extension("log"))
            .arg("-global")
            .arg("virtio-mmio.force-legacy=false");

        if let Some(network_socket) = cfg.network_socket {
            command
                .arg("-chardev")
                .arg(format!(
                    "socket,id=char-net0,path={}",
                    network_socket.display()
                ))
                .arg("-netdev")
                .arg("vhost-user,id=net0,chardev=char-net0,queues=1")
                .arg("-device")
                .arg("virtio-net-device,netdev=net0");
        }

        if log::log_enabled!(log::Level::Trace) {
            command.arg("-d").arg("guest_errors,unimp,cpu_reset");
        } else if log::log_enabled!(log::Level::Debug) {
            command.arg("-d").arg("guest_errors,unimp");
        }

        die_with_parent(&mut command);

        command
            .stdin(cfg.console.stdin)
            .stdout(cfg.console.stdout)
            .stderr(cfg.console.stderr);

        log::debug!("Starting command: {:?}", command);
        let handle = command.spawn().context("spawning qemu")?;

        Ok(Box::new(QemuVm {
            socket_path: api_socket,
            handle,
        }))
    }

    fn is_running(&self, sandbox_runtime_dir: &Path) -> bool {
        can_connect_to_socket(&socket_path_in(sandbox_runtime_dir, SOCKET_NAME))
    }

    fn shutdown(&self, sandbox_runtime_dir: &Path, _force: bool) -> Result<(), anyhow::Error> {
        let api_socket_path = socket_path_in(sandbox_runtime_dir, SOCKET_NAME);
        if can_connect_to_socket(&api_socket_path) {
            let response = qmp(&api_socket_path, "quit", None)
                .context("requesting qemu to shut down the sandbox")?;

            if response.get("return").is_none() {
                return Err(anyhow::anyhow!(
                    "shutting down the sandbox was not successful: {response}"
                ));
            }
        }

        Ok(())
    }

    fn state(&self, sandbox_runtime_dir: &Path) -> State {
        let api_socket_path = socket_path_in(sandbox_runtime_dir, SOCKET_NAME);
        if !can_connect_to_socket(&api_socket_path) {
            return State::Stopped;
        }

        match qmp(&api_socket_path, "query-status", None) {
            Ok(response) => match response.get("return").cloned() {
                Some(ret) => match serde_json::from_value::<QmpStatus>(ret) {
                    Ok(status) => State::Running(status.status.to_uppercase()),
                    Err(_) => State::Unknown,
                },
                None => State::Unknown,
            },
            Err(_) => State::Unknown,
        }
    }
}

impl Vm for QemuVm {
    fn wait(&mut self) -> Result<ExitStatus, std::io::Error> {
        self.handle.wait()
    }

    fn terminate(&mut self) {
        kill_child_and_cleanup(&mut self.handle, &[&self.socket_path]);
    }
}

impl Drop for QemuVm {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.socket_path);
    }
}

fn qmp(
    socket_path: &Path,
    command: &str,
    arguments: Option<Value>,
) -> Result<Value, anyhow::Error> {
    qmp_with_timeout(socket_path, command, arguments, QMP_TIMEOUT)
}

// See https://www.qemu.org/docs/master/interop/qmp-spec.html
fn qmp_with_timeout(
    socket_path: &Path,
    command: &str,
    arguments: Option<Value>,
    timeout: Duration,
) -> Result<Value, anyhow::Error> {
    let mut stream = UnixStream::connect(socket_path)
        .with_context(|| format!("connecting to qmp socket at {}", socket_path.display()))?;
    stream
        .set_read_timeout(Some(timeout))
        .context("setting a read timeout on the qmp socket")?;
    stream
        .set_write_timeout(Some(timeout))
        .context("setting a write timeout on the qmp socket")?;

    let mut responses =
        serde_json::Deserializer::from_reader(stream.try_clone()?).into_iter::<Value>();

    let ack = execute(&mut stream, &mut responses, "qmp_capabilities", None)?;
    if ack.get("return").is_none() {
        return Err(anyhow::anyhow!("qmp_capabilities failed: {ack}"));
    }

    execute(&mut stream, &mut responses, command, arguments)
}

fn execute<'de, R: serde_json::de::Read<'de>>(
    stream: &mut UnixStream,
    responses: &mut serde_json::StreamDeserializer<'de, R, Value>,
    command: &str,
    arguments: Option<Value>,
) -> Result<Value, anyhow::Error> {
    let mut request = json!({ "execute": command, "id": command });
    if let Some(arguments) = arguments {
        request["arguments"] = arguments;
    }
    serde_json::to_writer(&mut *stream, &request)
        .with_context(|| format!("sending qmp command {command}"))?;

    loop {
        let message = responses
            .next()
            .with_context(|| format!("qmp socket closed before answering {command}"))?
            .with_context(|| format!("reading the response to qmp command {command}"))?;

        if message.get("id").and_then(Value::as_str) == Some(command) {
            return Ok(message);
        }
    }
}

#[cfg(test)]
mod tests {
    use std::io::{Read, Write};
    use std::os::unix::net::UnixListener;
    use std::time::Instant;

    use crate::sandbox::socket_path_in;

    use super::*;
    use tempfile::tempdir;

    const TEST_TIMEOUT: Duration = Duration::from_millis(200);

    fn serve(socket_path: &Path, answer: fn(UnixStream)) {
        let listener = UnixListener::bind(socket_path).unwrap();
        std::thread::spawn(move || {
            while let Ok((stream, _)) = listener.accept() {
                answer(stream);
            }
        });
    }

    fn answer_query_status(mut stream: UnixStream) {
        let _ = stream.write_all(
            br#"{"QMP":{"version":{"qemu":{"major":10,"minor":0,"micro":0}},"capabilities":[]}}
                {"return":{},"id":"qmp_capabilities"}
                {"event":"VSERPORT_CHANGE","data":{"open":true,"id":"virtiocon0"}}
                {"timestamp":{"seconds":1,"microseconds":0},"event":"RTC_CHANGE"}
                {"return":{"status":"running","running":true},"id":"query-status"}"#,
        );
        // Keep the connection open until the client is done with it
        let _ = stream.read_to_end(&mut Vec::new());
    }

    #[test]
    fn the_state_is_read_from_the_socket_boot_creates() {
        let dir = tempdir().unwrap();
        serve(
            &socket_path_in(dir.path(), SOCKET_NAME),
            answer_query_status,
        );
        let hypervisor = Qemu::new(None);

        assert!(hypervisor.is_running(dir.path()));
        assert_eq!(hypervisor.state(dir.path()).to_string(), "RUNNING");
    }

    #[test]
    fn a_sandbox_without_a_socket_is_stopped() {
        let dir = tempdir().unwrap();
        let hypervisor = Qemu::new(None);

        assert!(!hypervisor.is_running(dir.path()));
        assert!(matches!(hypervisor.state(dir.path()), State::Stopped));
    }

    #[test]
    fn the_greeting_and_asynchronous_events_are_skipped() {
        let dir = tempdir().unwrap();
        let socket_path = dir.path().join("qmp.sock");
        serve(&socket_path, answer_query_status);

        let response = qmp(&socket_path, "query-status", None).unwrap();

        assert_eq!(response["return"]["status"], "running");
    }

    #[test]
    fn a_vmm_that_never_answers_times_out() {
        let dir = tempdir().unwrap();
        let socket_path = dir.path().join("qmp.sock");
        serve(&socket_path, |stream| {
            std::thread::sleep(Duration::from_secs(30));
            drop(stream);
        });

        let started = Instant::now();
        let error = qmp_with_timeout(&socket_path, "query-status", None, TEST_TIMEOUT)
            .expect_err("reading from a silent VMM has to fail");

        assert!(started.elapsed() < Duration::from_secs(5), "{error}");
    }
}
