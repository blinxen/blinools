# NAME

blinools-sandbox - Create and manage Linux microVM sandboxes

# SYNOPSIS

**blinools sandbox** \[OPTIONS\] \<COMMAND\>

# DESCRIPTION

Create and manage Linux microVM sandboxes.

All sandbox commands require an interactive terminal on standard input. To create a sandbox, set kernel, rootfs, memory_mb and cpus in the configuration.

# OPTIONS

**-c**, **--config** *\<FILE\>*  
Use FILE as the project configuration.

This replaces ./blinools.toml. The global configuration still applies.

**-v**, **--verbose**  
Increase logging verbosity

**-q**, **--quiet**  
Decrease logging verbosity

**-h**, **--help**  
Print help (see a summary with '-h')

# SUBCOMMANDS

[blinools-sandbox-ps(1)](blinools-sandbox-ps.md)  
List all sandboxes and their state

[blinools-sandbox-create(1)](blinools-sandbox-create.md)  
Create and start a sandbox

[blinools-sandbox-shutdown(1)](blinools-sandbox-shutdown.md)  
Stop a sandbox through its hypervisor

[blinools-sandbox-delete(1)](blinools-sandbox-delete.md)  
Delete a sandbox and all of its saved rootfs changes

[blinools-sandbox-prune(1)](blinools-sandbox-prune.md)  
Delete all stopped sandboxes and their saved rootfs changes

# EXAMPLES

      blinools sandbox ps
      blinools sandbox create work
      blinools sandbox shutdown work

# STORAGE

The rootfs image is a read-only base. Guest changes are saved in an overlay that survives shutdown. The create command reuses it. The **--recreate** option discards it and starts from the base image. Deleting or pruning a sandbox removes its saved state.

With systemd in the guest, shares are mounted at /mnt/NAME. Other init systems can mount them with **mount -t virtiofs NAME /mnt/NAME** after creating the mount directory. Writes to shares change host files and survive sandbox deletion. The base image and host share directories are kept when a sandbox is deleted.

# NETWORKING

Restricted is the default network mode. Outgoing public internet connections are allowed, while access to loopback, private, CGNAT, link-local and IPv6 unique-local ranges is blocked. Configured DNS servers are exceptions to that block. Unrestricted also permits local-network destinations. None disables networking. No incoming ports are forwarded in either enabled mode.

# GIT INTEGRATION

Before mounting a share, blinools scans for .git entries without following symlinks. ReadOnly is the default action and mounts them read-only. Hide replaces them with empty, read-only entries. None leaves them as they are. A share's git_action overrides \[sandbox\].git_action.

The scan is not repeated while the sandbox is running, so new .git entries are not covered. Whole-share and per-path restrictions still apply.

# SHUTDOWN

The shutdown command stops the VM through its hypervisor API. To let the guest operating system shut down cleanly, run poweroff inside the guest. The create command remains attached to the console until the guest exits.

# SEE ALSO

[blinools(1)](blinools.md), [blinools-sandbox-create(1)](blinools-sandbox-create.md), [blinools-sandbox-delete(1)](blinools-sandbox-delete.md), [blinools-sandbox-prune(1)](blinools-sandbox-prune.md), [blinools-sandbox-ps(1)](blinools-sandbox-ps.md), [blinools-sandbox-shutdown(1)](blinools-sandbox-shutdown.md), [blinools.toml(5)](../configuration.md)
