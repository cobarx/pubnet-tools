---
template_version: 1.0.0
epic: pubnet-android
ticket: 001
slug: snapshot-probe
type: feature
points: 3
status: in-review
tracker_ref: tbd
pr: "26"
related: [android-host-snapshot]
---

# Ticket 001: `SnapshotProbe` + `HostSnapshot` in pubnet-platform

## Goal

Add a data-driven `PlatformProbe` implementation that answers every probe method
from a plain struct of pre-gathered facts, so a caller that cannot shell out
(the Android app) can still drive the engine.

## Scope

- **In:** `crates/pubnet-platform/src/platform/snapshot.rs` with:
  - `HostSnapshot` — `#[derive(Deserialize)]`, `#[serde(rename_all =
    "camelCase")]`. Fields mirror what `PlatformProbe` returns: `defaultRoute
    {gateway, device}?`, `interfaceAddr {ip, prefix}?`, `arpNeighbors
    [{ip, mac?, isGateway}]`, `wifi {ssid?, ssidHidden, encryption, channel?,
    frequencyMhz?, signalPercent?}?`, `dns {servers[], currentServer?}?`,
    `interfaceKind` (`"wifi"|"ethernet"|"vpn"|"other"`).
  - `SnapshotProbe { snapshot: HostSnapshot }` implementing `PlatformProbe`
    (`crates/pubnet-platform/src/platform/mod.rs`). Every method returns
    pre-fetched data with no I/O `.await`.
  - `system_egress_ip()` returns `None` unconditionally — Android cannot obtain
    it, so the DNS-leak verdict is `uncertain` there (same as macOS/Windows).
  - Encryption string → `types::WifiEncryption` mapping (reuse a helper from
    `network.rs` if one fits; otherwise a small `match` beside
    `WifiEncryption::as_str`).
  - `pub mod snapshot;` in `platform/mod.rs`, unconditional (no `#[cfg]`) — it
    is pure data and also useful as a test seam.
- **In:** `docs/specs/android-host-snapshot.md` (Given-When-Then), written first.
- **Out:** anything Android-specific; the FFI crate; collecting the snapshot
  (that is Kotlin, ticket 5).

## Acceptance criteria

- `SnapshotProbe` implements `PlatformProbe` and compiles on all three desktop
  targets and for `aarch64-linux-android`.
- Unit tests in `snapshot.rs` (`#[cfg(test)]`) deserialize a JSON fixture and
  assert each `PlatformProbe` output, each citing `// spec:
  android-host-snapshot#S<n>`.
- Degradation cases covered: absent `wifi` → `wifi_info` is `None`; `ssid: null,
  ssidHidden: true` → `WifiInfo` with `ssid: None, ssid_hidden: true`; empty
  `arpNeighbors` → `arp_neighbors` returns `[]`.
- `cargo test -p pubnet-platform` passes; `just clippy` clean.

## Notes

`HostSnapshot`'s JSON is a maintained contract shared with Kotlin — keep the
casing camelCase and keep it documented in the spec. `arpNeighbors[].mac` is
optional (INCOMPLETE ARP entries have no MAC). The Rust `types::ArpNeighbor` has
more fields (`state`, `device`, `vendor`) than the snapshot carries — fill
`state: "REACHABLE"` / `device: <iface>` / `vendor: None` or add the fields to
the snapshot; decide in the spec.
