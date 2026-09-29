#!/usr/bin/env bash
# Docker test harness for the metanode systemd units.
#
#   tests/run.sh [lint|lifecycle|e2e|all] [--distro D]... [--bin-dir DIR] [--keep]
#
#   lint       golden tests of polkadot-metanodectl and shellcheck (host only, seconds)
#   lifecycle  per distro: install, systemd-analyze verify/security and the lifecycle checks
#   e2e        a four-machine local network producing relay, Asset Hub and People blocks
#
# --distro defaults to every supported distro (ubuntu-22.04 ubuntu-24.04 debian-12 debian-13).
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
    lint | lifecycle | e2e | all) tier=$1 ;;
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
  log "lint: golden tests"
  "$TESTS_DIR/golden.sh" || fail "golden tests"
  # Names from before the units became polkadot-<role> (built from pieces so this line does not
  # match itself).
  local old=metanode stale
  stale="$old-exec|$old-(validator|asset-hub|people)|(^|[^-])$old\.(target|slice)|/etc/$old|/var/lib/$old"
  stale+="|(^|[^_])METANODE_(BIN|CONF|BASE)_DIR|mn-(life|e2e)-"
  log "lint: no pre-rename names"
  if grep -rnE "$stale" "$METANODE_SRC" "$METANODE_SRC/../../.github/workflows/tests-metanode-systemd.yml"; then
    fail "pre-rename names left (see above)"
  fi
  # `cmd | grep -q` fails under pipefail whenever grep exits before cmd is done writing (SIGPIPE),
  # so it passes or fails depending on output size and timing. Capture first, then grep. Lines
  # continued after a trailing `|` are joined; comments are ignored.
  log "lint: no grep -q at the end of a pipe"
  local file hits=0
  for file in "$METANODE_SRC"/install.sh "$METANODE_SRC"/polkadot-metanodectl "$METANODE_SRC"/tests/*.sh; do
    if sed -e 's/^[[:space:]]*#.*//' -e ':a' -e '/|[[:space:]]*$/{N;s/|[[:space:]]*\n[[:space:]]*/| /;ba}' "$file" |
      grep -nE '\|[[:space:]]*grep[[:space:]]+-[a-zA-Z]*q' >/dev/null; then
      log "  $file pipes into grep -q"
      hits=1
    fi
  done
  [ "$hits" = 0 ] || fail "pipes into grep -q (see above)"
  # The same unit files serve install.sh (/usr/local) and a package (/usr): whatever installs them
  # fills in @BINDIR@, and neither the units nor the launcher name a layout.
  log "lint: units and launcher are layout-independent"
  for file in "$METANODE_SRC"/systemd/*.service; do
    grep -qE '^ExecStart=@BINDIR@/polkadot-metanodectl run [a-z-]+$' "$file" ||
      fail "$file: ExecStart= must be @BINDIR@/polkadot-metanodectl run <role>"
  done
  if grep -rn /usr/local "$METANODE_SRC/systemd" "$METANODE_SRC/polkadot-metanodectl"; then
    fail "the units or the launcher name /usr/local (see above)"
  fi
  # The configuration templates are sourced as bash on the operator's machine.
  log "lint: configuration templates are valid bash"
  for file in "$METANODE_SRC"/config/*.conf; do
    bash -n "$file" || fail "$file is not valid bash"
  done
  local scripts=(install.sh polkadot-metanodectl tests/run.sh tests/lib.sh tests/lifecycle.sh tests/golden.sh)
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
