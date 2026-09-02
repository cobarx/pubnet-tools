---
template_version: 1.0.0
slug: android-host-snapshot
status: draft
owner: hampton
date: 2026-08-30
related: [topology-default-route-precondition, dns-leak-detection, wifi-info-detection]
---

# Spec: Android host snapshot

## Intent

An Android app cannot shell out to `ip` / `nmcli` / `resolvectl` — the facts
those commands provide are only reachable through Android framework APIs on the
Kotlin side. So the engine's `PlatformProbe` seam
(`crates/pubnet-platform/src/platform/mod.rs`) is fed a **`HostSnapshot`**: one
struct of pre-gathered facts, gathered once by the caller, that `SnapshotProbe`
answers every probe method from with no I/O.

`SnapshotProbe` is the mechanism behind the [pubnet-android
epic](../epics/pubnet-android/epic.md). It is not Android-specific in the code
(no `#[cfg]`): it is a pure data → `PlatformProbe` adapter, also usable as a
test seam. This spec fixes the snapshot's field contract and how a partial
snapshot degrades — the checks downstream (`topology`, `security`) already
tolerate missing data, and `SnapshotProbe` must feed them the same shapes a real
probe would.

**Not in scope:** how Kotlin gathers the facts (that is epic ticket 5); the FFI
encoding (JSON string, epic ticket 3); reliability and speed (they take no probe
data — reliability pings, speed hits M-Lab); BSS scanning (`scan_bss_list`
always returns `None` here — that is `pubnetdiag`'s job, Windows-only).

## Terms

- **Snapshot** — a `HostSnapshot` value: the caller's one-shot capture of the
  active network's facts. Deserialized from JSON with
  `#[serde(rename_all = "camelCase")]`.
- **Absent field** — a snapshot sub-object (`defaultRoute`, `interfaceAddr`,
  `wifi`, `dns`) that is `null` / omitted because the caller could not read it.
- **Redacted SSID** — the caller is on Wi-Fi but the OS withheld the network
  name (Android withholds it without an `ACCESS_FINE_LOCATION` grant, the direct
  analogue of the macOS 15+ Location-Services gate in `wifi-info-detection`).

## Snapshot shape

```jsonc
{
  "defaultRoute":  { "gateway": "192.168.1.1", "device": "wlan0" },   // or null
  "interfaceAddr": { "ip": "192.168.1.34", "prefix": 24 },            // or null
  "arpNeighbors": [
    { "ip": "192.168.1.1", "mac": "a4:2b:...", "isGateway": true }    // mac may be null
  ],
  "wifi": {                                                            // or null
    "ssid": "CoffeeWiFi",          // null when redacted or not on Wi-Fi
    "ssidHidden": false,           // true only when on Wi-Fi and name withheld
    "encryption": "WPA2",          // WPA3 | WPA2 | WPA2-Enterprise | WPA | Open | Unknown
    "channel": 6,                  // may be null
    "frequencyMhz": 2437,          // may be null
    "signalPercent": 72            // may be null
  },
  "dns": {                                                             // or null
    "servers": ["192.168.1.1"],
    "currentServer": "192.168.1.1"  // may be null
  },
  "interfaceKind": "wifi"          // wifi | ethernet | vpn | other
}
```

- `encryption` deserializes straight into `types::WifiEncryption` (the JSON
  spellings above are its serde names) — no string mapping in `SnapshotProbe`.
- `SnapshotProbe` fills the fields `types::ArpNeighbor` / `types::DnsResolverInfo`
  carry that the snapshot does not:
  - `ArpNeighbor.state` → `"REACHABLE"`, `ArpNeighbor.device` → the queried
    iface, `ArpNeighbor.vendor` → `network::lookup_mac_vendor(mac)`.
  - `DnsResolverInfo.link` → the queried iface, `DnsResolverInfo.source` →
    `DnsSource::ResolvConf` (nothing branches on `source`; Android's list comes
    from `LinkProperties`, closer to a resolver list than to `resolvectl`).
- `system_egress_ip()` always returns `None` — Android has no way to observe it,
  so the DNS-leak verdict is `uncertain` there (same as macOS/Windows, per the
  `PlatformProbe` doc and `dns-leak-detection`).

## Scenarios

### S1 — Full snapshot, on Wi-Fi

**Happy path.**

- **Given** a snapshot with `defaultRoute`, `interfaceAddr`, non-empty
  `arpNeighbors`, `wifi` with an `ssid`, and `dns`
- **When** the engine runs topology and security against `SnapshotProbe`
- **Then** `default_route()` is `Some(RouteInfo { gateway, device })` from
  `defaultRoute`
- **And** `interface_addr(device)` is `Some(AddrInfo { ip, prefix })`
- **And** `arp_neighbors(device, Some(gateway))` returns one `ArpNeighbor` per
  entry, `is_gateway` taken from the snapshot, `vendor` derived from `mac`
- **And** `wifi_info(device, _)` is `Some(WifiInfo)` with `ssid: Some(name)`,
  `ssid_hidden: false`, and `encryption` as given
- **And** `dns_info(device)` is `Some(DnsResolverInfo)` with `servers` /
  `current_server` as given
- **And** `interface_type(device)` is `InterfaceKind::WiFi`
- **And** `system_egress_ip()` is `None`

### S2 — Redacted SSID (no location grant)

- **Given** a snapshot whose `wifi` is present with `ssid: null`,
  `ssidHidden: true`, `encryption: "WPA2"`
- **When** the engine reads Wi-Fi info
- **Then** `wifi_info()` is `Some(WifiInfo { ssid: None, ssid_hidden: true,
  encryption: Wpa2, .. })`
- **And** the real encryption is still reported (an open network with a hidden
  name still scores `security.wifi-open`, per `wifi-info-detection` S2)

### S3 — Not on Wi-Fi

- **Given** `interfaceKind: "ethernet"` (or `"vpn"`) and `wifi: null`
- **When** the engine reads Wi-Fi info and interface type
- **Then** `wifi_info()` is `None`
- **And** `interface_type()` is `InterfaceKind::Ethernet` (resp. `Vpn`)
- **And** no `security.wifi-*` finding is emitted (unchanged check behavior)

### S4 — No default route

**Edge — Wi-Fi off / airplane mode.**

- **Given** a snapshot with `defaultRoute: null`
- **When** the engine runs topology
- **Then** `default_route()` is `None`
- **And** topology returns status `skipped` with an error "No default route
  found" (unchanged, per `topology-default-route-precondition`)
- **And** security still runs (it takes `iface: None` and probes DoH / captive
  portal regardless)

### S5 — ARP cache unavailable

**Edge — Android 10+ commonly blocks `/proc/net/arp`.**

- **Given** an otherwise full snapshot with `arpNeighbors: []`
- **When** the engine runs topology
- **Then** `arp_neighbors()` returns `[]`
- **And** topology status is still `ok` (a present `interfaceAddr` is what makes
  it `ok`; neighbors are additive)

### S6 — Address unavailable

- **Given** a snapshot with `defaultRoute` present but `interfaceAddr: null`
- **When** the engine runs topology
- **Then** `interface_addr()` is `None`
- **And** topology status is `degraded` with an error naming the interface
  (unchanged check behavior)

### S7 — Missing MAC on an ARP entry

- **Given** an `arpNeighbors` entry with `mac: null` (an INCOMPLETE entry)
- **When** `arp_neighbors()` builds it
- **Then** the `ArpNeighbor` has `mac: None` and `vendor: None`
- **And** it is still returned (not dropped)

## Open questions

- Should `arpNeighbors[].state` cross the snapshot boundary (so an INCOMPLETE
  entry is visibly incomplete) rather than being forced to `"REACHABLE"`?
  Deferred until the UI needs it.
