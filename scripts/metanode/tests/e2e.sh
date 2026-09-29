#!/usr/bin/env bash
# Tier 2: a four-machine local network, every node run by the metanode units.
#
#   host-a  ubuntu-24.04  validator, asset-hub, people (alice)   collators use the local RPC
#   host-b  debian-12     validator (bob), RPC_PRIVATE_IP       serves collators on host-c
#   host-c  debian-13     asset-hub, people (bob)                remote relay RPC to host-b
#   host-d  ubuntu-22.04  asset-hub (charlie, follows only)     embedded relay node
#
# The relay chain is westend-local with Asset Hub and People registered at genesis; Alice and Bob
# are the genesis validators and the parachains' invulnerable collators.
#
# Usage: BIN_DIR=<node binaries> e2e.sh
# shellcheck disable=SC2329 # the check_* functions are called through check()
set -euo pipefail
# shellcheck source-path=SCRIPTDIR source=lib.sh
. "$(dirname "$0")/lib.sh"
: "${BIN_DIR:?BIN_DIR must point at the node binaries}"

NETWORK_NAME=polkadot-metanode-e2e
SUBNET=10.213.57.0/24
HOSTS=(host-a host-b host-c host-d)
declare -A IP=([host-a]=10.213.57.10 [host-b]=10.213.57.11 [host-c]=10.213.57.12 [host-d]=10.213.57.13)
declare -A DISTRO=([host-a]=ubuntu-24.04 [host-b]=debian-12 [host-c]=debian-13 [host-d]=ubuntu-22.04)
declare -A HOST_ROLES=([host-a]="validator asset-hub people" [host-b]=validator
  [host-c]="asset-hub people" [host-d]=asset-hub)
declare -A SEED=([host-a]=alice [host-b]=bob [host-c]=bob [host-d]=charlie)
declare -A P2P=([validator]=30333 [asset-hub]=30343 [people]=30353)
declare -A RPC=([validator]=9944 [asset-hub]=9954 [people]=9964)
CHAINS=/opt/metanode/chains

work=$(mktemp -d)
# The containers mount the chain specs from here, so --keep keeps it too.
if [ "${KEEP:-0}" = 1 ]; then
  log "work directory (chain specs, node keys): $work"
else
  trap 'rm -rf "$work"' EXIT
fi

cn() { echo "pm-e2e-$1"; }
on() {
  local host=$1
  shift
  docker exec "$(cn "$host")" "$@"
}

# ---- setup -------------------------------------------------------------------------------------

declare -A PEER
generate_node_keys() {
  local host role key
  mkdir -p "$work/keys"
  for host in "${HOSTS[@]}"; do
    for role in ${HOST_ROLES[$host]}; do
      key=$work/keys/$host-$role
      "$BIN_DIR/polkadot" key generate-node-key --file "$key" 2>/dev/null
      PEER[$host-$role]=$("$BIN_DIR/polkadot" key inspect-node-key --file "$key")
    done
  done
}

bootnode() {
  echo "/ip4/${IP[$1]}/tcp/${P2P[$2]}/p2p/${PEER[$1-$2]}"
}

write_config() {
  local host=$1 role urls=
  case "$host" in
    host-c) urls="ws://${IP[host-b]}:9944" ;;
    host-d) urls='""' ;;
  esac
  on "$host" bash -c "cat > /etc/polkadot-metanode/metanode.conf <<EOF
NETWORK=westend
NODE_NAME=$host
${urls:+RELAY_CHAIN_RPC_URLS=$urls}
EOF"
  for role in ${HOST_ROLES[$host]}; do
    if [ "$role" = validator ]; then
      on "$host" bash -c "cat > /etc/polkadot-metanode/validator.conf <<EOF
CHAIN=$CHAINS/westend-local.json
EXTRA_ARGS=\"--${SEED[$host]}\"
$([ "$host" != host-b ] || echo "RPC_PRIVATE_IP=${IP[host-b]}")
EOF"
    else
      on "$host" bash -c "cat > /etc/polkadot-metanode/$role.conf <<EOF
