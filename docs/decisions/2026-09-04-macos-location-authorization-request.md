---
template_version: 1.4.0
date: 2026-09-04
slug: macos-location-authorization-request
status: accepted
decided_by: hampton
related: [2026-08-26-macos-wifi-without-airport]
---

# Decision: `pubnetchk permissions --request` calls CoreLocation directly

## Context

`2026-08-26-macos-wifi-without-airport.md` (and its 2026-09-04 correction) established
that macOS 15+ withholds the Wi-Fi SSID from any process without Location Services
authorization, that no per-app grant appears in System Settings for a bare CLI binary
or the terminal hosting it, and deferred the only actual fix — pubnetchk requesting its
own CoreLocation authorization like a real `.app` — as future work (tracked as
[issue #48](https://github.com/cobarx/pubnet-tools/issues/48)), because it means new
`unsafe` FFI surface and uncertain payoff (historically, only bundled, GUI-capable
apps could present the TCC authorization alert at all).

This decision is that attempt.

## Decision

Two additions, both macOS-only:

1. **`crates/pubnetchk/build.rs`** embeds `crates/pubnetchk/Info.plist` — carrying
   `CFBundleIdentifier` and `NSLocationWhenInUseUsageDescription` — into the `pubnetchk`
   binary's Mach-O `__TEXT,__info_plist` section via
   `cargo:rustc-link-arg-bin=pubnetchk=-Wl,-sectcreate,__TEXT,__info_plist,<path>`. This
   is the established technique (used by other CLI tools, e.g. `terminal-notifier`) for
   giving a loose executable the usage-description metadata TCC needs to show its
   prompt, without building a full `.app` bundle.
2. **`crates/pubnetchk/src/macos_location.rs`** calls `CLLocationManager` directly
   (`objc2-core-location`): `authorization_status()` reads the current state;
   `request_when_in_use_authorization(timeout)` calls
   `requestWhenInUseAuthorization()` then pumps `CFRunLoopRunInMode` (via
   `core-foundation`) in a loop, since there's no `NSApplication`/AppKit run loop to
   drive it, until the status changes or `timeout` elapses.

`pubnetchk permissions` gains `--request`: it calls `request_when_in_use_authorization`
first (only when the current status is `NotDetermined`) and reports the resulting
status, then proceeds with the existing live Wi-Fi read as before. `--open` is
unchanged (still just navigates to the Settings pane).

## Rationale

**Why now, not left deferred.** The deferred-work assessment was reversible on cheap
new evidence: the dependencies exist, are actively maintained (`objc2-core-location`
0.3.2, part of the same `objc2` ecosystem the project would eventually want for
CoreWLAN too), and the Info.plist-embedding technique is well-established elsewhere.
Cheap to try, so tried.

**Why this may still not work, and that's an acceptable outcome to ship.** Apple's own
guidance and community reports consistently describe CoreLocation authorization as
something only a bundled application with a running UI event loop reliably obtains;
several forum threads note ambiguity for CLI-only processes specifically (see the
research trail in the corrected `2026-08-26` doc). This decision ships the attempt and
reports the real, observed `CLAuthorizationStatus` back to the person running it rather
than assuming success — `NotDetermined` after the full timeout, or `Denied` with no
prompt ever appearing, are both plausible and both handled as ordinary outcomes, not
failures. Whether it actually reveals the SSID is an empirical question this change
lets `pubnetchk permissions` answer honestly, on a specific person's specific Mac,
instead of staying purely theoretical.

**Why polling instead of a delegate.** `CLLocationManagerDelegate`'s
`locationManagerDidChangeAuthorization:` callback needs the same run loop pumping to
ever fire, so it buys nothing here over polling `authorizationStatus()` directly
between short `CFRunLoopRunInMode` ticks — simpler, no `ProtocolObject` trait impl to
write for a one-shot check.

**Why `objc2-core-location` over hand-written Objective-C FFI.** It's the same
binding-generation family (`objc2`) the project would use for CoreWLAN if that's
picked up later, actively maintained, and covers exactly the two calls needed
(`CLLocationManager::new`, `requestWhenInUseAuthorization`,
`authorizationStatus`) without hand-rolling `objc_msgSend` calls and selector
encoding by hand.

**Why not also bind CoreWLAN in this change.** Scope discipline: this decision is
specifically "can pubnetchk obtain the authorization at all," which is answerable with
just `CLLocationManager`. CoreWLAN (`CWInterface`) for instant channel/signal remains a
separate, larger dependency addition the original decision deferred independently —
still tracked as its own future item.

## Stakeholders

Solo call — no other stakeholders.

## Considerations / Revisit if

- **The prompt never appears / status never leaves `NotDetermined`, on a signed
  release build.** This is the expected-possible outcome the rationale above already
  accepts. Revisit if: it turns out there's a further requirement (e.g. a proper code
  signature with a Team ID, not ad-hoc; a `LSUIElement`/agent bundle instead of a loose
  binary) that a follow-up change could satisfy — file a new ticket rather than
  reopening this one, since by then this decision's actual question ("can the bare
  binary + Info.plist section approach work at all") will have been answered.
- **Ad-hoc code signing on rebuild invalidates the grant.** `cargo build`'s ad-hoc
  signature is typically tied to the binary's own hash, so a local dev rebuild may look
  like a "new" process to TCC even after a prior grant. Expected during iteration; not
  expected to affect a stable, once-built release binary. Revisit if: this turns out to
  also affect release builds (would need real code signing + notarization, a much
  bigger step this project has avoided so far).
- **`requestWhenInUseAuthorization` blocks past its timeout on some macOS version.**
  Today: bounded by the `timeout` argument regardless of the true system behavior — the
  loop always returns. Revisit if: a shorter/longer default proves better empirically.

## Consequences

- **New macOS-only dependencies:** `objc2`, `objc2-core-location`, `core-foundation` —
  `[target.'cfg(target_os = "macos")'.dependencies]` in `crates/pubnetchk/Cargo.toml`.
  No change to the Linux/Windows/Android dependency tree.
- **`crates/pubnetchk/build.rs`** (new) + **`crates/pubnetchk/Info.plist`** (new).
- **`crates/pubnetchk/src/macos_location.rs`** (new, `cfg(target_os = "macos")`).
- **`cli.rs`**: `Commands::Permissions` gains `--request`; `permissions_command_macos`
  calls it before the existing live Wi-Fi read.
- **`docs/specs/permissions-helper.md`**: gains scenarios for `--request`'s three
  outcomes (granted, denied/restricted, still undetermined after timeout).
