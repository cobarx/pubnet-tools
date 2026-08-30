---
template_version: 1.0.0
epic: pubnet-android
ticket: 005
slug: networkfacts-and-ui
type: feature
points: 5
status: todo
tracker_ref: tbd
pr: none
related: [android-host-snapshot]
---

# Ticket 005: `NetworkFacts` collector + Compose skeleton screen

## Goal

The ticket that produces a running app: gather the `HostSnapshot` from Android
framework APIs, run the audit, and show the result.

## Scope

- **In:** `NetworkFacts.kt` — builds a `HostSnapshot` and serializes it to the
  camelCase JSON contract (`kotlinx.serialization`):
  - `ConnectivityManager.activeNetwork` → `LinkProperties`: `dnsServers`,
    `routes` (default route → gateway), `linkAddresses` (ip/prefix),
    `interfaceName`.
  - `NetworkCapabilities` transport → `interfaceKind`
    (`wifi`/`ethernet`/`vpn`/`other`).
  - `WifiInfo` (from `ConnectivityManager`/`WifiManager`): SSID, RSSI →
    `signalPercent` (`WifiManager.calculateSignalLevel`), frequency →
    `channel` + `frequencyMhz`; encryption via
    `WifiInfo.getCurrentSecurityType()` (API 31+), else `"Unknown"`.
  - ARP: parse `/proc/net/arp` if readable, else `[]` (Android 10+ frequently
    blocks it — topology still returns `ok`, just no neighbors).
  - No SSID when `ACCESS_FINE_LOCATION` is not granted → `ssid = null,
    ssidHidden = true`.
- **In:** `AuditViewModel.kt` — coroutine on `Dispatchers.IO`: gather facts →
  `runAuditJson(snapshotJson, optionsJson)` with `only = ["topology",
  "security"]` → parse the report JSON into `@Serializable` data classes
  mirroring the schema (`Report`, `CheckResult`, `Finding`, `Score`). Expose a
  `StateFlow<AuditUiState>` (`Idle` / `Running` / `Done` / `Error`).
- **In:** `MainScreen.kt` (Compose, Material 3) — a **Scan** button, a runtime
  permission request for `ACCESS_FINE_LOCATION`, a risk badge
  (`score.level` → Low/Medium/High with color), and a findings list grouped by
  check. Reliability and speed shown as a disabled "not yet on Android" row.
- **In:** a JVM unit test for the report-JSON parser against a committed sample
  report (reuse or adapt `pubnetchk`'s sample report).
- **Out:** the console renderer's exact three-section layout; charts; history /
  saved reports; reliability + speed (tickets 6–7).

## Acceptance criteria

- On a device/emulator joined to Wi-Fi: launch → grant location → tap **Scan** →
  within a few seconds the screen shows a risk badge, the topology facts
  (interface, gateway, IP/CIDR), and the security findings (Wi-Fi encryption,
  DNS servers, DoH probe results, captive-portal verdict).
- With location **denied**: the audit still completes; the security section
  shows encryption but no SSID; no crash.
- The in-app report JSON (logged via `Log.d`) matches the schema of
  `cargo run -p pubnet-tools -- --json --only topology,security` on desktop —
  same field names, same casing.
- `./gradlew :app:testDebugUnitTest` passes.

## Notes

Keep the UI deliberately thin — this is the skeleton. A later ticket brings it
up to the console renderer's Network/Security/Performance structure. The
`AuditUiState.Error` path matters: a missing default route (airplane mode,
Wi-Fi off) should render a clear message, not a stack trace — the engine already
returns `topology: skipped` for that, so surface `errors[]`.
