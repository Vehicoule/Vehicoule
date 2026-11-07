# AGENTS.md

Vehicoule is a music, video and books app for mobile, desktop and the web, written in Rust on
[NSH](https://github.com/longbridge/nsh) (Native Shell). It ships no content source: content comes from WebAssembly
plugins installed from signed feeds. Music comes first, end to end, on Android and macOS; video, then books follow.
The look comes from [`nsh-m3e`](https://github.com/Vehicoule/nsh-m3e), our Material 3 Expressive library.

This file is the rule book. Every agent, human or AI, reads it before touching the repository, and follows it over
any habit, skill or training data. When a rule here conflicts with another document, this file wins; when it
conflicts with what the user says in the task, the user wins and the rule gets an ADR.

## What is true

The only truth is what the code does, shown by tests that run in CI. Not a document, not a plan, not a local run.

The first version of the app was written by agents that invented APIs, skipped tests and loosened checks to get green.
Its code is gone on purpose. Its notes survive as a list of ideas and lessons, often wrong or contradictory: never
copy a value, a design or an API from them, and never cite them as a source. A requirement enters this repository by
being written into a spec or an ADR that the user accepts, and is true once a test in CI shows it.

## Read first, in this order

1. This file.
2. The decisions: [`docs/adr/`](docs/adr/). They say what is settled and why. Do not reopen a decision without a new
   ADR that the user accepts.
3. The spec of the part you touch, in [`docs/`](docs/), once it exists.
4. For Rust style: the `rust-best-practices` and `rust-coding-guidelines` skills in [`.claude/skills/`](.claude/skills/),
   and [`reference/rust/google-rust-style-guide.md`](reference/rust/google-rust-style-guide.md).
5. For NSH, once a crate depends on it: its skills, vendored at the pinned commit, and its source at that commit.

[`docs/references.md`](docs/references.md) lists every source with its pinned version.

## Non-negotiable rules

1. **CI is the only truth.** Nothing reaches `main` without a pull request whose CI is green. A local run is a
   convenience, never evidence. Never push to `main`.
2. **Never weaken a check to get green.** No lowered lint level, no `#[allow]` without an ADR, no skipped or ignored
   test, no Kani proof removed or bounded lower, no raised performance ceiling, no reduced scope. If a check is wrong,
   say so and stop: changing a check is the user's decision.
3. **Never invent an API or a value.** Look a crate's or NSH's name up in its source at the pinned version before
   using it. If something cannot be found, write "not found" and stop. Do not write it by analogy with another
   framework, and do not take it from the first version's notes.
4. **The core does no I/O and draws nothing.** `vehicoule-core` and every crate under it are synchronous state machines:
   events in, commands and snapshots out. Network, disk, clock, platform players and UI stay at the edges
   ([ADR 0003](docs/adr/0003-core-architecture.md)). No thread, no async runtime, no `std::time::Instant::now()` in the
   core: time comes in as an event.
5. **Rust only.** No TSX, no QuickJS, no JavaScript ([ADR 0001](docs/adr/0001-rust-only.md)).
6. **One owner per state.** A piece of state lives in exactly one place, and nothing keeps a copy that can drift. No
   safety net stacked on another: a bug is fixed at its cause, never hidden by a retry, a timeout or a reload.
7. **A test proves what must happen and what must not.** A test that only confirms the intended action proves little.
   Never change a test to make it pass: a changed or deleted assertion is listed and justified in the pull request.
   No test depends on real time or on the machine's load.
8. **Every number has a source.** A budget cites its ADR. A measurement says what was measured (the real gesture or
   interaction), on which device or runner, in which build profile, and how many runs. If something was not measured,
   write "not measured" and the technical reason.
9. **No panics in library code.** No `unwrap`, `expect`, `panic!`, `todo!`, `unimplemented!`, indexing that can fail or
   arithmetic that can overflow outside tests. No `unsafe` (`forbid`). Errors are typed (`thiserror`).
10. **Every public item is documented**, with a doc test when it has a usage. `missing_docs` is an error.
11. **Dependencies are rare.** A new dependency is declared once in the root `Cargo.toml`, passes `cargo deny`, and its
    pull request says in one line why it is needed and what was considered instead. A dependency written in C, or
    exposed to untrusted content (network, files, plugins), needs the user's explicit agreement
    ([ADR 0005](docs/adr/0005-pinned-versions-and-dependencies.md)).
12. **One pull request does one thing.** No drive-by refactors, renames or formatting.
13. **Decisions belong to the user.** Anything that changes the architecture, the scope, a budget, a license, a
    dependency policy or a rule here is written as an ADR with options and a recommendation, and waits for the user.
14. **Do not guess the environment.** Use the pinned toolchain and versions. If a tool is missing or a command fails,
    report the exact output instead of working around it.

## Commands

The same commands run in CI ([`.github/workflows/ci.yml`](.github/workflows/ci.yml)):

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps --document-private-items --locked
cargo deny check
cargo deny --manifest-path crates/vehicoule-plugin-host/fuzz/Cargo.toml check
cargo deny --manifest-path crates/vehicoule-core/fuzz/Cargo.toml check
cargo deny --manifest-path crates/vehicoule-core/bench/Cargo.toml check
cargo mutants --workspace --cargo-arg=--locked   # cargo-mutants 27.1.0
wasm-tools component wit wit/ > /dev/null       # wasm-tools 1.263.0, the plugin contract parses
scripts/test-check-wit-compat.sh                 # the contract's version check passes its cases
scripts/check-wit-compat.sh OLD_WIT_DIR wit      # a change to the contract follows its version rule
cargo kani --workspace                           # Kani 0.68.0, after `cargo kani setup`
typos                                            # typos-cli 1.52.0
cargo machete                                    # cargo-machete 0.9.2, no unused dependency
zizmor .github/workflows/                        # zizmor 1.32.0, the workflows' security audit
# In crates/vehicoule-plugin-host/fuzz, on its pinned nightly (cargo-fuzz 0.13.2, ADR 0013), with the test plugins
# built by scripts/build-plugin-fixtures.sh into $FIXTURES:
cargo fuzz build
cargo fuzz run install "$FIXTURES" -- -runs=0
cargo fuzz run host_pattern -- -seed=1 -runs=200000
cargo fuzz run manifest -- -seed=1 -runs=200000
# In crates/vehicoule-core/fuzz, on the same nightly (ADR 0015):
cargo fuzz run queue -- -seed=1 -runs=200000
# In crates/vehicoule-core/bench, on Linux with Valgrind and gungraun-runner 0.20.0 (ADR 0016):
cargo bench --locked
```

Kani proofs live in `#[cfg(kani)]` modules beside the code they prove ([ADR 0004](docs/adr/0004-verification.md)). The
toolchain is pinned in [`rust-toolchain.toml`](rust-toolchain.toml). Run the full list before opening a pull request,
and report what you ran.

## Layout

| Path | What it holds |
|---|---|
| `crates/vehicoule-core/` | The core: app state, queue, player model. No UI, no I/O |
| `wit/` | The plugin contract, in WIT ([spec](docs/plugin-host-spec.md)) |
| `docs/adr/` | Decisions, numbered, one per file ([template](docs/adr/0000-template.md)) |
| `docs/` | Specs and references; [`core-playback-spec.md`](docs/core-playback-spec.md): sessions, queue, player model; [`plugin-providers.md`](docs/plugin-providers.md): which plugin provides what |
| `reference/` | Vendored sources of truth, pinned, never edited by hand |
| `.claude/skills/` | Vendored skills for coding agents, pinned |

## Conventions

- Everything in the repository is written in English: code, comments, docs, commit messages. Comments say why, not
  what.
- Commit subjects and pull request titles: `<area>: <What the change does>`, imperative, capitalized, no trailing
  period, about 72 characters (`queue: Keep the queue in the order of its source`). Areas: `core`, `queue`, `player`,
  `library`, another crate or module name in snake case, `docs`, `ci`, `deps`.
- The body explains why. A pull request that adds or changes public API lists every item under `## Public API`.
- New public structs are `#[non_exhaustive]` and implement `Default` when a default makes sense. Public enums are
  `#[non_exhaustive]`.
- States are enums, never several booleans. Invalid states are unrepresentable.
- No text shown to the user is written in code: each one is a Fluent message in the shell's language files, read
  through a typed key ([ADR 0008](docs/adr/0008-app-text-in-translation-files.md)).
- A core state machine, or a part that reads data from outside the app, comes with its fuzz target in the pull
  request that adds it ([ADR 0015](docs/adr/0015-fuzz-the-cores-state-machines.md)).
- A core state machine comes with benchmarks of its operations, each under a ceiling, in the pull request that adds
  it ([ADR 0016](docs/adr/0016-count-the-cores-instructions-with-callgrind.md)).
- A pull request that fixes a bug adds a test that fails without the fix, named `regression_<issue>_<what>` with a
  link to the issue above it ([ADR 0012](docs/adr/0012-one-regression-test-per-fixed-bug.md)).
- Durations and positions are typed (`std::time::Duration` or a newtype), never a bare integer or float in public API.
- Core abstractions are generic over the content domain (music, video, books); music-specific code lives in its own
  module or crate, so video and books add code instead of changing it.
