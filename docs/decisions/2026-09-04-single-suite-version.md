---
template_version: 1.4.0
date: 2026-09-04
slug: single-suite-version
status: accepted
decided_by: hampton
related: [2026-08-26-rename-to-pubnet-tools]
---

# Decision: one version number for the whole pubnet-tools suite

## Context

`pubnet-tools` is a Cargo workspace of four crates — `pubnet-platform`,
`pubnet-tools` (the `pubnetchk` binary), `pubnetdiag`, `pubnetchk-android` — plus
an `android/` Gradle project. Every crate carried its own `version = "0.1.0"`,
and the Android app was about to get a separate `android/version.properties`.
That is three-plus places to bump and an immediate source of drift ("app 0.2.0
ships engine 0.1.0").

The suite ships as one thing (the blog-post project, one binary today, the
Android app over the same engine). Users see one product; the report already
surfaces a single `version` (`report_schema_version()` = `CARGO_PKG_VERSION`).

## Decision

**One version, in `[workspace.package]` in the repo-root `Cargo.toml`.**

- Every member crate: `version.workspace = true`.
- `report_schema_version()` / `audit::VERSION` (`env!("CARGO_PKG_VERSION")`)
  automatically report it — no code change.
- The Android app reads it in `android/app/build.gradle.kts`:
  - `versionName` = the semver verbatim (debug builds append
    `-debug+<8-char git sha>`).
  - `versionCode` = `MAJOR*10000 + MINOR*100 + PATCH` — monotonic while minor and
    patch stay < 100 (0.2.0 → 200, 1.0.0 → 10000, 2.15.30 → 21530), well under
    Play's 2,100,000,000 ceiling. No separate counter to keep in sync.
  - Shown in the app header and the report footer via `BuildConfig.VERSION_NAME`.

**To release the suite:** bump the one line in the root `Cargo.toml`, commit,
tag `vX.Y.Z`.

Starting version: **0.2.0** — the Android front-end (all four checks, the SSID /
permission fixes, the rename) is a meaningful step past the 0.1.0 CLI-only state.

## Alternatives considered

- **Per-crate versions.** Right when crates are published independently to
  crates.io with their own release cadence. Nothing here is published; they move
  together. Rejected as premature.
- **`android/version.properties` + manually matched Cargo versions.** The
  drift the whole decision exists to avoid.
- **`versionCode` from git commit count / a date.** Breaks on shallow clones and
  is opaque; the semver-derived formula is legible and deterministic.

## Revisit if

- A crate is published to crates.io on its own cadence → give that one crate an
  explicit `version` again.
- Minor or patch needs to exceed 99 → widen the `versionCode` formula (e.g.
  `MAJOR*1_000_000 + MINOR*1_000 + PATCH`) in one place.
