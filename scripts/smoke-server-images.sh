#!/usr/bin/env bash
set -euo pipefail

api_image="${1:-sorrel-hub:smoke}"
web_image="${2:-sorrel-hub-web:smoke}"
smoke_suffix="${RANDOM}${RANDOM}"
network_name="sorrel-release-smoke-${smoke_suffix}"
volume_name="sorrel-release-smoke-${smoke_suffix}"
api_name="sorrel-hub-smoke-${smoke_suffix}"
web_name="sorrel-hub-web-smoke-${smoke_suffix}"

cleanup() {
  docker rm -f "${web_name}" "${api_name}" >/dev/null 2>&1 || true
  docker volume rm "${volume_name}" >/dev/null 2>&1 || true
  docker network rm "${network_name}" >/dev/null 2>&1 || true
}
trap cleanup EXIT

wait_for_url() {
  local container_name="$1"
  local url="$2"

  for _ in {1..30}; do
    if docker exec "${container_name}" node -e \
      "fetch('${url}').then((response) => { if (!response.ok) process.exit(1); }).catch(() => process.exit(1))" \
      >/dev/null 2>&1; then
      return 0
    fi
    sleep 1
  done

  docker logs "${container_name}" >&2 || true
  return 1
}

docker network create "${network_name}" >/dev/null
docker volume create "${volume_name}" >/dev/null

docker run -d --name "${api_name}" --network "${network_name}" \
  --network-alias hub \
  --read-only \
  --security-opt no-new-privileges:true \
  -e HOST=0.0.0.0 \
  -e SORREL_HUB_ALLOW_INSECURE_DEV_AUTH=1 \
  -v "${volume_name}:/app/data" \
  "${api_image}" >/dev/null

wait_for_url "${api_name}" http://127.0.0.1:3000/healthz
docker exec "${api_name}" node -e \
  "if (typeof process.getuid !== 'function' || process.getuid() === 0) process.exit(1)"

docker run -d --name "${web_name}" --network "${network_name}" \
  --read-only \
  --security-opt no-new-privileges:true \
  -e HUB_API_URL=http://hub:3000 \
  "${web_image}" >/dev/null

wait_for_url "${web_name}" http://127.0.0.1:5180/
wait_for_url "${web_name}" http://127.0.0.1:5180/api/healthz
docker exec "${web_name}" node -e \
  "if (typeof process.getuid !== 'function' || process.getuid() === 0) process.exit(1)"

docker stop --time 10 "${web_name}" "${api_name}" >/dev/null

echo "Server image smoke test passed for ${api_image} and ${web_image}."
