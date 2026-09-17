#!/bin/bash

set -euo pipefail

ROOTFS_IMG="./rootfs.img"
ROOTFS="./rootfs"

podman build --target base -t fedora-microvm-rootfs .

podman create --name export-tmp fedora-microvm-rootfs
podman export export-tmp -o rootfs.tar
podman rm export-tmp

podman unshare rm -f "${ROOTFS}"
podman unshare mkdir -p "${ROOTFS}"
podman unshare tar -xpf rootfs.tar -C "${ROOTFS}" --xattrs --xattrs-include='*' \
    --numeric-owner --exclude='.dockerenv' --exclude='/run/.containerenv'
podman unshare rm -f "${ROOTFS}/etc/resolv.conf"

rm -f "${ROOTFS_IMG}"
truncate -s 5G "${ROOTFS_IMG}"

podman unshare mkfs.ext4 -F -L rootfs -d "${ROOTFS}" "${ROOTFS_IMG}"

chmod -w "${ROOTFS_IMG}"
podman unshare rm -rf "${ROOTFS}"
