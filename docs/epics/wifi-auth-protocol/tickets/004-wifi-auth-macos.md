---
template_version: 1.0.0
epic: wifi-auth-protocol
ticket: 004
slug: wifi-auth-macos
type: feature
points: 3
status: todo
tracker_ref: none
pr: none
related: [wifi-auth-protocol-detection#S1, wifi-auth-protocol-detection#S2, wifi-auth-protocol-detection#S3, wifi-auth-protocol-detection#S4]
---

# Ticket 004: macOS probe — WiFiAuth from `ipconfig getsummary`

## Goal

Populate `WifiInfo.auth_protocol` on macOS by parsing the `WiFiAuth` key from
`ipconfig getsummary <iface>` output, which already runs on the fast path.
macOS is the platform most likely to expose `WPA3_SAE_TRANSITION` explicitly,
making it the primary source of `SaeTransition` detection.

## Scope

- **In:** Parser for the `WiFiAuth` field in `src/platform/macos.rs`; mapping of
  macOS string values to `WifiAuthProtocol`; an empirical fixture capturing
  `ipconfig getsummary` output on a WPA3 and/or WPA3-transition network;
  unit tests against the fixture; remove the `auth_protocol: Unknown` placeholder
  from ticket 001
- **Out:** `system_profiler` (slow path) — if `WiFiAuth` is not available there,
  it stays `Unknown` for the slow path; no changes to Linux or Windows probes

## Acceptance criteria

Per `wifi-auth-protocol-detection`:
- `S1` holds on a WPA3-only network: macOS contract test returns
  `auth_protocol: SAE`
- `S2` holds on a WPA3-transition network: macOS contract test returns
  `auth_protocol: SAE-Transition`
- `S3` holds on a WPA2 network: macOS contract test returns `auth_protocol: PSK`
- `S4` holds when the `WiFiAuth` key is absent from the output: returns `Unknown`
- Unit tests parse at least one fixture per case (WPA2, WPA3) with real captured
  output

## Notes

**Resolve the open question first**: capture `ipconfig getsummary <iface>` on a
WPA3 and WPA3-transition-mode network on macOS 15+ and confirm the exact
`WiFiAuth` values (e.g. `WPA3 SAE`, `WPA3 SAE Transition`, `WPA2 Personal`).
Run `tests/fixtures/capture.sh` on the target network and commit the output.
If a WPA3-transition network isn't available, note the expected value in a
`tests/fixtures/NEEDED.md` entry.

The `WiFiAuth` key lives in the plist-like text output of `ipconfig getsummary`.
The existing macOS probe already parses this output for SSID and cipher — add
the `WiFiAuth` key to the same parsing pass rather than running the command again.

Known approximate macOS string values (verify empirically — these may differ by
macOS version):
- `WPA2 Personal` → `Psk`
- `WPA3 Personal` → `Sae`
- `WPA3 Transition` or `WPA3 SAE Transition` → `SaeTransition`
- `WPA2 Enterprise` or `WPA3 Enterprise` → `Enterprise`
- `Open` → `Open`
