---
template_version: 1.0.0
epic: pubnetdiag
ticket: 003
slug: cli-output
type: feature
points: 3
status: todo
tracker_ref: "17"
pr: none
related: [pubnetdiag-scan#S1, pubnetdiag-scan#S2, pubnetdiag-scan#S3, pubnetdiag-scan#S4, pubnetdiag-scan#S7, pubnetdiag-scan#S8]
---

# Ticket 003: CLI, output, and exit codes

## Goal

Wire the BSS scanner output to a human-readable terminal display, implement the
optional SSID filter argument, and enforce the 0/1/2 exit code contract.

## Scope

- **In:** `clap` CLI for `pubnetdiag` with optional positional `<SSID>` arg and
  `--repair` flag (flag parsed but behavior is ticket 4's job); scan result
  table: SSID, BSSID, auth mode, band, channel, signal; transition-mode `⚠`
  marker; finding block with explanation and workaround text; "not found" message
  + visible SSID list when targeted SSID is absent; "no networks found" for an
  empty scan; "no Wi-Fi adapter" for exit 2; exit codes 0/1/2
- **Out:** Repair logic (ticket 4); JSON output mode (not in v1 scope); color
  theming

## Acceptance criteria

Per `pubnetdiag-scan`:
- `S1`: clean scan → table printed, exit 0
- `S2`: transition AP → `⚠` in table + finding block, exit 1
- `S3`: no adapter → error message, exit 2, nothing else printed
- `S4`: adapter present, empty scan → "no networks found", exit 0
- `S7`: `pubnetdiag attinternet` → only `attinternet` rows shown; finding fires
  if transition mode
- `S8`: `pubnetdiag notanetwork` → "'notanetwork' not found" + visible SSID list,
  exit 0

## Notes

Finding text for S2 (keep under 100 words):
> This AP accepts both WPA2 and WPA3 clients (transition mode). Some drivers —
> including Intel Wireless-AC 9000-series on Windows — fail the WPA3 handshake
> against such APs and report "bad password" even with the correct passphrase.
>
> To connect: run `pubnetdiag --repair <SSID>` to force WPA2 and connect, or
> manually add a WPA2-PSK profile:
>   netsh wlan add profile filename=profile.xml

The table should align columns; signal as a percentage bar or numeric `58%` is
fine — pick one and be consistent. Connected AP marked with `*` or `[connected]`
suffix on its row.

`--repair` is parsed by clap in this ticket so the flag is recognized and
produces a helpful error ("repair not yet implemented" or routes to ticket 4's
handler); it must not silently do nothing.
