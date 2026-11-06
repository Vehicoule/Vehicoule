# Architecture Decision Records

One decision per file, numbered, never renumbered. A decision is changed by a new ADR that supersedes it, never by
editing the old one beyond its status line.

Status: **Proposed** (written, waiting for the user), **Accepted** (the user decided), **Superseded by NNNN**.
Only the user moves an ADR from Proposed to Accepted.

| ADR | Decision | Status |
|---|---|---|
| [0001](0001-rust-only.md) | The app in Rust, no TSX | Accepted |
| [0002](0002-ci-is-the-source-of-truth.md) | CI is the only source of truth | Accepted |
| [0003](0003-core-architecture.md) | Core as synchronous state machines, one process, I/O at the edges | Proposed |
| [0004](0004-verification.md) | Types, Kani, property tests and mutation testing | Accepted |
| [0005](0005-pinned-versions-and-dependencies.md) | Pinned versions and the dependency policy | Accepted |
| [0006](0006-license.md) | The app is licensed under GPL-3.0 | Accepted |
