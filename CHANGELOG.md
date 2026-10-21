# Changelog for `blinools`

## 0.4.0

### Breaking changes

* The `network` option is now a table with `mode` and `exceptions` instead of a string. Replace `network = "Unrestricted"` with `network = { mode = "Unrestricted" }`. `mode` accepts the same values as before and still defaults to `Restricted`.
* Limit automatic DNS-server firewall exemptions in `Restricted` mode to destination port 53. Previously, these exemptions allowed every port. Access to other ports follows the normal destination rules and explicit `network.exceptions`.
* Set the minimum supported Rust version to 1.91 in `Cargo.toml`.

### Added

* Add `network.exceptions` to allow individual destinations in blocked ranges in `Restricted` mode. An exception is an IPv4 or IPv6 address, a CIDR range or a hostname, optionally limited to one destination port. Hostnames are resolved when the sandbox starts; duplicate network-and-port entries are merged.
* Add man pages for every command and for the configuration file. Release archives include the pages under `man/man1` and `man/man5`.
* Add a browser readable CLI reference in `docs/cli`, generated from the command man pages.

### Changed

* Remove the IPv4-only `-4` flag from passt and pasta so they can use IPv6 as well as IPv4.
* Split the documentation into `docs/configuration.md`, `docs/sandbox.md` and `docs/sandbox_architecture.md`, and shorten the README.
* Expand CLI help with examples and more precise option descriptions. Document `wip-pr` as not implemented.
* Update both example root filesystems from Fedora 44 to Fedora 45.
* Point both examples at the shared `misc/kernel/kernel.config`, selecting the Linux 7.2.11 configuration used by the build script.

### Fixed

* Fix the AI agent example's Dockerfile and systemd unit to use the supplied `auto-mounts.sh` and `auto-mounts.service` files, allowing the image to build and its share mounting service to start.
* Fix checksum verification instructions for users who download only one architecture's archive by ignoring missing archives listed in `SHA256SUMS`.
* Link release notes to the changelog at the release tag.

## 0.3.0

### Breaking changes

* The `network` option now accepts `Restricted`, `Unrestricted` and `None`. `Lan` was removed, use `Unrestricted` instead.
* The default `network` is now `Restricted`, which blocks access to the local network. Set `network = "Unrestricted"` to get the old behaviour.
* `.git` directories and files inside shares are now mounted read-only by default (`git_action = "ReadOnly"`).

### Added

* Add a firewall (nftables) for the new `Restricted` network mode. It blocks incoming connections and outgoing connections to loopback, private, CGNAT, link-local and unique-local ranges. The servers configured in `dns` are automatically excluded from the block.
* Add `pasta` as a requirement for networking, together with the `pasta.binary` config option. `nft` is required for the `Restricted` network mode.
* Introduce the concept of `git_action`s which defines how `.git` directories or files get mounted for shares. It can be set globally and per share (`ReadOnly`, `Hide`, `None`).
* Add `--read-only`, `--no-hidden`, `--hidden-as-read-only` flags to make shares handling easier.
* Add an `ai-agent` example which builds a hardened rootfs for Claude Code, OpenCode and Codex.

### Changed

* Every process (VMM, passt, virtiofsd) is now started in its own user, mount, network, PID, IPC, UTS, time and cgroup namespace and directly inside the sandbox cgroup.
* passt now runs in its own network namespace, pasta connects it to the host network.
* Centralize process management. Processes get `SIGTERM` and then `SIGKILL` after 5 seconds when they are stopped and failures before exec are reported with the actual reason.
* Move kernel and rootfs build scripts and kernel configs to `misc/` so that the examples can share them.
* Update the example kernel to Linux 7.2.9.
* Update dependencies.

### Fixed

* Fix the console handling when the guest exits: pending input is dropped and `EIO` as well as transient read errors from the pty are handled correctly.

## 0.2.0

* Introduce new `prune` command
* Support forcefully shutting down a sandbox
* Add support for qemu as the hypervisor
* Make qemu the default hypervisor
* Put sandboxes behind a cgroup

## 0.1.0

* Initial release
