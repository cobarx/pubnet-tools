---
template_version: 1.0.0
epic: pubnet-android
ticket: 007
slug: speed-android
type: feature
points: 3
status: in-review
tracker_ref: tbd
pr: tbd
related: [android-ndt7-rustls, android-tls-rustls]
---

# Ticket 007: Speed / NDT7 on Android — validate over rustls

## Goal

Run the NDT7 speed test (`checks::speed`) on Android: the M-Lab locate GET and
the `wss://` download/upload WebSocket, over rustls instead of native-tls.

## Decision doc

`docs/decisions/2026-09-02-android-ndt7-rustls.md` — no explicit
`tokio_tungstenite::Connector` is needed: on the standalone `tls-rustls` build
(`native-tls` absent) `connect_async`'s `connector: None` path auto-selects
rustls + `webpki-roots`.

## Scope

- **In:** `docs/decisions/2026-09-02-android-ndt7-rustls.md`.
- **In:** `crates/pubnetchk-android` — `"speed"` added to `AndroidOptions.only`'s
  default (all four checks now run).
- **In:** `justfile` — `test-speed-rustls` (runs the `speed` contract test
  `--no-default-features --features tls-rustls`).
- **Already done upstream:** `default_locate` uses `crate::tls::client_builder()`
  (webpki `ClientConfig`) — from the SIGABRT fix. `MainScreen.kt`'s Performance
  section already renders `speed.data` (from ticket 6) — it stops showing "not
  on Android yet" once speed is in `only`.
- **Out:** a mobile-data-aware default (speed off / shorter on cellular) — a UI
  toggle, follow-up; `speedDurationSecs` stays 10.

## Acceptance criteria

- `cargo test -p pubnet-tools --no-default-features --features tls-rustls --test
  speed` passes (real M-Lab, asserts shape).
- On a device: Scan → the Performance section shows a download/upload number
  within ~25 s; the report JSON's `speed` section has `status: "ok"`,
  `source: "ndt7"`, `downloadMbps > 0`.
- Desktop unchanged: default-feature `cargo test` / `cargo tree` for
  `pubnet-tools` unaffected (still native-tls for the WebSocket).

## Notes

Verified on the dev host 2026-09-02:
`cargo run -p pubnet-tools --no-default-features --features tls-rustls -- --json
--only speed` → `status: ok`, ~368/120 Mbps, `source: "ndt7"`.
