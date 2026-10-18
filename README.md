# blinxen's tools

A collection of my various bash scripts converted into a Rust CLI (Linux only).

## Commands

| Command | Description |
| --- | --- |
| `blinools sandbox` | Create and manage microVM sandboxes. See [docs/sandbox.md](docs/sandbox.md). |
| `blinools completions` | Generate shell completion scripts. |
| `blinools wip-pr` | Not implemented yet. |

Run `blinools --help` or `blinools <command> --help` for the options of each command.

## Installation

### Prebuilt binaries

Tagged releases are built for `x86_64-unknown-linux-gnu` and `aarch64-unknown-linux-gnu`.
Download the archive for your architecture and `SHA256SUMS` from [Releases](../../releases),
then verify and unpack them:

```bash
sha256sum -c SHA256SUMS
tar xzf blinools-<version>-<target>.tar.gz
```

### Build from source

You need a Rust toolchain that supports edition 2024 (Rust 1.85 or newer).

```bash
git clone https://github.com/blinxen/blinools.git
cd blinools
cargo install --path .
```

Cargo installs the binary to `~/.cargo/bin` and does not install the man pages.
To install both, use `just`. This also needs `pandoc`, because one of the man pages is generated from
[docs/configuration.md](docs/configuration.md):

```bash
PREFIX="$HOME/.local" just install
```

This installs to `~/.local/bin` and `~/.local/share/man`. Without `PREFIX`, `just install` uses
`/usr/local`, which usually needs administrator rights. `BINDIR`, `MANDIR` and `DESTDIR` are
supported as well. Make sure the binary directory is in your `PATH`.

To read the man pages without installing them, run `just man` and then, for example,
`man -l docs/man/generated/blinools-sandbox-create.1`.

### Shell completions

To enable completions in new shells, run the matching command once:

```bash
# Bash
echo 'source <(blinools completions bash)' >> ~/.bashrc

# Zsh
echo 'source <(blinools completions zsh)' >> ~/.zshrc

# Fish
echo 'blinools completions fish | source' >> ~/.config/fish/config.fish
```

`bash`, `elvish`, `fish`, `powershell` and `zsh` are supported. Open a new shell afterwards.

## Configuration

blinools reads a global file, `$XDG_CONFIG_HOME/blinools/blinools.toml` (or
`~/.config/blinools/blinools.toml`), and then a `blinools.toml` in the current directory.
`-c FILE` or `--config FILE` replaces the file in the current directory, on every command.
Project values override global ones key by key.
All options are described in [docs/configuration.md](docs/configuration.md), or in `man blinools.toml`.

## Documentation

The full documentation is split into multiple files:

- [docs/configuration.md](docs/configuration.md): every option of `blinools.toml`.
- [docs/sandbox.md](docs/sandbox.md): requirements, getting started and the sandbox commands.
- [docs/sandbox_architecture.md](docs/sandbox_architecture.md): how sandboxes work internally.
- [docs/cli](docs/cli/blinools.md): the command-line reference.

## Development

You need:

- A Rust toolchain (edition 2024) with `rustfmt` and `clippy`.
- [just](https://github.com/casey/just).
- `pandoc`, which generates the manpages.
- `mandoc`, which lints the generated pages.

Tests run on a single thread, which `.cargo/config.toml` enforces. Don't override it, because the
processes are created with a raw `clone3` and a child can deadlock on an allocator lock held by another test thread.

## License

The source code is distributed under the terms of the GNU General Public License v3.0 or later.
See LICENSE for details.
