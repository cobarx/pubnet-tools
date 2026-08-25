//! Port of src/checks/speed.ts: hand-rolled NDT7 (M-Lab) client over
//! tokio-tungstenite. See docs/decisions/2026-08-24-cloudflare-speedtest-not-node-compatible.md
//! for why this is a direct protocol implementation rather than a wrapped
//! library — not yet ported.
