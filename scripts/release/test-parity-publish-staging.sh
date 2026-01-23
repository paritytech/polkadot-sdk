#!/bin/bash
#
# Test parity-publish workflow against staging.crates.io
#
# This script tests the full parity-publish workflow:
#   1. parity-publish plan --prdoc prdoc
#   2. parity-publish apply
#   3. parity-publish apply -p (publish)
#
# Uses environment variables to redirect to staging.crates.io without
# modifying your local cargo config.
#
# Usage:
#   ./scripts/release/test-parity-publish-staging.sh [options]
#
# Options:
#   --dry-run              Don't actually publish
#   --token TOKEN          crates.io API token (or set STAGING_CRATESIO_TOKEN env var)
#   --parity-publish PATH  Path to parity-publish binary
#   --step STEP            Run only specific step: plan, apply, publish, or all (default: all)
#   --help                 Show this help message
#
# Examples:
#   # Full dry run
#   ./scripts/release/test-parity-publish-staging.sh --dry-run
#
#   # Run only the plan step
#   ./scripts/release/test-parity-publish-staging.sh --step plan
#
#   # Full publish to staging
#   ./scripts/release/test-parity-publish-staging.sh --token YOUR_TOKEN
#

set -e

# Default values
DRY_RUN=false
TOKEN="${STAGING_CRATESIO_TOKEN:-}"
PARITY_PUBLISH_PATH="${PARITY_PUBLISH_PATH:-../parity-publish/target/release/parity-publish}"
STEP="all"

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

print_help() {
    sed -n '2,/^$/p' "$0" | sed 's/^# //' | sed 's/^#//'
}

log_info() {
    echo -e "${GREEN}[INFO]${NC} $1"
}

log_step() {
    echo -e "${BLUE}[STEP]${NC} $1"
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
        --parity-publish)
            PARITY_PUBLISH_PATH="$2"
            shift 2
            ;;
        --step)
            STEP="$2"
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

# Validate step
if [[ ! "$STEP" =~ ^(plan|apply|publish|all)$ ]]; then
    log_error "Invalid step: $STEP. Must be: plan, apply, publish, or all"
    exit 1
fi

# Check parity-publish exists
if [[ ! -x "$PARITY_PUBLISH_PATH" ]]; then
    log_error "parity-publish not found at: $PARITY_PUBLISH_PATH"
    log_info "Set PARITY_PUBLISH_PATH environment variable or use --parity-publish flag"
    log_info "Example: export PARITY_PUBLISH_PATH=../parity-publish/target/release/parity-publish"
    exit 1
fi

log_info "Using parity-publish at: $PARITY_PUBLISH_PATH"
log_info "parity-publish version:"
"$PARITY_PUBLISH_PATH" --version || true
echo ""

# Check token for publish step
if [[ "$STEP" == "publish" || "$STEP" == "all" ]] && [[ -z "$TOKEN" ]] && [[ "$DRY_RUN" == "false" ]]; then
    log_error "No token provided for publish step. Use --token or set STAGING_CRATESIO_TOKEN env var"
    log_info "For dry run, use --dry-run flag"
    exit 1
fi

# Set environment variables for staging.crates.io
export CARGO_REGISTRIES_CRATES_IO_INDEX="sparse+https://index.staging.crates.io/"
export PARITY_PUBLISH_CRATESIO_TOKEN="$TOKEN"

log_info "============================================"
log_info "  Testing parity-publish with staging.crates.io"
log_info "============================================"
log_info "Registry: staging.crates.io"
log_info "Step: $STEP"
log_info "Dry run: $DRY_RUN"
echo ""

# Step 1: Plan
if [[ "$STEP" == "plan" || "$STEP" == "all" ]]; then
    log_step "1/3 - Running parity-publish plan..."
    echo ""

    "$PARITY_PUBLISH_PATH" plan --prdoc prdoc

    log_info "Plan completed. Check Plan.toml for the publish plan."
    echo ""

    if [[ "$STEP" == "plan" ]]; then
        log_info "Plan step completed. Run with --step apply to continue."
        exit 0
    fi
fi

# Step 2: Apply
if [[ "$STEP" == "apply" || "$STEP" == "all" ]]; then
    log_step "2/3 - Running parity-publish apply..."
    echo ""

    "$PARITY_PUBLISH_PATH" apply

    log_info "Apply completed. Version bumps have been applied to Cargo.toml files."
    echo ""

    # Update Cargo.lock
    log_info "Updating Cargo.lock..."
    cargo update --workspace --offline || cargo update --workspace
    log_info "Cargo.lock updated."
    echo ""

    if [[ "$STEP" == "apply" ]]; then
        log_info "Apply step completed. Run with --step publish to publish."
        exit 0
    fi
fi

# Step 3: Publish
if [[ "$STEP" == "publish" || "$STEP" == "all" ]]; then
    log_step "3/3 - Publishing crates..."
    echo ""

    if [[ "$DRY_RUN" == "true" ]]; then
        log_warn "DRY RUN - Not actually publishing"
        log_info "Would run: $PARITY_PUBLISH_PATH apply -p --batch-delay 15 --max-concurrent 1 --batch-size 1"
        echo ""
        log_info "Crates that would be published:"
        "$PARITY_PUBLISH_PATH" apply --print || true
    else
        log_info "Publishing to staging.crates.io..."
        "$PARITY_PUBLISH_PATH" apply -p --batch-delay 15 --max-concurrent 1 --batch-size 1

        log_info "Publish completed!"
    fi
    echo ""
fi

log_info "============================================"
log_info "  All steps completed!"
log_info "============================================"

if [[ "$DRY_RUN" == "false" ]] && [[ "$STEP" == "publish" || "$STEP" == "all" ]]; then
    log_info "Check your crates at: https://staging.crates.io"
fi
