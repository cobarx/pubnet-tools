# Runbook: the captive portal's sign-in page does not appear (Linux desktop)

**Symptom.** You join an open network (train, bus, hotel, airport). A phone on the same
network gets the sign-in page; the laptop gets nothing, or its sign-in prompt opens a
page that is not the portal.

Written for NetworkManager desktops (KDE Plasma, GNOME). First seen and fixed on
2026-09-24 on the Amtrak Thruway bus; see [Incidents](#incidents).

## Get online now

1. **Open a page that cannot be upgraded to HTTPS:** `http://neverssl.com`. A portal can
   only intercept plain HTTP, and this site has no HTTPS version for the browser to
   switch to. The portal should redirect it to the sign-in page.
2. If that page loads normally instead, the portal is not intercepting at all (already
   signed in, or no portal). If it does not load, go to [Diagnose](#diagnose).
3. If `pubnetchk` has run on this network, its JSON report holds the portal's address:
   `security.data.captivePortal.redirectLocation`. Open that.

## Diagnose

While connected to the network in question, work down the causes below; each has a
check you can run. Cause 1 is the most common on a desktop that has never shown a
portal prompt.

## Causes

| # | Cause | How to tell | Fix |
|---|---|---|---|
| 1 | **NetworkManager connectivity checking is off.** NM then calls any default route "full" connectivity and never "portal", so the desktop never offers to sign in. | `busctl get-property org.freedesktop.NetworkManager /org/freedesktop/NetworkManager org.freedesktop.NetworkManager ConnectivityCheckEnabled` prints `b false`. In `journalctl -u NetworkManager`, `CONNECTED_SITE` is followed by `CONNECTED_GLOBAL` within milliseconds (no HTTP check can finish that fast). | `busctl set-property … ConnectivityCheckEnabled b true` (same path and interface), Persists across reboots. Plasma's network settings also have a toggle; that is one way it gets switched off. |
| 2 | **The browser upgrades the check URL to HTTPS.** The desktop's "sign in" action opens NM's check URL (`http://ping.archlinux.org/nm-check.txt` on Arch); a browser with HTTPS upgrades on (Brave's default, Chrome/Firefox HTTPS-First/Only) requests it over HTTPS, which a portal cannot redirect. | The tab the prompt opens shows `https://ping.archlinux.org/nm-check.txt`, not the portal. Brave may open the portal in a second tab from its own detection. | Use the second tab, or step 1 above. Longer term: exempt the check host from HTTPS upgrades in the browser (Brave: Shields for `ping.archlinux.org`, HTTPS upgrade off). **Likely, not proven:** see incident notes. |
| 3 | **No check URI configured.** | `ConnectivityCheckAvailable` is `false`. | Add `[connectivity] uri=…` to a file in `/etc/NetworkManager/conf.d/` (Arch ships one in `/usr/lib/NetworkManager/conf.d/20-connectivity.conf`). |
| 4 | **The network's subnet overlaps a local one** (Docker `172.17.0.0/16`, VMware `vmnet*`). Portal traffic leaves by the wrong interface. | `ip -o -4 addr` shows another interface in the same range; `ip route get <gateway>` names a device other than the Wi-Fi one. | Stop the overlapping interface while signing in. |
| 5 | **A VPN, or DNS forced over HTTPS/TLS.** The portal relies on answering DNS or HTTP itself. | A `wg*`/`tun*`/`tailscale*` interface is up; Chrome-family secure DNS mode `secure`; Firefox `network.trr.mode` 3. | Disconnect the VPN and use automatic secure DNS until signed in. |
| 6 | **The portal's own host does not resolve.** | `getent hosts <portal host>` fails. | Usually cause 5; otherwise the network's walled garden is broken (try another device). |

## Incidents

- **2026-09-24, Amtrak Thruway bus.** Laptop got no prompt; Android did. **Cause 1**,
  confirmed: `ConnectivityCheckEnabled=false`, and the log showed `CONNECTED_GLOBAL`
  5 ms after `CONNECTED_SITE`. Turned on; on the next join NM stayed at "portal" and
  Plasma prompted. Clicking the prompt then opened `https://ping.archlinux.org/nm-check.txt`
  and, in a second tab, the portal: **cause 2**, likely. Brave's HTTPS upgrades are on
  (default mode, busy: its profile counts over a hundred upgraded navigations on some
  days) with no site exceptions. Not confirmed: whether Brave fell back to HTTP, and
  what that first tab actually displayed. To confirm next time: note what the first
  tab shows, and try again with an HTTPS-upgrade exception for `ping.archlinux.org`.

## Tracked elsewhere

- pubnetchk shows the portal's URL only in JSON: #59.
