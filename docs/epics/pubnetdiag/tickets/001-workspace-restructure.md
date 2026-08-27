---
template_version: 1.0.0
epic: pubnetdiag
ticket: 001
slug: workspace-restructure
type: chore
points: 5
status: todo
tracker_ref: "15"
pr: none
related: []
---

# Ticket 001: Cargo workspace restructure

## Goal

Convert the repo from a single crate to a Cargo workspace with a shared platform
library, so `pubnetchk` and `pubnetdiag` can share platform probe code without
duplication.

## Scope

- **In:** Root `Cargo.toml` becomes a workspace manifest; existing crate moves to
  `crates/pubnetchk/`; new shared lib crate at `crates/pubnet-platform/` (or
  `pubnet-core/`) containing `src/platform/`, `src/network.rs`, `src/exec.rs`;
  empty `pubnetdiag` binary crate at `crates/pubnetdiag/` that compiles and
  prints "not yet implemented"; all existing tests still pass; `cargo build` at
  workspace root produces both binaries
- **Out:** Any logic changes to `pubnetchk`; the `pubnetdiag` feature
  implementation (that's tickets 2–4); CI changes beyond making the workspace
  build

## Acceptance criteria

- `cargo build` at the repo root produces `pubnetchk` and `pubnetdiag` (the
  latter is a stub)
- `cargo test --lib` passes for the `pubnetchk` crate unchanged
- The contract tests in `tests/` still run against `pubnetchk`
- `pubnetdiag --help` prints something and exits 0

## Notes

The shared crate name is a decision — `pubnet-platform` if it only holds OS
abstractions, `pubnet-core` if types and scoring also move. Keep `pubnetchk`'s
public-facing behavior identical: this ticket is plumbing only.

The `windows-sys` dependency stays in the shared platform crate, not in either
binary crate. Same for `tokio`, `serde`, `reqwest` — they move to whichever
crate actually uses them.

`CLAUDE.md` Development setup section needs updating with the new `cargo build`
invocation (it won't change, but the crate path references will).
