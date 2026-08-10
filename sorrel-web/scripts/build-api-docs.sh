#!/usr/bin/env bash
# Build the generated API reference served under /api/.
#
# rustdoc output is generated from doc comments in the source — it is never
# committed. CI runs this before deploying the site; locally you can run it
# from a checkout where the engine repo is available.
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

echo "Building rustdoc for sorrel-core ($core_dir)..."
cargo doc --no-deps --manifest-path "$core_dir/Cargo.toml"

out="$site_dir/api/sorrel-core"
rm -rf "$out"
mkdir -p "$out"
cp -R "$core_dir/target/doc/." "$out/"

echo "API docs ready: $out"
echo "Entry point: api/sorrel-core/sorrel_core/index.html"
