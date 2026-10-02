FROM fedora:44 AS base

ARG AGENT_USER=agent
ARG TIMEZONE=Europe/Berlin

# ===========================================================================
# Packages
# ===========================================================================
RUN --mount=type=cache,target=/var/cache/libdnf5,sharing=locked \
    --mount=type=cache,target=/var/cache/dnf,sharing=locked \
    dnf -y --setopt=keepcache=1 upgrade --refresh && \
    dnf -y --setopt=keepcache=1 install \
        adoptium-temurin-java-repository \
        dnf5-plugins && \
    dnf config-manager setopt adoptium-temurin-java-repository.enabled=1 && \
    dnf -y --setopt=keepcache=1 install \
        systemd \
        systemd-udev \
        systemd-networkd \
        systemd-resolved \
        sudo \
        dbus \
        util-linux \
        iproute \
        iputils \
        procps-ng \
        ncurses \
        chrony \
        tzdata \
        ca-certificates \
        curl \
        tar \
        gzip \
        python3 \
        podman \
        podman-docker \
        git \
        jq \
        yq \
        make \
        golang \
        rsync \
        acl \
        neovim \
        ripgrep \
        fd-find \
        temurin-25-jdk \
        cargo \
        rust \
        nvim \
        kind \
        helm \
        kubernetes \
        temurin-17-jdk && \
    dnf -y --setopt=keepcache=1 reinstall shadow-utils

# ===========================================================================
# System configuration
# ===========================================================================
RUN ln -sf "/usr/share/zoneinfo/${TIMEZONE}" /etc/localtime && \
    systemctl enable \
        systemd-networkd.service \
        systemd-resolved.service \
        serial-getty@hvc0.service && \
    systemctl mask \
        systemd-udev-settle.service \
        systemd-networkd-wait-online.service \
        systemd-fsck-root.service \
        auditd.service  \
        console-getty.service \
        getty@tty1.service && \
    systemctl set-default multi-user.target

# ===========================================================================
# User
# ===========================================================================
RUN groupadd -f "${AGENT_USER}" && \
    useradd -m -s /bin/bash -g "${AGENT_USER}" "${AGENT_USER}" && \
    if ! grep -q "^${AGENT_USER}:" /etc/subuid; then \
        echo "${AGENT_USER}:100000:65536" >> /etc/subuid; \
    fi && \
    if ! grep -q "^${AGENT_USER}:" /etc/subgid; then \
        echo "${AGENT_USER}:100000:65536" >> /etc/subgid; \
    fi && \
    mkdir -p /var/lib/systemd/linger && \
    touch "/var/lib/systemd/linger/${AGENT_USER}" && \
    AGENT_HOME="$(getent passwd "${AGENT_USER}" | cut -d: -f6)" && \
    install -d -m 0755 -o "${AGENT_USER}" -g "${AGENT_USER}" \
        "${AGENT_HOME}/.claude" \
        "${AGENT_HOME}/.config" \
        "${AGENT_HOME}/.config/opencode"

# ===========================================================================
# Mount automation
# ===========================================================================
COPY files/mounts/auto-mounts.sh /usr/local/bin/auto-mounts.sh
COPY files/mounts/auto-mounts.service /etc/systemd/system/auto-mounts.service
RUN chmod 0500 /usr/local/bin/auto-mounts.sh && \
    chmod 0600 /etc/systemd/system/auto-mounts.service && \
    printf 'AGENT_USER=%s\n' "${AGENT_USER}" > /etc/default/agent-mounts && \
    chmod 0600 /etc/default/agent-mounts && \
    chmod 0700 /mnt && \
    systemctl enable auto-mounts.service

# ===========================================================================
# Config files
# ===========================================================================
COPY files/20-wired.network /etc/systemd/network/20-wired.network
COPY files/autologin.conf /etc/systemd/system/serial-getty@hvc0.service.d/autologin.conf
RUN chmod 0644 /etc/systemd/network/20-wired.network \
               /etc/systemd/system/serial-getty@hvc0.service.d/autologin.conf

COPY files/sudoers-deny-agent /etc/sudoers.d/90-deny-agent
RUN chmod 0400 /etc/sudoers.d/90-deny-agent && \
    visudo -cf /etc/sudoers.d/90-deny-agent

# Claude
COPY files/claude/statusline.py /usr/local/bin/claude_statusline.py
COPY files/claude/claude.json /claude.json
COPY files/claude/settings.json /tmp/settings.json
RUN chmod 0555 /usr/local/bin/claude_statusline.py
# Opencode
COPY files/opencode/opencode.json /tmp/opencode.json
COPY files/agent/AGENTS.md /tmp/AGENTS.md

RUN AGENT_HOME="$(getent passwd "${AGENT_USER}" | cut -d: -f6)" && \
    install -m 0555 -o root -g root /tmp/settings.json "${AGENT_HOME}/.claude/settings.json" && \
    install -m 0555 -o root -g root /tmp/opencode.json "${AGENT_HOME}/.config/opencode/opencode.json" && \
    install -m 0555 -o root -g root /tmp/AGENTS.md "${AGENT_HOME}/.config/opencode/AGENTS.md" && \
    install -m 0555 -o root -g root /tmp/AGENTS.md "${AGENT_HOME}/.claude/CLAUDE.md" && \
    rm -f /tmp/settings.json /tmp/opencode.json /tmp/AGENTS.md && \
    printf '%s\n' \
        'if [ -r "${HOME}/.claude-login/export-auth-token.sh" ]; then' \
        '    . "${HOME}/.claude-login/export-auth-token.sh"' \
        'fi' >> "${AGENT_HOME}/.bashrc" && \
    install -m 0444 -o "${AGENT_USER}" -g "${AGENT_USER}" /claude.json "${AGENT_HOME}/.claude.json" && \
    rm /claude.json

# ===========================================================================
# Agent binaries
# ===========================================================================
ARG CLAUDE_DOWNLOAD_BASE_URL=https://downloads.claude.ai/claude-code-releases
ARG CLAUDE_PLATFORM=linux-x64
RUN version="$(curl -fsSL "${CLAUDE_DOWNLOAD_BASE_URL}/latest")" && \
    curl -fsSL -o /usr/local/bin/claude \
        "${CLAUDE_DOWNLOAD_BASE_URL}/${version}/${CLAUDE_PLATFORM}/claude" && \
    chmod 0755 /usr/local/bin/claude

ARG OPENCODE_URL=https://github.com/anomalyco/opencode/releases/latest/download/opencode-linux-x64.tar.gz
RUN tmp_dir="$(mktemp -d)" && \
    curl -fsSL -o "${tmp_dir}/opencode.tar.gz" "${OPENCODE_URL}" && \
    tar -xzf "${tmp_dir}/opencode.tar.gz" -C "${tmp_dir}" && \
    install -m 0755 "${tmp_dir}/opencode" /usr/local/bin/opencode && \
    rm -rf "${tmp_dir}"

# ===========================================================================
# Cleanup
# ===========================================================================
RUN rm -rf /var/lib/dnf/history* /var/log/dnf* /var/log/hawkey.log /tmp/* && \
    rm -f /etc/machine-id && touch /etc/machine-id
