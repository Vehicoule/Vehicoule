# Plugin host spec: the WIT contract and the sandbox

- Status: Accepted (the user, 2026-11-06)
- Scope: the plugin host (an edge crate) and version 0.1.0 of the contract between the host and the plugins. The
  routing between plugins is the core's and comes in the core's own spec; feeds, signatures and installation from a
  feed come in their own spec.
- Rules: [ADR 0003](adr/0003-core-architecture.md) (I/O at the edges), [ADR 0004](adr/0004-verification.md),
  [ADR 0005](adr/0005-pinned-versions-and-dependencies.md), [ADR 0007](adr/0007-plugin-host.md) (Wasmtime, component
  model, no WASI).

This spec says what the host must do and what it must never do. Every "must" and "never" becomes a test. A behavior
that is not written here is not specified: the code does not invent it, it comes back here first.

## 1. Who does what

Accepted by the user on 2026-11-06:

- **The core** decides: which plugin to ask for which capability (a priority list per capability), what to do when
  a plugin fails (the next candidate), the user's corrections. It never sees Wasm.
- **The host** executes one plugin call at a time per store, inside a sandbox with budgets, and returns a typed
  result or a typed failure. It never decides to retry, to skip or to ask another plugin.
- **A plugin** provides service-dependent capabilities and nothing else. It reaches the network only through the
  host, only to the hosts it declares; it touches no file and keeps no state between calls.

## 2. Where things live

Proposed default, to be confirmed by the user (section 10):

| What | Where |
|---|---|
| The contract, `wit/` (package `vehicoule:plugin`) | `Vehicoule/Vehicoule`, next to the host, so a contract change and its host change are one pull request |
| The host, `crates/vehicoule-plugin-host` | `Vehicoule/Vehicoule` |
| Test plugins that misbehave on purpose (section 8) | `Vehicoule/Vehicoule`, under the host crate, built in CI |
| Real plugins, one crate each, and their feed | `Vehicoule/vehicoule-plugins`, building against a copy of `wit/` pinned to a Vehicoule commit and checked by a sync script, as `nsh-m3e` does for NSH's skills |
| Plugins exposed to DMCA takedowns (`ytmusic`) | `Vehicoule/vehicoule-plugins-extra`, said by the user on 2026-11-06; later mirrored on GitLab and Codeberg under the same user and name, so its build and its copy of `wit/` must not depend on GitHub-only features |

## 3. Plugin lifecycle

1. **Install.** The host receives the plugin's bytes, already verified by the feed layer. Bytes over the size ceiling
   (section 6) are rejected before any parsing or compilation. Otherwise it compiles them, checks
   that every import is one of the contract's (section 5), calls `describe` with a sandbox where every HTTP request is
   refused, and validates the manifest (section 4). Any failure rejects the plugin with a typed reason; nothing is
   kept.
2. **Grant.** The manifest is stored by the edge. The declared hosts and capabilities are shown to the user before
   the plugin is enabled. The stored manifest, not a later `describe`, is what the host enforces.
3. **Call.** Each capability call gets a fresh store and a fresh instance, with its own budgets (section 6), and is
   dropped when the call returns. Nothing survives from one call to the next, so a plugin cannot keep user state and a
   failed call cannot corrupt the next one.
4. **Compiled code** is compiled from the bytes at install and at each launch. Keeping it across launches waits for a
   measurement and a new ADR (ADR 0007). Compilation runs on a host thread, never on the core's or the UI's, and the
   app does not wait for it to start; a call to a plugin that is still compiling waits for it, under the call's wall
   time budget. Fuel and store limits do not apply to the compiler, so the size ceiling is what bounds its work; the
   compile time and memory of every plugin are recorded, and the ceiling is set from them (section 9).

## 4. Manifest

Returned by `describe`, validated at install:

- `id`: lowercase ASCII letters, digits and `-`, unique among installed plugins.
- `name` and `version` (semver), shown to the user.
- `domains`: the content domains the plugin serves (`music`, `video`, `books`).
- `capabilities`: the capabilities it provides. Each one must match an interface the component exports, and every
  capability interface it exports must be listed: a mismatch rejects the plugin.
