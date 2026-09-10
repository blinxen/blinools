FROM fedora:44 AS base

ARG VM_USER=vmuser
ARG TIMEZONE=Europe/Berlin

# ========================================================================== #
# Packages                                                                   #
# ========================================================================== #
RUN --mount=type=cache,target=/var/cache/libdnf5,sharing=locked \
    --mount=type=cache,target=/var/cache/dnf,sharing=locked \
    dnf -y --setopt=keepcache=1 upgrade --refresh && \
    dnf install -y \
        systemd \
        systemd-udev \
        systemd-networkd \
        systemd-resolved \
        sudo \
        dbus \
        shadow-utils \
        podman \
        podman-docker \
        iproute \
        iputils \
        procps-ng \
        kernel-modules \
        ncurses \
        neovim \
        ripgrep \
        fd-find \
        chrony \
        cargo \
        rust \
        golang

# ========================================================================== #
# Sytem configuration                                                        #
# ========================================================================== #
RUN ln -sf "/usr/share/zoneinfo/${TIMEZONE}" /etc/localtime && \
    systemd-hwdb update && \
    ldconfig && \
    systemctl enable \
        systemd-networkd.service \
        systemd-resolved.service \
        serial-getty@hvc0.service && \
    systemctl mask \
        systemd-udev-settle.service \
        systemd-networkd-wait-online.service \
        systemd-fsck-root.service \
        auditd.service \
        console-getty.service \
        systemd-hwdb-update.service \
        ldconfig.service \
        systemd-vconsole-setup.service \
        systemd-homed.service \
        systemd-homed-activate.service \
        systemd-userdbd.service \
        getty@tty1.service && \
    systemctl set-default multi-user.target

RUN cat > /etc/sudoers.d/vmuser-power <<'EOF'
vmuser ALL=NOPASSWD: /sbin/reboot, /sbin/poweroff, /sbin/halt
EOF
RUN chmod 0440 /etc/sudoers.d/vmuser-power

# Network configuration
RUN mkdir -p /etc/systemd/network && cat > /etc/systemd/network/20-wired.network <<'EOF'
[Match]
Name=en*

[Network]
DHCP=yes
EOF

# Autologin
RUN mkdir -p /etc/systemd/system/serial-getty@hvc0.service.d && cat > /etc/systemd/system/serial-getty@hvc0.service.d/autologin.conf <<'EOF'
[Service]
ExecStart=
ExecStart=-/sbin/agetty --noclear --autologin vmuser - $TERM
EOF

# ========================================================================== #
# User                                                                       #
# ========================================================================== #
RUN groupadd -f "${VM_USER}" && \
    useradd -m -s /bin/bash -g "${VM_USER}" "${VM_USER}" && \
    if ! grep -q "^${VM_USER}:" /etc/subuid; then \
        echo "vmuser:100000:65536" >> /etc/subuid; \
    fi && \
    if ! grep -q "^${VM_USER}:" /etc/subgid; then \
        echo "vmuser:100000:65536" >> /etc/subgid; \
    fi && \
    mkdir -p /var/lib/systemd/linger && \
    touch "/var/lib/systemd/linger/${VM_USER}"

# ========================================================================== #
# Cleanup                                                                    #
# ========================================================================== #
RUN rm -rf /var/lib/dnf/history* /var/log/dnf* /tmp/* && \
    rm -f /etc/machine-id && touch /etc/machine-id

FROM scratch AS kernel
COPY --from=base /usr/lib/modules/*.x86_64/vmlinuz /kernel
