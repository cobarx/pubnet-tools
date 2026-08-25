---
template_version: 1.0.0
slug: risk-scoring
status: agreed
owner: hampton
date: 2026-08-24
related: []
---

# Spec: Risk scoring

## Intent

Every conncheck run ends in one of three risk levels — Low, Medium, or High — computed
from the findings the individual checks produced, so the person running it gets a
single, glanceable answer instead of having to read four separate check results
themselves.

**Not in scope:** what any individual finding is or how many points it's worth (each
check's own spec decides that); how findings are rendered or colored.

## Terms

- **Finding** — one scored observation from a check (e.g. "WiFi is open"), carrying a
  point value.
- **Total** — the sum of every finding's points across all checks that produced any.
- **Skipped check** — a check that did not run because a precondition was absent (e.g.
  no default route). Distinct from a check that ran and found nothing wrong.

## Scenarios

### S1 — No findings score Low

**Happy path.**

- **Given** every check ran and produced only zero-point findings
- **When** the score is calculated
- **Then** the total is 0
- **And** the level is `Low`

### S2 — A skipped check contributes nothing

**Failure.**

- **Given** one check did not run because its precondition was absent
- **And** every check that did run produced only zero-point findings
- **When** the score is calculated
- **Then** the skipped check contributes zero points
- **And** the skipped check is not treated as a penalty or as a clean result
- **And** the level reflects only the checks that actually ran

### S3 — The Low/Medium boundary is exclusive of Medium

**Edge.**

- **Given** findings summing to exactly 19 points
- **When** the score is calculated
- **Then** the level is `Low`
- **And** given findings summing to exactly 20 points, the level is `Medium`

### S4 — The Medium/High boundary is exclusive of High

**Edge.**

- **Given** findings summing to exactly 49 points
- **When** the score is calculated
- **Then** the level is `Medium`
- **And** given findings summing to exactly 50 points, the level is `High`

## Open questions

None outstanding.

## Done when

- [ ] `S1` holds: zero total score is `Low`
- [ ] `S2` holds: a skipped check is excluded from scoring, not scored as a pass or a
      penalty
- [ ] `S3` holds: 19 is `Low`, 20 is `Medium`
- [ ] `S4` holds: 49 is `Medium`, 50 is `High`
- [ ] The score is a pure function of the findings passed to it — no I/O, no
      randomness, same input always produces the same output

## Why this behavior

An additive point model with fixed bands is simple enough for a public-WiFi audit tool
to explain in one sentence, and the boundary scenarios exist because "20–49 → Medium"
read unambiguously on paper but the inclusive/exclusive edges are exactly where an
off-by-one silently changes a real network's reported risk level. Calibration: an open
network alone scores 40 points (`High`) — see `PLAN.md`'s scoring table for the
per-finding point values this spec's totals are built from.
