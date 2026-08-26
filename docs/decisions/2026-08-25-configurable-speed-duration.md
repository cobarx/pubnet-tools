---
template_version: 1.2.0
date: 2026-08-25
slug: configurable-speed-duration
status: accepted
decided_by: hampton
related: [2026-08-24-cloudflare-speedtest-not-node-compatible]
---

# Decision: Speed-test duration is configurable (`--speed-duration`, `-q/--quick`), default unchanged

## Context

`security`, `reliability`, and `speed` already run concurrently (`tokio::join!` in
`cli.rs`), and `reliability` pings all targets concurrently too — the checks are
parallelized everywhere it's safe to do so. Once that was confirmed, the remaining
question was why a full run still takes ~20s+: `speed.rs` measures download and upload
sequentially, 10 seconds each (the NDT7 client convention this project already follows,
per [cloudflare-speedtest-not-node-compatible](2026-08-24-cloudflare-speedtest-not-node-compatible.md)),
and that pair of fixed windows is ~90% of total run time.

Download and upload can't be measured concurrently without corrupting both numbers —
running them at once means each direction's throughput reflects contention with the
other, not the link's actual one-way capacity. That's inherent to the measurement, not
a parallelism gap in this codebase. The only real lever is the window length itself.

## Decision

`check_speed` takes `test_duration: Duration` as a parameter instead of a hardcoded
constant. The CLI exposes two ways to set it, mutually exclusive (usage error, exit
code 2, if both given — same shape as `--only`/`--no-<check>`):

- `--speed-duration <SECONDS>` — exact control, per direction. Rejects `0`.
- `-q, --quick` — preset shorthand for `--speed-duration 3`.

Default stays 10 seconds per direction when neither flag is given — unchanged from
today, and consistent with `ndt7-js`'s own default.

## Rationale

Shortening the window is a real accuracy tradeoff, not a pure win: shorter samples are
noisier, and links with bufferbloat or rate-limiting that only shows up after the first
few seconds will look better than they are on a 3s window than on a 10s one. That's why
this is opt-in via an explicit flag rather than a lowered default — the 10s baseline
this project already uses is kept as-is, and speed is traded for accuracy only when the
user asks for it (e.g. iterating on other parts of a run, or a quick sanity check where
precision doesn't matter).

Two flags instead of one: `-q/--quick` covers the common case (just make it fast)
without requiring the user to pick a number, while `--speed-duration` covers picking a
specific value. `-f/--fast` was considered and dropped — `-f` is a natural short flag
for a future `--force`, and reserving it now for something else would mean relitigating
this later. `-q` was free.

No upper or lower bound beyond rejecting `0` (a zero-second window can't produce a
throughput number at all) — an unreasonably large value is the user's own call to make,
not something this tool should second-guess.

## Stakeholders

Solo call — no other stakeholders consulted.

## Considerations / Revisit if

- **`-q` is currently free.** Today: no other flag claims it. Revisit if: a future flag
  has a stronger claim to `-q` than "quick speed test" — unlikely, since it's already
  scoped to the one check that dominates run time.
- **3 seconds was picked without measurement.** Today: a round, clearly-short number
  chosen for readability, not derived from data on how noisy a 3s NDT7 sample actually
  is on this project's typical test networks. Revisit if: real usage shows 3s produces
  numbers users don't trust, or a different value is needed for a different check should
  a "quick mode" broaden beyond just `speed`.

## Consequences

- `speed.rs`'s `TEST_DURATION` constant becomes `DEFAULT_TEST_DURATION`, still 10s, now
  passed explicitly by callers instead of used internally.
- `rust/tests/speed.rs` (contract test against real M-Lab servers) now passes a shorter
  duration to keep the test fast and reduce repeated-run rate-limit pressure noted
  during this session's earlier speed-check work — the contract test's own runtime was
  never the point of the 10s default.
