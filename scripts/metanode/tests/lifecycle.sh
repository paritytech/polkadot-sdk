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
declare -A RPC=([validator]=9944 [asset-hub]=9954 [people]=9964)
ROLES=(validator asset-hub people)

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
  # as their chains are not registered on westend-dev.
  # shellcheck disable=SC2016 # $role expands inside the container
  sh_c 'cat > /etc/polkadot-metanode/metanode.conf <<EOF
NETWORK=westend
NODE_NAME=life
EOF
cat > /etc/polkadot-metanode/validator.conf <<EOF
CHAIN=westend-dev
EXTRA_ARGS="--alice --force-authoring"
EOF
for role in asset-hub people; do
  cat > /etc/polkadot-metanode/$role.conf <<EOF
CHAIN=$role-westend-dev
RELAY_CHAIN=westend-dev
EXTRA_ARGS="--alice"
EOF
done'
}

# ---- tier 0: unit lint -------------------------------------------------------------------------

check_units_verify() {
  local out
  out=$(sh_c 'cd /usr/local/lib/systemd/system &&
    systemd-analyze verify polkadot-validator.service polkadot-asset-hub.service \
      polkadot-people.service polkadot-metanode.target polkadot-metanode.slice 2>&1') || true
  if grep -E "polkadot-(validator|asset-hub|people|metanode)|10-common.conf" <<<"$out"; then
    return 1
  fi
}

# install.sh installs the unit templates with this layout's launcher path filled in.
check_installed_layout() {
  local role fragment exec ok=1
  for role in "${ROLES[@]}"; do
    fragment=$(unit_prop "$c" "polkadot-$role.service" FragmentPath)
    exec=$(unit_prop "$c" "polkadot-$role.service" ExecStart)
    if [ "$fragment" != "/usr/local/lib/systemd/system/polkadot-$role.service" ] ||
      [[ "$exec" != *"/usr/local/bin/polkadot-metanodectl run $role "* || "$exec" == *@BINDIR@* ]]; then
      log "polkadot-$role.service: FragmentPath=$fragment ExecStart=$exec"
      ok=0
    fi
  done
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
  [ "$(in_c stat -c '%a %U %s' "/var/lib/polkadot-$1/node-key")" = "600 polkadot-$1 64" ]
}

check_status_helper() {
  local out
  out=$(in_c polkadot-metanodectl status) || return 1
  echo "$out"
  [ "$(grep -cE '^(validator|asset-hub|people) +enabled +active ' <<<"$out")" = 3 ]
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
  declare -A p2p=([validator]=30333 [asset-hub]=30343 [people]=30353)
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

# Collators use the database of the docs.polkadot.com collator guide, the validator the node's own.
check_storage_defaults() {
  local role ok=1
  for role in asset-hub people; do
    journal_has "$c" "polkadot-$role.service" "" "Database: ParityDb at /var/lib/polkadot-$role/" || {
      log "polkadot-$role does not use ParityDB"
      ok=0
    }
  done
  journal_has "$c" polkadot-validator.service "" "Database: RocksDb at /var/lib/polkadot-validator/" || {
    log "the validator does not use the node's default database (RocksDB)"
    ok=0
  }
  [ "$ok" = 1 ]
}

check_isolation() {
  ! in_c runuser -u polkadot-asset-hub -- ls /var/lib/polkadot-validator >/dev/null 2>&1
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

check_resources() {
  [ "$(unit_prop "$c" polkadot-validator.service CPUWeight)" = 1000 ] &&
    [ "$(unit_prop "$c" polkadot-validator.service IOWeight)" = 1000 ] &&
    [ "$(unit_prop "$c" polkadot-validator.service Slice)" = polkadot-metanode.slice ] &&
    [ "$(unit_prop "$c" polkadot-asset-hub.service Slice)" = polkadot-metanode.slice ] &&
    [ "$(in_c cat /sys/fs/cgroup/polkadot.slice/polkadot-metanode.slice/polkadot-validator.service/cpu.weight)" = 1000 ]
}

# rotate-keys must return keys and a proof, and the node must then hold those keys.
check_rotate_keys() {
  local role=$1 out keys proof
  out=$(in_c polkadot-metanodectl rotate-keys "$role" "$ALICE") || return 1
  keys=$(awk '$1 == "keys:" { print $2 }' <<<"$out")
  proof=$(awk '$1 == "proof:" { print $2 }' <<<"$out")
  [[ "$keys" == 0x* && "$proof" == 0x* ]] || return 1
  [ "$(rpc "$c" "${RPC[$role]}" author_hasSessionKeys "[\"$keys\"]" | jq -r '.result')" = true ]
}

check_peer_id_stable() {
  local before after
  before=$(peer_id asset-hub)
  in_c systemctl restart polkadot-asset-hub.service
  wait_serving asset-hub || return 1
  after=$(peer_id asset-hub)
  [ -n "$before" ] && [ "$before" = "$after" ]
}

# restarted_since <role> <count>: systemd restarted the role more than <count> times.
restarted_since() {
  [ "$(unit_prop "$c" "polkadot-$1.service" NRestarts)" -gt "$2" ]
}

# SIGKILL the node itself (a crash), then check systemd saw exactly that and restarted it.
check_crash_restart() {
  local before pid since
  before=$(unit_prop "$c" polkadot-people.service NRestarts)
  pid=$(unit_prop "$c" polkadot-people.service MainPID)
  since=$(in_c date '+%Y-%m-%d %H:%M:%S')
  in_c kill -9 "$pid"
  wait_until 60 "people restarted" restarted_since people "$before" || return 1
  journal_has "$c" polkadot-people.service "$since" "status=9/KILL" || {
    log "systemd recorded no SIGKILL of the main process"
    return 1
  }
  wait_serving people
}

check_restart_properties() {
  [ "$(unit_prop "$c" polkadot-validator.service RestartUSec)" = 2min ] &&
    [ "$(unit_prop "$c" polkadot-asset-hub.service RestartUSec)" = 10s ] &&
    [ "$(unit_prop "$c" polkadot-people.service StartLimitIntervalUSec)" = 0 ]
}

check_graceful_stop() {
  in_c systemctl stop polkadot-people.service
  local result
  result=$(unit_prop "$c" polkadot-people.service Result)
  in_c systemctl start polkadot-people.service
  [ "$result" = success ] || {
    log "stop result: $result"
    return 1
  }
  wait_serving people
}

check_config_error() {
  local status sub
  sh_c 'echo AUTHORING=bogus >> /etc/polkadot-metanode/people.conf'
  in_c systemctl restart polkadot-people.service || true
  sleep 15
  status=$(unit_prop "$c" polkadot-people.service ExecMainStatus)
  sub=$(unit_prop "$c" polkadot-people.service SubState)
  sh_c "sed -i '/^AUTHORING=bogus$/d' /etc/polkadot-metanode/people.conf"
  in_c systemctl reset-failed polkadot-people.service
  in_c systemctl start polkadot-people.service
  [ "$status" = 78 ] && [ "$sub" = failed ] || {
    log "config error gave status $status, substate $sub"
    return 1
  }
  wait_serving people
}

# failed_with <role> <status>: the unit failed with that exit status and is not being restarted.
failed_with() {
  [ "$(unit_prop "$c" "polkadot-$1.service" SubState)" = failed ] &&
    [ "$(unit_prop "$c" "polkadot-$1.service" ExecMainStatus)" = "$2" ]
}

# The validator takes its PVF workers from BIN_DIR, else from /usr/lib/polkadot (the polkadot
# package's), and a missing worker or one of another version fails it as a configuration error.
check_workers_resolution() {
  local since out ok=1
  sh_c 'mkdir -p /usr/lib/polkadot &&
    mv /usr/local/bin/polkadot-prepare-worker /usr/local/bin/polkadot-execute-worker /usr/lib/polkadot/'
  out=$(in_c polkadot-metanodectl print validator)
  [[ "$out" == *" --workers-path /usr/lib/polkadot "* ]] || {
    log "print validator: $out"
    ok=0
  }
  since=$(in_c date '+%Y-%m-%d %H:%M:%S')
  in_c systemctl restart polkadot-validator.service
  wait_serving validator || ok=0
  journal_has "$c" polkadot-validator.service "$since" "Running in Secure Validator Mode" || {
    log "the validator with the workers of /usr/lib/polkadot is not in Secure Validator Mode"
    ok=0
  }
  sh_c 'mv /usr/lib/polkadot/polkadot-execute-worker /var/tmp/'
  in_c systemctl restart polkadot-validator.service || true
  wait_until 60 "validator fails without its execute worker" failed_with validator 78 || ok=0
  sh_c 'printf "#!/bin/sh\necho 0.0.1\n" > /usr/lib/polkadot/polkadot-execute-worker &&
    chmod 0755 /usr/lib/polkadot/polkadot-execute-worker'
  since=$(in_c date '+%Y-%m-%d %H:%M:%S')
  in_c systemctl reset-failed polkadot-validator.service
  in_c systemctl start polkadot-validator.service || true
  wait_until 60 "validator fails with a worker of another version" failed_with validator 78 || ok=0
  journal_has "$c" polkadot-validator.service "$since" "polkadot-execute-worker is version 0.0.1 but" || {
    log "no version mismatch reported"
    ok=0
  }
  sh_c 'mv -f /var/tmp/polkadot-execute-worker /usr/lib/polkadot/ &&
    mv /usr/lib/polkadot/polkadot-prepare-worker /usr/lib/polkadot/polkadot-execute-worker /usr/local/bin/ &&
    rmdir /usr/lib/polkadot'
  in_c systemctl reset-failed polkadot-validator.service
  in_c systemctl start polkadot-validator.service
  [ "$ok" = 1 ] && wait_serving validator asset-hub people
}

# While the validator is down, a collator's node exits once its RPC retries run out, systemd
# restarts it, and polkadot-metanodectl waits for the relay RPC before starting the node again.
waiting_for_relay_since() {
  journal_has "$c" polkadot-asset-hub.service "$1" "waiting for relay-chain RPC"
}

check_validator_outage() {
  local since before
  since=$(in_c date '+%Y-%m-%d %H:%M:%S')
  before=$(unit_prop "$c" polkadot-asset-hub.service NRestarts)
  in_c systemctl stop polkadot-validator.service
  wait_until 180 "asset-hub waits for the relay RPC" waiting_for_relay_since "$since" || return 1
  restarted_since asset-hub "$before" || {
    log "asset-hub did not restart after losing the relay RPC"
    return 1
  }
  in_c systemctl start polkadot-validator.service
  wait_serving validator asset-hub people
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
  [ "$ok" = 1 ] && wait_serving validator asset-hub people
}

check_target() {
  local ok=0
  in_c systemctl stop polkadot-metanode.target
  sleep 3
  if unit_inactive "$c" polkadot-validator.service && unit_inactive "$c" polkadot-asset-hub.service &&
    unit_inactive "$c" polkadot-people.service; then
    in_c systemctl disable --quiet polkadot-people.service
    in_c systemctl start polkadot-metanode.target
    sleep 3
    if unit_active "$c" polkadot-validator.service && unit_active "$c" polkadot-asset-hub.service &&
      unit_inactive "$c" polkadot-people.service; then
      ok=1
    fi
    in_c systemctl enable --now --quiet polkadot-people.service
  fi
  [ "$ok" = 1 ] && wait_serving validator asset-hub people
}

check_reboot() {
  # Give systemd time to stop the nodes cleanly, as a real reboot would.
  docker restart --time 60 "$c" >/dev/null
  wait_until 90 "systemd after reboot" systemd_up "$c" || return 1
  wait_serving validator asset-hub people
}

# A machine that runs every role uses polkadot-metanode-all.target instead of enabling the roles one
# by one: install.sh --enable all, the roles come back after a reboot although none is enabled
# itself, and stopping the target stops them all.
check_all_target() {
  local ok=0
  in_c systemctl disable --quiet polkadot-validator.service polkadot-asset-hub.service \
    polkadot-people.service
  if in_c /opt/metanode/src/install.sh --bin-dir /opt/metanode/bin --enable all >/dev/null &&
    [ "$(in_c systemctl is-enabled polkadot-metanode-all.target)" = enabled ] &&
    docker restart --time 60 "$c" >/dev/null &&
    wait_until 90 "systemd after reboot" systemd_up "$c" &&
    wait_serving validator asset-hub people; then
    in_c systemctl stop polkadot-metanode-all.target
    sleep 3
    if unit_inactive "$c" polkadot-validator.service && unit_inactive "$c" polkadot-asset-hub.service &&
      unit_inactive "$c" polkadot-people.service; then
      ok=1
    fi
  fi
  # Back to roles enabled one by one for the remaining checks.
  in_c systemctl disable --quiet polkadot-metanode-all.target
  in_c systemctl enable --now --quiet polkadot-validator.service polkadot-asset-hub.service \
    polkadot-people.service
  [ "$ok" = 1 ] && wait_serving validator asset-hub people
}

rpc_up() {
  rpc "$c" "$1" system_health >/dev/null 2>&1
}

no_run_all_left() {
  ! in_c pgrep -f "run-al[l]" >/dev/null && ! in_c pgrep -f "/var/tmp/run-al[l]/polkadot" >/dev/null
}

# run-all runs the roles in the foreground without systemd: every node serves, every output line
# is prefixed by its role, SIGTERM stops them all, and a bad role starts nothing.
check_run_all() {
  local ok=1 role port pid status=0
  in_c systemctl stop polkadot-metanode.target
  sleep 3
  # Its own base dir, so the state of the units is untouched; the configuration is the same.
  sh_c 'rm -rf /var/tmp/run-all && mkdir /var/tmp/run-all
    POLKADOT_METANODE_BASE_DIR=/var/tmp/run-all nohup polkadot-metanodectl run-all \
      > /var/tmp/run-all/out.log 2>&1 &
    echo $! > /var/tmp/run-all/pid'
  pid=$(in_c cat /var/tmp/run-all/pid)
  for port in 9944 9954 9964; do
    wait_until 300 "run-all node on port $port" rpc_up "$port" || ok=0
  done
  for role in "${ROLES[@]}"; do
    in_c grep -q "^\[$role\] " /var/tmp/run-all/out.log || {
      log "run-all printed no [$role] lines"
      ok=0
    }
  done
  in_c kill -TERM "$pid"
  wait_until 120 "run-all and its nodes to exit" no_run_all_left || ok=0
  in_c polkadot-metanodectl run-all validator bogus >/dev/null 2>&1 || status=$?
  [ "$status" = 78 ] || {
    log "run-all with an unknown role exited $status instead of 78"
    ok=0
  }
  in_c systemctl start polkadot-metanode.target
  [ "$ok" = 1 ] && wait_serving validator asset-hub people
}

# Re-installing changes nothing, and an operator's drop-in override of one of our units does not
# block it (unlike a unit file in /etc, see check_refuses_shadowing).
check_reinstall_idempotent() {
  local sums pids ok=0
  sh_c 'mkdir -p /etc/systemd/system/polkadot-people.service.d &&
    printf "[Service]\nEnvironment=OPERATOR_OVERRIDE=1\n" > /etc/systemd/system/polkadot-people.service.d/override.conf'
  sums=$(sh_c 'md5sum /etc/polkadot-metanode/*.conf')
  pids=$(main_pids)
  if in_c /opt/metanode/src/install.sh --bin-dir /opt/metanode/bin >/dev/null; then
    [ "$sums" = "$(sh_c 'md5sum /etc/polkadot-metanode/*.conf')" ] && [ "$pids" = "$(main_pids)" ] &&
      ok=1
  fi
  sh_c 'rm -r /etc/systemd/system/polkadot-people.service.d && systemctl daemon-reload'
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

# A unit file in /etc (the docs' hand-written polkadot-validator.service) would shadow ours, and so
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
  sh_c 'mkdir -p /etc/systemd/system/polkadot-asset-hub.service.d &&
    printf "[Service]\nExecStart=\nExecStart=/bin/true\n" > /etc/systemd/system/polkadot-asset-hub.service.d/override.conf'
  if out=$(in_c /opt/metanode/src/install.sh --bin-dir /opt/metanode/bin 2>&1); then
    log "install.sh installed although polkadot-asset-hub.service.d/ predates it"
    ok=0
  elif ! grep -q "polkadot-asset-hub.service.d/ predates" <<<"$out"; then
    log "unexpected refusal: $out"
    ok=0
  fi
  sh_c 'rm -r /etc/systemd/system/polkadot-asset-hub.service.d'
  # Units or the launcher of a package: ours would take precedence over them.
  for path in /lib/systemd/system/polkadot-people.service /usr/bin/polkadot-metanodectl; do
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
  peers="$(peer_id validator) $(peer_id asset-hub) $(peer_id people)"
  in_c /opt/metanode/src/install.sh --bin-dir /var/tmp/bin2 --restart >/dev/null || return 1
  in_c cmp -s /usr/local/bin/polkadot-parachain /var/tmp/bin2/polkadot-parachain || return 1
  wait_serving validator asset-hub people || return 1
  [ "$(unit_prop "$c" polkadot-validator.service ActiveEnterTimestampMonotonic)" -le \
    "$(unit_prop "$c" polkadot-asset-hub.service ActiveEnterTimestampMonotonic)" ] || return 1
  [ "$peers" = "$(peer_id validator) $(peer_id asset-hub) $(peer_id people)" ]
}

# --uninstall keeps configuration, state and users, and records the enabled roles for a package to
# enable again (the roles are enabled one by one here, see check_all_target).
check_uninstall() {
  local roles
  # A link into our unit directory that `systemctl disable` does not remove.
  sh_c 'mkdir -p /etc/systemd/system/other.service.d && ln -s \
    /usr/local/lib/systemd/system/polkadot-people.service.d/10-common.conf /etc/systemd/system/other.service.d/'
  in_c /opt/metanode/src/install.sh --uninstall >/dev/null || return 1
  ! in_c systemctl cat polkadot-validator.service >/dev/null 2>&1 || return 1
  in_c test -d /var/lib/polkadot-validator/chains || return 1
  in_c getent passwd polkadot-validator >/dev/null || return 1
  roles=$(in_c cat /etc/polkadot-metanode/enabled-roles) || return 1
  [ "$roles" = "$(printf '%s\n' validator asset-hub people)" ] || {
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
    ! in_c test -e /usr/local/bin/polkadot-metanodectl && ! in_c test -e /usr/local/lib/polkadot-metanode
}

# ---- run ---------------------------------------------------------------------------------------

setup
check "$distro: systemd-analyze verify reports nothing about our units" check_units_verify
check "$distro: units load from /usr/local/lib/systemd/system and run /usr/local/bin/polkadot-metanodectl" \
  check_installed_layout
check "$distro: the common drop-in applies to our units only" check_common_dropin_scope
report_security_scores
check "$distro: install.sh --enable validator,asset-hub,people starts them" \
  in_c /opt/metanode/src/install.sh --bin-dir /opt/metanode/bin --enable validator,asset-hub,people
if ! wait_serving validator asset-hub people; then
  fail "$distro: roles did not come up"
  for role in "${ROLES[@]}"; do journal "$c" "polkadot-$role.service" | tail -n 20; done
  exit 1
fi
for role in "${ROLES[@]}"; do
  check "$distro: polkadot-$role user has its state dir as home and no login shell" check_user "$role"
  check "$distro: /var/lib/polkadot-$role is 0700 and owned by its user" check_state_dir "$role"
  check "$distro: $role node-key is 0600, owned by its user, 64 bytes" check_node_key "$role"
done
check "$distro: validator authors blocks" wait_until 120 "validator blocks" reached_block "$c" 9944 3
check "$distro: polkadot-metanodectl status shows all roles active" check_status_helper
check "$distro: sandbox parity with the host's PVF security checks" check_sandbox_parity
check "$distro: collators use ParityDB, the validator the node default" check_storage_defaults
check "$distro: a collator cannot read the validator state" check_isolation
check "$distro: nodes run with NoNewPrivs and no effective capabilities" check_no_privileges
check "$distro: validator listens on 30333 9944 9615" listens validator 30333 9944 9615
check "$distro: asset-hub listens on 30343 30344 9954 9625 9626" listens asset-hub 30343 30344 9954 9625 9626
check "$distro: people listens on 30353 30354 9964 9635 9636" listens people 30353 30354 9964 9635 9636
check "$distro: every role serves WebRTC on its UDP p2p port, with a certhash" check_webrtc
check "$distro: validator weights and slice" check_resources
check "$distro: rotate-keys on the validator" check_rotate_keys validator
check "$distro: rotate-keys on asset-hub" check_rotate_keys asset-hub
check "$distro: restart keeps the peer id" check_peer_id_stable
check "$distro: restart settings (2min, 10s, no start limit)" check_restart_properties
check "$distro: a killed collator is restarted" check_crash_restart
check "$distro: systemctl stop is a clean shutdown" check_graceful_stop
check "$distro: a config error fails the unit with 78 and no restart loop" check_config_error
check "$distro: validator finds the packaged workers; a missing or mismatched one is a 78" \
  check_workers_resolution
check "$distro: collators ride out a validator outage" check_validator_outage
check "$distro: Conflicts=polkadot.service works both ways" check_conflicts
check "$distro: polkadot-metanode.target stops all roles and starts only enabled ones" check_target
check "$distro: enabled roles come back after a reboot" check_reboot
check "$distro: polkadot-metanode-all.target runs every role, also after a reboot" check_all_target
check "$distro: run-all runs every role without systemd and stops them on SIGTERM" check_run_all
check "$distro: re-running install.sh changes nothing" check_reinstall_idempotent
check "$distro: upgrades are refused while running and done by --restart" check_upgrade
check "$distro: --uninstall keeps state, --purge removes it" check_uninstall
check "$distro: install.sh refuses units that would shadow ours, or that ours would shadow" \
  check_refuses_shadowing
check "$distro: install.sh next to the polkadot package warns with both versions" \
  check_warns_packaged_polkadot

log "$distro: $FAILURES failure(s)"
exit $((FAILURES > 0))
