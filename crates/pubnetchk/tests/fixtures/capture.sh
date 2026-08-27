#!/usr/bin/env bash
# Captures real command output from the current machine and network into a
# named fixture directory. Run this at every new network environment.
#
# Usage: bash tests/fixtures/capture.sh <context-name>
# Example: bash tests/fixtures/capture.sh airport-captive-macos
#          bash tests/fixtures/capture.sh home-ethernet-linux
#
# Output is committed to git. See skills/empirical-fixtures for the full discipline.

set -euo pipefail

CONTEXT="${1:?Usage: $0 <context-name>}"
DIR="$(cd "$(dirname "$0")" && pwd)/$CONTEXT"

if [[ -d "$DIR" ]]; then
    echo "Directory $DIR already exists — files will be overwritten."
fi
mkdir -p "$DIR"

# Silently skip commands that aren't available or fail on this platform
run() {
    local label="$1"; shift
    "$@" > "$DIR/${label}.txt" 2>/dev/null || true
}

OS="$(uname -s)"
IFACE=""

if [[ "$OS" == "Darwin" ]]; then
    echo "Capturing macOS fixtures..."

    # Discover default interface first so we can use it in subsequent commands
    IFACE=$(route -n get default 2>/dev/null | awk '/interface:/{print $2}' || echo "unknown")

    run "route_-n_get_default"                  route -n get default
    run "ifconfig_${IFACE}"                     ifconfig "$IFACE"
    run "arp_-an_-i_${IFACE}"                   arp -an -i "$IFACE"
    run "scutil_--dns"                          scutil --dns
    run "networksetup_-listallhardwareports"    networksetup -listallhardwareports

    # Wi-Fi: `airport` was removed in macOS 15/26. `ipconfig getsummary` is the
    # fast path (SSID + Security, instant); `system_profiler` is the slow path
    # (~7s) that also carries channel and signal. See
    # docs/decisions/2026-08-26-macos-wifi-without-airport.md.
    # SSID/BSSID read `<redacted>` unless this terminal has a Location Services
    # grant — see tests/fixtures/NEEDED.md (`ssid-visible-macos`).
    run "ipconfig_getsummary_${IFACE}"          ipconfig getsummary "$IFACE"
    run "system_profiler_-json_SPAirPortDataType" system_profiler -json SPAirPortDataType

elif [[ "$OS" == "Linux" ]]; then
    echo "Capturing Linux fixtures..."

    IFACE=$(ip route show default 2>/dev/null | awk '/default/{print $5}' | head -1 || echo "unknown")

    run "ip_route_show_default"                 ip route show default
    run "ip_addr_show_${IFACE}"                 ip addr show "$IFACE"
    run "ip_neigh_show_dev_${IFACE}"            ip neigh show dev "$IFACE"
    run "resolvectl_status"                     resolvectl status
    run "nmcli_dev_wifi_list"                   nmcli -t -f active,ssid,security,chan,freq,signal dev wifi list

# No Windows branch: the Windows probes call the Win32 API directly (IP Helper
# / WLAN / ICMP) and parse no command output, so there is nothing to capture.
# See docs/decisions/2026-08-28-windows-probes-via-win32-api.md.

else
    echo "Warning: unsupported OS $OS — no commands captured"
fi

# Prompt for a short description to put in the notes field
echo ""
read -r -p "Short notes for this capture (network type, what's notable): " NOTES

cat > "$DIR/meta.toml" <<META
context      = "$CONTEXT"
captured_at  = "$(date -u +%Y-%m-%dT%H:%M:%SZ)"
os           = "$(uname -s) $(uname -r)"
interface    = "$IFACE"
notes        = "$NOTES"
META

echo ""
echo "Captured to $DIR/"
ls "$DIR/"
