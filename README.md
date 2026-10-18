# blinxen's tools

A collection of my various bash scripts converted into a Rust CLI (Linux only).

## Documentation

The command-line interface is documented in [blinools(1)](docs/man/blinools.1).
The configuration file is documented in [blinools.toml(5)](docs/man/blinools.toml.5).
The [architecture guide](docs/sandbox_architecture.md) describes how sandboxes work.

## Installation

### Prebuilt binaries

Tagged releases are built for `x86_64-unknown-linux-gnu` and `aarch64-unknown-linux-gnu`.
Grab the archive for your architecture from the [Releases](../../releases) page,
then verify and unpack it.

```bash
sha256sum -c SHA256SUMS
tar xzf blinools-<version>-<target>.tar.gz
```

### Build from source

```bash
git clone https://github.com/blinxen/blinools.git
cd blinools
cargo install --path .
# or
just install
```

This installs the `blinools` binary to `~/.cargo/bin`.

## Examples

The [examples](./examples) directory contains rootfs definitions you can build with `podman` or `docker`.

| Example | Description |
| --- | --- |
| [`examples/basic`](./examples/basic) | This provides a minimal Fedora 45 rootfs with `systemd`, networking, development tools and an auto-login user. |
| [`examples/ai-agent`](./examples/ai-agent) | This provides a Fedora 45 rootfs for isolating AI coding agents. It includes Claude Code, OpenCode and Codex, runs as an `agent` user with the only sudo permission's to run `poweroff`, and auto mounts shares into the agent's home directory. |

Each example has a `build-kernel.sh` and a `build-rootfs.sh` script, shared from
[`misc`](./misc). They produce `kernel` and `rootfs.img` in the example directory.

```bash
cd examples/ai-agent
./build-kernel.sh
./build-rootfs.sh
```

## License

The source code is distributed under the terms of the GNU General Public License v3.0 or later.
See LICENSE for details.
