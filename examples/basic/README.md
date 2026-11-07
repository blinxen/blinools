# Basic example

This example builds a very simple sandbox based on Fedora.

## Build and run

1. Prepare your host using the [sandbox requirements](../../docs/sandbox.md#requirements).

2. Build the kernel and rootfs that the sandbox will use:

```bash
pushd examples/basic/
    ./build-kernel.sh
    ./build-rootfs.sh
popd
```

3. Start the sandbox (assuming you are in the repository root):

```bash
blinools sandbox create work
```

4. Shut the VM down gracefully:

```bash
sudo poweroff
```
