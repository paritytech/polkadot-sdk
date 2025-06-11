#!/usr/bin/env bash

# This is used to build our binaries:
# - polkadot
# - polkadot-parachain
# - polkadot-omni-node
# set -e

BIN=$1
PACKAGE=${2:-$BIN}

PROFILE=${PROFILE:-production}
# parity-macos runner needs a path where it can
# write, so make it relative to github workspace.
ARTIFACTS=$GITHUB_WORKSPACE/artifacts/$BIN
VERSION=$(git tag -l --contains HEAD | grep -E "^v.*")
# must be given as feature1,feature2,feature3...
FEATURES=$3
if [ -n "$FEATURES" ]; then
  FEATURES="--features ${FEATURES}"
fi

echo "Artifacts will be copied into $ARTIFACTS"
mkdir -p "$ARTIFACTS"

git log --pretty=oneline -n 1
time cargo build --profile $PROFILE --locked --verbose --bin $BIN --package $PACKAGE $FEATURES

echo "Artifact target: $ARTIFACTS"

cp ./target/$PROFILE/$BIN "$ARTIFACTS"
pushd "$ARTIFACTS" > /dev/null
sha256sum "$BIN" | tee "$BIN.sha256"

EXTRATAG="$($ARTIFACTS/$BIN --version |
    sed -n -r 's/^'$BIN' ([0-9.]+.*-[0-9a-f]{7,13})-.*$/\1/p')"

EXTRATAG="${VERSION}-${EXTRATAG}-$(cut -c 1-8 $ARTIFACTS/$BIN.sha256)"

echo "$BIN version = ${VERSION} (EXTRATAG = ${EXTRATAG})"
echo -n ${VERSION} > "$ARTIFACTS/VERSION"
echo -n ${EXTRATAG} > "$ARTIFACTS/EXTRATAG"
