# blinools.toml

## Name

blinools.toml - configuration file for blinools

## Description

blinools reads TOML configuration from a global file and an optional project file.

## Files

- `$XDG_CONFIG_HOME/blinools/blinools.toml` is the global file. It is optional.
  If XDG_CONFIG_HOME is unset or empty, `~/.config/blinools/blinools.toml` is
  used instead.
- `./blinools.toml` is the project file. It is optional too.
  If `-c FILE` or `--config FILE` is defined then it replaces the project file.
  Defining this flag does not prevent the global file from being read.

## How the files combine

Project values override global values key by key.
A key that is missing from the project file keeps its global value,
and a key that is missing from both gets its default.
Shares follow their own rules, described under [Shares](#shares).

## The sandbox table

Everything in this section goes in `[sandbox]`.

### Paths and names

Relative paths are resolved from the current working directory, including paths in the global file.
Paths must exist, and symbolic links are resolved.
The `~` character in paths is not expanded.

Sandbox and share names must have between 1 to 36 characters.
Allow charactes are:

* ASCII letters
* digits
* hyphens (not at the beginning)
* underscores

### Options

- `name` (string): The sandbox name. By default it is derived from the current
  directory. If that fails for some reason, a random string is used.
  This option can be overriden by specifying the `NAME` when creating a sandbox
  using `sandbox create`.
- `kernel` (path, required): The Linux kernel file to boot.
- `rootfs` (path, required): The root filesystem image. It is used as a
  read-only base, and changes made in the guest are saved in an overlay.
- `hypervisor` (enum): Valid options are `"Qemu"` (the default) or `"CloudHypervisor"`.
- `rootfs_type` (enum): Valid optoins are `"Raw"` (the default) or `"QCOW2"`. Defines the format of the base image.
- `kernel_cmdline` (string): Extra kernel arguments, empty by default. Values
  containing `console=` or `root=` are rejected, because blinools adds `console=hvc0` and `root=/dev/vda` itself.
- `memory_mb` (integer, required): Guest memory in MiB. The accepted range is from 512 to 131072.
- `cpus` (integer, required): Number of guest CPUs. The accepted range is from 1 to 255.
- `network` (table): Controls what the sandbox can reach on the network. See [Network](#network).
- `dns` (array of strings): IPv4 or IPv6 addresses of DNS servers.
  In `Restricted` network mode, these addresses are exempt from the local-network block, but only on port 53.
- `git_action` (enum): How `.git` directories and worktree metadata files inside shares are mounted. Valid options are
  `"ReadOnly"` (the default) mounts them read-only,
  `"Hide"` replaces them with empty read-only entries and
  `"None"` deactivate this special handling and they are mounted like any other files.
  See blinools-sandbox(1) for limitations.
- `shares` (array of tables): Host directories to expose in the sandbox.
  If systemd is used as PID 1 then they are auto mounted at `/mnt/NAME`.
  See [Shares](#shares).
- `inherit_shares` (boolean): False by default.
  When true, global shares are included unless a project share has the same name.
  This option only matters when a project file is present.
- `sandbox_user_uid` (integer): The UID that shared host files have in the
  guest. The default is 1000. Any value from 0 to 4294967295 is accepted.
- `sandbox_user_gid` (integer): The same for the GID.

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

- `mode` (enum): Valid options are `"Restricted"` (the default), `"Unrestricted"` or `"None"`.
  `Restricted` allows outgoing connections to the public internet and blocks
  loopback, private, CGNAT, link-local and IPv6 unique-local destinations,
  except for the configured DNS servers and the defined exceptions (see the next option).
  `Unrestricted` does not block any connection (except connections to the loopback interface).
  `None` turns networking completely off.
- `exceptions` (array of tables): List of exceptions to add for outgoing connections.
  It only makes sense to define this when `mode` is set to `Restricted`.
  Each entry has these keys:
  - `ip_or_range_or_hostname` (string, required): An IPv4 or IPv6 address, a
    range in CIDR notation such as `192.168.1.0/24`, or a hostname.
    A hostname is resolved only once, when the sandbox starts.
    Every address it resolves to becomes an exception, and addresses that change later are not picked up.
    A name that can't be resolved is skipped without an error.
  - `port` (integer): Limits the exception to this destination port.
    The accepted range is from 1024 to 65535.
    Only needs to be defined if you want to limit the exception to a specific port instead of all ports.

Exceptions are applied before the blocked ranges, so they work for addresses
inside those ranges. Loopback is always blocked and can't be made an exception.

### Shares

Each `share` (entry) in the `shares` array is a table.

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

The `share` table has the following options:

- `name` (string, required): Decides the mount point `/mnt/NAME` in the guest.
  See [Paths and names](#paths-and-names) for the naming rules.
- `host_dir` (path, required): The host directory to expose.
- `read_only` (boolean): False by default. Set it to true to mount the whole share read-only.
  The `--read-only` flag overrides this setting.
- `read_only_paths` (array of paths): Paths inside `host_dir` to mount
  read-only. Each path must exist and stay inside `host_dir`
  after symlinks are resolved. Relative paths start from the working directory,
  not from `host_dir`.
  A path equal to `host_dir` is rejected, use `read_only` for that.
- `hidden_paths` (array of paths): Paths to hide from the sandbox.
  The same rules as `read_only_paths` apply.
- `git_action` (enum): Valid options are `"ReadOnly"`, `"Hide"` or `"None"`.
  Defaults to the `[sandbox.git_action]` value.

### Command line overrides

The name passed to `sandbox create` overrides the configured name.
`--read-only`, `--no-hidden` and `--hidden-as-read-only` only affect that run and never change a configuration file.
If both hidden-path flags are given, `--no-hidden` wins.
Neither of them turns off the git restrictions.

## Examples

A minimal configuration with an existing kernel and rootfs image:

```toml
[sandbox]
kernel = "/boot/vmlinuz"
rootfs = "./rootfs.img"
memory_mb = 8192
cpus = 4
```

A sandbox that can reach one service in the local network, and a build server by name:

```toml
[sandbox.network]
mode = "Restricted"
exceptions = [
  { ip_or_range_or_hostname = "192.168.1.10", port = 8080 },
  { ip_or_range_or_hostname = "build.example.internal" },
]
```

A sandbox with unrestricted network access:

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

A project file that inherits the global options and shares, and replaces the
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

blinools(1), blinools-sandbox(1), blinools-sandbox-create(1)

## Copyright

blinools is licensed under GPL-3.0-or-later. See LICENSE for the full text.
