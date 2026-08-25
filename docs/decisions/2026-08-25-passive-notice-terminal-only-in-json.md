---
template_version: 1.2.0
date: 2026-08-25
slug: passive-notice-terminal-only-in-json
status: proposed
decided_by: hampton
related: [2026-08-02-passive-topology]
---

# Decision: passiveNotice stays in the JSON report only, not the terminal view

## Context

`2026-08-02-passive-topology.md`'s Consequences section states the `passiveNotice`
field "appears in both terminal output and JSON reports, making the constraint
explicit and auditable." Building the condensed Network/Security/Performance terminal
layout surfaced a problem with that: the terminal view has never actually rendered the
ARP neighbor list itself — only the gateway — so the notice sat in the terminal with
nothing concrete to disclaim. It read as noise, not a disclosure.

## Decision

Drop `passiveNotice` from the terminal renderer. It stays in every JSON report
unchanged — the underlying passive-only behavior isn't changing, only where the
disclosure about it is shown.

Marked `proposed`, not `accepted`: this was a quick call to unblock the terminal
cleanup, made with "we can revisit later" attached, not a considered resolution of the
actual tension it reopens.

## Rationale

Two options were on the table in the moment:

- **Keep the notice, but give it something to refer to** — e.g. show a neighbor count
  ("3 other devices seen (passive ARP cache, no active scan)"), so the terminal
  disclosure has actual data next to it. This was offered and not chosen tonight, but
  it's the more complete fix and the likely candidate when this gets revisited.
- **Remove it from the terminal entirely** (chosen, for now) — simplest, unblocks the
  section cleanup immediately, defers the harder question of whether terminal-level
  auditability is actually load-bearing or was just always going to end up in the JSON
  anyway.

## Stakeholders

Solo call — no other stakeholders consulted.

## Considerations / Revisit if

- **Whether terminal-visible auditability was actually the point, or just how it was
  first implemented.** Today: unresolved — this decision doesn't settle it, it just
  stops blocking on it. Revisit if: anyone actually asks "did this scan my network?"
  without reading the JSON report, which would suggest the terminal disclosure was
  load-bearing after all. Then likely: adopt the neighbor-count version instead of a
  bare notice line.
- **The terminal might grow a neighbor list of its own later** (e.g. a `--verbose`
  flag), at which point the notice has something to attach to again and this decision
  should be revisited on its own terms rather than left as "removed."

## Consequences

- `src/output/renderer.ts` no longer renders `topo.passiveNotice`.
- `docs/decisions/2026-08-02-passive-topology.md`'s Consequences bullet about
  terminal+JSON is now only true for JSON; a forward-link note was added there rather
  than rewriting it (append-only).
- No change to `TopologyData` or the JSON report shape — `passiveNotice` is unchanged
  there.
