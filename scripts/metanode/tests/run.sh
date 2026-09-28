#!/usr/bin/env bash
# Docker test harness for the metanode systemd units.
#
#   tests/run.sh [lint|units|lifecycle|e2e|all] [--distro D]... [--bin-dir DIR] [--keep]
#
#   lint       shellcheck, and the units, polkadot-metanodectl and metanode.env agree (seconds)
#   units      per distro: the command lines the units start for a set of configurations, doctor's
#              configuration checks and the bandwidth reservation, with stand-in binaries (a minute)
#   lifecycle  per distro: install, systemd-analyze verify/security and the lifecycle checks
#   e2e        a four-machine local network producing relay, Asset Hub and People blocks
#
# --distro defaults to every supported distro (ubuntu-22.04 ubuntu-24.04 ubuntu-26.04 debian-12
# debian-13).
# --bin-dir defaults to target/release of this repository and must hold polkadot,
# polkadot-prepare-worker, polkadot-execute-worker and polkadot-parachain (x86_64, glibc <= 2.35).
# --keep leaves the containers running for inspection.
set -euo pipefail
# shellcheck source-path=SCRIPTDIR source=lib.sh
. "$(dirname "$0")/lib.sh"

tier=all
distros=()
BIN_DIR=
KEEP=0
while [ $# -gt 0 ]; do
  case "$1" in
    lint | units | lifecycle | e2e | all) tier=$1 ;;
    --distro) distros+=("${2:?--distro needs a name}"); shift ;;
    --bin-dir) BIN_DIR=${2:?--bin-dir needs a directory}; shift ;;
    --keep) KEEP=1 ;;
    -h | --help) sed -n '2,14p' "$0" | sed 's/^# \{0,1\}//'; exit 0 ;;
    *) die "unknown argument '$1' (see --help)" ;;
  esac
  shift
