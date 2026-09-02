# pubnetchk-android

UniFFI bindings for the `pubnetchk` audit engine — the Rust half of the Android
app (see [`docs/epics/pubnet-android/`](../../docs/epics/pubnet-android/)).

## What it exposes

One call, JSON in / JSON out — the `Report` crosses the FFI as a string rather
than as generated records, so `pubnet_tools::types` stays free of `uniffi`
derives and the existing JSON schema is the contract.

| Rust | Kotlin |
|---|---|
| `run_audit_json(snapshot_json, options_json) -> Result<String, AuditError>` | `runAuditJson(snapshotJson, optionsJson): String` *(throws `AuditException`)* |
| `report_schema_version() -> String` | `reportSchemaVersion(): String` |

- `snapshot_json` — a `HostSnapshot`
  ([`docs/specs/android-host-snapshot.md`](../../docs/specs/android-host-snapshot.md)).
- `options_json` — an `AndroidOptions` object, or `"{}"` for the default
  (`only: ["topology", "security"]`). Other fields: `speedDurationSecs`,
  `wifiDetail`.
- returns the `Report` JSON — identical schema to `pubnetchk --json`.
- `run_audit_json` **blocks** for the length of the audit (it builds its own
  tokio runtime). Call it off the main thread.

TLS is rustls (`pubnet-tools` built with `--features tls-rustls`) — Android has
no app-accessible system TLS. See
[`docs/decisions/2026-08-30-android-tls-rustls.md`](../../docs/decisions/2026-08-30-android-tls-rustls.md).

## Prerequisites (owner-installed — not by these steps)

- Android NDK; `ANDROID_NDK_HOME` set
- `cargo-ndk` on `PATH`
- Rust targets: `aarch64-linux-android`, `armv7-linux-androideabi`,
  `x86_64-linux-android`

## Build

```bash
# Kotlin bindings  ->  target/uniffi-kotlin/uniffi/pubnetchk_android/pubnetchk_android.kt
just android-bindings

# per-ABI .so  ->  android/app/src/main/jniLibs/<abi>/libpubnetchk_android.so
just android-lib
```

`android-bindings` generates from a **debug** build on purpose: the interface is
the same in any profile, and the workspace release profile's `strip = true`
removes the UniFFI metadata that library-mode `uniffi-bindgen` reads. The
shipped `.so` (`android-lib`, release) does not need that metadata at runtime —
only the `extern "C"` FFI entry points, which survive stripping.

## Host tests

`cargo test -p pubnetchk-android` runs the snapshot → probe → audit → JSON
pipeline for the offline `topology` check plus the error paths — no device or
NDK needed.
