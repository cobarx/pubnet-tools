# Dev container setup

A [Dev Containers](https://containers.dev/) environment for this repo:
`.devcontainer/Dockerfile` + `.devcontainer/devcontainer.json`. Works the same
way from VS Code ("Reopen in Container"), JetBrains, GitHub Codespaces, or the
bare `devcontainer` CLI — same config, whichever engine (Docker Desktop,
Docker Engine, Rancher Desktop, Colima) sits underneath on the host OS.

## What's inside

Base: `registry.fedoraproject.org/fedora-minimal:44` — not Docker Hub's
`fedora-minimal` (that name isn't published there; Fedora's minimal images
live at their own registry), and not the prebuilt
`mcr.microsoft.com/devcontainers/rust` image, which bundles a lot of tooling
this project doesn't use (zsh/oh-my-zsh, other language prereqs) in exchange
for less Dockerfile to write. Hand-rolling a minimal base instead, in service
of "lightweight and easy" over batteries-included.

On top of that:

- **`dnf`**, bootstrapped via `microdnf install -y dnf`. `fedora-minimal`
  ships only `microdnf`, a lightweight subset — but the Claude Code
  devcontainer feature's installer only recognizes `apt-get`/`apk`/`dnf`/`yum`,
  not `microdnf`. Bootstrapping keeps the minimal image's
  `install_weak_deps=false` / `tsflags=nodocs` config (it carries over to the
  newly-installed `dnf`), so this stays lean rather than pulling in what the
  full `fedora` image ships by default.
- **`gcc`, `openssl-devel`, `pkgconf-pkg-config`** — not optional. `reqwest`
  and `tokio-tungstenite` are both built with the `native-tls` feature (see
  `crates/pubnetchk/Cargo.toml`), which links system OpenSSL on Linux rather
  than vendoring `rustls`. Without these three, `openssl-sys` fails to build
  before any project code even compiles. Confirmed working: `cargo check`
  compiles `openssl-macros`, `reqwest`, and `tokio-tungstenite` clean in this
  image.
- **`rustup`** (stable, `clippy` + `rustfmt` components) — installed the same
  way regardless of base image; the distro was never actually the hard part.
- **`just`** — this repo's command runner (`justfile` wraps `build`/`release`/
  `check`/`test`/`test-all`/`clippy`). Needed `tar` added explicitly too —
  `fedora-minimal` really is minimal; `just`'s install script unpacks a
  release archive with it and fails without it.
- **`vim`, `glow`** — editor and terminal markdown rendering, for reading
  `docs/*.md` in place. `glow` is an official Fedora package, but only from
  **Fedora 43 onward** — this is the reason the tag is `:44`, not `:42`.
- **Claude Code CLI**, via the official
  [`ghcr.io/anthropics/devcontainer-features/claude-code`](https://github.com/anthropics/devcontainer-features/tree/main/src/claude-code)
  feature in `devcontainer.json`, not the Dockerfile.
- A non-root `dev` user with passwordless `sudo` — the Dev Containers
  convention, and also what Claude Code's `bypassPermissions` /
  `--dangerously-skip-permissions` mode requires (it refuses to start as root).

## Persisted volumes

Two named Docker volumes survive a container rebuild:

- `pubnet-tools-cargo-registry` → `/home/dev/.cargo/registry` (skip re-downloading
  every crate on every rebuild)
- `claude-code-config-${devcontainerId}` → `/home/dev/.claude`, with
  `CLAUDE_CONFIG_DIR` set to match (persists Claude Code auth across rebuilds)

**Gotcha, already fixed in `postCreateCommand`**: Docker creates a fresh named
volume owned by `root:root` by default, which overwrites whatever ownership
the image had baked in at that path. Without the `sudo chown -R dev:dev ...`
step first, `cargo fetch` (and Claude Code itself) fail with `Permission
denied` the first time a volume is mounted. If you add another persisted path
later, chown it the same way before anything writes there.

## Using it

```bash
# VS Code / JetBrains: Command Palette -> "Dev Containers: Reopen in Container"

# CLI (npm i -g @devcontainers/cli):
devcontainer up --workspace-folder .
devcontainer exec --workspace-folder . just check
devcontainer exec --workspace-folder . just clippy
devcontainer exec --workspace-folder . just test
```

Validated end to end: `just check`, `just clippy`, and `just test` (91 unit
tests across `pubnetchk` and `pubnetdiag`) all pass clean inside this
container as of this setup.

## What this container is *not* for

`just test-all` (the contract tests — "hit real commands and real endpoints,
need live network") and any manual "does pubnetchk actually detect this
network correctly" check **do not belong in this container**, and won't give
a trustworthy answer if run here:

- Container networking sits behind a virtual bridge/NAT. The ARP cache,
  default route, and DNS resolver behavior a sandboxed process sees are the
  container's, not the host's — exactly the things `pubnetchk` audits. A
  clean or dirty result from inside the container says nothing reliable about
  the actual WiFi you're on.
- The macOS and Windows platform probes (`scutil`/`system_profiler`/
  `ipconfig`, and the raw Win32 API calls in `pubnet-platform`'s
  `windows-sys` target) **cannot run here at all** — this is a Linux
  container regardless of host OS, by design of the Dev Containers spec.

So: build, lint, and unit-test in the container; run `just test-all` and any
real-world verification directly on a real install of each target OS, outside
any container.

## A bug this setup surfaced

Building here caught a pre-existing break on `main`: `crates/pubnetchk/Cargo.toml`
was missing a `regex` dependency that `crates/pubnetchk/src/cli.rs` actually
uses (it was only declared in `pubnet-platform`'s `Cargo.toml`, from the Cargo
workspace restructure). Already tracked and fixed separately in
[#25](https://github.com/cobarx/pubnet-tools/pull/25) — not part of this
change, just surfaced by actually running the build in a clean environment.

## Host setup notes (this machine)

This machine had no container engine at all before this setup — worth noting
in case another machine needs the same bootstrap:

- **Docker Engine** installed via `pacman` (`docker`, `docker-buildx`),
  `docker.service` enabled, and the user added to the `docker` group. Group
  membership doesn't take effect in an already-running login session — needs
  either a full logout/login, or `newgrp docker` run in a terminal *before*
  launching whatever process needs it (a new terminal alone isn't enough; it
  still inherits the original login session's groups).
- **`@devcontainers/cli`** installed via `npm i -g` — but *not* through the
  system `npm` (installing globally there needs root) or through the shell's
  `nvm`-wrapped `npm` function (broken on this machine, referencing a
  `_nvm_load` helper that doesn't exist). Installed instead directly against
  an actual `nvm` Node version:
  `~/.nvm/versions/node/<version>/bin/npm install -g @devcontainers/cli --prefix ~/.nvm/versions/node/<version>`.

## Reusing this pattern for other projects

The scaffolding here — Fedora-minimal base, bootstrapped `dnf`, rustup, `just`,
the Claude Code feature, the volume-ownership fix — isn't pubnet-tools-specific
and is meant to carry over. What *is* project-specific and would need
revisiting per project: the `openssl-devel`/`pkgconf-pkg-config` pair (only
needed because this project chose `native-tls` over `rustls`), and obviously
the language toolchain itself if a future reused copy isn't Rust.
