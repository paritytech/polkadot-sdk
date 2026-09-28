#!/usr/bin/env bash
# Tier 2: a four-machine local network, every node run by the metanode units.
#
#   host-a  ubuntu-24.04  validator, asset-hub, people (alice)   collators use the local RPC;
#                         RPC_LISTEN_IP=0.0.0.0, bandwidth reservation (UPLINK_RATE=1gbit)
#   host-b  debian-12     validator (bob), RPC_LISTEN_IP       serves collators on host-c
#   host-c  debian-13     asset-hub, people (bob)                relay RPC: host-b, then host-a
#   host-d  ubuntu-22.04  asset-hub (charlie, follows only)     embedded relay node
#   paseo   ubuntu-24.04  nothing enabled                        bootstrap install (curl | bash) of
#                                                                NETWORK=paseo: chain spec download
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
GATEWAY=10.213.57.1
HOSTS=(host-a host-b host-c host-d)
declare -A IP=([host-a]=10.213.57.10 [host-b]=10.213.57.11 [host-c]=10.213.57.12 [host-d]=10.213.57.13
  [paseo]=10.213.57.14)
declare -A DISTRO=([host-a]=ubuntu-24.04 [host-b]=debian-12 [host-c]=debian-13 [host-d]=ubuntu-22.04
  [paseo]=ubuntu-24.04)
declare -A HOST_ROLES=([host-a]="validator asset-hub-collator people-collator" [host-b]=validator
  [host-c]="asset-hub-collator people-collator" [host-d]=asset-hub-collator)
declare -A SEED=([host-a]=alice [host-b]=bob [host-c]=bob [host-d]=charlie)
declare -A P2P=([validator]=30333 [asset-hub-collator]=30334 [people-collator]=30336)
# The localhost RPC of every role (the unsafe endpoint, always on 127.0.0.1).
declare -A RPC=([validator]=9945 [asset-hub-collator]=9947 [people-collator]=9949)
CHAINS=/opt/metanode/chains
# Serves scripts/metanode to the bootstrap install, on the network's gateway.
HTTP_PORT=18733

work=$(mktemp -d)
# The containers mount the chain specs from here, so --keep keeps it too.
if [ "${KEEP:-0}" = 1 ]; then
  log "work directory (chain specs, node keys): $work"
fi
http_pid=
cleanup_e2e() {
  [ -z "$http_pid" ] || kill "$http_pid" 2>/dev/null || true
  [ "${KEEP:-0}" = 1 ] || rm -rf "$work"
}
trap cleanup_e2e EXIT

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

# metanode.env, the same on every host but for the relay chain RPC of the collators, and each role's
# own file: its dev key, and the validators' RPC address.
write_config() {
  local host=$1 role urls=ws://127.0.0.1:9944 listen=''
  case "$host" in
    # host-b's validator first, host-a's as the fallback.
    host-c) urls="ws://${IP[host-b]}:9944 ws://${IP[host-a]}:9944" ;;
    # None: an embedded relay chain node.
    host-d) urls='' ;;
  esac
  case "$host" in
    host-a) listen=0.0.0.0 ;;
    host-b) listen=${IP[host-b]} ;;
  esac
  on "$host" bash -c "cat > /etc/polkadot-metanode/metanode.env <<EOF
NETWORK=westend
RELAY_CHAIN=$CHAINS/westend-local.json
ASSET_HUB_CHAIN=$CHAINS/asset-hub-westend-local.json
PEOPLE_CHAIN=$CHAINS/people-westend-local.json
NODE_NAME=$host
RELAY_CHAIN_RPC_URLS=$urls
EOF"
  for role in ${HOST_ROLES[$host]}; do
    on "$host" bash -c "echo EXTRA_ARGS=--${SEED[$host]} > /etc/polkadot-metanode/polkadot-$role.env"
    if [ "$role" = validator ] && [ -n "$listen" ]; then
      on "$host" bash -c "echo RPC_LISTEN_IP=$listen >> /etc/polkadot-metanode/polkadot-$role.env"
    fi
  done
}

