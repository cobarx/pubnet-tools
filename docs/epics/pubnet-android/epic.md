---
template_version: 1.0.0
slug: pubnet-android
status: planned
owner: hampton
created: 2026-08-28
tracker_ref: tbd
related: [android-host-snapshot]
---

# Epic: pubnet-android — Android front-end for pubnetchk

## Goal

An Android app that runs the `pubnetchk` audit on the Wi-Fi/network the phone is
currently joined to, reusing the existing Rust engine (checks, scoring, network
parsers, the `PlatformProbe` seam) with a Kotlin/Jetpack Compose UI. The Rust
core is bridged to Kotlin with **UniFFI**; the whole thing lives in this
workspace as a fourth crate plus an `android/` Gradle project.

The engine is already OS-abstracted behind `PlatformProbe`
(`crates/pubnet-platform/src/platform/mod.rs`). Android differs from the three
existing platforms in one structural way: an Android app **cannot shell out** to
`ip`/`nmcli`/`resolvectl`/`ping`, and the facts those commands provide are only
reachable through Android framework APIs on the Kotlin side
(`ConnectivityManager`, `WifiManager`, `LinkProperties`). So the Android "probe"
is **fed a snapshot of pre-gathered facts** rather than running commands.

## Decisions (locked)

- **Bridge shape:** JSON snapshot in, JSON report out. Kotlin gathers a
  `HostSnapshot`, passes it as a JSON string to `run_audit_json()`; Rust returns
  the existing `Report` JSON. No `uniffi` derives on any type, no
  async-over-FFI, no callback interface.
- **Android TLS:** rustls, feature-gated. The desktop build keeps `native-tls`
  unchanged — the CLAUDE.md "no system-OpenSSL dependency" rule stays intact for
  Linux/macOS/Windows; Android has no app-accessible system TLS, so it is the
  one target that opts into rustls.
- **Rust↔Gradle build:** the `org.mozilla.rust-android-gradle` plugin drives
  `cargo-ndk` and packages the per-ABI `.so` into `jniLibs`.
- **First milestone:** a walking skeleton — tickets 1–5 below. Only **topology
  and security** checks run. Reliability needs an unprivileged-ICMP path and
  speed needs the NDT7 client validated over rustls; both are deferred.

## Prerequisites (installed by the repo owner, not by tooling)

The build needs host-level tools that are **not** auto-installed during this
epic — the owner installs them and the tickets only assume they are present:

- Android SDK + NDK (version pinned in `android/app/build.gradle.kts`)
- `cargo-ndk`
- Rust targets: `aarch64-linux-android`, `x86_64-linux-android`,
  `armv7-linux-androideabi`
- `uniffi-bindgen` (via the `[[bin]]` shim in `crates/pubnetchk-android`, so
  `cargo run` builds it — no separate install)

Ticket work must not run `rustup target add`, `cargo install`, `pacman`,
`scoop`, `sdkmanager`, or any other system-level install. If a step needs a
missing tool, stop and hand it back to the owner.

## Scope

- **In:** `SnapshotProbe`/`HostSnapshot` in `pubnet-platform`; TLS
  feature-gating in the `pubnetchk` crate; a `crates/pubnetchk-android` UniFFI
  cdylib exposing `run_audit_json`; extraction of a spinner-free
  `run_audit_with_probe` in `cli.rs`; an `android/` Gradle project with a
  Compose "Scan" screen; a `NetworkFacts` collector that builds the snapshot
  from framework APIs; topology + security working end-to-end on a device.
- **Out (this epic):** reliability and speed on Android (separate follow-up
  tickets, each with its own decision doc); a polished multi-section UI matching
  the console renderer; Play Store release/signing; `pubnetdiag` on Android
  (`pubnetdiag` is Windows-only); tablet/landscape layout; any change to desktop
  behavior or the report JSON schema.

## Tickets

