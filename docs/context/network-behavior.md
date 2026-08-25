# Network Behavior: Observed Constraints

Discovered during live recon on the target machine before implementation. These findings shaped concrete design decisions — not hypothetical edge cases.

## Captive / filtered networks break hostname resolution for ICMP

On Berkeley-Visitor (open public WiFi):

```
ping one.one.one.one  →  100% packet loss
ping 1.1.1.1          →  works
```

The network allows ICMP to numeric IPs but breaks DNS before the captive portal is dismissed. All reliability ping targets must use numeric IP addresses, not hostnames. Per-target failure must not abort the overall reliability check — use `Promise.allSettled`.

## Quad9 DoH is blocked on many public networks

`dns.quad9.net` and port 5053 are blocked on Berkeley-Visitor and other filtered networks. DNS leak detection uses only Cloudflare and Google DoH. See [dns-leak-detection decision](../decisions/2026-08-02-dns-leak-detection.md).

## nmcli reports Open WiFi as an empty security field

```
nmcli -t -f active,ssid,security dev wifi list
```

An open (unencrypted) network returns an empty string in the `security` column — not the string "Open" or "--". Empty last field after splitting on `:` = `Open`. This is a parser detail, not a bug.

SSIDs can contain colons — split terse nmcli output to at most 3 parts and take the last as security.

## resolvectl mode may be "foreign"

On systems where NetworkManager manages DNS directly, `resolvectl status` shows:

```
resolv.conf mode: foreign
```

This means `/etc/resolv.conf` is written by NetworkManager, not systemd-resolved. Parse the per-link block in `resolvectl status` for the active interface. Fall back to `/etc/resolv.conf` only if resolvectl returns no servers for that interface.

## iw scan requires root

`iw dev wlan0 scan` requires CAP_NET_ADMIN. conncheck runs as a non-root user. Use `nmcli` exclusively for WiFi information — it reads from NetworkManager without elevated privileges.

## VMware virtual interfaces appear in ip addr

`ip addr show` lists `vmnet1` and `vmnet8` alongside real interfaces. Always derive the active interface from `ip route show default` (the `dev` field), not by scanning all interfaces. The default route's interface is the one that matters.

## Working directory must not be the Google Drive Insync path

`/home/maxwell/Insync/...` is synced by Insync in the background. Running `npm install` there can trigger sync conflicts or file locking that corrupts `node_modules`. Always work in `/home/maxwell/Projects/ConnnectionChecker`.
