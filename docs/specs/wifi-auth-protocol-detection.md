---
template_version: 1.0.0
slug: wifi-auth-protocol-detection
status: draft
owner: hampton
date: 2026-08-27
related: [wifi-info-detection, risk-scoring]
---

# Spec: Wi-Fi authentication protocol detection

## Intent

pubnetchk reports not just the cipher encryption on the connected Wi-Fi network
(already in `wifi-info-detection`) but the **authentication protocol** — whether
the current connection uses WPA2-PSK, WPA3-SAE, or a mixed WPA2+WPA3
transition-mode AP. When the AP is in transition mode, pubnetchk emits a finding
that explains both the security trade-off and the "bad password on correct
credentials" failure mode that older drivers exhibit against such APs.

**Not in scope:** scanning for nearby networks or their auth modes; radio type
(802.11ac vs 802.11ax); driver version detection; per-platform probe mechanism
(that belongs in a decision doc); any scoring-point change — the transition-mode
finding is informational only.

## Terms

- **Auth protocol** — the handshake used to establish the connection's session
  keys, distinct from the data-plane cipher (`WifiEncryption`). WPA2-PSK and
  WPA3-SAE use the same AES-CCMP cipher; the difference is in how the key is
  derived.
- **SAE (Simultaneous Authentication of Equals)** — the WPA3 Personal handshake.
  Uses a Diffie-Hellman commit-confirm exchange, making offline dictionary attacks
  against captured handshakes impractical.
- **WPA2+WPA3 transition mode** — an AP that advertises both WPA2-PSK and
  WPA3-SAE, accepting both. WPA3-capable clients negotiate SAE; WPA2-only clients
  fall back to PSK. Detecting this requires the OS or AP beacon to report the
  mixed mode explicitly — merely seeing that a connection used PSK is not
  sufficient evidence.
- **`Unknown`** — the platform's probe did not return enough information to
  classify the auth protocol. Not an error; no finding is emitted.

## Scenarios

### S1 — Connected via pure WPA3-SAE

**Happy path.**

- **Given** a Wi-Fi interface connected to an AP that advertises WPA3-SAE only
- **When** the security check reads Wi-Fi info
- **Then** `auth_protocol` is `SAE`
- **And** no `security.wifi-wpa3-transition` finding is emitted
- **And** the existing `encryption` field still reflects `WPA3` as before

### S2 — Connected to a WPA2+WPA3 transition-mode AP

**Happy path — the primary new behavior.**

- **Given** a Wi-Fi interface connected to an AP in WPA2+WPA3 transition mode
- **And** the OS or probe can identify the AP as transition mode
- **When** the security check reads Wi-Fi info
- **Then** `auth_protocol` is `SAE-Transition`
- **And** a finding `security.wifi-wpa3-transition` (Info, 0 pts) is emitted
- **And** the finding detail explains that the AP accepts both WPA2 and WPA3
  clients, that some drivers fail the WPA3 handshake against such APs and show a
  "bad password" error, and that forcing WPA2-PSK in the OS profile is the
  workaround
- **And** the existing `encryption` field still reflects `WPA2` or `WPA3` as
  reported by the current connection

### S3 — Connected via WPA2-PSK

**Edge — no new finding.**

- **Given** a Wi-Fi interface connected to a WPA2-PSK-only AP
- **When** the security check reads Wi-Fi info
- **Then** `auth_protocol` is `PSK`
- **And** no `security.wifi-wpa3-transition` finding is emitted
- **And** the existing `encryption` field still reflects `WPA2` as before

### S4 — Auth protocol cannot be determined

**Failure — platform gap.**

- **Given** a Wi-Fi interface that is connected
- **And** the platform probe cannot classify the auth protocol (e.g. a platform
  path that does not expose this information yet, or the call fails)
- **When** the security check reads Wi-Fi info
- **Then** `auth_protocol` is `Unknown`
- **And** no `security.wifi-wpa3-transition` finding is emitted
- **And** no error is surfaced — `Unknown` is a first-class, non-error state

### S5 — Not on Wi-Fi

**Edge — existing behavior unchanged.**

- **Given** a default-route interface that is Ethernet or a VPN tunnel
- **When** the security check reads Wi-Fi info
- **Then** `auth_protocol` is `Unknown` (or absent from JSON when no Wi-Fi block
  is emitted)
- **And** no `security.wifi-wpa3-transition` finding is emitted
- **And** the not-on-Wi-Fi behavior specified in `wifi-info-detection#S3` is
  otherwise unchanged

## Open questions

- **What exact `WiFiAuth` values does `ipconfig getsummary` return for transition
  mode on macOS 15?** The probe needs an empirical fixture showing the
  `WPA3_SAE_TRANSITION` (or equivalent) key. Deferred until a macOS 15 machine
  is available. **Blocks: ticket 004.**

## Resolved questions

- **Does Windows expose transition mode separately from per-connection auth
  algorithm?** No. `wlan_intf_opcode_current_connection` →
  `wlanSecurityAttributes.dot11AuthAlgorithm` reports what the *current connection*
  negotiated (7 = RSNA_PSK = WPA2-PSK; 9 = WPA3_SAE; 10 = OWE; 6 = RSNA =
  WPA2-Enterprise). There is no `DOT11_AUTH_ALGORITHM` constant for transition
  mode — it is an AP-capability property, not a connection property. Detecting
  true transition mode would require `WlanGetNetworkBssList` + RSN IE parsing
  (checking that the AP's AKM Suite list contains both PSK and SAE selectors),
  which is substantially more complex. **Decision:** defer BSS IE parsing for v1.
  Windows reports `Psk` or `Sae` from the already-read `dot11AuthAlgorithm` field;
  `SaeTransition` is macOS-only in v1. The data read is already in place in
  `src/platform/windows.rs` — ticket 003 is a mapping change, not a new API call.
  *(Spike: 2026-08-27)*

## Done when

- [ ] `S1` holds: pure WPA3-SAE connection → `auth_protocol: SAE`, no transition
      finding
- [ ] `S2` holds: transition-mode AP → `auth_protocol: SAE-Transition`, finding
      emitted with the correct explanation
- [ ] `S3` holds: WPA2-PSK connection → `auth_protocol: PSK`, no finding
- [ ] `S4` holds: unknown auth → `auth_protocol: Unknown`, no finding, no error
- [ ] `S5` holds: not on Wi-Fi → no transition finding, existing S3 behavior of
      `wifi-info-detection` unchanged
- [ ] The `security.wifi-wpa3-transition` finding has 0 pts and does not affect
      the risk score
- [ ] `auth_protocol` is present in the JSON report under the security block
- [ ] Platform coverage: Windows, macOS, Linux each return a non-`Unknown`
      `auth_protocol` in the contract test on a live network (even if
      `SAE-Transition` is not reached in every environment)

## Why this behavior

This feature was motivated by a real incident: an Intel AC 9560 Windows driver
failing WPA3-SAE negotiation against an AT&T residential gateway in transition
mode, surfacing as a "bad password" error even with the correct passphrase. A
Linux client on the same AP worked. See
`docs/context/wpa3-driver-compatibility.md` for the full incident log.

Surfacing the auth protocol makes the "bad password with correct credentials"
failure diagnosable: if pubnetchk shows `SAE-Transition`, the reader knows the AP
is in transition mode and can look up the driver-compatibility angle. Zero scoring
points: this is an informational note, not a security flaw — transition mode is
meaningfully better than pure WPA2, and the driver issue is a client-side bug,
not an AP misconfiguration.
