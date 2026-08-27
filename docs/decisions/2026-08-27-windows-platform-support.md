---
template_version: 1.4.0
date: 2026-08-27
slug: windows-platform-support
status: superseded
decided_by: hampton
related: [2026-08-28-windows-probes-via-win32-api, 2026-08-25-rust-rewrite-technology-stack, 2026-08-02-passive-topology]
---

# Decision: Windows support via PowerShell `Get-Net*` cmdlets, on the GNU Rust toolchain

> **Superseded (2026-08-28) by
> [windows-probes-via-win32-api](2026-08-28-windows-probes-via-win32-api.md)** for the
> *probe mechanism* only — the Windows probes now call the Win32 API directly instead
> of shelling out to PowerShell / `netsh` / `ping.exe`. The **GNU-toolchain decision
> below still stands** and is restated in the successor's first Revisit-if bullet.

## Context

`pubnetchk` targeted Linux, then gained a macOS `PlatformProbe` implementation. The
`PlatformProbe` trait (`src/platform/mod.rs`) already isolates every OS-specific command
behind seven methods, so adding Windows is a matter of one more implementation plus the
`#[cfg]` wiring in `cli.rs` and the contract tests — no architectural change.

Two things had to be settled to do it:

1. **How to build on Windows at all.** The development machine (Windows 11, scoop-based,
   no Visual Studio) could not link the default `x86_64-pc-windows-msvc` target — there
   is no MSVC `link.exe`, and cargo picked up msys2's coreutils `link.exe` instead.
2. **Which commands each probe method runs.** Windows has no `ip`/`nmcli`/`resolvectl`.
   The candidates are the legacy console tools (`ipconfig`, `route print`, `netsh`,
   `arp`) or the PowerShell networking cmdlets (`Get-NetRoute`, `Get-NetIPAddress`,
   `Get-NetNeighbor`, `Get-NetAdapter`, `Get-DnsClientServerAddress`).

## Decision

- **Toolchain: GNU, not MSVC.** `rustup toolchain install stable-x86_64-pc-windows-gnu`
  plus `scoop install mingw` (the UCRT mingw-w64 build), and a directory-local
  `rustup override set` for this repo. The bundled `rust-mingw` linker is enough for a
  debug build, but `--release` needs `dlltool.exe` (some crates' build scripts
  generate import libraries), which only the full mingw-w64 provides — so
  `~/scoop/apps/mingw/current/bin` must be on `PATH`. The override is machine-local —
  no `rust-toolchain.toml` is committed, so Linux and macOS builds are untouched.
- **Probes: PowerShell `Get-Net*` / `Get-DnsClientServerAddress`, rendered with
  `Format-List`.** Each probe runs `powershell -NoProfile -NonInteractive -Command
  "<cmdlet> | Select-Object <fields> | Format-List"` and parses the `Key : Value` text.
- **WiFi is the exception: `netsh wlan show interfaces`.** There is no structured
  cmdlet equivalent, and its labels localize (see Revisit-if).
- **`ping` gets Windows-specific arguments** — see the dedicated bullet below.
- **`system_egress_ip` returns `None` on Windows** (same as macOS), so the DNS-leak
  verdict is `uncertain` rather than `clean`/`leaked`. `record` is unsupported and says
  so.

## Rationale

**GNU over MSVC.** MSVC is the more conventional Windows Rust target and the better
choice for a distributed binary, but installing the VS Build Tools C++ workload is
several GB, needs admin, and is a proprietary interactive installer — it can't be
scripted or lived-in the way the rest of this machine's toolchain (scoop, rustup,
msys2) is. The GNU toolchain plus `scoop install mingw` is two OSS package-manager
commands (GCC/binutils), and `native-tls` already resolves to SChannel on Windows
regardless of toolchain, so the "no system OpenSSL" property from
[the Rust stack decision](2026-08-25-rust-rewrite-technology-stack.md) holds either way.
The `--release` binary came out at **2.8 MB** and links only system DLLs (UCRT,
`bcryptprimitives`, `secur32`/`crypt32` for SChannel, `ws2_32`) — libgcc and
libwinpthread are statically linked by the UCRT mingw build, so it runs on any
Windows 10+ with no bundled runtime DLLs. (Smaller than the 5.0 MB Linux build,
mostly from SChannel replacing a bundled TLS stack.)

**`Get-Net*` cmdlets over `ipconfig`/`route print`/`netsh`.** The console tools'
output *localizes* — "Default Gateway" is "Standardgateway" on a German Windows — and
their formatting (column widths, section headers) is unstable across Windows versions.
The cmdlets' property names (`NextHop`, `InterfaceAlias`, `PrefixLength`,
`LinkLayerAddress`, `ServerAddresses`) are English on every Windows regardless of
display language, and `Format-List` is a flat, stable `Key : Value` shape — the same
kind of parse target as macOS's `route -n get default` and `scutil --dns`. The cost is
a PowerShell process launch per probe (~100–300ms); acceptable given every check is
already dominated by network round-trips.

