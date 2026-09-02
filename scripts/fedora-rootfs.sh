#!/bin/bash

set -eux
ROOTFS_IMG="$(pwd)/rootfs.img"
ROOTFS="$(pwd)/rootfs"

podman build -f Dockerfile.fedora --target base -t fedora-microvm-rootfs .

podman create --name export-tmp fedora-microvm-rootfs
podman export export-tmp -o rootfs.tar
podman rm export-tmp

truncate -s 5G "${ROOTFS_IMG}"
mkfs.ext4 -L rootfs -F "${ROOTFS_IMG}"

sudo rm -rf tmp
mkdir tmp
sudo mount "${ROOTFS_IMG}" tmp
sudo tar -xpf rootfs.tar -C tmp --xattrs --xattrs-include='*' --numeric-owner --exclude='.dockerenv' --exclude='/run/.containerenv'
sudo umount tmp
sudo rm -rf tmp
docker build -f Dockerfile.fedora --target kernel --output type=local,dest=./out .
mv out/kernel .
rmdir out
