---
template_version: 1.4.0
date: 2026-09-13
slug: release-automation-github-actions
status: accepted
decided_by: hampton
related: [2026-08-27-windows-platform-support, 2026-08-28-windows-probes-via-win32-api, 2026-08-30-android-app-architecture]
---

# Decision: hand-rolled GitHub Actions release matrix, not cargo-dist

## Context

`pubnet-tools` had no CI or release automation at all — binaries were only ever
built locally via `just release`. The repo is public
(`github.com/cobarx/pubnet-tools`), which means GitHub Actions is free with no
monthly minute cap on Linux, macOS, and Windows hosted runners (the per-minute
multipliers on macOS/Windows only apply to the private-repo minute quota).

Two options for producing tagged releases:

1. **[`cargo-dist`](https://opensource.axo.dev/cargo-dist/)** — a tool built
   specifically for cross-platform Rust binary releases: point it at a tag, it
   generates its own GitHub Actions workflow, builds a target matrix, packages
   archives + checksums, and creates the Release.
2. **A hand-rolled workflow** — a build matrix job per target, then
   `softprops/action-gh-release` to attach artifacts to a tag-triggered
   release.

## Decision

Hand-rolled workflow (`.github/workflows/release.yml`), not `cargo-dist`.

## Rationale

The project's Windows target is not the default `x86_64-pc-windows-msvc` —
CLAUDE.md's "Windows toolchain" section requires the **GNU** toolchain
(`stable-x86_64-pc-windows-gnu` + mingw) because `windows-sys` links cleanly
that way without the Visual Studio C++ build tools, and because `just release`
already assumes it locally. `cargo-dist`'s generated Windows job builds the
MSVC target by default; making it emit a GNU build instead means fighting its
generated-workflow conventions rather than writing the four `rustup`/`choco`
lines directly. Given there are only four targets (linux-gnu,
macos-aarch64, macos-x86_64, windows-gnu) and one non-cargo build (the Android
APK via Gradle, which `cargo-dist` has no concept of at all), a plain build
matrix is less code to maintain than an escape hatch through someone else's
generator.

**Desktop matrix:** `ubuntu-latest` (x86_64-unknown-linux-gnu, `libssl-dev` for
`native-tls` per CLAUDE.md's "no vendored OpenSSL" rule), `macos-latest` built
twice (aarch64 native + x86_64 cross — GitHub's `macos-latest` runners are
Apple Silicon since macOS 14, but the Xcode toolchain links `x86_64-apple-darwin`
fine from there), and `windows-latest` with the GNU toolchain installed
explicitly (mirrors the README's local setup instructions).

**Android:** builds `assembleDebug`, not a release build — Play Store
signing is explicitly out of scope for the whole
[pubnet-android epic](../epics/pubnet-android/epic.md), and the app's own
versioning already anticipates a side-loaded debug APK (`versionNameSuffix`
includes the git short SHA specifically so a debug build is identifiable —
see `android/app/build.gradle.kts`). This closes epic ticket 8 (`CI: build
pubnetchk-android + assembleDebug`), previously deferred.

**Trigger:** `push: tags: v*.*.*` plus `workflow_dispatch` so the whole matrix
(build + package, artifacts uploaded) can be dry-run from the Actions tab
without cutting a tag; the final `publish` job that creates the GitHub Release
only runs when the trigger was an actual tag push.

## Revisit if

- A fifth target is needed (e.g. `aarch64-unknown-linux-gnu`) — the matrix
  scales linearly and stays simple; if it grows past ~6 entries, reconsider
  `cargo-dist` since its per-target boilerplate stops being the cheaper side
  of the tradeoff.
- Android moves to a signed release build (Play Store or a public release
  APK) — needs a signing-key decision doc of its own (keystore storage as a
  GitHub secret, at minimum) before `assembleRelease` replaces
  `assembleDebug` here.
- `cargo install cargo-ndk` in CI (rebuilt from source every run, no prebuilt
  binary available via the usual install actions) becomes a meaningful chunk
  of the Android job's runtime — worth caching the compiled binary via
  `actions/cache` keyed on the pinned version, or switching to a
  prebuilt-binary install action if one adds coverage for it later.
