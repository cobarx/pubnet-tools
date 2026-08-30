---
template_version: 1.0.0
epic: pubnet-android
ticket: 002
slug: tls-feature-gating
type: chore
points: 2
status: todo
tracker_ref: tbd
pr: none
related: []
---

# Ticket 002: TLS backend feature-gating (rustls for Android)

## Goal

Let the Android crate build with rustls while every desktop build keeps
`native-tls` exactly as it is today.

## Scope

- **In:** `crates/pubnetchk/Cargo.toml`:
  - Two features: `tls-native` (default) and `tls-rustls`.
  - `reqwest` and `tokio-tungstenite` become `default-features = false` with the
    TLS feature threaded through:
    - `tls-native` → `reqwest/native-tls`, `tokio-tungstenite/native-tls`
    - `tls-rustls` → `reqwest/rustls-tls`,
      `tokio-tungstenite/rustls-tls-native-roots`
  - Keep `reqwest`'s `json` feature and the deliberate absence of `http2` /
    `default-tls` in both paths.
- **In:** `crates/pubnetchk/src/checks/speed.rs` — if the rustls path needs an
  explicit `Connector` for `tokio_tungstenite::connect_async` (native-tls
  auto-selects today), add a small `#[cfg(feature = "tls-rustls")]` helper;
  otherwise no code change.
- **In:** `docs/decisions/2026-08-28-android-tls-rustls.md`.
- **Out:** the Android crate itself (ticket 3 sets `default-features = false,
  features = ["tls-rustls"]` on its `pubnet-tools` dependency).

## Acceptance criteria

- `cargo tree -p pubnet-tools -e features | grep -iE 'native-tls|openssl'` on
  the default feature set still shows `native-tls` + `openssl-sys` (unchanged).
- `cargo tree -p pubnet-tools --no-default-features --features tls-rustls -e
  features | grep -iE 'rustls|openssl'` shows rustls and **no** `openssl-sys`.
- `just build`, `just test`, `just clippy` all pass on the default features.
- `cargo check -p pubnet-tools --no-default-features --features tls-rustls`
  passes on the host.

## Notes

The CLAUDE.md rule "verify with `cargo tree` after touching an HTTP/TLS
dependency" applies directly here — the decision doc must show both trees. The
rule's intent (no *system* OpenSSL on desktop) is preserved: desktop is
untouched; Android is a separate target with no system TLS to depend on.