| # | Title | Type | Points | Status | Tracker | PR |
|---|---|---|---|---|---|---|
| 1 | `SnapshotProbe` + `HostSnapshot` in pubnet-platform | feature | 3 | todo | tbd | none |
| 2 | TLS backend feature-gating (rustls for Android) | chore | 2 | todo | tbd | none |
| 3 | `pubnetchk-android` UniFFI cdylib + `run_audit_json` | feature | 5 | todo | tbd | none |
| 4 | Android Studio project + Gradle/Rust wiring | feature | 5 | in-review | tbd | tbd |
| 5 | `NetworkFacts` collector + Compose skeleton screen | feature | 5 | in-review | tbd | tbd |
| 6 | Reliability on Android — unprivileged ICMP | feature | 5 | in-review | tbd | tbd |
| 7 | Speed / NDT7 on Android — validate over rustls | feature | 3 | in-review | tbd | tbd |
| 8 | CI: build `pubnetchk-android` + `assembleDebug` | chore | 2 | deferred | tbd | none |
| 9 | DoH validation against the device trust store (platform verifier + JVM `Context`) | feature | 3 | deferred | tbd | none |
| 10 | Cellular / mobile-network facts in the snapshot + UI | feature | 3 | deferred | tbd | none |

Walking-skeleton points (1–5): `20`

## Sequencing

- Ticket 1 (`SnapshotProbe`) lands first — the Android crate has nothing to call
  without it, and it is independently testable with unit tests over snapshot
  JSON fixtures.
- Ticket 2 (TLS) is independent of 1 and can land in parallel; ticket 3 needs
  it (the cdylib won't cross-compile with `native-tls`).
- Ticket 3 needs 1 and 2. It also does the `run_audit_with_probe` extraction in
  `cli.rs` (pure refactor, no CLI behavior change).
- Ticket 4 needs 3 (there must be a crate to build a `.so` from).
- Ticket 5 needs 4. This is the ticket that produces a running app.
- Tickets 6–10 follow the skeleton in any order; 6, 7 and 9 each open with a
  decision doc before implementation. Tickets 6 (reliability — unprivileged
  datagram ICMP) and 7 (speed — NDT7 auto-selects rustls/webpki when native-tls
  is absent) are done; all four checks now run on Android. Ticket 9 is an
  enhancement (DoH against the
  device trust store instead of the bundled Mozilla roots), not a blocker — the
  skeleton's DoH probe already works via `webpki-roots` (`crate::tls`). Ticket 10
  extends the snapshot to cellular so the app is useful (and not misleading) off
  Wi-Fi; it touches the shared `InterfaceKind` type and the snapshot spec.

## Decision docs

- `docs/decisions/2026-08-30-android-app-architecture.md` — workspace layout,
  UniFFI + JSON-snapshot bridge (why not typed records or a callback probe),
  rust-android-gradle, Compose. Cross-references the Windows platform decision
  as the sibling "new platform" precedent.
- `docs/decisions/2026-08-30-android-tls-rustls.md` — no app-accessible system
  TLS on Android; feature-gated rustls for the Android crate only; desktop
  `cargo tree` stays `native-tls`.
- `docs/decisions/2026-09-02-android-unprivileged-icmp.md` (ticket 6) — datagram
  ICMP socket (`SOCK_DGRAM`/`IPPROTO_ICMP`, unprivileged on Android) vs
  `SOCK_RAW` vs `/system/bin/ping` vs TCP-connect latency.
- `docs/decisions/2026-09-02-android-ndt7-rustls.md` (ticket 7) — `connect_async`
  auto-selects rustls + webpki-roots for the NDT7 WebSocket when `native-tls` is
  absent; no explicit `Connector` needed.
- (ticket 9) `docs/decisions/<date>-android-rustls-platform-verifier.md` — how
  the rustls platform verifier gets its JVM `Context` (JNI init vs `ndk_context`
  vs UniFFI-exported init), to validate DoH against the device trust store. The
  skeleton uses `webpki-roots` via `use_preconfigured_tls` — the platform
  verifier `abort()`s the process uninitialized, recorded in
  `2026-08-30-android-tls-rustls.md`.

## Specs

- `docs/specs/android-host-snapshot.md` — the snapshot field contract and its
  degradation rules (missing SSID → `ssidHidden`; no system egress IP → DNS
  verdict `uncertain`; unreadable `/proc/net/arp` → empty neighbors, topology
  still `ok`).
- The existing `docs/specs/` carry over unchanged — they are platform-agnostic
  behavior contracts.
