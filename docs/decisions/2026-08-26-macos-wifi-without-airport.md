---
template_version: 1.4.0
date: 2026-08-26
slug: macos-wifi-without-airport
status: accepted
decided_by: hampton
related: [2026-08-28-windows-probes-via-win32-api, 2026-08-25-rust-rewrite-technology-stack]
---

# Decision: macOS Wi-Fi info without `airport` — fast `ipconfig`, slow `system_profiler`

## Context

`MacProbe::wifi_info` ran one command:
`/System/Library/PrivateFrameworks/Apple80211.framework/Versions/Current/Resources/airport -I`
and parsed its `SSID:` / `link auth:` / `channel:` / `agrCtlRSSI:` lines.

Two macOS changes broke this:

1. **`airport` is gone.** Deprecated in macOS 14.4, and by macOS 15 / 26 the binary is
   removed — the path does not exist. `exec_cmd` returns a spawn error, `.ok()?` turns
   it into `None`, and `check_security` falls back to `encryption: Unknown`. The
   consequence is not just a missing `SSID:` line: `wifi_findings(Unknown)` returns
   `[]`, so **an open network scored zero Wi-Fi risk on a Mac** (`risk-scoring` expects
   `security.wifi-open`, Alert, 40 pts).
2. **SSID/BSSID are permission-gated.** macOS 15+ withholds the SSID unless the calling
   process holds a CoreLocation authorization. A CLI that is not an app bundle cannot
   obtain one, so every access path returns `<redacted>`.

What each permission-free, non-root source actually returns (measured on macOS 26.4.1):

| Source | Wall time | SSID | Encryption | Channel / signal |
|---|---|---|---|---|
| `ipconfig getsummary <iface>` | instant | `<redacted>`\* | `Security : WPA2_PSK` | — |
| `system_profiler -json SPAirPortDataType` | **~7.6 s** | `<redacted>`\* | `spairport_security_mode_*` | `spairport_network_channel`, `spairport_signal_noise` |
| `wdutil info` | fast | `<redacted>`\* | yes | yes | — **requires `sudo`** |
| CoreWLAN (`CWInterface`) | instant | `<redacted>`\* | yes | yes | — needs an Obj-C binding |

\* the real value appears only if the terminal app has been granted Location Services
access. `-detailLevel mini` does not speed `system_profiler` up; the delay is the
network scan it always performs.

## Decision

`MacProbe::wifi_info(&self, iface: &str, detail: bool)` reads Wi-Fi info from **two
commands**, split by cost:

- **Fast path — always:** `ipconfig getsummary <iface>`. Parse `InterfaceType` /
  `LinkStatusActive` (bail if not an active Wi-Fi link), `SSID`, `Security`. `SSID`
  equal to `<redacted>` (or absent) → `ssid: None`, `ssid_hidden: true`.
- **Slow path — only when `detail`:** `system_profiler -json SPAirPortDataType`, parsed
  with `serde_json` into a small typed struct. From the interface whose `_name` matches
  `iface` and whose `spairport_status_information` is `spairport_status_connected`, take
  `spairport_current_network_information`: channel (leading integer of
  `spairport_network_channel`), signal (first dBm value of `spairport_signal_noise` →
  the existing `rssi_to_percent`), frequency (derived from channel + band), and — as a
  fallback only — encryption / SSID if the fast path could not read them.

Both `Security` (`ipconfig`) and `spairport_security_mode` (`system_profiler`) feed one
classifier that matches on substrings (`wpa3`, `enterprise` / `802.1x`, `wpa2`, `wpa`,
`wep`, `none`), so the two formats share code.

**`detail` is chosen in `cli.rs`:** default `true` when the speed check will run and
`--quick` was not passed — the speed test already spends ~10 s, and the slow path runs
concurrently inside the outer `tokio::join!`, so it adds no wall-clock time. Default
`false` under `--no-speed`, `--quick`, or an `--only` set without `speed`. Overridable
with `--wifi-detail` / `--no-wifi-detail` (the two conflict, rejected like
`--only`/`--no-<check>`).

Trait change: `wifi_info` gains `iface: &str` and `detail: bool`. Linux (`nmcli dev
wifi list` — one instant call with everything) and Windows (WLAN API) ignore both new
arguments. `WifiInfo.ssid` becomes `Option<String>` and gains `ssid_hidden: bool`
(always `false` off macOS). A redacted SSID adds a `security.wifi-ssid-hidden` finding
(Info, 0 pts) — see `docs/specs/wifi-info-detection.md`.

## Rationale

**Two commands, not one.** No single permission-free, non-root command returns SSID +
encryption + channel quickly. `ipconfig getsummary` is instant and carries the
score-critical facts (encryption, and SSID when allowed); `system_profiler` is the only
no-root source for channel/signal but is far too slow to run on every invocation. The
split lets the common `--no-speed` / `--quick` path stay instant while a full run —
which is already multi-second — gets the extra detail for free.

