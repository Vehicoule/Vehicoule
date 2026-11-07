# References

Every source an agent may rely on, with the version it is pinned to. Use the pinned copy when there is one; the live
site may describe a newer version than the one this repository builds against.

## Rust

| What | Where |
|---|---|
| Toolchain | `1.99.1`, in [`rust-toolchain.toml`](../rust-toolchain.toml) |
| Google's Rust style guide (Chrome and Fuchsia tagged rules do not apply here) | [`reference/rust/google-rust-style-guide.md`](../reference/rust/google-rust-style-guide.md), from https://google.github.io/styleguide/rustguide.html |
| Apollo's Rust best practices | [`.claude/skills/rust-best-practices/`](../.claude/skills/rust-best-practices/) |
| Rust coding guidelines (50 core rules) | [`.claude/skills/rust-coding-guidelines/`](../.claude/skills/rust-coding-guidelines/) |
| Rust API Guidelines | https://rust-lang.github.io/api-guidelines/ |
| Clippy lints | https://rust-lang.github.io/rust-clippy/ |

## Verification tools

| What | Where | Pinned |
|---|---|---|
| Mutation testing | https://mutants.rs | `27.1.0`, installed by CI (`.github/workflows/ci.yml`) |
| Dependency checks | https://embarkstudios.github.io/cargo-deny/ | `EmbarkStudios/cargo-deny-action` v2.1.1, by SHA in CI |
| Bounded model checking | https://model-checking.github.io/kani/ | `kani-verifier` `0.68.0`, installed by CI ([ADR 0004](adr/0004-verification.md)) |
| Property tests | https://docs.rs/proptest | `1.12.0` in the root `Cargo.toml`, exact version in `Cargo.lock` |
| WIT parser (plugin contract) | https://github.com/bytecodealliance/wasm-tools | `1.263.0`, installed by CI (`.github/workflows/ci.yml`) |
| Spell check | https://github.com/crate-ci/typos | `typos-cli` `1.52.0`, installed by CI (`.github/workflows/ci.yml`) |
| Unused dependencies | https://github.com/bnjbvr/cargo-machete | `0.9.2`, installed by CI (`.github/workflows/ci.yml`) |
| Workflow audit | https://docs.zizmor.sh | `zizmor` `1.32.0`, installed by CI (`.github/workflows/ci.yml`) |
| Fuzzing | https://rust-fuzz.github.io/book/cargo-fuzz.html | `cargo-fuzz` `0.13.2`, installed by CI; `libfuzzer-sys` `0.4.13`; toolchain `nightly-2026-11-01`, in [`crates/vehicoule-plugin-host/fuzz/rust-toolchain.toml`](../crates/vehicoule-plugin-host/fuzz/rust-toolchain.toml) and [`crates/vehicoule-core/fuzz/rust-toolchain.toml`](../crates/vehicoule-core/fuzz/rust-toolchain.toml) ([ADR 0013](adr/0013-fuzz-the-plugin-hosts-untrusted-inputs.md), [ADR 0015](adr/0015-fuzz-the-cores-state-machines.md)) |

## NSH and the component library

Not used by any crate yet. When a crate depends on them, they are pinned here and in the root `Cargo.toml`.

| What | Where |
|---|---|
| NSH | https://github.com/longbridge/nsh, branch `main`; docs for agents: https://nsh.dev/llms.txt |
| `nsh-m3e` | https://github.com/Vehicoule/nsh-m3e |
