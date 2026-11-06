# 0002. Treat CI as the only source of truth

- Status: Accepted
- Date: 2026-11-06
- Deciders: the user

## Context

In the first version, work was verified on the developer's machine only. Measurements were skewed by load, power
saving and debug builds, merges broke the main branch twice, and slow tests were skipped without anyone seeing them
red. Documents drifted from the code, and agents trusted the documents. The user decided on 2026-11-06 that only CI
decides what is accepted; local runs are for development, and the only truth is what the code really does.

## Decision

Every change reaches `main` through a pull request whose CI is green. CI runs the commands listed in `AGENTS.md`.
`main` is protected: no direct push, CI required. A document is a plan until a test in CI shows it.

## Consequences

- Any check that matters must exist in CI; a check that only runs locally does not count.
- Signing (releases, feeds) will happen in CI, with secrets limited to the signing job and actions pinned by SHA.
- Performance gates measured on a real device cannot run on GitHub-hosted runners; what runs in CI instead is decided
  in its own ADR before the first gate.
