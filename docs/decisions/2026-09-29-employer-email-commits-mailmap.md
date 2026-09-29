---
template_version: 1.2.0
date: 2026-09-29
slug: employer-email-commits-mailmap
status: accepted
decided_by: hampton
related: [2026-09-24-scrub-personal-data]
---

# Decision: Map the employer-email commits with `.mailmap`, don't rewrite history

## Context

Nine commits carry the author email `hampton.maxwell@crunchyroll.com`, an employer
address, instead of the personal addresses the rest of the history uses:

| Commits | Date | Subject |
|---|---|---|
| `e4b010d` `7a07eea` `3ae53e9` `2cdd4b7` `ffd9d0d` `5625728` `ff1593b` | 2026-08-26 | The Rust rewrite becoming canonical, through the first fixture capture |
| `6b2a089` `abc0222` | 2026-09-29 | macOS build fix, `permissions` helper (#48) |

The question was where they were developed. The facts:

- **The machine.** Per the author, it is a MacBook Pro the author bought personally (the
  receipt is on file with the author, not committed). It was used for employer work, with the
  employer's go-ahead, until that work moved to a different laptop some months
  before these commits. Its global git config kept the work email from that time, so
  any repo without its own `user.email` committed under it.
- **2026-08-26 commits: this machine.** `ff1593b` committed the first fixture capture
  (`home-wifi-macos`), taken before captures were scrubbed. Its raw `ifconfig` and
  `networksetup -listallhardwareports` output matches this machine's hardware
  addresses exactly: every adapter, including the three Thunderbolt ports the other
  laptop does not have. Its `meta.toml` records the same OS (Darwin 25.4.0). The other
  six commits that day came before it, within 90 minutes, from the same email and
  time zone. They predate the current clone (made 2026-09-04), so they were made in
  an earlier checkout on the same machine.
- **2026-09-29 commits: this machine.** They appear in the current clone's reflog.

No commits from the older `hmaxwell@ellation.com` address exist in the repository.

## Options

1. **Rewrite history** (`git filter-repo` with a mailmap) and force-push. Every commit
   from `e4b010d` onward gets a new hash: 75 on `main` plus all 27 remote branches,
   and two open PRs (#86, #43) would need their branches force-pushed. It still would
   not remove the originals, which stay reachable through GitHub's PR refs and in
   every existing clone. A silently rewritten record is also harder to trust than
   one that explains itself.
2. **Leave it.** The history stays accurate, but `git log` and `git shortlog` keep
   showing the employer address with no explanation.
3. **`.mailmap` plus this record.** Git's standard mechanism: `git log`,
   `git shortlog`, and `git blame` show the personal address, while the commit
   objects keep the email they were made with, and this record explains why.

## Decision

Option 3. `.mailmap` maps `hampton.maxwell@crunchyroll.com` to `me@hamptonmaxwell.com`,
and no commits are rewritten. History stays intact and every hash stays valid; the
display is corrected and the reasoning sits next to it.

To stop new commits picking up the work email, the repo's own config now sets
`user.email` to the personal address.

## Consequences

- Commits keep the employer email in their objects. Tools that ignore `.mailmap`
  (GitHub's commit view among them) still show it; this record is the explanation.
- The global git config on this machine still has the work email, so another repo
  here without its own `user.email` will commit under it.
- The pre-scrub `home-wifi-macos` capture, which provides the hardware-address
  evidence, remains in history. The addresses themselves are not repeated here.
