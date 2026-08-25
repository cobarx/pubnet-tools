# conncheck

## Summary

conncheck is a TypeScript CLI that audits the public WiFi or network you just joined. It checks security posture (WiFi encryption, DNS leak, captive portal), speed (M-Lab's open NDT7 protocol, implemented directly over WebSocket), reliability (ping/jitter/packet loss to three targets), and passive network topology (ARP cache only — no active scanning). Results are scored Low/Medium/High risk and saved as a JSON report. A `record` subcommand wraps the run in asciinema for session capture.

Built as a blog post project. Technology rationale is a first-class concern — every dependency is justified against its alternatives in `docs/decisions/`.

**Platform:** Linux (CachyOS / Arch-based), non-root user. Node ≥ 24 (`.nvmrc` pins 24).

## Architecture

```
conncheck
  ├── src/index.ts        shebang entry point (#!/usr/bin/env tsx), no logic
  ├── src/cli.ts          commander setup, orchestrates checks, manages spinners
  ├── src/types.ts        all interfaces and discriminated unions, zero runtime code
  ├── src/scoring.ts      pure function: CheckResult[] → { total, level, findings }
  ├── src/checks/
  │   ├── topology.ts     ip route/addr/neigh — passive only, seeds gateway for others
  │   ├── security.ts     nmcli + resolvectl + DoH probes + captive portal (axios)
  │   ├── reliability.ts  ping -c 10 -i 0.2, Promise.allSettled, per-packet RTT parsing
  │   └── speed.ts        NDT7 (M-Lab) client over `ws`, hand-rolled protocol — see decisions/
  ├── src/output/
  │   ├── renderer.ts     chalk only, condensed Network/Security/Performance sections, never calls network
  │   └── reporter.ts     saves JSON to ~/.conncheck/reports/<timestamp>.json
  └── src/utils/
      ├── exec.ts         spawn wrapper, no shell injection, LC_ALL=C, never rejects
      └── network.ts      pure synchronous parsers for all shell output
```

**Data flow:** topology runs first and yields `gatewayIp` + `interface`. All other checks run concurrently. scoring is a pure function over all results. render and save happen after all checks complete.

**CheckResult<T> contract:** checks never throw. Status is one of `ok | degraded | failed | skipped`. `data` is null only when status is `failed` or `skipped`. Callers inspect `status` and `errors[]`, never catch exceptions from checks.

## Development setup

```bash
# Working directory. Note: /home/maxwell/Projects/ConnectionChecker is currently a
# symlink into the Google Drive Insync path below, not a separate copy — the original
# "avoid Insync sync races" isolation this comment used to describe isn't actually in
# effect. npm install has worked fine here so far; if sync-related flakiness shows up,
# that symlink is why.
cd /home/maxwell/Projects/ConnectionChecker

npm install
npm run typecheck        # tsc --noEmit
npm run test             # vitest run --reporter=verbose (contract/workflow levels need live network)
npm link                 # installs conncheck globally via symlink
conncheck                # full run
conncheck --json | jq .  # JSON mode
conncheck record         # wraps in asciinema
```

## Conventions

- **Spec-driven, test-driven.** Load-bearing/conditional behavior is specified in `docs/specs/` (Given-When-Then, per [MetanoiaFramework's `spec` skill](~/Projects/MetanoiaFramework/skills/spec/SKILL.md)) *before* it's implemented, and implemented test-first (per [MetanoiaFramework's `tdd` skill](~/Projects/MetanoiaFramework/skills/tdd/SKILL.md)). Pure scaffolding (types, config, the exec wrapper) doesn't need a spec. A test cites the scenario it implements as `# spec: <slug>#S<n>`.
- **ESM imports require `.js` extensions.** `NodeNext` moduleResolution enforces this at compile time. `import { x } from './utils/exec.js'` — never omit the extension.
- **Checks never throw.** All failures surface as `CheckResult` state. Only actual spawn failures (`ENOENT`) throw, caught at the check level.
- **Tests are organized by scope, not entry point — never "e2e".** `tests/unit/` (single module, everything else mocked), `tests/contract/` (one real boundary — a real shell command or real network endpoint), `tests/workflow/` (the full CLI, nothing mocked). Contract and workflow tests assert on shape, not exact values — real networks vary; test that `verdict` is one of the three valid strings, not that it equals `'clean'`.
- **No mocks in contract/workflow tests.** They hit real system commands and real network endpoints. `testTimeout: 60_000`. Unit tests are the only level that mocks anything, and most of what's mockable here (shell output parsing, scoring) doesn't need to — it's pure functions over string/data fixtures.
- **`noUncheckedIndexedAccess: true`** in tsconfig. Every array access on shell output is guarded at compile time.

## What to avoid

- **Hostname ping targets.** Captive networks break DNS for ICMP. Always use numeric IPs (`1.1.1.1`, `8.8.8.8`).
- **Quad9 DoH.** Blocked on many public networks. Use only Cloudflare and Google DoH probes.
- **Scanning all interfaces.** `ip addr` shows VMware virtual interfaces. Always follow `ip route show default`'s `dev` field.
- **`ping -i 0.1`.** Non-root minimum interval is 200ms on Linux. Use `-i 0.2`.
- **Root.** Nothing in conncheck requires or requests elevated privileges. `iw scan` is excluded for this reason.
- **Proprietary speed test services, unless no open-source option covers the need.** Ookla EULA §14 prohibits automated use; fast.com is Netflix's closed service. Open source is still the default and every dependency still needs justifying — see [open-source-only decision](docs/decisions/2026-08-02-open-source-only.md) and the narrower fallback carved out in [2026-08-24-ookla-permitted-as-fallback.md](docs/decisions/2026-08-24-ookla-permitted-as-fallback.md).
- **Active scanning for topology.** Passive ARP cache only. See [passive-topology decision](docs/decisions/2026-08-02-passive-topology.md).
- **A bare `#!/usr/bin/env tsx` shebang.** Works in dev (tsx is resolved via `npm run`/`npx`), but breaks after `npm link`: `env` looks for a globally-installed `tsx` binary, which a devDependency isn't. Use `#!/usr/bin/env -S node --import tsx/esm` instead — Node resolves `tsx/esm` through the real project's `node_modules` (following the symlink `npm link` creates), no global tsx install required.

## Documentation index

- [README.md](README.md) — public-facing overview, installation, and usage
- [PLAN.md](PLAN.md) — original implementation plan with interfaces, file-by-file breakdown, and pitfalls; general parameters (checks, scoring model, report shape, tech stack) still hold, but `docs/specs/` is the authoritative behavior contract where the two differ
- [docs/specs/](docs/specs/) — what the system must do, in Given-When-Then scenarios, written before an implementation approach is chosen; cite scenarios by `<slug>#S<n>` from tests
- [docs/decisions/](docs/decisions/) — why key architectural and technology choices were made; read before changing a dependency or adding a new check
  - [2026-08-02-open-source-only.md](docs/decisions/2026-08-02-open-source-only.md) — why MIT/Apache only; why Ookla and fast.com are excluded
  - [2026-08-02-passive-topology.md](docs/decisions/2026-08-02-passive-topology.md) — why no active scanning; what passive ARP gives us
  - [2026-08-02-technology-stack.md](docs/decisions/2026-08-02-technology-stack.md) — rationale for every runtime dependency vs its alternatives
  - [2026-08-02-dns-leak-detection.md](docs/decisions/2026-08-02-dns-leak-detection.md) — why DoH, why Cloudflare+Google only, why `uncertain` beats false-negative
  - [2026-08-24-dns-leak-address-family-matching.md](docs/decisions/2026-08-24-dns-leak-address-family-matching.md) — why only IPv4-vs-IPv4 pairs are comparable; a live dual-stack run broke the original /24-only design
  - [2026-08-24-cloudflare-speedtest-not-node-compatible.md](docs/decisions/2026-08-24-cloudflare-speedtest-not-node-compatible.md) — why the original speed-test library was dropped for a hand-rolled NDT7 client
  - [2026-08-24-ookla-permitted-as-fallback.md](docs/decisions/2026-08-24-ookla-permitted-as-fallback.md) — the narrow exception to open-source-only, and why it hasn't been exercised
  - [2026-08-25-passive-notice-terminal-only-in-json.md](docs/decisions/2026-08-25-passive-notice-terminal-only-in-json.md) — why the passive-ARP notice was dropped from terminal output (proposed, not settled)
  - [2026-08-25-save-off-by-default.md](docs/decisions/2026-08-25-save-off-by-default.md) — why `--save` is now opt-in; there was never a recorded reason for the old default
- [docs/context/](docs/context/) — observed network behavior and domain background; read when debugging a check that behaves unexpectedly on a specific network
  - [network-behavior.md](docs/context/network-behavior.md) — live recon findings that shaped the implementation (captive portals, Quad9 blocking, nmcli quirks, VMware interfaces)
  - [dns-hardening.md](docs/context/dns-hardening.md) — what conncheck's DNS findings mean, how much TLS actually protects against a hostile resolver, how to override DNS globally, and Cloudflare vs Google as a personal choice (not part of conncheck's own leak-detection logic)
  - [nat-traversal.md](docs/context/nat-traversal.md) — how Tailscale punches through NAT; DERP relay fallback
  - [tailscale-wireguard-handshake.md](docs/context/tailscale-wireguard-handshake.md) — WireGuard Noise_IKpsk2 handshake walkthrough; cryptographic primitives
