# Decision: Open Source Only

**Date:** 2026-08-02
**Status:** accepted

## Context

conncheck is being built as a blog post project and a tool others can contribute to and build on. Two obvious speed test options exist: Ookla's Speedtest CLI and Netflix's fast.com.

## Decision

All dependencies must carry MIT, Apache 2.0, or ISC licenses. No proprietary services or closed APIs, even those with free tiers.

## Rationale

- Ookla Speedtest CLI: EULA §14 explicitly prohibits automated use without a commercial agreement. Using it would make conncheck non-redistributable.
- fast.com: Netflix's closed commercial infrastructure. No API, no license, no contribution path.
- Open technologies create a community. A blog post that depends on proprietary tools teaches readers they can't replicate the project without accepting restrictive terms.
- `@cloudflare/speedtest` (MIT) is the same source code powering speed.cloudflare.com — functionally equivalent to the Ookla CLI for our purposes, fully open.

## Consequences

- All 6 runtime dependencies carry MIT licenses.
- Speed measurement uses `@cloudflare/speedtest` exclusively.
- Any future dependency must be vetted for license before adding.
- DIY HTTP timing (fetch + measure TTFB) is also ruled out — it measures TCP+TLS overhead, not true bandwidth, and lacks a controlled reference server.
