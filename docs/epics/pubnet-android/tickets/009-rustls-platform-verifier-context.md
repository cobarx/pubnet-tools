---
template_version: 1.0.0
epic: pubnet-android
ticket: 009
slug: rustls-platform-verifier-context
type: feature
points: 3
status: deferred
tracker_ref: tbd
pr: none
related: [android-tls-rustls]
---

# Ticket 009: DoH validation against the device trust store (platform verifier)

## Goal

Let the security check's DoH sub-probe validate TLS against the **device's**
trust store (including user- and enterprise-installed CAs), not just the bundled
Mozilla root set.

## Background — what already works

The DoH probes *do* work on Android as of ticket 5. `crates/pubnetchk/src/tls.rs`
builds the `reqwest::Client` on the `tls-rustls` path with
`use_preconfigured_tls(ClientConfig)` where the config trusts `webpki-roots`
(Mozilla's CA bundle). This was forced by a crash, not a preference:
`rustls-platform-verifier` **`abort()`s the process** on Android when no JVM
`Context` was registered (the cdylib loads through JNA, not `System.loadLibrary`),
and with the workspace `panic = "abort"` that is an uncatchable app kill. See the
2026-09-02 update in
[`2026-08-30-android-tls-rustls.md`](../../decisions/2026-08-30-android-tls-rustls.md).

So this ticket is now an **enhancement**, not a crash fix. It only matters for
users whose DoH endpoints chain to a CA that Mozilla's bundle does not carry
(corp MITM proxies, some captive networks) — where webpki-roots would report the
probe unreachable and the platform verifier would succeed.

## Scope

- **In:** a decision doc — how the verifier gets its `Context`: `init_hosted`
  from a hand-written JNI function Kotlin calls at startup, vs `ndk_context`
  population, vs a UniFFI-exported init taking an opaque handle. The cdylib is
  JNA-loaded, so `JNI_OnLoad` semantics need checking.
- **In:** `crates/pubnetchk-android` — the chosen init entry point (likely a
  `#[cfg(target_os = "android")]` JNI fn; the `jni` crate is already in the tree
  via `rustls-platform-verifier`).
- **In:** `android/` — `MainApplication` / `MainActivity` calls the init with
  `applicationContext` before the first audit.
- **In:** `crates/pubnetchk/src/tls.rs` — once init is guaranteed, drop the
  `use_preconfigured_tls` override on Android (or make it a fallback) so reqwest
  uses the platform verifier.
- **In:** an on-device check that a DoH endpoint behind a device-only CA now
  validates.
- **Out:** the desktop `native-tls` path; the NDT7 WebSocket
  (`rustls-tls-webpki-roots`, ticket 7).

## Acceptance criteria

- On a device with a user-installed CA in the DoH chain: the probe reports
  `reachable: true`.
- If the init is somehow skipped, the audit still completes (no `abort()`):
  either the webpki fallback stays, or the failure is caught into `CheckResult`
  — verify the process does not die.
- Desktop `cargo tree -p pubnet-tools -e normal` unchanged; `just build` /
  `test` / `clippy` unaffected.

## Notes

Keeping `panic = "abort"` for the workspace means the Android path must never
panic — this ticket must not reintroduce an `.expect()` on the verifier without
a fallback.
