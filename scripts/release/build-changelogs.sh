#!/usr/bin/env bash

export PRODUCT=polkadot
export VERSION=${VERSION:-v1.0.1}

PROJECT_ROOT=`git rev-parse --show-toplevel`
echo $PROJECT_ROOT

TMP=$(mktemp -d)
TEMPLATE="${PROJECT_ROOT}/scripts/release/templates/prdoc.md.tera"
DATA_JSON="${TMP}/data.json"
CONTEXT_JSON="${TMP}/context.json"

echo "CONTEXT_JSON=$CONTEXT_JSON"

prdoc load -d "$PROJECT_ROOT/prdoc" --json > $DATA_JSON
ls -al $DATA_JSON

cat $DATA_JSON | jq ' { prdoc : .}' > $CONTEXT_JSON

ls -al $CONTEXT_JSON

# Fetch the list of valid audiences
SCHEMA_URL=https://raw.githubusercontent.com/paritytech/prdoc/master/schema_user.json
SCHEMA=$(curl -s $SCHEMA_URL | sed 's|^//.*||')
AUDIENCE_ARRAY=$(echo -E $SCHEMA | jq -r '."$defs".audience.enum')
readarray -t audiences < <(jq -r '.[]' <<<"$AUDIENCE_ARRAY")
declare -p audiences

# Create output folder
OUTPUT=/tmp/changelogs/$PRODUCT/$VERSION
mkdir -p $OUTPUT

# Generate a changelog per audience
for audience in "${audiences[@]}"; do
    export TARGET_AUDIENCE=$audience
    tera -t "${TEMPLATE}" --env --env-key env "${CONTEXT_JSON}" > "$OUTPUT/changelog_${audience}.md"
done

# Show the files
tree -s -h -c $OUTPUT/
