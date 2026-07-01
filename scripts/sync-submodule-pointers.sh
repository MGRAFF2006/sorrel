#!/usr/bin/env bash
#
# Sync first-party submodule checkouts to origin/main and optionally stage root
# gitlink updates. Sorrel treats sorrel-* submodules as monorepo members tracked
# on branch main (see .gitmodules), not as frozen foreign dependency pins.
#
# Usage:
#   scripts/sync-submodule-pointers.sh              # --remote + stage drift
#   scripts/sync-submodule-pointers.sh --check      # report drift only
#   scripts/sync-submodule-pointers.sh --no-fetch   # compare local HEAD to root gitlink
#
# Git note: the root tree always records a commit SHA per submodule. With
# branch = main in .gitmodules, this script follows branch tips; you commit the
# updated SHAs when you want root main to record a snapshot.

set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

CHECK_ONLY=0
NO_FETCH=0
for arg in "$@"; do
  case "$arg" in
    --check) CHECK_ONLY=1 ;;
    --no-fetch) NO_FETCH=1 ;;
    *) echo "unknown arg: $arg" >&2; exit 2 ;;
  esac
done

if [[ "$NO_FETCH" -eq 0 && "$CHECK_ONLY" -eq 0 ]]; then
  git submodule sync --recursive
  git submodule update --init --recursive
  git submodule update --remote --recursive
elif [[ "$NO_FETCH" -eq 0 && "$CHECK_ONLY" -eq 1 ]]; then
  git submodule foreach 'git fetch origin main 2>/dev/null || git fetch origin 2>/dev/null || true'
fi

drift=0

while IFS= read -r name; do
  [[ -z "$name" ]] && continue
  if [[ ! -d "$name" ]]; then
    echo "skip $name (missing)"
    continue
  fi

  local_sha="$(git -C "$name" rev-parse HEAD)"
  root_sha="$(git ls-tree HEAD "$name" 2>/dev/null | awk '{print $3}')"

  if [[ -z "$root_sha" ]]; then
    echo "drift $name: not recorded in root"
    drift=1
    [[ "$CHECK_ONLY" -eq 0 ]] && GIT_FS_MONITOR_ENABLED=false git update-index --cacheinfo "160000,$local_sha,$name"
    continue
  fi

  if [[ "$local_sha" != "$root_sha" ]]; then
    echo "drift $name: root=${root_sha:0:7} local=${local_sha:0:7}"
    drift=1
    [[ "$CHECK_ONLY" -eq 0 ]] && GIT_FS_MONITOR_ENABLED=false git update-index --cacheinfo "160000,$local_sha,$name"
  else
    echo "ok     $name @ ${local_sha:0:7}"
  fi
done < <(git config --file .gitmodules --get-regexp path | awk '{ print $2 }')

if [[ "$CHECK_ONLY" -eq 1 ]]; then
  exit "$drift"
fi

if [[ "$drift" -eq 1 ]]; then
  echo ""
  echo "Staged gitlink updates (branch-tracked snapshot). Review: git diff --cached"
else
  echo "Root gitlinks match submodule HEAD."
fi

exit 0
