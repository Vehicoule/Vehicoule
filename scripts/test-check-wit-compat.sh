#!/usr/bin/env bash
# Tests scripts/check-wit-compat.sh: each case writes an old and a new contract and states whether the change must be
# allowed or refused. The cases use a small contract of their own, shaped like wit/plugin.wit (a shared interface, a
# host import, a plugin export, a base world and a capability world that includes it), so they do not break when the
# real contract changes; one more case runs the check on the real contract.
#
# Usage: scripts/test-check-wit-compat.sh
# Needs `wasm-tools` (version in docs/references.md).
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
check="$root/scripts/check-wit-compat.sh"
work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT

# The base contract of every case, at version $1.
contract() {
  cat << EOF
package test:plugin@$1;

interface types {
  /// A track.
  record track { id: string, title: string }
  variant error { not-found, other(string) }
}

interface log {
  info: func(message: string);
}

interface plugin {
  name: func() -> string;
}

interface search {
  use types.{track, error};
  search: func(query: string) -> result<list<track>, error>;
}

world base {
  import log;
  export plugin;
}

world search-plugin { include base; export search; }
EOF
}

failures=0
case_number=0

# expect allow|refuse NAME: runs the check on $work/old and $work/new, which the case has just written.
expect() {
  local outcome
  if "$check" "$work/old" "$work/new" > "$work/out" 2>&1; then outcome=allow; else outcome=refuse; fi
  if [[ "$outcome" == "$1" ]]; then
    echo "ok: $2"
  else
    echo "FAILED: $2: expected the check to $1 the change, it did $outcome it. Its output:"
    cat "$work/out"
    failures=$((failures + 1))
  fi
}

# Writes the old contract at version $1 and an unchanged new one at version $2; a case then edits the new one.
start() {
  case_number=$((case_number + 1))
  rm -rf "$work/old" "$work/new"
  mkdir "$work/old" "$work/new"
  contract "$1" > "$work/old/contract.wit"
  contract "$2" > "$work/new/contract.wit"
}
edit_new() { sed -i -E "$1" "$work/new/contract.wit"; }

start 0.1.0 0.1.0
expect allow "an unchanged contract"

start 0.1.0 0.1.0
edit_new 's|/// A track.|/// A track, as a provider describes it.|; s|^world base \{$|world base {\n|'
expect allow "a change to doc comments and layout only"

start 0.1.0 0.1.0
edit_new 's|record track \{ id: string, title: string \}|record track { id: string, title: string, year: u16 }|'
expect refuse "a changed record under the same version"

start 0.1.0 0.1.0
edit_new 's|^world search-plugin.*$|&\nworld other-plugin { include base; }|'
expect refuse "an added world under the same version"

start 0.2.0 0.1.9
expect refuse "a version that goes down"

start 0.1.0 0.1.1
expect allow "a raised patch with no change"

start 0.1.0 0.1.1
edit_new 's|^world search-plugin.*$|&\ninterface lyrics { find: func(id: string) -> string; }\nworld lyrics-plugin { include base; export lyrics; }|'
expect allow "a new capability under a raised patch"

start 0.1.0 0.1.1
edit_new 's|  info: func\(message: string\);|&\n  warn: func(message: string);|'
expect allow "a new host function under a raised patch, which old plugins do not call"

start 0.1.0 0.1.1
edit_new 's|record track \{ id: string, title: string \}|record track { id: string, title: string, year: u16 }|'
expect refuse "a field added to a record under a raised patch"

start 0.1.0 0.1.1
edit_new 's|  search: func\(query: string\) -> result<list<track>, error>;|&\n  suggest: func(query: string) -> list<string>;|'
expect refuse "a function added to a plugin export under a raised patch, which old plugins do not export"

start 0.1.0 0.1.1
edit_new 's|  info: func\(message: string\);||'
expect refuse "a host function removed under a raised patch, which old plugins call"

start 0.1.0 0.1.1
edit_new 's|^world search-plugin.*$||'
expect refuse "a world removed under a raised patch"

start 0.1.0 0.2.0
edit_new 's|record track \{ id: string, title: string \}|record track { id: string, title: string, year: u16 }|'
expect allow "a field added to a record under a raised minor while the version is 0.x"

start 1.0.0 1.1.0
edit_new 's|record track \{ id: string, title: string \}|record track { id: string, title: string, year: u16 }|'
expect refuse "a field added to a record under a raised minor from 1.0"

start 1.0.0 2.0.0
edit_new 's|record track \{ id: string, title: string \}|record track { id: string, title: string, year: u16 }|'
expect allow "a field added to a record under a raised major"

start 0.1.0 0.1.0
edit_new 's|^package test:plugin@|package test:other@|'
expect refuse "a renamed package"

case_number=$((case_number + 1))
if "$check" "$root/wit" "$root/wit" > "$work/out" 2>&1; then
  echo "ok: the real contract against itself"
else
  echo "FAILED: the real contract against itself. The check's output:"
  cat "$work/out"
  failures=$((failures + 1))
fi

echo "$case_number cases, $failures failed"
(( failures == 0 ))
