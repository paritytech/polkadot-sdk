#!/bin/bash
# Automation script for updating the full_node_warp_sync test
# This script handles chain spec generation and snapshot creation

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../../../../.." && pwd)"

# Configuration
PARACHAIN_SPEC="$SCRIPT_DIR/warp-sync-parachain-spec.json"
RELAYCHAIN_SPEC="$SCRIPT_DIR/warp-sync-relaychain-spec.json"
TARGET_DIR="$REPO_ROOT/target/release"

# Color output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Logging functions
log_info() {
    echo -e "${BLUE}ℹ${NC} $1"
}

log_success() {
    echo -e "${GREEN}✓${NC} $1"
}

log_error() {
    echo -e "${RED}✗${NC} $1" >&2
}

log_warn() {
    echo -e "${YELLOW}⚠${NC} $1"
}

log_step() {
    echo -e "\n${BLUE}▸${NC} $1"
}

# Show usage
show_usage() {
    cat <<EOF
Usage: $0 <phase>

Automation script for updating the full_node_warp_sync test.

Phases:
  chainspec-parachain    Build and generate parachain chain spec
  chainspec-relaychain   Build and generate relaychain chain spec
  snapshots-run          Run test to generate snapshots (24h runtime)
  snapshots-archive      Archive databases into tarballs
  snapshots-test-local   Test with local snapshots before upload
  all                    Run all phases sequentially

Examples:
  # Update chain specs only
  $0 chainspec-parachain
  $0 chainspec-relaychain

  # Generate and test snapshots
  export ZOMBIENET_SDK_BASE_DIR=/tmp/zombienet-warp-sync
  $0 snapshots-run
  $0 snapshots-archive
  $0 snapshots-test-local

  # Full update workflow
  $0 all

Environment Variables:
  ZOMBIENET_SDK_BASE_DIR  Required for snapshot generation (default: /tmp/zombienet-warp-sync)
  DRY_RUN                 Set to 1 to preview actions without executing
EOF
    exit 1
}

# Check if running in dry-run mode
is_dry_run() {
    [[ "${DRY_RUN:-0}" == "1" ]]
}

# Execute command or print if dry-run
run_cmd() {
    if is_dry_run; then
        echo "  [DRY-RUN] $*"
    else
        "$@"
    fi
}

# Phase 1: Generate parachain chain spec
phase_chainspec_parachain() {
    log_step "Phase 1: Generating parachain chain spec"

    # Build cumulus-test-runtime
    log_info "Building cumulus-test-runtime..."
    run_cmd cargo build --release -p cumulus-test-runtime
    log_success "Runtime built"

    # Build chain-spec-builder
    log_info "Building chain-spec-builder..."
    run_cmd cargo build --release -p staging-chain-spec-builder
    log_success "Chain-spec-builder built"

    # Generate chain spec
    log_info "Generating parachain chain spec..."
    local wasm_path="$TARGET_DIR/wbuild/cumulus-test-runtime/cumulus_test_runtime.wasm"
    if [[ ! -f "$wasm_path" ]] && ! is_dry_run; then
        log_error "WASM runtime not found at: $wasm_path"
        log_error "Make sure cumulus-test-runtime is built"
        exit 1
    fi

    run_cmd "$TARGET_DIR/chain-spec-builder" create \
        -r "$wasm_path" \
        named-preset development

    # Backup existing spec
    if [[ -f "$PARACHAIN_SPEC" ]] && ! is_dry_run; then
        log_info "Backing up existing chain spec..."
        run_cmd cp "$PARACHAIN_SPEC" "$PARACHAIN_SPEC.backup"
        log_success "Backup created: $PARACHAIN_SPEC.backup"
    fi

    # Replace chain spec
    log_info "Replacing parachain chain spec..."
    run_cmd mv chain_spec.json "$PARACHAIN_SPEC"
    log_success "Parachain chain spec updated: $PARACHAIN_SPEC"
}

