# Shared helpers of the metanode docker harness; sourced by run.sh, lifecycle.sh and e2e.sh.
# shellcheck shell=bash

TESTS_DIR=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
METANODE_SRC=$(cd "$TESTS_DIR/.." && pwd)
IMAGE=polkadot-metanode-test
# Every container and network of the harness carries this label, plus one naming its run, so that
# cleanup only removes what the current run created and runs can go in parallel.
LABEL=polkadot-metanode-test
RUN_ID=${RUN_ID:-$$}
export RUN_ID
ALL_DISTROS=(ubuntu-22.04 ubuntu-24.04 debian-12 debian-13)
FAILURES=${FAILURES:-0}

log() {
  printf '[%s] %s\n' "$(date +%T)" "$*"
}

die() {
  log "ERROR: $*"
  exit 1
}

pass() {
  log "ok    $*"
}

fail() {
  log "FAIL  $*"
  FAILURES=$((FAILURES + 1))
}

# check <description> <command...>: records a pass or a failure and carries on.
check() {
  local desc=$1
  shift
  if "$@"; then pass "$desc"; else fail "$desc"; fi
}

# wait_until <seconds> <description> <command...>: polls every 2s until the command succeeds.
wait_until() {
  local timeout=$1 desc=$2 deadline
  shift 2
  deadline=$((SECONDS + timeout))
  until "$@"; do
    if ((SECONDS >= deadline)); then
      log "timed out after ${timeout}s waiting for: $desc"
      return 1
    fi
    sleep 2
  done
}

base_image() {
  case "$1" in
    ubuntu-22.04) echo ubuntu:22.04 ;;
    ubuntu-24.04) echo ubuntu:24.04 ;;
    debian-12) echo debian:12 ;;
    debian-13) echo debian:13 ;;
    *) die "unknown distro '$1' (expected: ${ALL_DISTROS[*]})" ;;
  esac
}

build_image() {
  log "building $IMAGE:$1 from $(base_image "$1")"
  docker build -q --build-arg BASE="$(base_image "$1")" -t "$IMAGE:$1" \
    -f "$TESTS_DIR/Dockerfile.systemd" "$TESTS_DIR" >/dev/null
}

systemd_up() {
  local state
  state=$(docker exec "$1" systemctl is-system-running 2>/dev/null) || true
  [ "$state" = running ] || [ "$state" = degraded ]
}

# start_container <name> <distro> [docker run args...]: boots systemd with the binaries and
# scripts/metanode mounted read-only at /opt/metanode/{bin,src}.
start_container() {
  local name=$1 distro=$2
  shift 2
  docker rm -f "$name" >/dev/null 2>&1 || true
  docker run -d --name "$name" --hostname "$name" --label "$LABEL" --label "$LABEL.run=$RUN_ID" \
    --privileged --cgroupns=private --tmpfs /run --tmpfs /run/lock \
    -v "$BIN_DIR:/opt/metanode/bin:ro" -v "$METANODE_SRC:/opt/metanode/src:ro" \
    "$@" "$IMAGE:$distro" >/dev/null
  wait_until 90 "systemd in $name" systemd_up "$name"
}

cleanup_containers() {
  local ids run_label=$LABEL.run=$RUN_ID
  if [ "${KEEP:-0}" = 1 ]; then
    log "--keep: leaving $(docker ps -q --filter "label=$run_label" | wc -l) container(s) running"
    return
  fi
  mapfile -t ids < <(docker ps -aq --filter "label=$run_label")
  [ ${#ids[@]} -eq 0 ] || docker rm -f "${ids[@]}" >/dev/null 2>&1 || true
  mapfile -t ids < <(docker network ls -q --filter "label=$run_label")
  [ ${#ids[@]} -eq 0 ] || docker network rm "${ids[@]}" >/dev/null 2>&1 || true
}

# rpc <container> <port> <method> [params]: JSON-RPC call to a node inside a container.
rpc() {
  docker exec "$1" curl -fsS -m 10 -H 'Content-Type: application/json' \
    -d "{\"id\":1,\"jsonrpc\":\"2.0\",\"method\":\"$3\",\"params\":${4:-[]}}" "http://127.0.0.1:$2"
}

# Best and finalized block numbers of the node behind <container> <port>.
best_number() {
  local hex
  hex=$(rpc "$1" "$2" chain_getHeader | jq -r '.result.number') || return 1
  echo $((hex))
}

finalized_number() {
  local head hex
  head=$(rpc "$1" "$2" chain_getFinalizedHead | jq -r '.result') || return 1
  hex=$(rpc "$1" "$2" chain_getHeader "[\"$head\"]" | jq -r '.result.number') || return 1
  echo $((hex))
}

# Succeeds once the chain behind <container> <port> reached block <n> (best, or finalized with
# the fourth argument "finalized").
reached_block() {
  local number
  if [ "${4:-best}" = finalized ]; then
    number=$(finalized_number "$1" "$2" 2>/dev/null) || return 1
  else
    number=$(best_number "$1" "$2" 2>/dev/null) || return 1
  fi
  [ "$number" -ge "$3" ]
}

unit_prop() {
  docker exec "$1" systemctl show -p "$3" --value "$2"
}

unit_active() {
  [ "$(docker exec "$1" systemctl is-active "$2" 2>/dev/null)" = active ]
}

unit_inactive() {
  ! unit_active "$@"
}

# The command name of a unit's main process, e.g. "polkadot" once polkadot-metanodectl exec'd the node.
main_comm() {
  local pid
  pid=$(unit_prop "$1" "$2" MainPID)
  [ "$pid" != 0 ] || return 1
  docker exec "$1" ps -o comm= -p "$pid"
}

# Succeeds once a role's node (not polkadot-metanodectl) runs and its RPC answers.
role_serving() {
  local container=$1 role=$2 port=$3 comm
  comm=$(main_comm "$container" "polkadot-$role.service" 2>/dev/null) || return 1
  [[ "$comm" == polkadot* ]] || return 1
  rpc "$container" "$port" system_health >/dev/null 2>&1
}

journal() {
  docker exec "$1" journalctl -u "$2" --no-pager -o cat
}

# journal_has <container> <unit> <since> <pattern>: the unit's journal since <since> (empty for
# all of it) contains <pattern>. The journal is read in full first: `journalctl | grep -q` fails
# under pipefail as soon as grep exits early and journalctl dies of SIGPIPE.
journal_has() {
  local args=(journalctl -u "$2" --no-pager -o cat) out
  [ -z "$3" ] || args+=(--since "$3")
  out=$(docker exec "$1" "${args[@]}") || return 1
  grep -q -- "$4" <<<"$out"
}

host_facts() {
  log "host: kernel $(uname -r), apparmor_restrict_unprivileged_userns=$(sysctl -n kernel.apparmor_restrict_unprivileged_userns 2>/dev/null || echo n/a), docker $(docker version --format '{{.Server.Version}}')"
  log "note: containers share this kernel and its LSMs, so landlock/userns/AppArmor results describe the host"
}
