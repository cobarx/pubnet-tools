# Amtrak Capitol Corridor on-train Wi-Fi (`YourTrainWiFi`)

**A snapshot, not a policy.** Everything here was observed on 2026-09-24, on one train
(Capitol Corridor 530, San Jose to Sacramento, portal unit in car 8802), between Oakland
and Martinez, on a Linux laptop. Other trainsets may run different equipment, and the
operator can change filtering, caps, or ports at any time; read every finding as "was,
on that train, that day". The raw capture in this directory was taken after
portal admission (see `meta.toml`). Vendor attributions below
are inferred from MAC OUIs, cookie names, and asset paths, not from any operator
statement.

## Network at a glance

| Item | Observed |
|---|---|
| SSID | `YourTrainWiFi`, **open** (no encryption), 2.4 GHz (channels 1, 6, 11 seen) |
| Access points | Extreme Networks (OUIs `48:9B:D5`, `94:9B:2C`), several per train. Each radio also beacons five hidden WPA/WPA2 BSSIDs (crew/operations networks, presumably) |
| Client subnet | `10.6.0.0/21`, gateway `10.6.0.1` (OUI: VRmagic), DHCP lease **600 s** |
| Portal host | `10.6.0.2` (OUI: duagon, a rail-comms vendor), serves `yourtrainwifi.com` |
| DNS | `10.6.0.1` only; search domain `yourtrainwifi.com` |
| IPv6 | None (link-local only; no v6 egress) |
| Path MTU | **1344** (1316-byte ICMP payload passes with DF, 1400 fails). Suggests a tunnel from the train to a hosted concentrator |
| Public egress | `15.204.24.167`, apparently an OVH-hosted range, not a cellular carrier's. All passengers share it |
| DHCP portal hint | No RFC 8910 captive-portal option (114) in the lease |

## Captive portal

- Platform: Nomad Digital passenger portal (`nomad_portal` session cookie,
  `/assets/modules/...` layout, "powered by" footer), themed for Capitol Corridor.
- `http://10.6.0.1/` returns `302` to `https://yourtrainwifi.com/connecttoweb`, which
  bounces through `/en/connecttoweb-cc` to `/en/index-cc` once the device is admitted.
  TLS is a real public cert: `*.yourtrainwifi.com`, Network Solutions DV, valid to 2027-02-11.
- After admission all four OS captive probes pass (Google `generate_204`, Firefox,
  Apple, Microsoft), so `pubnetchk` correctly reports no portal. **Gap:** the
  pre-admission splash (the terms/accept flow and how the probes are answered before
  acceptance) was not captured, because this laptop was already admitted. To capture
  it next time, run `capture.sh` and the probe curls before tapping "connect", or use
  the portal's Disconnect button and rejoin.
