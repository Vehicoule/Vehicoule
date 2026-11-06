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
  app text.

## 6. Budgets

Every call has, from the host's configuration (never written in the code; tests use arbitrary values; real values are
set from measurements, section 9):

- **CPU**: Wasmtime fuel. Deterministic, so a test can show exactly when a loop is stopped.
- **Wall time**: an epoch deadline, ticked by the edge, so a call blocked in a slow plugin ends; a test ticks it by
  hand, never with real time.
- **Memory**: a ceiling on linear memory and on instances.
- **HTTP**: a maximum number of requests per call, a maximum response body size, and a deadline per request.
- **Output**: a maximum size of the returned value (strings, lists).
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

## 8. Contract 0.1.0 (draft)

Written in WIT. Durations are milliseconds in `u64` fields named `-ms`, because WIT has no duration type; the host
converts them to `std::time::Duration` at the boundary, and no bare integer reaches a public Rust API. Ids are the
provider's own, opaque to the host and the core; the core maps them to its own library ids.

```wit
package vehicoule:plugin@0.1.0;

interface types {
  enum domain { music, video, books }
  enum capability { search, metadata, resolve, radio, artwork, lyrics, link-import }

  record artwork { url: string, width: option<u32>, height: option<u32> }
  record artist-ref { id: string, name: string }
  record album-ref { id: string, title: string }
  record track {
    id: string,
    title: string,
    artists: list<artist-ref>,
    album: option<album-ref>,
    duration-ms: option<u64>,
    isrc: option<string>,
    artwork: list<artwork>,
  }
  record album { id: string, title: string, artists: list<artist-ref>, year: option<u16>, artwork: list<artwork> }
  record artist { id: string, name: string, artwork: list<artwork> }
  record playlist { id: string, title: string, owner: option<string>, artwork: list<artwork> }

  variant item { track(track), album(album), artist(artist), playlist(playlist) }
  enum item-kind { track, album, artist, playlist }

  variant error {
    not-found,
    unsupported,
    rate-limited(option<u64>),   // retry-after-ms, as told by the service; the core decides what to do with it
    invalid-input(string),
    upstream(string),            // the service failed or answered something the plugin cannot read
  }
}

interface http {
  enum method { get, head, post }
  record request { method: method, url: string, headers: list<tuple<string, string>>, body: option<list<u8>> }
  record response { status: u16, headers: list<tuple<string, string>>, body: list<u8> }
  variant http-error { refused-host, refused-scheme, budget-exceeded, timeout, transport(string) }
  fetch: func(req: request) -> result<response, http-error>;
}

interface log {
  enum level { debug, info, warn, error }
  log: func(level: level, message: string);
}

interface plugin {
  use types.{domain, capability};
  record manifest {
    id: string,
    name: string,
    version: string,
    domains: list<domain>,
    capabilities: list<capability>,
    http-hosts: list<string>,
  }
  describe: func() -> manifest;
}

interface search {
  use types.{item, item-kind, error};
  record page { items: list<item>, next: option<string> }
  search: func(query: string, kinds: list<item-kind>, cursor: option<string>) -> result<page, error>;
  suggest: func(prefix: string) -> result<list<string>, error>;
}

interface metadata {
  use types.{track, album, artist, playlist, error};
  record album-page { album: album, tracks: list<track> }
  record artist-page { artist: artist, top-tracks: list<track>, albums: list<album> }
  record playlist-page { playlist: playlist, tracks: list<track>, next: option<string> }
  get-album: func(id: string) -> result<album-page, error>;
  get-artist: func(id: string) -> result<artist-page, error>;
  get-playlist: func(id: string, cursor: option<string>) -> result<playlist-page, error>;
}

interface resolve {
  use types.{track, error};
  record request {
    track: track,                 // what to find; from this plugin or from another one
    rejected: list<string>,       // ids the user said were the wrong match, never to be returned again
    preferred: option<string>,    // an id the user chose as the right match
  }
  record playable {
    url: string,
    headers: list<tuple<string, string>>,
    mime: option<string>,
    valid-for-ms: option<u64>,    // relative, so the plugin needs no clock; the edge turns it into an expiry
    matched-id: string,           // which provider item was chosen, so the user can reject it
  }
  resolve: func(req: request) -> result<playable, error>;
}

interface radio {
  use types.{track, error};
  continuation: func(seed: track, exclude: list<string>, count: u32) -> result<list<track>, error>;
}

interface artwork {
  use types.{item, artwork, error};
  find: func(of: item) -> result<list<artwork>, error>;
}

interface lyrics {
  use types.{track, error};
  record line { start-ms: u64, text: string }
  variant text { plain(string), synced(list<line>) }
  find: func(of: track) -> result<text, error>;
}

interface link-import {
  use types.{item, error};
  import-link: func(url: string) -> result<item, error>;
}

// What every plugin imports and exports.
world base {
  import http;
  import log;
  export plugin;
}

// One world per capability. A plugin's own world includes the ones it provides, for example
// `world example { include search-plugin; include resolve-plugin; }`, so it exports exactly those interfaces.
world search-plugin { include base; export search; }
world metadata-plugin { include base; export metadata; }
world resolve-plugin { include base; export resolve; }
world radio-plugin { include base; export radio; }
world artwork-plugin { include base; export artwork; }
world lyrics-plugin { include base; export lyrics; }
world link-import-plugin { include base; export link-import; }
```

The draft above parses with `wasm-tools component wit` 1.263.0. How the host binds an interface that a plugin may or
may not export (Wasmtime's `bindgen!` per interface, then a lookup in the component's exports) is checked against
Wasmtime's source at the pinned version in the host's first pull request; if it cannot be done without inventing an
API, every capability becomes a required export answering `unsupported`, and this section is changed first.

Versioning: the package follows semver. While it is `0.x`, the host accepts exactly the minor version it was built
with. A change to the contract is a pull request that changes `wit/`, the host and its tests together, and the plugins
repository bumps its pinned copy in a pull request of its own.

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
   whole chain (search, resolve, play, radio). The three others give metadata, artwork and lyrics.

## 11. Order of the work

Each step is one pull request with its tests:

1. `wit/` with the contract, and a CI step that parses it at a pinned tool version.
2. The host: compile, check imports, `describe`, manifest validation, and the sandbox tests of section 9.
3. Capability calls and budgets, with the misbehaving test plugins.
4. The first real plugin, in the plugins repository, against the pinned contract.
