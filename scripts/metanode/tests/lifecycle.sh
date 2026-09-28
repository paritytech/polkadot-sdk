#!/usr/bin/env bash
# Tier 0 (per distro) and tier 1: install, lint and the lifecycle of all three roles on one
# machine. One container, with no network: dev-chain nodes in containers on a shared network find
# each other over mDNS and fork each other.
#
# Usage: BIN_DIR=<node binaries> lifecycle.sh <distro>
# shellcheck disable=SC2329 # the check_* functions are called through check()
set -euo pipefail
# shellcheck source-path=SCRIPTDIR source=lib.sh
. "$(dirname "$0")/lib.sh"

distro=${1:?usage: lifecycle.sh <distro>}
: "${BIN_DIR:?BIN_DIR must point at the node binaries}"
c=pm-life-$distro
# //Alice, used as the stash that owns the rotated session keys.
ALICE=5GrwvaEF5zXb26Fz9rcQpDWS57CtERHpNehXCPcNoHGKutQY
ROLES=(validator asset-hub-collator people-collator)
# The localhost RPC of every role: the unsafe endpoint, which every method and status use.
declare -A RPC=([validator]=9945 [asset-hub-collator]=9947 [people-collator]=9949)
# MemoryLow=32G, in bytes.
MEMORY_LOW=34359738368

in_c() { docker exec "$c" "$@"; }
sh_c() { docker exec "$c" bash -c "$1"; }

serving() { role_serving "$c" "$1" "${RPC[$1]}"; }

wait_serving() {
  local role
  for role in "$@"; do
    wait_until 300 "$role serving" serving "$role" || return 1
  done
}

main_pids() {
  local role
  for role in "${ROLES[@]}"; do unit_prop "$c" "polkadot-$role.service" MainPID; done
}

peer_id() {
  rpc "$c" "${RPC[$1]}" system_localPeerId | jq -r '.result'
}

# ---- setup -------------------------------------------------------------------------------------

setup() {
  build_image "$distro"
  start_container "$c" "$distro" --network none
  log "$distro: systemd $(in_c systemctl --version | awk 'NR == 1 { print $2 }')"
  in_c /opt/metanode/src/install.sh --bin-dir /opt/metanode/bin >/dev/null
  # Dev chains: the validator authors alone; the collators follow it over RPC but do not author,
  # as their chains are not registered on westend-dev. For the same reason they full sync: warp sync
  # needs the parachain's head on the relay chain. Each role's own file adds its dev key.
  # shellcheck disable=SC2016 # $role expands inside the container
  sh_c 'cat > /etc/polkadot-metanode/metanode.env <<EOF
NETWORK=westend
RELAY_CHAIN=westend-dev
ASSET_HUB_CHAIN=asset-hub-westend-dev
PEOPLE_CHAIN=people-westend-dev
NODE_NAME=life
SYNC=full
EOF
echo "EXTRA_ARGS=--alice --force-authoring" > /etc/polkadot-metanode/polkadot-validator.env
for role in asset-hub-collator people-collator; do
  echo EXTRA_ARGS=--alice > /etc/polkadot-metanode/polkadot-$role.env
done'
}

# ---- tier 0: unit lint -------------------------------------------------------------------------

