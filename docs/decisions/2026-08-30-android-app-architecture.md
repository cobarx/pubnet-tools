---
template_version: 1.4.0
date: 2026-08-30
slug: android-app-architecture
status: accepted
decided_by: hampton
related: [2026-08-30-android-tls-rustls, 2026-08-28-windows-probes-via-win32-api]
---

# Decision: Android app = Rust engine + UniFFI + JSON snapshot bridge

## Context

`pubnetchk` is a desktop CLI. We want an Android app that runs the same audit on
the network the phone is joined to. The engine (`checks`, `scoring`, `network`
parsers, the `PlatformProbe` seam) is already OS-abstracted; the question is how
Kotlin drives it, and how the OS facts a probe needs get in when the app
**cannot shell out** to `ip` / `nmcli` / `resolvectl` / `ping`.

Full rationale and the ticket breakdown live in
[`docs/epics/pubnet-android/epic.md`](../epics/pubnet-android/epic.md); this
records the load-bearing choices.

## Decision

### 1. A UniFFI `cdylib`, `crates/pubnetchk-android`

A fourth workspace crate, `crate-type = ["cdylib", "lib"]`, depending on
`pubnet-tools` (the engine) and `pubnet-platform`. It uses UniFFI proc-macro
mode (`setup_scaffolding!()` — no UDL, no build script) and library-mode
`uniffi-bindgen` to emit the Kotlin.

### 2. The bridge is JSON in, JSON out — not generated records

```
run_audit_json(snapshot_json: String, options_json: String) -> Result<String, AuditError>
```

- **In:** a `HostSnapshot` (see
  [`docs/specs/android-host-snapshot.md`](../specs/android-host-snapshot.md)).
- **Out:** the `Report` as its existing JSON.

The report crosses as a string because its JSON schema is *already* a maintained
contract (the `--json` output, the HTML report). Deriving `uniffi::Record` /
`uniffi::Enum` across the ~25 types in `pubnet_tools::types` +
`pubnet_platform::types` would be a large, permanent annotation burden on those
modules for no gain over `serde_json` on the Kotlin side.

### 3. Facts flow as a one-shot snapshot — no callback interface

Kotlin gathers everything the probe needs (`ConnectivityManager`,
`WifiManager`, `LinkProperties`, `/proc/net/arp`) **once, up front**, and passes
it in. `SnapshotProbe` (`pubnet_platform::platform::snapshot`) implements
`PlatformProbe` by returning those pre-fetched values with no I/O.

Rejected: a UniFFI `callback_interface` that the Rust core calls back into per
probe method. That means async-over-FFI and a Kotlin object whose methods the
Rust side invokes mid-audit — much more surface, for data Kotlin can just as
easily collect in one pass (topology needs the interface + gateway first, but
Kotlin reads both from `LinkProperties` without help).

### 4. `run_audit` split into `audit::run_audit_with_probe`

The orchestration (four checks, spinner, `Report` assembly) moves from `cli.rs`
to a new platform-neutral `pubnet_tools::audit`. `cli.rs` — clap, the desktop
`run` / `record` paths — becomes `#[cfg(not(target_os = "android"))]`. The
Android crate calls `run_audit_with_probe(&SnapshotProbe::new(..), opts)`
directly; `quiet: true` suppresses the spinner.

### 5. Build: `org.mozilla.rust-android-gradle` + `cargo-ndk`

The Gradle plugin drives `cargo-ndk` for the ABI matrix and packages the `.so`
into `jniLibs`. Bindings are generated from a **debug** cdylib (the workspace
release profile's `strip = true` removes UniFFI's metadata; the runtime only
needs the `extern "C"` entry points, which survive stripping). (`android/`
Gradle project: ticket 4.)

## Consequences

- `pubnet_tools::types` stays annotation-free; the JSON schema is the one
  contract to keep stable, and it already is.
- Adding reliability / speed to Android is engine-side work (an
  unprivileged-ICMP `system_ping`; validating NDT7 over rustls) — no bridge
  change; `AndroidOptions.only` just grows.
- The Android crate pulls `clap` / `indicatif` transitively through
  `pubnet-tools`. Extracting a leaf `pubnetchk-core` crate is a tracked
  follow-up, not a blocker.
- `reqwest`'s rustls path needs an Android context for
  `rustls-platform-verifier` — see
  [2026-08-30-android-tls-rustls.md](2026-08-30-android-tls-rustls.md).

## Revisit if

- The Kotlin side ends up re-parsing the report JSON into a full typed model by
  hand anyway → reconsider generating `uniffi::Record`s for the `Report` subtree
  only.
- A second non-CLI consumer appears → promote `audit` + the snapshot types into
  a dedicated `pubnetchk-core` crate.
