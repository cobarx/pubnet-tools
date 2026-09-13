---
template_version: 1.0.0
epic: pubnet-android
ticket: 008
slug: ci-android-build
type: chore
points: 2
status: in-review
tracker_ref: tbd
pr: tbd
related: [release-automation-github-actions]
---

# Ticket 008: CI — build `pubnetchk-android` + `assembleDebug`

## Goal

Build the Android debug APK in CI so a broken cdylib/Gradle wiring is caught
before it reaches a device, and produce an installable artifact from every
tagged release without a local Android toolchain.

## Scope

- **In:** `build-android` job in `.github/workflows/release.yml` — JDK 21
  (Temurin), the Android SDK, the pinned NDK
  (`27.2.12479018`, matching `android/app/build.gradle.kts`), the three
  `*-linux-android` Rust targets, `cargo-ndk`, then
  `./gradlew :app:assembleDebug`. The APK is renamed to include the tag and
  uploaded as a workflow artifact; the `publish` job attaches it to the
  GitHub Release alongside the desktop archives.
- **Out:** a signed `assembleRelease` build (Play Store signing is out of
  scope for the whole epic); running the Android instrumentation/emulator
  tests in CI (no KVM on the hosted runners — same limitation the dev
  container has, per `docs/context/devcontainer-setup.md`); `just
  android-test` (`testDebugUnitTest`) is not wired into this workflow since it
  runs on every local build already and isn't release-gating.

## Acceptance criteria

- The `build-android` job in `.github/workflows/release.yml` produces
  `app/build/outputs/apk/debug/app-debug.apk` on a clean runner (no
  owner-installed prerequisites — everything the epic's Prerequisites section
  lists as owner-installed is installed by the job itself).
- A `workflow_dispatch` run uploads `pubnetchk-android-<ref>-debug.apk` as a
  workflow artifact without requiring a tag push.
- A tag push (`v*.*.*`) attaches that APK to the created GitHub Release.

## Notes

See
[docs/decisions/2026-09-13-release-automation-github-actions.md](../../../decisions/2026-09-13-release-automation-github-actions.md)
for why this rides in the same workflow as the desktop release matrix rather
than a separate CI file, and why it builds `assembleDebug` rather than a
release build.
