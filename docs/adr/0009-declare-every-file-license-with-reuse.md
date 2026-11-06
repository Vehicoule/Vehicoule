# 0009. Declare every file's license with REUSE, checked in CI

- Status: Proposed
- Date: 2026-11-06
- Deciders: the user

## Context

The user agreed on 2026-11-06 to check that every file has its license declared. The app is `GPL-3.0-only`
([ADR 0006](0006-license.md)); vendored files keep their own license (`reference/README.md`,
`.claude/skills/PROVENANCE.md`): MIT for the two skills, CC BY 3.0 for Google's style guide. Today nothing checks that
a new file, vendored or not, says under which license it is.

- [REUSE](https://reuse.software/spec-3.3/), from the Free Software Foundation Europe, is a specification for
  declaring each file's copyright and license, either in a header in the file or in a `REUSE.toml` file at the root
  that covers paths. Its tool, `reuse` 6.3.0 (PyPI, GPL-3.0-or-later), fails `reuse lint` when a file has neither.
- Vendored files are never edited by hand (AGENTS.md), so they cannot take a header.
- Other pull requests are changing the plugin host's files now; a header added to every file would conflict with them.

## Options

1. **A header in each file we write, `REUSE.toml` for vendored files.** Licensing visible in every file. Every new
   file needs the header; the first pull request touches every file.
2. **`REUSE.toml` alone, with explicit paths.** Our own paths are listed as `GPL-3.0-only`, each vendored path with its
   license. No file changes; a file under a path nobody listed (a new top-level directory, a new vendored copy) fails
   the check, which forces its license to be decided.
3. **No check.** Licensing stays in the ADRs and provenance files, unchecked.

## Decision

Recommended: option 2. The copyright text of our own files is "The Vehicoule contributors", whose authors are recorded
by Git; the user confirms this wording.

## Consequences

- `REUSE.toml` declares every path; `LICENSES/` holds the text of each license used, `LICENSES/GPL-3.0-only.txt` being
  a copy of `LICENSE`, which stays for GitHub's license detection.
- The `REUSE` workflow runs `reuse lint` 6.3.0 on every pull request. A vendored file added without its own entry in
  `REUSE.toml` fails it.
- A new top-level file or directory needs a line in `REUSE.toml`.
- A file under a path declared `GPL-3.0-only` but copied from elsewhere would be declared wrongly; vendored copies go
  under `reference/` or `.claude/skills/` with their entry, as the rules already say.
