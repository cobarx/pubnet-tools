---
template_version: 1.0.0
slug: wifi-info-detection
status: agreed
owner: hampton
date: 2026-08-26
related: [risk-scoring]
---

# Spec: Wi-Fi info detection

## Intent

pubnetchk tells the person running it what Wi-Fi network they are on — its name
(SSID), its encryption, and (when cheaply available) its channel and signal — so the
security score can weigh an open or WPA-only network and the report can name the
network the reader is looking at.

The load-bearing part is **encryption**: `risk-scoring` gives an open network
`security.wifi-open` (Alert, 40 pts). A run that cannot read encryption must not score
that network as if Wi-Fi were fine.

**Not in scope:** scanning for or listing other nearby networks; band steering / roaming
detail; per-platform command choice (that is a decision doc:
`docs/decisions/2026-08-26-macos-wifi-without-airport.md`); requesting OS permissions.

## Terms

- **Connected-Wi-Fi** — the default-route interface is a Wi-Fi interface with an active
  link.
- **Redacted SSID** — the OS reports that a Wi-Fi network is joined but withholds its
  name for privacy reasons (macOS 15+ gates the SSID behind Location Services
  authorization, which a plain CLI does not hold).
- **Fast path / slow path** — two ways to read Wi-Fi info that differ in cost. The fast
  path is effectively instant and yields SSID + encryption. The slow path additionally
  yields channel and signal but can take several seconds.
- **Detail requested** — the caller asked for the slow path. Default: on when the speed
  check is also running (its wall time hides the slow path), off otherwise; forced
  either way by `--wifi-detail` / `--no-wifi-detail`.

## Scenarios

### S1 — Connected Wi-Fi, name available

**Happy path.**

- **Given** a Connected-Wi-Fi interface whose SSID the OS discloses
- **When** the security check reads Wi-Fi info
- **Then** `ssid` is that name
- **And** `encryption` is the network's encryption (one of `WPA3`, `WPA2-Enterprise`,
  `WPA2`, `WPA`, `Open`)
- **And** the security check status is not `failed`

### S2 — Connected Wi-Fi, name redacted by the OS

**Edge — the macOS 15+ default.**

- **Given** a Connected-Wi-Fi interface whose SSID the OS withholds
- **When** the security check reads Wi-Fi info
- **Then** `ssid` is null
- **And** `encryption` is still the network's real encryption
- **And** a finding `security.wifi-ssid-hidden` (Info, 0 pts) is present, explaining the
  name was withheld and how to reveal it
- **And** the encryption-based finding from `risk-scoring` is still emitted (an open
  network with a hidden name is still `security.wifi-open`)

### S3 — Not on Wi-Fi

- **Given** a default-route interface that is Ethernet or a VPN tunnel
- **When** the security check reads Wi-Fi info
- **Then** `ssid` is null, `encryption` is `Unknown`, `channel` / `signal` are null
- **And** no `security.wifi-*` finding is emitted
- **And** the console renderer prints no `SSID:` or `Channel:` line

### S4 — Detail not requested

- **Given** a Connected-Wi-Fi interface
- **And** detail is not requested (e.g. `--no-speed`, or `--wifi-detail` was not passed
  while speed is off)
- **When** the security check reads Wi-Fi info
- **Then** `ssid` and `encryption` are populated as in S1/S2
- **And** `channel`, `frequencyMhz`, and `signalPercent` are null
- **And** the check does not pay the slow path's cost

### S5 — Detail requested

- **Given** a Connected-Wi-Fi interface
- **And** detail is requested
- **When** the security check reads Wi-Fi info
- **Then** `channel` is the current channel number when the OS reports one
- **And** `signalPercent` is derived from RSSI when the OS reports it
- **And** a slow-path failure still leaves S1/S2's `ssid` + `encryption` intact
  (channel/signal just stay null)

## Open questions

None outstanding.

## Done when

- [ ] `S1` holds on a network whose SSID is disclosed
- [ ] `S2` holds: redacted SSID → `ssid: null`, real `encryption`,
      `security.wifi-ssid-hidden` present, encryption finding unaffected
- [ ] `S3` holds: Ethernet/VPN → no Wi-Fi fields, no Wi-Fi findings, no renderer lines
- [ ] `S4` holds: without detail, channel/frequency/signal are null and the slow path
      is not run
- [ ] `S5` holds: with detail, channel/signal populate when the OS discloses them, and
      a slow-path failure does not clear `ssid`/`encryption`
- [ ] An open network still scores `security.wifi-open` whether or not its SSID is
      redacted

## Why this behavior

Apple removed the `airport` CLI and now gates the SSID behind Location Services, so on a
current Mac the previous single command returned nothing and encryption silently became
`Unknown` — an open café network scored zero Wi-Fi risk. Splitting the read into a fast
path (name + encryption, the score-critical facts) and an opt-in slow path (channel +
signal, informational) keeps scoring correct everywhere while only paying the multi-
second cost when another check is already spending that time. A redacted SSID is a
first-class outcome, not an error: the encryption is what the score needs, and the
finding tells the reader why the name is missing and how to get it.
