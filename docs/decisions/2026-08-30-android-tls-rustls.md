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
  store. It is linked but **not used** on this path — see the update below.

`tokio-tungstenite` is set to **`rustls-tls-webpki-roots`** — the WebSocket path
validates against the bundled Mozilla root set and needs no JVM context. Both
paths share the `aws-lc-rs` provider (rustls's default), so there is one crypto
backend in the binary.

### Update (2026-09-02): reqwest also uses webpki-roots, not the platform verifier

Shipping ticket 5 surfaced that `rustls-platform-verifier` does not degrade
gracefully: on Android it **`abort()`s the process** ("Expect
rustls-platform-verifier to be initialized") the first time reqwest builds a TLS
config, because the cdylib is loaded through JNA (not `System.loadLibrary`), so
no JVM `Context` is ever registered. With the workspace `panic = "abort"` this
took the whole app down before the security check could run — not a catchable
error.

Fix (`crates/pubnetchk/src/tls.rs`): on the `tls-rustls` path, build the
`reqwest::Client` with `use_preconfigured_tls(ClientConfig)` where the config
uses `webpki-roots` (Mozilla's CA bundle — the same roots the WebSocket path
already uses). reqwest then never constructs the platform verifier. `tls-native`
is unchanged; when a workspace-wide `cargo` run unifies both features,
`tls-native` wins the `#[cfg]`, so only the standalone Android build takes the
webpki path.

This is exactly the "Revisit if → reqwest gains a webpki-roots feature" case,
reached by a different route: reqwest has no such feature, but
`use_preconfigured_tls` gets the same result. `rustls` + `webpki-roots` are now
optional direct deps of `pubnet-tools`, enabled only by `tls-rustls` (they track
the versions reqwest's `rustls` feature already resolves — no new desktop tree).

**Still true:** DoH validates against Mozilla roots, not the device's
user/enterprise trust store. Wiring the platform verifier for that is
[epic ticket 9](../epics/pubnet-android/tickets/009-rustls-platform-verifier-context.md)
— now an enhancement, not a crash fix.

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
  *feature*, but `ClientBuilder::use_preconfigured_tls(rustls::ClientConfig)`
  reaches the same place. This is what the 2026-09-02 update above actually does.

## Revisit if

- `aws-lc-sys` cross-compilation to the Android ABIs is a recurring build
  problem → switch to the `ring` provider via `rustls-no-provider`.
- The DoH probe needs the device's user/enterprise CAs (not just Mozilla roots)
  → wire `rustls-platform-verifier` with a JVM `Context` (epic ticket 9) and
  drop the `use_preconfigured_tls` override.
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
