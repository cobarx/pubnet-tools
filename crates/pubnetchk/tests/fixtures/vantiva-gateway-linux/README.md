# Vantiva ISP gateway, WPA2-PSK

**A snapshot, not a policy.** Observed on 2026-09-24 on a residential network, on a
Linux laptop connected directly to the ISP gateway. No captive portal. The same
network, reached through an extender, is
[tplink-extender-linux](../tplink-extender-linux/README.md). Vendors are inferred from
MAC OUIs and WPS fields, not from the device's label; the model is not yet known.

This is the corpus's first capture of a secured, portal-free network on Linux. The
Amtrak captures are both open networks with captive portals.

## Network at a glance

| Item | Observed |
|---|---|
| Gateway | Likely an ISP-supplied gateway: Vantiva (formerly Technicolor) OUI `60:3d:26` on BSSID and LAN MAC; WPS reports manufacturer `Quantenna`, model `pearl` (the Wi-Fi radio, not the gateway) |
| Security | `WPA2`; `iw scan dump`: CCMP, PSK |
| Band | 5 GHz, ch 157 |
| Subnet | `10.0.0.0/24`, gateway `10.0.0.1` |
| DNS | The ISP's resolvers via DHCP, plus its search domain |
| IPv6 | Global addresses from the ISP's prefix (scrubbed to `2001:db8:ffff:ff01::/64`); router's EUI-64 link-local in `ip neigh` |

## Also in the scan

Several **hidden** BSSs on the gateway's two channels (11 and 157), one of them
`WPA1 WPA2 802.1X`. Likely the gateway's own extra virtual APs (an ISP's community
hotspot and its secured variant), going by the shared channels and signal strength;
their BSSIDs were scrubbed, so the OUI can't confirm it. The 802.1X one is a neighbour
in the scan, not the connected network, so the `wpa2-enterprise-linux` gap in
`NEEDED.md` stays open.

## `pubnetchk` findings

- `encryption: WPA2`, scored as the 5-point info finding.
- `dnsLeak: leaked` (25-point alert, Medium risk overall): the ISP's own resolver being
  compared with the DoH providers. Tracked in #74.
- Speed failed once with `401 Unauthorized` from M-Lab, then passed on the next run
  (448.7 down / 97.1 up Mbps). Tracked in #75.

## Not yet known

The gateway's model and firmware.
