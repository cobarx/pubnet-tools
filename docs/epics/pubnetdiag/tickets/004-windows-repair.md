---
template_version: 1.0.0
epic: pubnetdiag
ticket: 004
slug: windows-repair
type: feature
points: 5
status: todo
tracker_ref: "18"
pr: none
related: [pubnetdiag-scan#S9, pubnetdiag-scan#S10, pubnetdiag-scan#S11]
---

# Ticket 004: `--repair` flag (WlanSetProfile + WlanConnect)

## Goal

When `--repair` is passed with a target SSID, prompt for the passphrase, create
a WPA2-PSK saved profile for that SSID via the WLAN API, and connect — all
without shelling out.

## Scope

- **In:** Passphrase prompt (stdin, echoed as `*` or silent); profile XML
  construction (WPA2PSK / AES / no MAC randomization, matching the template in
  `docs/context/wpa3-driver-compatibility.md`); `WlanSetProfile` call (user
  scope, overwrites existing); `WlanConnect` call; connection state polling until
  connected or timeout; cleanup of the created profile on failure (S11); exit
  codes per spec
- **Out:** `-f`/`--force` (reserved, not implemented); Linux/macOS repair;
  generating a profile file for the user to apply manually (the API call does it
  directly)

## Acceptance criteria

Per `pubnetdiag-scan`:
- `S9`: `pubnetdiag --repair attinternet` on a transition-mode AP → prompts for
  passphrase → creates WPA2-PSK profile → connects → confirms connected; exit 0
- `S10`: `--repair` on a non-transition AP → "no repair needed for <SSID>"; exit 0
- `S11`: wrong passphrase or connection timeout → "repair failed: <reason>" →
  profile removed → exit 1
- Profile creation uses user scope (`WLAN_PROFILE_USER`) so no elevation is
  required

## Notes

Profile XML template (from `docs/context/wpa3-driver-compatibility.md`):
```xml
<WLANProfile xmlns="...">
  <name>{ssid}</name>
  <SSIDConfig><SSID><name>{ssid}</name></SSID></SSIDConfig>
  <connectionType>ESS</connectionType>
  <connectionMode>auto</connectionMode>
  <MSM><security>
    <authEncryption>
      <authentication>WPA2PSK</authentication>
      <encryption>AES</encryption>
      <useOneX>false</useOneX>
    </authEncryption>
    <sharedKey>
      <keyType>passPhrase</keyType>
      <protected>false</protected>
      <keyMaterial>{passphrase}</keyMaterial>
    </sharedKey>
  </security></MSM>
</WLANProfile>
```

`WlanSetProfile` takes the XML as a wide string. `WlanConnect` takes the profile
name and interface GUID. Poll `wlan_intf_opcode_current_connection` after connect
to confirm the state becomes `wlan_interface_state_connected` — timeout after
~15 seconds and treat as S11.

The passphrase is in memory only — never written to disk, never logged. The
profile XML `WlanSetProfile` creates in the OS profile store is OS-encrypted at
rest (Windows Credential Manager); we don't need to handle that.

S11 cleanup: on any failure after `WlanSetProfile` succeeds, call
`WlanDeleteProfile` before exiting.
