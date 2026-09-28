#!/usr/bin/env bash
# Tier 0: what the metanode units pass to the nodes, polkadot-metanodectl's configuration checks and
# the bandwidth reservation, in one container with systemd and no network. Stand-ins for the
# binaries (stub-node) record their command line and stay up, so no node runs and nothing needs to
# be built.
#
# Each case directory under units/ holds a metanode.env, optionally polkadot-<role>.env files and a
# route marker, and expected.txt: for each role, the command line its unit started and what
# public-addr logged. The container's short hostname shows as @HOST@.
#
#   units.sh <distro>            compare against expected.txt
#   UPDATE=1 units.sh <distro>   rewrite expected.txt (review the diff before committing)
#
# Without a marker, the default routes have public source addresses, IPv4 and IPv6; `route-private`
# has an IPv4 route with a private source address (behind NAT), `no-route` no default route at all.
# shellcheck disable=SC2329 # the check_* functions are called through check()
set -euo pipefail
# shellcheck source-path=SCRIPTDIR source=lib.sh
. "$(dirname "$0")/lib.sh"

distro=${1:?usage: units.sh <distro>}
c=pm-units-$distro
ROLES=(validator asset-hub-collator people-collator)
UNITS=("${ROLES[@]/#/polkadot-}")
UNITS=("${UNITS[@]/%/.service}")
CONF=/etc/polkadot-metanode
POLKADOT_ENV='NETWORK=polkadot RELAY_CHAIN=polkadot ASSET_HUB_CHAIN=asset-hub-polkadot PEOPLE_CHAIN=people-polkadot'

in_c() { docker exec "$c" "$@"; }
sh_c() { docker exec "$c" bash -c "$1"; }

# write_env <file> <assignment...>: one assignment per line.
write_env() {
  local file=$1
  shift
  sh_c "printf '%s\n' $(printf "'%s' " "$@") > $file"
}

started() {
  in_c test -s "/run/polkadot-$1/argv"
}

# ---- setup -------------------------------------------------------------------------------------

setup() {
  build_image "$distro"
  start_container "$c" "$distro" --network none
  # shellcheck disable=SC2016 # $bin expands in the container
  sh_c 'mkdir -p /opt/stub && for bin in polkadot polkadot-parachain polkadot-prepare-worker polkadot-execute-worker; do
    install -m 0755 /opt/metanode/src/tests/stub-node /opt/stub/$bin; done'
  INSTALL_OUT=$(in_c /opt/metanode/src/install.sh --bin-dir /opt/stub 2>&1)
  # IPv6 for the dummy uplinks: docker turns it off in containers without an IPv6 network.
  in_c sysctl -qw net.ipv6.conf.all.disable_ipv6=0 net.ipv6.conf.default.disable_ipv6=0
  HOST=$(in_c hostname | cut -d. -f1)
}

# set_route [dir]: the default routes of a case (see the markers above), on dummy interfaces.
set_route() {
  local dir=${1:-} v4=1.2.3.4 v6=2a01:4f8:1:2::7
  sh_c 'ip link del up0 2>/dev/null; ip link del up1 2>/dev/null; true'
  [ ! -e "$dir/no-route" ] || return 0
  [ ! -e "$dir/route-private" ] || v4=192.168.1.20 v6=
  sh_c "ip link add up0 type dummy && ip link set up0 up && ip addr add $v4/24 dev up0 &&
    ip route add default via ${v4%.*}.1 dev up0"
  if [ -n "$v6" ]; then
    sh_c "ip -6 addr add $v6/64 dev up0 nodad && ip -6 route add default via ${v6%::*}::1 dev up0"
  fi
}

# ---- cases -------------------------------------------------------------------------------------

# The command line a role's unit started, and what public-addr logged for that start.
role_report() {
  local id lines
  echo "## $1"
  in_c cat "/run/polkadot-$1/argv" | tr '\n' ' ' | sed 's/ $//'
  echo
  id=$(unit_prop "$c" "polkadot-$1.service" InvocationID)
  in_c journalctl --sync
  lines=$(in_c journalctl "_SYSTEMD_INVOCATION_ID=$id" -o cat --no-pager)
  grep '^public-addr: ' <<<"$lines" || true
}

