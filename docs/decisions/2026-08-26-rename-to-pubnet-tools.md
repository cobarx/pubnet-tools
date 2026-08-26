---
template_version: 1.2.0
date: 2026-08-26
slug: rename-to-pubnet-tools
status: accepted
decided_by: hampton
related: [2026-08-26-rust-becomes-canonical-implementation]
---

# Decision: Project renamed `conncheck` → `pubnetchk` (crate: `pubnet-tools`)

## Context

While preparing the repo for an eventual Hacker News post, the name "conncheck" came
under scrutiny: it doesn't say what actually makes this tool useful (auditing a
*public* network you don't control, not just any connection). A few names were
considered and rejected in conversation before landing here: `hscheck`/`hsstat`/
`hotspotstat` (rejected - "hotspot" technically means phone-tethered/dedicated-AP WiFi,
narrower than what the tool actually audits: any public network you just joined - hotel,
cafe, coworking space), and `netvet`/`netjoin`/`trustnet` (proposed, rejected without
much elaboration - "not happy with any of these").

`pubnetstat` was then proposed and liked specifically because "pub" (public) is
accurate without overclaiming, and it fits the real `*stat` Unix tool family
(`netstat`, `vmstat`, `iostat`). From there, the project was reframed as a planned
*suite* of three binaries under one prefix: `pubnetchk` (this tool, today), `pubnetstat`
(a future `vmstat`-style watch mode for diagnosing mid-session slowdowns - not yet
built, see project memory), and `pubnettop` (a possible future `top`-style live
dashboard - also not yet built).

## Decision

- Crate/package name: `pubnet-tools` (hyphenated - matches real precedent, `net-tools`
  is the long-standing Linux package shipping `ifconfig`/`netstat`/`route`; puts
  `pubnet-tools` in the tradition of `iproute2`/`procps-ng` as an umbrella name for a
  suite of individually-named binaries).
- Binary name: `pubnetchk` (unhyphenated, like the other planned binaries -
  `pubnetstat`/`pubnettop` - brevity matters more for something typed constantly than
  for a repo/crate name typed once). `conncheck` used a `chk` abbreviation
  inconsistently against `stat`/`top` both being full words; kept anyway, since it's
  what was actually used and confirmed in conversation, and the asymmetry is minor.
- Storage paths: `~/.conncheck/{reports,recordings}` → `~/.pubnetchk/{reports,recordings}`.

Availability was checked before renaming (2026-08-25): `pubnetchk`, `pubnetcheck`,
`pubnetstat`, `pubnettop`, `pubnettools`, and `pubnet-tools` were all confirmed
unregistered on crates.io and npm, with no colliding GitHub repositories.

## Rationale

This isn't cosmetic - it's the same discipline already applied to in-tool wording (see
[2026-08-25 DNS-leak terminal wording change](../../src/output/renderer.rs), where "DNS
leak" was reworded because it's VPN-testing jargon that doesn't match what the check
actually verifies). The project consistently prioritizes names that are provably
accurate to what something does over names that are merely evocative or familiar. A
project's own name is the highest-leverage instance of that same standard - it's the
first thing anyone evaluates before reading a line of code or documentation.

## Stakeholders

Solo call - no other stakeholders consulted.

## Considerations / Revisit if

- **`pubnetstat` and `pubnettop` don't exist yet.** Today: `pubnet-tools` names a suite
  of one. Revisit if: those two never actually get built - at that point `pubnet-tools`
  as an umbrella name for a single binary is slightly odd, though not wrong (it still
  correctly describes the domain).
- **`chk` vs. full-word consistency was raised and left unresolved.** Today: `pubnetchk`
  (not `pubnetcheck`) per the user's own consistent usage throughout the conversation
  that produced this decision. Revisit if: the asymmetry against `pubnetstat`/`pubnettop`
  ever becomes a real point of confusion in practice - `pubnetcheck` remains the more
  internally consistent alternative if so.

## Consequences

- Breaking change for anyone who'd built against `conncheck`: crate name, binary name,
  and the `~/.conncheck/` storage path all changed. No one outside this project had
  adopted it yet, so the practical impact is zero.
- `CLAUDE.md` and `README.md` (living documents, describing current state) were updated
  to the new name throughout.
- `docs/decisions/` entries written before this rename were **not** retroactively
  edited - they're a historical record of what was decided when the project actually
  was named `conncheck`, and rewriting them would misrepresent that history. This entry
  and [2026-08-26-rust-becomes-canonical-implementation.md](2026-08-26-rust-becomes-canonical-implementation.md)
  are the pointer for a future reader who encounters "conncheck" in an older decision
  doc and wants to know why the name is now different.
- `docs/context/` (technical reference notes, not decision history) was left unchanged
  in this pass - lower priority, tracked as a remaining item rather than done here.
