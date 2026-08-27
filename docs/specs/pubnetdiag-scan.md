---
template_version: 1.0.0
slug: pubnetdiag-scan
status: draft
owner: hampton
date: 2026-08-27
related: [wifi-auth-protocol-detection]
---

# Spec: pubnetdiag AP scanner

## Intent

`pubnetdiag` scans visible Wi-Fi access points without requiring an active
connection and reports each AP's SSID, authentication mode, band, channel, and
signal. When an AP is in WPA2+WPA3 transition mode, it flags it, explains the
driver-compatibility failure class (some adapters fail the WPA3-SAE handshake
and surface it as a bogus "bad password"), and — with `--repair` — applies the
WPA2-PSK workaround directly so the user can connect.

**Platform scope (v1): Windows only.** macOS and Linux require platform-specific
scan APIs not yet implemented; they are deferred to later releases.

**Not in scope (v1):** macOS or Linux; auditing the connected network (that is
`pubnetchk`'s job); anything requiring elevated privileges.

## Terms

- **Transition mode** — an AP whose RSN Information Element advertises both a
  PSK AKM suite (`00-0F-AC-2`) and an SAE AKM suite (`00-0F-AC-8`). Detected
  from the AP's beacon via `WlanGetNetworkBssList`, not from a connection
  attempt.
- **Scan** — passive enumeration of visible APs using the Windows WLAN API. No
  frames are injected; no active probing. The OS's existing BSS cache is used.
- **Repair** — creating a saved Wi-Fi profile that forces WPA2-PSK for the target
  SSID, bypassing the buggy SAE code path, then connecting to it. Requires the
  user to supply the passphrase.
- **Force (`-f`)** — reserved for a future release. Not implemented in v1.
- **Hidden SSID** — an AP broadcasting an empty or zero-length SSID field. Still
  visible by BSSID.

## Exit codes

| Code | Meaning |
|------|---------|
| 0 | Scan ran; nothing actionable found. With `--repair`: fix applied or not needed. |
| 1 | Scan ran; action needed (transition-mode AP found). With `--repair`: fix failed. |
| 2 | Scan could not run (no adapter, adapter disabled, API error). |

## Scenarios

### S1 — Scan finds visible APs, none in transition mode

**Happy path.**

- **Given** a Wi-Fi adapter that is present and enabled
- **When** the user runs `pubnetdiag`
- **Then** each visible AP is listed with its SSID (or `(hidden)`), BSSID,
  authentication mode, band, channel, and signal strength
- **And** no transition-mode warning is emitted
- **And** the exit code is 0

### S2 — Transition-mode AP is visible

**Happy path — primary behavior.**

- **Given** a Wi-Fi adapter that is present and enabled
- **And** at least one visible AP is in WPA2+WPA3 transition mode
- **When** the user runs `pubnetdiag`
- **Then** each transition-mode AP is marked distinctly in the output
- **And** a finding is shown explaining: the AP accepts both WPA2 and WPA3
  clients; some drivers fail the WPA3 handshake and show "bad password" with a
  correct passphrase; the workaround is to force WPA2-PSK
- **And** the exit code is 1

### S3 — No Wi-Fi adapter found or adapter is disabled

**Failure.**

- **Given** no Wi-Fi adapter is present, or the adapter is disabled
- **When** the user runs `pubnetdiag`
- **Then** the tool reports that no Wi-Fi interface was found
- **And** no scan results or findings are shown
- **And** the exit code is 2

### S4 — Adapter present but no networks in range

**Edge.**

- **Given** a Wi-Fi adapter that is present and enabled
- **And** no APs are visible
- **When** the user runs `pubnetdiag`
- **Then** the tool reports that no networks were found
- **And** the exit code is 0

### S5 — Currently connected AP appears in results

**Edge.**

- **Given** the user is currently connected to a Wi-Fi AP
- **When** the user runs `pubnetdiag`
- **Then** the connected AP appears in the scan list, marked as the current
  connection
- **And** it is evaluated for transition mode the same as any other AP
- **And** if it is in transition mode, S2's finding fires

### S6 — Hidden-SSID AP is visible

**Edge.**

- **Given** an AP in range that broadcasts an empty SSID
- **When** the user runs `pubnetdiag`
- **Then** it appears in the list as `(hidden)` with its BSSID, auth mode, band,
  channel, and signal
- **And** it is evaluated for transition mode; if transition mode, S2's finding
  fires
- **And** no error is surfaced for the missing SSID

### S7 — User targets a specific SSID

**Happy path — focused mode.**

- **Given** a Wi-Fi adapter that is present and enabled
- **When** the user runs `pubnetdiag <SSID>`
- **Then** only APs matching that SSID are shown
- **And** APs with other SSIDs are not shown
- **And** if the targeted SSID is in transition mode, S2's finding fires

### S8 — Targeted SSID not visible

**Edge — focused mode.**

- **Given** a Wi-Fi adapter that is present and enabled
- **And** the user runs `pubnetdiag <SSID>`
- **When** the scan completes and no AP with that SSID is found
- **Then** the tool reports that `<SSID>` was not found
- **And** lists the SSIDs that were visible (so the user can check for typos or
  band-steering aliases)
- **And** the exit code is 0

### S9 — Fix applied successfully

**Happy path — fix mode.**

- **Given** a Wi-Fi adapter that is present and enabled
- **And** the user runs `pubnetdiag --repair <SSID>`
- **And** an AP with that SSID is visible and in transition mode
- **When** the user supplies the correct passphrase when prompted
- **Then** a WPA2-PSK profile is created for that SSID (overwriting any existing
  profile for it)
- **And** the adapter connects to that SSID using WPA2-PSK
- **And** the tool confirms the connection succeeded
- **And** the exit code is 0

### S10 — Fix not needed

**Edge — fix mode.**

- **Given** a Wi-Fi adapter that is present and enabled
- **And** the user runs `pubnetdiag --repair <SSID>`
- **And** the targeted SSID is visible but is NOT in transition mode
- **Then** the tool reports no fix is needed for that SSID
- **And** no profile is created or modified
- **And** the exit code is 0

### S11 — Fix fails

**Failure — fix mode.**

- **Given** a Wi-Fi adapter that is present and enabled
- **And** the user runs `pubnetdiag --repair <SSID>`
- **And** the target SSID is in transition mode
- **When** the passphrase the user supplies is wrong, or the profile API call
  fails, or the connection attempt times out
- **Then** the tool reports the fix failed and why
- **And** any partially created profile is removed
- **And** the exit code is 1

## Open questions

- **macOS scanning.** `airport -s` was removed in macOS 15. Deferred to a later
  release; macOS is out of scope for v1.

## Done when

- [ ] `S1` holds: clean scan lists APs; exit 0
- [ ] `S2` holds: transition-mode AP flagged with finding; exit 1
- [ ] `S3` holds: no adapter → message, exit 2
- [ ] `S4` holds: adapter present, no APs → "no networks found", exit 0
- [ ] `S5` holds: connected AP in results, marked as current, evaluated for
      transition mode
- [ ] `S6` holds: hidden AP listed as `(hidden)` with BSSID and auth info
- [ ] `S7` holds: `pubnetdiag <SSID>` filters to matching APs; finding fires
      if applicable
- [ ] `S8` holds: targeted SSID not found → message + visible SSID list; exit 0
- [ ] `S9` holds: `--repair` succeeds → WPA2-PSK profile created, connected; exit 0
- [ ] `S10` holds: `--repair` on a non-transition AP → "no fix needed"; exit 0
- [ ] `S11` holds: `--repair` fails → failure message, partial profile cleaned up;
      exit 1
- [ ] Non-root on Windows — `WlanSetProfile` and `WlanConnect` do not require
      elevation for user-scope profiles
- [ ] Windows only for v1; macOS and Linux stubs compile but return "not
      supported on this platform"

## Why this behavior

`pubnetchk` can only run after a successful connection. The WPA3-SAE driver
failure class (documented in `docs/context/wpa3-driver-compatibility.md`)
surfaces as a "bad password" error before connection establishes — `pubnetchk`
is unreachable at that point. `pubnetdiag` reads AP beacon IEs directly via
`WlanGetNetworkBssList`, detecting transition mode without a connection, and with
`--repair` closes the loop by applying the workaround in the same session.

Exit code 1 on finding (not 0) follows the `grep` model: non-zero signals
"condition detected," making the tool scriptable without output parsing.
