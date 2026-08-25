# DNS Hardening: What conncheck's Findings Are Telling You

conncheck's security check reports on the DNS resolver a network handed you, but it
doesn't fix anything — that's a deliberate scope boundary (see
[docs/decisions/2026-08-02-dns-leak-detection.md](../decisions/2026-08-02-dns-leak-detection.md)).
This doc is where "now what do I do about it" lives instead: background on why the
resolver you're using matters, and how to actually change it.

## The risk of inheriting whatever DNS a network hands you

Every device that joins a network gets a DNS resolver via DHCP, almost always without
being asked. On public WiFi, that resolver belongs to whoever runs the network — the
coffee shop, the hotel, the airport — not to you.

- **Metadata exposure.** The resolver sees every hostname you look up, even though the
  actual traffic is TLS-encrypted. That's a fairly complete picture of what you're
  doing, handed to a stranger by default.
- **Tampering.** An untrustworthy resolver can answer with whatever it wants — ad
  injection, typo-redirects, or in a targeted attack, redirecting a specific domain
  somewhere malicious. (Captive portals do a *legitimate* version of this — redirecting
  everything until you accept the portal — which is exactly why detecting the malicious
  version by content alone is hard; see the mandatory-failure-scenario discussion in
  [docs/specs/captive-portal-detection.md](../specs/captive-portal-detection.md).)
- **Silent VPN bypass.** A misconfigured split-tunnel can leave DNS pointed at the
  local network's resolver even while a VPN "protects" everything else.
- **No transport privacy.** DHCP-assigned resolvers are almost always plain port 53,
  unencrypted.

### How much of this does TLS actually cover?

Most of the day-to-day risk, but not all of it — worth being precise rather than
alarmist:

**Protected:** ordinary HTTPS to an established site. A hijacked resolver pointing you
elsewhere can't produce a working connection without a certificate that validates for
that domain, signed by a CA your system trusts. Without that, the connection fails
loudly rather than silently succeeding.

**Not protected:**
- **Plain HTTP** — still used by old devices, IoT, some app/API traffic. No cert at
  all, so a hijacked resolver is a direct man-in-the-middle there.
- **SSL-stripping on first contact** — a domain without HSTS (or not HSTS-preloaded)
  can be downgraded to plain HTTP before a certificate is ever presented, if you reach
  it without typing `https://` explicitly.
- **Phishing via attacker-owned domains** — a certificate proves you're talking to
  whoever registered that domain, not that it's the domain you meant. DNS pointing you
  at a typosquat with its own legitimately-issued certificate defeats TLS entirely —
  this isn't circumventing certificate validation, it's routing you somewhere
  validation was never going to catch.
- **A compromised or coerced CA, or a malicious root certificate already on the
  device** — rare, but when it happens, DNS hijacking plus a "valid" certificate is a
  full man-in-the-middle even for HTTPS.

## Hardening: away from home (public/untrusted networks)

The reliable fix is a resolver that isn't the network's own — either a fixed public
resolver or your VPN's DNS, applied globally so it doesn't depend on remembering to
configure it per network.

On Linux with NetworkManager (conncheck's own target platform — see the top-level
`CLAUDE.md`), per-connection overrides (`nmcli connection modify ... ipv4.dns "..."`)
only help for networks you've already saved a profile for, which defeats the purpose
for a network you're joining for the first time. NetworkManager's **global DNS
override** applies to every connection instead:

```bash
sudo mkdir -p /etc/NetworkManager/conf.d
sudo tee /etc/NetworkManager/conf.d/global-dns.conf <<'EOF'
[global-dns-domain-*]
servers=1.1.1.1,8.8.8.8
EOF
sudo systemctl reload NetworkManager
```

Verify with `resolvectl status` on whatever network you're currently on — the active
link should show the configured servers regardless of what that network's DHCP
offered.

For encryption in transit too (not just choosing who you ask), pair this with DNS-over-TLS
via systemd-resolved — a larger configuration change (systemd-resolved needs to
actually own DNS resolution, which depends on how NetworkManager's `dns=` setting is
configured) and worth its own pass rather than folding in here.

## At home: a different trust model

At home, "the network" is your own router, so the calculus changes. The genuine
advantages of using your router as the DNS endpoint there:

- **Local hostname resolution** — routers typically resolve LAN device names. Point
  straight at a public resolver and you lose that.
- **Local ad-blocking / filtering** — Pi-hole, AdGuard Home, or router-integrated
  blocking only works if devices actually query it.
- **Custom internal records** — any self-hosted services with internal-only names only
  resolve through your own resolver.
- **Per-device parental controls** — router-level content filtering by device usually
  depends on devices using the router's DNS.

What it doesn't buy you is privacy from your ISP — if the router just forwards to
whatever DNS your ISP assigned, your ISP still sees every query. The setup that gets
you both: point devices at the router (for the local-network benefits above), and
configure the *router's own upstream* to a resolver you trust instead of your ISP's
default. Most consumer routers, and Pi-hole/AdGuard Home, support this directly — it's
not router-vs-public-resolver, it's router *and* public-resolver, each doing the part
it's actually good at.

## Cloudflare (1.1.1.1) vs. Google (8.8.8.8): which one

conncheck already treats these as two *independent* observers for leak detection
(hence using both, not two endpoints from the same operator — see
[docs/specs/dns-leak-detection.md](../specs/dns-leak-detection.md)). For choosing one
as your own everyday resolver, independent of conncheck's use of them:

**Privacy stance.** Cloudflare's is the more legally concrete commitment: third-party
KPMG-audited, published policy to not retain personally identifiable query data beyond
24 hours (kept briefly for debugging, then deleted), never sold to advertisers — this
was the explicit premise 1.1.1.1 launched under, in partnership with APNIC. Google's
public DNS privacy policy states queries aren't correlated with other Google
services/ads and permanent logs are anonymized, but Google's core business is
advertising, which is enough institutional reason for some people to prefer an
operator whose business model doesn't depend on data at all.

**Performance.** Both run massive global anycast networks with excellent latency in
practice. Independent benchmarks (e.g. DNSPerf-style trackers) often show Cloudflare
with a slight edge, but it's close and location-dependent — not a reason on its own to
pick one over the other.

**Protocol support.** Both support DoH and DoT and generally keep pace with new
standards; Cloudflare has tended to be an earlier mover (was among the first major
public resolvers with broad DoH support).

**The one property that actually matters for conncheck's own use:** independence from
each other, not which one is "better." The leak check's value comes specifically from
comparing two resolvers with no shared operator or infrastructure — if either were
swapped for another Cloudflare or Google endpoint, the comparison would prove nothing.
