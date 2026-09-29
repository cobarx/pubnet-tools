---
template_version: 1.0.0
slug: permissions-helper
status: agreed
owner: hampton
date: 2026-09-04
related: [wifi-info-detection]
---

# Spec: `pubnetchk permissions` helper

## Intent

`wifi-info-detection` documents that macOS 15+ withholds the Wi-Fi SSID from any
process — including `pubnetchk` — that lacks Location Services authorization, and that
a plain CLI binary (not an `.app` bundle) cannot itself trigger the authorization
dialog (`docs/decisions/2026-08-26-macos-wifi-without-airport.md`). That spec explicitly
puts "requesting OS permissions" out of scope.

`pubnetchk permissions` is the instrument for the human side of that gap: it tells the
person running pubnetchk *whether* the name is currently hidden, *why*, and gives them
a one-command way to land on the exact System Settings pane that grants it — rather
than making them find Privacy & Security ▸ Location Services themselves.

**Not in scope:** programmatically requesting or granting the authorization (still
impossible for a bare CLI, per the linked decision doc); any permission other than
macOS Location Services; verifying grants for non-macOS platforms, which do not gate
the SSID this way.

## Terms

- **Redacted SSID** — as in `wifi-info-detection`: the OS reports a Wi-Fi network is
  joined but withholds its name.
- **Settings pane URL** — the `x-apple.systempreferences:` URL scheme macOS's `open`
  command accepts to jump straight to a System Settings pane, here
  `com.apple.preference.security?Privacy_LocationServices`.

## Scenarios

### S1 — Off macOS

- **Given** the host OS is not macOS
- **When** `pubnetchk permissions` runs
- **Then** it prints that this helper is macOS-only and that nothing needs granting on
  this platform
- **And** it exits 0 without touching the network or the OS

### S2 — macOS, not currently on Wi-Fi

- **Given** macOS, and the default-route interface is not Wi-Fi (or there is no default
  route)
- **When** `pubnetchk permissions` runs
- **Then** it reports there's nothing to check right now and to re-run once connected
  to Wi-Fi
- **And** it exits 0

### S3 — macOS, on Wi-Fi, SSID already visible

- **Given** macOS, connected to Wi-Fi, and this terminal already holds Location
  Services authorization
- **When** `pubnetchk permissions` runs
- **Then** it reports the SSID is already visible (and names it) and that there is
  nothing to grant
- **And** it exits 0
- **And** it does not open System Settings

### S4 — macOS, on Wi-Fi, SSID redacted, `--open` not passed

- **Given** macOS, connected to Wi-Fi, and the SSID is redacted
- **When** `pubnetchk permissions` runs without `--open`
- **Then** it explains why (a process must itself request and be granted Location
  Services authorization; neither the detected terminal — from `TERM_PROGRAM`,
  falling back to a generic "your terminal app" — nor pubnetchk does that) and states
  plainly that no per-app entry for either one will appear to grant
- **And** it names the one unconfirmed thing worth trying (the System Services ▸
  "Wi-Fi Networking" toggle, reached from the same Settings pane) and the actual
  tracked fix (embedding CoreLocation — the GitHub issue for it)
- **And** it prints that `pubnetchk permissions --open` will jump to that Settings pane
- **And** it exits 0 without launching System Settings

### S5 — macOS, on Wi-Fi, SSID redacted, `--open` passed

- **Given** macOS, connected to Wi-Fi, and the SSID is redacted
- **When** `pubnetchk permissions --open` runs
- **Then** it prints the same explanation as S4
- **And** it launches System Settings directly on the Location Services pane via
  `open "x-apple.systempreferences:com.apple.preference.security?Privacy_LocationServices"`
  (a navigation aid for checking System Services, not a claim that it grants anything)
- **And** it exits 0 even if the `open` launch itself fails (a failed launch is reported,
  not treated as a fatal error — the manual instructions already printed are enough to
  proceed)

## Open questions

None outstanding.

## Done when

- [x] `S1` holds: non-macOS prints a macOS-only notice and exits 0
- [x] `S2` holds: not on Wi-Fi prints a re-run hint and exits 0
- [x] `S3` holds: already-granted prints the SSID and does not open Settings
- [x] `S4` holds: redacted + no `--open` explains and instructs, does not launch Settings
- [x] `S5` holds: redacted + `--open` explains and launches Settings; a launch failure
      still exits 0
- [x] Terminal app name detection (`TERM_PROGRAM` → friendly name) is a pure, unit-tested
      function

## Why this behavior

The original version of this spec assumed the fix was "grant your terminal Location
Services access" via System Settings — matching what the finding text and the
`wifi-info-detection` renderer hints said at the time. Empirical testing (2026-09-04,
against both Terminal.app and a third-party terminal) found neither ever appears in
the Location Services app list at all: macOS only lists a process there once it has
itself called CoreLocation's authorization API, which no terminal emulator does and a
bare CLI cannot trigger on its own behalf. See the correction section in
`docs/decisions/2026-08-26-macos-wifi-without-airport.md`.

So this command cannot walk anyone through a fix that doesn't exist yet. What it can
do — and the reason it's worth having over just reading the static finding text — is
report the *live* state (some future macOS release, or a future signed/entitled build
of pubnetchk per
[issue #48](https://github.com/cobarx/pubnet-tools/issues/48), could change the
answer), name the one unconfirmed system-wide toggle worth trying, and point at the
actual tracked fix instead of a dead end.