done
[ ${#distros[@]} -gt 0 ] || distros=("${ALL_DISTROS[@]}")
for distro in "${distros[@]}"; do base_image "$distro" >/dev/null; done
export KEEP

run_lint() {
  # Names from before the units became polkadot-<role> (built from pieces so this line does not
  # match itself).
  local old=metanode stale ah=asset-hub
  stale="$old-exec|$old-(validator|asset-hub|people)|(^|[^-])$old\.(target|slice)|/etc/$old|/var/lib/$old"
  stale+="|(^|[^_])METANODE_(BIN|CONF|BASE)_DIR|mn-(life|e2e)-"
  # Names from before the layout of operators/setup.md: the collators' units, users and state dirs
  # end in -collator, the node key is node.key, and RPC_LISTEN_IP replaced the private-address key.
  stale+="|polkadot-($ah|people)(\.service|\.conf|/|:)|(^|[^-])($ah|people)\.conf|RPC_PRIV""ATE_IP"
  stale+="|polkadot-[a-z-]+/node-key"
  # Names from before the units carried setup.md's commands: the launcher's run, print and run-all,
  # its configuration files and the settings only it read.
  stale+="|metanodectl (run|print|run-all)|(^|[^-])$old\.conf|(validator|$ah-collator|people-collator)\.conf"
  stale+="|RELAY_RPC_""WAIT|golden\.sh|tests/gol""den"
  log "lint: no pre-rename names"
  if grep -rnE "$stale" "$METANODE_SRC" "$METANODE_SRC/../../.github/workflows/tests-metanode-systemd.yml"; then
    fail "pre-rename names left (see above)"
  fi
  # `cmd | grep -q` fails under pipefail whenever grep exits before cmd is done writing (SIGPIPE),
  # so it passes or fails depending on output size and timing. Capture first, then grep. Lines
  # continued after a trailing `|` are joined; comments are ignored.
  log "lint: no grep -q at the end of a pipe"
  local file hits=0
  for file in "$METANODE_SRC"/install.sh "$METANODE_SRC"/polkadot-metanodectl "$METANODE_SRC"/public-addr \
    "$METANODE_SRC"/tests/*.sh; do
    if sed -e 's/^[[:space:]]*#.*//' -e ':a' -e '/|[[:space:]]*$/{N;s/|[[:space:]]*\n[[:space:]]*/| /;ba}' "$file" |
      grep -nE '\|[[:space:]]*grep[[:space:]]+-[a-zA-Z]*q' >/dev/null; then
      log "  $file pipes into grep -q"
      hits=1
    fi
  done
  [ "$hits" = 0 ] || fail "pipes into grep -q (see above)"
  # The same unit files serve install.sh (/usr/local) and a package (/usr): whatever installs them
  # fills in @BINDIR@, @WORKERSDIR@ and @LIBDIR@, and neither the units nor the tools name a layout.
  log "lint: units and tools are layout-independent"
  for file in "$METANODE_SRC"/systemd/polkadot-{validator,asset-hub-collator,people-collator}.service; do
    grep -qE '^ExecStart=@BINDIR@/polkadot(-parachain)? --(validator|collator) \\$' "$file" ||
      fail "$file: ExecStart= must run @BINDIR@/polkadot --validator or @BINDIR@/polkadot-parachain --collator"
    grep -qE '^ExecStartPre=-\+@LIBDIR@/public-addr [0-9]+$' "$file" ||
      fail "$file: ExecStartPre=-+@LIBDIR@/public-addr <p2p port> is missing"
  done
  grep -qE '^ExecStart=@BINDIR@/polkadot-metanodectl qos start$' "$METANODE_SRC/systemd/polkadot-metanode-qos.service" ||
    fail "polkadot-metanode-qos.service: ExecStart= must be @BINDIR@/polkadot-metanodectl qos start"
  if grep -rn /usr/local "$METANODE_SRC/systemd" "$METANODE_SRC/polkadot-metanodectl" "$METANODE_SRC/public-addr"; then
    fail "the units or the tools name /usr/local (see above)"
  fi
  # doctor, status and rotate-keys take the ports from the launcher's role table, and doctor checks
  # the variables of metanode.env against the launcher's list: both must match the units.
  log "lint: the units, polkadot-metanodectl and metanode.env agree"
  local role binary p2p relay_p2p rpc unsafe relay_rpc prometheus relay_prometheus chain unit command want
  while read -r role binary p2p relay_p2p rpc unsafe relay_rpc prometheus relay_prometheus chain; do
    unit=$METANODE_SRC/systemd/polkadot-$role.service
    command=$(sed -n '/^ExecStart=/,/[^\\]$/p' "$unit")
    local wants=("@BINDIR@/$binary " "--chain=\${$chain} " "--port=$p2p " ":$rpc,methods=safe,"
      "127.0.0.1:$unsafe,methods=unsafe," "--prometheus-port=$prometheus ")
    [ "$role" = validator ] || wants+=("--port=$relay_p2p " "--prometheus-port=$relay_prometheus "
      "--rpc-port=$relay_rpc " "--chain=\${RELAY_CHAIN} ")
    for want in "${wants[@]}"; do
      [[ "$command" == *"$want"* ]] || fail "$unit: its ExecStart lacks '$want', from the role table"
    done
    grep -q "public-addr $p2p\$" "$unit" || fail "$unit: public-addr is not given the p2p port $p2p"
  done < <(sed -n "/^ROLE_TABLE='/,/^'/p" "$METANODE_SRC/polkadot-metanodectl" | sed '1d;$d')
  local used known documented
  # shellcheck disable=SC2016 # the variables are the units'
  used=$(cat "$METANODE_SRC"/systemd/polkadot-{validator,asset-hub-collator,people-collator}.service |
    grep -oE '\$\{?[A-Z_]+\}?' | tr -d '${}' | grep -vx PUBLIC_ADDRS | sort -u)
  known=$(sed -n '/^VARIABLES=(/,/)$/p' "$METANODE_SRC/polkadot-metanodectl" | sed 's/^VARIABLES=//' |
    tr -d '()' | tr -s ' \n' '\n' | sed '/^$/d' | sort)
  documented=$(grep -oE '^#?[A-Z_]+=' "$METANODE_SRC/config/metanode.env" | tr -d '#=' | sort -u)
  [ -z "$(comm -23 <(echo "$used") <(echo "$known"))" ] ||
    fail "the units use variables missing from VARIABLES in polkadot-metanodectl: $(comm -23 <(echo "$used") <(echo "$known") | tr '\n' ' ')"
  [ "$known" = "$documented" ] ||
    fail "VARIABLES in polkadot-metanodectl and config/metanode.env differ: $(diff <(echo "$known") <(echo "$documented") | grep '^[<>]' | tr '\n' ' ')"
  if grep -vE '^(#.*|[A-Z_]+=.*|)$' "$METANODE_SRC/config/metanode.env"; then
    fail "config/metanode.env has lines that are neither comments nor KEY=value (see above)"
  fi
  local scripts=(install.sh polkadot-metanodectl public-addr tests/run.sh tests/lib.sh tests/units.sh
    tests/stub-node tests/lifecycle.sh)
  [ ! -e "$METANODE_SRC/tests/e2e.sh" ] || scripts+=(tests/e2e.sh tests/gen-local-specs.sh)
  log "lint: shellcheck ${scripts[*]}"
  if command -v shellcheck >/dev/null; then
    (cd "$METANODE_SRC" && shellcheck -x "${scripts[@]}") || fail "shellcheck"
  else
    docker run --rm -v "$METANODE_SRC:/mnt:ro" -w /mnt koalaman/shellcheck:stable -x "${scripts[@]}" ||
      fail "shellcheck"
  fi
}

# Resolved only for the tiers that run nodes, so lint works without a build.
require_binaries() {
  local bin
  BIN_DIR=$(cd "${BIN_DIR:-$METANODE_SRC/../../target/release}" 2>/dev/null && pwd) ||
    die "${BIN_DIR:-target/release} not found (build the binaries or pass --bin-dir)"
  export BIN_DIR
  for bin in polkadot polkadot-prepare-worker polkadot-execute-worker polkadot-parachain; do
    [ -x "$BIN_DIR/$bin" ] || die "$BIN_DIR/$bin not found (build it or pass --bin-dir)"
  done
  log "binaries: $("$BIN_DIR/polkadot" --version) from $BIN_DIR"
}

trap cleanup_containers EXIT
host_facts
case "$tier" in lint | all) run_lint ;; esac
case "$tier" in
  units | all)
    for distro in "${distros[@]}"; do
      log "=== units on $distro ==="
      "$TESTS_DIR/units.sh" "$distro" || fail "units on $distro"
      [ "$KEEP" = 1 ] || cleanup_containers
    done
    ;;
esac
case "$tier" in
  lifecycle | all)
    require_binaries
    for distro in "${distros[@]}"; do
      log "=== lifecycle on $distro ==="
      "$TESTS_DIR/lifecycle.sh" "$distro" || fail "lifecycle on $distro"
      [ "$KEEP" = 1 ] || cleanup_containers
    done
    ;;
esac
case "$tier" in
  e2e | all)
    require_binaries
    log "=== e2e ==="
    "$TESTS_DIR/e2e.sh" || fail "e2e"
    ;;
esac

if [ "$FAILURES" = 0 ]; then
  log "all passed"
else
  log "$FAILURES failed"
  exit 1
fi
