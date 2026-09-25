# TP-Link extender bridged into an ISP gateway

**A snapshot, not a policy.** Observed on 2026-09-24 on a residential network, on a
Linux laptop, connected through a TP-Link extender to the ISP gateway captured in
[vantiva-gateway-linux](../vantiva-gateway-linux/README.md), a few minutes before that
capture. No captive portal. Vendors are inferred from MAC OUIs and WPS fields, not from
the devices' labels; model numbers are not yet known.

## Network at a glance

| Item | Observed |
|---|---|
| Access point | Likely TP-Link (BSSID OUI `1c:3b:f3`); its own SSIDs, separate from the gateway's |
| Security | nmcli: `WPA1 WPA2`. `iw scan dump`: CCMP only in both the WPA and RSN elements, PSK. Legacy-client compatibility, not TKIP |
| Band | 5 GHz, ch 36 |
| Subnet | The gateway's `10.0.0.0/24`; default route `10.0.0.1`, the gateway itself |
| DNS | The ISP's resolvers via DHCP, plus its search domain |
| IPv6 | Global addresses from the ISP's prefix (scrubbed to `2001:db8:ffff:ff01::/64`) |

## Topology: a transparent bridge

The extender does not show up in the client's topology at all:

- The default gateway is the ISP gateway, whose **own MAC** appears in `ip neigh`
  (stand-in `60:3d:26:00:00:02`, Vantiva OUI), so there's no MAC address translation.
- Other devices on the LAN keep their own vendor MACs (Sony, Roku), again not rewritten
  to the extender's MAC.
- The subnet and gateway match those seen when connected to the gateway directly, so
  there's no double NAT.

The only sign of the extender is the BSSID. Passive topology (ARP / `ip neigh`) sees the
same network from either side of it.

## Performance compared with the gateway directly

From `pubnetchk --json` on each network, minutes apart (not committed; the check output
is not a fixture):

| | Through extender | Gateway directly |
|---|---|---|
| Gateway RTT (avg / jitter) | 34.7 ms / 26.4 ms | 3.6 ms / 0.6 ms |
| `8.8.8.8` RTT (avg) | 74.8 ms | 32.8 ms |
| Download / upload | 21.8 / 21.8 Mbps | 448.7 / 97.1 Mbps |

The gateway round trip and its jitter suggest a wireless backhaul between extender and
gateway (likely; the extender's uplink wasn't observed directly). That hop is where
the latency and throughput are lost.

## `pubnetchk` findings

- `encryption: WPA2`. The WPA1 element isn't surfaced; since it's CCMP-only, that's fair.
- `dnsLeak: leaked` (25-point alert, Medium risk overall): the ISP's own resolver being
  compared with the DoH providers. Tracked in #74.

## Not yet known

The extender's model and firmware, and whether its backhaul is Wi-Fi or wired.