check_units_verify() {
  local out
  out=$(sh_c 'cd /usr/local/lib/systemd/system &&
    systemd-analyze verify polkadot-validator.service polkadot-asset-hub-collator.service \
      polkadot-people-collator.service polkadot-metanode.target polkadot-metanode-all.target \
      polkadot-metanode.slice polkadot.slice polkadot-metanode-qos.service 2>&1') || true
  if grep -E "polkadot-(validator|asset-hub|people|metanode)|polkadot\.slice|10-common.conf" <<<"$out"; then
    return 1
  fi
}

# install.sh installs the unit templates with this layout's paths filled in.
check_installed_layout() {
  local role fragment exec pre binary ok=1
  for role in "${ROLES[@]}"; do
    fragment=$(unit_prop "$c" "polkadot-$role.service" FragmentPath)
    exec=$(unit_prop "$c" "polkadot-$role.service" ExecStart)
    pre=$(unit_prop "$c" "polkadot-$role.service" ExecStartPre)
    binary=polkadot-parachain
    [ "$role" != validator ] || binary="polkadot --validator"
    if [ "$fragment" != "/usr/local/lib/systemd/system/polkadot-$role.service" ] ||
      [[ "$exec" != *"argv[]=/usr/local/bin/$binary "* || "$exec$pre" == *@*DIR@* ]] ||
      [[ "$pre" != *"argv[]=/usr/local/lib/polkadot-metanode/public-addr "* ]]; then
      log "polkadot-$role.service: FragmentPath=$fragment ExecStartPre=$pre ExecStart=$exec"
      ok=0
    fi
  done
  exec=$(unit_prop "$c" polkadot-validator.service ExecStart)
  [[ "$exec" == *" --workers-path=/usr/local/bin "* ]] || {
    log "polkadot-validator.service: ExecStart=$exec"
    ok=0
  }
  exec=$(unit_prop "$c" polkadot-metanode-qos.service ExecStart)
  [[ "$exec" == *"/usr/local/bin/polkadot-metanodectl qos start "* ]] || {
    log "polkadot-metanode-qos.service: ExecStart=$exec"
    ok=0
  }
  [ "$ok" = 1 ]
}

report_security_scores() {
  local role score
  for role in "${ROLES[@]}"; do
    score=$(in_c systemd-analyze security --no-pager "polkadot-$role.service" 2>/dev/null |
      awk '/Overall exposure level/ { print $(NF - 2) }')
    log "exposure score polkadot-$role: ${score:-?}"
    echo "$role ${score:-?}" >>"${SCORES_FILE:-/dev/null}"
  done
}

# ---- tier 1 checks -----------------------------------------------------------------------------

check_user() {
  local entry
  entry=$(in_c getent passwd "polkadot-$1") || return 1
  [[ "$entry" == *":/var/lib/polkadot-$1:"*nologin ]]
}

check_state_dir() {
  [ "$(in_c stat -c '%a %U' "/var/lib/polkadot-$1")" = "700 polkadot-$1" ]
}

check_node_key() {
  [ "$(in_c stat -c '%a %U %s' "/var/lib/polkadot-$1/node.key")" = "600 polkadot-$1 64" ]
}

check_status_helper() {
  local out
  out=$(in_c polkadot-metanodectl status) || return 1
  echo "$out"
  [ "$(grep -cE '^(validator|asset-hub-collator|people-collator) +enabled +active ' <<<"$out")" = 3 ]
}

# Every PVF security check that passes for the validator user outside the unit must pass inside
# it too, so the unit's sandbox takes nothing away from the host's. Valid on any host.
check_sandbox_parity() {
  local j tmp flag args failed=0
  j=$(journal "$c" polkadot-validator.service)
  grep -q "Running in Secure Validator Mode" <<<"$j" || {
    log "the validator journal lacks 'Running in Secure Validator Mode'"
    failed=1
  }
  tmp=$(in_c runuser -u polkadot-validator -- mktemp -d /tmp/pvf-check.XXXXXX)
  declare -A inside=(
    [--check-can-enable-landlock]="Cannot enable landlock"
    [--check-can-enable-seccomp]="Cannot enable seccomp"
    [--check-can-unshare-user-namespace-and-change-root]="Cannot unshare user namespace"
    [--check-can-do-secure-clone]="Cannot call clone"
  )
  for flag in "${!inside[@]}"; do
    args=()
    [ "$flag" != --check-can-unshare-user-namespace-and-change-root ] || args=("$tmp")
    if in_c runuser -u polkadot-validator -- /usr/local/bin/polkadot-prepare-worker "$flag" \
      "${args[@]}" >/dev/null 2>&1; then
      if grep -q "${inside[$flag]}" <<<"$j"; then
        log "$flag passes on the host but fails inside the unit"
        failed=1
      else
        log "  $flag: passes on the host and in the unit"
      fi
    else
      log "  $flag: unavailable on this host (not a unit problem)"
    fi
  done
  return "$failed"
}

# Every role serves light clients over WebRTC: it listens on UDP with its p2p port and reports a
# webrtc-direct address with the certificate hash light clients need to dial it.
check_webrtc() {
  local role port pid sockets addresses ok=1
  declare -A p2p=([validator]=30333 [asset-hub-collator]=30334 [people-collator]=30336)
  sockets=$(in_c ss -lunpH)
  for role in "${ROLES[@]}"; do
    port=${p2p[$role]}
    pid=$(unit_prop "$c" "polkadot-$role.service" MainPID)
    grep -qE ":$port .*pid=$pid," <<<"$sockets" || {
      log "polkadot-$role (pid $pid) does not listen on UDP $port"
      ok=0
    }
    addresses=$(rpc "$c" "${RPC[$role]}" system_localListenAddresses | jq -r '.result[]')
    grep -qE "/udp/$port/webrtc-direct/certhash/" <<<"$addresses" || {
      log "polkadot-$role reports no webrtc-direct address with a certhash: $addresses"
      ok=0
    }
  done
  [ "$ok" = 1 ]
}

# Collators use setup.md's database, People its statement store, and the sync mode of metanode.env
# (full: these dev parachains cannot warp sync, see setup); the validator the node's defaults.
check_storage_defaults() {
  local role ok=1 cmdline
  for role in asset-hub-collator people-collator; do
    journal_has "$c" "polkadot-$role.service" "" "Database: ParityDb at /var/lib/polkadot-$role/" || {
      log "polkadot-$role does not use ParityDB"
      ok=0
    }
    cmdline=$(in_c cat "/proc/$(unit_prop "$c" "polkadot-$role.service" MainPID)/cmdline" | tr '\0' ' ')
    [[ "$cmdline" == *" --sync=full "* ]] || {
      log "polkadot-$role does not get SYNC=full from metanode.env: $cmdline"
      ok=0
    }
  done
  [[ "$cmdline" == *" --enable-statement-store "* ]] || {
    log "polkadot-people-collator keeps no statement store: $cmdline"
    ok=0
  }
  journal_has "$c" polkadot-validator.service "" "Database: RocksDb at /var/lib/polkadot-validator/" || {
    log "the validator does not use the node's default database (RocksDB)"
    ok=0
  }
  [ "$ok" = 1 ]
}

check_isolation() {
  ! in_c runuser -u polkadot-asset-hub-collator -- ls /var/lib/polkadot-validator >/dev/null 2>&1
}

check_no_privileges() {
  local role pid status
  for role in "${ROLES[@]}"; do
    pid=$(unit_prop "$c" "polkadot-$role.service" MainPID)
    status=$(in_c cat "/proc/$pid/status")
    grep -qE '^NoNewPrivs:\s+1$' <<<"$status" || return 1
    grep -qE '^CapEff:\s+0+$' <<<"$status" || return 1
  done
}

# listens <role> <port>...: the role's own node process listens on every port.
listens() {
  local role=$1 pid sockets port
  shift
  pid=$(unit_prop "$c" "polkadot-$role.service" MainPID)
  sockets=$(in_c ss -ltnpH)
  for port in "$@"; do
    grep -qE ":$port .*pid=$pid," <<<"$sockets" || {
      log "polkadot-$role (pid $pid) does not listen on $port"
      return 1
    }
  done
}

# setup.md's two RPC servers: the safe methods on the public-facing port, every method on localhost.
check_rpc_endpoints() {
  local out
  out=$(rpc "$c" 9944 author_rotateKeys 2>/dev/null) || true
  jq -e '.error.code == -32601' >/dev/null <<<"$out" || {
    log "author_rotateKeys on the safe port 9944: $out"
    return 1
  }
  out=$(rpc "$c" 9945 author_hasKey '["0x00", "babe"]') || return 1
  jq -e '.result == false' >/dev/null <<<"$out" || {
    log "author_hasKey on the unsafe port 9945: $out"
    return 1
  }
}

check_resources() {
  [ "$(unit_prop "$c" polkadot-validator.service CPUWeight)" = 1000 ] &&
    [ "$(unit_prop "$c" polkadot-validator.service IOWeight)" = 1000 ] &&
    [ "$(unit_prop "$c" polkadot-validator.service Slice)" = polkadot-metanode.slice ] &&
    [ "$(unit_prop "$c" polkadot-asset-hub-collator.service Slice)" = polkadot-metanode.slice ] &&
    [ "$(in_c cat /sys/fs/cgroup/polkadot.slice/polkadot-metanode.slice/polkadot-validator.service/cpu.weight)" = 1000 ]
}

# setup.md's memory protection: MemoryLow on the validator and on both slices above it (else it
# protects nothing), and the validator is the last one the kernel or systemd-oomd would kill.
check_memory_protection() {
  local cg=/sys/fs/cgroup/polkadot.slice pid ok=1 path value
  [ "$(unit_prop "$c" polkadot-validator.service MemoryLow)" = "$MEMORY_LOW" ] || ok=0
  [ "$(unit_prop "$c" polkadot-validator.service OOMScoreAdjust)" = -900 ] || ok=0
  [ "$(unit_prop "$c" polkadot-validator.service ManagedOOMPreference)" = avoid ] || ok=0
  [ "$(unit_prop "$c" polkadot-asset-hub-collator.service MemoryLow)" = 0 ] || ok=0
  pid=$(unit_prop "$c" polkadot-validator.service MainPID)
  [ "$(in_c cat "/proc/$pid/oom_score_adj")" = -900 ] || ok=0
  for path in "$cg" "$cg/polkadot-metanode.slice" "$cg/polkadot-metanode.slice/polkadot-validator.service"; do
    value=$(in_c cat "$path/memory.low" 2>/dev/null) || value=missing
    [ "$value" = "$MEMORY_LOW" ] || {
      log "$path/memory.low: $value"
      ok=0
    }
  done
  [ "$ok" = 1 ]
}

# rotate-keys must return keys and a proof, and the node must then hold those keys.
check_rotate_keys() {
  local role=$1 out keys proof
  out=$(in_c polkadot-metanodectl rotate-keys "$role" "$ALICE") || return 1
  keys=$(awk '$1 == "keys:" { print $2 }' <<<"$out")
  proof=$(awk '$1 == "proof:" { print $2 }' <<<"$out")
  [[ "$keys" == 0x* && "$proof" == 0x* ]] || return 1
  [ "$(rpc "$c" "${RPC[$(canonical "$role")]}" author_hasSessionKeys "[\"$keys\"]" | jq -r '.result')" = true ]
}

canonical() {
  case "$1" in
    asset-hub) echo asset-hub-collator ;;
    people) echo people-collator ;;
    *) echo "$1" ;;
  esac
}

