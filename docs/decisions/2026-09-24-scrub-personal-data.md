---
template_version: 1.2.0
date: 2026-09-24
slug: scrub-personal-data
status: accepted
decided_by: hampton
related: [2026-08-26-macos-wifi-without-airport, "#3"]
---

# Decision: Scrub personal data everywhere, with obviously fake stand-ins

## Context

This project's raw material is other people's networks. A capture on a train picks up
every hotspot in range (one was named after its owner, "<first name>'s iPhone"), the
laptop's own MAC, and the MACs of devices sharing the LAN. Prose picks it up too:
`network-behavior.md` named a friend's apartment and quoted a phone hotspot's BSSID.
All of it gets committed to a public repo.

Until now scrubbing was by hand and at commit time. Re-running the new scrubber over
the one existing capture (`home-wifi-macos`) showed how that goes: its notes said the
card MAC had been replaced, but it had not, and nine hardware-port MACs and four
household-device MACs had never been touched.

## Decision

Personal data is scrubbed from anything that leaves the machine: fixtures, docs,
decision records, issues, PR text, test comments, and reports quoted into any of them.
Stand-ins are **obviously fake**, never plausible fakes and never a bare `<redacted>`.
Two kinds:

- **Default placeholders** for data that carries no meaning of its own, such as the
  neighbouring SSIDs in a scan: `STAND-IN SSID NN`, written by the scrub. They stay.
- **A name the dev chooses** (not the tooling or the agent) where the hidden thing is
  meaningful to the reader: a person in a doc, or a network that is the subject of the
  capture. Whatever interests the dev: humorous, personal, or commentary (on a hot day
  in Sacramento, a replaced network might become "Sac Is Hot").

| Personal data | Stand-in |
|---|---|
| People's names | A plain description ("a friend's apartment"); a name the dev chooses if the person matters to the reader |
| SSIDs (every one, unless an operator's public name) | `STAND-IN SSID NN`; a name the dev chooses if the SSID is meaningful |
| This machine's MACs | `02:00:00:00:00:NN` (locally administered, clearly fake) |
| Other devices' MACs (the gateway too), BSSIDs | Vendor OUI kept, rest `00:00:NN` |
| Hostname | `standin-host` |
| Home / residential public IPs | Documentation ranges (`192.0.2.0/24`, `203.0.113.0/24`) |
| A site's IPv6 prefix (global or ULA) | Documentation prefix, `2001:db8:ffff:ffNN::/64` per original /64 |
| EUI-64 IPv6 interface IDs (a MAC in disguise) | Rebuilt from that MAC's stand-in |

Not personal, kept as-is: a public operator's SSID (`YourTrainWiFi`), operator and
public-service IPs, RFC 1918 addresses, the author's own name on authorship metadata.
The bar is issue #3's: anything not publicly available goes. A gateway's MAC fails it
(a router's MAC usually sits next to its BSSID, which war-driving databases geolocate),
so it is scrubbed like any other device.

For fixtures this is mechanical: `capture.sh` stages the capture, runs
`crates/pubnetchk/tests/fixtures/scrub.sh`, and moves nothing into the fixture tree
unless the scrub verifies. Values to replace are derived from the capture itself (the
interface's own `ether` line, the ARP table), not typed in. Verification is
default-deny: any MAC left that is not a stand-in or broadcast/multicast fails the
capture, so a new command's output fails loudly instead of leaking. The same holds for
IPv6: an address in the site's /48 or an EUI-64 address whose MAC is not a stand-in
fails it.

## Rationale

- **Where to scrub: at capture, not commit.** A commit-time step is the one that gets
  skipped once, and the `home-wifi-macos` record shows a hand scrub is also the one
  that is quietly incomplete. Scrubbing inside the capture script means an unscrubbed
  capture never exists on disk in the repo.
- **Why obvious fakes, not `<redacted>` or hashes.** macOS itself emits the literal string
  `<redacted>` for SSIDs without a Location Services grant, and the parsers treat it
  as "SSID unknown". Using the same token for our scrub would make fixtures lie about
  what the OS did. Hashes and realistic fakes read as real data; `STAND-IN SSID 03`
  cannot be mistaken for a real network. Where a name is worth giving, the dev gives
  it, and it should read as theirs rather than as data.
- **Why keep OUIs.** Vendor identity is the useful part for analysis (Extreme Networks
  APs, a duagon rail box); the low three octets are the part that identifies a device
  or, for BSSIDs, locates it via public war-driving databases.
- **Why every SSID, not just personal-looking ones.** Deciding which SSIDs contain a
  name is a judgement a script gets wrong. Replace them all; spare the operator's
  public name explicitly with `--keep-ssid`.

## Stakeholders

Solo call; requested by the project owner.

## Considerations / Revisit if

- **Chosen names are the dev's to write, and only where they mean something.** A
  first draft shipped agent-written joke lists in `scrub.sh`; the owner called that out
  as taking the fun out of it, and noted that neighbouring SSIDs don't matter enough to
  name. The scrub writes plain placeholders (the general principle is
  cobarx/build-skills-cobarx#45).
- **Git history still holds the old values.** Today: scrubbing is forward-only; the
  earlier unscrubbed `home-wifi-macos` files and the `network-behavior.md` wording are
  in history. Revisit if: any of it is judged sensitive enough to warrant a history
  rewrite and force-push (a separate, deliberate decision).
- **Free-text notes are not scrubbed.** `capture.sh`'s notes prompt says "no names",
  but nothing enforces it. Revisit if: a name slips into a `meta.toml`.
- **Default-deny covers MACs only.** SSIDs and hostnames are caught by derived rules,
  not by a catch-all. Revisit if: a new capture format carries SSIDs somewhere
  `scrub.sh` does not parse.

## Consequences

- New `crates/pubnetchk/tests/fixtures/scrub.sh`; `capture.sh` takes `--keep-ssid`,
  writes a `scrubbed = "..."` summary into `meta.toml`, and fails without writing if
  the scrub does not verify.
- `home-wifi-macos` was re-scrubbed in place (14 MACs, the gateway's included); its tests assert on IPs,
  gateway presence, and multicast filtering, none of which changed.
- `network-behavior.md` and `crates/pubnetdiag/tests/repair_flow.rs` comments had a
  friend's name replaced with "a friend's" and a phone BSSID scrubbed under this policy.
- Amended 2026-09-24: IPv6 added after a residential capture kept the house's global
  prefix and the router's EUI-64 link-local address (its MAC). `home-wifi-macos` was
  re-scrubbed for its ULA prefix; no test reads that address.
