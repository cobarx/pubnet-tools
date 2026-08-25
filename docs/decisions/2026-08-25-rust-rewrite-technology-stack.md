---
template_version: 1.2.0
date: 2026-08-25
slug: rust-rewrite-technology-stack
status: accepted
decided_by: hampton
related: [2026-08-02-technology-stack, 2026-08-24-cloudflare-speedtest-not-node-compatible]
---

# Decision: Rust rewrite, on a separate branch (`rust-rewrite`), technology stack

## Context

Discussion of packaging conncheck as a real Linux utility (man page, distro-native
install) surfaced that Node's Single Executable Applications feature — the path that
would have kept the TypeScript implementation — bundles the entire V8/Node runtime
into the binary, roughly 80–120MB for a tool whose own logic is ~1,500 lines. That was
rejected outright as disproportionate for "a simple utility like this." A from-scratch
size estimate for Rust (~5–8MB, later revised down further once static-vs-dynamic
linking was reconsidered — see the linking discussion in the conversation that produced
this entry) made the rewrite case on its own terms rather than by the TS path's
elimination.

Kept on a separate branch rather than replacing `main`: the TypeScript implementation
is complete, tested (111 tests), and working — nothing about the rewrite decision
implies it was wrong to build first. The specs in `docs/specs/` and prior decisions in
`docs/decisions/` are behavior contracts, not TypeScript-specific, and carry over to
this branch unchanged; this entry only concerns the Rust-specific technology choices,
the same role `2026-08-02-technology-stack.md` played for the original build.

## Decision

Rust project lives in `rust/` (not the repo root — cargo's default `cargo init`
collides with the existing TypeScript `src/` tree; caught and fixed before it was
committed). Dependencies, chosen to mirror each TypeScript dependency's job as closely
as possible:

| Need | TypeScript | Rust | Why this one |
|---|---|---|---|
| CLI parsing | `commander` | `clap` (derive) | The `clap`/`structopt` lineage is to Rust CLIs what `commander` is to Node — the de facto standard, derive macros keep the flag definitions declarative |
| Async runtime | (Node's event loop) | `tokio` | Required by `reqwest` and `tokio-tungstenite`; the standard async runtime, no real alternative in practice |
| HTTP client (DoH probes, captive portal) | `axios` | `reqwest`, **`default-features = false`**, `rustls` feature only | The concrete reason to disable defaults: `reqwest`'s default pulls in `native-tls` (OpenSSL). `rustls` is pure Rust — no system OpenSSL dependency, no `pkg-config`/`build.rs` needed, consistent with the static/dynamic-linking discussion that produced this stack |
| WebSocket client (NDT7) | hand-rolled over `ws` | `tokio-tungstenite`, `rustls-tls-webpki-roots` feature | Same rustls-only reasoning as `reqwest`. The NDT7 protocol implementation itself stays hand-rolled either way — see `2026-08-24-cloudflare-speedtest-not-node-compatible.md` for why (the *protocol* client is hand-rolled in both languages; only the transport library differs) |
| JSON | (native) | `serde` + `serde_json` | Rust has no built-in JSON; `serde`'s derive macros are the standard, and match `CheckResult<T>`'s shape directly via `#[derive(Serialize, Deserialize)]` |
| Spinner | `ora` | `indicatif` | The standard Rust progress/spinner crate; pairs naturally with `console` (same ecosystem convention) |
| Terminal color | `chalk` | `console` | NO_COLOR/TTY-aware like chalk; chosen specifically because it's the crate `indicatif` itself is commonly paired with, keeping the terminal-handling story from one ecosystem corner rather than two |
| Shell-output parsing | hand-rolled regex/string logic | `regex` | Same approach as TS — nmcli's terse-format colon-escaping (see `src/utils/network.ts`'s `splitTerseFields`) is a parsing problem, not something a crate solves for us either way |
| NDT7 upload payload | `node:crypto`'s `randomBytes` | `rand` | Direct equivalent, no leak-detection-grade randomness required (it's load-generation filler, not cryptographic) |
| Timestamps (ISO 8601 report timestamps) | native `Date` | `time`, `formatting`+`parsing` features | Chosen over `chrono` for a smaller dependency footprint — consistent with this whole exercise being about binary size in the first place; `chrono` remains the more common default but pulls in more than this tool needs |

**Not used, deliberately:** no `-sys` crate, no `build.rs`, no `pkg-config` dependency
anywhere in the tree — verified via `cargo tree | grep -i openssl` after adding every
dependency above; the only near-hit is `openssl-probe`, which is a pure-Rust crate that
locates the system CA bundle path for `rustls-native-certs` and does not link against
OpenSSL itself.

## Rationale

Every crate above was chosen to answer "what does the equivalent TypeScript dependency
actually do for us" rather than picking the most popular Rust crate in each category
in isolation — the same standard `2026-08-02-technology-stack.md` set for the original
build. The one deliberate divergence from "just port the choice" is the HTTP/WebSocket
TLS backend: TypeScript's `axios`/`ws` had no meaningful TLS-backend choice to make
(Node's TLS is Node's TLS), but Rust's ecosystem does, and `rustls` was chosen
specifically to preserve the "no system library dependency" property that was the
entire point of considering Rust in the first place.

## Stakeholders

Solo call — no other stakeholders consulted.

## Considerations / Revisit if

- **`indicatif`/`console` vs. hand-rolling minimal ANSI output.** Today: both are
  well-maintained, widely used, and small. Revisit if: binary size analysis (once real
  business logic is ported and a release build exists) shows either contributing
  meaningfully to the final size. Then likely: hand-roll spinner/color instead — the
  actual logic (start/stop a spinner, print colored text) is not complex enough to
  justify a dependency purely on size-sensitivity grounds if the number turns out to
  matter more than expected.
- **`time` vs `chrono`.** Today: `time` was picked for a smaller footprint on a guess,
  not a measurement. Revisit if: the report-timestamp formatting needs turn out to want
  something `time`'s feature set doesn't cover well. Then likely: `chrono` is the
  fallback, well-trodden and unlikely to be wrong, just larger.

## Consequences

- `rust/Cargo.toml` is the dependency manifest; `rust/src/` mirrors
  `src/`'s module structure 1:1 (`types.rs`, `exec.rs`, `network.rs`, `scoring.rs`,
  `checks/`, `output/`, `cli.rs`) so the two implementations stay easy to compare
  file-for-file during the port.
- `scoring.rs` is fully ported as of this entry (4/4 tests passing, same spec citations
  as `scoring.ts`'s tests) — proof the module-mirroring approach works before the much
  larger checks/parsing modules are attempted.
- Everything else (`network.rs`, `checks/*.rs`, `output/*.rs`, `cli.rs`) is scaffolded
  with doc-comment TODOs but not yet implemented.
