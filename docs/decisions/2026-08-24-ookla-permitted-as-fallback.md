---
template_version: 1.2.0
date: 2026-08-24
slug: ookla-permitted-as-fallback
status: accepted
decided_by: hampton
related: [2026-08-02-open-source-only]
---

# Decision: Ookla is permitted as a last-resort fallback when no open-source alternative exists

## Context

While researching a replacement for `@cloudflare/speedtest` (which turned out to be
non-functional outside a real browser — see
[docs/decisions/2026-08-24-cloudflare-speedtest-not-node-compatible.md](2026-08-24-cloudflare-speedtest-not-node-compatible.md)),
the search surfaced that most Node speed-test packages on npm (`speedtest-net`,
`speed-test`) are thin wrappers around Ookla's speedtest.net infrastructure — which the
original open-source-only decision hard-rejected outright, citing Ookla's EULA §14
prohibition on automated use.

That search also found a genuinely open, non-Ookla, non-Netflix alternative (M-Lab's
NDT7 protocol), so this decision doesn't change what conncheck's speed check actually
uses today. It records a policy question that came up while looking, independent of
today's outcome.

## Decision

Ookla is no longer categorically excluded. It's permitted as a fallback specifically
when **no open-source alternative exists** for the capability needed — checked and
exhausted first, not skipped because Ookla is more convenient.

## Rationale

The original decision's blanket rejection was made without having yet hit a case where
no open-source option existed at all. Having now actually searched and found the
non-Ookla landscape is thin for consumer bandwidth testing (most tooling either wraps
Ookla or wraps fast.com), a hard "never" turns "no open-source option exists" into a
dead end rather than a fallback. A conditional exception — open-source first, Ookla
only if genuinely nothing else covers the need — preserves the original value (open
source is the default, every dependency justified) without leaving no path forward.

The alternative considered was leaving the original decision as an absolute rule and
accepting that some capability might simply be unavailable to conncheck. Rejected: for
a tool whose whole point is auditing a network's real-world speed, "we couldn't check
that" is a worse outcome than a documented, narrow exception.

## Stakeholders

Solo call — no other stakeholders consulted.

## Considerations / Revisit if

- **This exception hasn't been exercised yet.** Today: every capability conncheck
  needs (speed, DNS, topology, reliability) has an open-source path (NDT7 for speed).
  Revisit if: a future capability is added with no open-source option and this
  exception is actually invoked. Then likely: the specific Ookla dependency taken on
  gets its own decision entry, since "permitted in principle" and "actually adopted"
  warrant separate records — this entry only establishes that the door isn't fully
  closed.
- **Ookla's EULA §14 itself hasn't changed.** Today: automated/programmatic use is
  still prohibited without a commercial agreement per the original decision's reading.
  Revisit if: Ookla's terms change, or a commercial agreement is obtained. Then likely:
  re-confirm the EULA reading before any actual Ookla adoption, not just cite this
  entry.

## Consequences

- `docs/decisions/2026-08-02-open-source-only.md` is not superseded — its core stance
  (open source by default, every dependency justified) still holds. This entry narrows
  one absolute clause of it into a conditional.
- Any future dependency decision that reaches for Ookla must show the open-source
  search that came up empty, the same way this entry's Rationale does.
