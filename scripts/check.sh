#!/usr/bin/env bash
set -euo pipefail

RED='\033[0;31m'
GREEN='\033[0;32m'
BOLD='\033[1m'
RESET='\033[0m'

pass() { echo -e "${GREEN}${BOLD}PASS${RESET} $1"; }
fail() { echo -e "${RED}${BOLD}FAIL${RESET} $1"; FAILED=1; }

FAILED=0

echo -e "${BOLD}Running checks...${RESET}\n"

# cargo fmt (nightly)
echo -e "${BOLD}[1/3] cargo +nightly fmt --check${RESET}"
if cargo +nightly fmt --check 2>/dev/null; then
	pass "cargo fmt"
else
	fail "cargo fmt — run 'cargo +nightly fmt' to fix"
fi

echo

# taplo
echo -e "${BOLD}[2/3] taplo fmt --check${RESET}"
if command -v taplo &>/dev/null; then
	if taplo fmt --check --config .config/taplo.toml 2>/dev/null; then
		pass "taplo"
	else
		fail "taplo — run 'taplo fmt --config .config/taplo.toml' to fix"
	fi
else
	fail "taplo — not installed, run 'cargo install taplo-cli --locked'"
fi

echo

# zepter
echo -e "${BOLD}[3/3] zepter run check${RESET}"
if command -v zepter &>/dev/null; then
	if zepter run check 2>/dev/null; then
		pass "zepter"
	else
		fail "zepter — run 'zepter run default' to fix"
	fi
else
	fail "zepter — not installed, run 'cargo install zepter --locked'"
fi

echo
if [ "$FAILED" -ne 0 ]; then
	echo -e "${RED}${BOLD}Some checks failed.${RESET}"
	exit 1
else
	echo -e "${GREEN}${BOLD}All checks passed.${RESET}"
fi