# Puts the pre-generated node key in place before the first start, so the bootnodes in the chain
# specs are right.
install_node_keys() {
  local host=$1 role
  for role in ${HOST_ROLES[$host]}; do
    on "$host" install -d -o "polkadot-$role" -g "polkadot-$role" -m 0700 "/var/lib/polkadot-$role"
    docker cp "$work/keys/$host-$role" "$(cn "$host"):/var/lib/polkadot-$role/node.key"
    on "$host" chown "polkadot-$role:polkadot-$role" "/var/lib/polkadot-$role/node.key"
    on "$host" chmod 0600 "/var/lib/polkadot-$role/node.key"
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
    "$(bootnode host-a asset-hub-collator),$(bootnode host-c asset-hub-collator)" \
    "$(bootnode host-a people-collator),$(bootnode host-c people-collator)"
  chmod -R a+rX "$work"
  docker network create --label "$LABEL" --label "$LABEL.run=$RUN_ID" --subnet "$SUBNET" \
    --gateway "$GATEWAY" "$NETWORK_NAME" >/dev/null
  for host in "${HOSTS[@]}"; do
    start_container "$(cn "$host")" "${DISTRO[$host]}" --network "$NETWORK_NAME" --ip "${IP[$host]}" \
      -v "$work/chains:$CHAINS:ro"
    on "$host" /opt/metanode/src/install.sh --bin-dir /opt/metanode/bin >/dev/null
    write_config "$host"
    install_node_keys "$host"
  done
  for host in "${HOSTS[@]}"; do
    roles=${HOST_ROLES[$host]}
    # host-a runs every role: through polkadot-metanode-all.target, with the bandwidth reservation;
    # the others enable theirs.
    enable=${roles// /,}
    local extra=()
    if [ "$host" = host-a ]; then
      enable=all
      extra=(--uplink-rate 1gbit)
    fi
    on "$host" /opt/metanode/src/install.sh --bin-dir /opt/metanode/bin --enable "$enable" "${extra[@]}" >/dev/null
    log "$host (${DISTRO[$host]}): --enable $enable ${extra[*]}"
  done
}

# ---- assertions --------------------------------------------------------------------------------

# authored_since <host> <role> <since>: the collator authored a block after <since>.
authored_since() {
  journal_has "$(cn "$1")" "polkadot-$2.service" "$3" "Prepared block for proposing"
}

relay_finalized() {
  reached_block "$(cn "$1")" 9945 "$2" finalized
}

para_finalized() {
  reached_block "$(cn host-a)" "${RPC[$1]}" "$2" finalized
}

now() {
  on "$1" date '+%Y-%m-%d %H:%M:%S'
}

follower_in_sync() {
  local tip mine
  tip=$(best_number "$(cn host-a)" 9947) || return 1
  mine=$(best_number "$(cn host-d)" 9947) || return 1
  [ "$tip" -gt 2 ] && [ $((tip - mine)) -le 3 ]
}

# A node in a restart loop can still catch up between two crashes, so the follower must also not
# have been restarted.
check_follower() {
  local restarts
  wait_until 600 "host-d in sync" follower_in_sync || return 1
  restarts=$(unit_prop "$(cn host-d)" polkadot-asset-hub-collator.service NRestarts)
  [ "$restarts" = 0 ] || {
    log "host-d asset-hub-collator was restarted $restarts time(s)"
    return 1
  }
}

check_parachains_author() {
  local start=$1 host role
  for role in asset-hub-collator people-collator; do
    wait_until 900 "$role finalizes" para_finalized "$role" 3 || return 1
    for host in host-a host-c; do
      wait_until 600 "$role on $host authors" authored_since "$host" "$role" "$start" || return 1
      log "  $role authored by $host (${SEED[$host]})"
    done
  done
}

# relay_rpc_peer <host> <role>: the address of the relay-chain RPC endpoint the collator is
# connected to (its established connection to port 9944).
relay_rpc_peer() {
  local pid
  pid=$(unit_prop "$(cn "$1")" "polkadot-$2.service" MainPID)
  on "$1" ss -Htnp state established "dport = :9944" | grep "pid=$pid," | awk '{ print $4 }' | head -n 1
}

on_relay_rpc_of() {
  [[ "$(relay_rpc_peer "$1" "$2")" == "$3:9944" ]]
}

# Killing host-b's validator: host-c's collators fail over to their second relay-chain RPC endpoint,
# host-a's validator, without a restart, and keep authoring.
check_validator_crash() {
  local restarts pid since
  # The collator may have started while host-b's RPC was still coming up, and then use host-a:
  # without polkadot-sdk#13525 it does not go back by itself. A restart tries host-b first again.
  if ! on_relay_rpc_of host-c asset-hub-collator "${IP[host-b]}"; then
    on host-c systemctl restart polkadot-asset-hub-collator.service
  fi
  wait_until 180 "host-c asset-hub-collator on host-b's RPC" on_relay_rpc_of host-c asset-hub-collator \
    "${IP[host-b]}" || return 1
  restarts=$(unit_prop "$(cn host-c)" polkadot-asset-hub-collator.service NRestarts)
  pid=$(unit_prop "$(cn host-b)" polkadot-validator.service MainPID)
  on host-b kill -9 "$pid"
  log "  killed the validator on host-b; systemd restarts it after RestartSec=120"
  wait_until 60 "host-c asset-hub-collator fails over to host-a" on_relay_rpc_of host-c asset-hub-collator \
    "${IP[host-a]}" || return 1
  since=$(now host-c)
  wait_until 600 "host-c authors on the fallback" authored_since host-c asset-hub-collator "$since" || return 1
  [ "$(unit_prop "$(cn host-c)" polkadot-asset-hub-collator.service NRestarts)" = "$restarts" ] || {
    log "host-c asset-hub-collator was restarted instead of failing over"
    return 1
  }
  wait_until 400 "host-b validator restarted" \
    role_serving "$(cn host-b)" validator 9945 || return 1
  [ "$(unit_prop "$(cn host-b)" polkadot-validator.service NRestarts)" -ge 1 ]
}

check_collator_host_reboot() {
  local since
  # Give systemd time to stop the nodes cleanly, as a real reboot would.
  docker restart --time 60 "$(cn host-c)" >/dev/null
  wait_until 90 "host-c boots" systemd_up "$(cn host-c)" || return 1
  since=$(now host-c)
  wait_until 900 "host-c authors after reboot" authored_since host-c people-collator "$since"
}

check_target_restart() {
  local before since
  on host-a systemctl restart polkadot-metanode.target
  wait_until 600 "host-a validator back" role_serving "$(cn host-a)" validator 9945 || return 1
  since=$(now host-a)
  before=$(finalized_number "$(cn host-b)" 9945) || return 1
  wait_until 600 "relay finality resumes" relay_finalized host-b $((before + 3)) || return 1
  wait_until 900 "host-a authors after the restart" authored_since host-a asset-hub-collator "$since"
}

# class_bytes <class>: bytes sent through an HTB class on host-a's uplink.
class_bytes() {
  on host-a tc -s class show dev eth0 |
    awk -v id="$1" '$1 == "class" { found = ($3 == id) } found && $1 == "Sent" { print $2; exit }'
}

qos_table_loaded() {
  local out
  out=$(on host-a nft list table inet polkadot_metanode_qos 2>/dev/null) || return 1
  # nft shows the user's name, or its uid where it cannot resolve the name.
  grep -qE 'meta skuid ("polkadot-validator"|[0-9]+) meta priority set 1:10' <<<"$out"
}

# host-a reserves 500 Mbit/s of its 1gbit uplink for the validator: the HTB tree is on its default
# route's interface, the validator's packets go through class 1:10 and the collators' through 1:20,
# and the table survives nftables starting, reloading and restarting (which flush the ruleset).
check_bandwidth_reservation() {
  local classes ok=1 v1 v2 o1
  classes=$(on host-a tc class show dev eth0)
  # Each class bursts 1 ms of its own rate, and 1 ms of LINK (950 Mbit/s) when it borrows.
  grep -q 'class htb 1:10 parent 1:1 .*rate 500Mbit ceil 950Mbit burst 62500b cburst 118750b' <<<"$classes" || ok=0
  grep -q 'class htb 1:20 parent 1:1 .*rate 450Mbit ceil 950Mbit burst 56250b cburst 118750b' <<<"$classes" || ok=0
  [ "$ok" = 1 ] || log "classes on host-a: $classes"
  qos_table_loaded || {
    log "the nft table is missing"
    ok=0
  }
  v1=$(class_bytes 1:10)
  o1=$(class_bytes 1:20)
  sleep 20
  v2=$(class_bytes 1:10)
  ((v2 > v1)) || {
    log "no validator traffic in class 1:10 ($v1 -> $v2 bytes)"
    ok=0
  }
  (($(class_bytes 1:20) > o1)) || {
    log "no other traffic in class 1:20"
    ok=0
  }
  # The firewall replaces the ruleset when it starts, reloads or restarts; the table must come back
  # each time. Stopping the firewall stops the reservation, and starting it brings both back.
  local action
  for action in start reload restart; do
    on host-a systemctl "$action" nftables.service
    sleep 2
    qos_table_loaded || {
      log "the nft table is gone after: systemctl $action nftables"
      ok=0
    }
  done
  on host-a systemctl stop nftables.service
  sleep 2
  [ "$(on host-a systemctl is-active polkadot-metanode-qos.service)" != active ] || {
    log "the reservation is still active after stopping nftables"
    ok=0
  }
  on host-a systemctl start nftables.service
  sleep 2
  { [ "$(on host-a systemctl is-active polkadot-metanode-qos.service)" = active ] && qos_table_loaded; } || {
    log "starting nftables did not bring the reservation back"
    ok=0
  }
  # Re-applying the reservation in place works (the HTB root is kept).
  on host-a systemctl reload polkadot-metanode-qos.service || {
    log "reloading polkadot-metanode-qos failed"
    ok=0
  }
  [ "$ok" = 1 ]
}

# doctor on host-a: nothing fails, and it sees the reservation.
check_doctor() {
  local out status=0
  out=$(on host-a polkadot-metanodectl doctor 2>&1) || status=$?
  echo "$out"
  [ "$status" = 0 ] && grep -q '^OK   bandwidth reservation possible (UPLINK_RATE=1gbit)' <<<"$out" &&
    grep -q '^OK   polkadot-metanode-qos.service is active' <<<"$out"
}

# A new machine installs from nothing but install.sh, piped from a web server (as from
# raw.githubusercontent.com), with NETWORK=paseo: it fetches the other files, downloads Paseo's
# chain specs and checks their sha256, and metanode.env points the roles at them.
check_bootstrap_paseo() {
  local ok=1 out spec_dir=/usr/local/lib/polkadot-metanode/chain-specs/paseo
  start_container "$(cn paseo)" "${DISTRO[paseo]}" --network "$NETWORK_NAME" --ip "${IP[paseo]}" || return 1
  out=$(on paseo bash -c "mkdir -p /var/tmp/empty && cd /var/tmp/empty &&
    curl -fsSL http://$GATEWAY:$HTTP_PORT/install.sh |
      bash -s -- --source http://$GATEWAY:$HTTP_PORT --bin-dir /opt/metanode/bin --network paseo" 2>&1) || {
    log "bootstrap install failed: $out"
    return 1
  }
  grep -q "fetched the metanode files from http://$GATEWAY:$HTTP_PORT" <<<"$out" || ok=0
  on paseo test -x /usr/local/bin/polkadot-metanodectl || ok=0
  for spec in paseo asset-hub-next-paseo people-next-paseo; do
    on paseo test -s "$spec_dir/$spec.json" || {
      log "$spec_dir/$spec.json missing"
      ok=0
    }
  done
  out=$(on paseo cat /etc/polkadot-metanode/metanode.env) || ok=0
  if ! grep -qx "RELAY_CHAIN=$spec_dir/paseo.json" <<<"$out" ||
    ! grep -qx "ASSET_HUB_CHAIN=$spec_dir/asset-hub-next-paseo.json" <<<"$out" ||
    ! grep -qx "PEOPLE_CHAIN=$spec_dir/people-next-paseo.json" <<<"$out"; then
    log "metanode.env: $out"
    ok=0
  fi
  out=$(on paseo polkadot-metanodectl doctor 2>&1) || true
  grep -qx "OK   configuration of people-collator" <<<"$out" || {
    log "doctor: $out"
    ok=0
  }
  [ "$ok" = 1 ]
}

serve_scripts() {
  python3 -m http.server --bind "$GATEWAY" --directory "$METANODE_SRC" "$HTTP_PORT" >/dev/null 2>&1 &
  http_pid=$!
}

# ---- run ---------------------------------------------------------------------------------------

setup
serve_scripts
start=$(now host-a)
check "e2e: relay chain finalizes (host-a)" wait_until 600 "relay finality" relay_finalized host-a 3
check "e2e: relay chain finalizes (host-b)" wait_until 600 "relay finality" relay_finalized host-b 3
check "e2e: Asset Hub and People finalize, authored on host-a and host-c" check_parachains_author "$start"
check "e2e: embedded-relay follower on host-d keeps up with Asset Hub, without restarts" \
  check_follower
check "e2e: bandwidth reservation on host-a: classes, tagging, nftables start/reload/restart/stop" \
  check_bandwidth_reservation
check "e2e: doctor on host-a passes and sees the reservation" check_doctor
check "e2e: validator crash on host-b: host-c's collators fail over to host-a and keep authoring" \
  check_validator_crash
check "e2e: reboot of the collator host host-c" check_collator_host_reboot
check "e2e: systemctl restart polkadot-metanode.target on host-a" check_target_restart
check "e2e: bootstrap install (curl | bash) of NETWORK=paseo downloads and uses its chain specs" \
  check_bootstrap_paseo
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
