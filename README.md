# pubnetchk

Audit the public WiFi or network you just joined.

Part of the planned `pubnet-tools` suite — `pubnetchk` audits a network right now;
`pubnetstat` (a `vmstat`-style watch mode) and `pubnettop` (a `top`-style live
dashboard) are planned, not yet built.

## What it does

Runs four checks and scores the result Low / Medium / High risk:

- **Security** — WiFi encryption (WPA3/WPA2/Open), DNS interception via DNS-over-HTTPS, captive portal detection
- **Speed** — download, upload, latency, jitter via M-Lab's open NDT7 protocol
- **Reliability** — ping/jitter/packet loss to gateway, 8.8.8.8, and 1.1.1.1
- **Topology** — passive ARP cache (no active scanning)

Pass `--save` to write a full JSON report to `~/.pubnetchk/reports/`.

pubnetchk reports what it finds; it doesn't fix it. If a run flags something about the
DNS resolver you're using, [docs/context/dns-hardening.md](docs/context/dns-hardening.md)
covers what that actually means and how to change it.

## Requirements

- Rust (edition 2024 toolchain — `rustup` is the easiest way to get one)
- **Linux:** `nmcli` (NetworkManager), `ip`, `ping`, `resolvectl`
- **macOS:** `route`, `ifconfig`, `arp`, `scutil`, `networksetup`, `ping` (all built in)
- **Windows:** PowerShell 5.1+ and `ping` (all built in). Build with the GNU toolchain
  (`rustup default stable-x86_64-pc-windows-gnu` + `scoop install mingw` on `PATH` for
  release builds) — the MSVC toolchain needs the Visual Studio C++ build tools.
  `record` is not supported on Windows; DNS-interception detection is limited (reports
  `uncertain`).

## Installation

```bash
git clone https://github.com/cobarx/pubnet-tools
cd pubnet-tools
cargo build --release
```

The binary is then at `target/release/pubnetchk`. To get a global `pubnetchk` command
on your `PATH` instead, run `cargo install --path .`.

## Usage

```bash
./target/release/pubnetchk              # full audit with terminal output
./target/release/pubnetchk --json | jq . # JSON to stdout (pipe-friendly)
./target/release/pubnetchk --save        # also write the report to ~/.pubnetchk/reports/
./target/release/pubnetchk -v            # add per-target reliability detail
./target/release/pubnetchk --no-speed    # skip a specific check (also: --no-topology/--no-security/--no-reliability)
./target/release/pubnetchk -q            # quick mode: shorter speed test
./target/release/pubnetchk record        # wrap in asciinema for session recording
```

Full flag reference: `pubnetchk --help`.

## Open source

pubnetchk itself is MIT. Every dependency is required to carry an MIT, Apache-2.0, or
ISC license — see [docs/decisions/2026-08-02-open-source-only.md](docs/decisions/2026-08-02-open-source-only.md)
for why, and `Cargo.toml` for the current dependency list rather than a copy here that
can go stale.

## License

MIT
