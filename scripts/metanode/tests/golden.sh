#!/usr/bin/env bash
# Golden tests for `polkadot-metanodectl print`: every case directory under golden/ holds a config
# (metanode.conf plus optional <role>.conf) and expected.txt, the output and exit status of
# `print` for each role. Needs no binaries, no docker and no root.
#
#   tests/golden.sh            compare against expected.txt
#   UPDATE=1 tests/golden.sh   rewrite expected.txt (review the diff before committing)
set -euo pipefail

here=$(cd "$(dirname "$0")" && pwd)
# The launcher takes its binaries from its own directory, and the validator's PVF workers from there
# or else from the polkadot package's /usr/lib/polkadot. Run a copy next to a stand-in worker, so the
# output cannot depend on what this machine has installed; that directory is shown as @BINDIR@.
bin_dir=$(mktemp -d)
trap 'rm -rf "$bin_dir"' EXIT
cp "$here/../polkadot-metanodectl" "$bin_dir/"
: >"$bin_dir/polkadot-prepare-worker"
exec_bin=$bin_dir/polkadot-metanodectl
failed=0

for case_dir in "$here"/golden/*/; do
  case_dir=${case_dir%/}
  name=$(basename "$case_dir")
  actual=$(
    for role in validator asset-hub people; do
      status=0
      # The config-key variables in the environment must be ignored: only the files count.
      out=$(CASE_DIR=$case_dir POLKADOT_METANODE_CONF_DIR=$case_dir STATE_DIRECTORY=/var/lib/polkadot-$role \
        BIN_DIR=/polluted NETWORK=polluted CHAIN=polluted EXTRA_ARGS=--polluted P2P_PORT=1 \
        "$exec_bin" print "$role" 2>&1) || status=$?
      echo "## $role (exit $status)"
      out=${out//$case_dir/@CASE@}
      echo "${out//$bin_dir/@BINDIR@}"
    done
  )
  if [ "${UPDATE:-}" = 1 ]; then
    printf '%s\n' "$actual" >"$case_dir/expected.txt"
    echo "updated $name"
  elif diff -u "$case_dir/expected.txt" <(printf '%s\n' "$actual"); then
    echo "ok      $name"
  else
    echo "FAILED  $name"
    failed=1
  fi
done
exit "$failed"