**`netsh wlan` for WiFi despite the localization problem.** There is genuinely no
structured alternative — `Get-NetAdapter` reports that an interface is 802.11 but not
its SSID, authentication, channel, or signal. `netsh wlan show interfaces` is the only
source. Its labels are localized, so `parse_netsh_wlan` returns `None` on a non-English
system — which the security check already handles identically to an Ethernet
connection (no SSID block). A localized-label fallback is possible later if anyone
actually needs it.

**Windows `ping` arguments `[human]`.** `ping` has no portable flag set:

| | Linux | macOS | Windows |
|---|---|---|---|
| count | `-c 10` | `-c 10` | `-n 10` |
| fast interval | `-i 0.2` (0.2s) | `-i 0.2` | none — ~1s between echoes, fixed |
| `-i` means | interval | interval | **TTL** |
| per-reply timeout | `-W` | `-W` | `-w 2000` (ms) |

Passing the existing `-c 10 -i 0.2` to Windows `ping` would fail on `-c` and then set
TTL to 0. Windows therefore runs `ping -n 10 -w 2000 <host>`, and the reliability check
selects the argv with `#[cfg(windows)]`. Consequence: the reliability check takes ~10s
on Windows (10 packets × ~1s) versus ~2s on Linux. The three targets still run
concurrently, so wall-clock is ~10s, comparable to the speed check's default. Keeping
the packet count at 10 (rather than dropping it to keep the check fast) preserves the
jitter/loss statistics' comparability across platforms.

**Windows `ping` output is a third format**, parsed by `parse_windows_ping` in
`network.rs` (dispatched on the `Ping statistics for` / `Packets: Sent =` marker that
neither Unix format produces): per-reply `time=3ms` / `time<1ms` (no decimal, no space,
`<1ms` parsed as `0.0`), summary `Packets: Sent = N, Received = M`, and the
"Approximate round trip times" block omitted entirely on 100% loss.

## Stakeholders

Solo call — no other stakeholders consulted.

## Considerations / Revisit if

- **GNU toolchain + scoop mingw on PATH.** Today: debug builds link with the bundled
  `rust-mingw`, but `--release` needs `dlltool.exe` from `scoop install mingw` on
  `PATH`; the resulting binary is self-contained (system DLLs only). Override is
  machine-local. Revisit if: `pubnetchk` is distributed as a Windows binary and the
  build needs to run in CI without scoop — then either vendor `dlltool`, drop to a
  debug-profile release, or move to MSVC.
- **`netsh wlan` label localization.** Today: `parse_netsh_wlan` matches English labels
  only; a non-English Windows falls through to "no WiFi info". Revisit if: a real user
  reports missing SSID/encryption on a localized Windows — then either match the
  localized label set, or switch to the WLAN API via a `windows`-crate binding.
- **PowerShell launch cost.** Today: ~5 `powershell.exe` spawns per full run, absorbed
  by network latency. Revisit if: a `--only topology` fast path is added and the
  spawns become the dominant cost — then a single combined PowerShell script emitting
  all sections at once, or a `windows`-crate binding, replaces the per-probe launches.
- **`system_egress_ip` is `None` on Windows.** Today: DNS-leak verdict is always
  `uncertain` on Windows (and macOS). Revisit if: the DoH-based leak check is
  considered important enough on Windows — `Resolve-DnsName -Type TXT
  whoami.cloudflare.com` uses the system resolver and would supply the egress IP.
- **No connected-WiFi fixture exists.** Today: `parse_netsh_wlan` is written against
  Microsoft's documented format and tested against synthetic input plus the one real
  capture (wlansvc stopped → no match). Revisit: capture `netsh wlan show interfaces`
  on a Windows machine actually associated to an AP and add exact-value assertions —
  tracked in `tests/fixtures/NEEDED.md`.

## Consequences

- New file `src/platform/windows.rs` (`WindowsProbe` + parsers + unit tests against
  `tests/fixtures/ethernet-vmware-windows/`). `cli.rs`, `tests/{topology,security,
  reliability}.rs` gain a `#[cfg(target_os = "windows")]` arm.
- `tests/reliability.rs` was also fixed to construct a `PlatformProbe` (it was calling
  `check_topology(&exec_cmd)` against the current trait signature and could not
  compile on any platform).
- `tests/fixtures/capture.sh` gained a Windows branch (detected via `uname -s` =
  `MINGW*`/`MSYS*`/`CYGWIN*`); `.gitattributes` marks `tests/fixtures/**` as `-text` so
  `core.autocrlf` does not rewrite the CRLF line endings in captured Windows output.
- `reporter.rs` `dirs_home()` now falls back to `%USERPROFILE%` when `$HOME` is unset
  (native Windows shells), via a testable `home_from` helper.
- Building on this machine requires the one-time
  `rustup override set stable-x86_64-pc-windows-gnu` in the repo (documented in
  `CLAUDE.md`).
