# 0011. Adopt the Contributor Covenant 3.0, under its own CC BY-SA 4.0 license

- Status: Accepted
- Date: 2026-11-07
- Deciders: the user

## Context

The user agreed on 2026-11-06 to give the project a code of conduct, and on 2026-11-07 gave its reporting address,
vehicoule@gmail.com. The app stays `GPL-3.0-only` ([ADR 0006](0006-license.md)); every file declares its license in
`REUSE.toml` ([ADR 0009](0009-declare-every-file-license-with-reuse.md)), so the code of conduct needs one too.

- The [Contributor Covenant 3.0](https://www.contributor-covenant.org/version/3/0/) is a code of conduct many open
  source projects use. Its text is licensed CC BY-SA 4.0 by the Organization for Ethical Source, as its own
  Attribution section says; adopters fill in their reporting address.
- A text of our own would take our license, but would need writing, reviewing and maintaining, with no gain for
  contributors who already know the Covenant.

## Options

1. **The Contributor Covenant 3.0, declared CC BY-SA 4.0.** A known text; one more license in `LICENSES/`, which covers
   only `CODE_OF_CONDUCT.md`. The app's license does not change.
2. **A code of conduct of our own, `GPL-3.0-only`.** No new license; a text nobody knows, to write and maintain.
3. **No code of conduct.** Contributors do not know what is expected or where to report.

## Decision

Option 1. `CODE_OF_CONDUCT.md` is the Covenant 3.0 with the reporting address filled in and the note to adopters
removed; `REUSE.toml` declares it `CC-BY-SA-4.0`, copyright the Organization for Ethical Source.

## Consequences

- `LICENSES/CC-BY-SA-4.0.txt` holds the license text; it applies to `CODE_OF_CONDUCT.md` alone.
- Changes to the Covenant's text stay under CC BY-SA 4.0. Security vulnerabilities are reported through
  `SECURITY.md`, not the code of conduct's address.
