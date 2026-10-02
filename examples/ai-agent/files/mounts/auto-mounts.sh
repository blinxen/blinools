#!/bin/bash
set -euo pipefail

AGENT_USER="${AGENT_USER:-agent}"
AGENT_HOME="$(getent passwd "${AGENT_USER}" | cut -d: -f6)"

# Names under /mnt that should not be mounted at all.
EXCLUDE=()

is_excluded() {
    local n="$1" e
    for e in "${EXCLUDE[@]}"; do [[ "$n" == "$e" ]] && return 0; done
    return 1
}

# Sources that land somewhere other than ${AGENT_HOME}/<name>.
dest_for() {
    case "$1" in
        claude-login) echo "${AGENT_HOME}/.claude-login" ;;
        *)            echo "${AGENT_HOME}/$1" ;;
    esac
}

# mountinfo escapes space, tab, newline and backslash as octal; \NNN -> \0NNN
# is what printf %b expects.
mount_targets() {
    awk '{print $5}' /proc/self/mountinfo | while read -r target; do
        printf '%b\n' "${target//\\/\\0}"
    done
}

do_mount() {
    shopt -s nullglob
    local src name dest

    for src in /mnt/*/; do
        src="${src%/}"
        name="$(basename "$src")"

        is_excluded "$name" && continue

        dest="$(dest_for "$name")"

        if mountpoint -q "$dest"; then
            echo "skipping ${dest}: already mounted"
            continue
        fi

        install -d -m 0755 -o "${AGENT_USER}" -g "${AGENT_USER}" "$dest"
        mount --rbind "$src" "$dest"
        mount --make-rslave "$dest"

        echo "bound ${src} -> ${dest}"
    done
}

do_umount() {
    local target rc=0

    # Reverse order so children come off before the tree they sit in.
    while read -r target; do
        case "$target" in
            "${AGENT_HOME}"/*) ;;
            *) continue ;;
        esac
        mountpoint -q "$target" || continue

        if ! umount "$target"; then
            echo "WARNING: could not unmount ${target}" >&2
            rc=1
        fi
    done < <(mount_targets | sort -r)

    return "$rc"
}

case "${1:-mount}" in
    mount)  do_mount ;;
    umount) do_umount ;;
    *) echo "usage: $0 [mount|umount]" >&2; exit 2 ;;
esac
