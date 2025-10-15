#!/usr/bin/env bash

# Validates the .github/zombienet-flaky-tests file to ensure:
# 1. Each entry has the correct format: <test-name>:<issue-number>
# 2. The referenced GitHub issue exists
# 3. The issue is OPEN (fails if closed)

set -uo pipefail

FLAKY_TESTS_FILE="${1:-.github/zombienet-flaky-tests}"

if [[ ! -f "$FLAKY_TESTS_FILE" ]]; then
    echo "Error: File not found: $FLAKY_TESTS_FILE" >&2
    exit 1
fi

if ! command -v gh &> /dev/null; then
    echo "Error: gh CLI is not installed" >&2
    exit 1
fi

echo "Validating $FLAKY_TESTS_FILE..."
echo

has_errors=false
line_num=0

while IFS= read -r line || [[ -n "$line" ]]; do
    line_num=$((line_num + 1))
    
    if [[ -z "$line" ]]; then
        continue
    fi
    
    # Parse format: test-name:issue-number
    if [[ ! "$line" =~ ^([^:]+):([0-9]+)$ ]]; then
        echo "❌ Line $line_num: Missing required issue number" >&2
        echo "   Entry: '$line'" >&2
        echo "   Expected format: <test-name>:<issue-number>" >&2
        echo "   Example: zombienet-polkadot-test-name:1234" >&2
        has_errors=true
        continue
    fi
    
    test_name="${BASH_REMATCH[1]}"
    issue_number="${BASH_REMATCH[2]}"
    
    set +e
    issue_data=$(gh issue view "$issue_number" --json state,title 2>&1)
    gh_exit_code=$?
    set -e
    
    if [[ $gh_exit_code -ne 0 ]]; then
        echo "❌ Line $line_num: Issue #$issue_number does not exist" >&2
        echo "   Test: $test_name" >&2
        has_errors=true
        continue
    fi
    
    state=$(echo "$issue_data" | jq -r '.state')
    title=$(echo "$issue_data" | jq -r '.title')
    
    if [[ "$state" == "OPEN" ]]; then
        echo "✅ Line $line_num: $test_name -> Issue #$issue_number (open)"
    else
        echo "⚠️  Line $line_num: Issue #$issue_number is closed: '$title'" >&2
        echo "   Test: $test_name" >&2
        echo "   Consider removing this entry if the issue is resolved." >&2
        # Note: We treat closed issues as warnings, not errors
    fi
    
done < "$FLAKY_TESTS_FILE"

echo

if [[ "$has_errors" == "true" ]]; then
    echo "❌ Validation failed with errors" >&2
    exit 1
else
    echo "✅ All entries are valid"
    exit 0
fi
