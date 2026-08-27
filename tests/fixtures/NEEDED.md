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
| `wifi-windows` | `netsh_wlan_show_interfaces` + all Windows commands | Run capture.sh on a Windows laptop associated to a real AP. `parse_netsh_wlan` (SSID/auth/channel/signal) has **no real capture** yet — only the wlansvc-stopped case in `ethernet-vmware-windows`. Needed for exact-value assertions. |
| `captive-windows` | all Windows commands | Run capture.sh on Windows at an airport/hotel before logging in |
| `wifi-windows-non-english` | `netsh_wlan_show_interfaces` | Same on a non-English Windows — confirms the localized-label fall-through to "no WiFi info" |
