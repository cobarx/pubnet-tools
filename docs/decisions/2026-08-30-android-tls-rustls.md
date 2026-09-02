---
template_version: 1.4.0
date: 2026-08-30
slug: android-tls-rustls
status: accepted
decided_by: hampton
related: [2026-08-25-rust-rewrite-technology-stack, 2026-08-02-dns-leak-detection]
---

# Decision: the Android build uses rustls; desktop keeps native-tls

## Context

`pubnetchk` makes TLS connections for the security check (DoH GETs to Cloudflare
and Google, captive-portal canaries) and the speed check (the M-Lab locate GET
and the NDT7 WebSocket). Today that goes through `reqwest` and
`tokio-tungstenite` with the **`native-tls`** feature, which resolves to the
platform's own TLS stack: SChannel on Windows, Secure Transport on macOS, and
system OpenSSL on Linux. `CLAUDE.md` names this deliberate — "no system-OpenSSL
dependency… verify with `cargo tree` after touching an HTTP/TLS dependency."

The [pubnet-android epic](../epics/pubnet-android/epic.md) adds an Android
target: a `cdylib` (`crates/pubnetchk-android`) cross-compiled with the NDK.
`native-tls` does not work there:

- Android exposes **no system TLS library** to an app. There is no
  `libssl`/`libcrypto` to link against, and Secure Transport / SChannel are
  Apple/Microsoft only.
- The only `native-tls` route on Android is `native-tls-vendored` — building
  OpenSSL from source under the NDK. That is a C build with its own
  cross-compile friction, and it re-introduces exactly the OpenSSL dependency
  the desktop build spent effort shedding.

## Decision

The TLS backend is a **Cargo feature of the `pubnet-tools` (pubnetchk) crate**,
defaulting to the current behavior:

```toml
[features]
default    = ["tls-native"]
tls-native = ["reqwest/native-tls",  "tokio-tungstenite/native-tls"]
tls-rustls = ["reqwest/rustls",      "tokio-tungstenite/rustls-tls-webpki-roots"]
```

`reqwest` and `tokio-tungstenite` become `default-features = false` (reqwest
keeps only `json`; tokio-tungstenite keeps its defaults `connect` + `handshake`).

- **Desktop (Linux/macOS/Windows)** builds unchanged: `default` → `tls-native`.
  `cargo tree` on the default feature set still shows `native-tls` + `openssl-sys`
  and **no** rustls.
- **The Android cdylib** depends on `pubnet-tools` with `default-features =
  false, features = ["tls-rustls"]`. `cargo tree` there shows rustls and **no
  `openssl-sys`**.

The CLAUDE.md rule's intent — no *system* OpenSSL on the desktop builds — is
preserved. Android is a separate target with no system TLS to depend on, so it
is the one place that opts into a bundled stack.

### What `tls-rustls` actually pulls (reqwest 0.13)

reqwest 0.13's `rustls` feature is not just "rustls." It brings:

- **`aws-lc-rs`** as the crypto provider (`aws-lc-sys` is a C build — the NDK
  toolchain plus `cmake` must be on `PATH` when cross-compiling; both are
  owner-installed prerequisites, see the epic).
- **`rustls-platform-verifier`** for certificate validation against the OS trust
  store. On Android this verifier needs a JVM + `Context` handle; wiring that
  from the cdylib (via `ndk-context` / an `init` call from Kotlin) is
  **ticket 3's** responsibility, not this decision's.

`tokio-tungstenite` is set to **`rustls-tls-webpki-roots`** instead — the
WebSocket path then validates against the bundled Mozilla root set and needs no
JVM context, which keeps the NDT7 speed test working before the platform
verifier is wired. Both paths share the `aws-lc-rs` provider (rustls's default),
so there is one crypto backend in the binary.

## Alternatives considered

- **`native-tls-vendored` on Android** — keeps one code path, but is a
  from-source OpenSSL C build under the NDK and undoes the desktop project's
  OpenSSL-shedding work. Rejected; rustls is the modern Android-Rust default.
- **`reqwest/rustls-no-provider` + hand-installed `ring`** — lighter than
  `aws-lc-rs`, no `cmake`. But reqwest 0.13 still forces
  `rustls-platform-verifier` in with it, so the hard part (the Android context)
  is unchanged, and we would own crypto-provider installation in the cdylib.
  Deferred as a possible follow-up if `aws-lc-sys` cross-compilation proves
  painful.
- **webpki-roots for reqwest too** — reqwest 0.13 exposes no webpki-roots
  feature (unlike tokio-tungstenite); `rustls` always means the platform
  verifier. Nothing to choose here.

## Revisit if

- `aws-lc-sys` cross-compilation to the Android ABIs is a recurring build
  problem → switch to the `ring` provider via `rustls-no-provider`.
- reqwest gains a webpki-roots feature → drop `rustls-platform-verifier` and the
  Android-context wiring entirely, matching the WebSocket path.
- A desktop target ever loses a working system TLS stack → reconsider making
  rustls the default everywhere.

## Verification

```
# desktop unchanged
cargo tree -p pubnet-tools -e normal | grep -iE 'openssl-sys|native-tls'   # present
cargo tree -p pubnet-tools -e normal | grep -i  rustls                     # nothing

# android feature set
cargo tree -p pubnet-tools --no-default-features --features tls-rustls -e normal \
  | grep -i openssl-sys                                                    # nothing
cargo build -p pubnet-tools                                                # native-tls, OK
cargo check -p pubnet-tools --no-default-features --features tls-rustls     # rustls, OK
```
