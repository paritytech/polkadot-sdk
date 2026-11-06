#!/usr/bin/env bash
#
# Ensures that backports always move forward: if a PR is labelled to be backported into
# `stableX`, it must also be labelled for every newer release `stableY` (Y > X).
#
# The set of known releases is taken from the `A4-backport-stableYYMM` labels that exist in the
# repository, so a newly created label is picked up automatically, and a release without a label
# is never required.
#
# Inputs (newline-separated label names):
#   PR_LABELS   - labels of the pull request under test.
#   REPO_LABELS - all labels of the repository.

set -euo pipefail

# Prints the sorted, unique `YYMM` release identifiers of all stable backport labels on stdin.
backport_releases() {
	grep -E '^A4-backport-stable[0-9]{4}$' | sed 's/^A4-backport-stable//' | sort -u || true
}

pr_releases=$(printf '%s\n' "${PR_LABELS:-}" | backport_releases)

if [ -z "$pr_releases" ]; then
	echo "✅ No stable backport labels, nothing to check."
	exit 0
fi

oldest=$(head -n 1 <<< "$pr_releases")
echo "Backport labels on this PR: $(printf 'stable%s ' $pr_releases)"
echo "Oldest backport target: stable$oldest"

# All known releases newer than the oldest backport target that the PR is not labelled for.
# `YYMM` identifiers are fixed-width, so lexicographic and numeric order agree.
missing=$(
	printf '%s\n' "${REPO_LABELS:-}" | backport_releases |
		awk -v oldest="$oldest" '$1 > oldest' |
		comm -23 - <(printf '%s\n' "$pr_releases")
)

if [ -n "$missing" ]; then
	echo "❌ This PR is backported into stable$oldest, so it must also be backported into every newer"
	echo "   release. Add the following labels:"
	for release in $missing; do
		echo "   - A4-backport-stable$release"
	done
	exit 1
fi

echo "✅ Backport labels cover every release from stable$oldest onwards."
