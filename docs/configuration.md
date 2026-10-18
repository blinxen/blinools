# blinools.toml

## Name

blinools.toml - configuration file for blinools

## Description

blinools reads TOML configuration from a global file and an optional project
file. Settings are grouped in tables. The only table at the moment is
`[sandbox]`, which the sandbox commands use. Other commands don't read
configuration files.

`sandbox create` needs the `[sandbox]` table. The other sandbox commands don't,
but they still check any configuration file they find. Unknown keys and invalid
values are an error.

## Files

- `$XDG_CONFIG_HOME/blinools/blinools.toml` is the global file. It is optional.
  If XDG_CONFIG_HOME is unset or empty, `~/.config/blinools/blinools.toml` is
  used instead.
- `./blinools.toml` is the project file. It is optional too. Only the current
  directory is checked, parent directories are not.

`-c FILE` or `--config FILE` replaces the project file. FILE must exist. The
global file is still read.

## How the files combine

Project values override global values key by key. A key that is missing from
the project file keeps its global value, and a key that is missing from both
gets its default. Shares follow their own rules, described under
[Shares](#shares).

## The sandbox table

Everything in this section goes in `[sandbox]`.

### Paths and names

Relative kernel, rootfs and share paths are resolved from the current working
directory, including paths in the global file. The directory of the config file
and `host_dir` are not used as a base. Paths must exist, and symbolic links are
resolved. TOML path strings don't expand `~` or environment variables.

Sandbox and share names are 1 to 36 characters long: ASCII letters, digits,
hyphens and underscores, and they can't start with a hyphen. When a name is
derived automatically, other characters become hyphens, the result is cut to 36
characters and leading and trailing hyphens are removed. If nothing usable is
left, a random 36-character alphanumeric name is used.

### Options

- `name` (string): The sandbox name. By default it is derived from the current
  directory, or random if that fails. The name given to `sandbox create` wins.
- `kernel` (path, required): The Linux kernel file to boot.
- `rootfs` (path, required): The root filesystem image. It is used as a
  read-only base, and changes made in the guest are saved in an overlay.
- `hypervisor` (string): `"Qemu"` (the default) or `"CloudHypervisor"`.
- `rootfs_type` (string): The format of the base image, `"Raw"` (the default)
  or `"QCOW2"`.
- `kernel_cmdline` (string): Extra kernel arguments, empty by default. Values
  containing `console=` or `root=` are rejected, because blinools adds
  `console=hvc0` and `root=/dev/vda` itself.
- `memory_mb` (integer, required): Guest memory in MiB, from 512 to 131072.
- `cpus` (integer, required): Number of guest CPUs, from 1 to 255.
- `network` (string): `"Restricted"` (the default), `"Unrestricted"` or
  `"None"`. Restricted allows outgoing connections to the public internet and
  blocks loopback, private, CGNAT, link-local and IPv6 unique-local
  destinations, except for the configured DNS servers. Unrestricted also
  allows local-network destinations. None turns networking off. No incoming
  ports are forwarded in the first two modes.
- `dns` (array of strings): IPv4 or IPv6 addresses of DNS servers, passed to
  passt. No explicit list is passed by default. In Restricted mode these
  addresses are exempt from the local-network block.
- `git_action` (string): How `.git` directories and worktree metadata files
  inside shares are mounted. `"ReadOnly"` (the default) mounts them read-only,
  `"Hide"` replaces them with empty read-only entries and `"None"` leaves them
  alone. See blinools-sandbox(1) for limitations.
- `shares` (array of tables): Host directories to mount at `/mnt/NAME` when the
  guest uses systemd. Empty by default, subject to the rules in
  [Shares](#shares).
- `inherit_shares` (boolean): False by default. When true, global shares are
  included unless a project share has the same name. It only matters when a
  project file is present.
- `sandbox_user_uid` (integer): The UID that shared host files have in the
  guest. The default is 1000. Any value from 0 to 4294967295 is accepted.
- `sandbox_user_gid` (integer): The same for the GID.

Enum values such as `"Qemu"`, `"QCOW2"` and `"ReadOnly"` are case-sensitive.

### Helper programs

These keys are optional. By default the helper programs are looked up in PATH.
An override must name an existing file, and a relative path is resolved from the
working directory.

- `qemu.binary`: defaults to `qemu-system-x86_64`. Used when `hypervisor` is `"Qemu"`.
- `cloud_hypervisor.binary`: defaults to `cloud-hypervisor`. Used when
  `hypervisor` is `"CloudHypervisor"`.
- `passt.binary`: defaults to `passt`. Used when networking is enabled.
- `pasta.binary`: defaults to `pasta`. Used when networking is enabled.
- `virtiofsd.binary`: defaults to `virtiofsd`. Used for directory shares.

### Shares

Each entry in `shares` is a table. The `[[sandbox.shares]]` syntax works as
well.

Which shares apply:

- Without a project file, the global shares are used.
- With a project file and `inherit_shares = false` (the default), only the
  project shares apply. If the project file has no shares, the list is empty.
- With `inherit_shares = true`, global and project shares are combined by name.
  A project share replaces the whole global share with the same name.
- Each `--share` argument to `sandbox create` replaces the whole configured
  share of the same name, including its hidden paths, read-only paths and
  `git_action`.

`inherit_shares` can be set in the global file and overridden in the project
file. With `inherit_shares = true`, `shares = []` still inherits the global
shares. Set `inherit_shares = false` to clear them.

Share options:

- `name` (string, required): Decides the mount point `/mnt/NAME` in the guest.
  See [Paths and names](#paths-and-names) for the naming rules.
- `host_dir` (path, required): The host directory to expose. It must exist.
- `read_only` (boolean): False by default. Set it to true to mount the whole
  share read-only. The `--read-only` flag forces this for every share.
- `read_only_paths` (array of paths): Paths inside `host_dir` to mount
  read-only, none by default. Each path must exist and stay inside `host_dir`
  after symlinks are resolved. Relative paths start from the working directory,
  not from `host_dir`. A path equal to `host_dir` is rejected, use `read_only`
  for that.
- `hidden_paths` (array of paths): Paths to replace with empty read-only files
  or directories, none by default. The same rules as `read_only_paths` apply.
- `git_action` (string): `"ReadOnly"`, `"Hide"` or `"None"`. Defaults to the
  `[sandbox]` value. Whole-share and per-path restrictions still apply.

### Command line overrides

The name passed to `sandbox create` overrides the configured name. `--read-only`,
`--no-hidden` and `--hidden-as-read-only` only affect that run and never change
a configuration file. If both hidden-path flags are given, `--no-hidden` wins.
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
