---
template_version: 1.0.0
epic: pubnet-android
ticket: 010
slug: cellular-network-facts
type: feature
points: 3
status: deferred
tracker_ref: tbd
pr: none
related: [android-host-snapshot]
---

# Ticket 010: Cellular / mobile-network facts in the snapshot + UI

## Goal

When the phone's active network is **cellular**, the audit should say so and show
what it can about the mobile link — instead of the current behaviour, where
`NetworkFacts` maps the cellular transport to `interfaceKind: "other"`, skips the
Wi-Fi block entirely, and the UI shows a bare "interface / gateway / IP" with no
context.

## Background — current behaviour

- `NetworkFacts.interfaceKind()` only recognises `TRANSPORT_VPN` / `TRANSPORT_WIFI`
  / `TRANSPORT_ETHERNET`; `TRANSPORT_CELLULAR` falls through to `"other"`.
- `HostSnapshot.wifi` is only populated `if (kind == "wifi")`, so on cellular the
  security section shows no link details at all.
- `types::InterfaceKind` (`crates/pubnet-platform/src/types.rs`) is
  `WiFi | Ethernet | Vpn | Other` — there is no cellular variant, and this enum
  is in the report JSON schema (`topology.interfaceKind`), shared with desktop.

Topology, DNS, DoH and captive-portal checks already work transport-agnostically
(they run off `LinkProperties` / plain HTTP(S)); this ticket is about *labelling*
and *mobile-specific facts*, not new checks.

## Scope

- **In:** `crates/pubnet-platform/src/types.rs` — add `InterfaceKind::Cellular`
  (serde `"cellular"`; `as_str` → `"Cellular"`). This is a **report-schema
  change** — bump nothing, but note it: `topology.interfaceKind` gains a value.
  The three desktop probes may keep returning their existing kinds (a laptop on
  a USB modem is rare); document that `Cellular` is Android-originated for now.
- **In:** `docs/specs/android-host-snapshot.md` — a new `mobile` sub-object and
  a scenario (S8?) for "on cellular": `mobile { carrier?, networkType?,
  roaming?, metered? }`, `wifi: null`, `interfaceKind: "cellular"`, ARP `[]`,
  topology still `ok`/`degraded` from the address.
- **In:** `crates/pubnet-platform/src/platform/snapshot.rs` — a `SnapshotMobile`
  field on `HostSnapshot`; `SnapshotProbe` carries it through (no new
  `PlatformProbe` method needed — expose via a `mobile_info()` default or fold
  into an existing accessor; decide in the spec).
- **In:** `NetworkFacts.kt` —
  - `interfaceKind()` recognises `TRANSPORT_CELLULAR` → `"cellular"`.
  - a `mobileFacts()` builder from `TelephonyManager`:
    `networkOperatorName` (carrier), `dataNetworkType` → a friendly string
    (LTE / NR / HSPA…), `isNetworkRoaming`, and metered state from
    `NetworkCapabilities.NET_CAPABILITY_NOT_METERED`.
  - **Permissions:** `getNetworkOperatorName()` is free; `getDataNetworkType()`
    needs `READ_PHONE_STATE` below API 30 (API 30+ allows it for the default
    subscription without). Add `READ_PHONE_STATE` to the manifest and request it
    at runtime *only* when on cellular; degrade to `networkType: null` if denied
    (mirror the location/SSID pattern).
- **In:** the report model + `MainScreen` — a "Mobile" line in the Network
  section (carrier · network type · roaming/metered badges) shown when
  `interfaceKind == "cellular"`; the Security section drops the Wi-Fi row and
  shows a short "on cellular — no local Wi-Fi exposure" note instead of blank.
- **In:** `crates/pubnetchk/src/output/renderer.rs` — make sure an unknown/new
  `InterfaceKind` renders sanely on desktop (it already uses `as_str()`).
- **Out:** IMSI-catcher / rogue-cell detection (needs privileged APIs, its own
  epic); band/ARFCN/cell-ID details; dual-SIM handling beyond the default
  subscription; any scoring change (cellular vs Wi-Fi risk weighting is a
  separate discussion).

## Acceptance criteria

- On a device with Wi-Fi off, on cellular data: launch → Scan → the Network
  section shows the carrier and network type (e.g. "Verizon · 5G NR"), the
  Security section shows DNS servers + DoH + captive-portal results and a
  "on cellular" note (no Wi-Fi row, no crash, no empty "SSID: hidden").
- `topology.interfaceKind` in the logged report JSON is `"cellular"`.
- With `READ_PHONE_STATE` denied: carrier still shows (it needs no permission),
  `networkType` is absent, audit completes.
- Desktop: `cargo test`, `cargo clippy`, the sample-report example, and the
  renderer are unaffected by the new `InterfaceKind` variant.
- `docs/specs/android-host-snapshot.md` scenario for cellular is implemented
  test-first in `snapshot.rs` (`// spec: android-host-snapshot#S<n>`).

## Notes

Motivated by on-device review of the ticket-5 skeleton — the app is Wi-Fi-shaped
and says nothing useful when the phone drops to LTE. Keep it factual: this tool
audits the *network you joined*; on cellular that's mostly "who's your carrier
and is DNS being messed with", not a security verdict.

This ticket is **facts only**. The performance-advice surface and the
Wi-Fi + cellular audit tabs are [ticket 011](011-cellular-performance-and-tabs.md);
the full fact/advice inventory is in
[`docs/context/cellular-mobile-network.md`](../../context/cellular-mobile-network.md).
