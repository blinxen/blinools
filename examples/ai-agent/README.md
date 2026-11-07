# AI agent example

This example builds a AI sandbox with Claude Code, Codex, and OpenCode already installed.
It also includes development tools and a dedicated user that should be used for the agent.

## Build and run

1. Prepare your host using the [sandbox requirements](../../docs/sandbox.md#requirements).

2. Build the kernel and rootfs that the sandbox will use:

```bash
pushd examples/ai-agent/
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

## API keys

A sandbox should always be treated as an ephemeral environment.
Keep API keys on your host and share them with the sandbox when needed.
The example rootfs automatically loads them from shell scripts in an `api-keys` share.

The next couple of steps showcase how this could be done.

1. Create a shell script that export the API keys as environment variables:

```sh
export YOUR_API_KEY='your-api-key'
```

2. Start the sandbox with the directory containing your shell scripts with the share name `api-keys`:

```bash
blinools sandbox create work --share "api-keys:<PATH-TO-API-KEYS>:ro"
```

The sandbox loads the shell scripts when it opens a bash session.

3. After changing keys, open a new shell or reload them in your current sandbox shell:

```sh
. /etc/profile.d/agent-api-keys.sh
```

## Client settings

The `files/` directory contains the configuration files used by the various harnesses.
