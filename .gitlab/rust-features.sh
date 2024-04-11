#!/usr/bin/env bash

##############################################################################
#
# This script checks that crates to not carelessly enable features that
# should stay disabled. It's important to check that since features
# are used to gate specific functionality which should only be enabled
# when the feature is explicitly enabled.
#
# Invocation scheme:
# 	./rust-features.sh <CARGO-ROOT-PATH>
#
# Example:
# 	./rust-features.sh path/to/substrate
#
# The steps of this script:
#   1. Check that all required dependencies are installed.
#   2. Check that all rules are fulfilled for the whole workspace. If not:
#   4. Check all crates to find the offending ones.
#   5. Print all offending crates and exit with code 1.
#
##############################################################################

set -eu

# Check that cargo and grep are installed - otherwise abort.
command -v cargo >/dev/null 2>&1 || { echo >&2 "cargo is required but not installed. Aborting."; exit 1; }
command -v grep >/dev/null 2>&1 || { echo >&2 "grep is required but not installed. Aborting."; exit 1; }

# Enter the workspace root folder.
cd "$1"
echo "Workspace root is $PWD"

function main() {
	feature_does_not_imply 'default' 'runtime-benchmarks'
	feature_does_not_imply 'std' 'runtime-benchmarks'
	feature_does_not_imply 'default' 'try-runtime'
	feature_does_not_imply 'std' 'try-runtime'
}

# Accepts two feature names as arguments.
# Checks that the first feature does not imply the second one.
function feature_does_not_imply() {
	ENABLED=$1
	STAYS_DISABLED=$2
	echo "📏 Checking that $ENABLED does not imply $STAYS_DISABLED ..."

	# Check if the forbidden feature is enabled anywhere in the workspace.
	# But only check "normal" dependencies, so no "dev" or "build" dependencies.
	if cargo tree --no-default-features --locked --workspace -e features,normal --features "$ENABLED" | grep -qF "feature \"$STAYS_DISABLED\""; then
		echo "❌ $ENABLED implies $STAYS_DISABLED in the workspace"
	else
		echo "✅ $ENABLED does not imply $STAYS_DISABLED in the workspace"
		return
	fi

	# Find all Cargo.toml files but exclude the root one since we already know that it is broken.
	CARGOS=`find . -name Cargo.toml -not -path ./Cargo.toml`
	NUM_CRATES=`echo "$CARGOS" | wc -l`
	FAILED=0
	PASSED=0
	echo "🔍 Checking all $NUM_CRATES crates - this takes some time."

	for CARGO in $CARGOS; do
		OUTPUT=$(cargo tree --no-default-features --locked --offline -e features,normal --features $ENABLED --manifest-path $CARGO 2>&1 || true)

		if echo "$OUTPUT" | grep -qF "not supported for packages in this workspace"; then
			# This case just means that the pallet does not support the
			# requested feature which is fine.
			PASSED=$((PASSED+1))
		elif echo "$OUTPUT" | grep -qF "feature \"$STAYS_DISABLED\""; then
			echo "❌ Violation in $CARGO by dependency:"
			# Best effort hint for which dependency needs to be fixed.
			echo "$OUTPUT" | grep -wF "feature \"$STAYS_DISABLED\"" | head -n 1
			FAILED=$((FAILED+1))
		else
			PASSED=$((PASSED+1))
		fi
	done

	echo "Checked $NUM_CRATES crates in total of which $FAILED failed and $PASSED passed."
	echo "Exiting with code 1"
	exit 1
}

main "$@"