- `http-hosts`: the hosts it may reach, each an exact host name (`api.example.org`) or a suffix wildcard
  (`*.example.org`, which matches subdomains only, not `example.org` itself). No IP address, no port, no bare `*`, no
  wildcard in the middle.

## 5. The sandbox

- **Imports.** A plugin may import only the contract's interfaces: `http`, `log`, and `types`, which holds types and
  no function. A component that imports anything else, WASI included, fails at install.
- **HTTP.** Through the host only. A request must be `https`, to a host matching the manifest, on the default port.
  The host follows no redirect: a 3xx goes back to the plugin, whose next request is checked again. No cookie, cache
  or credential is kept between requests by the host on the plugin's behalf. A refused request returns an error to
  the plugin and is counted in the call's report; it never reaches the network.
- **Streams.** A resolved stream URL must also be `https` on a declared host, since the platform player will fetch
  it; otherwise the result is rejected as invalid. The player edge receives the plugin's declared hosts with the
  stream and checks every redirect hop against them: a hop to an undeclared host, or off `https`, fails the load, which
  the core handles as any failed load. This check is the player edge's (each platform player), specified here because
  the sandbox depends on it; its tests come with each player.
- **Log.** Messages are bounded in size and count per call, tagged with the plugin id, and never shown to the user as
  app text. A call keeps its first messages up to the log budget (section 6) and counts the ones after as dropped; a
  message longer than the budget is cut at a character boundary and marked as cut. Every control character, line
  breaks included, and U+2028 and U+2029 (the line and paragraph separators, which are not control characters)
  become U+FFFD, so a message is one line and cannot forge another where the edge writes it. The
  messages come back in the call's report (section 7), in order, for the edge to write where it keeps logs; messages
  logged during install are dropped.

## 6. Budgets

Every call has, from the host's configuration (never written in the code; tests use arbitrary values; real values are
set from measurements, section 9):

- **CPU**: Wasmtime fuel. Deterministic, so a test can show exactly when a loop is stopped.
- **Wall time**: an epoch deadline, ticked by the edge, so a call blocked in a slow plugin ends; a test ticks it by
  hand, never with real time.
- **Memory**: a ceiling on linear memory and on instances.
- **HTTP**: a maximum number of requests per call, a maximum response body size, and a deadline per request.
- **Output**: a maximum size of the returned value (strings, lists).
- **Log**: a maximum number of messages per call, and a maximum size per message.
- **Plugin size**: a maximum size of the plugin's bytes, checked before compilation (section 3).

Exceeding any budget ends the call with the matching failure. None of them is retried by the host.

## 7. Failures

A call returns either the plugin's value or one `PluginFailure`, typed and `#[non_exhaustive]`:

- the plugin's own error from the contract (`not-found`, `unsupported`, `rate-limited`, `invalid-input`, `upstream`);
- a trap (the plugin crashed);
- a budget exceeded, saying which;
- an invalid result (over the output budget, a stream on an undeclared host, a value that breaks the contract);
- the plugin is not installed or does not provide that capability.

The host reports; the core decides. The host never retries, never falls back to another plugin and never hides a
failure behind a default value.

Every capability call also returns a report of what it used, with its value or its failure: the fuel it consumed, the
peak bytes of its memories and tables together, the requests it sent to the network, the requests the sandbox refused,
and the size of what the plugin returned (a value or its own error, as the output budget counts it), or none when it
returned nothing; and what the plugin logged (section 5), with the count of messages dropped. A call to a capability the plugin does not provide never runs and reports nothing used. The report
is what the measurements of section 9 read. It does not hold the wall time yet: Wasmtime 48.0.8 gives no way to read
the epoch (`Engine` has `increment_epoch` and no getter), so how to count a call's ticks is a later change.

## 8. Contract 0.1.0

Written in WIT. Durations are milliseconds in `u64` fields named `-ms`, because WIT has no duration type; the host
converts them to `std::time::Duration` at the boundary, and no bare integer reaches a public Rust API. Ids are the
provider's own, opaque to the host and the core; the core maps them to its own library ids.

