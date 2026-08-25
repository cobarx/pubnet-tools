# Decision: Passive Topology Only

**Date:** 2026-08-02
**Status:** accepted

## Context

conncheck audits public networks. Public WiFi environments include other people's devices, business infrastructure, and possibly hostile actors. Active network scanning (nmap-style) on a network you just joined is legally and ethically fraught.

## Decision

Topology discovery uses only passive OS reads — no packets generated for discovery purposes:

- `ip route show default` — gateway and interface
- `ip addr show <iface>` — own IP/CIDR
- `ip neigh show dev <iface>` — ARP cache (populated by the OS as a byproduct of normal traffic)

No port scanning. No ping sweeps. No ARP requests beyond what the OS already sent.

## Rationale

- The ARP cache reflects devices the OS has naturally communicated with — gateway, DHCP server, mDNS neighbors. This is enough to understand the network topology without probing.
- Active scanning on a network you don't own can trigger IDS alerts, violate terms of service, and in some jurisdictions constitutes unauthorized computer access.
- "Good citizen" is a design value. conncheck runs at join time on every network — the bar for what it does passively must be defensible everywhere.

## Consequences

- Topology data will be incomplete on quiet networks (ARP cache may only show the gateway).
- `iw scan` is excluded even though it would show nearby SSIDs — it requires root and transmits probe frames.
- Every topology output includes a `passiveNotice` field: `"Passive ARP cache — no active scan performed."` This appears in both terminal output and JSON reports, making the constraint explicit and auditable. **Reopened 2026-08-25 (proposed, not settled):** dropped from the terminal view specifically — it had nothing to refer to there (the terminal never rendered the neighbor list). Still present in every JSON report. See [passive-notice-terminal-only-in-json](2026-08-25-passive-notice-terminal-only-in-json.md).
- nmap and related tools are not dependencies and will not be added.
