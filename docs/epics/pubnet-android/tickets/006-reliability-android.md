---
template_version: 1.0.0
epic: pubnet-android
ticket: 006
slug: reliability-android
type: feature
points: 5
status: in-review
tracker_ref: tbd
pr: tbd
related: [android-unprivileged-icmp]
---

# Ticket 006: Reliability (ping) on Android — unprivileged ICMP

## Goal

Run the reliability check on Android: ping the gateway, `8.8.8.8`, and `1.1.1.1`
ten times each and report per-packet RTT, jitter, and packet loss — without root
and without shelling out to `/system/bin/ping`.

## Decision doc

`docs/decisions/2026-09-02-android-unprivileged-icmp.md` — datagram ICMP socket
(`SOCK_DGRAM` / `IPPROTO_ICMP`) vs `SOCK_RAW` vs `/system/bin/ping` vs
TCP-connect. Chosen: datagram ICMP (unprivileged on Android via
`ping_group_range 0 2147483647`).

## Scope

- **In:** `crates/pubnet-platform/src/net_icmp.rs` — `icmp_ping(host, count) ->
  PingSummary` over a datagram ICMP socket (`socket2`, hand-rolled ICMP echo,
  IPv4, seq-matched, RFC 1071 checksum). `#[cfg(any(target_os = "linux",
  target_os = "android"))]`. `socket2` added as a `cfg(linux|android)` dep of
  `pubnet-platform`.
- **In:** `checks::reliability::system_ping` — a `#[cfg(target_os = "android")]`
  arm calling `net_icmp::icmp_ping`. Linux/macOS shell-out and the Windows
  `IcmpSendEcho2` path are unchanged.
- **In:** `crates/pubnetchk-android` — `"reliability"` added to
  `AndroidOptions.only`'s default.
- **In:** `MainScreen.kt` — the Performance section shows gateway/internet
  reachability and per-target RTT + loss (speed stays a "not yet" line until
  ticket 7).
- **In:** tests — `net_icmp` unit tests (checksum vector, non-IPv4 → all-loss)
  plus a live test to `1.1.1.1` that self-skips where the socket can't be
  opened.
- **Out:** switching the Linux desktop ping off the `ping` binary (would change
  desktop behaviour + the empirical fixtures); IPv6 targets; a TCP-connect
  fallback (kept in reserve, see the decision doc).

## Acceptance criteria

- On a device on Wi-Fi or cellular: Scan → the Performance section shows RTT to
  `1.1.1.1` / `8.8.8.8` and gateway reachability within a few seconds; the
  report JSON's `reliability` section has `status: "ok"` and `targets[].rtts`
  populated.
- With ICMP blocked on the network: targets show `reachable: false`, the audit
  still completes, no crash.
- Desktop unaffected: `cargo test`, `cargo clippy`, the Linux/macOS/Windows ping
  paths all unchanged; `cargo tree` for `pubnet-tools` unchanged.
- `net_icmp` tests pass (`cargo test -p pubnet-platform`).

## Notes

`net_icmp` is intentionally in `pubnet-platform`, not the Android crate — it is a
plain Linux/Android capability, and keeping it there lets the Linux desktop
build adopt it later without a cross-crate move.
