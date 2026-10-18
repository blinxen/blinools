.SH STORAGE
The rootfs image is a read-only base. Guest changes are saved in an overlay
that survives shutdown. The create command reuses it. The \fB\-\-recreate\fR
option discards it and starts from the base image. Deleting or pruning a sandbox
removes its saved state.
.PP
With systemd in the guest, shares are mounted at /mnt/NAME. Other init systems
can mount them with \fBmount \-t virtiofs NAME /mnt/NAME\fR after creating
the mount directory. Writes to shares change host files and survive sandbox
deletion. The base image and host share directories are kept when a sandbox
is deleted.
.SH NETWORKING
Restricted is the default network mode. Outgoing public internet connections
are allowed, while access to loopback, private, CGNAT, link-local and IPv6
unique-local ranges is blocked. Configured DNS servers are exceptions to that
block. Unrestricted also permits local-network destinations. None disables
networking. No incoming ports are forwarded in either enabled mode.
.SH GIT INTEGRATION
Before mounting a share, blinools scans for .git entries without following
symlinks. ReadOnly is the default action and mounts them read-only.
Hide replaces them with empty, read-only entries. None leaves them as they are.
A share's git_action overrides [sandbox].git_action.
.PP
The scan is not repeated while the sandbox is running, so new .git entries
are not covered. Whole-share and per-path restrictions still apply.
.SH SHUTDOWN
The shutdown command stops the VM through its hypervisor API. To let the guest
operating system shut down cleanly, run poweroff inside the guest.
The create command remains attached to the console until the guest exits.
