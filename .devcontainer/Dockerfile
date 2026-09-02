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
#   tar, unzip           - tar: just's install script unpacks its release archive
#                          with it; unzip: the Android cmdline-tools zip (below)
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
        unzip \
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

# ---------------------------------------------------------------------------
# Android build toolchain - for the pubnet-android epic (crates/pubnetchk-android
# + the android/ Gradle project). See docs/epics/pubnet-android/epic.md.
#
# None of this is needed to build, lint, or unit-test pubnetchk itself - it is
# here so the one container covers every crate in the workspace and the
# justfile's `android-lib` / `android-bindings` recipes work out of the box.
# The epic lists these as owner-installed prerequisites; the devcontainer is
# where this machine's owner installs them.
# ---------------------------------------------------------------------------

# Temurin JDK 21 (LTS), for Gradle / the Android Gradle Plugin. Fedora 44 only
# packages JDK 25, which AGP does not support yet - hence a pinned Adoptium
# build under /opt rather than a dnf package.
ENV JAVA_HOME=/opt/java
RUN sudo mkdir -p "$JAVA_HOME" && sudo chown "$USERNAME:$USERNAME" "$JAVA_HOME" \
    && curl --proto '=https' --tlsv1.2 -sSfL \
        "https://api.adoptium.net/v3/binary/latest/21/ga/linux/x64/jdk/hotspot/normal/eclipse" \
        -o /tmp/jdk.tar.gz \
    && tar -xzf /tmp/jdk.tar.gz -C "$JAVA_HOME" --strip-components=1 \
    && rm /tmp/jdk.tar.gz
ENV PATH=$JAVA_HOME/bin:$PATH

# Android SDK: command-line tools, then platform-tools + a pinned platform,
# build-tools, and NDK via sdkmanager. Versions are pinned so a rebuild - and
# CI (epic ticket 8) - resolve the same toolchain. Keep the NDK version in sync
# with android/app/build.gradle.kts once epic ticket 4 lands.
ARG ANDROID_CMDLINE_TOOLS=11076708
ARG ANDROID_PLATFORM=android-35
ARG ANDROID_BUILD_TOOLS=35.0.0
ARG ANDROID_NDK_VERSION=27.2.12479018
ENV ANDROID_HOME=/opt/android-sdk \
    ANDROID_SDK_ROOT=/opt/android-sdk
ENV ANDROID_NDK_HOME=${ANDROID_HOME}/ndk/${ANDROID_NDK_VERSION}
RUN sudo mkdir -p "$ANDROID_HOME/cmdline-tools" \
    && sudo chown -R "$USERNAME:$USERNAME" "$ANDROID_HOME" \
    && curl --proto '=https' --tlsv1.2 -sSfL \
        "https://dl.google.com/android/repository/commandlinetools-linux-${ANDROID_CMDLINE_TOOLS}_latest.zip" \
        -o /tmp/cmdline-tools.zip \
    && unzip -q /tmp/cmdline-tools.zip -d /tmp/ct \
    && mv /tmp/ct/cmdline-tools "$ANDROID_HOME/cmdline-tools/latest" \
    && rm -rf /tmp/cmdline-tools.zip /tmp/ct \
    && yes | "$ANDROID_HOME/cmdline-tools/latest/bin/sdkmanager" --licenses > /dev/null \
    && "$ANDROID_HOME/cmdline-tools/latest/bin/sdkmanager" \
        "platform-tools" \
        "platforms;${ANDROID_PLATFORM}" \
        "build-tools;${ANDROID_BUILD_TOOLS}" \
        "ndk;${ANDROID_NDK_VERSION}" > /dev/null
ENV PATH=$ANDROID_HOME/cmdline-tools/latest/bin:$ANDROID_HOME/platform-tools:$PATH

# Rust Android targets + cargo-ndk (drives the per-ABI cross-compile in
# `just android-lib`). uniffi-bindgen is the `[[bin]]` shim in
# crates/pubnetchk-android - no separate install.
RUN rustup target add \
        aarch64-linux-android \
        armv7-linux-androideabi \
        x86_64-linux-android \
    && cargo install cargo-ndk --locked
