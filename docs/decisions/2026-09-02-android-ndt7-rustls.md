---
template_version: 1.4.0
date: 2026-09-02
slug: android-ndt7-rustls
status: accepted
decided_by: hampton
related: [2026-08-30-android-tls-rustls, 2026-08-24-cloudflare-speedtest-not-node-compatible]
---

# Decision: the NDT7 speed test runs on Android over rustls with no extra wiring

## Context

The speed check (`checks::speed`) does two TLS things:

1. `default_locate()` — an HTTPS GET to `locate.measurementlab.net` for a nearby
   NDT7 server.
2. `measure_direction()` — a `wss://` WebSocket to that server via
   `tokio_tungstenite::connect_async`, download then upload.

Both went through `native-tls` before the Android work. Ticket 2
([2026-08-30-android-tls-rustls.md](2026-08-30-android-tls-rustls.md)) set the
`tls-rustls` feature to `tokio-tungstenite/rustls-tls-webpki-roots` and flagged
that `connect_async` *might* need an explicit `Connector` on the rustls path.
The `default_locate` GET was moved onto `crate::tls::client_builder()` (webpki
`ClientConfig`) by the SIGABRT fix, so it already works.

## Decision

**No explicit `Connector` is needed, and none is added.**

`tokio-tungstenite` 0.30's `connect_async` calls
`client_async_tls_with_config(req, stream, None, None)`. With `connector: None`
its selection is:

```rust
#[cfg(feature = "native-tls")]                                   { native_tls(..) }
#[cfg(all(feature = "__rustls-tls", not(feature = "native-tls"))) { rustls(..)     }
```

The **standalone Android cdylib build** (`cargo ndk build -p pubnetchk-android`,
`pubnet-tools` with `default-features = false, features = ["tls-rustls"]`)
enables only `__rustls-tls` — nothing pulls `tokio-tungstenite/native-tls` — so
`connect_async` takes the rustls branch and builds a `ClientConfig` from
`webpki_roots::TLS_SERVER_ROOTS` (the `rustls-tls-webpki-roots` feature). No JVM
`Context`, same root set the DoH path uses, one `aws-lc-rs` provider in the
binary.

Under a **workspace-wide `cargo test`**, feature unification turns *both*
`native-tls` and `__rustls-tls` on, so `connect_async` there picks native-tls —
which is fine (that build is the desktop engine). Only the dedicated
`--no-default-features --features tls-rustls` build exercises the rustls
WebSocket, and that is the one shipped to the phone.

`AndroidOptions.only` defaults to all four checks — speed included.
`speedDurationSecs` stays 10 (the desktop default); a mobile-data-aware UI toggle
is a follow-up, not this decision.

## Verification

```
# rustls path: locate GET + NDT7 WebSocket, end to end
cargo test -p pubnet-tools --no-default-features --features tls-rustls --test speed
# or, ad hoc:
cargo run -p pubnet-tools --no-default-features --features tls-rustls -- --json --only speed
```

Observed on this host (2026-09-02): `status: ok`, `downloadMbps ≈ 368`,
`uploadMbps ≈ 120`, `source: "ndt7"` — the M-Lab locate GET and the `wss://`
upgrade both validated against webpki roots.

`just test-speed-rustls` runs the first command.

## Revisit if

- A future `tokio-tungstenite` changes the `connector: None` selection so both
  features on prefers rustls, or removes the `not(feature = "native-tls")` guard
  → the workspace test would start hitting rustls (harmless) and this note is
  stale.
- Speed on mobile data needs to be off by default or shorter → change
  `AndroidOptions::default` / add the UI toggle; the transport choice here is
  unaffected.
