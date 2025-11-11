# run this, then copy all files to `substrate/frame/election-provider-multi-block/src/weights/`
source ~/.zshrc

STEPS=2
REPEAT=2

# if any of the command line arguments are equal to `--log=X`, set X to the below log levels
LOG="runtime::multiblock-election=info,runtime::staking-async=info,polkadot_sdk_frame::benchmark=info"

if [[ "${NO_COMPILE}" == "1" ]]; then
    echo "Skipping compilation because 'NO_COMPILE' was set"
else
	cargo build --release -p frame-omni-bencher
  	FORCE_WASM_BUILD=$RANDOM  WASMTIME_BACKTRACE_DETAILS=1 RUST_LOG=${LOG} cargo build --release -p pallet-staking-async-parachain-runtime --features runtime-benchmarks
fi

WASM_BLOB_PATH=../../../../../target/release/wbuild/pallet-staking-async-parachain-runtime/pallet_staking_async_parachain_runtime.compact.wasm

echo "WASM_BLOB_PATH: $WASM_BLOB_PATH"
echo "Last modified date of WASM_BLOB:"
stat -f "%Sm" $WASM_BLOB_PATH

run_benchmark() {
  local pallet_name="$1"
  local genesis_preset="$2"
  local output_file="./${pallet_name}_${genesis_preset}.rs"

  echo "Running benchmark for pallet '$pallet_name' with preset '$genesis_preset'..."
  echo "Outputting to '$output_file'"

  WASMTIME_BACKTRACE_DETAILS=1 RUST_LOG=${LOG} \
    ../../../../../target/release/frame-omni-bencher v1 benchmark pallet \
    --pallet "$pallet_name" \
    --extrinsic "*" \
    --runtime "$WASM_BLOB_PATH" \
    --steps "$STEPS" \
    --repeat "$REPEAT" \
    --genesis-builder-preset "$genesis_preset" \
    --template "../../../../../substrate/frame/election-provider-multi-block/src/template.hbs" \
    --heap-pages 65000 \
    --output "$output_file"
}

# run_benchmark "pallet_staking_async" "fake-dot"
run_benchmark "pallet_election_provider_multi_block" "fake-dot"
run_benchmark "pallet_election_provider_multi_block_signed" "fake-dot"
run_benchmark "pallet_election_provider_multi_block_unsigned" "fake-dot"
run_benchmark "pallet_election_provider_multi_block_verifier" "fake-dot"

# run_benchmark "pallet_staking_async" "fake-ksm"
run_benchmark "pallet_election_provider_multi_block" "fake-ksm"
run_benchmark "pallet_election_provider_multi_block_signed" "fake-ksm"
run_benchmark "pallet_election_provider_multi_block_unsigned" "fake-ksm"
run_benchmark "pallet_election_provider_multi_block_verifier" "fake-ksm"
