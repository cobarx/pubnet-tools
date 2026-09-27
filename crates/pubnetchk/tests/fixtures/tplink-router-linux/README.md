# TP-Link router, café guest Wi-Fi, WPA2-PSK

**A snapshot, not a policy.** Observed on 2026-09-25, around 14:00 PDT, on a café's
guest network in Chico, CA, on a Linux laptop. Joined with the WPA2 password only; no
captive portal or splash page (see `meta.toml`). `Chico Coffee Shop` in
`nmcli_dev_wifi_list.txt` is a stand-in chosen for this capture, not the café's SSID.
Vendors are inferred from MAC OUIs, not from the device's label; the model is not yet
known.

This is the corpus's first capture of a secured, portal-free public network. The other
TP-Link capture, [tplink-extender-linux](../tplink-extender-linux/README.md), is a
residential extender bridged into another gateway's subnet; here the TP-Link box is
likely the router itself.

## Network at a glance

| Item | Observed |
|---|---|
| Router | Likely a TP-Link router acting as AP and gateway: TP-Link OUI `6c:5a:b0` on both the BSSID and the gateway's MAC, and the gateway's MAC is the BSSID + 1 |
| Security | `WPA2` (nmcli) |
| Band | 5 GHz, ch 149 |
| Subnet | `192.168.0.0/24`, gateway `192.168.0.1` |
| DNS | `192.168.0.1` only (the router); no search domain |
| Upstream resolver | The router's resolver egressed from `208.111.39.0/24`, registered to NetActuate (ARIN RDAP, `VR-SJC-CLOUD-03`), a hosting provider. Likely a third-party DNS service hosted there, not the ISP's own resolver; unconfirmed |
| IPv6 | Unique local addresses only (scrubbed to `2001:db8:ffff:ff01::/64`); no global IPv6 |

## Client isolation

Guest clients were not isolated from each other. An Apple device (OUI `f4:34:f0`) sent
router advertisements on the guest network: `ip neigh` marks it `router`, and it
advertised a route to a separate unique-local /64. Likely a HomePod or Apple TV acting
as a Thread border router, going by the Apple OUI and a route to a second ULA prefix;
unconfirmed. Either way, one client's link-local traffic reached another. `pubnetchk` has
no finding for this; tracked in #84.

## `pubnetchk` findings

- `encryption: WPA2`, scored as the 5-point info finding.
- `dnsLeak: leaked` (25-point alert, Medium risk overall): the router's upstream
  resolver compared with the DoH providers. Tracked in #74.
- `captivePortal: detected false`, `generate_204` returned 204.
- Speed (NDT7): 91.4 down / 16.5 up Mbps, 35 ms latency.
- Reliability: gateway ~9 ms, Google and Cloudflare DNS ~51 and ~56 ms, jitter ~20 ms.

## Not yet known

The router's model and firmware, the café's ISP, and the upstream resolver's operator.