check_peer_id_stable() {
  local before after
  before=$(peer_id asset-hub-collator)
  in_c systemctl restart polkadot-asset-hub-collator.service
  wait_serving asset-hub-collator || return 1
  after=$(peer_id asset-hub-collator)
  [ -n "$before" ] && [ "$before" = "$after" ]
}

# restarted_since <role> <count>: systemd restarted the role more than <count> times.
restarted_since() {
  [ "$(unit_prop "$c" "polkadot-$1.service" NRestarts)" -gt "$2" ]
}

# SIGKILL the node itself (a crash), then check systemd saw exactly that and restarted it.
check_crash_restart() {
  local before pid since
  before=$(unit_prop "$c" polkadot-people-collator.service NRestarts)
  pid=$(unit_prop "$c" polkadot-people-collator.service MainPID)
  since=$(in_c date '+%Y-%m-%d %H:%M:%S')
  in_c kill -9 "$pid"
  wait_until 60 "people-collator restarted" restarted_since people-collator "$before" || return 1
  journal_has "$c" polkadot-people-collator.service "$since" "status=9/KILL" || {
    log "systemd recorded no SIGKILL of the main process"
    return 1
  }
  wait_serving people-collator
}

check_restart_properties() {
  [ "$(unit_prop "$c" polkadot-validator.service RestartUSec)" = 2min ] &&
    [ "$(unit_prop "$c" polkadot-asset-hub-collator.service RestartUSec)" = 10s ] &&
    [ "$(unit_prop "$c" polkadot-people-collator.service StartLimitIntervalUSec)" = 0 ]
}

