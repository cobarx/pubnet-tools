# pubnet-tools

![MIT licensed](https://img.shields.io/badge/license-MIT-blue.svg)

Audit the public WiFi or network you just joined.

### pubnetchk

Runs a full network audit — WiFi security, speed, reliability, and passive
topology — and prints a scored terminal report.

![pubnetchk running a full audit in a terminal, then printing a scored Network / Security / Performance report](docs/assets/demo.gif)

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

## Installing

You need a Rust toolchain (edition 2024) — [`rustup`](https://rustup.rs) is the
easiest way to get one:

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

pubnetchk isn't packaged anywhere yet, so build it from source and install it onto
your `PATH`:

```bash
git clone https://github.com/cobarx/pubnet-tools
cd pubnet-tools
cargo install --path crates/pubnetchk
```

Or build without installing:

```bash
cargo build --release        # optimized binary at target/release/pubnetchk
cargo build                  # faster debug build at target/debug/pubnetchk
```

Runtime prerequisites vary by OS — see [Platform support](#platform-support) below,
and [Development](#development) for the extra build-time setup each platform needs
(Linux OpenSSL headers, the Windows GNU toolchain, etc.).

## Usage

Run it right after joining a network. With no arguments it runs the full audit and
prints a scored terminal report:

```bash
pubnetchk
```

```
✔ All checks passed
Risk: Low (5 pts)

Network:
  Interface: wlan0 · WiFi (192.168.1.42/24)
  Gateway: 192.168.1.1
  SSID: MyHomeNetwork — WPA2
  Channel: 153 (5765 MHz), Signal: 70%

Security:
  DNS check: not intercepted
  Captive portal: none

Performance:
  Local: 0% loss, 21.7ms
  Internet: 0% loss, 23.9ms
  Speed: 356.3 Mbps down / 432.4 Mbps up
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

## Platform support

| | Linux | macOS | Windows 10+ | Android |
|---|---|---|---|---|
| How it reads the network | `ip`, `nmcli`, `resolvectl` | `route`, `ifconfig`, `arp`, `scutil` | Win32 API directly (no shelling out) | Kotlin gathers facts, Rust engine via UniFFI |
| Needs root/admin | No | No | No | No |
| WiFi SSID | Yes | Redacted unless you grant Location Services | Yes | Yes |
| DNS-interception verdict | `clean`/`leaked` | `uncertain` (no egress-IP read) | `uncertain` (no egress-IP read) | `uncertain` |
| `pubnetchk record` | Yes (asciinema) | Yes (asciinema) | No — use Windows Terminal's own capture, or run under WSL | n/a (no CLI) |
| Extra build-time setup | OpenSSL dev headers | Xcode CLI tools | GNU toolchain (see [Development](#development)) | See [android/README.md](android/README.md) |

The Android app is in progress — see [docs/epics/pubnet-android/](docs/epics/pubnet-android/).

## Development

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

### Building

```bash
just build                  # debug — also sets execute bit for MSYS2/zsh (see Windows note)
just release                # release — target/release/pubnetchk + pubnetdiag
just test                   # unit tests — fast, no network
just test-all               # + contract tests: hit real commands and real endpoints (need live network)
just clippy
```

`just` is a cross-platform command runner (`scoop install just`). The `justfile`
at the repo root wraps the common cargo invocations and, on Windows, runs
`chmod +x` on the output binaries so they are executable from MSYS2/zsh. In
PowerShell the chmod line fails silently (the `-` prefix suppresses the error);
in PowerShell you can also use `cargo build` / `cargo build --release` directly.

### Regenerating the demo GIF

The GIF at the top of this README is a real `pubnetchk -q -v` run, captured with
[asciinema](https://asciinema.org/) and rendered with
[agg](https://github.com/asciinema/agg) (both installable via `cargo install`, no
system packages needed):

```bash
asciinema rec --window-size 100x24 --idle-time-limit 1 \
  -c "./target/release/pubnetchk -q -v" demo-raw.cast

# drop redundant spinner frames instead of speeding up playback —
# see scripts/trim-demo-cast.py for what "redundant" means here
python3 scripts/trim-demo-cast.py demo-raw.cast demo-trimmed.cast

agg --theme github-dark --font-size 15 --rows 24 --last-frame-duration 11.6 demo-trimmed.cast docs/assets/demo.gif
```

Redact anything network-identifying (your real SSID, in particular) out of the
`.cast` file before rendering — it's plain JSON, a text substitution is enough.

## Open source

pubnetchk itself is MIT. Every dependency is required to carry a compatible open source
license — see [docs/decisions/2026-08-02-open-source-only.md](docs/decisions/2026-08-02-open-source-only.md)
for why, and `Cargo.toml` for the current dependency list rather than a copy here that
can go stale.

## License

MIT
