#!/usr/bin/env bash
# Checks that a change to the plugin contract follows its semver rule (docs/plugin-host-spec.md, section 8).
#
# Usage: scripts/check-wit-compat.sh OLD_WIT_DIR NEW_WIT_DIR
# Needs `wasm-tools` (version in docs/references.md). Exits 0 when the change is allowed, 1 when it is not.
#
# - A contract that changed (anything but its doc comments and its layout) must raise its version.
# - The version never goes down.
# - When the version stays on the same compatible line (same minor while it is 0.x, same major after), a plugin built
#   for the old contract must still fit the new one: for every world of the old contract, a component that implements
#   that world must target the world of the same name in the new contract. The host accepts exactly that line, so a
#   change that breaks such a plugin needs a new line.
set -euo pipefail

old="${1:?usage: scripts/check-wit-compat.sh OLD_WIT_DIR NEW_WIT_DIR}"
new="${2:?usage: scripts/check-wit-compat.sh OLD_WIT_DIR NEW_WIT_DIR}"
work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT

# wasm-tools prints the resolved package in one canonical layout, so only the doc comments are left to drop.
wasm-tools component wit "$old" | grep -v -E '^[[:space:]]*///' > "$work/old.wit"
wasm-tools component wit "$new" | grep -v -E '^[[:space:]]*///' > "$work/new.wit"

package_line() { grep -m 1 -E '^package ' "$1"; }
version_of() {
  package_line "$1" | sed -n -E 's/^package [a-z0-9:-]+@([0-9]+)\.([0-9]+)\.([0-9]+);$/\1 \2 \3/p'
}

old_package="$(package_line "$work/old.wit")"
new_package="$(package_line "$work/new.wit")"
read -r old_major old_minor old_patch <<< "$(version_of "$work/old.wit")"
read -r new_major new_minor new_patch <<< "$(version_of "$work/new.wit")"
if [[ -z "${old_patch:-}" || -z "${new_patch:-}" ]]; then
  echo "error: the package needs a version of the form MAJOR.MINOR.PATCH: '$old_package', '$new_package'" >&2
  exit 1
fi
if [[ "${old_package%@*}" != "${new_package%@*}" ]]; then
  old_name="${old_package%@*}"
  new_name="${new_package%@*}"
  echo "error: the package was renamed from '${old_name#package }' to '${new_name#package }'" >&2
  exit 1
fi
old_version="$old_major.$old_minor.$old_patch"
new_version="$new_major.$new_minor.$new_patch"

# Versions compare as numbers, part by part.
if (( new_major != old_major )); then
  order=$(( new_major > old_major ? 1 : -1 ))
elif (( new_minor != old_minor )); then
  order=$(( new_minor > old_minor ? 1 : -1 ))
elif (( new_patch != old_patch )); then
  order=$(( new_patch > old_patch ? 1 : -1 ))
else
  order=0
fi

if (( order < 0 )); then
  echo "error: the contract's version went down, from $old_version to $new_version" >&2
  exit 1
fi

if (( order == 0 )); then
  if ! diff -u "$work/old.wit" "$work/new.wit"; then
    echo "error: the contract changed but its version stayed $new_version" >&2
    exit 1
  fi
  echo "The contract did not change ($new_version)."
  exit 0
fi

if (( new_major != old_major )) || (( old_major == 0 && new_minor != old_minor )); then
  echo "The contract went from $old_version to $new_version, a new compatible line: any change is allowed."
  exit 0
fi

# Same line: compare the shapes under one name, since `wasm-tools component targets` matches names exactly.
mkdir "$work/new-as-old"
sed -E "s/^package .*;$/$old_package/" "$work/new.wit" > "$work/new-as-old/contract.wit"
mkdir "$work/old-dir"
cp "$work/old.wit" "$work/old-dir/contract.wit"
broken=0
for world in $(sed -n -E 's/^world ([a-z0-9-]+) \{$/\1/p' "$work/old.wit"); do
  wasm-tools component embed --dummy --world "$world" "$work/old-dir" -o "$work/$world.core.wasm"
  wasm-tools component new "$work/$world.core.wasm" -o "$work/$world.wasm"
  if ! wasm-tools component targets --world "$world" "$work/new-as-old" "$work/$world.wasm" > /dev/null 2> "$work/$world.err"; then
    echo "error: a plugin built for world '$world' of $old_version does not fit $new_version:" >&2
    cat "$work/$world.err" >&2
    broken=1
  fi
done
if (( broken )); then
  if (( old_major == 0 )); then
    echo "error: this change breaks plugins built for $old_version; it needs version 0.$((old_minor + 1)).0" >&2
  else
    echo "error: this change breaks plugins built for $old_version; it needs version $((old_major + 1)).0.0" >&2
  fi
  exit 1
fi
echo "The contract went from $old_version to $new_version and every plugin built for $old_version still fits it."
