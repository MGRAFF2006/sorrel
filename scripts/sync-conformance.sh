#!/usr/bin/env bash
#
# Optional monorepo helper: copy the canonical policy conformance manifest and
# its sidecar metadata from sorrel-protocol into every consumer's vendored
# conformance directory, so vendored copies cannot drift.
#
# This is a convenience for side-by-side checkouts (e.g. this root monorepo with
# its submodules). Normal package use does NOT require it: each consumer also
# guards drift on its own via a sidecar-checksum test, and a maintainer can copy
# the two files by hand. The actual copy/regeneration logic lives in
# sorrel-protocol/scripts/export-conformance.mjs; this script just fans it out.
#
# Usage:
#   scripts/sync-conformance.sh            # export into all consumers
#   scripts/sync-conformance.sh --check    # report which consumers are out of sync
#
# After exporting, re-run each touched consumer's tests:
#   sorrel-core/sorrel-cli/sorrel-runners:  cargo test [--workspace]
#   sorrel-hub/sorrel-vault:                npm test

set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
PROTOCOL_DIR="$ROOT_DIR/sorrel-protocol"
EXPORTER="$PROTOCOL_DIR/scripts/export-conformance.mjs"
CANON="$PROTOCOL_DIR/conformance/policy-conformance.json"

# Consumer vendored conformance directories, relative to the root.
CONSUMERS=(
  "sorrel-core/tests/conformance"
  "sorrel-cli/tests/conformance"
  "sorrel-runners/tests/conformance"
  "sorrel-hub/test/conformance"
  "sorrel-vault/tests/conformance"
)

CHECK_ONLY=0
if [[ "${1:-}" == "--check" ]]; then
  CHECK_ONLY=1
fi

if [[ ! -f "$EXPORTER" ]]; then
  echo "error: missing $EXPORTER (is the sorrel-protocol submodule checked out?)" >&2
  exit 1
fi

if [[ "$CHECK_ONLY" == "1" ]]; then
  canon_sum="$(sha256sum "$CANON" | awk '{print $1}')"
  drift=0
  for rel in "${CONSUMERS[@]}"; do
    target="$ROOT_DIR/$rel/policy-conformance.json"
    if [[ ! -f "$target" ]]; then
      echo "MISSING  $rel/policy-conformance.json"
      drift=1
      continue
    fi
    sum="$(sha256sum "$target" | awk '{print $1}')"
    if [[ "$sum" == "$canon_sum" ]]; then
      echo "ok       $rel"
    else
      echo "DRIFT    $rel"
      drift=1
    fi
  done
  if [[ "$drift" == "1" ]]; then
    echo "one or more consumers are out of sync; run scripts/sync-conformance.sh" >&2
    exit 1
  fi
  echo "all consumers match the canonical manifest ($canon_sum)"
  exit 0
fi

for rel in "${CONSUMERS[@]}"; do
  echo "==> $rel"
  node "$EXPORTER" "$ROOT_DIR/$rel"
done

echo "done. Re-run each touched consumer's tests to enforce the new vectors."
