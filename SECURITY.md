# Security policy

## Reporting a vulnerability

Report it privately, through GitHub's private vulnerability reporting:
[report a vulnerability](https://github.com/Vehicoule/Vehicoule/security/advisories/new). Do not open a public
issue, pull request or discussion about it.

Say what is affected (the crate and the commit), how to reproduce it, and what an attacker gains. A test plugin or a
failing test is the best reproduction.

## Supported versions

Vehicoule has no release yet. Only the latest commit of `main` is supported.

## Scope

In scope, the boundaries the app relies on:

- **The plugin sandbox** ([ADR 0007](docs/adr/0007-plugin-host.md), [spec](docs/plugin-host-spec.md)): a plugin that
  reaches anything the host does not hand it, escapes Wasmtime, or exceeds a budget (fuel, memory, tables, output,
  size) without its call being stopped.
- **Plugin installation**: a plugin installed without passing the feed layer's verification.
- **The core**: input that makes it panic, loop forever or grow memory without bound.
- **The supply chain of this repository**: its CI workflows, its pinned dependencies and vendored sources.

Out of scope: bugs in a plugin's own code or in a content provider's service; report those to their authors.
