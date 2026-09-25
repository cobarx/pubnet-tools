# Amtrak Thruway bus Wi-Fi (`Amtrak Thruway`)

**A snapshot, not a policy.** Observed on 2026-09-24, on one Amtrak Thruway connecting
bus (Sacramento toward Chico), on a Linux laptop, **before signing in to the portal**.
Other buses (and contractors) may differ, and the operator can change the portal or
its rules at any time. The raw capture in this directory was taken before sign-in
(see `meta.toml`); after sign-in there is only the first-attempt timeline below, no
capture yet. Vendor attributions are inferred from hostnames and address ranges, not
from any operator statement.

## Network at a glance (before sign-in)

| Item | Observed |
|---|---|
| SSID | `Amtrak Thruway`, **open**, seen on 2.4 GHz (ch 11) and 5 GHz (ch 36) |
| Client subnet | `192.168.1.0/24`, gateway `192.168.1.1` with a locally administered (randomised-style) MAC |
| DNS | `192.168.1.1` only. Resolves pre-sign-in, including the portal's host |
| Backhaul | Probably cellular: the DNS path's egress was in a US mobile carrier's IPv6 range |
| DoH (Cloudflare, Google) | Unreachable pre-sign-in, so `pubnetchk`'s DNS verdict is `uncertain` |
| Gateway ICMP | No reply pre-sign-in (`gatewayReachable: false`); unknown after sign-in |

## Captive portal

- **Cloud-hosted, not on the bus.** Every HTTP canary (NetworkManager's, Google,
  Firefox, Apple, KDE) gets `302` to
  `http://captive-2022.aio.cloudauth.net/swarm.cgi?opcode=cp_generate&orig_url=<hex>`.
  The host and the `swarm.cgi` path look like an Aruba Instant/Instant On cloud guest
  portal. `orig_url` is the original URL, hex-encoded (e.g. `…6e6d2d636865636b2e747874`
  decodes to `http://ping.archlinux.org/nm-check.txt`).
- No RFC 8910 portal option in DHCP.
- Android showed the sign-in page on its own; the Linux laptop did not until
  NetworkManager's connectivity checking was turned back on. See the runbook:
  [captive-portal-not-appearing.md](../../../../../docs/runbooks/captive-portal-not-appearing.md).

## After sign-in (first attempt, 2026-09-24 ~16:00 PDT)

Websites did not load after signing in. From the laptop's NetworkManager, wpa_supplicant
and kernel logs:

| Time | Event |
|---|---|
| 15:59:07 | Signed in; NetworkManager's check passes ("full") |
| 15:59:29 | **The access point deauthenticates the laptop** (`deauthenticated from <AP> (Reason: 3=DEAUTH_LEAVING)`, received, not local) |
| 15:59:32 | Re-associates with the **same** access point within 3 s |
| 15:59:32 to 16:00:20 | **DHCP gets no answer for ~48 s**: no address, so nothing loads. With no link DNS, systemd-resolved falls back to its FallbackDNS servers (Quad9 first) |
| 16:00:20 | Lease returns (same address); NM check passes again at 16:00:25 |

Also: the router's DNS (`192.168.1.1`) handled EDNS0/TCP poorly (systemd-resolved kept
downgrading its feature set), and DHCP hands out the search domain `myverizon`, which
fits a Verizon cellular router as the backhaul. The laptop kept one MAC throughout, so
the portal session should have survived the reconnect.

Likely reading (not confirmed): the portal kicks a client off once after sign-in to move
it into the authenticated role, and the router's DHCP was slow to answer the rejoin. It
is the network's behaviour, not the laptop's, and not (from this evidence) the cellular
signal.

A later attempt the same afternoon (north of Sacramento) got NetworkManager's
"limited connectivity": the router answered but its uplink did not, consistent with a
cellular dead zone on the route.

## Not yet known

After sign-in: allowed ports and protocols, content filtering, speed, and whether the
portal session is tied to the MAC (it will be, if the network follows the usual
pattern). Compare with the train, which differs in almost every respect:
[amtrak-train-linux](../amtrak-train-linux/README.md).
