---
template_version: 1.2.0
date: 2026-08-25
slug: save-off-by-default
status: accepted
decided_by: hampton
related: []
---

# Decision: Report saving is opt-in (`--save`), not opt-out (`--no-save`)

## Context

Since the original `PLAN.md`, conncheck has saved a JSON report to
`~/.conncheck/reports/` by default, with `--no-save` to opt out. Asked why, a search of
`PLAN.md` and every `docs/decisions/` entry turned up nothing — it was never a
deliberated decision. `PLAN.md`'s very first sentence just states "saves a JSON report"
as a headline feature, and the flag design followed from that without anyone weighing
the tradeoff of writing files to disk on every run, unasked, forever.

## Decision

Flip it: `--save` is now required to write a report file. Default behavior is
print-only — nothing touches `~/.conncheck/reports/` unless asked.

## Rationale

No real alternative was on the table here — this wasn't a close call between two
designs, it was noticing an unexamined default and asking whether it would survive
being examined. A CLI that writes files to disk on every invocation without being asked
is the kind of thing that should require an opt-in, not an opt-out, absent an actual
reason (this tool has none on record — no history/comparison feature reads old reports
back, nothing downstream depends on the files existing).

## Stakeholders

Solo call — no other stakeholders consulted.

## Considerations / Revisit if

- **Nothing in conncheck currently reads old reports back.** Today: each run is
  independent; `~/.conncheck/reports/` is write-only from the tool's own perspective.
  Revisit if: a feature is added that compares runs over time or otherwise depends on
  historical reports existing by default. Then likely: reconsider defaulting to save,
  or add a prompt/first-run notice instead of a silent default.

## Consequences

- Breaking CLI change: `--no-save` no longer exists; `--save` does. Anyone scripting
  against the old default (relying on files silently accumulating) needs to add
  `--save`.
- `README.md` and `src/cli.ts`'s option help text updated to match.
