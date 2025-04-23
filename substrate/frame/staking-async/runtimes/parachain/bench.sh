source ~/.zshrc

STEPS=2
REPEAT=1

# if any of the command line arguments are equal to `--log=X`, set X to the below log levels
LOG="runtime::multiblock-election=debug,runtime::staking-async=debug,polkadot_sdk_frame::benchmark=debug"

if [ "$3" != "no-compile" ]; then
    FORCE_WASM_BUILD=$RANDOM  WASMTIME_BACKTRACE_DETAILS=1 RUST_LOG=${LOG} cargo build --release -p pallet-staking-async-parachain-runtime --features runtime-benchmarks
else
      echo "Skipping compilation because 'no-compile' argument was provided."
fi

WASM_BLOB_PATH=../../../../../target/release/wbuild/pallet-staking-async-parachain-runtime/pallet_staking_async_parachain_runtime.compact.wasm

echo "WASM_BLOB_PATH: $WASM_BLOB_PATH"
echo "Last modified date of WASM_BLOB:"
stat -f "%Sm" $WASM_BLOB_PATH

WASMTIME_BACKTRACE_DETAILS=1 RUST_LOG=${LOG}  \
  frame-omni-bencher v1 benchmark pallet \
  --pallet "$1" \
  --extrinsic "all" \
  --runtime $WASM_BLOB_PATH \
  --steps $STEPS \
  --repeat $REPEAT \
  --genesis-builder-preset $2 \
  --template ../../../../../substrate/.maintain/frame-weight-template.hbs \
  --heap-pages 65000 \
  --output ./$1_$2.rs \
