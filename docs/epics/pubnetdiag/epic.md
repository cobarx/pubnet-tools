---
template_version: 1.0.0
slug: pubnetdiag
status: planned
owner: hampton
created: 2026-08-27
tracker_ref: "14"
related: [pubnetdiag-scan, wifi-auth-protocol-detection]
---

# Epic: pubnetdiag — Wi-Fi diagnostic tool

## Goal

A new binary, `pubnetdiag`, that scans visible Wi-Fi APs without requiring a
connection, flags WPA2+WPA3 transition-mode APs (the root cause of the
"bad password with correct credentials" failure class), and — with `--repair` —
applies the WPA2-PSK workaround and connects in one step.

This fills the gap `pubnetchk` cannot: that tool requires a successful connection
to run; the WPA3-SAE driver failure happens before connection. See
`docs/context/wpa3-driver-compatibility.md` and `docs/specs/pubnetdiag-scan.md`.

**v1 scope: Windows only.** macOS and Linux are deferred.

## Scope

- **In:** Cargo workspace restructure (shared platform lib); `pubnetdiag` binary;
  Windows BSS scanner (`WlanGetNetworkBssList`); RSN IE parser (PSK+SAE AKM
  detection); CLI with optional SSID arg; `--repair` flag with passphrase prompt;
  exit codes 0/1/2; `-f` reserved but not implemented
- **Out:** macOS scanning; Linux scanning; auditing a connected network (that is
  `pubnetchk`); anything requiring elevation; `pubnetchk` changes beyond the
  workspace restructure

## Tickets

| # | Title | Type | Points | Status | Tracker | PR |
|---|---|---|---|---|---|---|
| 1 | Cargo workspace restructure | chore | 5 | todo | #15 | none |
| 2 | Windows BSS scanner + RSN IE parser | feature | 8 | todo | #16 | none |
| 3 | CLI, output, and exit codes | feature | 3 | todo | #17 | none |
| 4 | `--repair` flag (WlanSetProfile + WlanConnect) | feature | 5 | todo | #18 | none |

Total points: `21`

## Sequencing

- Ticket 1 (workspace) must land first — all other tickets depend on the new
  crate structure.
- Ticket 2 (BSS scanner) must land before 3 and 4 — it provides the scan data
  both consume.
- Tickets 3 and 4 are independent of each other; either can follow ticket 2.
