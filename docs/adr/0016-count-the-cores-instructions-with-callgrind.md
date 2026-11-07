# 0016. Count the core's instructions with Callgrind, and fail above a ceiling

- Status: Accepted
- Date: 2026-11-07
- Deciders: the user

## Context

The core runs on every event the app gets ([ADR 0003](0003-core-architecture.md)). Nothing in CI says how much an
operation costs, so a change that makes the queue much slower passes every check. The user asked for benchmarks of the
queue and of plugin loading in the list of CI additions (2026-11-07).

A benchmark that measures time on GitHub's shared runners depends on their load, which AGENTS.md rule 7 forbids for a
test. Callgrind, a Valgrind tool, counts the instructions an operation runs instead. The same code, built by the same
compiler, run by the same Valgrind, gives the same count on any machine and at any load.

iai-callgrind, the Rust harness for Callgrind named in the user's list, was renamed gungraun in 2025. Its last
release under the old name is 0.16.1 (2025-07-30), and gungraun's latest is 0.20.0 (2026-09-26), both MIT or
Apache-2.0 (crates.io). gungraun is Rust and supports Callgrind's counts with a ceiling per benchmark
(`Callgrind::hard_limits`) or a limit relative to a saved baseline (`Callgrind::soft_limits`), in its source at 0.20.0.
It needs Valgrind and a runner binary, `gungraun-runner`, at the same version. Valgrind runs on Linux, not on macOS on
Apple silicon nor on Windows, so the benchmarks run in CI only.

## Options

1. **A ceiling per benchmark.** Each benchmark records the count it ran when its ceiling was set, from the CI's
   `bench` job, and fails above 10 % more. A real change of cost changes the recorded count in the same pull request,
   and raising one is the user's decision (AGENTS.md rule 2). It catches a slow drift over many pull requests, since
   the ceiling never moves on its own.
2. **A limit relative to the base commit.** The job measures the base and the pull request and fails above 5 % more.
   No count is kept in the repository, but a drift of 4 % per pull request passes forever, and the job runs twice.
3. **Report only.** The job prints the counts and never fails. Nothing stops a regression.

## Decision

Option 1, chosen by the user on 2026-11-07:

- The benchmarks live in `crates/vehicoule-core/bench/`, a workspace of its own like the fuzz targets
  ([ADR 0015](0015-fuzz-the-cores-state-machines.md)), with its own `Cargo.lock`: gungraun never joins the app's
  dependency graph. `cargo deny` checks that workspace.
- The CI's `bench` job runs on `ubuntu-24.04` and installs Valgrind at the exact package version the counts were
  recorded with (`1:3.22.0-0ubuntu3`), and checks it: when GitHub's weekly image updates no longer offer it, the job
  fails at the install instead of counting with another Valgrind. Moving to another version is a pull request that
  records the counts again. It installs `gungraun-runner` at gungraun's version.
- The rest of the image (the C library, the kernel) can still change under the counts. The margin absorbs small
  shifts; a larger one fails the job with no code change, and is then recorded again in a pull request of its own.
- The margin over a recorded count is 10 %, and at least 100 instructions: the same build counted up to 32
  instructions apart in CI and locally, in the benchmarks that allocate (one run each), so 10 % of a count of a few
  dozen would fail on noise.
- The first benchmarks cover every queue operation on a queue of 10,000 entries, unshuffled and shuffled. That length
  is chosen for the benchmarks; the core spec sets no limit on the queue's length.
- Plugin loading gets its benchmark in a pull request coordinated with the plugin host's work. It lives in
  `crates/vehicoule-plugin-host/bench/`, a workspace of its own, and counts the install of the host's `good` test
  plugin: about 646 million instructions, 15 s under Valgrind locally (Valgrind 3.22.0, one run).
- A new core state machine comes with its benchmarks, as it comes with its proofs and its fuzz target.

## Consequences

- A pull request that makes an operation cost more than 10 % over its recorded count fails until the user agrees to
  the new count.
- A pull request that makes an operation cheaper should lower its recorded count, or the margin grows. CI does not
  enforce that.
- The first run already shows one cost: on a shuffled queue of 10,000 entries, `Queue::play_order` runs 250 million
  instructions, because it looks each id up among the entries one by one. Fixing that is its own pull request, which
  lowers that count.
- The job adds the build of the bench workspace, the install of Valgrind and of `gungraun-runner`, and the run: 19
  benchmarks took 6 s in CI, and 23 took 11 s locally in a shared container (Valgrind 3.22.0, one run each).
