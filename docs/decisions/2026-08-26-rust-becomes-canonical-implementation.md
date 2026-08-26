---
template_version: 1.2.0
date: 2026-08-26
slug: rust-becomes-canonical-implementation
status: accepted
decided_by: hampton
related: [2026-08-25-rust-rewrite-technology-stack]
---

# Decision: Rust is now the canonical implementation; TypeScript moved to `typescript-archive`

## Context

The Rust rewrite (see
[2026-08-25-rust-rewrite-technology-stack.md](2026-08-25-rust-rewrite-technology-stack.md))
was built on a separate `rust-rewrite` branch specifically so the working TypeScript
implementation on `main` wouldn't be touched or put at risk during the port. By
2026-08-25 the port was complete and verified module-by-module against the TypeScript
original, with every check, the CLI, the renderer, and the reporter ported and tested.
User's own framing for the next step: preparing to post about the project on Hacker
News, wanting the repo "in a more polished state" first - a repo whose default branch
still built and ran the old TypeScript implementation, with the smaller/faster Rust one
sitting on a side branch, was the wrong thing to show anyone.

## Decision

`rust-rewrite` was renamed to `main`; the old `main` (TypeScript) was renamed to
`typescript-archive`. Both branches keep their full independent history - nothing was
deleted, squashed, or force-pushed (no remote was configured at the time, so this was a
purely local, fully reversible operation). `main`'s working tree was then cleaned up to
match: the TypeScript source tree (`src/*.ts`, `tests/*.ts`, `package.json`,
`package-lock.json`, `tsconfig.json`, `vitest.config.ts`, `.nvmrc`, `.npmrc`,
`node_modules/`) was removed from `main` (still fully present on `typescript-archive`),
and `rust/{src,tests,Cargo.toml,Cargo.lock}` were flattened up to the repo root so the
project looks like a normal single-crate Cargo project instead of a Rust
implementation nested under a TypeScript one.

## Rationale

The TypeScript version was never a fallback or an equal alternative - the whole
motivation for the Rust rewrite was a real, measured problem (a Node single-executable
bundle would have been 80-120MB for a tool this small; the Rust binary is ~5MB). Once
the rewrite was verified complete, there was no reason for `main` to keep pointing at
the version being replaced. Keeping both trees physically coexisting on one branch
(TypeScript at the root, Rust nested in `rust/`) was a reasonable staging state during
the port itself, but became actively confusing once the port was done - `CLAUDE.md`'s
own Architecture section would otherwise need to describe two implementations at once,
and a repo root with both a `package.json` and a `Cargo.toml` reads as unfinished or
abandoned, not as a deliberate architecture.

Archiving (rename) rather than deleting was the obvious choice - the TypeScript
implementation remains a complete, working reference (it was the tie-breaker every Rust
module was verified against during the port) and there's no reason to lose that.

## Stakeholders

Solo call - no other stakeholders consulted.

## Considerations / Revisit if

- **No remote was configured when this happened.** Today: this was a purely local
  branch rename with no force-push or shared-history risk. Revisit if: a remote gets
  added later and there's ever a reason to reconsider `typescript-archive`'s visibility
  (e.g. hiding it, or making it more prominent as "see the old version").
- **The TypeScript implementation is now frozen, not deprecated-with-a-plan.** Today:
  nothing removes or archives it further than a branch rename; it still builds and runs
  exactly as it did. Revisit if: `typescript-archive` starts bit-rotting in a way that
  matters (e.g. a dependency's security advisory) and someone actually needs to run it
  again - unlikely, since it's reference material now, not a shipped artifact.

## Consequences

- `main`'s working tree is now Rust-only: `Cargo.toml`/`Cargo.lock` at the root,
  `src/`/`tests/` are Rust, no `node_modules`/`package.json`/`tsconfig.json` anywhere.
- `CLAUDE.md`'s Architecture, Development setup, and Conventions sections were rewritten
  to describe the Rust project exclusively - see
  [2026-08-26-rename-to-pubnet-tools.md](2026-08-26-rename-to-pubnet-tools.md) for the
  accompanying rename, done the same session.
- Anyone wanting the TypeScript implementation needs `git checkout typescript-archive`
  explicitly - it's no longer what a fresh clone of `main` gives you.
