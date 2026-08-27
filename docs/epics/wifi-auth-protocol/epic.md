---
template_version: 1.0.0
slug: wifi-auth-protocol
status: abandoned
owner: hampton
created: 2026-08-27
tracker_ref: none
related: [wifi-auth-protocol-detection, wifi-info-detection]
---

# Epic: Wi-Fi authentication protocol detection

## Goal

Surface the authentication protocol used on the connected Wi-Fi network — WPA2-PSK,
WPA3-SAE, or WPA2+WPA3 transition mode — and emit an informational finding when
transition mode is detected. The finding explains the security trade-off and the
"bad password on correct passphrase" driver-compatibility failure that transition-mode
APs trigger on some Windows adapters. See `docs/specs/wifi-auth-protocol-detection.md`
for the full behavior spec and `docs/context/wpa3-driver-compatibility.md` for the
incident that motivated it.

**Abandoned 2026-08-27.** The post-connection `auth_protocol` field is a valid
improvement but doesn't serve the diagnostic use case that motivated it — the
WPA3-SAE driver failure happens before connection, so `pubnetchk` can't help.
Superseded by a dedicated diagnostic binary that scans visible APs without
requiring a connection. See `docs/specs/pubnetwifi-scan.md`. The `auth_protocol`
field work may be folded into the new tool's connected-AP output in a later ticket.

This earns an epic rather than one PR because it requires coordinated changes across
three platform probes (Linux, macOS, Windows), the shared types, the security check,
and the renderer — each of which is independently reviewable.

## Scope

- **In:** `WifiAuthProtocol` enum; `auth_protocol` field on `WifiInfo`; Windows WLAN
  API probe for auth algorithm; macOS `ipconfig getsummary` WiFiAuth parsing; Linux
  `nmcli` key-mgmt parsing; `security.wifi-wpa3-transition` finding; renderer display
  of `auth_protocol`; contract tests on all three platforms; empirical fixtures for
  macOS and Linux
- **Out:** radio type (802.11ac vs 802.11ax); driver version detection; Windows
  transition-mode detection via BSS beacon IE parsing (deferred — see open question
  in the spec); any scoring-point change; changes to the existing `WifiEncryption`
  enum values

## Tickets

| # | Title | Type | Points | Status | Tracker | PR |
|---|---|---|---|---|---|---|
| 1 | Types + spec wiring | chore | 2 | todo | none | none |
| 2 | Security check finding + renderer | feature | 3 | todo | none | none |
| 3 | Windows probe: auth algorithm via WLAN API | feature | 2 | todo | none | none |
| 4 | macOS probe: WiFiAuth from `ipconfig getsummary` | feature | 3 | todo | none | none |
| 5 | Linux probe: key-mgmt from `nmcli` | feature | 3 | todo | none | none |

Total points: `13`

## Sequencing

- Ticket 1 (types) must land before all others — everything else imports
  `WifiAuthProtocol`.
- Tickets 3, 4, 5 (platform probes) are independent of each other; any order after 1.
- Ticket 2 (finding + renderer) depends on the types (1) and is more useful to review
  once at least one platform probe is done, but can technically land after 1 alone
  since it handles `Unknown` gracefully.
- Recommend: 1 → 3/4/5 in parallel → 2 last, so the full behavior is reviewable in
  the final PR.