**Not CoreWLAN yet.** `objc2` + `objc2-core-wlan` would get channel/signal instantly
and mirror the Windows "call the platform API directly" decision, and it is the likely
end state. It is deferred because it is a new dependency family and a larger `unsafe`
surface, and the two-command approach clears the actual regression (encryption scoring)
today with zero new dependencies. Tracked as an issue.

**Not `wdutil`.** It now requires `sudo`, and the project takes no elevated privileges
(`docs/decisions/2026-08-28-windows-probes-via-win32-api.md` and CLAUDE.md).

**`system_profiler -json` over its text form.** The text output nests by indentation and
puts the SSID as a dictionary *key*; the JSON form parses with `serde_json` into a
typed struct and is stable across the fields we read. `serde_json` is already a
dependency.

**Redacted SSID is an outcome, not a failure.** `check_security` must not go `degraded`
just because macOS hid a name it was never going to share with a CLI. The encryption —
what the score needs — is still readable, and the `security.wifi-ssid-hidden` finding
tells the reader the name was withheld and that granting the terminal Location Services
access reveals it.

## Stakeholders

Solo call — no other stakeholders. Supersedes the `airport` mechanism from the original
macOS probe (PR #1); the fast/slow split and the `--wifi-detail` flags are new surface.

## Considerations / Revisit if

- **Channel/signal disappear under `--no-speed` / `--quick`.** Today: acceptable — they
  are informational and `risk-scoring` does not read them. Revisit if: a scoring rule
  starts depending on channel (e.g. flagging a crowded 2.4 GHz channel) — then the slow
  path can no longer be optional and CoreWLAN becomes necessary.
- **CoreWLAN binding.** Revisit if: the deferred issue is picked up, or a second macOS
  Wi-Fi fact is needed that only the framework exposes. That would collapse fast+slow
  back into one instant call and drop both shell-outs.
- **`ipconfig getsummary` output shape.** Today: `key : value` lines plus a nested DHCP
  dump we ignore; we read `InterfaceType`, `LinkStatusActive`, `SSID`, `Security`.
  Revisit if: a macOS release renames those keys — the contract test on a real Mac
  catches it.
- **`system_profiler` JSON key churn.** Today: `SPAirPortDataType[0]
  .spairport_airport_interfaces[].spairport_current_network_information`. Revisit if: a
  macOS release restructures it — again caught by the contract test.
- **`--wifi-detail` defaulting off the speed check is surprising.** Today: documented in
  `--help` and the spec; the flag overrides it. Revisit if: users report confusion —
  then always run the slow path and accept the cost under `--no-speed`.
- **Non-`en0` Wi-Fi interfaces.** Today: `iface` comes from the default route, and both
  commands are scoped to it. Revisit if: a machine with two Wi-Fi interfaces reads the
  wrong one.

## Consequences

- **`src/platform/macos.rs`** rewritten around `ipconfig getsummary` + `system_profiler
  -json`; `AIRPORT` const, `parse_airport`, `classify_airport_security` and their tests
  deleted. New parsers `parse_ipconfig_getsummary`, `parse_system_profiler_wifi`,
  `classify_wifi_security`, `channel_band_to_mhz`, all fixture-tested.
- **`PlatformProbe::wifi_info` signature** gains `iface: &str, detail: bool`.
  `src/platform/linux.rs`, `src/platform/windows.rs`, and the `MockProbe` in
  `src/checks/topology.rs` updated to match; Linux/Windows ignore the new args.
- **`WifiInfo`**: `ssid: String` → `Option<String>`; new `ssid_hidden: bool`.
  `src/platform/linux.rs` / `windows.rs` construct with `Some(...)` / `false`.
- **`check_security`** gains a `wifi_detail: bool` parameter; emits
  `security.wifi-ssid-hidden` (Info, 0 pts) when on Wi-Fi with no readable SSID; never
  goes `degraded` on a redacted SSID.
- **`src/cli.rs`**: `--wifi-detail` / `--no-wifi-detail` flags, `resolve_wifi_detail`,
  `RunAuditOptions.wifi_detail`.
- **`src/output/renderer.rs` / `html.rs`**: show `SSID:` / `Channel:` when on Wi-Fi even
  if the name is hidden; render the hidden-SSID hint.
- **Fixtures:** `tests/fixtures/home-wifi-macos/` gains `ipconfig_getsummary_en0.txt`
  and `system_profiler_-json_SPAirPortDataType.json` (real captures; SSID/BSSID already
  `<redacted>`, card MAC scrubbed). `capture.sh` drops the `airport` block and adds the
  two commands. `NEEDED.md` gains `ssid-visible-macos` and `open-wifi-macos`.
- **New spec:** `docs/specs/wifi-info-detection.md`.
- **Issue to file:** CoreWLAN binding (`objc2-core-wlan`) for instant channel/signal
  without the 7 s `system_profiler` call, and for a `ssid-visible-macos` /
  `open-wifi-macos` fixture (see `tests/fixtures/NEEDED.md`).
- `system_egress_ip` still `None` on macOS; `record` unaffected.
