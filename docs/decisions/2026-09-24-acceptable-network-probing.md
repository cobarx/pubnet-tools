---
template_version: 1.2.0
date: 2026-09-24
slug: acceptable-network-probing
status: accepted
decided_by: hampton
related: [2026-09-24-scrub-personal-data, 2026-08-02-open-source-only, 2026-08-02-passive-topology]
---

# Decision: What probing a network is acceptable, especially for content filtering

## Context

Finding out what a network allows means sending traffic it may not like. On the Amtrak
train (2026-09-24) the agent tested content filtering by looking up real adult, piracy and
imageboard domains, fetching the sinkhole's block page for them, and sending `HEAD`
requests to two piracy sites to show the filtering was DNS-only. It worked, and it was the
wrong way to do it: a lookup is not a visit, but a filter logs category hits against the
device that asked, and in a log review "looked up an adult site" reads the same as
visiting one. For a tool other people run, including on work and school networks, that
lands on them. Network terms of use also often cover attempting prohibited categories, not
just reaching them.

This is a first pass, set before `pubnetchk` gains any filtering or egress check (#53, #57)
and before the `network-capture` skill's filtering rule is written.

## Decision

The criteria: probing **never harms the person running it**, stays **within the network's
and each service's terms**, would not look to an operator like **abuse or content
access**, and is still informative enough to be worth it.

| Tier | What | Acceptable |
|---|---|---|
| Mainstream legal categories | Streaming, social, VPN vendors' own sites, news | Yes: DNS lookups and TLS handshakes that send no request |
| Designated test domains | Domains filter vendors publish so admins can test category blocking | Yes; that is what they are for |
| Real adult and gambling sites | Anything that puts a real site in those categories in the user's logs | **Opt-in only**: an explicit flag the user chooses, never a default, and only after reading the network's terms of use |
| Real piracy and torrent-index sites | As above | **Never provided**, not even as an opt-in. Whether to test them is the user's own decision, made and acted on outside these tools |
| Passive detection | Recognising a sinkhole or block page when the user's own traffic meets one | Yes; it sends nothing extra |

The same bar applies to manual reconnaissance (the `network-capture` skill) as to the
tools. For every tier: check the terms of the network and of any testing service before
relying on it, and send the least that answers the question (a handshake before a request,
a `HEAD` before a `GET`, one connection rather than a sweep).

## Rationale

- **The person running the tool is the one exposed.** Their device, their session, their
  employer's logs. A default that could embarrass or discipline the user is a defect, however
  informative it is.
- **Real sites are stand-ins for categories; designated test domains are better stand-ins.**
  They exist so this test can be run without anyone visiting the real thing. Their limit:
  each is usually categorised only by its own vendor's filter, so a hit is strong evidence
  and a miss is weak. Findings must say which.
- **Opt-in, not banned, for adult and gambling.** Some users need to know (an operator
  auditing their own network, a researcher); the choice and its consequences are theirs,
  made knowingly, after reading the terms.
- **Piracy is not the project's to enable.** Lawful adult and gambling sites are one
  thing; the project shipping a switch that reaches piracy sites is another. A user who
  wants to know makes that personal decision and acts on it themselves.
- **Lookups detect only DNS filtering.** Address blocking, SNI inspection and HTTP
  interception pass the lookup and stop the visit, so a lookup-only test would report
  "not blocked" for them. A handshake that sends no request catches the address and SNI
  layers without fetching content, which is why it is in the acceptable tier.

## Stakeholders

The project owner, prompted by the agent's own probing on the train. Users of the tools are
the stakeholders the criteria protect.

## Considerations / Revisit if

- **Designated test domains are not yet verified.** Today: none are named here. Revisit
  when a check adopts them: each name, its vendor and its category are verified from the
  vendor's documentation, not recalled.
- **Terms of use can't be read by a tool.** Today: the user reads them; the opt-in flag's
  help says so. Revisit if a portal's terms become machine-readable.

## Consequences

- The `network-capture` skill's filtering rule (#68) follows this tiering.
- #53 (DNS-sinkhole detection) and #57 (egress check) are designed within it: default
  behaviour uses the first two tiers and passive detection only.
- The train capture's notes (`amtrak-train-linux/README.md`) name the real domains that
  were tested before this decision existed; they are revised to say what was learned
  without listing them.
