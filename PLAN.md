# conncheck — Implementation Plan

## What it is

A TypeScript CLI that audits the public WiFi/network you just joined: security posture (encryption, DNS leak, captive portal), speed (via Cloudflare's open infrastructure), reliability (ping/jitter/loss), and passive network topology. Scores the result Low/Medium/High and saves a JSON report. A `record` subcommand wraps the run in asciinema for blog post session capture.

**Values driving every decision:** open source only (MIT/Apache/ISC), technology choices justified against alternatives, good citizen (passive topology, no port scanning), real-world integration tests only (no mocks).

**Working directory:** `/home/maxwell/Projects/ConnnectionChecker` — NOT the Google Drive Insync path (Insync sync races corrupt `npm install`).

---

## Live environment findings

Discovered by running reconnaissance on the actual target machine before writing this plan. These shaped several design decisions:

- Currently on `Berkeley-Visitor` — **open network** (empty security field in nmcli output). Empty = Open, not a parse error.
- `ping one.one.one.one` → 100% loss while `ping 1.1.1.1` succeeds. Captive/filtered networks break name-based ICMP. Per-target failure must not abort the whole reliability check.
- **Quad9 DoH is blocked** on this network. DNS leak detection must work with just Cloudflare + Google DoH. If both are blocked, report `uncertain` — never false-negative as "no leak."
- `resolvectl status` shows `resolv.conf mode: foreign` — `/etc/resolv.conf` is written by NetworkManager directly. Parse `resolvectl status` for active link DNS; fall back to `/etc/resolv.conf` only if resolvectl finds no servers for the active link.
- `iw scan` requires root. Use `nmcli` exclusively for WiFi info (no root needed).
- VMware virtual interfaces (`vmnet1`, `vmnet8`) appear in `ip addr`. Topology must follow the default route's interface, not scan all interfaces.

---

## Technology rationale

Every choice justified against its alternatives.

### TypeScript (Apache-2.0)
Typed `CheckResult<T>` generics and shell output parsers need compile-time safety. Plain JS loses the type net exactly where parsing is most error-prone. Deno would break the npm ecosystem we're already in.

### tsx 4.x (MIT)
Run `.ts` files directly without a build step — both in dev and as the bin shebang.
- **vs `ts-node`:** tsx uses esbuild internally, cold start <100ms vs ~500ms.
- **vs a build step:** eliminates the dist/ compile cycle during development.
- Trade-off: tsx doesn't type-check; that's `tsc --noEmit`'s job in CI.

### commander 15.x (MIT)
Subcommand dispatch (`conncheck record`), flag parsing, `--help` generation.
- **vs yargs:** heavier (pulls in `cliui`, `y18n`, string-width).
- **vs meow:** no subcommand routing.
- **vs oclif:** full framework, overkill.
- Commander 15 has zero runtime dependencies.

### chalk 6.x (MIT)
ANSI color for severity indicators.
- **vs kleur/picocolors/ansis:** all MIT and faster, but chalk's `NO_COLOR`/TTY auto-detection and `chalk.level` API for `--no-color` are first-class.
- Performance difference is irrelevant on a CLI running multi-second network checks.
- Trade-off: chalk 6 is pure ESM — forces `"type": "module"` (which we want anyway).

### ora 9.x (MIT)
Animated spinners while checks run.
- **vs listr2:** would require wrapping every check as a task object, changing the architecture.
- **vs nanospinner:** less ergonomic `.warn()` state.
- ora's `.start()/.succeed()/.fail()/.warn()` API maps exactly onto our `CheckStatus` states. Pure ESM.

### cli-table3 0.6.x (MIT)
Ping results table.
- **vs columnify:** alignment only, no borders.
- **vs console.table:** not customizable.
- Bundles its own `.d.ts` types.

### axios 1.x (MIT)
HTTP for captive portal detection and DoH queries.
- **vs native fetch:** `fetch` with `redirect: 'manual'` exists in Node 18+ but capturing the `Location` header requires extra work and TypeScript typing is awkward.
- **vs got:** heavier (multiple internal packages).
- axios's `{ maxRedirects: 0, validateStatus: () => true }` pattern captures redirect `Location` cleanly — this is the concrete reason axios is a dependency, not convenience.

### @cloudflare/speedtest 1.x (MIT)
Open-source speed measurement.

**Hard-rejected alternatives:**
- `speedtest-net` / Ookla CLI: proprietary EULA §14 explicitly prohibits automated use without a commercial agreement.
- `fast-speedtest-api` / fast.com: Netflix's closed commercial infrastructure.
- DIY HTTP timing: measures TCP+TLS+TTFB, not true bandwidth. Meaningless without a controlled reference server.

`@cloudflare/speedtest` is the same source code powering `speed.cloudflare.com`. MIT licensed. Cloudflare's anycast edge minimizes route variance on public WiFi. The library uses `fetch()` internally — Node 22 (on this machine) provides it natively.

**Confirmed API shape** (from inspecting `dist/speedtest.js`):
```typescript
import SpeedTestEngine from '@cloudflare/speedtest';
const engine = new SpeedTestEngine({ autoStart: false });
engine.onFinished = (results) => {
  const s = results.getSummary();
  // s.download → bits/sec, s.upload → bits/sec, s.latency → ms, s.jitter → ms
};
engine.onError = (err) => { /* ... */ };
engine.play();
```

### vitest 4.x (MIT)
Integration test runner.
- **vs Jest:** requires `--experimental-vm-modules` for ESM as of 2025 — still painful.
- **vs `node:test`:** no `describe`/`it` nesting, inadequate timeout controls.
- vitest has native ESM+TS via esbuild, per-test `timeout`, and `--reporter=verbose` that reads well in terminals.

---

## Project structure

```
ConnnectionChecker/
├── package.json
├── tsconfig.json
├── vitest.config.ts
├── LICENSE
├── PLAN.md
├── src/
│   ├── index.ts              # shebang + CLI entry
│   ├── cli.ts                # commander setup, run + record commands
│   ├── scoring.ts            # pure risk score calculation
│   ├── types.ts              # all TypeScript interfaces
│   ├── checks/
│   │   ├── security.ts       # WiFi encryption, DNS, DNS leak, captive portal
│   │   ├── speed.ts          # @cloudflare/speedtest wrapper
│   │   ├── reliability.ts    # ping x3 targets, jitter as stddev
│   │   └── topology.ts       # ip addr, ip route, ip neigh (passive)
│   ├── output/
│   │   ├── renderer.ts       # chalk + ora + cli-table3 terminal display
│   │   └── reporter.ts       # JSON report writer (~/.conncheck/reports/)
│   └── utils/
│       ├── exec.ts           # child_process.spawn wrapper (no shell injection)
│       └── network.ts        # pure synchronous shell output parsers
└── tests/
    ├── topology.integration.test.ts
    ├── reliability.integration.test.ts
    ├── security.integration.test.ts
    ├── speed.integration.test.ts
    └── scoring.test.ts       # pure unit tests, no network
```

---

## TypeScript interfaces (`src/types.ts`)

```typescript
export type CheckStatus = 'ok' | 'degraded' | 'failed' | 'skipped';
// ok=complete, degraded=partial data, failed=no data, skipped=precondition absent

export type Severity = 'good' | 'warn' | 'alert' | 'info';

export interface Finding {
  id: string;           // stable key e.g. 'wifi.open', 'dns.leak'
  severity: Severity;
  points: number;       // 0 for good/info
  title: string;
  detail?: string;
}

export interface CheckResult<T> {
  name: string;
  status: CheckStatus;
  data: T | null;       // null only when status === 'failed' | 'skipped'
  errors: string[];
  findings: Finding[];
  durationMs: number;
}

// --- Security ---

export type WifiEncryption = 'WPA3' | 'WPA2' | 'WPA2-Enterprise' | 'WPA' | 'Open' | 'Unknown';

export interface DnsResolverInfo {
  link: string;
  currentServer: string | null;
  servers: string[];
  source: 'resolvectl' | 'resolv.conf';
}

export interface DohProbe {
  provider: 'cloudflare' | 'google';
  egressIp: string | null;
  reachable: boolean;
}

export interface DnsLeakResult {
  systemEgressIp: string | null;    // from resolvectl query whoami.cloudflare.com
  probes: DohProbe[];
  leaked: boolean;
  verdict: 'clean' | 'leaked' | 'uncertain'; // uncertain = all probes unreachable
}

export interface CaptivePortalResult {
  detected: boolean;
  method: 'redirect' | 'content-mismatch' | 'none';
  redirectLocation: string | null;
  canaryUrl: string;
  httpStatus: number | null;
}

export interface SecurityData {
  ssid: string | null;
  encryption: WifiEncryption;
  dns: DnsResolverInfo | null;
  dnsLeak: DnsLeakResult;
  captivePortal: CaptivePortalResult;
}

// --- Speed ---

export interface SpeedData {
  downloadMbps: number;
  uploadMbps: number;
  latencyMs: number;
  jitterMs: number;
  source: '@cloudflare/speedtest';
}

// --- Reliability ---

export interface PingTargetResult {
  host: string;
  label: 'gateway' | 'google-dns' | 'cloudflare-dns';
  transmitted: number;
  received: number;
  packetLossPct: number;
  minMs: number | null;
  avgMs: number | null;
  maxMs: number | null;
  jitterMs: number | null;  // stddev of individual RTTs (not ping's mdev)
  rtts: number[];           // per-packet RTTs from non-quiet output
  reachable: boolean;
}

export interface ReliabilityData {
  targets: PingTargetResult[];
  gatewayReachable: boolean;
  internetReachable: boolean;
}

// --- Topology ---

export interface ArpNeighbor {
  ip: string;
  mac: string | null;
  state: string;
  device: string;
  isGateway: boolean;
}

export interface TopologyData {
  interface: string;
  ipCidr: string;         // e.g. "10.59.140.42/22"
  gateway: string;
  neighbors: ArpNeighbor[];
  passiveNotice: string;  // always "Passive ARP cache — no active scan performed."
}

// --- Report ---

export type RiskLevel = 'Low' | 'Medium' | 'High';

export interface Report {
  version: string;
  timestamp: string;      // ISO 8601
  security: CheckResult<SecurityData>;
  speed: CheckResult<SpeedData>;
  reliability: CheckResult<ReliabilityData>;
  topology: CheckResult<TopologyData>;
  score: { total: number; level: RiskLevel; findings: Finding[] };
}
```

---

## File-by-file breakdown

### `src/index.ts`
Shebang entry point only. `#!/usr/bin/env tsx` + `buildCli().parseAsync(process.argv)`. No logic.

### `src/cli.ts`
Commander setup. Two commands:

**Default command (`conncheck` / `conncheck run`):** orchestrates all checks, manages ora spinners around each await, feeds results to renderer, saves JSON report. Flags:
- `--json` — print JSON to stdout, suppress spinners
- `--no-save` — skip writing the report file
- `--only <checks>` — comma list to run a subset
- `--strict` — exit non-zero on Medium/High (useful in CI)

**`conncheck record`:** checks for asciinema via `which asciinema`, detects version, constructs the correct invocation:
- v2: `asciinema rec <file> -- conncheck`
- v3: `asciinema rec --output <file> -- conncheck`

Saves to `~/.conncheck/recordings/YYYY-MM-DD_HH-MM-SS.cast`. Uses `child_process.spawn` with `stdio: 'inherit'` so the PTY passes through. If asciinema is absent: print install hint (`sudo pacman -S asciinema`) and exit 1.

### `src/types.ts`
All interfaces above. Zero runtime code.

### `src/scoring.ts`
Pure function `calculateScore(report)`. No I/O. The only place findings are mapped to points and band thresholds are applied.

### `src/checks/security.ts`
Four sub-checks; one failure doesn't abort the others:

1. **WiFi:** `nmcli -t -f active,ssid,security dev wifi list` → filter `active=yes` row → take last colon-delimited field as security. Empty = `Open`. Handle SSIDs with colons by splitting max 3 parts.

2. **DNS:** `resolvectl status` → parse link-scoped block for the active interface. Fallback to `/etc/resolv.conf`.

3. **DNS leak:** Query `whoami.cloudflare.com TXT` via:
   - System: `resolvectl query --type=TXT whoami.cloudflare.com`
   - Cloudflare DoH: `GET https://cloudflare-dns.com/dns-query?name=whoami.cloudflare.com&type=TXT`
   - Google DoH: `GET https://dns.google/resolve?name=whoami.cloudflare.com&type=TXT`

   TXT records contain `remote_ip=<egress IP>`. Compare system egress to DoH egress by /24 prefix. Divergence = leaked. All probes unreachable → `verdict: 'uncertain'`. axios timeout 8s. Quad9 skipped (blocked on many networks).

4. **Captive portal:** axios GET to `http://connectivitycheck.gstatic.com/generate_204` (expects 204) and `http://captive.apple.com/hotspot-detect.html`, `{ maxRedirects: 0, validateStatus: () => true, timeout: 5000 }`. 3xx or unexpected body = detected. Capture `Location` header.

### `src/checks/speed.ts`
Wrap `@cloudflare/speedtest` event API in a `Promise`. Hard timeout via `Promise.race` at 60s. Convert bits/sec → Mbps.

### `src/checks/reliability.ts`
Accepts `gatewayIp: string`. Targets: gateway, `8.8.8.8`, `1.1.1.1` (not `one.one.one.one` — name resolution breaks on captive networks). All three via `Promise.allSettled`.

Command: `ping -c 10 -i 0.2 <host>` with `LC_ALL=C`. Parse non-quiet output for per-packet `time=X.XX ms` lines → `rtts[]`. Jitter = population stddev of `rtts`.

### `src/checks/topology.ts`
Sequential: `ip route show default` → gateway + interface → `ip addr show <iface>` → IP/CIDR → `ip neigh show dev <iface>` → ARP neighbors. Mark gateway entry `isGateway: true`. `passiveNotice` is a hardcoded constant in every output and every JSON report.

### `src/output/renderer.ts`
All chalk/ora/cli-table3 formatting. Takes a `Report`, produces terminal output. Never touches the network. cli-table3 style `{ head: [], border: [] }` — apply chalk colors to cell values directly to avoid the `colors` package conflict.

### `src/output/reporter.ts`
`saveReport(report): Promise<string>`. Creates `~/.conncheck/reports/` with `{ recursive: true }`. Filename from `report.timestamp` with colons replaced by dashes. Returns the saved path.

### `src/utils/exec.ts`
`execCmd(cmd: string[], timeoutMs?: number): Promise<{stdout, stderr, exitCode}>`. Uses `child_process.spawn` with array args — no shell, no injection surface. AbortController for timeout. Never rejects on non-zero exit; callers inspect `exitCode`. Injects `LC_ALL=C` into env for stable output parsing.

### `src/utils/network.ts`
Pure synchronous parsers — all testable without a network:
- `parseNmcliWifi(raw)` → `{ ssid, security } | null`
- `parseResolvectlStatus(raw, iface)` → `DnsResolverInfo | null`
- `parseIpRoute(raw)` → `{ gateway, device } | null`
- `parseIpAddr(raw, iface)` → `{ ip, prefix } | null`
- `parseIpNeigh(raw)` → `ArpNeighbor[]`
- `parsePingOutput(raw)` → `{ transmitted, received, rtts: number[] }`
- `stddev(values: number[])` → `number`
- `isValidIPv4(s)` → `boolean`

---

## Risk scoring (`src/scoring.ts`)

Additive point model. `good`/`info` findings = 0 points.

| Finding | Points | Severity |
|---|---|---|
| Open WiFi | 40 | alert |
| WPA (not WPA2/3) | 20 | warn |
| WPA2 (not WPA3) | 5 | info |
| WPA3 / Enterprise | 0 | good |
| DNS leak confirmed | 25 | alert |
| DNS leak uncertain (all probes blocked) | 5 | warn |
| Captive portal detected | 15 | warn |
| Gateway unreachable | 30 | alert |
| Internet unreachable (all external targets) | 25 | alert |
| Packet loss > 10% on any target | 10 | warn |
| Avg RTT > 200ms on any target | 5 | warn |
| Jitter > 30ms | 5 | warn |
| Download < 1 Mbps | 10 | warn |
| Speed check failed | 5 | warn |

**Bands:**
- 0–19 → **Low** (green)
- 20–49 → **Medium** (yellow)
- 50+ → **High** (red)

**Calibration:** Berkeley-Visitor (open network) = 40 pts → High. Correct — open public network is inherently high risk. Corporate WPA3, good speeds = 0 pts → Low.

---

## Integration tests

`vitest.config.ts`: `testTimeout: 60_000`, `sequence.concurrent: false`, `include: ['tests/**/*.integration.test.ts']`.

Assert on **invariants and shape**, not exact values — real networks vary.

```typescript
// topology.integration.test.ts
test('discovers default interface, gateway, and ARP neighbors passively', async () => {
  const r = await runTopologyCheck();
  expect(r.status).not.toBe('failed');
  expect(r.data!.interface).toMatch(/^\w+$/);
  expect(r.data!.gateway).toMatch(/^(\d{1,3}\.){3}\d{1,3}$/);
  expect(r.data!.passiveNotice).toContain('no active scan');
});

// reliability.integration.test.ts
test('per-target failure does not abort the check', async () => {
  const r = await runReliabilityCheck(gatewayIp);
  expect(r.status).not.toBe('failed');
  expect(r.data!.targets).toHaveLength(3);
  for (const t of r.data!.targets) {
    expect(t.transmitted).toBe(10);
    expect(t.packetLossPct).toBeGreaterThanOrEqual(0);
    if (t.reachable) {
      expect(t.rtts.length).toBeGreaterThan(0);
      expect(t.jitterMs).toBeGreaterThanOrEqual(0);
      expect(t.minMs).toBeLessThanOrEqual(t.avgMs!);
    }
  }
});

// security.integration.test.ts
test('DoH probes run against real endpoints', async () => {
  const r = await runSecurityCheck();
  expect(r.data!.dnsLeak.probes.length).toBeGreaterThan(0);
  expect(['clean', 'leaked', 'uncertain']).toContain(r.data!.dnsLeak.verdict);
});

// speed.integration.test.ts
test('returns data or fails gracefully', async () => {
  const r = await runSpeedCheck();
  if (r.status === 'ok') {
    expect(r.data!.downloadMbps).toBeGreaterThan(0);
    expect(r.data!.source).toBe('@cloudflare/speedtest');
  } else {
    expect(r.data).toBeNull();
    expect(r.errors.length).toBeGreaterThan(0);
  }
}, 60_000);
```

`tests/scoring.test.ts` — pure unit tests with synthetic `Report` objects. No network, no timeout.

---

## Error handling philosophy

**Core rule: checks never throw.** All failures become `CheckResult` state.

| Status | Meaning |
|---|---|
| `ok` | Full data, all sub-checks succeeded |
| `degraded` | Partial data — one DoH probe unreachable, one ping target down |
| `failed` | No usable data — `data: null`, `errors[]` explains why |
| `skipped` | Precondition absent — no WiFi, no default route. Not an error. |

**No-network behavior:** topology detects no default route → `skipped` with "No default route". Downstream checks receive this via context and short-circuit to `skipped` rather than timing out. Risk scorer treats `skipped` as unknown (no points, no penalties).

`exec.ts` never rejects on non-zero exit — callers inspect `exitCode`.

---

## Implementation sequencing

| Phase | Work | Gate |
|---|---|---|
| 1 | Scaffold: `package.json`, `tsconfig.json`, `vitest.config.ts`, `LICENSE` | `tsc --noEmit` passes |
| 2 | Utils: `exec.ts`, `network.ts` pure parsers | Sanity `tsx` script runs |
| 3 | Topology check + integration test | First passing test |
| 4 | Reliability check + integration test | Per-target failure resilience verified |
| 5 | Security check + integration test | DoH + captive portal tested live |
| 6 | Speed check + integration test | Graceful `onError` fallback verified |
| 7 | Scoring + pure unit tests | Band thresholds locked |
| 8 | Output: renderer + reporter | Visual check with `tsx src/index.ts` |
| 9 | CLI wiring + `record` subcommand + `npm link` | End-to-end `conncheck` works |

---

## Key pitfalls

1. **ESM imports need `.js` extension** — `import { x } from './utils/exec.js'`. `NodeNext` moduleResolution enforces this at compile time.

2. **Ping minimum interval** — `ping -i 0.2` (200ms) is the floor for non-root on Linux. Never use `-i 0.1`, never suggest root.

3. **nmcli SSID colons** — SSIDs can contain colons. Split terse output max 3 parts; last part = security, empty = Open.

4. **resolvectl link scope** — Parse the block for the active interface. The global `Fallback DNS Servers` line (Quad9) appears even when the active resolver is `128.32.x`. Don't grep for the first DNS line.

5. **`resolvectl query` flag** — Use `--type=TXT`, not `-t TXT`. Short flags differ by version.

6. **Quad9 DoH blocked on many networks** — DNS leak detection must function with just two providers. "All probes blocked" ≠ "no leak" → report `uncertain`.

7. **VMware interfaces** — `ip addr` shows `vmnet1`, `vmnet8`. Always follow the default route's interface name from `ip route show default`.

8. **`@cloudflare/speedtest` is browser-first** — Uses `fetch()`. Node 20+ required. If a captive portal blocks `speed.cloudflare.com`, `onError` fires — itself a useful data point.

9. **cli-table3 + chalk color conflict** — Set `style: { head: [], border: [] }` on the table; apply chalk coloring directly to cell values.

10. **asciinema v2 vs v3** — Detect major version, branch the invocation. The `--` separator before `conncheck` is mandatory.

11. **`noUncheckedIndexedAccess: true`** in tsconfig — catches `lines[0]` bugs in shell output parsing at compile time.

---

## npm dependencies

### Runtime (all MIT)

| Package | Version | Why |
|---|---|---|
| `commander` | `^15.0.0` | CLI parsing, subcommands, zero deps |
| `chalk` | `^6.0.0` | Color, NO_COLOR support, pure ESM |
| `ora` | `^9.4.1` | Spinners with ok/warn/fail states |
| `cli-table3` | `^0.6.5` | Ping table with borders, bundled types |
| `axios` | `^1.7.7` | maxRedirects:0 + Location capture for portal detection |
| `@cloudflare/speedtest` | `^1.13.0` | MIT speed test, same engine as speed.cloudflare.com |

### Dev

| Package | Version | Why |
|---|---|---|
| `typescript` | `^7.0.2` | Type checking via `tsc --noEmit` |
| `tsx` | `^4.23.4` | Run .ts directly, shebang runtime |
| `vitest` | `^4.1.10` | Native ESM+TS test runner |
| `@types/node` | `^22.0.0` | Node built-in types |

**Total runtime deps: 6. All MIT. No Ookla. No fast.com.**

---

## Verification checklist

1. `npm run typecheck` — zero errors
2. `vitest run --reporter=verbose` — all integration tests pass on a live network
3. `npm link && conncheck` — full run with spinners, risk score, JSON saved to `~/.conncheck/reports/`
4. `conncheck --json | jq '.score.level'` — JSON output mode works
5. `conncheck record` — produces `.cast` file, verify with `asciinema play`
