# Changelog for `blinools`

## UNRELEASED

* Introduce the concept of `git_action`s which defines how `.git` directories or files get mounted for shares.
* Add `--read-only`, `--no-hidden`, `--hidden-as-read-only` flags to make shares handling easier.

## 0.2.0

* Introduce new `prune` command
* Support forcefully shutting down a sandbox
* Add support for qemu as the hypervisor
* Make qemu the default hypervisor
* Put sandboxes behind a cgroup

## 0.1.0

* Initial release
