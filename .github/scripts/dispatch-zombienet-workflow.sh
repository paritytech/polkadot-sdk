#!/bin/bash

# Zombienet Workflow Dispatcher
#
# This script triggers GitHub Actions workflows for zombienet tests and monitors their execution.
# It can run workflows multiple times for reliability testing and optionally filter tests by pattern.
# Results are automatically saved to a timestamped CSV file for analysis.
#
# Features:
# - Trigger workflows on specific branches
# - Filter tests by pattern (useful for debugging specific tests)
# - Run workflows multiple times for flaky test detection
# - Monitor workflow completion and collect results
# - Export results to CSV with job details (ID, name, conclusion, timing, URLs)
#
# Requirements:
# - GitHub CLI (gh) must be installed and authenticated
# - Must be run from polkadot-sdk repository root
# - Target branch must have corresponding PR with CI enabled

# Exit on error
# set -e

function dbg {
  local msg="$@"

  local tstamp=$(date "+%Y-%m-%d %T")
  printf "%s - %s\n" "$tstamp" "$msg"
}

function write_job_results_to_csv {
  local run_id="$1"
  local branch="$2"
  local csv_file="$3"

  dbg "Writing job results for run $run_id to $csv_file"

  # Get job details for the completed run, filtering only jobs starting with 'zombienet-' and with success or failure conclusions
  gh run view "$run_id" --json jobs --jq \
    '.jobs[] | select(.name | startswith("zombienet-")) |
      select(.conclusion == "success" or .conclusion == "failure") |
      [.databaseId, .name, .conclusion, .startedAt, "'"$branch"'", .url] | @csv' >> "$csv_file"
}

# Parse command line arguments
WORKFLOW_FILE=""
BRANCH=""
MAX_RESULT_CNT=-1
TEST_PATTERN=""

while getopts "w:b:m:p:h" opt; do
  case $opt in
    w) WORKFLOW_FILE="$OPTARG" ;;
    b) BRANCH="$OPTARG" ;;
    m) MAX_RESULT_CNT="$OPTARG" ;;
    p) TEST_PATTERN="$OPTARG" ;;
    h) echo "Usage: $0 -w <workflow-file> -b <branch> [-m max-triggers] [-p test-pattern]"
       echo "  -w: Workflow file (required)"
       echo "  -b: Branch name (required)"
       echo "  -m: Maximum number of triggers (optional, default: infinite)"
       echo "  -p: Test pattern for workflow input (optional)"
       exit 0 ;;
    \?) echo "Invalid option -$OPTARG" >&2
        echo "Use -h for help"
        exit 1 ;;
  esac
done

if [[ -z "$WORKFLOW_FILE" || -z "$BRANCH" ]]; then
  echo "Error: Both workflow file (-w) and branch (-b) are required"
  echo "Usage: $0 -w <workflow-file> -b <branch> [-m max-triggers] [-p test-pattern]"
  echo "Use -h for help"
  exit 1
fi

# Create CSV file with headers
CSV_FILE="workflow_results_$(date +%Y%m%d_%H%M%S).csv"
echo "job_id,job_name,conclusion,started_at,branch,job_url" > "$CSV_FILE"
dbg "Created CSV file: $CSV_FILE"

dbg "Starting loop for workflow: $WORKFLOW_FILE on branch: $BRANCH"

TRIGGER_CNT=0
RESULT_CNT=0

while [[ $MAX_RESULT_CNT -eq -1 || $RESULT_CNT -lt $MAX_RESULT_CNT ]]; do

  dbg "Waiting until workflow $WORKFLOW_FILE (branch: $BRANCH) jobs are completed"

  while true ; do
    echo ""
    gh run list  --workflow=$WORKFLOW_FILE -e workflow_dispatch -b $BRANCH -L 5
    sleep 2
    # if job is completed it should have non-empty conclusion field
    ALL_JOBS_COMPLETED=$(gh run list --workflow=$WORKFLOW_FILE -e workflow_dispatch -b $BRANCH --json conclusion --jq 'all(.[]; .conclusion != "")')
    if [[ "$ALL_JOBS_COMPLETED" == "true" ]]; then
      break
    fi
    sleep 60
  done
  dbg "Workflow $WORKFLOW_FILE (branch: $BRANCH) jobs completed"

  # Skip the first iteration - latest run id is not the one we triggered here
  if [ $TRIGGER_CNT -gt 0 ]; then
    # Get the most recent completed run ID and write job results to CSV
    LATEST_RUN_ID=$(gh run list --workflow=$WORKFLOW_FILE -e workflow_dispatch -b $BRANCH -L 1 --json databaseId --jq '.[0].databaseId')
    write_job_results_to_csv "$LATEST_RUN_ID" "$BRANCH" "$CSV_FILE"
    RESULT_CNT=$(( RESULT_CNT + 1 ))
  fi

  TRIGGER_CNT=$(( TRIGGER_CNT + 1 ))
  dbg "Triggering #$TRIGGER_CNT workflow $WORKFLOW_FILE (branch: $BRANCH)"

  if [[ -n "$TEST_PATTERN" ]]; then
    gh workflow run "$WORKFLOW_FILE" --ref "$BRANCH" -f test_pattern="$TEST_PATTERN"
  else
    gh workflow run "$WORKFLOW_FILE" --ref "$BRANCH"
  fi

  dbg "Sleeping 60s"
  sleep 60
done