check_graceful_stop() {
  in_c systemctl stop polkadot-people-collator.service
  local result
  result=$(unit_prop "$c" polkadot-people-collator.service Result)
  in_c systemctl start polkadot-people-collator.service
  [ "$result" = success ] || {
    log "stop result: $result"
    return 1
  }
  wait_serving people-collator
}

# failed_with <role> <status>: the unit failed with that exit status and is not being restarted.
failed_with() {
  [ "$(unit_prop "$c" "polkadot-$1.service" SubState)" = failed ] &&
    [ "$(unit_prop "$c" "polkadot-$1.service" ExecMainStatus)" = "$2" ]
}

# An invalid value (here in a role's own file) makes the node exit with a usage error, which fails
# the unit instead of restarting it forever.
check_config_error() {
  local ok=1 env=/etc/polkadot-metanode/polkadot-people-collator.env
  sh_c "echo SYNC=bogus >> $env"
  in_c systemctl restart polkadot-people-collator.service || true
  wait_until 60 "people-collator fails with a usage error" failed_with people-collator 2 || {
    log "an invalid SYNC gave status $(unit_prop "$c" polkadot-people-collator.service ExecMainStatus)," \
      "substate $(unit_prop "$c" polkadot-people-collator.service SubState)"
    ok=0
  }
  sh_c "sed -i '/^SYNC=bogus$/d' $env"
  in_c systemctl reset-failed polkadot-people-collator.service
  in_c systemctl start polkadot-people-collator.service
  [ "$ok" = 1 ] && wait_serving people-collator
}

# doctor checks that every relay chain RPC endpoint of a collator serves the relay chain, here the
# validator's on this machine: a fallback of another chain (People's own RPC) fails.
check_relay_genesis() {
  local out ok=1 env=/etc/polkadot-metanode/polkadot-asset-hub-collator.env
  sh_c "echo 'RELAY_CHAIN_RPC_URLS=ws://127.0.0.1:9944 ws://127.0.0.1:9948' >> $env"
  out=$(in_c polkadot-metanodectl doctor asset-hub-collator 2>&1) || true
  sh_c "sed -i '/^RELAY_CHAIN_RPC_URLS=/d' $env"
  grep -q '^OK   asset-hub-collator: relay-chain RPC ws://127.0.0.1:9944 answers, with the genesis of the validator on this machine' <<<"$out" || ok=0
  grep -q '^FAIL asset-hub-collator: relay-chain RPC ws://127.0.0.1:9948 serves another chain' <<<"$out" || ok=0
  [ "$ok" = 1 ] || log "doctor: $out"
  [ "$ok" = 1 ]
}

# doctor reports a missing PVF worker, or one of another version, which would stop the validator at
# its first PVF job.
check_workers() {
  local out ok=1
  in_c systemctl stop polkadot-validator.service
  sh_c 'mv /usr/local/bin/polkadot-execute-worker /var/tmp/ &&
    printf "#!/bin/sh\necho polkadot-execute-worker 0.0.1\n" > /usr/local/bin/polkadot-execute-worker &&
    chmod 0755 /usr/local/bin/polkadot-execute-worker'
  out=$(in_c polkadot-metanodectl doctor validator 2>&1) || true
  grep -q '^FAIL PVF workers: /usr/local/bin/polkadot-execute-worker is version 0.0.1 but /usr/local/bin/polkadot is ' <<<"$out" || {
    log "doctor with a worker of another version: $out"
    ok=0
  }
  sh_c 'rm /usr/local/bin/polkadot-execute-worker'
  out=$(in_c polkadot-metanodectl doctor validator 2>&1) || true
  grep -q '^FAIL PVF workers: /usr/local/bin/polkadot-execute-worker not found or not executable' <<<"$out" || {
    log "doctor without the execute worker: $out"
    ok=0
  }
  sh_c 'mv /var/tmp/polkadot-execute-worker /usr/local/bin/'
  out=$(in_c polkadot-metanodectl doctor validator 2>&1) || true
  grep -q '^OK   PVF workers of version [0-9.]* in /usr/local/bin$' <<<"$out" || {
    log "doctor with the right workers: $out"
    ok=0
  }
  in_c systemctl start polkadot-validator.service
  [ "$ok" = 1 ] && wait_serving "${ROLES[@]}"
}

