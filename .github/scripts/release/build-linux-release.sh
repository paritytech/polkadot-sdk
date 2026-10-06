#!/usr/bin/env bash

# This is used to build our binaries:
# - polkadot
# - polkadot-parachain
# - polkadot-omni-node
#
set -e

BIN=$1
PACKAGE=${2:-$BIN}
# must be given as feature1,feature2,feature3...
FEATURES=$3
if [ -n "$FEATURES" ]; then
  FEATURES="--features ${FEATURES}"
fi

PROFILE=${PROFILE:-production}
ARTIFACTS=/artifacts/$BIN

# Cross build when TARGET is set; the binary then only runs under qemu.
TARGET_ARGS=""
TARGET_DIR="./target/$PROFILE"
RUN_BIN=""
if [ -n "$TARGET" ]; then
  TARGET_ARGS="--target $TARGET"
  TARGET_DIR="./target/$TARGET/$PROFILE"
  case "$TARGET" in
    aarch64-unknown-linux-gnu) RUN_BIN="qemu-aarch64-static -L /usr/aarch64-linux-gnu" ;;
    *) echo "Unsupported cross target: $TARGET"; exit 1 ;;
  esac
fi

echo "Artifacts will be copied into $ARTIFACTS"
mkdir -p "$ARTIFACTS"

git log --pretty=oneline -n 1
time cargo build --profile $PROFILE --locked --verbose $TARGET_ARGS --bin $BIN --package $PACKAGE $FEATURES

echo "Artifact target: $ARTIFACTS"

cp "$TARGET_DIR/$BIN" "$ARTIFACTS"
pushd "$ARTIFACTS" > /dev/null
sha256sum "$BIN" | tee "$BIN.sha256"
chmod a+x "$BIN"
VERSION="$($RUN_BIN $ARTIFACTS/$BIN --version)"
EXTRATAG="$(echo "${VERSION}" |
    sed -n -r 's/^'$BIN' ([0-9.]+.*-[0-9a-f]{7,13})-.*$/\1/p')"
EXTRATAG="${VERSION}-${EXTRATAG}-$(cut -c 1-8 $ARTIFACTS/$BIN.sha256)"

echo "$BIN version = ${VERSION} (EXTRATAG = ${EXTRATAG})"
echo -n ${VERSION} > "$ARTIFACTS/VERSION"
echo -n ${EXTRATAG} > "$ARTIFACTS/EXTRATAG"
