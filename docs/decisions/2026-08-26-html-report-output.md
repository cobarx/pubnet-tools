---
template_version: 1.2.0
date: 2026-08-26
slug: html-report-output
status: accepted
decided_by: hampton
related: [save-off-by-default]
---

# Decision: A plain-language HTML report (`--html` / `--open`), self-contained, opened via `xdg-open`

## Context

The terminal and `--json` views both assume a reader who knows what a gateway,
CIDR, DNS interception, packet loss, and jitter are. The tool needed a mode that
a non-technical person (the motivating case: showing it to a family member) can
read and act on — "is this network safe, and for what?" — without any of that
vocabulary.

## Decision

Add a third output: a plain-language HTML report.

- `--html` writes a self-contained `.html` file to `~/.pubnetchk/reports/` and
  prints its path (parallel to `--save` for JSON).
- `--open` launches it in the default browser and implies `--html` (opening a
  report you never generated is meaningless).
- The opener is `xdg-open` on Linux, `open` on macOS — the desktop's own
  default-handler mechanism, never a hardcoded browser.

The HTML leads with one verdict card (safe / some caution / take care) derived
from the existing `RiskLevel`, translates each `Finding` into one sentence of
"what it means for you" (keyed by finding id, prefix-matched for the per-target
reliability ids, falling back to the finding's own title for any id without a
gloss so nothing is silently dropped), and tucks the technical detail (interface,
CIDR, per-target RTT/jitter) into a collapsed `<details>` block.

## Rationale

- **Multi-format-behind-a-flag is the idiomatic Unix shape.** The tool already
  has `--json`; `--html` + `xdg-open` is exactly how coverage tools, `pytest
  --html`, lighthouse, flamegraph, etc. surface a browser report. Nothing novel
  to justify — it follows the pattern already established here.
- **Self-contained (inline CSS, no assets, no JS) is a hard requirement, not a
  nicety.** It's what lets the file open from `file://`, be emailed, or copied to
  another machine and still render. It also means zero new runtime dependencies —
  the report is string formatting, same as `renderer.rs` already does. Consistent
  with the open-source-only / justify-every-dependency posture: this adds no
  dependency at all.
- **Reuses `Report` unchanged.** No check, type, or scoring change; `html.rs`
  sits beside `renderer.rs` as a second consumer of the same struct.

## Alternatives considered

- **A live/interactive page (JS, charts).** Rejected: breaks self-containment,
  pulls in assets, and adds attack surface and dependency weight for no gain over
  a static page a non-technical reader can read top-to-bottom.
- **Templating/asset-embedding crate (askama, rust-embed).** Rejected: one
  string-built page doesn't justify a dependency; `format!` is enough and matches
  the existing renderer.
- **Print HTML to stdout like `--json`.** Rejected: HTML's value here is being
  opened in a browser, so writing a file + printing its path (and optionally
  opening it) is the useful shape. `--json` stays the stdout/pipe format.

## Stakeholders

Solo call — no other stakeholders consulted.

## Footer timestamp: local time, captured before the runtime starts

The footer shows *local* time ("August 26, 2026 at 10:51 PM", no zone label —
it matches the reader's own clock), not the machine timestamp's UTC. Reading the
local offset needs the `time` crate's `local-offset` feature, and that crate
refuses to read the offset once the process is multithreaded (concurrent access
to the environment's timezone is unsound). A `#[tokio::main]` body runs *after*
the runtime has spawned worker threads, so `main` was changed to a plain `fn`
that calls `init_local_offset()` while still single-threaded, stashes the offset
in a `OnceLock`, and only then builds the runtime. If the offset can't be read,
the footer falls back to UTC, labelled as such so a time that doesn't match the
reader's clock is never shown unlabelled. The stored `Report.timestamp` (used for
the JSON report and the filename) stays UTC/RFC3339 — only the HTML footer is
localized.

Enabling `local-offset` pulls in one transitive dependency, `num_threads`
(MIT/Apache-2.0, by the `time` maintainer, used to make the soundness check
above), which is consistent with the open-source-only posture. No new *direct*
dependency.

## Considerations / Revisit if

- **Wording is first-draft and meant to be tuned.** The verdict/finding
  sentences are deliberately plain and will be iterated on against a real reader.
  Revisit if the plain-language glosses drift out of sync with what a finding
  actually means (they live in `explain()` in `html.rs`, keyed by finding id).
- **`xdg-open` availability.** Present on essentially all desktop Linux
  (`xdg-utils`); on a headless box it will fail, which is reported without failing
  the run — the file is already written and its path printed.

## Consequences

- New module `src/output/html.rs` and `save_html_report` in `reporter.rs`; new
  `--html`/`--open` flags in `cli.rs`. No dependency added.
- Files land in the same `~/.pubnetchk/reports/` directory as `--save`'s JSON,
  and (like `--save`) only when asked — consistent with [[save-off-by-default]].
</content>