# While the validator is down, a collator's node exits once its relay chain RPC fails, and systemd
# restarts it every 10 s until the validator is back.
check_validator_outage() {
  local before
  before=$(unit_prop "$c" polkadot-asset-hub-collator.service NRestarts)
  in_c systemctl stop polkadot-validator.service
  wait_until 300 "asset-hub-collator restarts without its relay RPC" restarted_since asset-hub-collator "$before" ||
    return 1
  in_c systemctl start polkadot-validator.service
  wait_serving "${ROLES[@]}"
}

# doctor reports on the machine: here every role is configured and holds its own ports, nothing
# fails (warnings are expected in a container without network or clock sync).
check_doctor() {
  local out status=0 ok=1 role
  out=$(in_c polkadot-metanodectl doctor 2>&1) || status=$?
  echo "$out"
  [ "$status" = 0 ] || ok=0
  ! grep -q '^FAIL' <<<"$out" || ok=0
  for role in "${ROLES[@]}"; do
    grep -qx "OK   configuration of $role" <<<"$out" || ok=0
    grep -qx "OK   ports of $role free or in use by it" <<<"$out" || ok=0
  done
  grep -q '^OK   people-collator: relay-chain RPC ws://127.0.0.1:9944 answers' <<<"$out" || ok=0
  # A port held by another program is a failure.
  in_c systemctl stop polkadot-validator.service
  sh_c 'nohup nc -lk 9615 >/dev/null 2>&1 & echo $! > /var/tmp/port-squatter'
  sleep 2
  out=$(in_c polkadot-metanodectl doctor validator 2>&1) || true
  # shellcheck disable=SC2016 # expands inside the container
  sh_c 'kill "$(cat /var/tmp/port-squatter)"'
  in_c systemctl start polkadot-validator.service
  grep -q '^FAIL ports of validator taken by other programs: tcp/9615 (nc)' <<<"$out" || {
    log "doctor missed a taken port: $out"
    ok=0
  }
  [ "$ok" = 1 ] && wait_serving "${ROLES[@]}"
}

# The bandwidth reservation needs a route to the uplink; install.sh refuses it here (no network)
# before installing anything, and --uplink-rate none works.
check_qos_refused_without_route() {
  local out ok=1
  if out=$(in_c /opt/metanode/src/install.sh --bin-dir /opt/metanode/bin --uplink-rate 1gbit 2>&1); then
    log "install.sh accepted --uplink-rate 1gbit without a default route"
    ok=0
  elif ! grep -q "no default route found" <<<"$out"; then
    log "unexpected refusal: $out"
    ok=0
  fi
  in_c grep -q '^UPLINK_RATE=1gbit' /etc/polkadot-metanode/metanode.env && {
    log "the refused rate was written to metanode.env"
    ok=0
  }
  in_c /opt/metanode/src/install.sh --bin-dir /opt/metanode/bin --uplink-rate none >/dev/null || ok=0
  [ "$(in_c systemctl is-enabled polkadot-metanode-qos.service 2>/dev/null)" != enabled ] || ok=0
  [ "$ok" = 1 ]
}

check_conflicts() {
  local ok=0
  sh_c 'printf "[Service]\nExecStart=/bin/sleep infinity\n" > /etc/systemd/system/polkadot.service
    systemctl daemon-reload'
  in_c systemctl start polkadot.service
  sleep 3
  if unit_inactive "$c" polkadot-validator.service && unit_active "$c" polkadot.service; then
    in_c systemctl start polkadot-validator.service
    sleep 3
    if unit_inactive "$c" polkadot.service && unit_active "$c" polkadot-validator.service; then
      ok=1
    fi
  fi
  sh_c 'systemctl stop polkadot.service; rm /etc/systemd/system/polkadot.service; systemctl daemon-reload'
  in_c systemctl start polkadot-validator.service
  [ "$ok" = 1 ] && wait_serving "${ROLES[@]}"
}

check_target() {
  local ok=0
  in_c systemctl stop polkadot-metanode.target
  sleep 3
  if unit_inactive "$c" polkadot-validator.service && unit_inactive "$c" polkadot-asset-hub-collator.service &&
    unit_inactive "$c" polkadot-people-collator.service; then
    in_c systemctl disable --quiet polkadot-people-collator.service
    in_c systemctl start polkadot-metanode.target
    sleep 3
    if unit_active "$c" polkadot-validator.service && unit_active "$c" polkadot-asset-hub-collator.service &&
      unit_inactive "$c" polkadot-people-collator.service; then
      ok=1
    fi
    in_c systemctl enable --now --quiet polkadot-people-collator.service
  fi
  [ "$ok" = 1 ] && wait_serving "${ROLES[@]}"
}

check_reboot() {
  # Give systemd time to stop the nodes cleanly, as a real reboot would.
  docker restart --time 60 "$c" >/dev/null
  wait_until 90 "systemd after reboot" systemd_up "$c" || return 1
  wait_serving "${ROLES[@]}"
}

