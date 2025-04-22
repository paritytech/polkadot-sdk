source ~/.zshrc

STEPS=10
REPEAT=20

# if any of the command line arguments are equal to `--log=X`, set X to the below log levels
LOG="runtime::multiblock-election=debug,runtime::staking-async=debug,polkadot_sdk_frame::benchmark=debug"

if [ "$1" != "no-compile" ]; then
	  cargo build --release -p frame-omni-bencher
    FORCE_WASM_BUILD=$RANDOM  WASMTIME_BACKTRACE_DETAILS=1 RUST_LOG=${LOG} cargo build --release -p pallet-staking-async-parachain-runtime --features runtime-benchmarks
else
      echo "Skipping compilation because 'no-compile' argument was provided."
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
    --extrinsic "all" \
    --runtime "$WASM_BLOB_PATH" \
    --steps "$STEPS" \
    --repeat "$REPEAT" \
    --genesis-builder-preset "$genesis_preset" \
    --template "../../../../../substrate/.maintain/frame-weight-template.hbs" \
    --heap-pages 65000 \
    --output "$output_file"
}

run_benchmark "pallet_staking_async" "dot_size"
run_benchmark "pallet_election_provider_multi_block" "dot_size"
run_benchmark "pallet_election_provider_multi_block_signed" "dot_size"
run_benchmark "pallet_election_provider_multi_block_unsigned" "dot_size"
run_benchmark "pallet_election_provider_multi_block_verifier" "dot_size"

run_benchmark "pallet_staking_async" "ksm_size"
run_benchmark "pallet_election_provider_multi_block" "ksm_size"
run_benchmark "pallet_election_provider_multi_block_signed" "ksm_size"
run_benchmark "pallet_election_provider_multi_block_unsigned" "ksm_size"
run_benchmark "pallet_election_provider_multi_block_verifier" "ksm_size"
