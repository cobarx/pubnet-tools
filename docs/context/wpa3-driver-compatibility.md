# WPA3 Driver Compatibility — Troubleshooting Log

**Date:** 2026-08-27
**Machine:** Windows 11 Home (10.0.26200)
**Adapter:** Intel Wireless-AC 9560 160MHz — interface "Wi-Fi 2"
**Driver at time of fix:** 23.110.0.5 (2025-01-02)

## Symptom

Connecting to SSID `attinternet` (AT&T residential gateway) failed with a "bad
password" / incorrect password error on this Windows machine. The same passphrase
connected successfully on the user's Linux laptop.

## Diagnosis

1. **Listed saved Wi-Fi profiles** — `netsh wlan show profiles`
   - **No `attinternet` profile existed.** This was not Windows silently
     reusing a stale/incorrect saved password — the handshake was genuinely
     failing on the first attempt.

2. **Checked system clock** — correct (clock skew can break auth). Not the cause.

3. **Checked adapter + driver** — Intel Wireless-AC 9560, driver 23.110.0.5.
   A 2017 Wi-Fi 5 chip near the end of Intel's release line.

### Root cause

The AC 9560's Windows driver mishandles **WPA3-Personal / SAE** and Protected
Management Frame (PMF) negotiation, particularly against **WPA2+WPA3
transition-mode** APs (which AT&T residential gateways broadcast). The failed
SAE exchange surfaces to the user as a bogus "bad password" error. Linux
`wpa_supplicant` negotiates the same AP correctly.

This failure mode is not unique to this adapter — any driver that misimplements
the SAE (Simultaneous Authentication of Equals) handshake against a
transition-mode AP will exhibit it. The user sees an authentication failure that
looks like a wrong password; the actual cause is a protocol negotiation bug.

## Fix applied

Created a Wi-Fi profile that **forces plain WPA2-PSK / AES**, bypassing the
buggy SAE code path. Also disabled per-network MAC randomization (AT&T gateways
sometimes reject randomized MACs during association).

Commands:

```powershell
netsh wlan add profile filename="attinternet.xml" user=all
netsh wlan connect name=attinternet
```

The profile XML forces `WPA2PSK` / `AES`, sets `useOneX: false`, and disables
MAC randomization. Passphrase stored only in the OS-managed profile, not here.

## Result — connected

```
State           : connected
SSID            : attinternet
AP BSSID        : 10:f0:68:a0:b2:c0
Authentication  : WPA2-Personal
Band            : 5 GHz
Channel         : 120
Radio type      : 802.11ac
Cipher          : CCMP
Signal          : ~58%
```

## Driver situation (checked 2026-08-27)

**Installed:** `netwtw08.inf` v23.110.0.5 (2025-01-02)

**Newer available:** Intel 24.x branch still covers the 9560. Latest appears
to be ~v24.60.0 (Aug 2026). Pending in Windows Update:
- `Intel net Driver Update (24.40.0.4)` — dated 2026-04-13. Better WPA3/SAE
  and PMF handling; may resolve the root cause without the forced-WPA2 workaround.

## Driver archive

Links preserved for reproduction of the bug against v23.x (the affected branch):

- **Intel AC 9560 downloads (official):**
  [intel.com product page](https://www.intel.com/content/www/us/en/products/sku/99446/intel-wirelessac-9560/downloads.html)
- **Intel PROSet/Wireless driver index** (version history, all supported adapters):
  [intel.com/support/000046918](https://www.intel.com/content/www/us/en/support/articles/000046918/wireless.html)
- **Softpedia mirror** of v23.110.0 64-bit (closest available public mirror for the
  exact `netwtw08.inf` build; verify checksum against Intel's package before use):
  [drivers.softpedia.com](https://drivers.softpedia.com/get/NETWORK-CARD/INTEL/Intel-Wireless-AC-9560-WLAN-Driver-23-110-0-64-bit.shtml)

To reproduce: install v23.110.0.5 on an Intel AC 9560 machine, delete any saved
`attinternet` profile (`netsh wlan delete profile name=attinternet`), and attempt
to connect to a WPA2+WPA3 transition-mode AP via the Windows UI. The bogus "bad
password" error appears immediately.

## Relevance to pubnetchk

This incident is the direct motivation for the **Wi-Fi auth protocol detection**
feature. When pubnetchk runs on a machine connected to a WPA2+WPA3
transition-mode AP, it should:

1. Surface the auth protocol actually in use (PSK vs SAE).
2. Flag transition mode as a security finding with context — including the
   "bad password on correct passphrase" failure mode — so a user who just
   fought this knows what they're looking at.
3. On Windows, derive this from the WLAN API's
   `wlan_intf_opcode_current_connection` / `dot11AuthAlgorithm` rather than
   shelling out to `netsh`.

See `docs/specs/wifi-auth-protocol-detection.md` and the epic that implements it.
