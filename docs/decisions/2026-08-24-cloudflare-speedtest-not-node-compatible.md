---
template_version: 1.2.0
date: 2026-08-24
slug: cloudflare-speedtest-not-node-compatible
status: accepted
decided_by: hampton
related: [2026-08-02-technology-stack, 2026-08-02-open-source-only, 2026-08-24-ookla-permitted-as-fallback]
---

# Decision: Replace @cloudflare/speedtest with a direct NDT7 (M-Lab) client

## Context

`docs/decisions/2026-08-02-technology-stack.md` chose `@cloudflare/speedtest` on the
strength of "the library uses `fetch()` internally — Node 22 (on this machine) provides
it natively." That turned out to be necessary but not sufficient. Building
`src/checks/speed.ts` against the real package surfaced two layered problems, found by
actually running it in plain Node rather than by reading its source:

1. It references `window.location.origin` unconditionally at three call sites (as the
   base argument to `new URL(apiUrl, window.location.origin)`), which the WHATWG URL
   constructor evaluates even when `apiUrl` is already absolute. This alone throws
   `window is not defined` in Node before any request is made.
2. Patching a minimal `window` shim gets past that, but the library's actual bandwidth
   measurement mechanism reads from the browser's Resource Timing API —
   `performance.clearResourceTimings()`, `performance.setResourceTimingBufferSize()`,
   and `performance.getEntriesByName(url)` returning populated
   `PerformanceResourceTiming` entries with real `transferSize`. Node's global
   `performance` (from `perf_hooks`) doesn't implement this surface for `fetch()`
   requests the way a browser does. With the shim in place, every download/upload
   request failed with `Cannot read properties of undefined (reading 'transferSize')`
   — the library was still fetching data over the network successfully, but had no way
   to measure how fast it happened.

In short: `@cloudflare/speedtest` is not a Node library that happens to also work in a
browser. It's a browser library, full stop, and the earlier decision's "uses fetch()"
observation was true but answered the wrong question.

## Decision

Drop `@cloudflare/speedtest`. Implement the speed check as a direct client for M-Lab's
**NDT7** protocol — a WebSocket-based bandwidth measurement protocol, open and
non-commercial (Measurement Lab is a consortium including Google, Internet2, and
academic partners; not Ookla, not Netflix). The protocol itself is plain WebSocket
frames, so it has no browser-API dependency; only the *official* `@m-lab/ndt7` client
library is browser-oriented (it wraps the protocol in Web Workers for UI reasons), so
conncheck implements the protocol directly against the `ws` package rather than
depending on that library.

## Rationale

Options actually researched (see the conversation that produced this entry, and
[docs/decisions/2026-08-24-ookla-permitted-as-fallback.md](2026-08-24-ookla-permitted-as-fallback.md)
for the fallback policy this search also produced):

- **Polyfill/patch around the Resource Timing gap** (e.g. a `PerformanceObserver`
  shim wired to Node's undici instrumentation). Rejected: exploratory testing showed
  Node's fetch-instrumentation resource-timing entries don't reliably populate for this
  library's usage pattern, and even if coaxed into working, it would mean maintaining a
  fragile compatibility shim against a library that was never designed to run outside a
  browser — fighting the dependency rather than using it.
- **Run it in a real headless browser** (Playwright/Puppeteer). Rejected: works
  correctly (real Resource Timing), but adds a full browser-binary dependency to a CLI
  tool — wildly out of proportion for one check, and contrary to the project's
  minimalism.
- **`speedtest-net` / `speed-test` (Ookla-backed)**. Rejected per the original
  open-source-only decision — Ookla's EULA prohibits automated use.
- **`network-speed` or DIY HTTP timing against an arbitrary server**. Rejected per the
  original technology-stack decision's reasoning: measures TCP+TLS+TTFB, not true
  bandwidth, without a controlled reference server.
- **NDT7 via the official `@m-lab/ndt7` client**. Partially rejected: the client itself
  assumes a browser (Web Workers). The *protocol* underneath it doesn't, so this
  decision uses the protocol directly rather than the packaged client.
- **NDT7 implemented directly over `ws`** (chosen). Genuinely open infrastructure, a
  protocol purpose-built for exactly this measurement (not a repurposed HTTP timing
  hack), and no browser dependency once implemented against the raw protocol.

## Stakeholders

Solo call, but the direction was chosen from a set of options presented to Hampton
(NDT7-direct vs. DIY-against-Cloudflare's-own-endpoints) — he picked NDT7 for
technical correctness over the lower-effort DIY option. Recorded here since it's the
concrete choice this entry commits to.

## Considerations / Revisit if

- **This adds a new runtime dependency (`ws`) that the original technology-stack
  decision's dependency count didn't include.** Today: `ws` is a well-established,
  MIT-licensed WebSocket client with no further runtime dependencies of its own.
  Revisit if: `ws` is ever unmaintained or a lighter option becomes clearly better.
  Then likely: swap the WebSocket transport only — the NDT7 protocol implementation
  itself doesn't change.
- **NDT7 protocol implementation is being hand-rolled, not vendored from a maintained
  client.** Today: the protocol is simple enough (locate a server, WebSocket handshake
  with the `net.measurementlab.ndt.v7` subprotocol, timed binary frame exchange, parse
  JSON measurement messages) to implement directly and test against real M-Lab servers.
  Revisit if: the protocol changes in a way that breaks conncheck's implementation, or
  M-Lab ships an official Node-targeted client. Then likely: switch to the official
  client if one becomes available and Node-compatible.
- **M-Lab's public infrastructure has different availability/rate-limit characteristics
  than Cloudflare's anycast edge.** Today: unknown at the time of writing — this is a
  new dependency on M-Lab's infrastructure specifically. Revisit if: M-Lab endpoints
  prove unreliable or rate-limited in practice. Then likely: reconsider the
  DIY-against-Cloudflare's-own-endpoints option that was the other candidate here.

## Consequences

- `docs/decisions/2026-08-02-technology-stack.md`'s `@cloudflare/speedtest` entry is
  now known-incorrect on Node compatibility; this entry supersedes that row's
  conclusion without rewriting the original (append-only).
- `package.json` drops `@cloudflare/speedtest`, adds `ws`.
- `src/checks/speed.ts` is rewritten around a hand-implemented NDT7 client instead of
  wrapping a library's event API.
- README and any blog-post material describing the speed check's technology choice
  needs updating to match.
