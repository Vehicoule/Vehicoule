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
| Bounded model checking | https://model-checking.github.io/kani/ | to pin with the first proof harness ([ADR 0004](adr/0004-verification.md)) |
| Property tests | https://docs.rs/proptest | to pin with the first property test |
| WIT parser (plugin contract) | https://github.com/bytecodealliance/wasm-tools | `1.263.0`, installed by CI (`.github/workflows/ci.yml`) |

## NSH and the component library

Not used by any crate yet. When a crate depends on them, they are pinned here and in the root `Cargo.toml`.

| What | Where |
|---|---|
| NSH | https://github.com/longbridge/nsh, branch `main`; docs for agents: https://nsh.dev/llms.txt |
| `nsh-m3e` | https://github.com/Vehicoule/nsh-m3e |
