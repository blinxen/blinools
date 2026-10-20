# blinools sandbox

> [!WARNING]
> This is under heavy development and still has rough edges.

`blinools sandbox` creates and manages microVMs. It was written to isolate AI coding agents, but it works
for anything you want to run in a throwaway VM. [sandbox_architecture.md](sandbox_architecture.md)
explains how it is built.

`sandbox create` needs a `[sandbox]` table in your configuration,
see [configuration.md](configuration.md).

## Requirements

- Linux with unprivileged user namespaces and hardware virtualization (KVM). Your user needs read and
  write access to `/dev/kvm`. Most distributions have a `kvm` group for that.
- A VMM, either [QEMU](https://www.qemu.org/) (`qemu-system-x86_64`) or
  [Cloud Hypervisor](https://www.cloudhypervisor.org/docs/prologue/quick-start/). Cloud Hypervisor is
  not packaged by most distributions, its guide covers prebuilt binaries and building from source.
- [virtiofsd](https://gitlab.com/virtio-fs/virtiofsd), when you share host directories.
- [passt](https://passt.top) and `pasta`, when networking is enabled. Both come in the `passt` package of most distributions.
  For `network.mode = "None"`, this is not needed.
- `nft` from [nftables](https://netfilter.org/projects/nftables/), for the default `Restricted` network mode.
  For `network.mode = "Unrestricted"` and `"None"`, this is not needed.
- A Linux kernel and a root filesystem image that suit the hypervisor.
  A distribution kernel also works, you don't have to compile your own.

On Fedora, for QEMU:

```bash
sudo dnf install qemu-system-x86 passt virtiofsd nftables
```

Fedora installs `virtiofsd` in `/usr/libexec`.
If it isn't in your `PATH`, set `virtiofsd.binary = "/usr/libexec/virtiofsd"` in `[sandbox]`.

## Getting started with your own images

Create a `blinools.toml` in your project directory:

```toml
[sandbox]
kernel = "/path/to/kernel"
rootfs = "/path/to/rootfs.img"
memory_mb = 8192
cpus = 4
```

Then start a sandbox:

```bash
blinools sandbox create work
```

The command stays attached to the sandbox console until the sandbox shuts down.
To shutdown the sandbox from the outside run this from another terminal:

```bash
blinools sandbox shutdown work
```

Shutting down the sandbox from the outside goes through the hypervisor and doesn't guarantee a clean shutdown of the sandbox.
Therefore it is always advised to shutdown the sandbox from the inside using `poweroff` or a similar command.
A stopped sandbox with the same name is started again with its saved state.
If you want a clean start then pass `--recreate` to the `create` command.

A sandbox name has to be unique among the running sandboxes.
`create` fails if a sandbox with that name is already running.

## Build and run the AI agent example

The build scripts target x86-64. You need Podman, the ext4 tools (`mkfs.ext4`) and the kernel build dependencies.
On Fedora:

```bash
sudo dnf install podman e2fsprogs curl tar xz make gcc flex bison bc openssl-devel elfutils-libelf-devel perl diffutils
```

From a checkout of the repository:

```bash
cd examples/ai-agent
./build-kernel.sh
./build-rootfs.sh
cd ../..
blinools sandbox create work
```

The scripts download and compile the linux kernel and build a Fedora root filesystem with Podman.
The `blinools.toml` in the repository already points to these images.
Check the binary paths and the DNS setting before you use it.

| Example | Description |
| --- | --- |
| [`examples/basic`](../examples/basic) | A minimal Fedora rootfs with systemd, networking, development tools and an auto-login user. |
| [`examples/ai-agent`](../examples/ai-agent) | A Fedora rootfs with Claude Code, OpenCode and Codex. The `agent` user is a unprivileged user may only run `poweroff` with sudo. Shares are automatically mounted into its home directory. |

## Commands

Checkout [blinools-sandbox.md](cli/blinools-sandbox.md) for a complete documentation on all sandbox specific commands.

## Networking

The `network` option and what each mode allows are described in [sandbox_architecture.md](sandbox_architecture.md#networking).
The option itself is listed under [Network](configuration.md#network) in the configuration reference.

If the sandbox only needs a few local IPs, then use the `Restricted` mode and add them as exceptions
instead of switching to `Unrestricted`. An exception is an address, a range or a hostname, and can be
limited to a port:

```toml
[sandbox.network]
mode = "Restricted"

# A service on the local network
[[sandbox.network.exceptions]]
ip_or_range_or_hostname = "192.168.1.10"
port = 8080

# A whole range, for example a tailnet
[[sandbox.network.exceptions]]
ip_or_range_or_hostname = "100.64.0.0/10"
```

DNS servers from the `dns` option are always limited to port 53 and don't need an exception.

## Git integration

How `.git` directories and worktree files inside shares are handled is described in
[sandbox_architecture.md](sandbox_architecture.md#git-integration).
The `git_action` setting is listed under [Options](configuration.md#options)
and [Shares](configuration.md#shares) in the configuration reference.

Which action to pick:

- `ReadOnly` (the default) when the sandbox should see the repository but not change it.
- `Hide` when it should only see the working tree and not the history.
- `None` when it needs full access to the repository, for example to commit.

`git_action` in `[sandbox]` is the default for all shares, and a share can override it:

```toml
[sandbox]
git_action = "ReadOnly"
shares = [
    # Uses the default, ReadOnly
    { name = "project", host_dir = "/home/me/project" },
    # Needs to commit, so no special handling
    { name = "scratch", host_dir = "/home/me/scratch", git_action = "None" },
    # Only the working tree is visible
    { name = "docs", host_dir = "/home/me/docs", git_action = "Hide" },
]
```
