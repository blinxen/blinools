# blinxen's tools

A collection of my various bash scripts converted into a Rust CLI (Linux only).

## Commands

| Command | Description |
| --- | --- |
| [`blinools wip-pr`](#blinools-wip-pr) | 🚧 TODO |
| [`blinools sandbox`](#blinools-sandbox) | Create and manage sandboxes |
| [`blinools completions`](#shell-completions) | Generate shell completion scripts |

## Installation

### Prebuilt binaries

Tagged releases are built for `x86_64-unknown-linux-gnu` and `aarch64-unknown-linux-gnu`.
Grab the archive for your architecture from the [Releases](../../releases) page,
then verify and unpack it:

```bash
sha256sum -c SHA256SUMS
tar xzf blinools-<version>-<target>.tar.gz
```

### Build from source

```bash
git clone https://github.com/blinxen/blinools.git
cd blinools
cargo install --path .
```

This installs the `blinools` binary to `~/.cargo/bin`.

### Shell completions

`blinools completions` supports `bash`, `elvish`, `fish`, `powershell` and `zsh`:

```bash
# Bash
echo 'source <(blinools completions bash)' >> ~/.bashrc

# Zsh
echo 'source <(blinools completions zsh)' >> ~/.zshrc

# Fish
echo 'blinools completions fish | source' >> ~/.config/fish/config.fish
```

## Configuration reference

The configuration file uses the TOML format and can be configured using:

- a global configuration file located at `$XDG_CONFIG_HOME/blinools/blinools.toml` or `$HOME/.config/blinools/blinools.toml` if `$XDG_CONFIG_HOME` is not defined
- a `blinools.toml` in the current working directory if it exists
- the `-c` / `--config` flag, available on every command, which is read instead of the `blinools.toml` in the current working directory

```bash
blinools --config ./my-sandbox.toml sandbox create
```

The order in which they are loaded is global -> project, where the project configuration is the
file passed with `-c` / `--config` or, when the flag is not passed, the `blinools.toml` in the
current working directory.
The two are merged **per key**, so the project file only overrides the
keys it actually sets and inherits everything else from the global file.

`shares` is the exception, because arrays are replaced instead of merged: the project file's
`shares` are the whole list and the global ones are dropped, so a project file that does not
mention `shares` gets none at all. Set `inherit_shares = true` to get the global shares as well,
layered underneath the project file's ones. A share that both files define under the same name is
taken from the project file as a whole. `inherit_shares` is itself a normal key, so the global file can
set it and the project file can override it. Without a project configuration there is nothing to
inherit from and the global `shares` always apply, whatever `inherit_shares` says.

Shares therefore layer up as global -> project configuration -> `--share`, each layer replacing a
share of the same name entirely.

Currently the only config section is `[sandbox]`, used by the [`sandbox`](#blinools-sandbox) command:

| Key | Type | Required | Default | Notes |
| --- | --- | --- | --- | --- |
| `name` | string | No | The current directory's name, or a random 16 character name if that does not work | Overridden by the `[NAME]` argument to `sandbox create`. |
| `kernel` | path | **Yes** | - | - |
| `kernel_cmdline` | string | No | `""` | Must not contain `console=` or `root=` (already set for you, see [How it works](#how-it-works)) |
| `rootfs` | path | **Yes** | - | Treated as a read-only base image |
| `rootfs_type` | `"Raw"` \| `"QCOW2"` | No | `"Raw"` | Format of the file at `rootfs` |
| `memory_mb` | integer | **Yes** | - | Accepted range is 512 – 131072 (0.5 – 128 GiB) |
| `cpus` | integer | **Yes** | - | Accepted range is 1 – 255 |
| `dns` | array of strings | No | - | DNS server IPs to use inside the guest |
| `shares` | array of tables | No | - | Can also be set / overridden per-run with `--share`, see [`sandbox create`](#sandbox-create) |
| `shares[].name` | string | **Yes** | - | Used as the guest mount point `/mnt/<name>`. |
| `shares[].host_dir` | path | **Yes** | - | - |
| `shares[].read_only` | bool | No | `false` | |
| `shares[].read_only_paths` | array of paths | No | `[]` | Paths inside the share that are read-only |
| `shares[].hidden_paths` | array of paths | No | `[]` | Paths inside the share that should be hidden (replaced by an empty file or directory) |
| `inherit_shares` | bool | No | `false` | Only relevant when there is a project configuration: when `true` the global `shares` are merged underneath the ones of the project file |
| `guest_uid` | integer | No | `1000` | UID host files appear as inside the guest, see [shares](#shares-and-file-ownership) |
| `guest_gid` | integer | No | `1000` | GID host files appear as inside the guest, see [ shares](#shares-and-file-ownership) |
| `cloud_hypervisor.binary` | path | No | Resolved from `$PATH` as `cloud-hypervisor` | |
| `passt.binary` | path | No | Resolved from `$PATH` as `passt` | |
| `virtiofsd.binary` | path | No | Resolved from `$PATH` as `virtiofsd` | |

```toml
[sandbox]
# Optional custom paths to the binary files
cloud_hypervisor.binary = "/path/to/cloud-hypervisor"
passt.binary = "/path/to/passt"
virtiofsd.binary = "/path/to/virtiofsd"
# Path to the kernel
kernel = "/boot/vmlinuz-7.1.10-200.fc44.x86_64"
# Kernel command line parameters to pass
# "console" and "root" must not be configured here
# They are hardcoded to "console=hvc0 root=/dev/vda" for now
kernel_cmdline = "rw quiet"
# Path to the rootfs
rootfs = "./rootfs.img"
# How much memory the VM should have in megabytes
memory_mb = 8192
# How many cores the VM should have
cpus = 4
# Optional list of DNS servers to use in the VM
dns = ["192.168.1.1"]
# Optional paths to automatically mount under /mnt when the VM is started
# Can also be defined with the --share flag, see blinools sandbox create --help
shares = [
    { name = "share-name", host_dir = "/path/to/a/directory", read_only = false },
]
# Whether the shares of the global config file should be kept when this file is used as the
# project configuration
inherit_shares = false
```

## `blinools wip-pr`

> [!WARNING]
> **TODO**: not implemented yet. The command is wired up and accepts the
> arguments below, but currently does nothing.

### CLI reference

```bash
blinools wip-pr <BRANCH_NAME> [-t|--branch-type <TYPE>] [-n|--task-number <NUM>]
```

| Argument | Required | Default | Description |
| --- | --- | --- | --- |
| `<BRANCH_NAME>` | **Yes** | - | Branch name |
| `-t, --branch-type <TYPE>` | No | none | Branch type |
| `-n, --task-number <NUM>`| No | none | Task number |

## `blinools sandbox`

> [!WARNING]
> This is still under heavy development and still contains rough edges.

Manage sandboxes. The `sandbox` subcommand was initially created for isolating
AI harnesses, but it can be used for anything.

Every `sandbox` subcommand requires a `[sandbox]` table in your resolved
configuration. Also see [Configuration reference](#configuration-reference).

### Requirements

- [Cloud Hypervisor](https://github.com/cloud-hypervisor/cloud-hypervisor) is used for creating microVMs.
- [passt](https://passt.top) is used to enable networking.
- [virtiofsd](https://gitlab.com/virtio-fs/virtiofsd) is used for sharing directories with the microVM.
- Hardware virtualization (KVM) enabled, with access to `/dev/kvm` (e.g. your user is a member of the `kvm` group).

Fedora:

```bash
# virtiofsd is installed under /usr/libexec by default, I recommend configuring "virtiofsd.binary" in the config file.
sudo dnf install passt virtiofsd
```

Cloud Hypervisor is not packaged in most distributions, you can either
[download a pre-built binary](https://www.cloudhypervisor.org/docs/prologue/quick-start/#use-pre-built-binaries)
or
[build it from source](https://www.cloudhypervisor.org/docs/prologue/quick-start/#building-from-source).

## Quick start

To create a sandbox, you will need a compiled Linux kernel and a rootfs.
You don't *have* to actually compile your own kernel, you can just use whatever
your distro provides. The example configuration below uses the official Fedora 44 kernel.
The rootfs can also be easily created using `podman` (or `docker`).
Check out the [examples](./examples) directory. The example builds a minimal Fedora
kernel + rootfs pair with `examples/fedora/Dockerfile` and
`examples/fedora/build-rootfs.sh`.

The next steps assume you already have a compiled Linux kernel and a built rootfs.
See [Configuration reference](#configuration-reference) below for the full list of options.

1. Create a configuration file

```toml
[sandbox]
kernel = "/boot/vmlinuz-7.1.10-200.fc44.x86_64"
kernel_cmdline = "rw quiet"
rootfs = "./rootfs.img"
memory_mb = 8192
cpus = 4
dns = ["192.168.1.1"]
```

2. Create the sandbox

```bash
blinools sandbox create
```

3. From inside the guest, shut it down when you're done (`sudo poweroff`), or from another terminal:

```bash
blinools sandbox shutdown <name>
```

### CLI reference

#### `sandbox ps`

Lists every sandbox and its state (`Running`, `Stopped`, or `Unknown`).

```bash
blinools sandbox ps

+-----------+---------+
| name      | state   |
+-----------+---------+
| blinools2 | Running |
+-----------+---------+
| blinools  | Stopped |
+-----------+---------+
```

#### `sandbox create`

Creates a sandbox and attaches to its console in the foreground. The command
blocks until the guest shuts down
(either from inside the VM, e.g. `sudo poweroff`, or via `blinools sandbox shutdown` from another terminal).

```bash
blinools sandbox create [NAME] [-s|--share <SHARE>]... [--recreate] [--delete-after-shutdown]
```

| Argument | Default | Description |
| --- | --- | --- |
| `[NAME]` | The `name` from the config file, if omitted then the current directory name is used. If for any reason that fails too then a random name is generated. | Name of the sandbox. Must not collide with an already-running sandbox. |
| `-s, --share <SHARE>` | none | Mount a host directory into the guest. Repeatable. See [share syntax](#share-syntax) below. Merges with (and overrides, by name) the `shares` list in the config file. |
| `--recreate` | `false` | Reset the sandbox back to a clean state, wiping any changes made to the rootfs. |
| `--delete-after-shutdown` | `false` | Automatically run the equivalent of `sandbox delete --force` once the guest shuts down. |

##### Share syntax

| Format | Example | Result |
| --- | --- | --- |
| `PATH` | `-s ./data` | Mounted read-write, name taken from the directory's name |
| `PATH:(ro\|rw)` | `-s ./data:ro` | Mounted read-only, name taken from the directory's name |
| `NAME:PATH:(ro\|rw)` | `-s data:./data:rw` | Mounted read-write under the explicit name `data` |

Inside the guest, a share appears at `/mnt/<name>` if you are using `systemd` as your init system.

```bash
# Start (or resume) a sandbox named "scratch", sharing the current
# directory read-write and ~/notes read-only
blinools sandbox create scratch -s "$(pwd)" -s "notes:$HOME/notes:ro"
```

If you are not using `systemd` then you can manually mount the shares with `mount -t virtiofs <name> mount_dir/`.

#### `sandbox shutdown`

Asks a running sandbox to shut down. No-op if the sandbox isn't running.

```bash
blinools sandbox shutdown <NAME>
```

#### `sandbox delete`

Deletes a sandbox and everything it stored, including any files created
inside the guest. This is destructive and cannot be undone.

```bash
blinools sandbox delete <NAME> [-f|--force]
```

| Flag | Default | Description |
| --- | --- | --- |
| `-f, --force` | `false` | Skip the "sandbox is running" check and delete anyway, without first shutting it down cleanly. |

Without `--force`, deleting a running sandbox fails with an error asking you
to shut it down first (or pass `--force`).

### How it works

Each sandbox is a Linux microVM. `blinools sandbox create` acts as an
orchestrator. It spawns a small set of helper processes and wires them
together over local Unix sockets, then hands control of the VM's console to
your terminal.

```mermaid
flowchart LR
    CLI["blinools sandbox create"] -->|spawns| CH["cloud-hypervisor (VMM)"]
    CLI -->|spawns| PASST["passt (networking)"]
    CLI -->|spawns, 1 per share| VFSD["virtiofsd"]

    PASST <-->|vhost-user socket| CH
    VFSD <-->|virtio-fs socket| CH
    CH -->|KVM| VM["microVM: kernel + rootfs"]

    CH <-->|pty| FILTER["escape sequence filter"]
    FILTER <--> TERM(("your terminal"))

    PASST -->|NAT via 10.200.0.2/24| NET(("host network"))
    VFSD -.->|mounted at /mnt/name| VM
```

- **[Cloud Hypervisor](https://github.com/cloud-hypervisor/cloud-hypervisor)**
  is the VMM. It boots your configured `kernel` against a disk built from
  `rootfs`, using KVM for hardware virtualization.
- **[passt](https://passt.top)** provides user-mode networking over a vhost-user socket.
  The guest always gets the static address `10.200.0.2/24` with gateway `10.200.0.1`, NAT'd out to the host's network.
  Custom DNS servers can be pushed to the guest via the `dns` config option.
- **[virtiofsd](https://gitlab.com/virtio-fs/virtiofsd)** creates one instance per
  configured share and exposes a host directory to the guest over virtio-fs.
  Each share is mounted automatically at boot at `/mnt/<name>` via a kernel
  command line option, so nothing needs to run inside the guest to pick it up.
  This only happens if you are using `systemd`.
- **Disk overlay**: `rootfs` is treated as a read-only backing image and is
  never modified. On first run, blinools creates a writable qcow2 overlay in
  the sandbox's [state directory](#on-disk-layout). All writes made inside
  the guest land in that overlay and persist across restarts.
  `--recreate` discards this overlay and starts clean.
- **Console filter**: the guest console is not attached to your terminal directly. blinools puts a
  pty in between and filters what the guest writes, see
  [Console and your terminal](#console-and-your-terminal).
- The kernel command line is always prefixed with
  `console=hvc0 root=/dev/vda rw systemd.hostname=<name>`.
  `console` and `root` can't be overridden via `kernel_cmdline`.

#### Console and your terminal

The guest gets a real terminal, and you get a normal shell, but the two are not wired straight
together. blinools allocates a pty, gives the hypervisor the slave side and keeps the master, then
filters problematic escape sequences on their way to your terminal emulator.

#### Shares and file ownership

Shares are read-write unless you ask for `ro`, which means the guest can rewrite anything it can see.

virtiofsd is told to squash every guest UID and GID onto the user that started the sandbox, so the
guest cannot express any ownership on the host beyond what that user already has. In the other
direction every host UID and GID shows up in the guest as `guest_uid` / `guest_gid` (`1000` by
default), so shared files stay writable for the guest's normal user.

#### On-disk layout

| Path | Contents | Lifetime |
| --- | --- | --- |
| `$XDG_RUNTIME_DIR/blinools/<name>/` (or `/run/user/<uid>/blinools/<name>/`) | Cloud Hypervisor API socket, passt socket, virtiofsd sockets | While the sandbox is running, cleaned up on shutdown / delete |
| `$XDG_RUNTIME_DIR/blinools/<name>.lock` | Empty file, locked while blinools works on that sandbox | Until the sandbox is shutdown |
| `$XDG_STATE_HOME/blinools/<name>/` (or `~/.local/state/blinools/<name>/`) | The qcow2 disk overlay holding everything written inside the guest | Persists across restarts, until `sandbox delete` or `--recreate` |

## License

The source code is primarily distributed under the terms of the MIT License.
See LICENSE for details.
