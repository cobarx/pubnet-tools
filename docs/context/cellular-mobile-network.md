# Cellular / mobile-network facts and advice (Android)

What an **unprivileged** Android app can learn about a mobile-data connection,
and what actionable advice it can give a user on a slow one. Background for
[pubnet-android ticket 010](../epics/pubnet-android/tickets/010-cellular-network-facts.md)
(facts into the snapshot) and
[ticket 011](../epics/pubnet-android/tickets/011-cellular-performance-and-tabs.md)
(the advice layer + dual-network tabs).

The audit's four checks are transport-agnostic and already run on cellular:
reliability (datagram ICMP ping), security (carrier DNS, DoH-blocked /
DNS-intercept, captive portal — some roaming partners have one), speed (NDT7).
This doc is about the **mobile-link facts** those checks don't cover, and turning
them into advice.

## What we can read

### `TelephonyManager` — no runtime permission

- `networkOperatorName` — serving carrier
- `simOperatorName` / MCC-MNC — home carrier (differs when roaming)
- `isNetworkRoaming` — roaming flag

### `TelephonyManager` — `READ_PHONE_STATE` (only needed on Android ≤ 10)

- `dataNetworkType` → **5G NR / LTE / HSPA+ / UMTS / EDGE / GPRS**. On Android 11+
  the no-arg call on the default subscription needs no permission.

### `SignalStrength` (API 28+ synchronous; earlier via a listener)

`getCellSignalStrengths()` → per-RAT `CellSignalStrengthLte` / `...Nr` / `...Wcdma`:

- `getDbm()` — **RSRP** (LTE) / **SS-RSRP** (NR): raw signal power
- `getRsrq()` — signal quality
- `getRssnr()` / `getSsSinr()` — signal-to-noise
- `getCqi()` — LTE channel-quality index
- `getLevel()` — 0–4 bars (the framework's own bucketing)

### `getAllCellInfo()` — needs `ACCESS_FINE_LOCATION`

(The app already requests this for the Wi-Fi SSID, so on a device that granted
it we get cell detail for free.)

Serving cell (`CellInfoLte` / `CellInfoNr` with `isRegistered() == true`):

- `CellIdentityLte.getEarfcn()` / `CellIdentityNr.getNrarfcn()` → the RF channel →
  the **band** (a small lookup table)
- `CellIdentityLte.getBandwidth()` (API 28+) — **channel bandwidth in kHz**
  (1.4 / 3 / 5 / 10 / 15 / 20 MHz for LTE)
- `CellIdentityNr.getBands()` (API 30+)
- `getPci()` / `getCi()` / `getTac()` — physical cell id, cell id, tracking area

Neighbour cells (`isRegistered() == false`) — count and their signal; a proxy
for "how contended is this site".

### `NetworkCapabilities` (the active network)

- `NET_CAPABILITY_NOT_METERED` — almost always **false** on cellular
- `NET_CAPABILITY_NOT_ROAMING`
- `NET_CAPABILITY_NOT_CONGESTED` (API 28+) — carrier's own congestion signal
- `NET_CAPABILITY_TEMPORARILY_NOT_METERED` (API 30+) — a carrier "free data" window
- `getLinkDownstreamBandwidthKbps()` / `getLinkUpstreamBandwidthKbps()` — the
  **carrier's bandwidth estimate**. Coarse and often stale, but instant and free
  — useful as a sanity check against a measured speed-test number.

### `ConnectivityManager.getRestrictBackgroundStatus()` (API 24+)

- **Data Saver** on/off (`RESTRICT_BACKGROUND_STATUS_ENABLED`)

### `LinkProperties`

Carrier DNS servers; interface name (`rmnet_data*`). No meaningful LAN gateway —
cellular is a point-to-point link, the "default route" gateway is typically a
`/32` or absent. Topology's gateway ping is not meaningful here.

## Turning facts into advice

| Observation | Advice |
|---|---|
| RSRP ≤ −110 dBm / 1 bar | Weak signal — move toward a window / outdoors / higher up. Weak signal caps throughput regardless of plan. |
| Strong signal **and** low throughput / high RTT | Coverage is fine → congestion or a plan throttle, not location. |
| `dataNetworkType` = 3G / HSPA, or NR→LTE fallback | Toggle Airplane mode ~10 s to force re-selection; may reattach to LTE/5G. |
| `isNetworkRoaming` | Roaming plans are often capped (128–512 kbps) and higher-latency; check the plan's roaming terms. |
| Data Saver enabled | Restricting background data — Settings → Network → Data Saver. |
| Carrier bw-estimate ≈ measured throughput, both low (~0.5 Mbps) | A plan throttle (hit the high-speed cap), not congestion — a speed test won't change it. |
| `NOT_CONGESTED` false | Carrier is signalling congestion right now — retry later / different spot. |
| Carrier DNS, DoH not blocked | Set Private DNS (`dns.google` / `one.one.one.one`) — faster lookups, avoids carrier DNS hijacking. |
| DoH to Cloudflare **and** Google both blocked | Carrier is intercepting DNS; Private DNS won't connect — a full VPN is the workaround. |
| Single low-band 5–10 MHz LTE carrier | Narrow channel — a different location may catch wider carrier aggregation. |
| ping RTT ≫ 60 ms LTE / 30 ms 5G, or loss > 2 % | Distant/contended cell or a silent 3G fallback. |

## What we cannot get

Actual throttling / QoS policy (QCI / 5QI, deprioritisation), forcing a band or
network type, or measuring throughput without spending data. The engineering
menu (`*#*#4636#*#*`) is being removed across OEMs.

## Consequences for the app

- The mobile advice is **not a security risk** — it must not feed the
  Low/Medium/High score. It belongs in a separate advisory surface, not
  `findings`.
- On a **metered** connection the NDT7 speed test (~10–40 MB, ~25 s) should
  default to a short run or be opt-in with a data-cost warning.
- The topology gateway row and its ping should be suppressed on cellular.
- A phone can hold Wi-Fi **and** cellular at once (Wi-Fi is the default route,
  cellular stays up for MMS / carrier services / fast-handover). Auditing both
  needs the UI to show two result sets — see ticket 011.
