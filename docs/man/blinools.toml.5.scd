blinools.toml(5) "blinools" "blinools manual"

# NAME

blinools.toml - configuration for blinools sandboxes

# DESCRIPTION

Sandbox commands read TOML configuration. Options belong in *[sandbox]*.
Unknown keys and invalid values cause an error. Only sandbox create requires
this table, but all sandbox commands validate any configuration files they find.

# FILES

*$XDG_CONFIG_HOME/blinools/blinools.toml*
	This optional file contains global configuration. If XDG_CONFIG_HOME is
	unset or empty, blinools uses ~/.config/blinools/blinools.toml.

*./blinools.toml*
	This optional file contains project configuration in the current working
	directory. Parent directories are not searched.

Use *-c FILE* or *--config FILE* to select a different project file. That file
must exist. The global configuration still applies.

# PRECEDENCE

Project values override global values per key. Omitted keys keep their global
values. Defaults apply to keys absent from both files.

The *shares* option follows these rules.

- Without a project file, use global shares.
- With a project file and *inherit_shares = false* (the default), only project
  shares apply. If the project file omits shares, the list is empty.
- With *inherit_shares = true*, combine global and project shares by name.
  A project share replaces the entire global share with that name.
- Each *--share* argument to *sandbox create* replaces the entire configured
  share of the same name, including its hidden paths, read-only paths and
  git_action.

*inherit_shares* can be set globally and overridden in the project file.
With inherit_shares = true, *shares = []* still inherits global shares.
Set inherit_shares = false to clear them.

The name passed to *sandbox create* overrides the configured name.
*--read-only*, *--no-hidden* and *--hidden-as-read-only* affect only that run.
They do not modify configuration files. If both hidden-path flags are supplied,
*--no-hidden* takes precedence. Neither disables git restrictions.

# PATHS AND NAMES

Relative kernel, rootfs and share paths are resolved from the current working
directory, including paths in the global file. The config file's directory and
host_dir are not used as a base. Paths must exist. blinools resolves symbolic links.
TOML path strings do not expand ~ or environment variables.

Sandbox names and share names contain 1 to 36 ASCII letters, digits, hyphens or
underscores, and must not start with a hyphen. Automatically derived names
replace other characters with hyphens, truncate to 36 characters and remove
leading and trailing hyphens. If no usable name can be derived, a random
36-character alphanumeric name is generated.

# SANDBOX OPTIONS

The following options belong in *[sandbox]*.

*name* (string)
	The sandbox name defaults to a name derived from the current directory,
	or to a random name when that fails. The name passed to sandbox create
	takes precedence.

*kernel* (path, required)
	The sandbox boots this Linux kernel file.

*rootfs* (path, required)
	The sandbox uses this root filesystem image as a read-only base.
	blinools saves guest changes in an overlay.

*hypervisor* (string)
	This selects the virtual machine monitor. Valid values are *"Qemu"* and
	*"CloudHypervisor"*. The default is *"Qemu"*.

*rootfs_type* (string)
	This sets the base rootfs image format to *"Raw"* or *"QCOW2"*.
	The default is *"Raw"*.

*kernel_cmdline* (string)
	This supplies additional kernel arguments and defaults to an empty string.
	Values containing *console=* or *root=* are rejected. blinools supplies
	console=hvc0 and root=/dev/vda itself.

*memory_mb* (integer, required)
	This sets the guest's memory in MiB. The value must be between 512 and
	131072 inclusive.

*cpus* (integer, required)
	This sets the guest's CPU count. The value must be between 1 and 255 inclusive.

*network* (string)
	The network mode can be *"Restricted"*, *"Unrestricted"* or *"None"*.
	The default is *"Restricted"*. Restricted permits outgoing public internet
	access and blocks loopback, private, CGNAT, link-local and IPv6 unique-local
	destinations, except configured DNS servers. Unrestricted also permits
	local-network destinations. None disables networking. No incoming ports
	are forwarded in either enabled mode.

