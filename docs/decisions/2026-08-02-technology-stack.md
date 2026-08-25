# Decision: Technology Stack

**Date:** 2026-08-02
**Status:** accepted

## Context

conncheck is a TypeScript CLI targeting Linux. Every dependency choice is blog post material — rationale is a first-class concern, not an afterthought.

## Decision

Runtime: TypeScript with `tsx` for direct execution. Six runtime dependencies, all MIT.

## Rationale by component

### TypeScript over plain JS
Typed `CheckResult<T>` generics and shell output parsers need compile-time safety. The parsing layer is where bugs hide — `lines[0]` on empty output, missing fields, unexpected nmcli formatting. TS catches these at build time with `noUncheckedIndexedAccess: true`.

### tsx over ts-node or a build step
tsx uses esbuild internally: cold start <100ms vs ~500ms for ts-node. No `dist/` directory to keep in sync. The shebang is `#!/usr/bin/env tsx` — the file runs directly in development and in production after `npm link`. tsx doesn't type-check (that's `tsc --noEmit`'s job in CI).

### commander over yargs / meow / oclif
Zero runtime dependencies. yargs pulls in `cliui`, `y18n`, and `string-width`. meow has no subcommand routing. oclif is a full framework — overkill for two commands. Commander 15's API maps cleanly onto `conncheck` (default run command) and `conncheck record`.

### chalk over kleur / picocolors / ansis
All are MIT and faster than chalk, but chalk's `NO_COLOR` env var and TTY auto-detection are first-class. The performance difference is noise on a CLI running multi-second network checks. Chalk 6 is pure ESM, which matches our `"type": "module"` requirement.

### ora over listr2 / nanospinner
ora's `.start()/.succeed()/.fail()/.warn()` API maps exactly onto `CheckStatus` ('ok'/'failed'/'degraded'/'skipped'). listr2 would require wrapping every check as a task object, restructuring the architecture around the UI library. nanospinner's state API is less expressive.

### axios over native fetch / got
The concrete reason: captive portal detection needs `{ maxRedirects: 0, validateStatus: () => true }` to capture the `Location` header from redirect responses. Native `fetch` with `redirect: 'manual'` has awkward TypeScript typing for the `Response.redirected` property and `Location` header extraction. got brings multiple internal packages. axios handles this pattern cleanly.

### @cloudflare/speedtest over Ookla / fast.com / DIY
See [open-source-only decision](2026-08-02-open-source-only.md). Cloudflare's anycast edge also minimizes route variance on public WiFi compared to a fixed iPerf3 server.

**Superseded 2026-08-24:** `@cloudflare/speedtest` turned out to be a browser-only library — it depends on the browser Resource Timing API, not just `fetch()`, so it cannot actually measure bandwidth in Node. See [cloudflare-speedtest-not-node-compatible](2026-08-24-cloudflare-speedtest-not-node-compatible.md) for what replaced it (a direct NDT7 client).

### vitest over Jest / node:test
Jest requires `--experimental-vm-modules` for ESM as of 2025 — still painful. `node:test` lacks `describe`/`it` nesting and inadequate timeout controls. vitest has native ESM+TS via esbuild, per-test `timeout`, and `--reporter=verbose` that reads well in terminals.

## Consequences

- `"type": "module"` in package.json — all imports need `.js` extensions (NodeNext moduleResolution enforces this at compile time).
- Node ≥ 20 required (`@cloudflare/speedtest` uses `fetch()` natively; Node 22 is on the target machine). **Superseded 2026-08-24:** Node ≥ 24 required — see the decision linked above; unrelated to this line's original reasoning.
- No CommonJS fallback. No dual-package publishing complexity.
- The 6 runtime deps are: `commander`, `chalk`, `ora`, `cli-table3`, `axios`, `@cloudflare/speedtest`. **Superseded 2026-08-24:** `@cloudflare/speedtest` was replaced by a direct NDT7 client built on `ws` — see the decision linked above. **`cli-table3` removed 2026-08-25:** the per-target ping table it rendered was replaced by a condensed Local/Internet loss+latency summary (user preference — the table was more detail than wanted by default), leaving nothing in the codebase to use it. Not a rejection of the library itself; if a `--verbose` per-target view gets added later, it's the natural fit again.
