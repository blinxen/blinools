# blinxen's tools

A collection of my various bash scripts converted into a Rust CLI (Linux only).

## Commands

| Command | Description |
| --- | --- |
| `blinools sandbox` | Create and manage microVM sandboxes. See [docs/sandbox.md](docs/sandbox.md). |
| `blinools completions` | Generate shell completion scripts. |
| `blinools worktree` | Common git worktree operations used during development. |

Run `blinools --help` or `blinools <command> --help` for the options of each command.

## Installation

Fedora:

```bash
dnf copr enable blinxen/tools
dnf install blinools
```

### Prebuilt binaries

Tagged releases are built for `x86_64-unknown-linux-gnu` and `aarch64-unknown-linux-gnu`.
Download the archive for your architecture and `SHA256SUMS` from [Releases](../../releases),
then verify the downloaded archive and unpack it:

```bash
sha256sum --ignore-missing -c SHA256SUMS
tar xzf blinools-<version>-<target>.tar.gz
```

The archive contains the `blinools` binary, `LICENSE`, `README.md`, `CHANGELOG.md` and the man pages in
`man/man1` and `man/man5`. Copy the binary to a directory in your `PATH` and the man pages to a
directory searched by `man`. For example, install the binary in `~/.local/bin` and copy
`man/man1` and `man/man5` into `~/.local/share/man`.

### Build from source

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
`just uninstall` removes the installed files again, using the same variables.

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

## Documentation

The full documentation is split into multiple files:

- [docs/configuration.md](docs/configuration.md): every option of `blinools.toml`.
- [docs/sandbox.md](docs/sandbox.md): requirements, getting started and the sandbox commands.
- [docs/sandbox_architecture.md](docs/sandbox_architecture.md): how sandboxes work internally.
- [docs/cli](docs/cli/blinools.md): the command-line reference.

## Development

You need:

- A current stable Rust toolchain with `rustfmt` and `clippy` (see [Build from source](#build-from-source)).
- [just](https://github.com/casey/just).
- `pandoc`, which generates the configuration man page and converts command man pages to Markdown.
- `mandoc`, which lints the generated pages.

Run `just check` before you push.
It generates and lints the man pages, checks that the generated CLI reference is up to date,
checks formatting, runs Clippy and runs all tests.

Tests run on a single thread, as configured in `.cargo/config.toml`.
The reason for this is because we use the raw `clone3` system call
and running tests concurrently can deadlock a child on an allocator lock held by another test thread.

## License

The source code is distributed under the terms of the GNU General Public License v3.0 or later.
See LICENSE for details.
