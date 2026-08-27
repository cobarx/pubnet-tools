---
template_version: 1.0.0
epic: wifi-auth-protocol
ticket: 002
slug: wifi-auth-finding
type: feature
points: 3
status: todo
tracker_ref: none
pr: none
related: [wifi-auth-protocol-detection#S1, wifi-auth-protocol-detection#S2, wifi-auth-protocol-detection#S3, wifi-auth-protocol-detection#S4, wifi-auth-protocol-detection#S5]
---

# Ticket 002: Security check finding + renderer

## Goal

Emit `security.wifi-wpa3-transition` (Info, 0 pts) when `auth_protocol` is
`SaeTransition`, and display `auth_protocol` in the console renderer's Security
section alongside the existing encryption line.

## Scope

- **In:** Finding emission logic in `src/checks/security.rs` keyed on
  `WifiAuthProtocol::SaeTransition`; finding copy (title + detail explaining the
  trade-off and the driver-compatibility failure mode); renderer output line in
  `src/output/renderer.rs`; `auth_protocol` field added to `SecurityData` in
  `src/types.rs`; contract test assertion that `auth_protocol` is a valid enum
  variant in `tests/security.rs`
- **Out:** How `auth_protocol` gets its value — that's tickets 3–5; any changes
  to `WifiEncryption` or existing findings

## Acceptance criteria

Per `wifi-auth-protocol-detection`:
- `S2` holds when `auth_protocol == SaeTransition`: finding is present with 0 pts
  and detail text covering the security trade-off and driver workaround
- `S4` holds when `auth_protocol == Unknown`: no finding emitted, no error
- `S5` holds: not-on-Wi-Fi behavior is unchanged
- The contract test (`tests/security.rs`) asserts that `auth_protocol` is one of
  the valid string values (shape check, not exact value — real networks vary)

## Notes

Finding copy should mention: AP accepts both WPA2 and WPA3 clients; some older
drivers fail the WPA3 handshake on such APs and show "bad password" with the
correct passphrase; workaround is forcing WPA2-PSK in the OS connection profile.
Keep it under ~120 words — this is an Info note, not a warning essay.

The renderer line should be: `Auth:     PSK` / `SAE` / `SAE (transition)` /
`Unknown` — label to be decided during implementation, following the existing
indentation and colon-alignment style. Don't show the line at all when not on
Wi-Fi (consistent with how `SSID:` is hidden in `wifi-info-detection#S3`).

`SecurityData` in `types.rs` currently mirrors `WifiInfo`'s fields flat — add
`auth_protocol: WifiAuthProtocol` there too. It will carry `Unknown` until the
platform probes land; that is correct and not a placeholder.