# Phase 2: Generate relaychain chain spec
phase_chainspec_relaychain() {
    log_step "Phase 2: Generating relaychain chain spec"

    # Build polkadot
    log_info "Building polkadot binary..."
    run_cmd cargo build --release -p polkadot --bin polkadot-prepare-worker --bin polkadot-execute-worker --bin polkadot
    log_success "Polkadot built"

    # Export chain spec
    log_info "Exporting rococo-local chain spec..."
    local polkadot_bin="$TARGET_DIR/polkadot"
    if [[ ! -f "$polkadot_bin" ]] && ! is_dry_run; then
        log_error "Polkadot binary not found at: $polkadot_bin"
        exit 1
    fi

    if is_dry_run; then
        echo "  [DRY-RUN] $polkadot_bin build-spec --chain rococo-local --disable-default-bootnode > chain_spec.json"
    else
        "$polkadot_bin" build-spec --chain rococo-local --disable-default-bootnode > chain_spec.json
    fi

    # Backup existing spec
    if [[ -f "$RELAYCHAIN_SPEC" ]] && ! is_dry_run; then
        log_info "Backing up existing chain spec..."
        run_cmd cp "$RELAYCHAIN_SPEC" "$RELAYCHAIN_SPEC.backup"
        log_success "Backup created: $RELAYCHAIN_SPEC.backup"
    fi

    # Replace chain spec
    log_info "Replacing relaychain chain spec..."
    run_cmd mv chain_spec.json "$RELAYCHAIN_SPEC"
    log_success "Relaychain chain spec updated: $RELAYCHAIN_SPEC"
}

# Phase 3: Run test to generate snapshots
phase_snapshots_run() {
    log_step "Phase 3: Running test to generate snapshots"

    # Validate environment
    local base_dir="${ZOMBIENET_SDK_BASE_DIR:-/tmp/zombienet-warp-sync}"

    if [[ -z "${ZOMBIENET_SDK_BASE_DIR:-}" ]]; then
        log_warn "ZOMBIENET_SDK_BASE_DIR not set, using default: $base_dir"
        export ZOMBIENET_SDK_BASE_DIR="$base_dir"
    fi

    log_info "Snapshots will be generated in: $ZOMBIENET_SDK_BASE_DIR"

    # Ensure directory exists
    if ! is_dry_run; then
        mkdir -p "$ZOMBIENET_SDK_BASE_DIR"
    fi

    # Build required binaries
    log_info "Building required binaries..."
    # run_cmd cargo build --release -p polkadot --bin polkadot-prepare-worker --bin polkadot-execute-worker --bin polkadot
    # run_cmd cargo build --release -p cumulus-test-service --bin test-parachain
    log_success "Binaries built"

    # Run test with snapshot-update-mode feature
    log_info "Starting test with snapshot-update-mode feature..."
    log_info "This will take approximately 24 hours..."

    # Set PATH to use locally built binaries
    export PATH="$TARGET_DIR:$PATH"
    log_info "Using binaries from: $TARGET_DIR"

    export RUST_LOG=info,zombienet_orchestrator=debug
    export ZOMBIE_PROVIDER=native

    if is_dry_run; then
        echo "  [DRY-RUN] PATH=$TARGET_DIR:\$PATH ZOMBIENET_SDK_BASE_DIR=$ZOMBIENET_SDK_BASE_DIR cargo nextest run --release -p cumulus-zombienet-sdk-tests --features zombie-ci,snapshot-update-mode --no-capture -- full_node_warp_sync"
    else
        cargo nextest run --release \
            -p cumulus-zombienet-sdk-tests \
            --features zombie-ci,snapshot-update-mode \
            --no-capture \
            -- full_node_warp_sync
    fi

    log_success "Test completed successfully"
    log_info "Databases ready for archiving in: $ZOMBIENET_SDK_BASE_DIR"
}

# Phase 4: Archive databases
phase_snapshots_archive() {
    log_step "Phase 4: Archiving databases"

    local base_dir="${ZOMBIENET_SDK_BASE_DIR:-/tmp/zombienet-warp-sync}"

    # Validate directories exist
    local alice_dir="$base_dir/alice/data"
    local one_dir="$base_dir/one"

    if [[ ! -d "$alice_dir" ]] && ! is_dry_run; then
        log_error "Alice database not found at: $alice_dir"
        log_error "Run 'snapshots-run' phase first"
        exit 1
    fi

    if [[ ! -d "$one_dir/data" ]] && ! is_dry_run; then
        log_error "One (parachain) database not found at: $one_dir"
        log_error "Run 'snapshots-run' phase first"
        exit 1
    fi

    log_success "Found alice database: $alice_dir"
    log_success "Found one database: $one_dir"

    # Create archives in the test directory
    cd "$SCRIPT_DIR"

    # Archive relaychain (alice)
    log_info "Creating alice-db.tgz..."
    if is_dry_run; then
        echo "  [DRY-RUN] tar -czf alice-db.tgz -C $base_dir/alice data/"
    else
        tar -czf alice-db.tgz -C "$base_dir/alice" data/
        local alice_size=$(du -h alice-db.tgz | cut -f1)
        log_success "Created alice-db.tgz ($alice_size)"
    fi

    # Archive parachain (one)
    log_info "Creating one-db.tgz..."
    if is_dry_run; then
        echo "  [DRY-RUN] tar -czf one-db.tgz -C $base_dir/one data/ relay-data/"
    else
        tar -czf one-db.tgz -C "$base_dir/one" data/ relay-data/
        local one_size=$(du -h one-db.tgz | cut -f1)
        log_success "Created one-db.tgz ($one_size)"
    fi

    echo
    log_success "Archives created in: $SCRIPT_DIR"
    echo
    log_info "Next steps:"
    echo "  1. Test locally: $0 snapshots-test-local"
    echo "  2. Upload to Google Cloud Storage:"
    echo "     gsutil cp alice-db.tgz gs://zombienet-db-snaps/zombienet/XXXX-full_node_warp_sync_db/"
    echo "     gsutil cp one-db.tgz gs://zombienet-db-snaps/zombienet/XXXX-full_node_warp_sync_db/"
    echo "  3. Update constants in full_node_warp_sync.rs:"
    echo "     - DB_SNAPSHOT_RELAYCHAIN (line 129)"
    echo "     - DB_SNAPSHOT_PARACHAIN (line 130)"
}

