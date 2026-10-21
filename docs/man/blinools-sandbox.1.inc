.SH STORAGE
The rootfs image is a read-only base. Changes to the guest root disk are saved
in an overlay that survives shutdown. The create command reuses it on the next
boot. Memory and running processes are not restored. The \fB\-\-recreate\fR option
discards the overlay and starts from the base image. Deleting or pruning a sandbox
removes its saved disk changes.
.PP
With systemd in the guest, shares are mounted at /mnt/NAME. Other init systems
can mount them with \fBmount \-t virtiofs NAME /mnt/NAME\fR after creating
the mount directory. Writes to shares change host files and survive sandbox
deletion. The base image and host share directories are kept when a sandbox
is deleted.
.SH NETWORKING
Restricted is the default mode and allows
outgoing public internet connections while blocking local network destinations.
Configured DNS addresses get an automatic exemption on port 53. Other ports
follow the normal destination rules. network.exceptions allows additional
destinations and optional ports. Unrestricted also permits local network destinations.
None disables networking. ICMP, incoming port forwarding, and host loopback and
gateway mappings are disabled in both enabled modes. See \fBblinools.toml\fR(5)
for the mode and exception settings.
.SH GIT INTEGRATION
Before serving a share, blinools scans for .git entries without following
symlinks. ReadOnly is the default action and mounts them read-only, leaving
the working tree writable. Hide covers them with empty, read-only entries.
None skips this step. See \fBblinools.toml\fR(5) for git_action and per-share settings.
.PP
The scan runs once at startup. Later .git entries use the share's normal access
settings. A worktree's .git pointer file is handled, but its target metadata
directory is not followed. A target outside the shares may be unavailable while
a target exposed by another share uses that share's access settings.
.SH SHUTDOWN
The shutdown command stops the VM through its hypervisor API. To let the guest
operating system shut down cleanly, run poweroff inside the guest.
The create command remains attached to the console until the guest exits.
