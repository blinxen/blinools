# Changelog for `blinools`

## Unreleased

### Breaking changes

* The `network` option is now a table with `mode` and `exceptions` instead of a string. Replace `network = "Unrestricted"` with `network = { mode = "Unrestricted" }`. `mode` accepts the same values as before and still defaults to `Restricted`.
* In `Restricted` mode the servers configured in `dns` are now only reachable on port 53. Before, every port on them was allowed.

### Added

* Add `network.exceptions` to keep individual destinations reachable in `Restricted` mode although they are in a blocked range. An exception is an IP address, a CIDR range or a hostname, optionally limited to one port. Hostnames are resolved when the sandbox starts.
* Add manual pages for every command (`blinools(1)`, `blinools-sandbox(1)`, ...) and for the configuration file (`blinools.toml(5)`). They are generated with `just man` and included in the release archives under `man/man1` and `man/man5`.

### Changed

* Split the documentation into `docs/configuration.md`, `docs/sandbox.md` and `docs/sandbox_architecture.md`, and slim down the README. The documentation is now regular Markdown that can be read in the browser.

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
