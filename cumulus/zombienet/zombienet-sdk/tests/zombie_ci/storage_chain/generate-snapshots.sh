#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
TARGET_DIR="$(dirname "$(cargo locate-project --workspace --message-format plain)")/target/release"
SNAPSHOT_DIR="${ZOMBIENET_SDK_BASE_DIR:-/tmp/zombienet-storage-chain}"
DB_OUTPUT_DIR="${DB_OUTPUT_DIR:-$SCRIPT_DIR/fixtures/test-databases}"

usage() {
	cat <<EOF
Usage: $0 <phase>

Phases:
  build                  Build required binaries
  snapshots-run          Run the Rust snapshot generator
  snapshots-archive      Archive generated databases into fixture tarballs
  snapshots-test-local   Run the tip-sync test against local fixture tarballs
  all                    Run all phases

Environment:
  ZOMBIENET_SDK_BASE_DIR  Zombienet base dir (default: /tmp/zombienet-storage-chain)
  DB_OUTPUT_DIR           Fixture output dir (default: ./fixtures/test-databases)
EOF
	exit 1
}

build_binaries() {
	cargo build --release -p polkadot --bin polkadot
	cargo build --release -p polkadot-omni-node --bin polkadot-omni-node
}

snapshots_run() {
	mkdir -p "$SNAPSHOT_DIR" "$DB_OUTPUT_DIR"
	export PATH="$TARGET_DIR:$PATH"
	export RUST_LOG=info,zombienet_orchestrator=debug
	export ZOMBIE_PROVIDER=native
	export ZOMBIENET_SDK_BASE_DIR="$SNAPSHOT_DIR"
	export DB_OUTPUT_DIR

	cargo test --release \
		-p cumulus-zombienet-sdk-tests \
		--features zombie-ci,generate-snapshots \
		-- storage_chain::parachain_generate_db::parachain_generate_databases
}

archive_node() {
	local node_dir="$1"
	local output="$2"
	shift 2

	[[ -d "$node_dir/data" ]] || { echo "missing data dir: $node_dir/data" >&2; exit 1; }
	tar \
		--exclude='*/keystore' \
		--exclude='*/network' \
		-czf "$output" \
		-C "$node_dir" \
		"$@"
}

snapshots_archive() {
	mkdir -p "$DB_OUTPUT_DIR"
	archive_node "$SNAPSHOT_DIR/pruned-node" "$DB_OUTPUT_DIR/tip-sync-100.tgz" data relay-data
	archive_node "$SNAPSHOT_DIR/alice" "$DB_OUTPUT_DIR/relay.tgz" data

	[[ -f "$DB_OUTPUT_DIR/raw-chain-spec.json" ]] || { echo "missing raw-chain-spec.json" >&2; exit 1; }
	[[ -f "$DB_OUTPUT_DIR/raw-relay-chain-spec.json" ]] || { echo "missing raw-relay-chain-spec.json" >&2; exit 1; }
	[[ -f "$DB_OUTPUT_DIR/tip-sync-100-metadata.json" ]] || { echo "missing tip-sync-100-metadata.json" >&2; exit 1; }

	echo "Created storage-chain fixtures in $DB_OUTPUT_DIR"
}

snapshots_test_local() {
	export PATH="$TARGET_DIR:$PATH"
	export RUST_LOG=info,zombienet_orchestrator=debug
	export ZOMBIE_PROVIDER=native
	export STORAGE_CHAIN_TIP_SYNC_SNAPSHOT="$DB_OUTPUT_DIR/tip-sync-100.tgz"
	export STORAGE_CHAIN_RELAY_SNAPSHOT="$DB_OUTPUT_DIR/relay.tgz"
	export STORAGE_CHAIN_RAW_CHAIN_SPEC="$DB_OUTPUT_DIR/raw-chain-spec.json"
	export STORAGE_CHAIN_RAW_RELAY_CHAIN_SPEC="$DB_OUTPUT_DIR/raw-relay-chain-spec.json"
	export STORAGE_CHAIN_TIP_SYNC_METADATA="$DB_OUTPUT_DIR/tip-sync-100-metadata.json"

	cargo test --release \
		-p cumulus-zombienet-sdk-tests \
		--features zombie-ci \
		-- storage_chain::parachain_tip_sync_with_renewals::parachain_tip_sync_with_renewals_test
}

all() {
	build_binaries
	snapshots_run
	snapshots_archive
	snapshots_test_local
}

[[ $# -eq 0 ]] && usage

case "$1" in
	build) build_binaries ;;
	snapshots-run) snapshots_run ;;
	snapshots-archive) snapshots_archive ;;
	snapshots-test-local) snapshots_test_local ;;
	all) all ;;
	*) echo "Unknown phase: $1" >&2; usage ;;
esac
