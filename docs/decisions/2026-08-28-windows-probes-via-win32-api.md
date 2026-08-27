---
template_version: 1.4.0
date: 2026-08-28
slug: windows-probes-via-win32-api
status: accepted
decided_by: hampton
related: [2026-08-27-windows-platform-support, 2026-08-25-rust-rewrite-technology-stack]
---

# Decision: Windows probes call the Win32 API directly, not PowerShell

## Context

[2026-08-27-windows-platform-support.md](2026-08-27-windows-platform-support.md)
shipped a Windows `PlatformProbe` that shells out to
`powershell -NoProfile -Command "Get-Net* | Format-List"` for every probe and to
`ping.exe` for reliability. That got Windows working, but three problems were named in
that same doc's Revisit-if section and in review:

1. **`ping.exe` output localizes.** The `Get-Net*` cmdlet *property names* are
   language-invariant, but `ping.exe`'s `Reply from … time=3ms` / `Packets: Sent = 4,
   Received = 4` lines are fully localized (`Antwort von … Zeit=3ms`,
   `Pakete: Gesendet = 4, Empfangen = 4` on German Windows). `parse_windows_ping`
   silently returns 0/0 there. And `netsh wlan show interfaces` localizes every label.
2. **Windows `ping` is slow.** No sub-second interval exists, so `ping -n 10` takes
   ~10s, and the reliability check inherited that.
3. **PowerShell is a dependency to reason about.** It's present on every supported
   Windows, but Constrained Language Mode / hardened enterprise images can restrict it,
   and ~5 process spawns per run is inelegant for a small utility.

Separately, the project's owner set a fixture policy: **captured fixtures must not
contain network-identifying data** — MACs, SSIDs, anything not publicly available.
That is in tension with the `empirical-fixtures` skill (which commits captures
verbatim) and with the `ethernet-vmware-windows/` corpus the previous PR added (real
MACs, real private IPs).

## Decision

The Windows `PlatformProbe` calls the Win32 API directly via the `windows-sys` crate.
No `powershell.exe`, no `netsh`, no `ping.exe`.

| Probe | API (all from `iphlpapi.dll` / `wlanapi.dll`) |
|---|---|
| `default_route` | `GetBestRoute2` to a public IPv4 → `NextHop` + `InterfaceLuid` |
| `interface_addr`, `dns_info`, `interface_type` | one `GetAdaptersAddresses` walk; `IfType` is an enum (`IF_TYPE_IEEE80211` / `IF_TYPE_ETHERNET_CSMACD` / `IF_TYPE_TUNNEL` / `IF_TYPE_PPP`) |
| `arp_neighbors` | `GetIpNetTable2` filtered by interface LUID; `State` is an enum (`NlnsReachable`, `NlnsStale`, …) |
| `wifi_info` | `WlanOpenHandle` → `WlanQueryInterface(current_connection)` → `WLAN_CONNECTION_ATTRIBUTES`; `dot11AuthAlgorithm` is an enum (`DOT11_AUTH_ALGO_RSNA_PSK` = WPA2, `…_WPA3_*`, `…_80211_OPEN`), signal quality is already 0–100 %; channel via `WlanQueryInterface(channel_number)` |
| reliability ping | `IcmpCreateFile` + `IcmpSendEcho2` per echo, in `spawn_blocking`, with a 200 ms inter-echo gap → 10 echoes in ~2s |

`windows-sys` is added as `[target.'cfg(windows)'.dependencies]` — it never touches the
Linux or macOS build. It is already a transitive dependency (via `tokio`/`mio`/
`socket2`), so this only enables a few more (mostly declaration-only) feature modules:
`Win32_NetworkManagement_IpHelper`, `_Ndis`, `_WiFi`, `Win32_Networking_WinSock`,
`Win32_Foundation`, `Win32_System_IO`. License is MIT/Apache-2.0.

**Fixture consequence:** there is no captured command output on Windows any more, so
there is nothing to sanitize — the `ethernet-vmware-windows/` corpus and
`parse_windows_ping` / the `Get-*` text parsers are deleted. Windows parser tests become
pure mapping-function unit tests (`DOT11_AUTH_ALGORITHM` → `WifiEncryption`, `IfType` →
`InterfaceKind`, ICMP status → reachable, multicast/broadcast MAC filtering on
synthetic bytes). Coverage that the API is walked correctly comes from the contract
tests (`tests/topology.rs`, `tests/security.rs`, `tests/reliability.rs`) run on a real
Windows machine — the same testing shape `speed.rs` already uses (no faked transport,
contract test carries the integration claim).

## Rationale

**Win32 API over the console tools.** The console tools (`ipconfig`, `route print`,
`netsh`) were never a real option — they localize worse than `ping.exe`. The only
question was PowerShell-cmdlet-text vs. the API the cmdlets themselves call. The API
wins on every axis except lines-of-code: language-invariant, no child processes,
structured enums instead of string matching (an auth algorithm is a number, not a
label to pattern-match), and it's the same data source with one fewer layer of
translation. `GetAdaptersAddresses` alone returns address + prefix + DNS servers +
interface type + friendly name in one call.

**`IcmpSendEcho2` over `ping.exe`.** It removes the localization hole *and* the 10s
penalty in one move — the inter-echo interval is ours to choose. It needs no elevated
privileges (unlike a raw ICMP socket). The cost is that the reliability check's
injection seam changes from "a function that runs a command" to "a function that pings
one host and returns a `PingSummary`" — see Consequences.

**`windows-sys` over hand-rolled `extern "system"` blocks.** The project hand-rolls
where hand-rolling is small and stable (the NDT7 protocol, the OUI table). The Win32
structs here are not that — `IP_ADAPTER_ADDRESSES_LH` is ~40 fields with unions and
four linked-list heads, and it's versioned. Getting it subtly wrong is a memory-safety
bug, not a parse miss. `windows-sys` is generated from the official metadata, is
already compiled in our tree, and the `cfg(windows)` scoping means it is genuinely
zero-cost for the other two platforms.

**Deleting the fixtures rather than sanitizing them.** Sanitised captures are, per the
`empirical-fixtures` skill's own wording, no longer real captures — a parser built on a
scrubbed MAC can still fail on the real format. The API approach dissolves the dilemma:
nothing is captured, so nothing needs scrubbing or faking. The remaining fixture
corpora (`home-wifi-macos/`) and the fixture-sanitisation policy are tracked as a
separate issue.

## Stakeholders

Solo call — no other stakeholders consulted. The previous approach was implemented in
the same week (PR #1) and merged; this supersedes its probe mechanism after one round
of review, not after a long bake.

## Considerations / Revisit if

- **The GNU-toolchain decision from
  [2026-08-27-windows-platform-support.md](2026-08-27-windows-platform-support.md) still
  stands unchanged.** Today: build with `stable-x86_64-pc-windows-gnu` +
  `scoop install mingw`; that doc is `superseded` only for its *probe* content, not its
  toolchain content. Revisit if: distribution needs MSVC (unchanged from that doc).
- **`windows-sys` version churn.** Today: pinned to whatever `tokio` pulls (`0.61`);
  our direct dep floats with it. Revisit if: a future `tokio` bump moves to a
  `windows-sys` major that renames the modules we use — then pin our direct dep
  explicitly.
- **No committed Windows fixtures means non-English Windows is untested in CI.** Today:
  the API is language-invariant by construction, so there is less to test. Revisit if:
  a real non-English-Windows bug appears anyway (locale-dependent behaviour in an API
  we assumed was invariant) — tracked in the localization issue.
- **`IcmpSendEcho2` in `spawn_blocking`.** Today: 3 targets × 10 echoes, each echo a
  blocking call with a 200 ms gap, on blocking-pool threads. Revisit if: this contends
  with `speed.rs` for the blocking pool — then move to the async completion form
  (`IcmpSendEcho2` with an event handle).
- **WLAN channel needs a second `WlanQueryInterface` call.** Today: two calls
  (`current_connection` then `channel_number`). Revisit if: the second call proves
  unreliable across Windows versions — `WLAN_ASSOCIATION_ATTRIBUTES` has enough to
  derive the band but not the channel.

## The `unsafe` surface

Every FFI call, pointer dereference, linked-list walk, union field read, and
`mem::zeroed()` out-param is `unsafe`. It stays sound because:

- **`windows-sys` models every "enum" as a plain `i32`/`u32`**, so `mem::zeroed()` on
  the MIB out-param structs is a valid value (no forbidden bit patterns) — the same
  code against the `windows` crate's real enums would be UB.
- **Backing buffers outlive every pointer walk** — `list_adapters`'s `Vec<u64>` (8-byte
  aligned, which is what `IP_ADAPTER_ADDRESSES_LH` needs), the OS-allocated MIB tables,
  the WLAN blobs. MIB tables are `FreeMibTable`'d and the WLAN handle + blobs are freed
  by `Drop` guards, on every exit path.
- **Sockaddr reads use `ptr::read_unaligned`** (`SOCKET_ADDRESS.lpSockaddr` isn't
  documented to be `SOCKADDR_IN`-aligned); the ICMP reply buffer is `[u64]`-backed so
  the `ICMP_ECHO_REPLY` read is aligned.
- **A validation layer bounds everything the OS reports.** `MAX_ADAPTERS` /
  `MAX_NEIGHBORS` / `MAX_WLAN_INTERFACES` clamp any count before it reaches
  `slice::from_raw_parts` (clamping only ever shortens a slice); linked-list walks and
  the wide-string reader are iteration-capped; addresses are checked with
  `is_plausible_host_ipv4` (unicast, not loopback/multicast/broadcast/`0.0.0.0`),
  prefix lengths against `0..=32`, channel against `1..=196`, and every returned blob's
  size against `size_of::<T>()` before it is dereferenced.

- **Compile-time layout checks** — a `const _: ()` block asserts the size/alignment
  properties the `unsafe` depends on (`SOCKADDR_IN` is 16 bytes so the `read_unaligned`
  is in bounds; `ICMP_ECHO_REPLY` align ≤ 8 so the `[u64]` buffer is enough;
  `NET_LUID_LH` is 8 bytes). A `windows-sys` bump that moved a struct stops the build
  rather than reading the wrong bytes.

Enum/byte mappings and the validators are unit-tested. The one thing static checks
*can't* catch — `windows-sys` putting a field at the wrong offset, yielding a
plausible-but-wrong value — is caught by the topology contract test's new cross-field
invariant: on a WiFi/Ethernet link the gateway must be on the interface's own subnet
(`network::ipv4_in_cidr`), which a mis-read `NextHop` or address/prefix would violate.

## Consequences

- **New dependency:** `windows-sys` (Windows target only). `src/platform/windows.rs` is
  rewritten around FFI — roughly 2–3× the line count of the PowerShell version, isolated
  behind the unchanged `PlatformProbe` trait so `cli.rs` and the contract tests need no
  change beyond the ping seam.
- **`check_reliability`'s signature changes.** Was generic over
  `Fn(Vec<String>) -> Future<Output = io::Result<ExecResult>>` (a command runner); now
  generic over `Fn(String) -> Future<Output = PingSummary>` (ping one host). Production
  wiring passes `system_ping` (`#[cfg]`-split: `IcmpSendEcho2` on Windows,
  `ping -c 10 -i 0.2` + `parse_ping_output` elsewhere). The unit tests
  (`reliability-check-resilience#S1`–`#S4`) are rewritten to fake the ping function
  instead of the command runner — same scenarios, same citations.
- **Deleted:** `tests/fixtures/ethernet-vmware-windows/`, `parse_windows_ping` and its
  tests in `network.rs`, the Windows branch of `tests/fixtures/capture.sh`, and the
  `Get-*` / `netsh` text parsers in `windows.rs`.
- **`2026-08-27-windows-platform-support.md` → `status: superseded`**, forward-linked
  here. Its toolchain content is restated in the first Revisit-if bullet above so the
  chain is self-contained.
- **Two issues filed:** [#2](https://github.com/cobarx/pubnet-tools/issues/2)
  (non-English Windows / localization) and
  [#3](https://github.com/cobarx/pubnet-tools/issues/3) (fixture-sanitisation policy
  for the remaining corpora).
- `record` is still unsupported on Windows; `system_egress_ip` still returns `None`
  (DNS-interception verdict stays `uncertain`). Neither changes here.
