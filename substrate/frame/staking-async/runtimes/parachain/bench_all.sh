# run this, then copy all files to `substrate/frame/election-provider-multi-block/src/weights/`
source ~/.zshrc

STEPS=2
REPEAT=22

# if any of the command line arguments are equal to `--log=X`, set X to the below log levels
<<<<<<< HEAD
LOG="runtime::multiblock-election=debug,runtime::staking-async=debug,polkadot_sdk_frame::benchmark=debug"
=======
LOG="runtime::multiblock-election=info,runtime::staking-async=info,frame::benchmark=info"
>>>>>>> 05a3fb10 (Staking-Async + EPMB: Migrate operations to `poll` (#9925))

if [[ "${NO_COMPILE}" == "1" ]]; then
    echo "Skipping compilation because 'NO_COMPILE' was set"
else
	cargo build --release -p frame-omni-bencher
<<<<<<< HEAD
  FORCE_WASM_BUILD=$RANDOM  WASMTIME_BACKTRACE_DETAILS=1 RUST_LOG=${LOG} cargo build --release -p pallet-staking-async-parachain-runtime --features runtime-benchmarks
=======
  	FORCE_WASM_BUILD=$RANDOM SKIP_PALLET_REVIVE_FIXTURES=1  WASMTIME_BACKTRACE_DETAILS=1 RUST_LOG=${LOG} cargo build --release -p pallet-staking-async-parachain-runtime --features runtime-benchmarks
>>>>>>> 05a3fb10 (Staking-Async + EPMB: Migrate operations to `poll` (#9925))
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

<<<<<<< HEAD
# run_benchmark "pallet_staking_async" "dot_size"
run_benchmark "pallet_election_provider_multi_block" "dot_size"
run_benchmark "pallet_election_provider_multi_block_signed" "dot_size"
run_benchmark "pallet_election_provider_multi_block_unsigned" "dot_size"
run_benchmark "pallet_election_provider_multi_block_verifier" "dot_size"

# run_benchmark "pallet_staking_async" "ksm_size"
run_benchmark "pallet_election_provider_multi_block" "ksm_size"
run_benchmark "pallet_election_provider_multi_block_signed" "ksm_size"
run_benchmark "pallet_election_provider_multi_block_unsigned" "ksm_size"
run_benchmark "pallet_election_provider_multi_block_verifier" "ksm_size"
=======
run_benchmark "pallet_staking_async" "fake-dot"
run_benchmark "pallet_election_provider_multi_block" "fake-dot"
run_benchmark "pallet_election_provider_multi_block_signed" "fake-dot"
run_benchmark "pallet_election_provider_multi_block_unsigned" "fake-dot"
run_benchmark "pallet_election_provider_multi_block_verifier" "fake-dot"

run_benchmark "pallet_staking_async" "fake-ksm"
run_benchmark "pallet_election_provider_multi_block" "fake-ksm"
run_benchmark "pallet_election_provider_multi_block_signed" "fake-ksm"
run_benchmark "pallet_election_provider_multi_block_unsigned" "fake-ksm"
run_benchmark "pallet_election_provider_multi_block_verifier" "fake-ksm"
>>>>>>> 05a3fb10 (Staking-Async + EPMB: Migrate operations to `poll` (#9925))