*dns* (array of strings)
	This lists the DNS server IP addresses passed to passt. Each entry must be
	an IPv4 or IPv6 address. By default, no explicit server list is passed.
	In Restricted mode, listed addresses are exempt from the local-network
	destination block.

*git_action* (string)
	This controls how .git directories and worktree metadata files inside shares
	are mounted. Valid values are *"ReadOnly"*, *"Hide"* and *"None"*.
	The default is *"ReadOnly"*. ReadOnly mounts those entries read-only.
	Hide replaces them with empty read-only entries. None applies no special
	git handling. See blinools-sandbox(1) for limitations.

*shares* (array of tables)
	This lists the host directories to mount at /mnt/NAME with systemd in the
	guest. The list is empty by default, subject to the inheritance rules above.
	See SHARE OPTIONS for the available settings.

*inherit_shares* (boolean)
	The default is false. When this is true, global shares are included unless
	a project share replaces them by name. This setting applies only when a
	project file is present.

*sandbox_user_uid* (integer)
	This sets the UID that shared host files have inside the guest. The default
	is 1000. The value must be between 0 and 4294967295 inclusive.

*sandbox_user_gid* (integer)
	This sets the GID that shared host files have inside the guest. The default
	is 1000. The value must be between 0 and 4294967295 inclusive.

Enum values such as "Qemu", "QCOW2" and "ReadOnly" are case-sensitive.

## Helper binaries

These optional keys belong in *[sandbox]*. By default, helper programs are
found through PATH. Overrides must name existing files, with relative paths
resolved from the working directory.

*qemu.binary* (path)
	The default executable is *qemu-system-x86_64*. blinools uses it when
	hypervisor = "Qemu".

*cloud_hypervisor.binary* (path)
	The default executable is *cloud-hypervisor*. blinools uses it when
	hypervisor = "CloudHypervisor".

*passt.binary* (path)
	The default executable is *passt*. blinools uses it when networking is enabled.

*pasta.binary* (path)
	The default executable is *pasta*. blinools uses it when networking is enabled.

*virtiofsd.binary* (path)
	The default executable is *virtiofsd*. blinools uses it for directory shares.

# SHARE OPTIONS

Each entry in *shares* is a table. The *[[sandbox.shares]]* TOML syntax is also
supported.

*name* (string, required)
	This name determines the guest mount point /mnt/NAME. See PATHS AND NAMES
	for the naming rules.

*host_dir* (path, required)
	This host directory is exposed to the guest and must already exist.

*read_only* (boolean)
	The default is false. Set this to true to mount the whole share read-only.
	The *--read-only* CLI flag forces this for every share.

*read_only_paths* (array of paths)
	This lists paths inside host_dir to mount read-only. The default is an
	empty list. Paths must exist and remain inside host_dir after resolving
	symlinks. Relative paths start from the working directory, not host_dir.
	A path equal to host_dir is rejected. Use read_only to make the whole
	share read-only.

*hidden_paths* (array of paths)
	This lists paths to replace with empty, read-only files or directories.
	The default is an empty list. The same path rules as read_only_paths apply.

*git_action* (string)
	Valid values are "ReadOnly", "Hide" and "None".
	The default is [sandbox].git_action. Whole-share and per-path restrictions
	still apply.

# EXAMPLES

This minimal configuration uses an existing kernel and rootfs image.

```
[sandbox]
kernel = "/boot/vmlinuz"
rootfs = "./rootfs.img"
memory_mb = 8192
cpus = 4
```

The following share allows writes while keeping .git read-only.

```
[[sandbox.shares]]
name = "project"
host_dir = "."
git_action = "ReadOnly"
```

This project configuration inherits global options and shares. It replaces the
share named *project*.

```
[sandbox]
inherit_shares = true

[[sandbox.shares]]
name = "project"
host_dir = "."
read_only = true
```

# SEE ALSO

blinools(1), blinools-sandbox(1), blinools-sandbox-create(1)

# COPYRIGHT

blinools is licensed under GPL-3.0-or-later. See LICENSE for the full text.
