#!/usr/bin/env bash

set -Eeu -o pipefail
shopt -s inherit_errexit

PR_TITLE="$1"
HEAD_REF="$2"

ORG="paritytech"
REPO="$CI_PROJECT_NAME"
BASE_REF="$CI_COMMIT_BRANCH"
# Change threshold in %. Bigger values excludes the small changes.
THRESHOLD=${THRESHOLD:-30}

WEIGHTS_COMPARISON_URL_PARTS=(
  "https://weights.tasty.limo/compare?"
  "repo=$REPO&"
  "threshold=$THRESHOLD&"
  "path_pattern=**%2Fweights%2F*.rs&"
  "method=guess-worst&"
  "ignore_errors=true&"
  "unit=time&"
  "old=$BASE_REF&"
  "new=$HEAD_REF"
)
printf -v WEIGHTS_COMPARISON_URL %s "${WEIGHTS_COMPARISON_URL_PARTS[@]}"

PAYLOAD="$(jq -n \
  --arg title "$PR_TITLE" \
  --arg body "
This PR is generated automatically by CI.

Compare the weights with \`$BASE_REF\`: $WEIGHTS_COMPARISON_URL

- [ ] Backport to master and node release branch once merged
" \
  --arg base "$BASE_REF" \
  --arg head "$HEAD_REF" \
  '{
      title: $title,
      body: $body,
      head: $head,
      base: $base
   }'
)"

echo "PAYLOAD: $PAYLOAD"

curl \
  -H "Authorization: token $GITHUB_TOKEN" \
  -X POST \
  -d "$PAYLOAD" \
  "https://api.github.com/repos/$ORG/$REPO/pulls"
