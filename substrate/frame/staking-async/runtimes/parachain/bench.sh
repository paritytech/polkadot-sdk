source ~/.zshrc

STEPS=2
REPEAT=1
LOG="runtime::multiblock-election=debug,runtime::staking-async=debug,polkadot_sdk_frame::benchmark=debug"

# Check if an argument was provided
if [ -z "$1" ]; then
    echo "Error: No argument provided."
    echo "Usage: $0 [dot|ksm|fast]"
    exit 1
fi

# Set variables based on argument
case "$1" in
    dot)
        VALIDATOR_COUNT=500
        NOMINATORS=25000
        VALIDATORS=2000
        ;;
    ksm)
        VALIDATOR_COUNT=1000
        NOMINATORS=15000
        VALIDATORS=4000
        ;;
    fast)
        VALIDATOR_COUNT=100
        NOMINATORS=100
        VALIDATORS=20
        ;;
    *)
        echo "Error: Invalid argument \"$1\""
        echo "Usage: $0 [dot|ksm|fast]"
        exit 1
        ;;
esac

if [ "$3" != "no-compile" ]; then
    FORCE_WASM_BUILD=$RANDOM  WASMTIME_BACKTRACE_DETAILS=1 VALIDATOR_COUNT=${VALIDATOR_COUNT} VALIDATORS=${VALIDATORS} NOMINATORS=${NOMINATORS} RUST_LOG=${LOG} cargo build --release -p pallet-staking-async-parachain-runtime --features runtime-benchmarks
else
      echo "Skipping compilation because 'no-compile' argument was provided."
fi

WASM_BLOB_PATH=../../../../../target/release/wbuild/pallet-staking-async-parachain-runtime/pallet_staking_async_parachain_runtime.compact.wasm

echo "WASM_BLOB_PATH: $WASM_BLOB_PATH"
echo "Last modified date of WASM_BLOB:"
stat -f "%Sm" $WASM_BLOB_PATH

WASMTIME_BACKTRACE_DETAILS=1 RUST_LOG=${LOG}  \
  frame-omni-bencher v1 benchmark pallet \
  --pallet $2 \
  --extrinsic "*" \
  --runtime $WASM_BLOB_PATH \
  --steps $STEPS \
  --repeat $REPEAT \
  --template ../../../../../substrate/.maintain/frame-weight-template.hbs \
  --heap-pages 65000 \
