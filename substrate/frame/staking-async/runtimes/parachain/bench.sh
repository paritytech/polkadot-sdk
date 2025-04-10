source ~/.zshrc

STEPS=2
REPEAT=3
LOG="runtime::multiblock-election=debug,runtime::staking=debug,polkadot_sdk_frame::benchmark=debug"

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

if [ "$2" != "no-compile" ]; then
    WASMTIME_BACKTRACE_DETAILS=1 VALIDATOR_COUNT=${VALIDATOR_COUNT} VALIDATORS=${VALIDATORS} NOMINATORS=${NOMINATORS} RUST_LOG=${LOG} WASM_BUILD_TYPE=debug cargo build --release -p pallet-staking-async-parachain-runtime --features runtime-benchmarks
else
      echo "Skipping compilation because 'no-compile' argument was provided."
fi

WASMTIME_BACKTRACE_DETAILS=1 RUST_LOG=${LOG}  \
  frame-omni-bencher v1 benchmark pallet \
  --pallet pallet-election-provider-multi-block \
  --extrinsic "export_terminal" \
  --runtime ../../../../../target/release/wbuild/pallet-staking-async-parachain-runtime/pallet_staking_async_parachain_runtime.compact.compressed.wasm \
  --steps $STEPS \
  --repeat $REPEAT \
  --genesis-builder-policy=none \
  --template ../../../../../substrate/.maintain/frame-weight-template.hbs \
  --heap-pages 65000 \
