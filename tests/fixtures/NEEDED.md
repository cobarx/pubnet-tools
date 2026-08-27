# Fixtures needed

Cases not yet covered by any real capture. Each entry names what to capture and where.

| Context slug | Commands | How to get it |
|---|---|---|
| `airport-captive-macos` | all macOS commands | Run capture.sh at an airport or hotel before logging in |
| `airport-captive-linux` | all Linux commands | Same, on a Linux machine |
| `home-ethernet-macos` | all macOS commands | Plug in USB-C Ethernet, disconnect WiFi, run capture.sh |
| `vpn-tailscale-macos` | all macOS commands | Connect Tailscale, run capture.sh |
| `open-wifi-macos` | `airport_-I` | Connect to an open (no password) network |
| `wpa2-enterprise-linux` | `nmcli_dev_wifi_list` | Connect to a WPA2-Enterprise network (corporate, university) |

Windows has no fixtures: its probes call the Win32 API directly and parse no
command output (see `docs/decisions/2026-08-28-windows-probes-via-win32-api.md`).
Windows coverage lives in the contract tests, run on a real machine.
