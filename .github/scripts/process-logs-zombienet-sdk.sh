#!/bin/bash

# This script processes logs produced by nodes spawned using the zombienet-sdk framework.
# The logs are prepared for upload as GitHub artifacts.
# If Loki logging is available, the corresponding log URLs are also printed.

LOKI_URL_FOR_NODE='https://grafana.teleport.parity.io/explore?orgId=1&left=%7B%22datasource%22:%22PCF9DACBDF30E12B3%22,%22queries%22:%5B%7B%22refId%22:%22A%22,%22datasource%22:%7B%22type%22:%22loki%22,%22uid%22:%22PCF9DACBDF30E12B3%22%7D,%22editorMode%22:%22code%22,%22expr%22:%22%7Bnamespace%3D%5C%22{{namespace}}%5C%22,pod%3D%5C%22{{podName}}%5C%22%7D%22,%22queryType%22:%22range%22%7D%5D,%22range%22:%7B%22from%22:%22{{from}}%22,%22to%22:%22{{to}}%22%7D%7D'

BASE_DIR=$(ls -dt /tmp/zombie-* | head -1)
ZOMBIE_JSON="$BASE_DIR/zombie.json"

if [[ ! -f "$ZOMBIE_JSON" ]]; then
  echo "Zombie file $ZOMBIE_JSON not present"
  exit 1
fi

# Extract namespace
NS=$(jq -r '.ns' "$ZOMBIE_JSON")
# test start time in milliseconds
FROM=$(jq -r '.start_time_ts' "$ZOMBIE_JSON")
# current time in milliseconds
TO=$(date +%s%3N)

make_url() {
  local name="$1"
  local url="${LOKI_URL_FOR_NODE//\{\{namespace\}\}/$NS}"
  url="${url//\{\{podName\}\}/$name}"
  url="${url//\{\{from\}\}/$FROM}"
  url="${url//\{\{to\}\}/$TO}"
  echo "$url"
}

# Make sure target directory exists
TARGET_DIR="$BASE_DIR/logs"
mkdir -p "$TARGET_DIR"

if [[ "$ZOMBIE_PROVIDER" == "k8s" ]]; then
  echo "Relay nodes:"
  jq -r '.relay.nodes[].name' "$ZOMBIE_JSON" | while read -r name; do
    # Fetching logs from k8s
    if ! kubectl logs "$name" -c "$name" -n "$NS" > "$TARGET_DIR/$name.log" ; then
      echo "::warning ::Failed to fetch logs for $name"
    fi
    echo -e "\t$name: $(make_url "$name")"
  done
  echo ""

  # Handle parachains grouped by paraId
  jq -r '.parachains | to_entries[] | "\(.key)"' "$ZOMBIE_JSON" | while read -r para_id; do
    echo "ParaId: $para_id"
    jq -r --arg pid "$para_id" '.parachains[$pid][] .collators[].name' "$ZOMBIE_JSON" | while read -r name; do
      # Fetching logs from k8s
      if ! kubectl logs "$name" -c "$name" -n "$NS" > "$TARGET_DIR/$name.log" ; then
        echo "::warning ::Failed to fetch logs for $name"
      fi
      echo -e "\t$name: $(make_url "$name")"
    done
    echo ""
  done
else
  jq -r '[.relay.nodes[].name] + [.parachains[][] .collators[].name] | .[]' "$ZOMBIE_JSON" | while read -r name; do
    cp "$BASE_DIR/$name/$name.log" "$TARGET_DIR/$name.log"
  done
fi