CHAIN=$CHAINS/$role-westend-local.json
RELAY_CHAIN=$CHAINS/westend-local.json
EXTRA_ARGS=\"--${SEED[$host]}\"
EOF"
    fi
  done
}

# Puts the pre-generated node key in place before the first start, so the bootnodes in the chain
# specs are right.
install_node_keys() {
  local host=$1 role
  for role in ${HOST_ROLES[$host]}; do
    on "$host" install -d -o "polkadot-$role" -g "polkadot-$role" -m 0700 "/var/lib/polkadot-$role"
    docker cp "$work/keys/$host-$role" "$(cn "$host"):/var/lib/polkadot-$role/node-key"
    on "$host" chown "polkadot-$role:polkadot-$role" "/var/lib/polkadot-$role/node-key"
    on "$host" chmod 0600 "/var/lib/polkadot-$role/node-key"
  done
}

setup() {
  local host distro roles enable other
  # Container names and the subnet are fixed, so there is one e2e network per docker daemon.
  if other=$(docker network inspect -f "{{index .Labels \"$LABEL.run\"}}" "$NETWORK_NAME" 2>/dev/null); then
    die "docker network $NETWORK_NAME exists (e2e run $other, running or kept with --keep); remove it:" \
      "docker rm -f \$(docker ps -aq --filter label=$LABEL.run=$other); docker network rm $NETWORK_NAME"
  fi
  generate_node_keys
  for distro in $(printf '%s\n' "${DISTRO[@]}" | sort -u); do build_image "$distro"; done
  # Generate the specs inside the ubuntu-24.04 image: they need jq >= 1.7 (see gen-local-specs.sh).
  log "generating chain specs"
  mkdir -p "$work/chains"
  docker run --rm --user "$(id -u):$(id -g)" -e HOME=/tmp --entrypoint /opt/metanode/src/tests/gen-local-specs.sh \
    -v "$BIN_DIR:/opt/metanode/bin:ro" -v "$METANODE_SRC:/opt/metanode/src:ro" -v "$work/chains:/out" \
    "$IMAGE:ubuntu-24.04" /opt/metanode/bin /out \
    "$(bootnode host-a validator),$(bootnode host-b validator)" \
    "$(bootnode host-a asset-hub),$(bootnode host-c asset-hub)" \
    "$(bootnode host-a people),$(bootnode host-c people)"
  chmod -R a+rX "$work"
  docker network create --label "$LABEL" --label "$LABEL.run=$RUN_ID" --subnet "$SUBNET" \
    "$NETWORK_NAME" >/dev/null
  for host in "${HOSTS[@]}"; do
    start_container "$(cn "$host")" "${DISTRO[$host]}" --network "$NETWORK_NAME" --ip "${IP[$host]}" \
      -v "$work/chains:$CHAINS:ro"
    on "$host" /opt/metanode/src/install.sh --bin-dir /opt/metanode/bin >/dev/null
    write_config "$host"
    install_node_keys "$host"
  done
  for host in "${HOSTS[@]}"; do
    roles=${HOST_ROLES[$host]}
    # host-a runs every role: through polkadot-metanode-all.target; the others enable theirs.
    enable=${roles// /,}
    [ "$host" != host-a ] || enable=all
    on "$host" /opt/metanode/src/install.sh --bin-dir /opt/metanode/bin --enable "$enable" >/dev/null
    log "$host (${DISTRO[$host]}): --enable $enable"
  done
}

# ---- assertions --------------------------------------------------------------------------------

# authored_since <host> <role> <since>: the collator authored a block after <since>.
authored_since() {
  journal_has "$(cn "$1")" "polkadot-$2.service" "$3" "Prepared block for proposing"
}

relay_finalized() {
  reached_block "$(cn "$1")" 9944 "$2" finalized
}

para_finalized() {
  reached_block "$(cn host-a)" "${RPC[$1]}" "$2" finalized
}

now() {
  on "$1" date '+%Y-%m-%d %H:%M:%S'
}

follower_in_sync() {
  local tip mine
  tip=$(best_number "$(cn host-a)" 9954) || return 1
  mine=$(best_number "$(cn host-d)" 9954) || return 1
  [ "$tip" -gt 2 ] && [ $((tip - mine)) -le 3 ]
}

# A node in a restart loop can still catch up between two crashes, so the follower must also not
# have been restarted.
check_follower() {
  local restarts
  wait_until 600 "host-d in sync" follower_in_sync || return 1
  restarts=$(unit_prop "$(cn host-d)" polkadot-asset-hub.service NRestarts)
  [ "$restarts" = 0 ] || {
    log "host-d asset-hub was restarted $restarts time(s)"
    return 1
  }
}

check_parachains_author() {
  local start=$1 host role
  for role in asset-hub people; do
    wait_until 900 "$role finalizes" para_finalized "$role" 3 || return 1
    for host in host-a host-c; do
      wait_until 600 "$role on $host authors" authored_since "$host" "$role" "$start" || return 1
      log "  $role authored by $host (${SEED[$host]})"
    done
  done
}

# Timestamps for "authored since" are taken once the disruption is over, so blocks authored just
# before it cannot satisfy the check.
check_validator_crash() {
  local restarts pid since
  restarts=$(unit_prop "$(cn host-c)" polkadot-asset-hub.service NRestarts)
  pid=$(unit_prop "$(cn host-b)" polkadot-validator.service MainPID)
  on host-b kill -9 "$pid"
  log "  killed the validator on host-b; systemd restarts it after RestartSec=120"
  wait_until 400 "host-b validator restarted" \
    role_serving "$(cn host-b)" validator 9944 || return 1
  [ "$(unit_prop "$(cn host-b)" polkadot-validator.service NRestarts)" -ge 1 ] || return 1
  # host-c's collator lost its relay RPC meanwhile: its node exited and systemd restarted it.
  [ "$(unit_prop "$(cn host-c)" polkadot-asset-hub.service NRestarts)" -gt "$restarts" ] || {
    log "host-c asset-hub was not restarted while its relay RPC was down"
    return 1
  }
  since=$(now host-c)
  wait_until 900 "host-c authors again" authored_since host-c asset-hub "$since"
}

check_collator_host_reboot() {
  local since
  # Give systemd time to stop the nodes cleanly, as a real reboot would.
  docker restart --time 60 "$(cn host-c)" >/dev/null
  wait_until 90 "host-c boots" systemd_up "$(cn host-c)" || return 1
  since=$(now host-c)
  wait_until 900 "host-c authors after reboot" authored_since host-c people "$since"
}

check_target_restart() {
  local before since
  on host-a systemctl restart polkadot-metanode.target
  wait_until 600 "host-a validator back" role_serving "$(cn host-a)" validator 9944 || return 1
  since=$(now host-a)
  before=$(finalized_number "$(cn host-b)" 9944) || return 1
  wait_until 600 "relay finality resumes" relay_finalized host-b $((before + 3)) || return 1
  wait_until 900 "host-a authors after the restart" authored_since host-a asset-hub "$since"
}

# ---- run ---------------------------------------------------------------------------------------

setup
start=$(now host-a)
check "e2e: relay chain finalizes (host-a)" wait_until 600 "relay finality" relay_finalized host-a 3
check "e2e: relay chain finalizes (host-b)" wait_until 600 "relay finality" relay_finalized host-b 3
check "e2e: Asset Hub and People finalize, authored on host-a and host-c" check_parachains_author "$start"
check "e2e: embedded-relay follower on host-d keeps up with Asset Hub, without restarts" \
  check_follower
check "e2e: validator crash on host-b: restarted, host-c collators resume" check_validator_crash
check "e2e: reboot of the collator host host-c" check_collator_host_reboot
check "e2e: systemctl restart polkadot-metanode.target on host-a" check_target_restart
if [ "$FAILURES" -gt 0 ]; then
  for host in "${HOSTS[@]}"; do
    for role in ${HOST_ROLES[$host]}; do
      log "--- $host polkadot-$role (last lines)"
      on "$host" journalctl -u "polkadot-$role.service" --no-pager -o cat -n 15 || true
    done
  done
fi
log "e2e: $FAILURES failure(s)"
exit $((FAILURES > 0))
