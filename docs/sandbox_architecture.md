# Sandbox Architecture

This document describes how sandboxes are built.

## Architecture

Each sandbox is a Linux microVM. `blinools sandbox create` acts as an
orchestrator. It spawns a small set of helper processes and wires them
together over local Unix sockets, then hands control of the VMs console to
your terminal.

```mermaid
flowchart LR
    CLI["blinools sandbox create"] -->|spawns| VMM["qemu-system-x86_64 or cloud-hypervisor (VMM)"]
    CLI -->|spawns| PASST["passt (networking)"]
    CLI -->|spawns| PASTA["pasta (host side of the network)"]
    CLI -->|spawns, 1 per share| VFSD["virtiofsd"]

    PASST <-->|vhost-user socket| VMM
    VFSD <-->|virtio-fs socket| VMM
    VMM -->|KVM| VM["microVM: kernel + rootfs"]

    VMM <-->|pty| FILTER["escape sequence filter"]
    FILTER <--> TERM(("your terminal"))

    PASTA <-->|joins passt's network namespace| PASST
    PASTA -->|NAT via 10.200.0.2/24| NET(("host network"))
    VFSD -.->|mounted at /mnt/name| VM
```

- **VMM**: [QEMU](https://www.qemu.org/) (`qemu-system-x86_64`) is the VMM by
  default (`hypervisor = "Qemu"`). [Cloud
  Hypervisor](https://github.com/cloud-hypervisor/cloud-hypervisor) can be
  used instead with `hypervisor = "CloudHypervisor"`. The chosen VMM boots
  your configured `kernel` against a disk built from `rootfs`, using KVM for
  hardware virtualization, and is controlled over a local socket (QEMU QMP
  socket, or Cloud Hypervisor API socket).
- **[passt](https://passt.top) and pasta**: Provide user-mode networking over a vhost-user socket.
  The sandbox always gets the static address `10.200.0.2/24` with gateway `10.200.0.1`, NAT'd out to the hosts network.
  Custom DNS servers can be pushed to the sandbox via the `dns` config option.
  The `network` option decides how much of the host network the sandbox can reach,
  see [Networking](#networking).
- **[virtiofsd](https://gitlab.com/virtio-fs/virtiofsd)**: Creates one instance per
  configured share and exposes a host directory to the sandbox over virtio-fs.
  Each share is mounted automatically at boot at `/mnt/<name>` via a kernel
  command line option, so nothing needs to run inside the sandbox to pick it up.
  The automatic mounting only happens if you are using `systemd`.
- **Disk overlay**: `rootfs` is treated as a read-only backing image and is
  never modified. On the first run, `blinools` creates a writable qcow2 overlay in
  the sandboxs [state directory](#on-disk-layout). All writes made inside
  the sandbox land in that overlay and persist across restarts.
  `--recreate` discards this overlay and starts clean.
- **Process isolation**: The VMM and every helper process are started in their own set of Linux
  namespaces, see [Process isolation](#process-isolation).
- **Resource confinement**: Linux cgroups v2 is used to limit CPU, memory, and pids
  for the VMM and its helper processes (passt, pasta, virtiofsd). CPU is capped to
  the configured `cpus`, memory to `memory_mb` plus a 256 MB overhead with
  swap disabled, and pids to `1024`. This currently only works if
  `user.slice/user-<uid>.slice/user@<uid>.service` already exists under
  `/sys/fs/cgroup` (created by systemd when your user session starts) and
  `blinools` can create a `blinools.slice` subgroup under it.
  A warning will be logged if `blinools` can't create the cgroup.
  Processes are started directly inside the cgroup (`CLONE_INTO_CGROUP`) instead of moving
  themselves into it after they started.
- **Console filter**: The sandbox console is not attached to your terminal directly. `blinools` puts a
  pty in between and filters what the sandbox writes, see
  [Console and your terminal](#console-and-your-terminal).
- The kernel command line is always prefixed with
  `console=hvc0 root=/dev/vda rw systemd.hostname=<name>`.
  `console` and `root` can't be overridden via `kernel_cmdline`.

### Console and your terminal

The sandbox gets a real terminal, and you get a normal shell, but the two are not wired straight
together. `blinools` allocates a pty, gives the hypervisor the slave side and keeps the master, then
filters problematic escape sequences on their way to your terminal emulator.

Keyboard signals such as `Ctrl-C` are not interpreted by the hypervisor, they are passed on to the
guest (QEMU is started with `signal=off` on its console for this reason). When the guest goes away
(the pty reports `EIO`/EOF), any input that is still queued for it is dropped and `blinools` moves on
to waiting for the VM to exit.

### Process isolation

All processes `blinools` starts (the VMM, passt and every virtiofsd) go through the same code path.
A process is created using `clone3` and does the following configurations (before the actual binary is executed):

- fresh **user, mount, network, PID, IPC, UTS and time namespaces**.
  The caller's `UID`/`GID` is mapped to `0` inside the user namespace, `setgroups` is denied.
- a **private mount tree** (`/` is made recursively private) with a freshly mounted `/proc` (`nosuid,nodev,noexec`).
- a **new cgroup namespace**, which hides the cgroup root from the process.
- placement **directly into the sandbox cgroup** (see [Resource confinement](#architecture)).

### Networking

Unless `network = "None"` is configured, a sandbox gets its network from two cooperating programs:

1. `passt` is started first, isolated in its **own network namespace** (see [Process isolation](#process-isolation))
   and serves the sandbox NIC over a vhost-user socket. It is not allowed to listen on any host port
   (port forwarding is disabled), `ICMP` is disabled and the host loopback and gateway mappings are off.
2. `pasta` is started in the host namespaces and attaches to the network namespace of `passt`.
   It connects that namespace to the real host network and NATs the sandbox traffic out.

Because `passt` runs in a separate network namespace, the firewall for the `Restricted` mode is installed
**inside that namespace** right before `passt` is executed (so there is no window where it runs without
it). It is an `nftables` table (`ROOT_TABLE`, family `inet`) with these rules:

| Chain | Policy | Rules |
| --- | --- | --- |
| `input` | drop | **ONLY** accept established and related connections |
| `forward` | drop | None, the namespace never routes |
| `output` | accept | Drop everything going to `lo`, accept the IPs listed in `dns` (IPv4 and IPv6), drop `10.0.0.0/8`, `172.16.0.0/12`, `192.168.0.0/16`, `100.64.0.0/10` and `169.254.0.0/16`, drop `fc00::/7` and `fe80::/10` |

| `network` | passt/pasta | Firewall |
| --- | --- | --- |
| `Restricted` (default) | Started | Installed as described above |
| `Unrestricted` | Started | None |
| `None` | Not started, the VMM gets no NIC | - |

### Shares and file ownership

[virtiofsd](https://gitlab.com/virtio-fs/virtiofsd) is used to share host directories into the sandbox.
One instance is spawned per configured share, exposing that share's `host_dir` to the sandbox over `virtio-fs`.
Shares are read-write by default unless you set `ro` via the command line or set the `read_only` field to `true` in the config file.

`virtiofsd` is also told to squash every sandbox `UID` and `GID` onto the user that created / started the sandbox.
The sandbox cannot express any ownership on the host beyond what that user already has.
In the other direction (host to sandbox) every host `UID` and `GID` shows up in the sandbox as `sandbox_user_uid` / `sandbox_user_gid` (`1000` by default).

`read_only_paths` and `hidden_paths` are an additional mechanism to limit what a sandbox can see in a share. They work on subpaths inside the share instead of the whole share.
Both are implemented with mount namespaces on the host.
This means that a read-only or hidden file or directory in the sandbox is genuinely read-only or hidden on the host too.
To be precise, they are read-only / hidden in the created mount namespace of the `virtiofsd` process.

Before `virtiofsd` is started, `blinools` unshares a fresh user, mount and network
namespace for it and makes `/` private and recursive, so none of these mounts are visible outside
that one process:

- Paths in `read_only_paths` are bind-mounted onto themselves and remounted read-only, leaving the
  rest of the share writable. This is only done when the share itself isn't already read-only.
  If it is already configured read-only, then the whole `host_dir` gets bind-mounted onto itself and then remounted as read-only.
- Paths in `hidden_paths` are replaced but not actually removed / replaced.
  A directory gets covered with an empty, read-only `tmpfs`.
  A file gets bind-mounted over itself with an empty file.
  The paths still exist and are still visible to the sandbox, they are just empty.

Some command line flags of `sandbox create` change what happens to these paths for a single run:

- `--read-only` mounts every share read-only, the same way a share with `read_only = true` is handled.
- `--no-hidden` skips `hidden_paths` entirely, hidden paths stay read-write.
- `--hidden-as-read-only` mounts `hidden_paths` read-only instead of hiding them.

### Git integration

The [configuration reference](configuration.md#options) documents `git_action`.

#### Git actions

Using the same mount namespace mechanism as `read_only_paths` and `hidden_paths`, `blinools` walks every share
(without following symlinks) and looks for entries named `.git`. This covers both repositories (a directory) and
worktrees (a file). What happens to them is controlled by `git_action`, which can be set globally and per share
(the per share value wins):

- `ReadOnly` (default): the `.git` directory or file is bind-mounted onto itself and remounted read-only.
- `Hide`: a `.git` directory is covered with an empty read-only `tmpfs`, a `.git` file is replaced by an empty file.
- `None`: nothing is done.

The walk happens once, in the `virtiofsd` process before it starts serving the share. A `.git` that is created
inside the share afterwards is not covered.

### On-disk layout

| Path | Contents | Lifetime |
| --- | --- | --- |
| `$XDG_RUNTIME_DIR/blinools/<name>/` (or `/run/user/<uid>/blinools/<name>/`) | Hypervisor control socket (QEMU QMP socket or Cloud Hypervisor API socket), passt socket, pasta pid file, virtiofsd sockets, log files (`passt.log`, `pasta.log`, `vfsd-<share>.log`, ...) | While the sandbox is running, cleaned up on shutdown / delete |
| `$XDG_RUNTIME_DIR/blinools/<name>.lock` | Empty file, locked while blinools works on that sandbox | Until the sandbox is shutdown |
| `$XDG_STATE_HOME/blinools/<name>/` (or `~/.local/state/blinools/<name>/`) | The qcow2 disk overlay holding everything written inside the guest | Persists across restarts, until `sandbox delete` or `--recreate` or `--delete-after-shutdown` |
