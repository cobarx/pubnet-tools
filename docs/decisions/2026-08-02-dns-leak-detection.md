# Decision: DNS Leak Detection via DoH

**Date:** 2026-08-02
**Status:** accepted

## Context

DNS leak detection needs to compare what DNS resolver the system is using against what an external observer sees. On public WiFi, port 53 UDP/TCP is often blocked or intercepted. A VPN that leaks DNS sends queries through the host resolver instead of the tunnel — revealing the user's true network to the authoritative name server.

## Decision

Use DNS-over-HTTPS (DoH) on port 443 to two providers only: Cloudflare and Google. Compare egress IPs by /24 prefix. Quad9 is excluded.

## Rationale

**Why DoH instead of raw DNS queries:**  
Port 53 is frequently blocked or intercepted on captive/filtered networks. Port 443 HTTPS is almost never blocked — it would break the web. DoH runs over the same port as regular web traffic.

**Why `whoami.cloudflare.com TXT`:**  
The TXT record returns `remote_ip: <egress IP>` — the IP the DNS query arrived from. By querying this via both the system resolver and directly via DoH, we compare what resolver is being used without needing to know the resolver's IP in advance.

**Why Cloudflare + Google only (not Quad9):**  
Live recon on Berkeley-Visitor confirmed Quad9 DoH is blocked on many networks. Including a third provider that's commonly blocked adds a source of false `uncertain` results without improving detection. Two providers is sufficient: if both agree with the system, it's clean; if they disagree, it's leaked; if both are unreachable, it's `uncertain`.

**Why `uncertain` instead of `clean` when all probes fail:**  
A VPN that blocks DoH would appear clean if we defaulted to "clean on no data." `uncertain` is the honest answer — we couldn't verify. Never false-negative.

## Consequences

- Quad9 (`9.9.9.9`, `dns.quad9.net`) is excluded from all probe lists.
- If all DoH probes time out (e.g., captive portal blocks all HTTPS), verdict is `uncertain` with a note.
- Comparison is by /24 prefix, not exact IP — Cloudflare and Google use anycast so a small range of egress IPs is expected for the same resolver.
- axios timeout per probe: 8 seconds.