- No terms-of-use or acceptable-use policy is linked from the admitted page.
- **Usage cap:** the portal carries a per-session data cap. It warns at 80%
  (`usageThreshold = 80`) and then throttles ("Continue browsing with limited
  bandwidth"). `GET /api/v1/connection/data-usage` returned `[]` for this session
  and the page showed "Usage: -- / --", so the actual cap size is unknown.

## What traffic was allowed (for developers, 2026-09-24)

| Traffic | Result |
|---|---|
| ICMP to internet (`8.8.8.8`, `1.1.1.1`, `github.com`) | Allowed; high jitter and some loss |
| ICMP to gateway `10.6.0.1` | **Dropped** (100% loss). `10.6.0.2` answers |
| `tracepath` / TTL-expired | Works (the hops show the tunnel into OVH) |
| TCP egress | **Open on every port tested** via portquiz.net: 21, 22, 23, 25, 80, 110, 143, 443, 465, 587, 993, 995, 1194, 1723, 3000, 3306, 3389, 5060, 5222, 5432, 5900, 6379, 6881, 8080, 8443, 9418, 25565, 27017, 51413 |
| TCP/UDP 53 to outside resolvers | **Blocked** (8.8.8.8 and 1.1.1.1 time out on both). Only the train resolver works |
| DNS on a non-53 port (OpenDNS `:5353`) | Allowed, so plain DNS can bypass the train resolver |
| DoH (Cloudflare, Google) | Allowed |
| QUIC / HTTP/3 (UDP 443) | Allowed (Cloudflare and Google both negotiate h3) |
| NTP (UDP 123) | Allowed |
| SSH | Allowed (`ssh -T git@github.com` authenticates) |
| Proxy | No transparent HTTP proxy seen: no `Via` header was added, and the public IP is the same over HTTP and HTTPS |

### Content filtering

Filtering is **DNS-only**. The train resolver answers blocked names with a sinkhole,
`45.54.28.15`, which serves a bare "Website Filtered" page over HTTP (HTTPS fails,
since the sinkhole has no cert for the name). Reaching the real IP (from DoH, or by
`--resolve`) returns the origin's own response, so there is no IP- or SNI-level blocking.

| Category | Tested | Result |
|---|---|---|
| Adult | pornhub.com, xvideos.com | Sinkholed |
| Piracy / torrent indexes | thepiratebay.org, 1337x.to, nyaa.si | Sinkholed |
| Imageboards | 4chan.org | Sinkholed |
| BitTorrent tracker | tracker.opentrackr.org | Resolves; HTTP 200 |
| BitTorrent ports | 6881, 51413 (TCP) | Open |
| Video/music streaming | YouTube, googlevideo, Netflix, Twitch, Hulu, Disney+, TikTok, Spotify | Resolves normally, not blocked (throttling after the usage cap is the practical limit) |
| Social | Facebook, Reddit, Discord | Not blocked |
| VPN / anonymity | nordvpn.com, mullvad.net, torproject.org | Not blocked (sites); VPN protocols untested beyond TCP 1194/1723 being open |
| Gambling | draftkings.com | Not blocked |

Other names resolve to different but legitimate CDN addresses than DoH gives, which
is normal geo/anycast variation, not interception.

## Performance (one sample, Richmond area, `pubnetchk --quick`)

Download 1.7 Mbps, upload 3.4 Mbps, NDT7 latency 170 ms, jitter 140 ms. Pings: 8.8.8.8
averaged 196 ms (64 to 340 ms), 1.1.1.1 averaged 378 ms with 30% loss. Expect this to
swing with cellular coverage along the route.

## How `pubnetchk` scores it

Scored **High (110)**. Open Wi-Fi (40) is fair. However, **"Gateway unreachable" (30) plus
100% gateway loss (10) is a false alarm**: the gateway just drops ICMP while the
internet is reachable. The DNS-leak verdict is `uncertain` because the system-resolver
egress it sees is IPv6 (`2607:f740:21:90::2`, the train resolver's upstream) and the
probes are compared IPv4-to-IPv4 only. The DNS sinkhole filtering is invisible to it.

## Traveler information

### Where the train is and when it arrives

The schedule itself is not stored here; these are ways to look it up live.

**On board (no internet needed, served by the train):**

- `GET https://yourtrainwifi.com/api/gps`: `{"Latitude","Longitude","JSON":{latitude,
  longitude, altitude, heading, speed, fix, timestamp}}`.
- **MQTT over secure WebSocket**: `wss://yourtrainwifi.com:9001/mqtt`, subprotocol
  `mqtt`, **no credentials**. Subscribe to `rtpiPortalJourney/#`. Messages are retained,
  so you get the full state as soon as you subscribe:
  - `journey`: train number, route, origin/destination with planned and estimated times
  - `current-status`: next station, **arrival platform, and exit side**
  - `station-list`: every stop with `arrivalSchedule`/`arrivalForecast`/
    `departureSchedule`/`departureForecast` (UTC ISO) and a `nextStation` flag
  - `situation`: e.g. `leaving_station`
  - `vehicles`: the consist (car numbers and order). `window.designation` on the portal
    page (here `8802`) is the car whose Wi-Fi unit you're on
  - Separate topic `operationalMessage`: the portal's service-message ticker
- The portal's "Live Journey" panel renders the same data in a browser.

**Before boarding or on board (public internet):**

- Amtraker v3, an open, unofficial API over Amtrak's own tracking data:
  - `GET https://api-v3.amtraker.com/v3/trains/<number>`: per-stop `schArr`/`arr`/`dep`,
    `status` (Departed/Enroute), `platform`, plus `lat`/`lon`/`velocity`
  - `GET https://api-v3.amtraker.com/v3/stations/<code>` (e.g. `SAC`, `OKJ`)
  - `GET https://api-v3.amtraker.com/v3/trains` returns every active train (~1 MB, slow
    on the train link). To find which train you're on, match `lat`/`lon` against
    `/api/gps`; this trip matched train 530 to within 0.2 km.
  - Its ETAs agreed with the on-board feed (Sacramento 13:49 vs 13:50 forecast).
- Amtrak's official Track Your Train (amtrak.com) and the Capitol Corridor site carry
  the same data for humans.

### Café Car menu

- Current menu (effective 2026-06-12):
  `https://www.capitolcorridor.org/wp-content/uploads/2026/06/CafeCarMenu-6-12-2026.pdf`
- Landing page: `https://www.capitolcorridor.org/cafe-car/`. It sits behind a Cloudflare
  bot challenge, so it works in a browser but `curl` gets 403. The menu PDFs themselves
  download fine with `curl`.
- **Stale trap:** the stable-looking `https://www.capitolcorridor.org/cafecarmenu/Cafe-Car-Menu.pdf`
  still serves the November 2025 menu. Go through the landing page for the current PDF.
- The on-board portal links to the café page but does not host the menu.
