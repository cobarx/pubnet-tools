//! HTTP client construction, split by TLS backend.
//!
//! Desktop (`tls-native`) uses the platform trust store through `reqwest`'s
//! defaults. Android (`tls-rustls`) cannot: reqwest 0.13's `rustls` feature
//! hard-links `rustls-platform-verifier`, which `abort()`s the process the
//! first time a TLS config is built if no JVM `Context` was ever registered —
//! and the app loads this cdylib through JNA, not `System.loadLibrary`, so
//! there is none.
//!
//! On the rustls path we therefore preconfigure the client with a
//! `webpki-roots` `ClientConfig` (Mozilla's CA bundle — the same roots the
//! NDT7 WebSocket path uses) and hand it to reqwest via
//! `use_preconfigured_tls`, so the platform verifier is linked but never
//! invoked. See `docs/decisions/2026-08-30-android-tls-rustls.md`.

/// A `reqwest::ClientBuilder` with this build's TLS backend already wired.
/// Callers still add their own options (timeouts, redirect policy) and
/// `.build()`.
///
/// `tls-native` wins when a workspace-wide `cargo` invocation unifies both
/// features (e.g. `cargo test` with `pubnetchk-android` in the graph) — only
/// the standalone Android cdylib build resolves `tls-rustls` alone, and that is
/// the build that must avoid `rustls-platform-verifier`.
#[cfg(feature = "tls-native")]
pub(crate) fn client_builder() -> reqwest::ClientBuilder {
    reqwest::Client::builder()
}

#[cfg(all(feature = "tls-rustls", not(feature = "tls-native")))]
pub(crate) fn client_builder() -> reqwest::ClientBuilder {
    reqwest::Client::builder().use_preconfigured_tls(rustls_config())
}

#[cfg(all(feature = "tls-rustls", not(feature = "tls-native")))]
fn rustls_config() -> rustls::ClientConfig {
    let roots = rustls::RootCertStore::from_iter(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
    rustls::ClientConfig::builder()
        .with_root_certificates(roots)
        .with_no_client_auth()
}
