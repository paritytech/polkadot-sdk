#!/bin/bash
#
# Test script for publishing crates to staging.crates.io
#
# This script allows you to test the crate publishing flow without affecting
# production crates.io. It uses environment variables to redirect cargo to
# staging.crates.io instead of modifying your local cargo config.
#
# Usage:
#   ./scripts/release/test-staging-publish.sh [options]
#
# Options:
#   --dry-run       Don't actually publish, just show what would be published
#   --token TOKEN   crates.io API token (or set STAGING_CRATESIO_TOKEN env var)
#   --crates LIST   Comma-separated list of crates to publish (default: test crates)
#   --help          Show this help message
#
# Examples:
#   # Dry run with test crates
#   ./scripts/release/test-staging-publish.sh --dry-run
#
#   # Publish test crates to staging
#   ./scripts/release/test-staging-publish.sh --token YOUR_TOKEN
#
#   # Publish specific crates
#   ./scripts/release/test-staging-publish.sh --crates "parity-staging-test-a,parity-staging-test-b"
#

set -e

# Default values
DRY_RUN=false
TOKEN="${STAGING_CRATESIO_TOKEN:-}"
CRATES="parity-staging-test-a,parity-staging-test-b,parity-staging-test-c"
PARITY_PUBLISH_PATH="${PARITY_PUBLISH_PATH:-../parity-publish/target/release/parity-publish}"

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

print_help() {
    sed -n '2,/^$/p' "$0" | sed 's/^# //' | sed 's/^#//'
}

log_info() {
    echo -e "${GREEN}[INFO]${NC} $1"
}

log_warn() {
    echo -e "${YELLOW}[WARN]${NC} $1"
}

log_error() {
    echo -e "${RED}[ERROR]${NC} $1"
}

# Parse arguments
while [[ $# -gt 0 ]]; do
    case $1 in
        --dry-run)
            DRY_RUN=true
            shift
            ;;
        --token)
            TOKEN="$2"
            shift 2
            ;;
        --crates)
            CRATES="$2"
            shift 2
            ;;
        --parity-publish)
            PARITY_PUBLISH_PATH="$2"
            shift 2
            ;;
        --help|-h)
            print_help
            exit 0
            ;;
        *)
            log_error "Unknown option: $1"
            print_help
            exit 1
            ;;
    esac
done

# Check parity-publish exists
if [[ ! -x "$PARITY_PUBLISH_PATH" ]]; then
    log_error "parity-publish not found at: $PARITY_PUBLISH_PATH"
    log_info "Set PARITY_PUBLISH_PATH environment variable or use --parity-publish flag"
    log_info "Example: export PARITY_PUBLISH_PATH=/path/to/parity-publish"
    exit 1
fi

log_info "Using parity-publish at: $PARITY_PUBLISH_PATH"

# Check token
if [[ -z "$TOKEN" ]] && [[ "$DRY_RUN" == "false" ]]; then
    log_error "No token provided. Use --token or set STAGING_CRATESIO_TOKEN env var"
    log_info "For dry run, use --dry-run flag"
    exit 1
fi

# Set environment variables for staging.crates.io
# This redirects cargo to use staging without modifying your config files
export CARGO_REGISTRIES_CRATES_IO_INDEX="sparse+https://index.staging.crates.io/"
export CARGO_REGISTRY_TOKEN="$TOKEN"

log_info "Configured to publish to: staging.crates.io"
log_info "Crates to publish: $CRATES"
log_info "Dry run: $DRY_RUN"
echo ""

# Convert comma-separated crates to array
IFS=',' read -ra CRATE_ARRAY <<< "$CRATES"

if [[ "$DRY_RUN" == "true" ]]; then
    log_info "=== DRY RUN MODE ==="
    log_info "Would publish the following crates to staging.crates.io:"
    echo ""
    for crate in "${CRATE_ARRAY[@]}"; do
        echo "  - $crate"
    done
    echo ""
    log_info "To actually publish, run without --dry-run flag"
else
    log_info "=== PUBLISHING TO STAGING.CRATES.IO ==="
    echo ""

    # Publish each crate in order (respecting dependencies)
    for crate in "${CRATE_ARRAY[@]}"; do
        log_info "Publishing $crate..."

        # Use cargo publish directly for individual crates
        # Use --index to publish directly to staging (not --registry which still uploads to prod)
        cargo publish \
            -p "$crate" \
            --index "sparse+https://index.staging.crates.io/" \
            --token "$TOKEN" \
            --allow-dirty \
            2>&1 || {
                log_error "Failed to publish $crate"
                exit 1
            }

        log_info "Successfully published $crate"

        # Wait a bit between publishes to avoid rate limiting
        log_info "Waiting 30 seconds before next publish..."
        sleep 30
    done

    echo ""
    log_info "=== ALL CRATES PUBLISHED SUCCESSFULLY ==="
    log_info "Check them at: https://staging.crates.io"
fi
