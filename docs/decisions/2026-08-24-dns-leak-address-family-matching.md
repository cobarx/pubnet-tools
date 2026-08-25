---
template_version: 1.2.0
date: 2026-08-24
slug: dns-leak-address-family-matching
status: accepted
decided_by: hampton
related: [2026-08-02-dns-leak-detection]
---

# Decision: Compare DNS leak egress IPs only within matching address family

## Context

Building `dns-leak-detection.md`'s spec against real endpoints (this dev machine, which
has working IPv6) surfaced two problems with the original decision
(`2026-08-02-dns-leak-detection.md`):

1. The TXT record format is `remote_ip: <ip>` (colon-space), not `remote_ip=<ip>` as
   that entry's Rationale stated. Minor — a parsing fix.
2. The three egress-IP lookups came back in **different address families** on one live
   run: the system resolvectl query and the Google DoH probe both returned an IPv6
   egress IP; the Cloudflare DoH probe returned IPv4. Each lookup independently chose
   whichever transport it happened to use to reach its own upstream — nothing here
   indicates a leak, but it breaks the original decision's "compare by /24 prefix"
   plan outright: an IPv4 /24 has no defined relationship to an IPv6 address.

## Decision

**Only IPv4-vs-IPv4 pairs are ever comparable in v1.** A probe's egress IP is compared
against the system egress IP by the original decision's /24 rule only when both are
IPv4. Every other case — a family-mismatched pair, *or* a same-family IPv6-vs-IPv6
pair — is treated as **not comparable**: recorded as reachable, with its egress IP kept
in the result, but contributing neither an agreement nor a disagreement to the
clean/leaked verdict, the same way an unreachable probe is excluded. If no probe ends
up comparable at all, the verdict is `uncertain`, not `clean` — consistent with the
original decision's "never false-negative" principle.

IPv6-vs-IPv6 is deliberately **not** given its own prefix rule (e.g. /32) in v1 — see
Considerations below. Treating it as non-comparable rather than guessing a prefix width
is itself the decision, not an oversight: an invented IPv6 prefix would look exactly as
authoritative as the real /24 rule and would be exactly as wrong if the guess is off.

## Rationale

Three options were on the table:

- **Force IPv4 for every lookup** (pin resolvectl and the DoH HTTP clients to IPv4
  only). Rejected: resolvectl doesn't expose a way to force which protocol the
  upstream query uses, and forcing IPv4-only on the DoH HTTP clients would make
  conncheck itself less accurate on genuinely IPv6-native networks — exactly the kind
  of network this tool exists to audit.
- **Add a parallel IPv6 prefix-comparison rule** (e.g. /32 or /48, mirroring the IPv4
  /24). Rejected for now: unlike IPv4 /24 anycast routing (observed directly on this
  machine), there's no equivalent live evidence yet for what IPv6 prefix range
  Cloudflare's and Google's anycast IPv6 edges actually vary across. Inventing a number
  here would violate the same "never invent a threshold" principle the original
  decision leaned on for /24 — see [`docs/specs/dns-leak-detection.md`](../specs/dns-leak-detection.md).
- **Exclude family-mismatched probes from comparison** (chosen). No invented number,
  no loss of accuracy on IPv6 networks, and it degrades gracefully to `uncertain`
  rather than a false `clean` or a false `leaked` when a fair comparison isn't
  possible.

## Stakeholders

Solo call — no other stakeholders consulted.

## Considerations / Revisit if

- **No IPv6 anycast range is known for Cloudflare's or Google's DoH edges.** Today: zero
  live IPv6 comparisons have been observed (this machine's one live run happened to get
  an IPv4 answer from Cloudflare). Revisit if: real IPv6-vs-IPv6 comparisons are
  observed across multiple runs/networks and a stable prefix range becomes apparent.
  Then likely: add a genuine IPv6 prefix rule (probably /32, matching typical RIR
  allocations) instead of excluding same-family IPv6 comparisons from ever counting.
- **Excluding mismatched-family probes could mask a real leak in a narrow case**: a
  leak that happens to change only the address family (e.g. a VPN that tunnels IPv4 but
  leaks IPv6 DNS) would not be caught by this rule, since the mismatched probe is
  excluded rather than flagged. Today: no evidence this specific leak shape occurs in
  practice. Revisit if: a real captured leak of this shape is found. Then likely: a
  family mismatch between system and probe becomes its own flagged finding, distinct
  from a same-family disagreement.

## Consequences

- `docs/specs/dns-leak-detection.md` is amended in place (S1/S3/S4 gain a "same
  address family" qualifier; a new S5 covers the mismatched-family case) rather than
  superseded — the core DoH/Cloudflare+Google/uncertain-on-no-data decision is
  unchanged.
- The TXT parser must extract `remote_ip` regardless of whether it's rendered as
  `key: value` (resolvectl, Google DoH) or `\"key: value\"` (Cloudflare DoH's escaped
  quoting) — one regex tolerant of both, not per-provider parsing.
- `DohProbe` needs to carry the IP family (or it must be derivable from the egress IP
  string) so the comparison function can apply this rule without re-parsing.
