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

This replaces ./blinools.toml. The global configuration is read first.

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

The rootfs image is a read-only base. Changes to the guest root disk are saved in an overlay that survives shutdown. The create command reuses it on the next boot. Memory and running processes are not restored. The **--recreate** option discards the overlay and starts from the base image. Deleting or pruning a sandbox removes its saved disk changes.

With systemd in the guest, shares are mounted at /mnt/NAME. Other init systems can mount them with **mount -t virtiofs NAME /mnt/NAME** after creating the mount directory. Writes to shares change host files and survive sandbox deletion. The base image and host share directories are kept when a sandbox is deleted.

# NETWORKING

Networking currently uses IPv4 only. Restricted is the default mode and allows outgoing public internet connections while blocking local network destinations. Configured DNS addresses get an automatic exemption on port 53. Other ports follow the normal destination rules. network.exceptions allows additional destinations and optional ports. Unrestricted also permits local network destinations. None disables networking. ICMP, incoming port forwarding, and host loopback and gateway mappings are disabled in both enabled modes. See [blinools.toml(5)](../configuration.md) for the mode and exception settings.

# GIT INTEGRATION

Before serving a share, blinools scans for .git entries without following symlinks. ReadOnly is the default action and mounts them read-only, leaving the working tree writable. Hide covers them with empty, read-only entries. None skips this step. See [blinools.toml(5)](../configuration.md) for git_action and per-share settings.

The scan runs once at startup. Later .git entries use the share's normal access settings. A worktree's .git pointer file is handled, but its target metadata directory is not followed. A target outside the shares may be unavailable while a target exposed by another share uses that share's access settings.

# SHUTDOWN

The shutdown command stops the VM through its hypervisor API. To let the guest operating system shut down cleanly, run poweroff inside the guest. The create command remains attached to the console until the guest exits.

# SEE ALSO

[blinools(1)](blinools.md), [blinools-sandbox-create(1)](blinools-sandbox-create.md), [blinools-sandbox-delete(1)](blinools-sandbox-delete.md), [blinools-sandbox-prune(1)](blinools-sandbox-prune.md), [blinools-sandbox-ps(1)](blinools-sandbox-ps.md), [blinools-sandbox-shutdown(1)](blinools-sandbox-shutdown.md), [blinools.toml(5)](../configuration.md)
