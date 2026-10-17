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

| Key | Type | Required | Default | Description |
| --- | --- | --- | --- | --- |
| `name` | string | No | The name of the current directory, or a random 16 character name if that does not work | Overridden by the `[NAME]` argument passed to `sandbox create`. |
| `kernel` | path | **Yes** | - | Path to a Linux Kernel that should be used for the sandbox. |
| `hypervisor` | `"Qemu"` \| `"CloudHypervisor"` | No | `"Qemu"` | VMM to use to boot the sandbox with, see [Requirements](#requirements). |
| `kernel_cmdline` | string | No | `""` | Kernel arguments to pass. Must not contain `console=` or `root=` (see [Architecture](docs/sandbox_architecture.md)). |
| `rootfs` | path | **Yes** | - | Path to a root filesystem that should be used for the sandbox. This root filesystem is always treated as a read-only base. |
| `rootfs_type` | `"Raw"` \| `"QCOW2"` | No | `"Raw"` | File format of `rootfs`. |
| `network` | `"Restricted"` \| `"Unrestricted"` \| `"None"` | No | `"Restricted"` | Type of network access the sandbox gets, see [Networking](#networking). `Restricted` allows outgoing connections to the internet but blocks the local network (private, CGNAT and link-local ranges) and everything incoming, `Unrestricted` additionally allows reaching the local network and `None` deactivates networking completely (sandbox has no connectivity). |
| `memory_mb` | integer | **Yes** | - | How much memory in megabytes the sandbox should received in total. Accepted range is 512 – 131072 (0.5 – 128 GiB). |
| `cpus` | integer | **Yes** | - | How many CPUs the sandbox is allowed to use. Accepted range is 1 – 255. |
| `dns` | array of strings | No | - | DNS server IPs to use inside the sandbox. With `network = "Restricted"` these addresses are also exempt from the local network block, so a resolver on a private address has to be listed here to stay reachable. |
| `git_action` | `"ReadOnly"` \| `"Hide"` \| `"None"` | No | `ReadOnly` | Defines the action to take when encountering `.git` directories (repositories) or files (`worktrees`) when mounting a share, see [Git integration](#git-integration). |
| `shares` | array of tables | No | - | List of host directories to mount into the sandbox. Can also be set / overridden per-run with `--share`, see [`sandbox create`](#sandbox-create). |
| `shares[].name` | string | **Yes** | - | Name of the share. It is used as the sandbox mount point `/mnt/<name>`. |
| `shares[].host_dir` | path | **Yes** | - | Path to the host directory that should be shared. |
| `shares[].read_only` | bool | No | `false` | Defines if the shared directory should be read-only or not. The diffrence between this field and the `read_only_paths` field is that here we are talking about the share as a whole. |
| `shares[].read_only_paths` | array of paths | No | `[]` | Paths inside the share that are read-only. The diffrence between this field and the `read_only` field is that here we are talking about subpaths and not the whole share. |
| `shares[].hidden_paths` | array of paths | No | `[]` | Paths inside the share that should be hidden (replaced by an empty and read-only file or directory). |
| `shares[].git_action` | `"ReadOnly"` \| `"Hide"` \| `"None"` | No | `ReadOnly` | Overrides the global `git_action` for this share, see [Git integration](#git-integration). |
| `inherit_shares` | bool | No | `false` | Decides if the global `shares` are merged underneath the ones of the project file. Only relevant when there is a project configuration. |
| `sandbox_user_uid` | integer | No | `1000` | UID host files appear as inside the guest, see [Shares and file ownership](docs/sandbox_architecture.md#shares-and-file-ownership) |
| `sandbox_user_gid` | integer | No | `1000` | GID host files appear as inside the guest, see [Shares and file ownership](docs/sandbox_architecture.md#shares-and-file-ownership) |
| `qemu.binary` | path | No | Resolved from `$PATH` as `qemu-system-x86_64` | Path to the qemu binary. Only used when `hypervisor` is `"Qemu"` (the default) |
| `cloud_hypervisor.binary` | path | No | Resolved from `$PATH` as `cloud-hypervisor` | Path to the `CloudHypervisor` binary. Only used when `hypervisor` is `"CloudHypervisor"` |
| `passt.binary` | path | No | Resolved from `$PATH` as `passt` | Path to the `passt` binary. |
| `pasta.binary` | path | No | Resolved from `$PATH` as `pasta` | Path to the `pasta` binary. Only used when `network` is not `"None"`. |
| `virtiofsd.binary` | path | No | Resolved from `$PATH` as `virtiofsd` | Path to the `virtiofsd` binary. |

```toml
[sandbox]
hypervisor = "Qemu"
qemu.binary = "/path/to/qemu-system-x86_64"
cloud_hypervisor.binary = "/path/to/cloud-hypervisor"
passt.binary = "/path/to/passt"
pasta.binary = "/path/to/pasta"
virtiofsd.binary = "/path/to/virtiofsd"
kernel = "/boot/vmlinuz-7.1.10-200.fc44.x86_64"
kernel_cmdline = "rw quiet"
rootfs = "./rootfs.img"
memory_mb = 8192
cpus = 4
dns = ["192.168.1.1"]
network = "Restricted"
git_action = "ReadOnly"
shares = [
    { name = "share-name", host_dir = "/path/to/a/directory", read_only = false },
]
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

An architectural overview can be found [here](docs/sandbox_architecture.md).

### Requirements

- A VMM to create microVMs. `hypervisor` in the config selects which one to use (see [Configuration reference](#configuration-reference)):
  - [QEMU](https://www.qemu.org/) (`qemu-system-x86_64`)
  - [Cloud Hypervisor](https://github.com/cloud-hypervisor/cloud-hypervisor)
- [passt](https://passt.top) and its companion `pasta` are used to enable networking (both are shipped in the `passt` package of most distributions).
- `nft` ([nftables](https://netfilter.org/projects/nftables/)) is used to install the firewall of the default `Restricted` network mode. It is not needed with `network = "None"` or `"Unrestricted"`.
- Unprivileged user namespaces. Every process `blinools` starts runs in its own set of namespaces, see [Architecture](docs/sandbox_architecture.md#process-isolation).
- [virtiofsd](https://gitlab.com/virtio-fs/virtiofsd) is used for sharing directories with the microVM.
- Hardware virtualization (KVM) enabled, with access to `/dev/kvm` (e.g. your user is a member of the `kvm` group).

Fedora:

```bash
# virtiofsd is installed under /usr/libexec by default, I recommend configuring "virtiofsd.binary" in the config file.
sudo dnf install qemu-system-x86 passt virtiofsd nftables
```

Cloud Hypervisor is not packaged in most distributions, you can either:
[download a pre-built binary](https://www.cloudhypervisor.org/docs/prologue/quick-start/#use-pre-built-binaries)
or
[build it from source](https://www.cloudhypervisor.org/docs/prologue/quick-start/#building-from-source).

### Quick start

To create a sandbox, you will need a compiled Linux kernel and a rootfs.
You don't *have* to actually compile your own kernel, you can just use whatever
your distro provides. The example configuration below uses the official Fedora 44 kernel.
The rootfs can also be easily created using `podman` (or `docker`).
Check out the [examples](./examples) directory, see [Examples](#examples) below.

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

### Networking

The `network` option controls what the sandbox can reach. The sandbox always gets the static
address `10.200.0.2/24` (gateway `10.200.0.1`), see the
[architecture](docs/sandbox_architecture.md#networking) for how it is wired up.

| Value | Outgoing | Incoming | Description |
| --- | --- | --- | --- |
| `Restricted` (default) | Public internet only | Blocked (except replies to outgoing connections), no ports are forwarded | The sandbox can't reach the host or any machine on your local network. The loopback interface, RFC 1918 ranges, CGNAT (`100.64.0.0/10`, which includes Tailscale), link-local and IPv6 unique-local ranges are blocked. The servers in `dns` are exempt from this, so they stay reachable even when they are on a private address. |
| `Unrestricted` | Internet and local network | No ports are forwarded to the sandbox | Same as `Restricted` but without the local network block. |
| `None` | Nothing | Nothing | Networking is deactivated completely, `passt`, `pasta` and `nft` are not needed. |

### Git integration

`blinools` has some git specific handling when it mounts shares.

#### Git actions

A *git action* decides what happens to the git database and metadata inside a share. When a share is mounted,
`blinools` looks for entries named `.git` anywhere inside it (symlinks are not followed). This covers
repositories (a `.git` directory) and worktrees (a `.git` file that points to the real repository).
The configured action is then applied to each of them:

| Action | Behaviour | Use it when |
| --- | --- | --- |
| `ReadOnly` (default) | `.git` is mounted read-only. | The sandbox should see the repository, but must not be able to change it. |
| `Hide` | `.git` is replaced by an empty, read-only directory or file. The sandbox does not see any repository data. | The sandbox should only see the working tree, not the history. |
| `None` | `.git` is not treated specially. | The sandbox needs full access to the repository, for example to commit. |

The action is selected with `git_action`. Set it in `[sandbox]` to change the default for every share,
and use `shares[].git_action` to override it for a single share. A per-share value always wins.

```toml
[sandbox]
# Default for all shares
git_action = "ReadOnly"
shares = [
    # Uses the default (ReadOnly)
    { name = "project", host_dir = "/home/me/project" },
    # Needs to commit, so no special handling
    { name = "scratch", host_dir = "/home/me/scratch", git_action = "None" },
    # Only the working tree is visible
    { name = "docs", host_dir = "/home/me/docs", git_action = "Hide" },
]
```

A few things to keep in mind:

- Git actions are independent of `read_only`, `read_only_paths` and `hidden_paths`. They are an additional
  layer that only targets `.git`. `--read-only` makes the whole share read-only anyway.
- `.git` entries are looked up once, when the sandbox is created. A `.git` that is created inside the
  share while the sandbox is running is not covered.
- How the actions are enforced is described in the
  [architecture](docs/sandbox_architecture.md#git-integration).

### Examples

The [examples](./examples) directory contains rootfs definitions you can build with `podman` (or `docker`):

| Example | Description |
| --- | --- |
| [`examples/basic`](./examples/basic) | A minimal Fedora 44 rootfs with `systemd`, networking, development tools and an auto-login user. A good starting point for your own rootfs. |
| [`examples/ai-agent`](./examples/ai-agent) | A hardened Fedora 44 rootfs for isolating AI coding agents. Ships Claude Code, OpenCode and Codex, runs as an `agent` user that has no `sudo` rights other than `poweroff`, and automatically bind-mounts every share from `/mnt/<name>` into the agent's home directory. The accompanying `blinools.toml` in the repository root is configured for this example. |

Each example directory has a `build-kernel.sh` and a `build-rootfs.sh` (shared, in [`misc`](./misc)) which produce
`kernel` and `rootfs.img` in that directory:

```bash
cd examples/ai-agent
./build-kernel.sh   # downloads and compiles a Linux kernel with the matching kernel.config
./build-rootfs.sh   # builds the Dockerfile and turns it into an ext4 rootfs.img
cd ../..
blinools sandbox create
```

### CLI reference

### `sandbox ps`

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

### `sandbox create`

Creates a sandbox and attaches to its console in the foreground. The command
blocks until the guest shuts down
(either from inside the VM, e.g. `sudo poweroff`, or via `blinools sandbox shutdown` from another terminal).

```bash
blinools sandbox create [NAME] [-s|--share <SHARE>]... [--recreate] [--delete-after-shutdown] [--read-only] [--no-hidden | --hidden-as-read-only]
```

| Argument | Default | Description |
| --- | --- | --- |
| `[NAME]` | The `name` from the config file, if omitted then the current directory name is used. If for any reason that fails too then a random name is generated. | Name of the sandbox. Must not collide with an already-running sandbox. |
| `-s, --share <SHARE>` | none | Mount a host directory into the guest. Repeatable. See [share syntax](#share-syntax) below. Merges with (and overrides, by name) the `shares` list in the config file. |
| `--recreate` | `false` | Reset the sandbox back to a clean state, wiping any changes made to the rootfs. |
| `--delete-after-shutdown` | `false` | Automatically run the equivalent of `sandbox delete --force` once the guest shuts down. |
| `--read-only` | `false` | When set, all shares are mounted read-only, overriding any other configuration. Without it, shares are read-write unless configured otherwise. |
| `--no-hidden` | `false` | When set, the hidden attribute on paths is ignored, and hidden paths are mounted as read-write. |
| `--hidden-as-read-only` | `false` | When set, the hidden attribute on paths is ignored, and hidden paths are mounted as read-only. |

The `--read-only`, `--no-hidden` and `--hidden-as-read-only` flags only change how the
shares of this run are mounted, the configuration file is left untouched. This makes it easy to
temporarily loosen or tighten what the sandbox can touch, for example to inspect a hidden path
(`--no-hidden`) or to make sure nothing can be modified (`--read-only`).

#### Share syntax

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

### `sandbox shutdown`

Asks a running sandbox to shut down gracefully. No-op if the sandbox isn't running.

```bash
blinools sandbox shutdown <NAME>
```

| Flag | Default | Description |
| --- | --- | --- |
| `-f, --force` | `false` | Force a shutdown and ignore failures. |

### `sandbox delete`

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


### `sandbox prune`

Delete all sandboxes that are not running. This command also deletes their state.

```bash
blinools sandbox prune
```

## License

The source code is distributed under the terms of the GNU General Public License v3.0 (or later).
See LICENSE for details.
