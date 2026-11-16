# Basic example

This example builds a very simple sandbox based on Fedora.

## Build and run

```bash
cd examples/basic
./build-rootfs.sh
./build-kernel.sh
blinools --config blinool.toml sandbox create basic
```

Start from the repository root. The included [blinool.toml](blinool.toml) uses
`kernel` and `rootfs.img` in the example directory. Adjust its DNS server and
virtiofsd path for your host before starting the sandbox.

The host needs the [sandbox requirements](../../docs/sandbox.md#requirements).
This example builds an x86_64 kernel and rootfs. Building also needs Bash,
`curl`, `tar`, `xz`, a kernel build toolchain, a working rootless Podman setup,
and `mkfs.ext4` from e2fsprogs.

Shut the VM down gracefully from inside the guest:

```bash
sudo poweroff
```

The `basic` sandbox keeps its rootfs overlay between runs. After rebuilding the
base image, add `--recreate` to discard the old overlay and boot the rebuilt image.
