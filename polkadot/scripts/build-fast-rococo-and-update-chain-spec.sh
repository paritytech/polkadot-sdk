#!/bin/bash -eux

CURRENT_DIR=$(pwd)

#directory were polkadot-sdk is checked out
POLKADOT_SDK_DIR=$(git rev-parse --show-toplevel)

# required epoch duration:
EPOCH_DURATION_IN_BLOCKS=10

# polkadot command:
DOCKER_IMAGE=paritypr/polkadot-debug:1539-2085d3f0
POLKADOT_CMD="docker run --rm -v $CURRENT_DIR:/dir -w /dir $DOCKER_IMAGE"
# POLKADOT_CMD=$POLKADOT_SDK_DIR/target/release/polkadot 

# path to built rococo runtime:
WASM_RUNTIME_BLOB_PATH=$POLKADOT_SDK_DIR/target/release/wbuild/rococo-runtime/rococo_runtime.compact.compressed.wasm

# build rococo runtime with adjusted epoch diration
pushd $POLKADOT_SDK_DIR
ROCOCO_EPOCH_DURATION=$EPOCH_DURATION_IN_BLOCKS cargo build --features fast-runtime --release -p rococo-runtime
popd

# do hexdump of runtime:
hexdump -v -e '/1 "%02x"' $WASM_RUNTIME_BLOB_PATH > ./runtime.hex

# get westend spec:
$POLKADOT_CMD build-spec --chain westend-staging  > $CURRENT_DIR/wococo-source.json

# replace runtime in chainspec with newly built runtime with overwritten epoch duration:
jq --rawfile code runtime.hex  '.genesis.runtime.system.code = "0x" + $code' > $CURRENT_DIR/chainspec-nonraw.json < $CURRENT_DIR/wococo-source.json 

# jq will write numbers in compact way with 1e+18, substrtate json parser dont support it. 
sed 's/1e+18/1000000000000000000/' -i $CURRENT_DIR/chainspec-nonraw.json

# generate raw
$POLKADOT_CMD build-spec --chain ./chainspec-nonraw.json --raw > $CURRENT_DIR/chainspec-raw.json
