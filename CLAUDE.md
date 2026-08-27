# pubnet-tools

## Summary

`pubnetchk` is a Rust CLI that audits the public WiFi or network you just joined. It
checks security posture (WiFi encryption, DNS interception, captive portal), speed
(M-Lab's open NDT7 protocol, implemented directly over WebSocket), reliability
(ping/jitter/packet loss to three targets), and passive network topology (ARP cache
only — no active scanning). Results are scored Low/Medium/High risk and, with `--save`,
written as a JSON report. A `record` subcommand wraps the run in asciinema for session
capture (not on Windows).

It is the first binary of a planned `pubnet-tools` suite (`pubnetstat`, `pubnettop` —
not yet built). Built as a blog post project. Technology rationale is a first-class
concern — every dependency is justified against its alternatives in `docs/decisions/`.

The project began as a TypeScript CLI (`conncheck`); the Rust rewrite is now canonical
(see [2026-08-26-rust-becomes-canonical-implementation.md](docs/decisions/2026-08-26-rust-becomes-canonical-implementation.md)
— the TS tree was archived to a local `typescript-archive` branch; only `main` exists
on `origin`), and the rename is recorded in
[2026-08-26-rename-to-pubnet-tools.md](docs/decisions/2026-08-26-rename-to-pubnet-tools.md).
The specs in `docs/specs/` and the pre-rewrite decisions are behavior contracts and
carry over unchanged.

**Platforms:** Linux, macOS, and Windows, each with a `PlatformProbe` implementation.
Non-root everywhere — nothing in `pubnetchk` requests elevated privileges.

- **Linux:** `ip`, `nmcli` (NetworkManager), `resolvectl`, `ping` (shelled out to)
- **macOS:** `route`, `ifconfig`, `arp`, `scutil`, `networksetup`, `ipconfig getsummary`,
  `system_profiler` (Wi-Fi — `airport` was removed in macOS 15/26), `ping` (shelled out to)
- **Windows:** the Win32 API directly (IP Helper + WLAN + `IcmpSendEcho2`) via
  `windows-sys` — no child processes, no PowerShell. Build with the GNU toolchain —
  see Development setup.

## Architecture

```
pubnet-tools
  ├── src/main.rs         captures local UTC offset (pre-runtime), builds the tokio runtime, calls cli::run()
  ├── src/lib.rs          module declarations only
  ├── src/cli.rs          clap setup, orchestrates checks, manages the shared spinner
  ├── src/types.rs        all structs and enums, serde derives, CheckResult<T> — no logic
  ├── src/scoring.rs      pure function: &[ScorableResult] → { total, level, findings }
  ├── src/exec.rs         tokio process wrapper, array argv (no shell), never Errs on non-zero exit
  ├── src/network.rs      pure synchronous parsers + classification helpers for command output
  ├── src/checks/
  │   ├── topology.rs     default route → interface → addr/neigh; passive only; seeds gateway
  │   ├── security.rs     WiFi info + DNS servers + DoH probes + captive portal (reqwest)
  │   ├── reliability.rs  ping ×10, join_all over targets, per-packet RTT parsing
  │   └── speed.rs        NDT7 (M-Lab) client over tokio-tungstenite, hand-rolled protocol
  ├── src/output/
  │   ├── renderer.rs     console only, condensed Network/Security/Performance sections, never calls network
  │   └── reporter.rs     writes JSON to ~/.pubnetchk/reports/<timestamp>.json (only with --save)
  └── src/platform/
      ├── mod.rs          PlatformProbe trait + shared types (RouteInfo, AddrInfo, WifiInfo)
      ├── linux.rs        ip / nmcli / resolvectl
      ├── macos.rs        route / ifconfig / arp / scutil / ipconfig getsummary (fast Wi-Fi)
      │                   / system_profiler -json (slow Wi-Fi: channel+signal) (+ inline parsers)
      └── windows.rs      Win32 API via windows-sys: GetAdaptersAddresses / GetBestRoute2 /
                          GetIpNetTable2 / WLAN API / IcmpSendEcho2 — no shelling out
```

**Data flow:** topology runs first and yields `gateway` + `interface`. Security,
reliability, and speed then run concurrently (`tokio::join!`). scoring is a pure
function over all four results. render and save happen after all checks complete.

**`CheckResult<T>` contract:** checks never throw / never return `Err`. `status` is one
of `ok | degraded | failed | skipped`. `data` is `None` only when status is `failed`
or `skipped`. Callers inspect `status` and `errors`, never propagate a panic out of a
check. Only a genuine spawn failure (binary not found) surfaces as an `Err` from
`exec.rs`, and it's caught at the check level.

**`PlatformProbe`** (`src/platform/mod.rs`) is the OS-abstraction seam: seven async
methods (`default_route`, `interface_addr`, `arp_neighbors`, `wifi_info`, `dns_info`,
`system_egress_ip`, `interface_type`) returning common types. Checks call probe
methods — they never invoke a platform-specific binary directly. Adding an OS is one
new file implementing the trait plus a `#[cfg(target_os = "…")]` arm in `cli.rs` and in
the three contract tests. `system_egress_ip` returns `None` on macOS and Windows, so
the DNS-interception verdict is `uncertain` there rather than `clean`/`leaked`.

## Development setup

```bash
git clone https://github.com/cobarx/pubnet-tools
cd pubnet-tools

cargo build                 # debug
cargo build --release       # target/release/pubnetchk (~2.8–5 MB)
cargo test --lib            # unit tests — fast, no network
cargo test                 # + contract tests: hit real commands and real endpoints (need live network)
cargo clippy --all-targets
cargo run -- --json | jq .  # JSON mode
cargo run -- --no-speed     # skip a check while iterating
```

**Windows toolchain.** The default `x86_64-pc-windows-msvc` target needs the Visual
Studio C++ build tools. Instead use the GNU toolchain:

```powershell
rustup toolchain install stable-x86_64-pc-windows-gnu
rustup override set stable-x86_64-pc-windows-gnu    # machine-local; do NOT commit a rust-toolchain.toml
scoop install mingw                                  # dlltool.exe on PATH — needed to link windows-sys and for --release
```

`windows-sys` is a `[target.'cfg(windows)'.dependencies]` entry — it never touches the
Linux/macOS build. See
[2026-08-28-windows-probes-via-win32-api.md](docs/decisions/2026-08-28-windows-probes-via-win32-api.md)
(and the superseded [2026-08-27](docs/decisions/2026-08-27-windows-platform-support.md)
for the toolchain rationale).

## Conventions

- **Spec-driven, test-driven.** Load-bearing/conditional behavior is specified in
  `docs/specs/` (Given-When-Then, per
  [MetanoiaFramework's `spec` skill](~/Code/MetanoiaFramework/skills/spec/SKILL.md))
  *before* an implementation approach is chosen, and implemented test-first (per
  [the `tdd` skill](~/Code/MetanoiaFramework/skills/tdd/SKILL.md)). Pure scaffolding
  (types, the exec wrapper, the platform trait) doesn't need a spec. A test cites the
  scenario it implements as `// spec: <slug>#S<n>`.
- **Test levels are by scope, not entry point — never "e2e".**
  - *Unit* — inline `#[cfg(test)] mod tests` in the module. Everything is a pure
    function over a string/data fixture; almost nothing is mocked because almost
    nothing needs to be.
  - *Contract* — `tests/*.rs` (`topology.rs`, `security.rs`, `reliability.rs`,
    `speed.rs`). One real boundary: a real system command or a real network endpoint.
    `#[cfg]`-selects the platform probe. No mocks.
  - Contract tests **assert on shape, not exact values** — real networks vary. Check
    that `verdict` is one of the three valid strings, not that it equals `clean`.
- **Empirical fixtures.** Any test input representing external-command output is a
  *real capture*, never hand-typed — see
  [the `empirical-fixtures` skill](~/Code/MetanoiaFramework/skills/empirical-fixtures/SKILL.md).
  `tests/fixtures/capture.sh <context>` captures a new environment (Linux/macOS);
  output is committed. `tests/fixtures/NEEDED.md` tracks gaps. Windows has no fixtures —
  its probes call the Win32 API and parse no command output, so its coverage is the
  contract tests plus pure mapping unit tests.
- **Feature requests and known gaps are GitHub issues** (`gh issue`), not TODO comments
  or notes buried in decision docs.
- **Checks never throw.** All failure surfaces as `CheckResult` state.
- **`serde` field casing is deliberate.** JSON is camelCase; enums use explicit
  `rename`/`rename_all`. Never render an enum to the user with `{:?}` — use the
  `as_str()` methods, which match the JSON form (see the note on `PingTargetLabel`).

## What to avoid

- **Hostname ping targets.** Captive networks break DNS for ICMP. Always use numeric
  IPs (`1.1.1.1`, `8.8.8.8`).
- **Quad9 DoH.** Blocked on many public networks. Use only Cloudflare and Google DoH
  probes. If both are blocked, the verdict is `uncertain` — never a false "no leak".
- **Scanning all interfaces.** `ip addr` / `ifconfig` / `Get-NetIPAddress` show virtual
  and VMware interfaces. Always follow the default route's device.
- **`ping -i` below 0.2 on Linux** (non-root floor is 200ms). `reliability.rs` shells
  out to `ping` on Linux/macOS; on Windows it sends ICMP echoes via `IcmpSendEcho2`
  (no `ping.exe`, no ~1s floor) — the seam is `system_ping`, `#[cfg]`-split.
- **Root.** Nothing requires or requests elevated privileges. `iw scan` is excluded for
  this reason, and so is macOS `wdutil info` (now sudo-only).
- **`system_profiler SPAirPortDataType` on the default path.** It takes ~7s (it scans
  for nearby networks; `-detailLevel mini` doesn't help). It's the macOS *slow* Wi-Fi
  read — only run when `wifi_detail` is set, which by default tracks the speed check so
  the cost is hidden. The *fast* read (`ipconfig getsummary`, SSID + encryption) is
  always safe to run. See
  [2026-08-26-macos-wifi-without-airport.md](docs/decisions/2026-08-26-macos-wifi-without-airport.md).
- **Expecting the macOS SSID.** macOS 15+ redacts it for any CLI without a Location
  Services grant; `wifi_info` returns `ssid: None`, `ssid_hidden: true`, and encryption
  is still read. Never treat a missing SSID as "not on Wi-Fi".
- **Proprietary speed-test services, unless no open-source option covers the need.**
  Ookla EULA §14 prohibits automated use; fast.com is Netflix's closed service. See
  [open-source-only](docs/decisions/2026-08-02-open-source-only.md) and the narrow
  fallback in [2026-08-24-ookla-permitted-as-fallback.md](docs/decisions/2026-08-24-ookla-permitted-as-fallback.md).
- **Active scanning for topology.** Passive ARP cache only. See
  [passive-topology](docs/decisions/2026-08-02-passive-topology.md).
- **Committing a `rust-toolchain.toml`.** The Windows GNU-toolchain override is
  machine-local on purpose — pinning it would force the GNU toolchain on Linux/macOS
  contributors.
- **A system-OpenSSL dependency.** `reqwest`/`tokio-tungstenite` use `native-tls`
  (SChannel on Windows, Secure Transport on macOS, system OpenSSL only on Linux). Keep
  it that way — verify with `cargo tree` after touching an HTTP/TLS dependency.

## Documentation index

- [README.md](README.md) — public-facing overview, installation, and usage
- [PLAN.md](PLAN.md) — the original (TypeScript-era) implementation plan; the general
  parameters (checks, scoring model, report shape) still hold, but `docs/specs/` is the
  authoritative behavior contract and this file describes the current architecture
- [docs/specs/](docs/specs/) — what the system must do, in Given-When-Then scenarios;
  cite scenarios by `<slug>#S<n>` from tests
  - `topology-default-route-precondition`, `dns-leak-detection`,
    `captive-portal-detection`, `reliability-check-resilience`, `risk-scoring`,
    `wifi-info-detection`
- [docs/decisions/](docs/decisions/) — why key architectural and technology choices
  were made; read before changing a dependency, adding a check, or adding a platform
  - [2026-08-02-open-source-only.md](docs/decisions/2026-08-02-open-source-only.md) — MIT/Apache/ISC only; why Ookla and fast.com are excluded
  - [2026-08-02-passive-topology.md](docs/decisions/2026-08-02-passive-topology.md) — why no active scanning; what passive ARP gives us
  - [2026-08-02-technology-stack.md](docs/decisions/2026-08-02-technology-stack.md) — original (TS) runtime-dependency rationale
  - [2026-08-02-dns-leak-detection.md](docs/decisions/2026-08-02-dns-leak-detection.md) — why DoH, why Cloudflare+Google only, why `uncertain` beats a false negative
  - [2026-08-24-dns-leak-address-family-matching.md](docs/decisions/2026-08-24-dns-leak-address-family-matching.md) — why only IPv4-vs-IPv4 pairs are comparable
  - [2026-08-24-cloudflare-speedtest-not-node-compatible.md](docs/decisions/2026-08-24-cloudflare-speedtest-not-node-compatible.md) — why the speed test is a hand-rolled NDT7 client
  - [2026-08-24-ookla-permitted-as-fallback.md](docs/decisions/2026-08-24-ookla-permitted-as-fallback.md) — the narrow exception to open-source-only
  - [2026-08-25-passive-notice-terminal-only-in-json.md](docs/decisions/2026-08-25-passive-notice-terminal-only-in-json.md) — dropping the passive-ARP notice from terminal output (proposed)
  - [2026-08-25-save-off-by-default.md](docs/decisions/2026-08-25-save-off-by-default.md) — why `--save` is opt-in
  - [2026-08-25-configurable-speed-duration.md](docs/decisions/2026-08-25-configurable-speed-duration.md) — why `--speed-duration`/`-q`/`--quick` exist
  - [2026-08-25-rust-rewrite-technology-stack.md](docs/decisions/2026-08-25-rust-rewrite-technology-stack.md) — every Rust crate vs its alternative and the TS dep it replaces
  - [2026-08-26-rust-becomes-canonical-implementation.md](docs/decisions/2026-08-26-rust-becomes-canonical-implementation.md) — Rust is canonical; TS moved to `typescript-archive`
  - [2026-08-26-rename-to-pubnet-tools.md](docs/decisions/2026-08-26-rename-to-pubnet-tools.md) — `conncheck` → `pubnetchk` / crate `pubnet-tools`
  - [2026-08-27-windows-platform-support.md](docs/decisions/2026-08-27-windows-platform-support.md) — GNU toolchain (still current); PowerShell probe mechanism (**superseded**)
  - [2026-08-28-windows-probes-via-win32-api.md](docs/decisions/2026-08-28-windows-probes-via-win32-api.md) — Windows probes call the Win32 API directly (`windows-sys`); no PowerShell/netsh/ping.exe; why the fixture corpus was dropped
  - [2026-08-26-macos-wifi-without-airport.md](docs/decisions/2026-08-26-macos-wifi-without-airport.md) — `airport` was removed in macOS 15/26; fast `ipconfig getsummary` (SSID+encryption) + opt-in slow `system_profiler` (channel+signal); SSID is Location-Services-gated
- [docs/context/](docs/context/) — observed network behavior and domain background;
  read when debugging a check that misbehaves on a specific network
  - [network-behavior.md](docs/context/network-behavior.md) — live recon findings (captive portals, Quad9 blocking, nmcli quirks, VMware interfaces)
  - [dns-hardening.md](docs/context/dns-hardening.md) — what the DNS findings mean; how much TLS protects against a hostile resolver
  - [nat-traversal.md](docs/context/nat-traversal.md) — how Tailscale punches through NAT; DERP relay fallback
  - [tailscale-wireguard-handshake.md](docs/context/tailscale-wireguard-handshake.md) — WireGuard Noise_IKpsk2 handshake walkthrough
