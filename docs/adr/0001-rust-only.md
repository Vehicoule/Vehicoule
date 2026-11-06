# 0001. Write the app in Rust, with no TSX

- Status: Accepted
- Date: 2026-11-06
- Deciders: the user

## Context

NSH has three modes: Rust, Rust with TSX, and TSX. TSX runs on QuickJS, a JavaScript engine written in C. NSH's own
skill recommends Rust for an app shipped in its binary, for medium and large apps. Vehicoule has strict performance
budgets, and the plugin host (Wasmtime), NSH and the sync transport candidates are Rust. The user decided on
2026-11-06 that the components and the app are written in Rust; the same decision is recorded for the component
library in `Vehicoule/nsh-m3e`'s ADR 0001.

## Options

1. Rust only: one language, no JavaScript engine, `cargo` everywhere.
2. Rust core with TSX screens: hot reload, at the cost of QuickJS and two languages.

## Decision

Rust only, for the core, the shells and every crate of this repository.

## Consequences

- No QuickJS in the app binary.
- Edits rebuild and restart instead of hot reloading.
- Moving a screen to TSX later needs a new ADR.
