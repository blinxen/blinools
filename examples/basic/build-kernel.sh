#!/bin/bash

set -euo pipefail

VERSION="7.2.6"

TARBALL="linux-${VERSION}.tar.xz"
SRC_DIR="linux-${VERSION}"
CONFIG_FILE="linux-${VERSION}.config"

MAJOR="${VERSION%%.*}"
KERNEL_URL="https://cdn.kernel.org/pub/linux/kernel/v${MAJOR}.x/${TARBALL}"

if [[ ! -f "${CONFIG_FILE}" ]]; then
    echo "error: config file '${CONFIG_FILE}' not found in $(pwd)" >&2
    exit 1
fi

echo "Downloading ${KERNEL_URL}"
curl -fL -o "${TARBALL}" "${KERNEL_URL}"

echo "Extracting ${TARBALL}"
tar -xf "${TARBALL}"

cp "${CONFIG_FILE}" "${SRC_DIR}/.config"

cd "${SRC_DIR}"

make olddefconfig
make -j14

cp "arch/x86/boot/bzImage" ../kernel

echo "Kernel built: $(pwd)/../kernel"
