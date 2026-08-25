---
template_version: 1.0.0
slug: reliability-check-resilience
status: agreed
owner: hampton
date: 2026-08-24
related: [docs/specs/topology-default-route-precondition.md]
---

# Spec: Reliability check resilience

## Intent

conncheck measures packet loss, latency, and jitter to three targets (the gateway, and
two external DNS servers) to tell the person running it whether the network itself is
reliable, not just reachable. One target being down — including the gateway — must
never stop the other targets from being measured and reported.

**Not in scope:** which targets are used or why (numeric IPs, not hostnames — see
`PLAN.md`'s "What to avoid"); how jitter is calculated.

## Terms

- **Target** — one of the three hosts pinged: the gateway, and two external numeric IPs.
- **Reachable** — a target that returned at least one reply out of the packets sent.
- **Gateway reachable / internet reachable** — two independent booleans: whether the
  gateway target replied at all, and whether *either* external target replied at all.

## Scenarios

### S1 — All three targets reachable

**Happy path.**

- **Given** the gateway, and both external targets, all reply to pings
- **When** the reliability check runs
- **Then** each of the three targets' results report their packet loss, latency, and
  jitter
- **And** gateway reachable is true
- **And** internet reachable is true
- **And** the check status is `ok`

### S2 — Gateway unreachable, internet reachable

**Failure.**

- **Given** the gateway target does not reply to any ping
- **And** at least one external target replies
- **When** the reliability check runs
- **Then** gateway reachable is false
- **And** internet reachable is true
- **And** the gateway target's own result still reports 100% packet loss rather than
  being omitted
- **And** the other two targets' results are still produced in full
- **And** the check status is `degraded`, not `failed`

### S3 — No target reachable

**Edge.**

- **Given** none of the three targets replies to any ping
- **When** the reliability check runs
- **Then** gateway reachable is false
- **And** internet reachable is false
- **And** all three targets' results are still produced, each reporting 100% packet loss
- **And** the check status is `degraded`, not `failed` — pings ran and produced real
  measurements, even though every measurement is bad news

### S4 — No gateway IP available to test against

**Edge.**

- **Given** the network topology check could not determine a gateway IP (no default
  route)
- **When** the reliability check is invoked
- **Then** it does not attempt to ping any target
- **And** the check status is `skipped`
- **And** the check's data is null

## Open questions

None outstanding.

## Done when

- [ ] `S1` holds: three reachable targets, `ok` status, both reachable flags true
- [ ] `S2` holds: one unreachable target does not suppress the other two; status is
      `degraded`
- [ ] `S3` holds: three unreachable targets still produce three real (bad) results;
      status is `degraded`, not `failed`
- [ ] `S4` holds: no gateway IP means no pings attempted, status `skipped`, data null
- [ ] A single target's ping failure never raises or aborts the check

## Why this behavior

`degraded` versus `failed` is the load-bearing distinction this spec exists to settle:
the `CheckResult` contract reserves `failed` for when there's no usable data at all, and
"every target shows 100% loss" is itself usable, actionable data — it's the worst
possible network, correctly reported, not an absence of measurement. Conflating the two
would make a completely dead network indistinguishable, in the report's `status` field,
from a check that never ran.
