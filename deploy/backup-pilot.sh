#!/usr/bin/env bash
set -euo pipefail
umask 077

checkout=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)
cd "$checkout"
: "${RESTIC_REPOSITORY:?Use an existing encrypted Restic repository}"
: "${RESTIC_PASSWORD_FILE:?Set the existing Restic password-file path}"
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
hub_container=$(dc ps -q hub)
[[ -n $hub_container && $hub_container != *$'\n'* ]] || {
  printf 'Expected exactly one running Hub container\n' >&2
  exit 1
}
data_source=$(docker inspect --format '{{range .Mounts}}{{if eq .Destination "/app/data"}}{{.Source}}{{end}}{{end}}' "$hub_container")
[[ -n $data_source && -d $data_source ]] || {
  printf 'Hub data mount is not a readable local directory\n' >&2
  exit 1
}
data_source=$(realpath -e -- "$data_source")
if [[ -n ${SORREL_HUB_DATA_PATH:-} && $(realpath -e -- "$SORREL_HUB_DATA_PATH") != "$data_source" ]]; then
  printf 'SORREL_HUB_DATA_PATH does not match the running Hub data mount\n' >&2
  exit 1
fi
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
restic --no-cache backup "$data_source" "$checkout" --tag sorrel-pilot \
  --exclude '**/.git' --exclude '**/node_modules' --exclude '**/target' \
  --exclude "$checkout/deploy/backups"
dc up -d --no-build --wait --wait-timeout 60 hub hub-web
paused=0
printf 'Encrypted Sorrel backup completed; Hub resumed\n'
