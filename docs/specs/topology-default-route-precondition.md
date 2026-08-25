---
template_version: 1.0.0
slug: topology-default-route-precondition
status: agreed
owner: hampton
date: 2026-08-24
related: [docs/decisions/2026-08-02-passive-topology.md]
---

# Spec: Topology default-route precondition

## Intent

conncheck reads the network's own topology (default interface, own IP, gateway, ARP
neighbors) passively, using only what the OS already knows, and this is the one check
every other check depends on for a gateway to test against. When there's no default
route at all, that absence has to be reported cleanly rather than surfacing as timeouts
in every other check.

**Not in scope:** anything about *how* topology is read (passive-only, no active
scanning — already decided; see the linked decision).

## Terms

- **Default route** — the OS's chosen outbound interface and gateway for traffic with
  no more specific route. Its absence means the host isn't meaningfully connected to
  any network.

## Scenarios

### S1 — A default route exists

**Happy path.**

- **Given** the host has an active default route
- **When** the topology check runs
- **Then** it reports the default interface, the host's own IP/CIDR on that interface,
  and the gateway address
- **And** the check status is `ok`

### S2 — No default route exists

**Failure.**

- **Given** the host has no default route
- **When** the topology check runs
- **Then** the check status is `skipped`
- **And** the check's data is null
- **And** the result states plainly that no default route was found

### S3 — Default route exists but the ARP cache is nearly empty

**Edge.**

- **Given** the host has an active default route
- **And** the OS's ARP cache contains few or no neighbor entries (a quiet network)
- **When** the topology check runs
- **Then** the neighbors list is empty or short, reflecting exactly what the OS has
  observed
- **And** the check status remains `ok` — a quiet ARP cache is expected, passive-only
  behavior, not a degraded result
- **And** the passive notice is present in the result regardless of how many neighbors
  were found

## Open questions

None outstanding.

## Done when

- [ ] `S1` holds: a default route yields full topology data and `ok` status
- [ ] `S2` holds: no default route yields `skipped`, null data, and a clear reason
- [ ] `S3` holds: a sparse ARP cache is reported as-is, still `ok`, still carrying the
      passive notice
- [ ] Every result — full or sparse — carries the passive notice verbatim: "Passive ARP
      cache — no active scan performed."
- [ ] A `skipped` topology result is what downstream checks (reliability — see
      `reliability-check-resilience#S4`) use to skip themselves rather than timing out
      against a nonexistent gateway

## Why this behavior

Topology is sequenced first specifically because everything else needs its gateway IP.
Treating "no default route" as a `skipped` precondition rather than a `failed` error
lets that absence propagate cleanly to reliability instead of every downstream check
independently discovering the same missing gateway through its own timeout.
