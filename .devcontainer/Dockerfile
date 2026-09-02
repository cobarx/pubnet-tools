FROM registry.fedoraproject.org/fedora-minimal:44

# fedora-minimal ships microdnf, a lightweight subset of dnf. Bootstrap full
# dnf so later steps - and Claude Code's own devcontainer feature installer,
# which only recognizes apt-get/apk/dnf/yum, not microdnf - can rely on it.
# fedora-minimal's /etc/dnf/dnf.conf already sets install_weak_deps=false and
# tsflags=nodocs, and that config carries over to the bootstrapped dnf, so
# this stays lean rather than pulling in what the full fedora image ships by
# default.
RUN microdnf install -y dnf && microdnf clean all

# Build prerequisites, shell tooling, and editors:
#   gcc                  - linker/compiler cargo shells out to
#   openssl-devel,
#   pkgconf-pkg-config   - reqwest and tokio-tungstenite build with the
#                          `native-tls` feature (see crates/pubnetchk/Cargo.toml),
#                          which links system OpenSSL on Linux rather than
#                          vendoring rustls - openssl-sys needs both to find it
#   git                  - cargo uses it for git-source deps and vcs info
#   ca-certificates      - TLS to crates.io and the endpoints pubnetchk probes
#   sudo, shadow-utils   - non-root dev user + passwordless sudo below
#   curl, which          - installers (rustup, just) and general scripting
#   tar                  - just's install script unpacks its release archive with it
#   vim                  - editor
#   glow                 - terminal markdown renderer, for reading docs/*.md in place
#                          (Fedora package since F43 - hence fedora-minimal:44, not :42)
#   zsh                  - login shell for the dev user, set below
#   procps-ng            - provides `ps` (and top/free/etc.) - not in fedora-minimal by default
#   bc                   - calculator, for ad-hoc arithmetic in shell one-liners
RUN dnf install -y \
        gcc \
        openssl-devel \
        pkgconf-pkg-config \
        git \
        ca-certificates \
        sudo \
        shadow-utils \
        curl \
        which \
        tar \
        vim \
        glow \
        zsh \
        procps-ng \
        bc \
    && dnf clean all

# Non-root user with passwordless sudo. Dev Containers convention, and also
# what Claude Code's bypassPermissions/--dangerously-skip-permissions mode
# requires - it refuses to start as root or under sudo.
ARG USERNAME=dev
RUN useradd -m -s /bin/zsh "$USERNAME" \
    && echo "$USERNAME ALL=(ALL) NOPASSWD:ALL" > /etc/sudoers.d/"$USERNAME" \
    && chmod 0440 /etc/sudoers.d/"$USERNAME"

USER $USERNAME
ENV HOME=/home/$USERNAME
WORKDIR /home/$USERNAME

# Rust via rustup - distro-agnostic, same install path the MS devcontainers/rust
# image uses under the hood, just run explicitly here instead of pre-baked.
ENV RUSTUP_HOME=/home/$USERNAME/.rustup \
    CARGO_HOME=/home/$USERNAME/.cargo \
    PATH=/home/$USERNAME/.cargo/bin:$PATH
RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y \
        --default-toolchain stable --profile default \
    && rustup component add clippy rustfmt

# just - the command runner this repo's justfile depends on (build/release/test/clippy).
RUN curl --proto '=https' --tlsv1.2 -sSf https://just.systems/install.sh \
    | sudo bash -s -- --to /usr/local/bin
