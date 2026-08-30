---
template_version: 1.0.0
epic: pubnet-android
ticket: 003
slug: uniffi-cdylib
type: feature
points: 5
status: in-review
tracker_ref: tbd
pr: none
related: []
---

# Ticket 003: `pubnetchk-android` UniFFI cdylib + `run_audit_json`

## Goal

A new crate that exposes the audit to Kotlin as a single blocking call:
snapshot JSON in, report JSON out.

## Scope

- **In:** extract a spinner-free orchestrator in
  `crates/pubnetchk/src/cli.rs`:
  - `pub async fn run_audit_with_probe<P: PlatformProbe>(probe: &P, options:
    RunAuditOptions) -> Report` — the body of today's `run_audit` minus the
    `indicatif` spinner creation/finish and minus the `#[cfg(target_os)]` probe
    selection.
  - `run_audit` becomes: build the `#[cfg]` probe, make the spinner, call
    `run_audit_with_probe`, finish the spinner. **No CLI behavior change** —
    existing `cli.rs` unit tests stay green.
- **In:** `crates/pubnetchk-android/`:
  - `Cargo.toml` — `name = "pubnetchk-android"`, `crate-type = ["cdylib",
    "staticlib"]`; deps: `pubnet-tools = { path = "../pubnetchk",
    default-features = false, features = ["tls-rustls"] }`, `pubnet-platform`,
    `uniffi` (proc-macro mode), `serde`, `serde_json`, `tokio` (`rt`,
    `macros`). `[[bin]] uniffi-bindgen` shim per UniFFI docs.
  - `src/lib.rs`:
    - `uniffi::setup_scaffolding!()`
    - `#[uniffi::export] fn run_audit_json(snapshot_json: String, options_json:
      String) -> Result<String, AuditError>` — deserialize `HostSnapshot` and a
      small `AndroidOptions` (which checks to run, speed duration; skeleton
      passes `only = [topology, security]`), build a current-thread tokio
      runtime, `block_on(run_audit_with_probe(&SnapshotProbe { .. }, ..))`,
      `serde_json::to_string(&report)`.
    - `#[uniffi::export] fn report_schema_version() -> String`
    - `#[derive(uniffi::Error)] enum AuditError { BadSnapshot(String) }`
  - Add `crates/pubnetchk-android` to `Cargo.toml` `[workspace] members`.
- **In:** `justfile` — `android-lib` recipe running
  `cargo ndk -t arm64-v8a -t x86_64 -t armeabi-v7a -o android/app/src/main/jniLibs
  build -p pubnetchk-android --release`, and `android-bindings` running the
  `uniffi-bindgen` Kotlin generation.
- **Out:** any `types.rs` changes (report crosses as a string); the Gradle
  project (ticket 4); reliability/speed wiring.

## Acceptance criteria

- `cargo build -p pubnetchk-android` succeeds on the host.
- `cargo test -p pubnet-tools --lib` still passes (refactor is behavior-neutral).
- with the Android Rust targets already installed by the owner, `cargo ndk -t
  arm64-v8a build -p pubnetchk-android` produces `libpubnetchk_android.so`.
- `cargo run -p pubnetchk-android --bin uniffi-bindgen -- generate --library
  <.so> --language kotlin --out-dir /tmp/bindings` emits a `.kt` file with a
  `runAuditJson` function.
- A host-side unit test feeds a captured `HostSnapshot` JSON to `run_audit_json`
  with `only = [topology]` and asserts the returned JSON parses as a `Report`
  with a `topology` section.

## Notes

`cargo-ndk`, the Android NDK, and the Android Rust targets are owner-installed
prerequisites (see the epic) — document required versions in `android/README.md`
(ticket 4); do not `cargo install` or `rustup target add` them here. Keep
`AndroidOptions` minimal and camelCase; it will grow when reliability/speed
land.
