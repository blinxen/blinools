# blinools.toml

## Name

blinools.toml - configuration file for blinools

## Description

`blinools.toml` configures blinools using TOML.
Configuration comes from an optional global file and an optional project file.

## Files

- `$XDG_CONFIG_HOME/blinools/blinools.toml` is the global file. It is optional.
  If XDG_CONFIG_HOME is unset or empty, `~/.config/blinools/blinools.toml` is
  used instead.
- `./blinools.toml` is the project file. It is optional too.
  `-c FILE` or `--config FILE` selects a different project file.
  The global file is read first. An explicitly selected project file must exist.

## How the files combine

Project values override global values key by key.
A key that is missing from the project file keeps its global value,
and a key that is missing from both gets its default.

## The sandbox table

Everything in this section goes in `[sandbox]`.
Shares have their own merge rules, described under [Shares](#shares).

### Paths and names

Relative paths are resolved from the current working directory, including paths in the global file.
Paths must exist, and symbolic links are resolved.
The `~` character in paths is not expanded.

Sandbox and share names must have between 1 and 36 characters.
Allowed characters are:

* ASCII letters
* digits
* hyphens (not at the beginning)
* underscores

### Options

- `name` (string): The sandbox name. By default it is derived from the current
  directory, with unsupported characters replaced by hyphens and the name shortened
  to 36 characters. If no valid name remains, a random string is used.
- `kernel` (path, required): The Linux kernel file to boot.
- `rootfs` (path, required): The root filesystem image. It is used as a
  read-only base, and changes to the guest root disk are saved in an overlay.
- `hypervisor` (enum): Valid options are `"Qemu"` (the default) or `"CloudHypervisor"`.
- `rootfs_type` (enum): The format of the base image: `"Raw"` (the default) or `"QCOW2"`.
- `kernel_cmdline` (string): Extra kernel arguments, empty by default. Values
  containing `console=` or `root=` are rejected, because blinools adds `console=hvc0` and `root=/dev/vda` itself.
- `memory_mb` (integer, required): Guest memory in MiB. The accepted range is from 512 to 131072.
- `cpus` (integer, required): Number of guest CPUs. The accepted range is from 1 to 255.
- `network` (table): Controls what the sandbox can reach on the network. See [Network](#network).
- `dns` (array of strings): DNS server addresses advertised to the guest. The parser accepts
  IPv4 and IPv6 addresses, but networking currently runs in IPv4 only mode, so use IPv4 servers.
  See [Network](#network) for how these addresses interact with the firewall.
- `git_action` (enum): How existing `.git` entries inside shares are exposed.
  - `"ReadOnly"` (the default): Mount the entries read-only. Working tree access follows the share's normal settings.
  - `"Hide"`: Cover the entries with empty, read-only files or directories.
  - `"None"`: Leave the entries subject to the same access settings as other shared files.

  This scan runs once at startup. For worktrees, it handles the `.git` pointer file,
  not the metadata directory it points to. See [Git integration](sandbox_architecture.md#git-integration)
  for details and limitations.
- `shares` (array of tables): Host directories to expose in the sandbox.
  With systemd as PID 1, they are mounted automatically at `/mnt/NAME`.
  With another init system, create the mount directory and run `mount -t virtiofs NAME /mnt/NAME` in the guest.
  See [Shares](#shares).
- `inherit_shares` (boolean): False by default.
  When true, global shares are included unless a project share has the same name.
  This option only matters when a project file is present.
- `sandbox_user_uid` (integer): The UID that shared host files have in the
  guest. The default is 1000. Any value from 0 to 4294967295 is accepted.
- `sandbox_user_gid` (integer): The GID shown for shared host files in the guest. The default and range are the same as for `sandbox_user_uid`.

### Custom binary paths

These keys are optional. By default, binaries are looked up in PATH.
An override must name an existing file, and a relative path is resolved from the
working directory.

- `qemu.binary`: Defaults to `qemu-system-x86_64`. Used when `hypervisor` is `"Qemu"`.
- `cloud_hypervisor.binary`: Defaults to `cloud-hypervisor`. Used when `hypervisor` is `"CloudHypervisor"`.
- `passt.binary`: Defaults to `passt`.
- `pasta.binary`: Defaults to `pasta`.
- `virtiofsd.binary`: Defaults to `virtiofsd`.

### Network

The `network` table has the following options:

- `mode` (enum):
  - `"Restricted"` (the default): Allow outgoing public internet connections. Block
    loopback, private, CGNAT and link-local destinations, with the DNS and destination
    exceptions described below.
  - `"Unrestricted"`: Allow outgoing connections to public and local network destinations.
    Host loopback and gateway mappings remain disabled.
  - `"None"`: Disable networking.

  Enabled modes currently use IPv4 only. ICMP and incoming port forwarding are disabled.
  In `Restricted` mode, each configured `dns` address gets an automatic exemption on
  destination port 53. Other ports follow the normal destination rules: public addresses
  remain reachable, and an explicit exception can allow other ports on a blocked address.
- `exceptions` (array of tables): List of exceptions to add for outgoing connections.
  These entries are used in `Restricted` mode.
  Each entry has these keys:
  - `ip_or_range_or_hostname` (string, required): An IPv4 or IPv6 address, a
    range in CIDR notation such as `192.168.1.0/24`, or a hostname.
    A hostname is resolved only once, when the sandbox starts.
    Every resolved address becomes an exception for that run.
    A name that can't be resolved is skipped without an error.
  - `port` (integer): Limits the exception to this destination port.
    The accepted range is from 1 to 65535.
    Omit `port` to allow all destination ports.

Exceptions are applied before the blocked ranges, so they work for addresses
inside those ranges. Traffic routed through the loopback interface is dropped before
exceptions are evaluated. See [Networking](sandbox_architecture.md#networking) for the rule order.

### Shares

Each entry in the `shares` array is a table.

Depending on your setup, `shares` are applied differently:

- Without a project file, the global shares are used.
- With a project file and `inherit_shares = false` (the default), only the project shares apply.
- With `inherit_shares = true`, global and project shares are combined by name.
  A project share replaces the whole global share with the same name.
- Each `--share` argument given to the `sandbox create` command replaces the whole configured
  share of the same name, including its hidden paths, read-only paths and
  `git_action`.

`inherit_shares` can be set in the global file and overridden in the project
file. With `inherit_shares = true`, `shares = []` still inherits the global
shares. Set `inherit_shares = false` to clear them.

Each share table has the following options:

- `name` (string, required): Decides the mount point `/mnt/NAME` in the guest.
  See [Paths and names](#paths-and-names) for the naming rules.
- `host_dir` (path, required): The host directory to expose.
- `read_only` (boolean): False by default. Set it to true to mount the whole share read-only.
- `read_only_paths` (array of paths): Paths inside `host_dir` to mount
  read-only. Each path must exist and stay inside `host_dir`
  after symlinks are resolved. Relative paths start from the working directory,
  not from `host_dir`.
  To make `host_dir` itself read-only, use `read_only = true`.
- `hidden_paths` (array of paths): Paths to hide from the sandbox.
  The same rules as `read_only_paths` apply.
- `git_action` (enum): Override the [sandbox default](#options) for this share.

### Command line overrides

The name passed to `sandbox create` overrides the configured name.
The following flags affect the current run:

- `--read-only`: Mount every share read-only.
- `--no-hidden`: Show the original contents of `hidden_paths`. Access still depends
  on `read_only`, `read_only_paths`, `git_action` and host permissions.
- `--hidden-as-read-only`: Show the original contents of `hidden_paths` as read-only.

If both hidden-path flags are given, `--no-hidden` wins. `git_action` applies
independently of these flags. Configuration files are left unchanged.

## Examples

A minimal configuration with an existing kernel and rootfs image:

```toml
[sandbox]
kernel = "./kernel"
rootfs = "./rootfs.img"
memory_mb = 8192
cpus = 4
```

Add the following settings to an existing sandbox configuration to reach one local
service and a build server by name:

```toml
[sandbox.network]
mode = "Restricted"
exceptions = [
  { ip_or_range_or_hostname = "192.168.1.10", port = 8080 },
  { ip_or_range_or_hostname = "build.example.internal" },
]
```

Add the following setting to an existing sandbox configuration for unrestricted network access:

```toml
[sandbox]
network = { mode = "Unrestricted" }
```

A share that allows writes but keeps `.git` read-only:

```toml
[[sandbox.shares]]
name = "project"
host_dir = "."
git_action = "ReadOnly"
```

A project file that inherits global shares and replaces the
share named `project`:

```toml
[sandbox]
inherit_shares = true

[[sandbox.shares]]
name = "project"
host_dir = "."
read_only = true
```

## See also

[blinools(1)](cli/blinools.md), [blinools-sandbox(1)](cli/blinools-sandbox.md),
[blinools-sandbox-create(1)](cli/blinools-sandbox-create.md)

## Copyright

blinools is licensed under GPL-3.0-or-later. See LICENSE for the full text.
