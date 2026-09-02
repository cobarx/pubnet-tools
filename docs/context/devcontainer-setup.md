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

## File sharing with the host

**The workspace itself is automatic**: the project folder is bind-mounted,
so edits on either side (host editor, container shell) are the same files
instantly — no config, no copying. It lands at `/home/dev/code/pubnet-tools`
rather than the Dev Containers default of `/workspaces/pubnet-tools` — see
[Sibling repos](#sibling-repos-dotfiles-cobarx-and-ai-skills-cobarx) below
for why.

### Sibling repos: dotfiles-cobarx and ai-skills-cobarx

Two more repos are bind-mounted alongside pubnet-tools, from the same `~/code`
layout on the host:

```json
"workspaceMount": "source=${localWorkspaceFolder},target=/home/dev/code/pubnet-tools,type=bind",
"workspaceFolder": "/home/dev/code/pubnet-tools",
"mounts": [
  "source=${localEnv:HOME}/code/dotfiles-cobarx,target=/home/dev/code/dotfiles-cobarx,type=bind",
  "source=${localEnv:HOME}/code/ai-skills-cobarx,target=/home/dev/code/ai-skills-cobarx,type=bind"
]
```

This is why pubnet-tools itself moved off the Dev Containers default
`/workspaces/pubnet-tools` path — `workspaceMount`/`workspaceFolder`
override it so all three repos sit as true siblings under `/home/dev/code/`,
matching the host's own `~/code/<repo>` layout exactly. `${localEnv:HOME}`
keeps this portable across machines rather than hardcoding this machine's
home directory.

Verified: `git status`/`git log` work cleanly for all three from inside the
container — same healthy `.git` directories as the host, since these are
bind mounts, not copies.

**SSH agent forwarding** is wired up in `devcontainer.json`:

```json
"mounts": [
  "source=${localEnv:SSH_AUTH_SOCK},target=/ssh-agent,type=bind"
],
"remoteEnv": {
  "SSH_AUTH_SOCK": "/ssh-agent"
}
```

Bind-mounts the host's live agent socket (whatever `$SSH_AUTH_SOCK` resolves
to when the container is created) rather than copying key material into the
container — no private key ever lives inside it. Verified working: `ssh-add
-l` inside the container returns the identical response as running it on the
host directly (including "no identities" when the host agent is empty),
confirming it's the same live socket, not a stale copy.

Chose this over bind-mounting `~/.ssh` read-only specifically to keep private
key material off the container filesystem entirely.

`origin` was switched from HTTPS to SSH
(`git@github.com:cobarx/pubnet-tools.git`) specifically to use this. Two
things had to be true before it actually worked, in order encountered:

1. **A `known_hosts` entry for `github.com` inside the container.** Without
   one, `ssh` fails with `Host key verification failed` — the container had
   never seen GitHub's host key before. Fixed with a second, read-only bind
   mount:
   ```json
   "source=${localEnv:HOME}/.ssh/known_hosts,target=/home/dev/.ssh/known_hosts,type=bind,readonly"
   ```
   Reuses the host's already-trusted host keys rather than blindly
   auto-accepting on first connect inside the container. Readable by the
   container's `dev` user here because its UID (`1000`) happens to match the
   host user's UID — not guaranteed on every machine; if they differ, either
   loosen the file's permissions on the host or drop this mount and accept
   the one-time host-key prompt inside the container instead.
2. **An identity actually loaded in the host agent.** Forwarding the socket
   only helps if there's a key in it — `ssh-add -l` reporting "no
   identities" (on the host, and thus in the container too) means `ssh-add
   ~/.ssh/id_ed25519` (or whichever key) still needs to be run on the host
   first. Confirmed end to end: `ssh -T git@github.com` inside the
   container failed with `Host key verification failed` before the
   `known_hosts` mount, then with `Permission denied (publickey)` after it
   (expected — no identity loaded yet), pinpointing each blocker in turn
   rather than assuming.

`${localEnv:SSH_AUTH_SOCK}` is resolved once, at container creation time. If
the host agent restarts with a new socket path after that (rare with a
persistent agent manager; more likely with the ephemeral
`/tmp/ssh-XXXXXX/agent.NNNN` path `ssh-agent` uses by default per login
session), the mount goes stale until the container is recreated
(`devcontainer up --remove-existing-container`, or rebuild from the editor).

For any other host folder you want visible inside the container beyond the
workspace, add a bind mount the same way:

```json
"mounts": [
  "source=/host/path,target=/container/path,type=bind"
]
```

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
