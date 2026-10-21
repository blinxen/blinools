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
  blinools boots without an initramfs, with the root filesystem on `/dev/vda`.
  The kernel needs built-in drivers for virtio block devices, the root filesystem
  and the virtio console. Sharing also needs virtio-fs support. A distribution kernel
  works if it includes the required drivers rather than relying on initramfs modules.

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
To stop the sandbox from the host, run this in another terminal:

```bash
blinools sandbox shutdown work
```

This stops the VM through the hypervisor API. For a clean operating-system shutdown,
run `poweroff` inside the guest instead.

Creating a stopped sandbox with the same name boots it using its saved rootfs overlay.
Memory and running processes are not restored. Pass `--recreate` to discard the overlay
and boot from the base image.

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

The scripts download and compile the Linux kernel and build a Fedora root filesystem with Podman.
The `blinools.toml` in the repository already points to these images.
Check the binary paths and the DNS setting before you use it.

| Example | Description |
| --- | --- |
| [`examples/basic`](../examples/basic) | A minimal Fedora rootfs with systemd, networking, development tools and an auto-login user. |
| [`examples/ai-agent`](../examples/ai-agent) | A Fedora rootfs with Claude Code, OpenCode and Codex. The `agent` user is unprivileged and can run only `poweroff` with sudo. Shares are automatically mounted into its home directory. |

## Commands

See the [sandbox command reference](cli/blinools-sandbox.md) for all commands and options.

## Networking

See [Network](configuration.md#network) in the configuration reference for modes,
DNS settings and destination exceptions. [Networking](sandbox_architecture.md#networking)
in the architecture guide explains the implementation.

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

## Git integration

The [configuration reference](configuration.md#options) defines the `git_action` values.
The default, `ReadOnly`, protects existing `.git` entries while leaving the working tree
writable. For scanning and worktree limitations, see
[Git integration](sandbox_architecture.md#git-integration).

A share can override the sandbox default, for example when a coding task needs to commit:

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
