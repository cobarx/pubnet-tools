---
template_version: 1.0.0
epic: pubnet-android
ticket: 004
slug: gradle-project
type: feature
points: 5
status: todo
tracker_ref: tbd
pr: none
related: []
---

# Ticket 004: Android Studio project + Gradle/Rust wiring

## Goal

An `android/` Gradle project that cross-compiles the Rust cdylib, generates the
UniFFI Kotlin bindings, and produces an installable debug APK — with no app
logic yet beyond a blank screen that calls `reportSchemaVersion()`.

## Scope

- **In:** `android/` — Gradle (Kotlin DSL), `settings.gradle.kts` + an `app/`
  module. `minSdk 26`, `targetSdk 35`, Kotlin, Jetpack Compose + Material 3,
  `kotlinx.serialization`.
- **In:** `org.mozilla.rust-android-gradle` plugin pointed at
  `../crates/pubnetchk-android`, ABI matrix `arm64-v8a`, `x86_64` (emulator),
  `armeabi-v7a`. The plugin runs `cargo-ndk` and packs the `.so` into `jniLibs`.
- **In:** a Gradle task that runs the `uniffi-bindgen` Kotlin generation (see
  ticket 3) into `app/build/generated/uniffi`, wired as a `preBuild` dependency;
  document the manual command in `android/README.md`.
- **In:** `AndroidManifest.xml` permissions — `INTERNET`,
  `ACCESS_NETWORK_STATE`, `ACCESS_WIFI_STATE`, `ACCESS_FINE_LOCATION`.
- **In:** `android/README.md` — NDK version, `cargo-ndk` install, Rust targets
  to add, how to open in Android Studio, how bindings are generated.
- **In:** `.gitignore` entries for `android/` build output, `.so` under
  `jniLibs`, `local.properties`.
- **In:** `CLAUDE.md` — Android in the platform list, the architecture tree
  (`crates/pubnetchk-android/`, `android/`), the `PlatformProbe` section
  (`SnapshotProbe`), a Development-setup subsection for the Android build, and
  the docs index.
- **Out:** the `NetworkFacts` collector and the real UI (ticket 5).

## Acceptance criteria

- `cd android && ./gradlew :app:assembleDebug` produces
  `app/build/outputs/apk/debug/app-debug.apk` on a clean checkout (given NDK +
  `cargo-ndk` installed).
- The APK contains `lib/arm64-v8a/libpubnetchk_android.so` and the UniFFI
  runtime `.kt`.
- `adb install -r` then launching shows a screen rendering the string from
  `reportSchemaVersion()` (proves the JNI load + UniFFI binding path work).
- `just build` / `just test` / `just clippy` at the repo root are unaffected.

## Notes

The `rust-android-gradle` plugin needs `ANDROID_NDK_HOME` (or `ndk.dir`). Pin
the NDK version in `android/app/build.gradle.kts` so CI (ticket 8) and local
builds agree. Do not commit `local.properties` or `jniLibs/*.so`.
