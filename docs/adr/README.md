# Architecture Decision Records

One decision per file, numbered, never renumbered. A decision is changed by a new ADR that supersedes it, never by
editing the old one beyond its status line.

Status: **Proposed** (written, waiting for the user), **Accepted** (the user decided), **Superseded by NNNN**.
Only the user moves an ADR from Proposed to Accepted.

| ADR | Decision | Status |
|---|---|---|
| [0001](0001-rust-only.md) | The app in Rust, no TSX | Accepted |
| [0002](0002-ci-is-the-source-of-truth.md) | CI is the only source of truth | Accepted |
| [0003](0003-core-architecture.md) | Core as synchronous state machines, one process, I/O at the edges | Accepted |
| [0004](0004-verification.md) | Types, Kani, property tests and mutation testing | Accepted |
| [0005](0005-pinned-versions-and-dependencies.md) | Pinned versions and the dependency policy | Accepted |
| [0006](0006-license.md) | The app is licensed under GPL-3.0 | Accepted |
| [0007](0007-plugin-host.md) | Plugins as Wasm components in Wasmtime, with no WASI | Accepted |
| [0008](0008-app-text-in-translation-files.md) | Every text the app shows in translation files, from the start | Accepted |
| [0009](0009-declare-every-file-license-with-reuse.md) | Every file's license declared with REUSE, checked in CI | Accepted |
| [0010](0010-generated-changelog-and-signed-releases.md) | A changelog generated from commits, and signed releases | Accepted |
| [0011](0011-adopt-the-contributor-covenant.md) | The Contributor Covenant 3.0 is the code of conduct, under its own CC BY-SA 4.0 license | Accepted |
| [0012](0012-one-regression-test-per-fixed-bug.md) | Every fixed bug gets a regression test named after its issue | Accepted |
| [0013](0013-fuzz-the-plugin-hosts-untrusted-inputs.md) | The plugin host's untrusted inputs are fuzzed with cargo-fuzz and libFuzzer | Accepted |
| [0014](0014-prove-the-plugin-host-checks-with-kani.md) | The plugin host's sandbox checks are proved with Kani, written as byte loops | Accepted |
| [0015](0015-fuzz-the-cores-state-machines.md) | The core's state machines are fuzzed with sequences of operations | Accepted |
| [0016](0016-count-the-cores-instructions-with-callgrind.md) | The core's operations are benchmarked by instruction count, each under a ceiling | Accepted |
| [0017](0017-no-telemetry-crash-reports-on-request.md) | Nothing is sent to the project, except a crash report the user chooses to send | Accepted |
