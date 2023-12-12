#!/usr/bin/env bash

export PRODUCT=polkadot
export VERSION=${VERSION:-v1.5.0}

PROJECT_ROOT=`git rev-parse --show-toplevel`
echo $PROJECT_ROOT

TMP=$(mktemp -d)
TEMPLATE_AUDIENCE="${PROJECT_ROOT}/scripts/release/templates/audience.md.tera"
TEMPLATE_CHANGELOG="${PROJECT_ROOT}/scripts/release/templates/changelog.md.tera"

DATA_JSON="${TMP}/data.json"
CONTEXT_JSON="${TMP}/context.json"
echo "TEMPLATE_AUDIENCE=$TEMPLATE_AUDIENCE"
echo "DATA_JSON=$DATA_JSON"
echo "CONTEXT_JSON=$CONTEXT_JSON"

prdoc load -d "$PROJECT_ROOT/prdoc" --json > $DATA_JSON
# ls -al $DATA_JSON

cat $DATA_JSON | jq ' { "prdoc" : .}' > $CONTEXT_JSON
# ls -al $CONTEXT_JSON

# Fetch the list of valid audiences
SCHEMA_URL=https://raw.githubusercontent.com/paritytech/polkadot-sdk/master/prdoc/schema_user.json
SCHEMA=$(curl -s $SCHEMA_URL | sed 's|^//.*||')
AUDIENCE_ARRAY=$(echo -E $SCHEMA | jq -r '."$defs".audience.oneOf[] | .const')

readarray -t audiences < <(echo "$AUDIENCE_ARRAY")
declare -p audiences

# Create output folder
OUTPUT="${TMP}/changelogs/$PRODUCT/$VERSION"
echo "OUTPUT=$OUTPUT"
mkdir -p $OUTPUT

# Generate a changelog
echo "Generating changelog..."
tera -t "${TEMPLATE_CHANGELOG}" --env --env-key env "${CONTEXT_JSON}" > "$OUTPUT/changelog.md"

code $OUTPUT/changelog.md

# Generate a release notes doc per audience
for audience in "${audiences[@]}"; do
    audience_id="$(tr [A-Z] [a-z] <<< "$audience")"
    audience_id="$(tr ' ' '_' <<< "$audience_id")"
    echo "Processing audience: $audience ($audience_id)"
    export TARGET_AUDIENCE=$audience
    tera -t "${TEMPLATE_AUDIENCE}" --env --env-key env "${CONTEXT_JSON}" > "$OUTPUT/relnote_${audience_id}.md"
done

# Show the files
tree -s -h -c $OUTPUT/