# A machine that runs every role uses polkadot-metanode-all.target instead of enabling the roles one
# by one: install.sh --enable all, the roles come back after a reboot although none is enabled
# itself, and stopping the target stops them all.
check_all_target() {
  local ok=0
  in_c systemctl disable --quiet polkadot-validator.service polkadot-asset-hub-collator.service \
    polkadot-people-collator.service
  if in_c /opt/metanode/src/install.sh --bin-dir /opt/metanode/bin --enable all >/dev/null &&
    [ "$(in_c systemctl is-enabled polkadot-metanode-all.target)" = enabled ] &&
    docker restart --time 60 "$c" >/dev/null &&
    wait_until 90 "systemd after reboot" systemd_up "$c" &&
    wait_serving "${ROLES[@]}"; then
    in_c systemctl stop polkadot-metanode-all.target
    sleep 3
    if unit_inactive "$c" polkadot-validator.service && unit_inactive "$c" polkadot-asset-hub-collator.service &&
      unit_inactive "$c" polkadot-people-collator.service; then
      ok=1
    fi
  fi
  # Back to roles enabled one by one for the remaining checks.
  in_c systemctl disable --quiet polkadot-metanode-all.target
  in_c systemctl enable --now --quiet polkadot-validator.service polkadot-asset-hub-collator.service \
    polkadot-people-collator.service
  [ "$ok" = 1 ] && wait_serving "${ROLES[@]}"
}

# Re-installing changes nothing, and an operator's drop-in override of one of our units does not
# block it (unlike a unit file in /etc, see check_refuses_shadowing).
check_reinstall_idempotent() {
  local sums pids ok=0
  sh_c 'mkdir -p /etc/systemd/system/polkadot-people-collator.service.d &&
    printf "[Service]\nEnvironment=OPERATOR_OVERRIDE=1\n" > /etc/systemd/system/polkadot-people-collator.service.d/override.conf'
  sums=$(sh_c 'md5sum /etc/polkadot-metanode/*.env')
  pids=$(main_pids)
  if in_c /opt/metanode/src/install.sh --bin-dir /opt/metanode/bin >/dev/null; then
    [ "$sums" = "$(sh_c 'md5sum /etc/polkadot-metanode/*.env')" ] && [ "$pids" = "$(main_pids)" ] &&
      ok=1
  fi
  sh_c 'rm -r /etc/systemd/system/polkadot-people-collator.service.d && systemctl daemon-reload'
  [ "$ok" = 1 ]
}

# --config installs a shared metanode.env, refuses to replace a different one without
# --force-config, and keeps the replaced one.
check_config_file() {
  local ok=1 out
  sh_c 'cp /etc/polkadot-metanode/metanode.env /var/tmp/shared.env && echo "#changed" >> /var/tmp/shared.env'
  if out=$(in_c /opt/metanode/src/install.sh --bin-dir /opt/metanode/bin --config /var/tmp/shared.env 2>&1); then
    log "install.sh replaced a different metanode.env without --force-config"
    ok=0
  elif ! grep -q "differs from /var/tmp/shared.env" <<<"$out"; then
    log "unexpected refusal: $out"
    ok=0
  fi
  in_c /opt/metanode/src/install.sh --bin-dir /opt/metanode/bin --config /var/tmp/shared.env \
    --force-config >/dev/null || ok=0
  in_c cmp -s /var/tmp/shared.env /etc/polkadot-metanode/metanode.env || ok=0
  [ "$(sh_c 'ls /etc/polkadot-metanode/metanode.env.2* | wc -l')" -ge 1 ] || ok=0
  [ "$ok" = 1 ]
}

# The common settings apply to our units only: a prefix drop-in would also have applied to e.g. the
# polkadot-collator.service that docs.polkadot.com has operators write.
check_common_dropin_scope() {
  local role ok=1 out
  for role in "${ROLES[@]}"; do
    # Captured, not piped into grep -q (see journal_has in lib.sh).
    out=$(in_c systemctl cat "polkadot-$role.service")
    grep -qx "# /usr/local/lib/systemd/system/polkadot-$role.service.d/10-common.conf" <<<"$out" || {
      log "polkadot-$role.service lacks 10-common.conf"
      ok=0
    }
  done
  sh_c 'printf "[Service]\nExecStart=/bin/sleep infinity\n" > /etc/systemd/system/polkadot-collator.service
    systemctl daemon-reload'
  out=$(in_c systemctl cat polkadot-collator.service)
  if grep -q 10-common.conf <<<"$out" ||
    [ "$(unit_prop "$c" polkadot-collator.service Slice)" != system.slice ]; then
    log "the common settings leak into an unrelated polkadot-collator.service"
    ok=0
  fi
  sh_c 'rm /etc/systemd/system/polkadot-collator.service && systemctl daemon-reload'
  [ "$ok" = 1 ]
}

