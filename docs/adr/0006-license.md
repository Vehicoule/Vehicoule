# 0006. License the app under GPL-3.0

- Status: Accepted
- Date: 2026-11-06
- Deciders: the user

## Context

The user chose the GNU General Public License version 3 for the app on 2026-11-06. The component library
`nsh-m3e` is MPL-2.0 (its ADR 0007), which GPL-3.0 code may use.

## Options

1. `GPL-3.0-only`: exactly the version chosen.
2. `GPL-3.0-or-later`: also lets anyone use a later GPL version the user has not read.

## Decision

GPL-3.0, declared as `GPL-3.0-only`, the version the user named.

## Consequences

- `LICENSE` holds the GPL-3.0 text; `Cargo.toml` declares `license = "GPL-3.0-only"`.
- Every dependency must be GPL-3.0 compatible; `deny.toml` lists the allowed licenses.
- The Free Software Foundation holds that Apple's App Store terms conflict with GPL-3.0; iOS distribution is decided in its own ADR before an iOS release.
- Vendored files keep their own licenses, listed in their provenance files.
