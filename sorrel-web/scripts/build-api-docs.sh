#!/usr/bin/env bash
# Build the generated API reference served under /api/.
#
# rustdoc output is generated from doc comments in the source — it is never
# committed. Run this locally from a checkout where the engine is available;
# the static production deployment does not build or publish this output.
#
# Usage:
#   scripts/build-api-docs.sh [path-to-sorrel-core]
#
# The engine checkout defaults to ../sorrel-core (the root monorepo layout).
set -euo pipefail

site_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
core_dir="${1:-$site_dir/../sorrel-core}"

if [[ ! -f "$core_dir/Cargo.toml" ]]; then
  echo "error: sorrel-core checkout not found at $core_dir" >&2
  echo "usage: scripts/build-api-docs.sh [path-to-sorrel-core]" >&2
  exit 1
fi

target_dir="${CARGO_TARGET_DIR:-$site_dir/../target}"

echo "Building rustdoc for sorrel-core ($core_dir)..."
cargo doc --no-deps --manifest-path "$core_dir/Cargo.toml" --target-dir "$target_dir"

out="$site_dir/api/sorrel-core"
rm -rf "$out"
mkdir -p "$out"
cp -R "$target_dir/doc/." "$out/"

echo "API docs ready: $out"
echo "Entry point: api/sorrel-core/sorrel_core/index.html"