# A unit file in /etc (setup.md's hand-written polkadot-validator.service) would shadow ours, and so
# would a drop-in directory left by it before the first installation; ours would shadow the units
# and launcher of a package. install.sh must refuse all of them and install nothing. Runs on a
# machine without an installation.
check_refuses_shadowing() {
  local out ok=1 path
  sh_c 'printf "[Service]\nExecStart=/bin/true\n" > /etc/systemd/system/polkadot-validator.service'
  if out=$(in_c /opt/metanode/src/install.sh --bin-dir /opt/metanode/bin 2>&1); then
    log "install.sh installed although /etc/systemd/system/polkadot-validator.service exists"
    ok=0
  elif ! grep -q "/etc/systemd/system/polkadot-validator.service exists" <<<"$out"; then
    log "unexpected refusal: $out"
    ok=0
  fi
  sh_c 'rm /etc/systemd/system/polkadot-validator.service'
  sh_c 'mkdir -p /etc/systemd/system/polkadot-asset-hub-collator.service.d &&
    printf "[Service]\nExecStart=\nExecStart=/bin/true\n" > /etc/systemd/system/polkadot-asset-hub-collator.service.d/override.conf'
  if out=$(in_c /opt/metanode/src/install.sh --bin-dir /opt/metanode/bin 2>&1); then
    log "install.sh installed although polkadot-asset-hub-collator.service.d/ predates it"
    ok=0
  elif ! grep -q "polkadot-asset-hub-collator.service.d/ predates" <<<"$out"; then
    log "unexpected refusal: $out"
    ok=0
  fi
  sh_c 'rm -r /etc/systemd/system/polkadot-asset-hub-collator.service.d'
  # Units or the launcher of a package: ours would take precedence over them.
  for path in /lib/systemd/system/polkadot-people-collator.service /usr/bin/polkadot-metanodectl; do
    sh_c "printf '# stand-in for a packaged file\n' > $path"
    if out=$(in_c /opt/metanode/src/install.sh --bin-dir /opt/metanode/bin 2>&1); then
      log "install.sh installed although $path exists"
      ok=0
    elif ! grep -q "$path is installed by a package" <<<"$out"; then
      log "unexpected refusal: $out"
      ok=0
    fi
    in_c rm "$path"
  done
  in_c test ! -e /usr/local/bin/polkadot-metanodectl || {
    log "install.sh installed files before refusing"
    ok=0
  }
  [ "$ok" = 1 ]
}

# The polkadot package may stay installed next to ours: install.sh warns, naming both versions.
check_warns_packaged_polkadot() {
  local out ok=1
  sh_c 'printf "#!/bin/sh\necho polkadot 1.0.0-packaged\n" > /usr/bin/polkadot && chmod 0755 /usr/bin/polkadot'
  if out=$(in_c /opt/metanode/src/install.sh --bin-dir /opt/metanode/bin 2>&1); then
    grep -q "warning: the polkadot package's /usr/bin/polkadot (1.0.0) is installed too" <<<"$out" || {
      log "no warning about /usr/bin/polkadot: $out"
      ok=0
    }
  else
    log "install.sh refused: $out"
    ok=0
  fi
  in_c rm /usr/bin/polkadot
  [ "$ok" = 1 ]
}

check_upgrade() {
  local peers
  # A different polkadot-parachain: same version, one extra byte (ELF ignores trailing data). Copy
  # only the four binaries: the mounted bin dir may be a whole target/release, and /tmp is a tmpfs
  # on Debian 13.
  sh_c 'rm -rf /var/tmp/bin2 && mkdir /var/tmp/bin2 &&
    cp /opt/metanode/bin/{polkadot,polkadot-prepare-worker,polkadot-execute-worker,polkadot-parachain} /var/tmp/bin2/ &&
    printf x >> /var/tmp/bin2/polkadot-parachain'
  if in_c /opt/metanode/src/install.sh --bin-dir /var/tmp/bin2 >/dev/null 2>&1; then
    log "install.sh replaced binaries under running roles"
    return 1
  fi
  in_c cmp -s /usr/local/bin/polkadot-parachain /opt/metanode/bin/polkadot-parachain || return 1
  peers="$(peer_id validator) $(peer_id asset-hub-collator) $(peer_id people-collator)"
  in_c /opt/metanode/src/install.sh --bin-dir /var/tmp/bin2 --restart >/dev/null || return 1
  in_c cmp -s /usr/local/bin/polkadot-parachain /var/tmp/bin2/polkadot-parachain || return 1
  wait_serving "${ROLES[@]}" || return 1
  [ "$(unit_prop "$c" polkadot-validator.service ActiveEnterTimestampMonotonic)" -le \
    "$(unit_prop "$c" polkadot-asset-hub-collator.service ActiveEnterTimestampMonotonic)" ] || return 1
  [ "$peers" = "$(peer_id validator) $(peer_id asset-hub-collator) $(peer_id people-collator)" ]
}

# --uninstall keeps configuration, state and users, and records the enabled roles for a package to
# enable again (the roles are enabled one by one here, see check_all_target).
check_uninstall() {
  local roles
  # A link into our unit directory that `systemctl disable` does not remove.
  sh_c 'mkdir -p /etc/systemd/system/other.service.d && ln -s \
    /usr/local/lib/systemd/system/polkadot-people-collator.service.d/10-common.conf /etc/systemd/system/other.service.d/'
  in_c /opt/metanode/src/install.sh --uninstall >/dev/null || return 1
  ! in_c systemctl cat polkadot-validator.service >/dev/null 2>&1 || return 1
  in_c test -d /var/lib/polkadot-validator/chains || return 1
  in_c getent passwd polkadot-validator >/dev/null || return 1
  roles=$(in_c cat /etc/polkadot-metanode/enabled-roles) || return 1
  [ "$roles" = "$(printf '%s\n' "${ROLES[@]}")" ] || {
    log "enabled-roles lists: $roles"
    return 1
  }
  [ -z "$(in_c find /etc/systemd/system -lname '/usr/local/lib/systemd/system/*')" ] || {
    log "links into /usr/local/lib/systemd/system are left"
    return 1
  }
  in_c rmdir /etc/systemd/system/other.service.d
  in_c /opt/metanode/src/install.sh --uninstall --purge --yes >/dev/null || return 1
  local role
  for role in "${ROLES[@]}"; do
    ! in_c test -e "/var/lib/polkadot-$role" || return 1
    ! in_c test -e "/usr/local/lib/systemd/system/polkadot-$role.service.d" || return 1
    ! in_c getent passwd "polkadot-$role" >/dev/null || return 1
  done
  ! in_c test -e /etc/polkadot-metanode && ! in_c test -e /usr/local/bin/polkadot &&
    ! in_c test -e /usr/local/bin/polkadot-metanodectl && ! in_c test -e /usr/local/lib/polkadot-metanode &&
    ! in_c test -e /usr/local/lib/systemd/system/polkadot.slice
}

