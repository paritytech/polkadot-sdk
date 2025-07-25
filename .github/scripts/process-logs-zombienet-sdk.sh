#!/bin/bash

# This script processes logs produced by nodes spawned using the zombienet-sdk framework.
# The logs are prepared for upload as GitHub artifacts.
# If Loki logging is available, the corresponding log URLs are also printed.
# NOTE: P2838773B5F7DE937 is the loki.cicd until we switch to loki.zombienet
LOKI_URL_FOR_NODE='https://grafana.teleport.parity.io/explore?orgId=1&left=%7B%22datasource%22:%22P2838773B5F7DE937%22,%22queries%22:%5B%7B%22refId%22:%22A%22,%22datasource%22:%7B%22type%22:%22loki%22,%22uid%22:%22P2838773B5F7DE937%22%7D,%22editorMode%22:%22code%22,%22expr%22:%22%7Bzombie_ns%3D%5C%22{{namespace}}%5C%22,zombie_node%3D%5C%22{{podName}}%5C%22%7D%22,%22queryType%22:%22range%22%7D%5D,%22range%22:%7B%22from%22:%22{{from}}%22,%22to%22:%22{{to}}%22%7D%7D'

BASE_DIR=$(ls -dt /tmp/zombie-* | head -1)
ZOMBIE_JSON="$BASE_DIR/zombie.json"

LOKI_DIR_FOR_NATIVE_LOGS="/tmp/zombienet"

JQ_QUERY_RELAY_V1='.relay[].name'
JQ_QUERY_RELAY_SDK='.relay.nodes[].name'


JQ_QUERY_PARA_NODES_V1='.paras[$pid].nodes[].name'
JQ_QUERY_PARA_NODES_SDK='.parachains[$pid][] .collators[].name'


if [[ ! -f "$ZOMBIE_JSON" ]]; then
  echo "Zombie file $ZOMBIE_JSON not present"
  exit 1
fi

# Extract namespace (ns in sdk / namespace in v1)
NS=$(jq -r '.ns // .namespace' "$ZOMBIE_JSON")
# test start time in milliseconds
FROM=$(jq -r '.start_time_ts' "$ZOMBIE_JSON")
# current time in milliseconds + 60 secs to allow loki to ingest logs
TO=$(($(date +%s%3N) + 60000))

make_url() {
  local name="$1"
  local to="$2"
  local url="${LOKI_URL_FOR_NODE//\{\{namespace\}\}/$NS}"
  url="${url//\{\{podName\}\}/$name}"
  url="${url//\{\{from\}\}/$FROM}"
  url="${url//\{\{to\}\}/$to}"
  echo "$url"
}

# Make sure target directory exists
TARGET_DIR="$BASE_DIR/logs"
mkdir -p "$TARGET_DIR"

echo "Relay nodes:"

JQ_QUERY_RELAY=$JQ_QUERY_RELAY_V1
JQ_QUERY_PARA_NODES=$JQ_QUERY_PARA_NODES_V1
if [[ $(echo "$NS" | grep -E "zombie-[A-Fa-f0-9]+-") ]]; then
    JQ_QUERY_RELAY=$JQ_QUERY_RELAY_SDK
    JQ_QUERY_PARA_NODES=$JQ_QUERY_PARA_NODES_SDK
fi;

jq -r $JQ_QUERY_RELAY "$ZOMBIE_JSON" | while read -r name; do
  local_to=$TO
  if [[ "$ZOMBIE_PROVIDER" == "k8s" ]]; then
    # Fetching logs from k8s
    if ! kubectl logs "$name" -c "$name" -n "$NS" > "$TARGET_DIR/$name.log" ; then
      echo "::warning ::Failed to fetch logs for $name"
    fi
  else
    # zombienet v1 dump the logs to the `/logs` directory
    if [ ! -f "$TARGET_DIR/$name.log" ]; then
      # `sdk` use this pattern to store the logs in native provider
      cp "$BASE_DIR/$name/$name.log" "$TARGET_DIR/$name.log"
    fi

    # send logs to loki
    if [ -d "$LOKI_DIR_FOR_NATIVE_LOGS" ]; then
      awk -v NS="$NS" -v NAME="$name" '{print NS" "NAME" " $0}' $TARGET_DIR/$name.log >> $LOKI_DIR_FOR_NATIVE_LOGS/to-loki.log
      local_to=$(($(date +%s%3N) + 60000))
    fi
  fi
  echo -e "\t$name: $(make_url "$name" "$local_to")"
done
echo ""

# Handle parachains grouped by paraId
jq -r '.paras // .parachains | to_entries[] | "\(.key)"' "$ZOMBIE_JSON" | while read -r para_id; do
  echo "ParaId: $para_id"
  jq -r --arg pid "$para_id" "$JQ_QUERY_PARA_NODES" "$ZOMBIE_JSON" | while read -r name; do
    local_to=$TO
    if [[ "$ZOMBIE_PROVIDER" == "k8s" ]]; then
      # Fetching logs from k8s
      if ! kubectl logs "$name" -c "$name" -n "$NS" > "$TARGET_DIR/$name.log" ; then
        echo "::warning ::Failed to fetch logs for $name"
      fi
    else
      # zombienet v1 dump the logs to the `/logs` directory
      if [ ! -f "$TARGET_DIR/$name.log" ]; then
        # `sdk` use this pattern to store the logs in native provider
        cp "$BASE_DIR/$name/$name.log" "$TARGET_DIR/$name.log"
      fi

      # send logs to loki
      if [ -d "$LOKI_DIR_FOR_NATIVE_LOGS" ]; then
        awk -v NS="$NS" -v NAME="$name" '{print NS" "NAME" " $0}' $TARGET_DIR/$name.log >> $LOKI_DIR_FOR_NATIVE_LOGS/to-loki.log
        local_to=$(($(date +%s%3N) + 60000))
      fi
    fi
    echo -e "\t$name: $(make_url "$name" "$local_to")"
  done
  echo ""
done
