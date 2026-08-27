---
template_version: 1.0.0
epic: pubnetdiag
ticket: 002
slug: windows-bss-scanner
type: feature
points: 8
status: todo
tracker_ref: "16"
pr: none
related: [pubnetdiag-scan#S1, pubnetdiag-scan#S2, pubnetdiag-scan#S3, pubnetdiag-scan#S4, pubnetdiag-scan#S5, pubnetdiag-scan#S6]
---

# Ticket 002: Windows BSS scanner + RSN IE parser

## Goal

Implement the core scan capability for Windows: enumerate visible APs via
`WlanGetNetworkBssList` and parse each AP's RSN Information Element to detect
WPA2+WPA3 transition mode (both PSK and SAE AKM suites present).

## Scope

- **In:** `WlanGetNetworkBssList` call (new — not currently in `windows.rs`);
  raw IE byte slice extraction from each BSS entry; RSN IE parser
  (`tag=0x30`, version, group cipher, pairwise suites, AKM suite list); AKM
  detection: `00-0F-AC-2` = PSK, `00-0F-AC-8` = SAE; per-AP result type
  (`BssEntry`: SSID, BSSID, auth mode, band, channel, signal, is_connected);
  platform trait method `scan_bss_list() -> Vec<BssEntry>`; unit tests for the
  RSN IE parser against hand-crafted byte fixtures; contract test that
  `scan_bss_list` returns at least one entry on a live machine with Wi-Fi enabled
- **Out:** CLI display (ticket 3); repair logic (ticket 4); Linux/macOS stubs

## Acceptance criteria

Per `pubnetdiag-scan`:
- `S1`/`S2`: `scan_bss_list` returns entries with correct `auth_mode` — unit
  tests cover pure-WPA2, pure-WPA3-SAE, and transition-mode IE byte sequences
- `S3`: when `WlanOpenHandle` or `WlanEnumInterfaces` fails, `scan_bss_list`
  returns an empty `Vec` (not a panic); the CLI layer converts empty + no adapter
  to exit 2
- `S5`: the currently connected AP is identified in results (match BSSID against
  `wlan_intf_opcode_current_connection`)
- `S6`: APs with zero-length SSID are included with `ssid: None`
- Contract test: at least one BSS entry returned on a Wi-Fi-enabled Windows
  machine; shape check only (SSID is a string, auth_mode is a valid variant,
  signal is 0–100)

## Notes

RSN IE layout (all little-endian):
```
[0]    tag = 0x30
[1]    length
[2–3]  version = 0x0100
[4–7]  group cipher suite (OUI + type)
[8–9]  pairwise suite count
[10…]  pairwise suite list (4 bytes each)
[n]    AKM suite count (2 bytes)
[n+2…] AKM suite list (4 bytes each: 3-byte OUI + 1-byte type)
```

AKM types (OUI `00-0F-AC`): 2 = PSK, 4 = FT-PSK, 6 = FT-SAE, 8 = SAE, 9 = FT-SAE.
Transition mode = AKM list contains both type 2 (or 4) AND type 8 (or 6/9).

`WLAN_BSS_ENTRY` (from `windows-sys`) contains `IeOffset` and `IeSize` giving
the byte range of the IEs within the BSS list blob. The RSN IE is one of
potentially many IEs in that range — scan for tag `0x30`.

The parser is a pure function over a `&[u8]` slice — fully unit-testable without
a real adapter. Build it and test it before wiring up the WLAN API call.

Defensive bounds: clamp all counts from the IE bytes before iterating; a
malformed IE should return `AuthMode::Unknown`, not panic.