# ---- run ---------------------------------------------------------------------------------------

setup
check "$distro: systemd-analyze verify reports nothing about our units" check_units_verify
check "$distro: units load from /usr/local/lib/systemd/system and run the binaries of /usr/local/bin" \
  check_installed_layout
check "$distro: the common drop-in applies to our units only" check_common_dropin_scope
report_security_scores
check "$distro: install.sh --enable validator,asset-hub,people (aliases) starts them" \
  in_c /opt/metanode/src/install.sh --bin-dir /opt/metanode/bin --enable validator,asset-hub,people
if ! wait_serving "${ROLES[@]}"; then
  fail "$distro: roles did not come up"
  for role in "${ROLES[@]}"; do journal "$c" "polkadot-$role.service" | tail -n 20; done
  exit 1
fi
for role in "${ROLES[@]}"; do
  check "$distro: polkadot-$role user has its state dir as home and no login shell" check_user "$role"
  check "$distro: /var/lib/polkadot-$role is 0700 and owned by its user" check_state_dir "$role"
  check "$distro: $role node.key is 0600, owned by its user, 64 bytes" check_node_key "$role"
done
check "$distro: validator authors blocks" wait_until 120 "validator blocks" reached_block "$c" 9944 3
check "$distro: polkadot-metanodectl status shows all roles active" check_status_helper
check "$distro: sandbox parity with the host's PVF security checks" check_sandbox_parity
check "$distro: collators use ParityDB and SYNC from metanode.env, People its statement store" check_storage_defaults
check "$distro: a collator cannot read the validator state" check_isolation
check "$distro: nodes run with NoNewPrivs and no effective capabilities" check_no_privileges
check "$distro: validator listens on 30333 9944 9945 9615" listens validator 30333 9944 9945 9615
check "$distro: asset-hub-collator listens on 30334 30335 9946 9947 9616 9617" \
  listens asset-hub-collator 30334 30335 9946 9947 9616 9617
check "$distro: people-collator listens on 30336 30337 9948 9949 9618 9619" \
  listens people-collator 30336 30337 9948 9949 9618 9619
check "$distro: the safe RPC refuses unsafe methods, the localhost one serves them" check_rpc_endpoints
check "$distro: every role serves WebRTC on its UDP p2p port, with a certhash" check_webrtc
check "$distro: validator weights and slice" check_resources
check "$distro: validator memory protection (MemoryLow on it and its slices, OOM preference)" \
  check_memory_protection
check "$distro: rotate-keys on the validator" check_rotate_keys validator
check "$distro: rotate-keys on asset-hub (alias)" check_rotate_keys asset-hub
check "$distro: restart keeps the peer id" check_peer_id_stable
check "$distro: restart settings (2min, 10s, no start limit)" check_restart_properties
check "$distro: a killed collator is restarted" check_crash_restart
check "$distro: systemctl stop is a clean shutdown" check_graceful_stop
check "$distro: an invalid value fails the unit with a usage error and no restart loop" check_config_error
check "$distro: doctor fails a relay RPC of another chain, and passes the validator's" check_relay_genesis
check "$distro: doctor reports a missing PVF worker or one of another version" check_workers
check "$distro: collators ride out a validator outage" check_validator_outage
check "$distro: doctor passes, and reports a port taken by another program" check_doctor
check "$distro: install.sh refuses the bandwidth reservation without a route" check_qos_refused_without_route
check "$distro: Conflicts=polkadot.service works both ways" check_conflicts
check "$distro: polkadot-metanode.target stops all roles and starts only enabled ones" check_target
check "$distro: enabled roles come back after a reboot" check_reboot
check "$distro: polkadot-metanode-all.target runs every role, also after a reboot" check_all_target
check "$distro: re-running install.sh changes nothing" check_reinstall_idempotent
check "$distro: --config installs a shared metanode.env, replacing one only with --force-config" \
  check_config_file
check "$distro: upgrades are refused while running and done by --restart" check_upgrade
check "$distro: --uninstall keeps state, --purge removes it" check_uninstall
check "$distro: install.sh refuses units that would shadow ours, or that ours would shadow" \
  check_refuses_shadowing
check "$distro: install.sh next to the polkadot package warns with both versions" \
  check_warns_packaged_polkadot

log "$distro: $FAILURES failure(s)"
exit $((FAILURES > 0))
