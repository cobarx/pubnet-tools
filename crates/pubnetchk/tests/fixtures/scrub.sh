#!/usr/bin/env bash
# Scrubs personal data out of a fixture capture directory, in place, replacing it
# with stand-ins that are obviously fake: SSIDs become `STAND-IN SSID NN`. Run by
# capture.sh before anything is written to the fixture tree; can also be run by hand on
# an older capture.
#
# Usage: bash tests/fixtures/scrub.sh <dir> <context> [--keep-ssid NAME]...
#
#   --keep-ssid NAME   a public operator SSID that is not personal and matters to the
#                      fixture (e.g. YourTrainWiFi). Repeatable. Every other SSID is
#                      replaced.
#
# What is replaced (values are derived from the capture itself, not typed in):
#   SSIDs                  STAND-IN SSID NN, consistent across files
#   this machine's MACs    02:00:00:00:00:NN   (link/ether, ether, card MAC, and
#                                               networksetup's per-port addresses)
#   other devices' MACs    <OUI>:00:00:NN      (every ARP/neigh entry, the gateway
#                                               included; broadcast/multicast kept)
#   BSSIDs                 <OUI>:00:00:NN      (vendor kept, AP identity dropped)
#   hostname               standin-host
#
# After rewriting, the capture is re-read and the script fails (leaving the originals
# untouched) if any original value survives, or if any MAC remains that is not a
# stand-in or broadcast/multicast: a new capture format fails loudly
# rather than leaking. Prints a one-line summary for meta.toml on success.
# See docs/decisions/2026-09-24-scrub-personal-data.md.

set -euo pipefail

DIR="${1:?Usage: $0 <dir> <context> [--keep-ssid NAME]...}"
CONTEXT="${2:?Usage: $0 <dir> <context> [--keep-ssid NAME]...}"
shift 2
KEEP=()
while (($#)); do
    case "$1" in
        --keep-ssid) KEEP+=("${2:?--keep-ssid needs a value}"); shift 2 ;;
        *) echo "scrub.sh: unknown argument: $1" >&2; exit 2 ;;
    esac
done

WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

# --- Stand-ins ---------------------------------------------------------------------
# Plain placeholders: obviously fake, <= 32 bytes (the SSID limit), free of ':' and '\'.
PERSONA=standin-host
: >"$WORK/keep"
for k in "${KEEP[@]+"${KEEP[@]}"}"; do printf '%s\n' "$k" >>"$WORK/keep"; done

HOST="$(hostname -s 2>/dev/null || hostname)"

