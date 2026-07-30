#!/usr/bin/env bash
#
# Compare root gitlinks with each submodule's origin/main, or stage updates.
# Active submodule branches and working trees are never changed.
#
# Usage:
#   scripts/sync-submodule-pointers.sh
#   scripts/sync-submodule-pointers.sh --check
#   scripts/sync-submodule-pointers.sh --no-fetch [--check]

set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

check_only=0
no_fetch=0
for arg in "$@"; do
  case "$arg" in
    --check) check_only=1 ;;
    --no-fetch) no_fetch=1 ;;
    *)
      echo "unknown argument: $arg" >&2
      exit 2
      ;;
  esac
done

drift=0

while IFS= read -r path; do
  [[ -z "$path" ]] && continue

  if ! git -C "$path" rev-parse --git-dir >/dev/null 2>&1; then
    if [[ "$no_fetch" -eq 1 ]]; then
      echo "error  $path: submodule is not initialized" >&2
      drift=1
      continue
    fi
    git submodule update --init --recursive -- "$path"
  fi

  if [[ "$no_fetch" -eq 0 ]]; then
    git -C "$path" fetch --quiet origin \
      "main:refs/remotes/origin/main"
  fi

  if ! target_sha="$(git -C "$path" rev-parse refs/remotes/origin/main 2>/dev/null)"; then
    echo "error  $path: origin/main is unavailable" >&2
    drift=1
    continue
  fi

  root_sha="$(git ls-tree HEAD -- "$path" | awk '{print $3}')"
  if [[ "$root_sha" == "$target_sha" ]]; then
    echo "ok     $path @ ${target_sha:0:7}"
    continue
  fi

  echo "drift  $path: root=${root_sha:0:7} main=${target_sha:0:7}"
  drift=1
  if [[ "$check_only" -eq 0 ]]; then
    git update-index --cacheinfo "160000,$target_sha,$path"
  fi
done < <(git config --file .gitmodules --get-regexp path | awk '{print $2}')

if [[ "$check_only" -eq 1 ]]; then
  exit "$drift"
fi

if [[ "$drift" -eq 1 ]]; then
  echo
  echo "Staged gitlink updates. Review with:"
  echo "  git diff --cached --submodule=short"
else
  echo "Root gitlinks already match all origin/main branches."
fi
