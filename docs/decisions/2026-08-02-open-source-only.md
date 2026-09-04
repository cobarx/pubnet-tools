# Decision: Open Source Only

**Date:** 2026-08-02
**Status:** accepted

> **Amended (2026-09-04):** the original license allowlist (MIT/Apache-2.0/ISC only)
> was narrower than the underlying goal requires. Updated below — permissive licenses
> are still preferred, but a dependency isn't ruled out just for being copyleft.

## Context

conncheck is being built as a blog post project and a tool others can contribute to and build on. Two obvious speed test options exist: Ookla's Speedtest CLI and Netflix's fast.com.

## Decision

Every dependency must carry a compatible open source license. No proprietary services or closed APIs, even those with free tiers.

Prefer BSD-style permissive licenses (MIT, Apache-2.0, BSD, ISC). A copyleft license
(LGPL or similar) is fine when it's genuinely the best tool for the job — it isn't
avoided just for being copyleft — as long as it doesn't force this project's own MIT
code under copyleft terms (e.g. static linking against a strong-copyleft/GPL
dependency would; LGPL's dynamic-linking carve-out generally wouldn't).

## Rationale

- Ookla Speedtest CLI: EULA §14 explicitly prohibits automated use without a commercial agreement. Using it would make conncheck non-redistributable.
- fast.com: Netflix's closed commercial infrastructure. No API, no license, no contribution path.
- Open technologies create a community. A blog post that depends on proprietary tools teaches readers they can't replicate the project without accepting restrictive terms.
- `@cloudflare/speedtest` (MIT) is the same source code powering speed.cloudflare.com — functionally equivalent to the Ookla CLI for our purposes, fully open.
- Restricting to a fixed allowlist (MIT/Apache-2.0/ISC) ruled out otherwise-good dependencies for no real reason — the actual constraint is redistributability and not forcing this project's own code under copyleft, not the specific license name.

## Consequences

- Runtime dependencies are permissive today; nothing currently pulls in a copyleft
  dependency, but one isn't disqualified on sight.
- Speed measurement uses `@cloudflare/speedtest` exclusively.
- Any future dependency must still be vetted for license before adding — now for
  redistributability and copyleft-obligation risk, not membership in a fixed allowlist.
- DIY HTTP timing (fetch + measure TTFB) is also ruled out — it measures TCP+TLS overhead, not true bandwidth, and lacks a controlled reference server.
