---
template_version: 1.0.0
slug: dns-leak-detection
status: agreed
owner: hampton
date: 2026-08-24
related: [docs/decisions/2026-08-02-dns-leak-detection.md, docs/decisions/2026-08-24-dns-leak-address-family-matching.md]
---

# Spec: DNS leak detection

## Intent

conncheck tells the person running it whether their DNS queries are actually leaving
the network the way they expect, by comparing what their system's DNS resolver reports
against what independent DNS-over-HTTPS providers see. If the system resolver's answer
disagrees with the independent providers, DNS is leaking outside whatever path (VPN,
private resolver) the person believes it's taking.

**Not in scope:** fixing or preventing a leak; DNS-over-TLS or plain-port-53 probing;
any provider other than Cloudflare and Google (Quad9 is excluded — see the linked
decision).

## Terms

- **System egress IP** — the egress IP reported when the DNS query is made through the
  system's configured resolver.
- **Probe** — an independent DNS-over-HTTPS query made directly to a named provider
  (Cloudflare or Google), bypassing the system resolver, reporting its own egress IP.
- **Agree** — a probe's egress IP and the system egress IP are the same address family
  (both IPv4 or both IPv6), and fall in the same /24 prefix (IPv4 only — see S5). Anycast
  providers route to different edge IPs for the same effective resolver, so exact-IP
  comparison would produce false leaks.
- **Comparable** — a probe's egress IP and the system egress IP are both IPv4. An IPv6
  egress IP, from either side, makes that pair not comparable — see S5. (No IPv6 prefix
  rule exists yet; see the linked address-family decision for why.)

## Scenarios

### S1 — System resolver matches independent providers

**Happy path.**

- **Given** the system egress IP is known
- **And** the Cloudflare probe is reachable, comparable, and agrees with the system
  egress IP
- **And** the Google probe is reachable, comparable, and agrees with the system egress
  IP
- **When** the DNS leak check runs
- **Then** the verdict is `clean`
- **And** leaked is false

### S2 — Every probe is unreachable

**Failure.**

- **Given** the system egress IP is known
- **And** neither the Cloudflare probe nor the Google probe is reachable
- **When** the DNS leak check runs
- **Then** the verdict is `uncertain`
- **And** leaked is false
- **And** the verdict is never `clean` when no probe could be checked

### S3 — A reachable provider disagrees with the system resolver

**Edge.**

- **Given** the system egress IP is known
- **And** at least one reachable, comparable probe's egress IP does not agree with the
  system egress IP
- **When** the DNS leak check runs
- **Then** the verdict is `leaked`
- **And** leaked is true
- **And** the disagreeing probe's provider and egress IP are recorded in the result

### S4 — One provider reachable, one blocked

**Edge.**

- **Given** the system egress IP is known
- **And** exactly one of the two probes is reachable
- **And** the reachable probe is comparable and agrees with the system egress IP
- **When** the DNS leak check runs
- **Then** the verdict is `clean`
- **And** the unreachable probe is recorded as unreachable, not as agreeing or
  disagreeing

### S5 — A reachable probe is not comparable to the system egress IP

**Edge.**

- **Given** the system egress IP is known
- **And** a reachable probe's egress IP is not comparable to it (either one of the pair
  is IPv6 — only IPv4-vs-IPv4 pairs are comparable in v1)
- **And** every other reachable probe (if any) is comparable and agrees
- **When** the DNS leak check runs
- **Then** the non-comparable probe is recorded as reachable, with its egress IP kept
  in the result
- **And** the non-comparable probe counts as neither agreeing nor disagreeing
- **And** if at least one other probe was comparable and agreed, the verdict is `clean`
- **And** if no probe was comparable, the verdict is `uncertain`, not `clean`

## Open questions

None outstanding.

## Done when

- [ ] `S1` holds: two agreeing, reachable probes produce `clean`
- [ ] `S2` holds: zero reachable probes produce `uncertain`, never `clean`
- [ ] `S3` holds: any disagreeing reachable probe produces `leaked`, with the
      disagreeing probe identified in the result
- [ ] `S4` holds: one reachable agreeing probe is enough for `clean`; the unreachable
      probe is marked unreachable, not silently dropped
- [ ] `S5` holds: a family-mismatched probe counts as neither agreement nor
      disagreement; `uncertain` if it's the only usable probe, `clean` if another probe
      was comparable and agreed
- [ ] Quad9 never appears in the probe list

## Why this behavior

A resolver that leaks queries outside an expected path (e.g., a VPN that doesn't tunnel
DNS) is invisible to the person using it unless something compares the resolver's
actual behavior against an independent view. Defaulting to `clean` when probes can't be
reached would hide exactly the networks most worth checking — ones that block or
intercept HTTPS to third parties. See
[docs/decisions/2026-08-02-dns-leak-detection.md](../decisions/2026-08-02-dns-leak-detection.md)
for why DoH-over-443 and the /24 comparison were chosen, and
[docs/decisions/2026-08-24-dns-leak-address-family-matching.md](../decisions/2026-08-24-dns-leak-address-family-matching.md)
for why S5 exists — a live dual-stack run returned an IPv4 answer from one probe and
IPv6 from the system resolver and the other probe, which the original /24-only design
didn't account for.
