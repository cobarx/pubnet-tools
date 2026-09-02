# pubnetchk

Audit the public WiFi or network you just joined.

Part of the planned `pubnet-tools` suite — `pubnetchk` audits a network right now;
`pubnetstat` (a `vmstat`-style watch mode) and `pubnettop` (a `top`-style live
dashboard) are planned, not yet built.

## What it does

Runs four checks and scores the result Low / Medium / High risk:

- **Security** — WiFi encryption (WPA3/WPA2/Open), DNS interception via DNS-over-HTTPS, captive portal detection
- **Speed** — download, upload, latency, jitter via M-Lab's open NDT7 protocol
- **Reliability** — ping/jitter/packet loss to your gateway (the router you're connected to) and two well-known public DNS servers: Google's `8.8.8.8` and Cloudflare's `1.1.1.1`
- **Topology** — passive ARP cache (no active scanning)

Nothing here needs root, and topology is passive-only — it reads the ARP cache, it
never scans. pubnetchk reports what it finds; it doesn't fix it. If a run flags
something about the DNS resolver you're using,
[docs/context/dns-hardening.md](docs/context/dns-hardening.md) covers what that actually
means and how to change it.

## Setting up the build environment

You need a Rust toolchain (edition 2024). [`rustup`](https://rustup.rs) is the easiest
way to get one:

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

### Dev Container (any host OS)

`.devcontainer/` is a ready-made [Dev Containers](https://containers.dev/) environment
(Fedora base) with the Rust toolchain, `just`, and the Android cross-compile toolchain
(Temurin JDK, Android SDK + NDK, `cargo-ndk`, the `*-linux-android` Rust targets) already
installed. From VS Code or a JetBrains IDE use "Reopen in Container"; with the
[`devcontainer` CLI](https://github.com/devcontainers/cli):

```bash
devcontainer up --workspace-folder .
docker exec -it pubnet-tools-dev zsh
```

Good for `just build` / `just clippy` / `just test` on any crate in the workspace. It is
a Linux container, so it can't run `just test-all`, the macOS/Windows probes, or any
"does the audit read this network correctly" check (those need host access on the target
OS). See [docs/context/devcontainer-setup.md](docs/context/devcontainer-setup.md).

### Native host setup

The runtime prerequisites depend on your OS:

#### Linux

Uses `nmcli` (NetworkManager), `ip` (iproute2), `resolvectl` (systemd), and `ping` —
all present on a standard desktop distro. Building also needs the system OpenSSL
development headers for the TLS stack:

```bash
# Debian / Ubuntu
sudo apt install build-essential pkg-config libssl-dev

# Fedora
sudo dnf install @development-tools pkg-config openssl-devel
```

#### macOS

Everything the probes call (`route`, `ifconfig`, `arp`, `scutil`, `networksetup`,
`ping`) ships with the OS. Install the Xcode command-line tools for a linker:

```bash
xcode-select --install
```

#### Windows 10+

The probes call the Win32 API directly, so there's nothing extra at runtime. For
building, use the **GNU toolchain** — the default MSVC toolchain would pull in the
Visual Studio C++ build tools:

```powershell
rustup toolchain install stable-x86_64-pc-windows-gnu
rustup override set stable-x86_64-pc-windows-gnu    # run inside the repo; machine-local
scoop install mingw                                  # puts dlltool.exe on PATH
```

The override is deliberately local — don't commit a `rust-toolchain.toml`, or you'd
force the GNU toolchain on Linux/macOS contributors too. On Windows, `record` is
unsupported and DNS-interception detection reports `uncertain`.

## Building

```bash
git clone https://github.com/cobarx/pubnet-tools
cd pubnet-tools

cargo build --release        # optimized binary at target/release/pubnetchk
cargo build                  # faster debug build at target/debug/pubnetchk
```

To install a global `pubnetchk` command on your `PATH`:

```bash
cargo install --path .
```

## Usage

Run it right after joining a network. With no arguments it runs the full audit and
prints a scored terminal report:

```bash
pubnetchk
```

Common options:

```bash
pubnetchk --json | jq .        # JSON to stdout (pipe-friendly, no spinners)
pubnetchk --save               # also write a JSON report to ~/.pubnetchk/reports/
pubnetchk --html --open        # write a plain-language HTML report and open it
pubnetchk -q                   # quick mode: shorter speed test
pubnetchk -v                   # add per-target reliability detail
pubnetchk --no-speed           # skip a check (also --no-topology/--no-security/--no-reliability)
pubnetchk --only security,speed # run only the named checks (topology,security,reliability,speed)
pubnetchk --strict             # exit non-zero on Medium/High risk (for scripts)
pubnetchk record               # wrap the run in asciinema for session capture
```

Full flag reference: `pubnetchk --help`.

If you built without installing, the binary is at `./target/release/pubnetchk` (or
`./target/debug/pubnetchk`) — use that path in place of `pubnetchk` above.

## Open source

pubnetchk itself is MIT. Every dependency is required to carry an MIT, Apache-2.0, or
ISC license — see [docs/decisions/2026-08-02-open-source-only.md](docs/decisions/2026-08-02-open-source-only.md)
for why, and `Cargo.toml` for the current dependency list rather than a copy here that
can go stale.

## License

MIT
