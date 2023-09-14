#!/bin/bash -eu

CURRENT_DIR=$(pwd)

usage() {
  echo "usage $0 docker-image epoch-duration [options]"
  echo " -c chain-name - base chain-spec to be used [default: westend-staging)"
  exit -1
}

if [ "$#" -lt 2 ]; then
  usage
fi

# docker image to be used
DOCKER_IMAGE=$1
EPOCH_DURATION_IN_BLOCKS=$2

shift 2

CHAIN_NAME="westend-staging"

while getopts "c:" o; do
    case "${o}" in
        c)
            CHAIN_NAME=${OPTARG}
            ;;
        *)
            usage
            ;;
    esac
done

if [ -z $DOCKER_IMAGE ]; then
  usage
fi

OUTPUT_ROOT_DIR=exported-runtimes/

# polkadot command:
POLKADOT_CMD="docker run --rm -v $CURRENT_DIR:/dir -w /dir $DOCKER_IMAGE"

# extract rococo runtime with adjusted epoch diration from docker image
docker export $(docker create $DOCKER_IMAGE) | \
  tar --transform="s|polkadot/runtimes/|$OUTPUT_ROOT_DIR/|" -xf - polkadot/runtimes/rococo-runtime-$EPOCH_DURATION_IN_BLOCKS/rococo_runtime.wasm

# path to extracted rococo runtime:
WASM_RUNTIME_BLOB_PATH=$OUTPUT_ROOT_DIR/rococo-runtime-$EPOCH_DURATION_IN_BLOCKS/rococo_runtime.wasm

# do hexdump of runtime:
hexdump -v -e '/1 "%02x"' $WASM_RUNTIME_BLOB_PATH > $WASM_RUNTIME_BLOB_PATH.hex

# get westend spec:
$POLKADOT_CMD build-spec --chain $CHAIN_NAME > $CURRENT_DIR/wococo-source.json

# replace runtime in chainspec with newly built runtime with overwritten epoch duration:
jq --rawfile code $WASM_RUNTIME_BLOB_PATH.hex '.genesis.runtime.system.code = "0x" + $code' > $CURRENT_DIR/chainspec-nonraw.json < $CURRENT_DIR/wococo-source.json 

# jq will write numbers in compact way with 1e+18, substrtate json parser dont support it. 
sed 's/1e+18/1000000000000000000/' -i $CURRENT_DIR/chainspec-nonraw.json

# generate raw
$POLKADOT_CMD build-spec --chain ./chainspec-nonraw.json --raw > $CURRENT_DIR/chainspec-raw.json
