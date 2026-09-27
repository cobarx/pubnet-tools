---
template_version: 1.4.0
date: 2026-09-24
slug: output-locations
status: accepted
decided_by: hampton
related: [2026-08-25-save-off-by-default, 2026-08-26-html-report-output]
---

# Decision: Files pubnet-tools writes go where each platform says, by who reads them

## Context

Everything `pubnetchk` writes lands in `~/.pubnetchk/`: `--save` JSON reports,
`--html` reports, and `record` sessions. The location was inherited from the
TypeScript tool (`~/.conncheck/`) and never weighed. The two diagnostic scripts in
`scripts/` put their logs and transcripts in `$XDG_CACHE_HOME/pubnet-tools/`.

Two things are wrong with that. A dotfile directory in `$HOME` is the pattern the
platform specifications exist to replace. And it hides files a person is meant to
open: an HTML report and a terminal recording are documents, not program state.
Cache has the opposite problem: it is for files that are safe to delete, and a log or
a saved report is not.

The criterion is **who reads the file**. The program (or a later tool in the suite)
reads state back; a person opens a document.

## Decision

| Output | Written to | Reader |
|---|---|---|
| `--save` JSON reports | The platform's per-user data directory, in `pubnet-tools/reports/` (below) | Kept records; the natural input for `pubnetstat` |
| `--html` report | The current directory | A person, in a browser |
| `record` sessions (`.cast`) | The current directory | A person, via `asciinema play` or upload |
| Script logs and transcripts (`scripts/*.sh`, Linux-only) | `$XDG_STATE_HOME/pubnet-tools/` (default `~/.local/state`) | A person looking back, rarely |

The per-user data directory, by platform:

| Platform | Directory | Source |
|---|---|---|
| Linux | `$XDG_DATA_HOME/pubnet-tools` if set and absolute, else `~/.local/share/pubnet-tools` | [XDG Base Directory spec 0.8](https://specifications.freedesktop.org/basedir/latest/) |
| macOS | `~/Library/Application Support/com.cobarx.pubnet-tools` | [Apple, File System Programming Guide: macOS Library directory](https://developer.apple.com/library/archive/documentation/FileManagement/Conceptual/FileSystemProgrammingGuide/MacOSXDirectories/MacOSXDirectories.html) |
| Windows | `%LOCALAPPDATA%\pubnet-tools` (default `%USERPROFILE%\AppData\Local`) | [Microsoft, KNOWNFOLDERID: `FOLDERID_LocalAppData`](https://learn.microsoft.com/en-us/windows/win32/shell/knownfolderid) |

Every write prints its full path, as `--save` and `--html` already do. Existing files
in `~/.pubnetchk/` are left where they are: nothing reads them back, so there is
nothing to migrate for.

## Rationale

- **The specifications sort by reader, and so does this.** XDG gives
  `$XDG_DATA_HOME` for user data files, `$XDG_STATE_HOME` for "actions history (logs,
  history, recently used files, …)", and `$XDG_CACHE_HOME` for "non-essential data
  files". Apple's Application Support "contains all app-specific data and support
  files", in a subdirectory named by bundle identifier. Windows' Local AppData is the
  per-user, per-machine folder; a report on the network this machine joined belongs
  to the machine, so it does not roam.
- **Documents go where the person is.** Tools that produce a page to open write it to
  the current directory: `coverage html` writes `./htmlcov` ([docs](https://coverage.readthedocs.io/en/latest/commands/cmd_html.html)),
  and `asciinema rec` takes the output file as an argument. `--open` still opens the
  report directly, so where it lands only matters to someone looking for it later,
  and the current directory is where they will look.
- **Hidden is right for state, and state stays hidden.** On Linux, the JSON reports
  and the logs still sit under a dot directory (`~/.local/…`). That's by design in the
  spec: nobody browses them, and the printed path is how a person finds one.
- **XDG requires absolute paths.** "If an implementation encounters a relative path in
  any of these variables it should consider the path invalid and ignore it." A relative
  `$XDG_DATA_HOME` falls back to the default.

## Alternatives considered

- **Keep `~/.pubnetchk/`.** Rejected: it hides documents and ignores all three
  platforms' conventions. Its only merit is that it already exists.
- **XDG paths on macOS too.** Many cross-platform CLIs do this. Rejected: Apple's
  guidance names Application Support, and `pubnetchk` already follows each platform's
  own mechanisms (`system_profiler`, the Win32 API) rather than one platform's
  everywhere.
- **The `dirs` crate.** It maps these directories for all three platforms. Rejected
  for now: the lookup is three environment variables and a fallback each, which the
  reporter already does by hand for `$HOME`. `dirs` would add `dirs-sys` and
  `option-ext` (MPL-2.0), and its `data_dir()` is Roaming AppData on Windows, which is
  the wrong one here.
- **Everything in the current directory, `--save` included.** Rejected: JSON reports
  are a history, meant to accumulate in one place for a later tool to read, not to be
  scattered across wherever the command was run.
- **HTML in `xdg-user-dir DOCUMENTS`.** Rejected: that folder is Linux-desktop-only
  (and macOS/Windows have their own), and a CLI writing into a person's Documents
  unasked is more surprising than writing to the directory they ran it from.

## Stakeholders

Solo call; requested by the project owner.

## Considerations / Revisit if

- **Someone wants the HTML or recording elsewhere.** An output-directory flag is the
  usual answer; not added until asked for.
- **`pubnetstat` is built.** It reads the data directory above; if it needs an index or
  other state of its own, that goes in state (`$XDG_STATE_HOME` on Linux), not beside
  the reports.
- **Windows' Local AppData is relocated** (roaming profiles, folder redirection) and
  `%LOCALAPPDATA%` disagrees with `SHGetKnownFolderPath`. Switch to the API, which
  `windows-sys` already exposes.
- **The current directory isn't writable** (run from `/`). The write fails and prints
  why, like any failed save; revisit if that turns out to be common.

## Consequences

- Supersedes the **location** in
  [save-off-by-default](2026-08-25-save-off-by-default.md) and
  [html-report-output](2026-08-26-html-report-output.md); both decisions otherwise
  stand, and each gets a forward link.
- `--save` moves to the per-user data directory; `--html` and `record` write to the
  current directory. Separate PRs, each updating `--help`, the README, and CLAUDE.md.
- The scripts' logs move from `$XDG_CACHE_HOME` to `$XDG_STATE_HOME`.
