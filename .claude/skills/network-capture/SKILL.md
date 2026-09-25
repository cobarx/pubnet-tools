---
name: network-capture
description: This skill should be used when capturing, probing, or writing up a real network you are on (a train, bus, hotel, cafe, airport, or any public Wi-Fi), including running capture.sh, testing what traffic a network allows, and writing the README.md beside a capture. It governs how a network is recorded, not how pubnetchk's checks work.
---

# network-capture

A captured network is a dated, scrubbed snapshot of what one network did, gathered without
abusing it.

What a network allowed was true of one vehicle or venue, on one day. The next bus on the same
route may run other equipment, and operators change portals, filters, and caps without notice.

## Rules

1. **Record a snapshot, never a policy.** Date, place on the route, and the specific unit where
   known (train number, car, bus). Write every finding as "was, there, then", never as what "the
   network" does.

2. **Record the sign-in state with every capture.** Before and after a portal are different
   networks. Capture before signing in whenever you can, since that state is gone once you accept,
   and say which state each capture is in its `meta.toml`.

3. **Never leave the working connection without a way back.** Switching Wi-Fi drops everything
   riding on it, an agent's session included. Let the dev switch, or use a tool that returns on
   its own with a watchdog.

4. **Passive before active, and the project's tools before hand probes.** Run `pubnetchk --json`
   and `capture.sh` first; they are repeatable and scrubbed. Hand probes fill gaps and are
   written down so a later capture can repeat them.

5. **Probe politely, through services built for testing.** Port reachability via portquiz.net
   (a short list, one connection each, never a sweep); UDP and NAT via public STUN; DNS via
   public resolvers and DoH; ICMP to `1.1.1.1` and `8.8.8.8`. Respect each service's terms, and
   never generate load a network or service would call abuse.

6. **Test filtering only as far as the acceptable-probing decision allows.** Mainstream sites
   and designated test domains: compare the network's DNS answer with DoH, then complete a TLS
   handshake that sends no request, since a lookup alone sees only DNS filtering. Real adult or
   gambling sites only if the dev opts in after reading the network's terms; piracy sites
   never. Fetch only a block page the network itself serves.

7. **Label inferences with their evidence.** Vendor from a MAC OUI, backhaul from a DHCP
   search domain or an egress range: say "likely", and say why. A guess stated as fact becomes
   something a later capture "contradicts".

8. **Scrub before anything leaves the machine.** `capture.sh` scrubs fixtures; notes, issues,
   and PR text are scrubbed by hand to the same bar (see the scrub-personal-data decision).
   Placeholders by default; a chosen name only where the hidden thing matters, and the dev
   chooses it.

9. **Notes go in a `README.md` beside the capture.** Not in `docs/context/`; there will be many
   networks. Link the runbook for any fix, rather than repeating it.

10. **Record where to look things up, not what they said.** Schedules, menus, and live status
    change; the note keeps the source (an API, a page, an on-board feed) and how to query it.

11. **Every problem found is an issue before you move on.** A check that misread the network, a
    gap in the tools, a fixture the tests can't use. Severity and fix are discussed there.

## Not here

Where fixture data comes from and what it records, in general, is `fixtures`. How a check
decides what it reports is `docs/specs/`. Fixing a network problem on the dev's machine is a
runbook in `docs/runbooks/`.
