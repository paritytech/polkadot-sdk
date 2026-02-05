#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")"

exec nix-shell -p nodejs --run ./build.sh
