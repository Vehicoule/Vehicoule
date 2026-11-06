#!/usr/bin/env bash
# Builds the plugin host's test plugins (crates/vehicoule-plugin-host/fixtures) into components.
#
# Usage: scripts/build-plugin-fixtures.sh OUT_DIR
# Needs the `wasm32-unknown-unknown` target of the pinned toolchain and `wasm-tools` (version in docs/references.md).
# The host's tests read the components from the directory named by VEHICOULE_PLUGIN_FIXTURES.
set -euo pipefail

out="${1:?usage: scripts/build-plugin-fixtures.sh OUT_DIR}"
root="$(cd "$(dirname "$0")/.." && pwd)"
crate="$root/crates/vehicoule-plugin-host/fixtures"
mkdir -p "$out"

features=(
  good
  extra-import
  endless-loop
  memory-bomb
  trap
  fetch-in-describe
  declares-missing-export
  undeclared-export
  bad-host-pattern
)

for feature in "${features[@]}"; do
  cargo build --manifest-path "$crate/Cargo.toml" --locked --release --target wasm32-unknown-unknown \
    --no-default-features --features "$feature"
  wasm-tools component new "$crate/target/wasm32-unknown-unknown/release/vehicoule_plugin_fixtures.wasm" \
    -o "$out/$feature.wasm"
done

# Plugins the Rust toolchain cannot produce, written as core modules in WAT.
for wat in "$crate"/wat/*.wat; do
  name="$(basename "$wat" .wat)"
  wasm-tools component embed "$root/wit" --world vehicoule:plugin/base@0.1.0 "$wat" -o "$out/$name.core.wasm"
  wasm-tools component new "$out/$name.core.wasm" -o "$out/$name.wasm"
  rm "$out/$name.core.wasm"
done
