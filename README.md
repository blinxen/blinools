# blinxen's tools

A collection of my various bash scripts converted into a Rust cli.

## Sandboxes

*This is still under heavy development and still contains rough edges*

The `sandbox` subcommand was initially created for isolating AI harnesses.
However, it can be used for anything.

### Requirements

* [Cloud Hypervisor](https://github.com/cloud-hypervisor/cloud-hypervisor) is used for creating microVMs.
* [passt](https://passt.top) is used to enable networking.
* [virtiofsd](https://gitlab.com/virtio-fs/virtiofsd) is used for sharing directories with the microVM.

Fedora:

```bash
# virtiofsd is installed under /usr/libexec by default, I recommend configuring "virtiofsd.binary" in the config file.
sudo dnf install passt virtiofsd
```

Cloud Hypervisor is not packaged in most ditrobutions, you can either
[download a pre-built binary](https://www.cloudhypervisor.org/docs/prologue/quick-start/#use-pre-built-binaries)
or
[build it from source](https://www.cloudhypervisor.org/docs/prologue/quick-start/#building-from-source).

### Configuration reference

The configuration file uses the TOML format and can be configured using:

* a global configuration file located at `$XDG_CONFIG_HOME/blinools/blinools.toml` or `$HOME/.config/blinools/blinools.toml` if `$XDG_CONFIG_HOME` is not defined
* a `-c` or `--config` which defaults to `./blinools.toml`

The order in which they are loaded is global -> `--config` flag. The flag overwrites whatever was defined in the global configuration file.

All optional fields have a comment that starts with "Optional".

```toml
[sandbox]
# Optional custom paths to the binary files
cloud_hypervisor.cloud_hypervisor_binary = "/path/to/cloud-hypervisor"
passt.binary = "/path/to/passt"
virtiofsd.binary = "/path/to/virtiofsd"
# Path to the kernel
kernel = "/boot/vmlinuz-7.1.10-200.fc44.x86_64"
# Kernel command line parameters to pass
# "console" and "root" must not be configured here
# They are hardcoded to "console=hvc0 root=/dev/vda" for now
kernel_cmdline = "rw quiet"
# Path to the rootfs
rootfs = "./rootfs.img"
# How much memory the VM should have in megabytes
memory_mb = 8192
# How many cores the VM should have
cpus = 4
# Optional list of DNS servers to use in the VM
dns = ["192.168.1.1"]
# Optional paths to automatically mount under /mnt when the VM is started
# Can also be defined with the --share flag, see blinools sandbox create --help
shares = [
    { name = "share-name", host_dir = "/path/to/a/directory", read_only = false },
]
```

### How to use it

To create a sandbox, you will need a compiled Linux kernel and a rootfs.
You don't *have* to actually compile your own kernel, you can just use whatever
your Distro provides. The example configuration below uses the official Fedora 44 kernel.
The rootfs can also be easily created using `podman` (or `docker`).
Checkout the [examples](./example).

The next steps assume you have a already compiled Linux kernel and a already built rootfs.

1. Create a configuration file

```toml
[sandbox]
kernel = "/boot/vmlinuz-7.1.10-200.fc44.x86_64"
kernel_cmdline = "rw quiet"
rootfs = "./rootfs.img"
memory_mb = 8192
cpus = 4
dns = ["192.168.1.1"]
```

2. Create the sandbox

```bash
blinools sandbox create
```

License
-------

The source code is primarily distributed under the terms of the MIT License.
See LICENSE for details.
