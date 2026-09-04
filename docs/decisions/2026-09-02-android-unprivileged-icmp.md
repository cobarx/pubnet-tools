---
template_version: 1.4.0
date: 2026-09-02
slug: android-unprivileged-icmp
status: accepted
decided_by: hampton
related: [2026-08-30-android-app-architecture, 2026-08-28-windows-probes-via-win32-api]
---

# Decision: Android reliability check pings over an unprivileged datagram ICMP socket

## Context

The reliability check pings three targets (the gateway, `8.8.8.8`, `1.1.1.1`)
ten times each and reports per-packet RTT, jitter, and packet loss
(`checks::reliability`, spec `reliability-check-resilience`). The production ping
is `#[cfg]`-split:

- **Linux / macOS** — shell out to `ping -c 10 -i 0.2`.
- **Windows** — `IcmpSendEcho2` via `windows-sys` (no `ping.exe`).

Android needs its own path. It **cannot shell out** (the whole reason the app
feeds the engine a `HostSnapshot` — see
[2026-08-30-android-app-architecture.md](2026-08-30-android-app-architecture.md)),
and even if it could, `toybox ping` / `/system/bin/ping` output is not a stable
contract and SELinux policy for `untrusted_app` executing it varies by OEM.

## Options

| | RTT | packet loss | works unprivileged on Android |
|---|---|---|---|
| **Datagram ICMP socket** (`SOCK_DGRAM`, `IPPROTO_ICMP`) | yes | yes | **yes** |
| `SOCK_RAW` ICMP | yes | yes | no — needs `CAP_NET_RAW` / root |
| `/system/bin/ping` exec | yes (parsed) | yes (parsed) | no — cannot exec; output unstable |
| TCP-connect latency to `:443` | handshake only | no (no loss metric) | yes |

## Decision

**Datagram ICMP socket.** Linux (since 3.0) and Android expose
`socket(AF_INET, SOCK_DGRAM, IPPROTO_ICMP)` to any process whose gid falls in
`/proc/sys/net/ipv4/ping_group_range`. Android ships that range as
`0 2147483647` — every app — on every release since the feature existed; it is
how Chrome, Flutter's `dart:io`, and `ping` itself (which is setgid-less on
Android) send echoes. No `CAP_NET_RAW`, no root, no manifest permission beyond
`INTERNET`.

`crates/pubnet-platform/src/net_icmp.rs` — `#[cfg(any(target_os = "linux",
target_os = "android"))]`, `socket2` for the socket, hand-rolled ICMP echo
(matching the project's hand-rolled NDT7 client). IPv4 only (all three targets
are v4). `checks::reliability::system_ping` gets a fourth `#[cfg(target_os =
"android")]` arm calling it.

The kernel rewrites the ICMP `id` to the socket and matches replies to it, so
request/reply pairing is by `seq`; it also fills the checksum for `SOCK_DGRAM`,
but `net_icmp` computes it anyway so the code does not depend on that quirk.
Per-packet timeout 1 s, 200 ms inter-packet gap (the non-root floor on the
shell-out path, kept for parity), sequential on a blocking task.

### Fallbacks and failure

- Socket creation failing (a device with a locked-down `ping_group_range`, or a
  future policy change) → `PingSummary { transmitted: count, received: 0 }`,
  same shape as a `ping`-binary-not-found on the shell path. The check turns
  that into `reachable: false` / an "unreachable" finding — never a panic.
- A gateway that does not answer ICMP reports `reachable: false` for that
  target. This is existing cross-platform behaviour, not Android-specific.

### Not chosen: TCP-connect

`connect()` latency to `1.1.1.1:443` always works and needs no ICMP, but it
measures the TCP handshake through every middlebox, has no packet-loss or jitter
signal, and can't probe a gateway that isn't listening on a known port. The
`ReliabilityData` shape (`transmitted`, `received`, `packetLossPct`, `rtts[]`)
wants real echoes. Kept in reserve if a device is found where datagram ICMP is
blocked.

## Consequences

- `socket2` becomes a `cfg(linux|android)` dependency of `pubnet-platform`
  (~30 KB source, already in the tree transitively via `tokio`).
- Linux could switch off the `ping` shell-out to `net_icmp` too — deliberately
  **not** done here (it would change desktop behaviour and the empirical `ping`
  fixtures). Tracked as a possible follow-up.
- `net_icmp`'s live test (`pings_a_public_address_when_icmp_is_permitted`)
  self-skips where the socket can't be opened, so CI without ICMP still passes.

## Revisit if

- A shipped device blocks datagram ICMP → add the TCP-connect fallback behind a
  runtime probe.
- Reliability needs IPv6 targets → the socket type is `IPPROTO_ICMPV6` with the
  same unprivileged story; `net_icmp` would grow a v6 path.
