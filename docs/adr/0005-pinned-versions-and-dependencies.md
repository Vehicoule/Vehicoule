# 0005. Pin every version and keep dependencies few

- Status: Accepted
- Date: 2026-11-06
- Deciders: the user

## Context

An agent in an ambiguous environment guesses; the first app's agents invented APIs and drifted between versions, and
its Rust core reached 19.9 MB, mostly Wasmtime, Cranelift and Sentry. The policy below was accepted by the user on
2026-11-06 for `Vehicoule/nsh-m3e` (its ADR 0006) and applies here the same way.

## Decision

- Rust toolchain pinned in `rust-toolchain.toml`; `Cargo.lock` committed; `--locked` everywhere in CI.
- Dependencies are declared once in the workspace; `cargo deny` checks advisories, licenses (GPL-3.0 compatible) and
  sources; a new dependency needs a one-line justification. A dependency written in C, or exposed to untrusted
  content (network, files, plugins), needs the user's explicit agreement.
- NSH, once a crate depends on it, follows its `main` branch pinned by `rev`, bumped by a scheduled pull request, as in
  `nsh-m3e`; its vendored skills match the pinned commit.
- Vendored references (`reference/`, `.claude/skills/`) carry their source revision and are never edited by hand.
- GitHub Actions are pinned by commit SHA.

## Consequences

- Upgrades are deliberate and reviewable; nothing changes under an agent's feet.
- Bumps are opened by a bot as pull requests and merged only when CI is green.