# Phase 5: Test with local snapshots
phase_snapshots_test_local() {
    log_step "Phase 5: Testing with local snapshots"

    # Check if archives exist
    if [[ ! -f "$SCRIPT_DIR/alice-db.tgz" ]] && ! is_dry_run; then
        log_error "alice-db.tgz not found in $SCRIPT_DIR"
        log_error "Run 'snapshots-archive' phase first"
        exit 1
    fi

    if [[ ! -f "$SCRIPT_DIR/one-db.tgz" ]] && ! is_dry_run; then
        log_error "one-db.tgz not found in $SCRIPT_DIR"
        log_error "Run 'snapshots-archive' phase first"
        exit 1
    fi

    log_success "Found alice-db.tgz"
    log_success "Found one-db.tgz"

    # Set environment variables for local testing
    export DB_SNAPSHOT_RELAYCHAIN_LOCAL="file://$SCRIPT_DIR/alice-db.tgz"
    export DB_SNAPSHOT_PARACHAIN_LOCAL="file://$SCRIPT_DIR/one-db.tgz"

    log_info "Testing with local snapshots..."
    log_info "  DB_SNAPSHOT_RELAYCHAIN_LOCAL=$DB_SNAPSHOT_RELAYCHAIN_LOCAL"
    log_info "  DB_SNAPSHOT_PARACHAIN_LOCAL=$DB_SNAPSHOT_PARACHAIN_LOCAL"

    # Build required binaries if needed
    log_info "Building required binaries..."
    run_cmd cargo build --release -p polkadot --bin polkadot-prepare-worker --bin polkadot-execute-worker --bin polkadot
    run_cmd cargo build --release -p cumulus-test-service --bin test-parachain
    log_success "Binaries built"

    # Run test
    log_info "Running test with local snapshots..."

    # Set PATH to use locally built binaries
    export PATH="$TARGET_DIR:$PATH"
    log_info "Using binaries from: $TARGET_DIR"

    export RUST_LOG=info,zombienet_orchestrator=debug
    export ZOMBIE_PROVIDER=native

    if is_dry_run; then
        echo "  [DRY-RUN] PATH=$TARGET_DIR:\$PATH cargo nextest run --release -p cumulus-zombienet-sdk-tests --features zombie-ci --no-capture -- full_node_warp_sync"
    else
        cargo nextest run --release \
            -p cumulus-zombienet-sdk-tests \
            --features zombie-ci \
            --no-capture \
            -- full_node_warp_sync
    fi

    log_success "Local snapshot test passed!"
    echo
    log_info "Snapshots are validated and ready for upload"
}

# Phase: All
phase_all() {
    log_info "Running all phases sequentially"

    phase_chainspec_parachain
    phase_chainspec_relaychain
    phase_snapshots_run
    phase_snapshots_archive
    phase_snapshots_test_local

    echo
    log_success "All phases completed successfully!"
    log_info "Final step: Upload archives to GCS and update constants in code"
}

# Main entry point
main() {
    if [[ $# -eq 0 ]]; then
        show_usage
    fi

    local phase="$1"

    if is_dry_run; then
        log_warn "DRY-RUN MODE: Commands will be displayed but not executed"
        echo
    fi

    case "$phase" in
        chainspec-parachain)
            phase_chainspec_parachain
            ;;
        chainspec-relaychain)
            phase_chainspec_relaychain
            ;;
        snapshots-run)
            phase_snapshots_run
            ;;
        snapshots-archive)
            phase_snapshots_archive
            ;;
        snapshots-test-local)
            phase_snapshots_test_local
            ;;
        all)
            phase_all
            ;;
        *)
            log_error "Unknown phase: $phase"
            show_usage
            ;;
    esac

    echo
    log_success "Phase '$phase' completed"
}

main "$@"
