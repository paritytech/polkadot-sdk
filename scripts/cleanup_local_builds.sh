#!/usr/bin/env bash
set -euo pipefail

# Conservative cleanup for local dev:
# - Keeps the release binaries you need for zombienet-ttt (polkadot, polkadot-parachain)
# - Optionally keeps test-parachain if present (--keep-test)
# - Removes heavy/rebuildable artifacts (debug builds, release deps, wbuild, incremental, dSYM)
# - Optionally strips binaries to reduce size (--strip)
# - Prunes old Zombienet temp dirs older than 1 day (--prune-zombie)
#
# Usage:
#   scripts/cleanup_local_builds.sh [--keep-test] [--strip] [--prune-zombie] [--dry-run]

KEEP_TEST=false
DO_STRIP=false
PRUNE_ZOMBIE=false
DRY_RUN=false

for arg in "$@"; do
  case "$arg" in
    --keep-test) KEEP_TEST=true ;;
    --strip) DO_STRIP=true ;;
    --prune-zombie) PRUNE_ZOMBIE=true ;;
    --dry-run) DRY_RUN=true ;;
    *) echo "Unknown flag: $arg" >&2; exit 2;;
  esac
done

ROOT_DIR=$(cd "$(dirname "$0")/.." && pwd)
cd "$ROOT_DIR"

announce() { echo "[cleanup] $*"; }
run() { if $DRY_RUN; then echo "DRY: $*"; else eval "$*"; fi }

ensure_dir() { run "mkdir -p \"$1\""; }

KEEP_DIR=$(mktemp -d 2>/dev/null || mktemp -d -t keepbins)

BIN_DIR="target/release"
NEEDED=("polkadot" "polkadot-parachain")
if $KEEP_TEST; then NEEDED+=("test-parachain"); fi

announce "Backing up required binaries to $KEEP_DIR"
for bin in "${NEEDED[@]}"; do
  if [ -x "$BIN_DIR/$bin" ]; then
    run "cp -p \"$BIN_DIR/$bin\" \"$KEEP_DIR/$bin\""
  else
    announce "Binary not found (skipping): $BIN_DIR/$bin"
  fi
done

announce "Removing heavy build artifacts"
run "rm -rf target/debug"
run "rm -rf target/release"

# Remove cargo incremental/caches that can grow large
run "find target -type d -name incremental -prune -exec rm -rf {} + 2>/dev/null || true"
run "find target -type d -name wbuild -prune -exec rm -rf {} + 2>/dev/null || true"
run "find target -type d -name build -prune -exec rm -rf {} + 2>/dev/null || true"
run "find target -type d -name .fingerprint -prune -exec rm -rf {} + 2>/dev/null || true"

announce "Restoring required binaries"
ensure_dir "$BIN_DIR"
for bin in "${NEEDED[@]}"; do
  if [ -f "$KEEP_DIR/$bin" ]; then
    run "cp -p \"$KEEP_DIR/$bin\" \"$BIN_DIR/$bin\""
    run "chmod +x \"$BIN_DIR/$bin\""
  fi
done

if $DO_STRIP; then
  if command -v strip >/dev/null 2>&1; then
    announce "Stripping binaries to reduce size"
    for bin in "${NEEDED[@]}"; do
      if [ -x "$BIN_DIR/$bin" ]; then
        # macOS prefers -x; on Linux plain strip is fine
        run "strip -x \"$BIN_DIR/$bin\" 2>/dev/null || strip \"$BIN_DIR/$bin\" 2>/dev/null || true"
      fi
    done
  else
    announce "strip not found; skipping binary stripping"
  fi
fi

if $PRUNE_ZOMBIE; then
  announce "Pruning Zombienet temp runs older than 1 day"
  run "find /private/var/folders -type d -name 'zombie-*' -mtime +1 -prune -exec rm -rf {} + 2>/dev/null || true"
fi

announce "Done. Kept binaries in $BIN_DIR:"
for bin in "${NEEDED[@]}"; do
  [ -x "$BIN_DIR/$bin" ] && ls -lh "$BIN_DIR/$bin" || true
done

announce "Tip: re-run with --strip and --prune-zombie for extra space savings."

