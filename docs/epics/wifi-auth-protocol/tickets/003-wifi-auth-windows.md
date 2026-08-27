---
template_version: 1.0.0
epic: wifi-auth-protocol
ticket: 003
slug: wifi-auth-windows
type: feature
points: 2
status: todo
tracker_ref: none
pr: none
related: [wifi-auth-protocol-detection#S1, wifi-auth-protocol-detection#S3, wifi-auth-protocol-detection#S4]
---

# Ticket 003: Windows probe — auth protocol from existing WLAN API read

## Goal

Populate `WifiInfo.auth_protocol` on Windows by adding a `classify_dot11_auth_protocol`
function that maps `wlanSecurityAttributes.dot11AuthAlgorithm` to `WifiAuthProtocol`.
The API call that reads this field already exists — this is a mapping change, not a new
call.

## Spike result (2026-08-27)

Open question #1 from the spec is resolved: `wlan_intf_opcode_current_connection` →
`wlanSecurityAttributes.dot11AuthAlgorithm` reports what the *current connection*
negotiated, not the AP's capability set. There is no `DOT11_AUTH_ALGORITHM` constant
for transition mode. `SaeTransition` is not achievable without `WlanGetNetworkBssList`
+ RSN IE parsing — deferred for v1. Windows reports `Psk` or `Sae`, never
`SaeTransition`. See the "Resolved questions" section of the spec.

## Scope

- **In:** New `classify_dot11_auth_protocol(DOT11_AUTH_ALGORITHM) -> WifiAuthProtocol`
  pure function in `src/platform/windows.rs`; call it in `wlan_info()` alongside the
  existing `classify_dot11_auth()` call (line 472); unit tests for the new mapping;
  remove the `auth_protocol: WifiAuthProtocol::Unknown` placeholder from ticket 001
- **Out:** `WlanGetNetworkBssList` / RSN IE parsing for transition-mode detection
  (deferred); any change to `classify_dot11_auth()` or the existing `WifiEncryption`
  mapping; any other field in the Windows probe

## Acceptance criteria

Per `wifi-auth-protocol-detection`:
- `S1` holds on a live WPA3-SAE network: Windows contract test returns
  `auth_protocol: SAE`
- `S3` holds on a live WPA2 network: Windows contract test returns
  `auth_protocol: PSK`
- `S4` holds when the WLAN API call fails or `wlan_info()` returns `None`: the
  auth_protocol field is `Unknown`, no panic
- Unit tests cover all handled `DOT11_AUTH_ALGORITHM` integer values

## Notes

The integer values (from the existing test comments and the Windows SDK, confirmed
against the live code at `src/platform/windows.rs:646`):
- 1 → `Open` (`DOT11_AUTH_ALGO_80211_OPEN`)
- 2–5 → `Unknown` (WEP-era; not worth a named variant — no modern public network
  uses WEP)
- 6 → `Enterprise` (`DOT11_AUTH_ALGO_RSNA` = WPA2-Enterprise)
- 7 → `Psk` (`DOT11_AUTH_ALGO_RSNA_PSK` = WPA2-PSK)
- 8 → `Enterprise` (WPA3-Enterprise)
- 9 → `Sae` (`DOT11_AUTH_ALGO_WPA3_SAE` = WPA3 Personal)
- 10 → `Owe` (Opportunistic Wireless Encryption)
- 11 → `Enterprise` (WPA3 192-bit Suite B)
- anything else → `Unknown`

The existing `classify_dot11_auth()` (which maps to `WifiEncryption`) stays
untouched. The new function is a second, independent mapping over the same integer.
Both are called at `wlan_info()` line 472 — the `dot11AuthAlgorithm` field is read
once and passed to both.
