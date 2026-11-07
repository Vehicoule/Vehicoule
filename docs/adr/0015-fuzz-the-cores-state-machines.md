# 0015. Fuzz the core's state machines with sequences of operations

- Status: Accepted
- Date: 2026-11-07
- Deciders: the user

## Context

The core is a set of synchronous state machines ([ADR 0003](0003-core-architecture.md)). Kani proves each queue
operation from any state up to small bounds, and property tests check chosen properties over random values
([ADR 0004](0004-verification.md)). Neither runs long sequences of mixed operations: a bug that needs, say, a shuffle,
two removals and a move in that order is out of a proof's bounds and unlikely in a property test's strategy. A
coverage-guided fuzzer that reads its input as a sequence of operations looks for exactly those paths.

[ADR 0013](0013-fuzz-the-plugin-hosts-untrusted-inputs.md) set up cargo-fuzz and libFuzzer for the plugin host, in a
workspace of its own on a pinned nightly; the user agreed on 2026-11-07 to libFuzzer's C++ dependency for fuzzing. The
same day the user asked for the whole core to be fuzzed.

A target needs to check the invariants of the core spec after every operation. `Queue::invariants_hold` checks them
for the tests and the proofs, but it is crate-private, out of a fuzz target's reach. ADR 0013 left reaching
crate-private items to a later decision.

## Options

1. **cargo-fuzz targets in `crates/vehicoule-core/fuzz/`, and a `fuzzing` feature of `vehicoule-core` that makes
   `invariants_hold` public.** The same tool, toolchain and dependency as ADR 0013. The feature adds no code to the
   app's build, since nothing in the app enables it.
2. **The same targets, with the invariants written again in the target from the public API.** No feature, but the
   target's copy can drift from the one the proofs use (AGENTS.md rule 6).
3. **Longer property tests instead.** No new crate, but no coverage guidance: the strategy decides which sequences
   run.

## Decision

Option 1, chosen by the user on 2026-11-07:

- Each core state machine gets a target in `crates/vehicoule-core/fuzz/`, a workspace of its own like the plugin
  host's, with its own `Cargo.lock` and the same pinned nightly. It depends on `vehicoule-core` with the `fuzzing`
  feature; nothing else enables that feature.
- A target reads its input as a sequence of operations, calls only the public API, and checks after every operation:
  the invariants (`invariants_hold`), and that the operation did what its documentation says and nothing else (an
  operation that fails leaves the state unchanged, a new entry has an id never seen before, and so on).
- The first target is `queue`, over every queue operation. The player model, failures and radio get theirs in the pull
  requests that add them.
- Beyond the core, the user agreed on 2026-11-07 that every part of the app that reads data from outside it (plugin
  feeds and their signatures, local files such as audio tags, lyrics or books, imported links, saved state read back)
  gets its fuzz target in the pull request that adds it.
- CI runs each target for a fixed number of inputs from a fixed seed, in the `fuzz` job of ADR 0013, and `cargo deny`
  checks the new workspace. A crash found is a bug, fixed with a regression test
  ([ADR 0012](0012-one-regression-test-per-fixed-bug.md)).

## Consequences

- `invariants_hold` becomes public API under the `fuzzing` feature; changing it is a change to the fuzz targets too.
- The `fuzz` job runs longer: 200,000 inputs of `queue` took 23 s, then 79 s (measured locally in a shared container:
  4 cores, cargo-fuzz's release profile, two runs), plus the build of the new workspace.
- A new core state machine comes with its fuzz target, as it comes with its proofs.
