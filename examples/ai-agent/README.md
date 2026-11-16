# AI agent example

This example builds an AI sandbox with Claude Code, Codex, and OpenCode already installed.
It also includes development tools and a dedicated user that should be used for the agent.

## Build and run

```bash
cd examples/ai-agent
./build-rootfs.sh
./build-kernel.sh
blinools --config blinool.toml sandbox create ai-agent
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

The `ai-agent` sandbox keeps its rootfs overlay between runs. After rebuilding the
base image, add `--recreate` to discard the old overlay and boot the rebuilt image.

## API keys

Keep API keys on your host and share them with the sandbox when needed.
The example rootfs automatically loads them from shell scripts in an `api-keys` share.

The next couple of steps showcase how this could be done.

1. Create a readable shell script with a `.sh` extension that exports the API keys as environment variables:

```sh
export YOUR_API_KEY='your-api-key'
```

2. From `examples/ai-agent`, start the sandbox with the directory containing your shell scripts with the share name `api-keys`:

```bash
blinools --config blinool.toml sandbox create ai-agent --share "api-keys:<PATH-TO-API-KEYS>:ro"
```

The sandbox loads the shell scripts when it opens a bash session.

3. After changing keys, open a new shell or reload them in your current sandbox shell:

```sh
. /etc/profile.d/agent-api-keys.sh
```

## Client settings

The `files/` directory contains the configuration files used by the various harnesses.
