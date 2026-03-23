#!/usr/bin/env bash
set -e

# Auto-fix formatting and lint issues, committing at each step.

# Bail if the working tree is dirty.
if [ -n "$(git status --porcelain)" ]; then
	echo "ERROR: working tree is dirty. Please commit or stash your changes first."
	git status --short
	exit 1
fi

commit_if_changes() {
	if [ -n "$(git diff --name-only)" ]; then
		echo "changes detected, committing..."
		git add -u
		git commit -m "$1"
		echo "commit created."
	fi
}

# Step 1: cargo fmt
echo "==> cargo +nightly fmt"
cargo +nightly fmt
commit_if_changes "cargo fmt"

# Step 2: zepter
echo "==> zepter run default"
if command -v zepter >/dev/null 2>&1; then
	zepter run default
	commit_if_changes "zepter"
else
	echo "SKIP: zepter not installed (cargo install zepter --locked)"
fi

# Step 3: taplo fmt
echo "==> taplo fmt"
if command -v taplo >/dev/null 2>&1; then
	taplo fmt --config .config/taplo.toml
	commit_if_changes "taplo fmt"
else
	echo "SKIP: taplo not installed (cargo install taplo-cli --locked)"
fi

echo "done."
