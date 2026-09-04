---
template_version: 1.0.0
epic: pubnet-android
ticket: 011
slug: cellular-performance-and-tabs
type: feature
points: 8
status: planned
tracker_ref: tbd
pr: none
related: [android-host-snapshot, cellular-network-facts]
---

# Ticket 011: Cellular performance advice + dual-network tabs

## Goal

On a slow **mobile-data** connection, tell the user what's limiting it and what
they can do about it; and when the phone holds Wi-Fi **and** cellular at once,
let them audit and see both.

Builds on [ticket 010](010-cellular-network-facts.md) (which adds
`InterfaceKind::Cellular` + a `mobile` snapshot sub-object). Background and the
full fact/advice inventory: [`docs/context/cellular-mobile-network.md`](../../context/cellular-mobile-network.md).

## Part A — performance advice (the "Tips" surface)

- **In:** a non-scored advisory list. A mobile connection being slow is **not a
  security risk** — it must not touch the Low/Medium/High score. Options:
  - a new `advisories: [{ id, severity: info|suggestion, title, detail }]` array
    on the `Report` (engine-side, schema-additive), **or**
  - Kotlin-only, derived from the `mobile` snapshot + the existing check results.
  Decide in a short decision doc.
- **In:** the rules from `docs/context/cellular-mobile-network.md` — weak signal,
  strong-signal-but-slow, 3G/HSPA, roaming, Data Saver, carrier throttle
  (bw-estimate ≈ measured), `NOT_CONGESTED` false, carrier DNS → Private DNS,
  DoH-blocked → VPN, narrow-channel, high RTT / loss.
- **In:** `NetworkFacts` — read `SignalStrength` (RSRP/RSRQ/SINR), serving-cell
  band + bandwidth from `getAllCellInfo()` (needs the location grant we already
  ask for), `dataNetworkType`, `getRestrictBackgroundStatus()` (Data Saver),
  `NetworkCapabilities` metered / congested / bandwidth-estimate.
- **In:** on a **metered** connection, the NDT7 speed test defaults to a short
  run (≈5 s/direction) or is opt-in, with a "~N MB of data" note — the full run
  is ~10–40 MB. Wire `speedDurationSecs` / a "quick" flag from the UI.
- **In:** suppress the topology gateway row + its ping on cellular (no LAN
  gateway).
- **In:** `READ_PHONE_STATE` handling — only needed for `dataNetworkType` on
  Android ≤ 10; request lazily, degrade to `networkType: null` if denied.
- **Out:** anything privileged (band lock, QoS policy, forcing network type);
  IMSI-catcher detection (its own epic).

## Part B — dual-network tabs

- **In:** when `ConnectivityManager` reports **both** an active Wi-Fi and an
  active cellular network, gather a `HostSnapshot` for **each** (Wi-Fi is the
  default route; cellular is reachable via `cm.allNetworks` /
  `getNetworkCapabilities` filtering by transport) and run the audit twice.
  - Reliability/speed bound to a specific `Network`: `network.bindSocket()` /
    `network.openConnection()` so the cellular probe actually goes over cellular
    while Wi-Fi is default. `net_icmp`'s socket needs
    `Network.bindSocket(fd)` — a new bound-socket variant.
- **In:** `MainScreen` grows a tab row (`PrimaryTabRow`) — "Wi-Fi" / "Cellular"
  — shown only when both are connected; single-pane otherwise. Each tab is a
  full result set (risk badge + Network/Security/Performance + tips).
- **Out:** more than two transports at once (VPN-over-Wi-Fi-over-cellular
  stacking); auditing a network the phone isn't the default on beyond Wi-Fi +
  cellular.

## Decision docs (write first)

- `docs/decisions/<date>-mobile-advisories-not-scored.md` — where the advice
  lives (engine `advisories[]` vs Kotlin), and why it stays out of the score.
- `docs/decisions/<date>-per-network-audit.md` — binding probes to a specific
  `Network`; how `net_icmp` / reqwest / tokio-tungstenite each bind; snapshot
  gathering per transport.

## Acceptance criteria

- On cellular only: the report shows carrier / network type / signal / band, a
  "Tips" list with at least the applicable advice, no gateway row, and (metered)
  a short speed test with a data-cost note.
- On Wi-Fi + cellular simultaneously: two tabs, each a complete audit; the
  cellular tab's ping/speed numbers differ from Wi-Fi's (proving the bind).
- Score/verdict is unchanged by any mobile advisory.
- Desktop `pubnetchk` unaffected (no `advisories[]` consumer required; renderer
  tolerates the new field).

## Notes

Part A is the higher-value half and is independently shippable. Part B (tabs +
per-network binding) is more involved — split into 011a/011b if Part A lands
first.