# Read order matters: interface files first, so this machine's MAC is classed as its
# own before it shows up again in an ARP table.
FILES=()
while IFS= read -r f; do FILES+=("$f"); done < <(
    find "$DIR" -maxdepth 1 -type f \( -name '*.txt' -o -name '*.json' \) | sort |
        awk '/ip_addr|ifconfig|system_profiler|networksetup/ { iface[++ni] = $0; next }
             { rest[++nr] = $0 }
             END { for (i = 1; i <= ni; i++) print iface[i]; for (i = 1; i <= nr; i++) print rest[i] }'
)
[[ ${#FILES[@]} -gt 0 ]] || { echo "scrub.sh: no .txt/.json files in $DIR" >&2; exit 1; }

# --- The scrubber: one awk program, run in three modes ----------------------------
#   collect  build the replacement maps (first pass over every file)
#   rewrite  write <file>.scrubbed with the maps applied
#   verify   re-extract from the scrubbed files; exit 1 if any original survives
#            or any MAC remains that is not a stand-in or broadcast/multicast
cat >"$WORK/scrub.awk" <<'AWK'
function hexre() { return "[0-9a-fA-F][0-9a-fA-F]?" }
function macre(  h) { h = hexre(); return h ":" h ":" h ":" h ":" h ":" h }
# Lower-case, zero-padded form, or "" if not a MAC.
function norm(m,   p, i, n, out) {
    n = split(tolower(m), p, ":"); if (n != 6) return ""
    out = ""
    for (i = 1; i <= 6; i++) { if (length(p[i]) == 1) p[i] = "0" p[i]; out = out (i > 1 ? ":" : "") p[i] }
    return out
}
# Render normalized MAC m in the padding style of orig (macOS arp drops leading zeros).
function styled(orig, m,   q, i, s, out) {
    if (orig !~ /(^|:)[0-9a-fA-F](:|$)/) return m
    split(m, q, ":"); out = ""
    for (i = 1; i <= 6; i++) { s = q[i]; sub(/^0/, "", s); if (s == "") s = "0"; out = out (i > 1 ? ":" : "") s }
    return out
}
function oui(m,   p) { split(m, p, ":"); return p[1] ":" p[2] ":" p[3] }
# The I/G bit: an odd first octet (second hex digit odd) is multicast.
function multicast(m) { return index("13579bdf", substr(m, 2, 1)) > 0 }
# Already a stand-in this script wrote (so re-running on a scrubbed capture is a no-op).
function placeholder(m) { return m ~ /^02:00:00:00:00:/ || m ~ /:00:00:[0-9a-f][0-9a-f]$/ }
function add_own(m) { m = norm(m); if (m == "" || placeholder(m) || m == "00:00:00:00:00:00" || m in mac_map) return; mac_map[m] = sprintf("02:00:00:00:00:%02x", ++n_own); kind[m] = "own" }
function add_other(m, what) { m = norm(m); if (m == "" || placeholder(m) || m in mac_map || m == "ff:ff:ff:ff:ff:ff" || multicast(m)) return; mac_map[m] = oui(m) sprintf(":00:00:%02x", ++n_other); kind[m] = what }
# Already a stand-in (a theme name, possibly with a " N" suffix), so a re-run is a no-op.
function standin_name(s) { return s ~ /^STAND-IN SSID [0-9]+$/ }
function special(s) { return s == "" || s == "<redacted>" || s == "--" || (s in keep) || standin_name(s) }
function add_ssid(s,   k) {
    if (special(s) || s in ssid_map) return
    k = ++n_ssid
    ssid_map[s] = sprintf("STAND-IN SSID %02d", k)
}
function iface_name(s) { return s ~ /^(en|awdl|llw|utun|bridge|ap|lo|wl|wlan|eth)[0-9]+$/ }
# Replace every literal occurrence of `from` in s with `to`.
function repl(s, from, to,   out, i) {
    out = ""
    while ((i = index(s, from)) > 0) { out = out substr(s, 1, i - 1) to; s = substr(s, i + length(from)) }
    return out s
}
# SSID-bearing fields, by capture format. Sets ssid_val / ssid_pre / ssid_post; returns 1 if found.
function ssid_field(line,   f, n, i, rest) {
    if (FILENAME ~ /nmcli/) {
        rest = line; gsub(/\\:/, "\034", rest)
        n = split(rest, f, ":"); if (n < 2) return 0
        ssid_pre = f[1] ":"; ssid_val = f[2]; ssid_post = ""
        for (i = 3; i <= n; i++) ssid_post = ssid_post ":" f[i]
        gsub(/\034/, "\\:", ssid_pre); gsub(/\034/, "\\:", ssid_val); gsub(/\034/, "\\:", ssid_post)
        return 1
    }
    if (match(line, /^[ \t]*(SSID|NetworkID) : /)) {
        ssid_pre = substr(line, 1, RLENGTH); ssid_val = substr(line, RLENGTH + 1); ssid_post = ""; return 1
    }
    if (match(line, /"_name" *: *"/)) {
        ssid_pre = substr(line, 1, RSTART + RLENGTH - 1); rest = substr(line, RSTART + RLENGTH)
        i = index(rest, "\""); if (i == 0) return 0
        ssid_val = substr(rest, 1, i - 1); ssid_post = substr(rest, i)
        return !iface_name(ssid_val)
    }
    return 0
}
BEGIN {
    while ((getline l < KEEP) > 0) keep[l] = 1
    MAC = macre()
}
mode == "collect" {
    if (match($0, "(link/ether|[ \t]ether) " MAC)) { s = substr($0, RSTART, RLENGTH); sub(/.*ether /, "", s); add_own(s) }
    if (match($0, "Ethernet Address: " MAC)) { s = substr($0, RSTART, RLENGTH); sub(/.*: /, "", s); add_own(s) }
    if (match($0, "\"spairport_wireless_mac_address\" *: *\"" MAC)) { s = substr($0, RSTART, RLENGTH); sub(/.*"/, "", s); add_own(s) }
    if (FILENAME ~ /ip_neigh/ && $2 == "lladdr") add_other($3, "neighbor")
    if (FILENAME ~ /arp_/ && $3 == "at") add_other($4, "neighbor")
    if (match($0, "(BSSID : |\"spairport_network_bssid\" *: *\")" MAC)) { s = substr($0, RSTART, RLENGTH); sub(/.*[ "]/, "", s); add_other(s, "bssid") }
    if (ssid_field($0)) add_ssid(ssid_val)
    next
}
mode == "rewrite" {
    line = $0
    if (ssid_field(line) && (ssid_val in ssid_map)) line = ssid_pre ssid_map[ssid_val] ssid_post
    out = ""; rest = line
    while (match(rest, MAC)) {
        tok = substr(rest, RSTART, RLENGTH); m = norm(tok)
        out = out substr(rest, 1, RSTART - 1) ((m in mac_map) ? styled(tok, mac_map[m]) : tok)
        rest = substr(rest, RSTART + RLENGTH)
    }
    line = out rest
    if (length(HOST) >= 3) line = repl(line, HOST, PERSONA)
    print line > (FILENAME ".scrubbed")
    next
}
mode == "verify" {
    if (ssid_field($0) && (ssid_val in ssid_map)) { print "scrub.sh: SSID survived in " FILENAME ": " ssid_val > "/dev/stderr"; bad = 1 }
    rest = $0
    while (match(rest, MAC)) {
        m = norm(substr(rest, RSTART, RLENGTH))
        if (m in mac_map) { print "scrub.sh: " kind[m] " MAC survived in " FILENAME ": " m > "/dev/stderr"; bad = 1 }
        # Default-deny: any other MAC left must be one we mean to keep. A new capture
        # format that carries MACs fails here until the collect rules learn it.
        else if (m != "" && !placeholder(m) && m != "ff:ff:ff:ff:ff:ff" && m != "00:00:00:00:00:00" && !multicast(m)) {
            print "scrub.sh: unexplained MAC in " FILENAME ": " m " (teach scrub.sh where it comes from)" > "/dev/stderr"; bad = 1
        }
        rest = substr(rest, RSTART + RLENGTH)
    }
    if (length(HOST) >= 3 && index($0, HOST)) { print "scrub.sh: hostname survived in " FILENAME > "/dev/stderr"; bad = 1 }
    next
}
END {
    if (mode == "rewrite") {
        printf "%d SSID(s) -> STAND-IN placeholders, %d own MAC(s), %d other MAC(s)/BSSID(s)", n_ssid, n_own, n_other
    }
    if (mode == "verify") exit bad
}
AWK

AWKV=(-v KEEP="$WORK/keep" -v HOST="$HOST" -v PERSONA="$PERSONA")
# collect+rewrite share one awk run so the maps built in pass 1 are used in pass 2.
SUMMARY="$(awk "${AWKV[@]}" -f "$WORK/scrub.awk" mode=collect "${FILES[@]}" mode=rewrite "${FILES[@]}")"

# verify needs the maps too: rebuild them from the originals, then read the scrubbed files.
SCRUBBED=(); for f in "${FILES[@]}"; do SCRUBBED+=("$f.scrubbed"); done
if ! awk "${AWKV[@]}" -f "$WORK/scrub.awk" mode=collect "${FILES[@]}" mode=verify "${SCRUBBED[@]}"; then
    rm -f "${SCRUBBED[@]}"
    echo "scrub.sh: verification failed; originals left untouched" >&2
    exit 1
fi
for f in "${FILES[@]}"; do mv "$f.scrubbed" "$f"; done

KEPT=""; [[ ${#KEEP[@]} -gt 0 ]] && KEPT="; kept SSID(s): $(IFS=,; echo "${KEEP[*]}")"
echo "$SUMMARY$KEPT"
