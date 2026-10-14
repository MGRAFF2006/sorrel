#!/usr/bin/env bash
set -euo pipefail
umask 077

checkout=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)
cd "$checkout"
: "${SORREL_HUB_DATA_PATH:?Set the persistent Hub data directory}"
: "${RESTIC_REPOSITORY:?Use an existing encrypted Restic repository}"
: "${RESTIC_PASSWORD_FILE:?Set the existing Restic password-file path}"
test -d "$SORREL_HUB_DATA_PATH"
test -f "$RESTIC_PASSWORD_FILE"
mkdir -p deploy/backups
exec 9>"${SORREL_BACKUP_LOCK:-$checkout/deploy/backups/backup.lock}"
flock -n 9 || { printf 'Another backup is active\n' >&2; exit 1; }

compose=(docker compose --env-file deploy/.env -f deploy/sorrel-server.compose.yml -f deploy/sorrel-hosted.compose.yml)
if [[ ${SORREL_PILOT_TUNNEL:-0} == 1 ]]; then
  compose+=(-f deploy/sorrel-tunnel.compose.yml)
fi
dc() { "${compose[@]}" "$@" </dev/null; }
dc config --quiet
running=$(dc ps --status running --services)
for service in hub hub-web; do
  [[ $'\n'"$running"$'\n' == *$'\n'"$service"$'\n'* ]] || {
    printf 'Start %s before backing up the pilot\n' "$service" >&2
    exit 1
  }
done
paused=0
resume() {
  local result=$?
  if [[ $paused == 1 ]] && ! dc up -d --no-build --wait --wait-timeout 60 hub hub-web; then
    printf 'Backup could not resume Sorrel; inspect the containers\n' >&2
    result=1
  fi
  return "$result"
}
trap resume EXIT
paused=1
dc stop hub-web hub
restic --no-cache backup "$SORREL_HUB_DATA_PATH" "$checkout" --tag sorrel-pilot \
  --exclude '**/.git' --exclude '**/node_modules' --exclude '**/target' \
  --exclude "$checkout/deploy/backups"
dc up -d --no-build --wait --wait-timeout 60 hub hub-web
paused=0
printf 'Encrypted Sorrel backup completed; Hub resumed\n'
