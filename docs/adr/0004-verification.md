# 0004. Show correctness with types, Kani, property tests and mutation testing

- Status: Accepted
- Date: 2026-11-06
- Deciders: the user

## Context

The first app had tests that only confirmed the intended action and green goldens that hid a hard-coded shadow. Its
queue and player state broke in ways no test caught. The user decided on 2026-11-06 to keep formal checking where an
error costs most, the app's core, and property tests elsewhere (recorded in `Vehicoule/nsh-m3e`'s ADR 0005, which
assigns Kani to this repository). Lean and Aeneas were judged too costly.

## Decision

For this repository:

1. **Types first**: states as enums, typed durations and positions, `#[non_exhaustive]`; invalid states
   unrepresentable.
2. **Kani** (bounded model checking) on the critical state machines: the queue, the player model, and later the sync
   merge rules. Each proof states an invariant (for example: the current index always points into the queue; a removed
   track is never played; every command sequence leaves the player in a defined state).
3. **Property tests** (`proptest`) on the same machines over long random event sequences, beyond Kani's bounds.
4. **Behavior tests** that check what must happen *and* what must not (no retry loop on a failed track, no second
   owner of the position, no command after a stop).
5. **Mutation testing** (`cargo-mutants`) in CI, so a test that asserts nothing is caught.
6. **API stability** with `cargo-semver-checks` once a crate is published, if one ever is.

## Consequences

- Kani, `proptest` and each other tool are added with the first code that needs them, with their pinned versions and
  a CI job; the Kani job is required from the first proof harness.
- A Kani proof is never removed or bounded lower to get green (AGENTS.md rule 2).
- Mutation testing is slow; its survivors fail the check.
