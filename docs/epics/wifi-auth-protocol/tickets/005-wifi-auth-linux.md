---
template_version: 1.0.0
epic: wifi-auth-protocol
ticket: 005
slug: wifi-auth-linux
type: feature
points: 3
status: todo
tracker_ref: none
pr: none
related: [wifi-auth-protocol-detection#S1, wifi-auth-protocol-detection#S3, wifi-auth-protocol-detection#S4]
---

# Ticket 005: Linux probe — key-mgmt from `nmcli`

## Goal

Populate `WifiInfo.auth_protocol` on Linux by reading the
`802-11-wireless-security.key-mgmt` property from `nmcli`, which already runs
in the Linux probe for DNS info and other fields.

## Scope

- **In:** `nmcli` invocation to fetch `802-11-wireless-security.key-mgmt` for
  the connected interface; mapping of `nmcli` key-mgmt strings to
  `WifiAuthProtocol`; empirical fixture; unit tests against the fixture; remove
  the `auth_protocol: Unknown` placeholder from ticket 001
- **Out:** `iw dev link` / BSS IE parsing for transition-mode detection; Linux
  is not expected to expose `SaeTransition` in v1 — `nmcli` reports what was
  negotiated, not the AP's full capability set

## Acceptance criteria

Per `wifi-auth-protocol-detection`:
- `S1` holds on a WPA3-SAE network: Linux contract test returns
  `auth_protocol: SAE`
- `S3` holds on a WPA2 network: Linux contract test returns
  `auth_protocol: PSK`
- `S4` holds when `nmcli` does not return a `key-mgmt` value (e.g. not managed
  by NetworkManager, or on an open network with no security section): returns
  `Unknown`
- Unit tests parse at least one fixture per case (WPA2-PSK, SAE) with real
  captured output

## Notes

`nmcli -g 802-11-wireless-security.key-mgmt dev show <iface>` is the minimal
command. Known `key-mgmt` values from NetworkManager:
- `wpa-psk` → `Psk`
- `sae` → `Sae` (WPA3-SAE, possibly transition — nmcli does not distinguish)
- `wpa-eap` or `wpa-eap-suite-b-192` → `Enterprise`
- Empty / `none` on an open network → `Open`
- No output or error → `Unknown`

Run `tests/fixtures/capture.sh` on both a WPA2 and a WPA3 network to capture
real `nmcli` output. If SAE is not available in the test environment, add a
`NEEDED.md` entry for the SAE fixture.

The Linux probe already calls `nmcli` in several places — add this as a
targeted field fetch rather than re-parsing existing command output, since
`nmcli -g` is cheap and focused.