run_case() {
  local dir=$1 name file role actual
  name=$(basename "$dir")
  sh_c "rm -f $CONF/metanode.env $CONF/polkadot-*.env"
  for file in "$dir"/*.env; do docker cp "$file" "$c:$CONF/"; done
  set_route "$dir"
  in_c systemctl restart "${UNITS[@]}"
  for role in "${ROLES[@]}"; do
    wait_until 30 "$role started" started "$role" || return 1
  done
  actual=$(for role in "${ROLES[@]}"; do role_report "$role"; done)
  actual=${actual//$HOST/@HOST@}
  if [ "${UPDATE:-}" = 1 ]; then
    printf '%s\n' "$actual" >"$dir/expected.txt"
    log "updated $dir/expected.txt"
  else
    diff -u "$dir/expected.txt" <(printf '%s\n' "$actual")
  fi
}

# ---- checks ------------------------------------------------------------------------------------

# A machine installed without a network has no metanode.env: the roles cannot start, and --enable
# says why.
check_no_configuration() {
  local out ok=1
  ! in_c test -e "$CONF/metanode.env" || {
    log "install.sh without --network wrote $CONF/metanode.env"
    ok=0
  }
  grep -q "Next: re-run install.sh with --network NAME" <<<"$INSTALL_OUT" || {
    log "install.sh without --network: $INSTALL_OUT"
    ok=0
  }
  # --no-block: with Restart=always, systemd keeps retrying it.
  in_c systemctl start --no-block polkadot-validator.service
  wait_until 30 "polkadot-validator refused to start" \
    journal_has "$c" polkadot-validator.service "" "Failed to load environment files" || ok=0
  ! started validator || {
    log "polkadot-validator started without $CONF/metanode.env"
    ok=0
  }
  in_c systemctl stop polkadot-validator.service
  if out=$(in_c /opt/metanode/src/install.sh --bin-dir /opt/stub --enable all 2>&1); then
    log "install.sh --enable all without a network succeeded"
    ok=0
  elif ! grep -q "$CONF/metanode.env does not exist, so the roles cannot start" <<<"$out"; then
    log "unexpected refusal: $out"
    ok=0
  fi
  [ "$ok" = 1 ]
}

# --config takes a file as short as NETWORK alone: install.sh fills in the chains of the network. A
# file with errors is refused, naming them, and the installed one stays.
check_config_install() {
  local out ok=1 installed
  write_env /var/tmp/minimal.env NETWORK=kusama NODE_NAME=k1
  in_c /opt/metanode/src/install.sh --bin-dir /opt/stub --config /var/tmp/minimal.env >/dev/null || ok=0
  installed=$(in_c grep -E '^[A-Z_]+=' "$CONF/metanode.env" | sort | tr '\n' ' ')
  [ "$installed" = "ASSET_HUB_CHAIN=asset-hub-kusama NETWORK=kusama NODE_NAME=k1 PEOPLE_CHAIN=people-kusama RELAY_CHAIN=kusama " ] || {
    log "installed metanode.env: $installed"
    ok=0
  }
  if out=$(in_c /opt/metanode/src/install.sh --bin-dir /opt/stub --network polkadot 2>&1); then
    log "install.sh --network polkadot replaced NETWORK=kusama"
    ok=0
  elif ! grep -q "already sets NETWORK=kusama" <<<"$out"; then
    log "unexpected refusal: $out"
    ok=0
  fi
  write_env /var/tmp/broken.env NETWORK=kusama SYNC=slow 'RELAY_CHAIN_RPC_URLS=http://10.0.0.5:9944' RELAY_CHAN=x
  if out=$(in_c /opt/metanode/src/install.sh --bin-dir /opt/stub --config /var/tmp/broken.env --force-config 2>&1); then
    log "install.sh installed a broken metanode.env"
    ok=0
  elif ! grep -q "SYNC: 'slow' must be warp, full, fast or fast-unsafe" <<<"$out" ||
    ! grep -q "RELAY_CHAIN_RPC_URLS: 'http://10.0.0.5:9944' is not a ws:// or wss:// URL" <<<"$out" ||
    ! grep -q "no unit uses RELAY_CHAN (a typo?)" <<<"$out"; then
    log "the refusal does not name every problem: $out"
    ok=0
  fi
  in_c grep -q '^NODE_NAME=k1$' "$CONF/metanode.env" || {
    log "the refused file replaced the installed one"
    ok=0
  }
  [ "$ok" = 1 ]
}

# A drop-in's Environment= sets what the files leave alone, but a file wins where it sets the same
# variable: systemd's EnvironmentFile= overrides Environment=. doctor sees the same.
check_dropin_environment() {
  local ok=1 argv out
  # shellcheck disable=SC2086 # one assignment per word
  write_env "$CONF/metanode.env" $POLKADOT_ENV NODE_NAME=d1 RPC_MAX_CONNECTIONS=42
  sh_c "rm -f $CONF/polkadot-*.env && mkdir -p /etc/systemd/system/polkadot-validator.service.d &&
    printf '[Service]\nEnvironment=EXTRA_ARGS=--prometheus-external RPC_MAX_CONNECTIONS=7\n' \
      > /etc/systemd/system/polkadot-validator.service.d/override.conf &&
    systemctl daemon-reload && systemctl restart polkadot-validator.service"
  wait_until 30 "validator started" started validator || ok=0
  argv=$(in_c cat /run/polkadot-validator/argv | tr '\n' ' ')
  [[ "$argv" == *" --prometheus-external "* ]] || {
    log "the drop-in's EXTRA_ARGS is missing: $argv"
    ok=0
  }
  [[ "$argv" == *",max-connections=42 "* ]] || {
    log "the drop-in's RPC_MAX_CONNECTIONS won over metanode.env's: $argv"
    ok=0
  }
  out=$(in_c polkadot-metanodectl doctor validator 2>&1) || true
  grep -qx "OK   configuration of validator" <<<"$out" || {
    log "doctor: $out"
    ok=0
  }
  sh_c 'rm -r /etc/systemd/system/polkadot-validator.service.d && systemctl daemon-reload'
  [ "$ok" = 1 ]
}

# doctor checks what each unit passes to its node: invalid values, extra flags the unit already
# passes or that do not apply to its RPC endpoints, and variables no unit uses.
check_doctor_config() {
  local out ok=1
  # shellcheck disable=SC2086 # one assignment per word
  write_env "$CONF/metanode.env" $POLKADOT_ENV
  write_env "$CONF/polkadot-validator.env" 'EXTRA_ARGS=--rpc-port=9999'
  write_env "$CONF/polkadot-asset-hub-collator.env" SYNC=slow 'EXTRA_ARGS=--database=rocksdb --in-peers=8' \
    RELAY_CHAN=polkadot
  out=$(in_c polkadot-metanodectl doctor validator asset-hub-collator people-collator 2>&1) || true
  grep -E '^(OK|WARN|FAIL) +configuration' <<<"$out" | sed 's/^/  doctor says: /' || true
  grep -qx "FAIL configuration of validator: EXTRA_ARGS sets --rpc-port, which does not apply to the RPC endpoints; use RPC_LISTEN_IP and RPC_MAX_CONNECTIONS" <<<"$out" || ok=0
  grep -q "^FAIL configuration of asset-hub-collator: .*SYNC: 'slow' must be warp, full, fast or fast-unsafe" <<<"$out" || ok=0
  grep -q "^FAIL configuration of asset-hub-collator: .*EXTRA_ARGS repeats --database, which polkadot-asset-hub-collator.service already passes" <<<"$out" || ok=0
  grep -qx "WARN configuration of asset-hub-collator: no unit uses RELAY_CHAN (a typo?)" <<<"$out" || ok=0
  grep -qx "OK   configuration of people-collator" <<<"$out" || ok=0
  sh_c "rm -f $CONF/polkadot-*.env"
  [ "$ok" = 1 ]
}

qos_classes() {
  in_c tc class show dev up0
}

qos_root() {
  in_c tc qdisc show dev up0 root
}

# The bandwidth reservation of a 1gbit uplink on up0: bandwidth.md's tree, with per-class bursts; a
# reload keeps it; a foreign HTB root with our handle is replaced; several default-route interfaces
# need UPLINK_DEVICE; stop removes the tree and the table.
check_qos() {
  local ok=1 classes out
  set_route
  # shellcheck disable=SC2086 # one assignment per word
  write_env "$CONF/metanode.env" $POLKADOT_ENV UPLINK_RATE=1gbit
  in_c systemctl start polkadot-metanode-qos.service || ok=0
  classes=$(qos_classes)
  # A burst of 1 ms of the class's own rate, a cburst of 1 ms of LINK, 950 Mbit/s.
  if ! grep -qE 'class htb 1:1 root .*rate 950Mbit ceil 950Mbit burst 118750b cburst 118750b' <<<"$classes" ||
    ! grep -qE 'class htb 1:10 parent 1:1 .*rate 500Mbit ceil 950Mbit burst 62500b cburst 118750b' <<<"$classes" ||
    ! grep -qE 'class htb 1:20 parent 1:1 .*rate 450Mbit ceil 950Mbit burst 56250b cburst 118750b' <<<"$classes"; then
    log "classes: $classes"
    ok=0
  fi
  out=$(in_c nft list table inet polkadot_metanode_qos 2>&1) || true
  grep -qE 'meta skuid ("polkadot-validator"|[0-9]+) meta priority set 1:10' <<<"$out" || {
    log "nft table: $out"
    ok=0
  }
  in_c systemctl reload polkadot-metanode-qos.service || {
    log "reloading the reservation over its own tree failed"
    ok=0
  }
  in_c systemctl stop polkadot-metanode-qos.service
  in_c tc qdisc add dev up0 root handle 1: htb default 1
  in_c systemctl start polkadot-metanode-qos.service || ok=0
  [[ "$(qos_root)" =~ ^qdisc\ htb\ 1:\ root\ .*\ default\ 0x20\  ]] || {
    log "a foreign root was kept: $(qos_root)"
    ok=0
  }
  # IPv6 through up1: two interfaces.
  sh_c 'ip link add up1 type dummy && ip link set up1 up && ip -6 addr add 2a01:4f8:9::7/64 dev up1 nodad &&
    ip -6 route del default && ip -6 route add default via 2a01:4f8:9::1 dev up1'
  if out=$(in_c polkadot-metanodectl qos check 2>&1); then
    log "qos check accepted two default-route interfaces"
    ok=0
  elif ! grep -q "the default routes use more than one interface (up0 up1); set UPLINK_DEVICE" <<<"$out"; then
    log "unexpected refusal: $out"
    ok=0
  fi
  sh_c "echo UPLINK_DEVICE=up0 >> $CONF/metanode.env"
  in_c polkadot-metanodectl qos check || ok=0
  in_c systemctl stop polkadot-metanode-qos.service
  [[ "$(qos_root)" != *htb* ]] || {
    log "the tree is left after stop: $(qos_root)"
    ok=0
  }
  ! in_c nft list table inet polkadot_metanode_qos >/dev/null 2>&1 || {
    log "the nft table is left after stop"
    ok=0
  }
  [ "$ok" = 1 ]
}

# ---- run ---------------------------------------------------------------------------------------

setup
check "$distro: units: without a network, there is no metanode.env and the roles cannot start" \
  check_no_configuration
check "$distro: units: --config fills in NETWORK's chains, and a broken file is refused" check_config_install
for dir in "$TESTS_DIR"/units/*/; do
  dir=${dir%/}
  check "$distro: units: $(basename "$dir")" run_case "$dir"
done
check "$distro: units: a drop-in's Environment= applies, and metanode.env wins over it" check_dropin_environment
check "$distro: units: doctor reports invalid values, repeated or RPC flags, and typos" check_doctor_config
check "$distro: units: bandwidth reservation: tree, bursts, reload, foreign root, two uplinks, stop" check_qos

log "$distro: $FAILURES failure(s)"
exit $((FAILURES > 0))
