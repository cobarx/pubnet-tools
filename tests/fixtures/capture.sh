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

    AIRPORT="/System/Library/PrivateFrameworks/Apple80211.framework/Versions/Current/Resources/airport"
    if [[ -x "$AIRPORT" ]]; then
        run "airport_-I"                        "$AIRPORT" -I
    fi

elif [[ "$OS" == "Linux" ]]; then
    echo "Capturing Linux fixtures..."

    IFACE=$(ip route show default 2>/dev/null | awk '/default/{print $5}' | head -1 || echo "unknown")

    run "ip_route_show_default"                 ip route show default
    run "ip_addr_show_${IFACE}"                 ip addr show "$IFACE"
    run "ip_neigh_show_dev_${IFACE}"            ip neigh show dev "$IFACE"
    run "resolvectl_status"                     resolvectl status
    run "nmcli_dev_wifi_list"                   nmcli -t -f active,ssid,security,chan,freq,signal dev wifi list

elif [[ "$OS" == MINGW* || "$OS" == MSYS* || "$OS" == CYGWIN* ]]; then
    echo "Capturing Windows fixtures..."

    # Every probe on Windows goes through PowerShell's Get-Net* cmdlets. Their
    # property names are English regardless of the Windows display language, so
    # `Format-List` output is a stable "Key : Value" shape to parse — unlike
    # `ipconfig`/`route print`, which localize. Raw (verbatim) output is saved:
    # the leading/trailing blank lines Format-List emits are part of the data.
    ps() { powershell -NoProfile -NonInteractive -Command "$1"; }

    IFACE=$(ps "(Get-NetRoute -DestinationPrefix 0.0.0.0/0 | Sort-Object RouteMetric | Select-Object -First 1).InterfaceAlias" 2>/dev/null | tr -d '\r' | head -1 || echo "unknown")

    run "get-netroute_default"            ps "Get-NetRoute -DestinationPrefix 0.0.0.0/0 | Select-Object NextHop,InterfaceAlias,InterfaceIndex,RouteMetric | Format-List"
    run "get-netipaddress_ipv4"           ps "Get-NetIPAddress -AddressFamily IPv4 | Select-Object IPAddress,InterfaceAlias,PrefixLength | Format-List"
    run "get-netneighbor_ipv4"            ps "Get-NetNeighbor -AddressFamily IPv4 | Select-Object IPAddress,LinkLayerAddress,State,InterfaceAlias | Format-List"
    run "get-netadapter"                  ps "Get-NetAdapter | Select-Object Name,InterfaceDescription,PhysicalMediaType,Status,ifIndex | Format-List"
    run "get-dnsclientserveraddress_ipv4" ps "Get-DnsClientServerAddress -AddressFamily IPv4 | Select-Object InterfaceAlias,InterfaceIndex,ServerAddresses | Format-List"
    run "netsh_wlan_show_interfaces"      netsh wlan show interfaces
    run "arp_-a"                          arp -a
    run "ping_-n_4_1.1.1.1"              ping -n 4 1.1.1.1
    run "ping_-n_4_-w_1000_192.0.2.1"    ping -n 4 -w 1000 192.0.2.1

else
    echo "Warning: unsupported OS $OS — no commands captured"
fi

# Prompt for a short description to put in the notes field
echo ""
read -r -p "Short notes for this capture (network type, what's notable): " NOTES

OS_STRING="$(uname -s) $(uname -r)"
if [[ "$OS" == MINGW* || "$OS" == MSYS* || "$OS" == CYGWIN* ]]; then
    OS_STRING="$(powershell -NoProfile -NonInteractive -Command '(Get-CimInstance Win32_OperatingSystem).Caption + " " + [Environment]::OSVersion.Version' 2>/dev/null | tr -d '\r' | head -1)"
fi

cat > "$DIR/meta.toml" <<META
context      = "$CONTEXT"
captured_at  = "$(date -u +%Y-%m-%dT%H:%M:%SZ)"
os           = "$OS_STRING"
interface    = "$IFACE"
notes        = "$NOTES"
META

echo ""
echo "Captured to $DIR/"
ls "$DIR/"