The contract is [`wit/plugin.wit`](../wit/plugin.wit), its only copy. Its shape: an interface `types` (no
function), the two host interfaces a plugin may import (`http`, `log`), the `plugin` interface every plugin exports
(`describe`), one interface per capability, a world `base` and one world per capability that includes it. A plugin's
own world includes the capability worlds it provides, for example
`world example { include search-plugin; include resolve-plugin; }`, so it exports exactly those interfaces.

CI parses it with `wasm-tools component wit` at the version pinned in the workflow. How the host binds an interface that a plugin may or may not export was checked with `wasmtime` 48.0.8: `bindgen!`
on a capability world generates, per exported interface, a `GuestIndices::new` that returns an error when the
component does not export that interface, and `Component::get_export_index` finds an export by name before
instantiation. The host binds each capability world and reads which interfaces a plugin exports from these lookups.

Versioning: the package follows semver. While it is `0.x`, the host accepts exactly the minor version it was built
with. A change to the contract is a pull request that changes `wit/`, the host and its tests together, and the plugins
repository bumps its pinned copy in a pull request of its own.

CI checks every change to `wit/` against the parent commit with `scripts/check-wit-compat.sh`: a contract that changed
(apart from its doc comments and layout) raises its version, the version never goes down, and while it stays on the
line the host accepts (the same minor while it is `0.x`), a component that implements any world of the old contract
still targets the world of the same name in the new one (`wasm-tools component targets`).

## 9. Tests

The host is tested with small plugins written for the purpose, built to components in CI. The network is a trait
given to the host; tests use a fake that records every request and never opens a socket. Wall time is an epoch the
test ticks by hand.

Must happen:

1. A well-behaved plugin installs, describes itself, and answers each capability it declares.
2. Each budget, exceeded, ends the call with the failure that names it, within the budget (fuel: exactly).
3. A trap returns a failure; the next call to the same plugin works.

Must never happen:

1. A request to an undeclared host, a non-`https` URL, an IP address or another port reaches the fake network.
2. A component importing WASI or any interface outside the contract installs.
3. A manifest whose capabilities differ from the component's exports installs.
4. A value from one call is visible in the next call (state kept in the plugin's memory).
5. The host retries a failed call, or calls a second plugin, by itself.
6. A stream URL on an undeclared host is returned to the caller.
7. A redirect is followed to an undeclared host.
8. A plugin over the size ceiling reaches the compiler.
9. A platform player follows a stream's redirect to an undeclared host (tested with each player edge).

Property tests (`proptest`) on the host-pattern matcher and the manifest validation. Mutation testing as everywhere.

Measurements for the budgets: once a real plugin runs, the host records each call's fuel, wall time, memory peak,
request count and output size, in release builds on the Xiaomi 14 and the Mac; each budget is then set above the worst
measured case with a margin the user chooses, in an ADR, the same method as the core's deadlines.

## 10. Decisions

Answered by the user on 2026-11-06:

1. **Where things live**: section 2. The user said what `vehicoule-plugins-extra` is for and raised no objection to
   the rest of the layout.
2. **Wasmtime**: agreed, pinned to the LTS 48.x (ADR 0007).
3. **No WASI**: left to Claude; taken: plugins get only the contract's `http` and `log`.
4. **First music plugin**: `ytmusic`, in `vehicoule-plugins-extra`. The published plugins are `deezer`, `itunes`,
   `lrclib` and `ytmusic`; only `ytmusic` resolves a full track to a stream, so it is the only one that tests the
   whole chain (resolve, play, radio). The three others give search, metadata, artwork and lyrics; which plugin
   provides which capability, and in which default order, is in [plugin providers](plugin-providers.md), decided by
   the user later on 2026-11-06.

## 11. Order of the work

Each step is one pull request with its tests:

1. `wit/` with the contract, and a CI step that parses it at a pinned tool version.
2. The host: compile, check imports, `describe`, manifest validation, and the sandbox tests of section 9.
3. Capability calls and budgets, with the misbehaving test plugins.
4. The first real plugin, in the plugins repository, against the pinned contract.
