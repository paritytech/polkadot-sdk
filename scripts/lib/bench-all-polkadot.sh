#!/bin/bash

# Runs all benchmarks for all pallets, for a given runtime, provided by $1
# Should be run on a reference machine to gain accurate benchmarks
# current reference machine: https://github.com/paritytech/polkadot/pull/6508/files
# original source: https://github.com/paritytech/polkadot/blob/b9842c4b52f6791fef6c11ecd020b22fe614f041/scripts/run_all_benches.sh

get_arg required --runtime "$@"
runtime="${out:-""}"

# default RUST_LOG is error, but could be overridden
export RUST_LOG="${RUST_LOG:-error}"

echo "[+] Compiling benchmarks..."
cargo build --profile $profile --locked --features=runtime-benchmarks -p polkadot

POLKADOT_BIN="./target/$profile/polkadot"

# Update the block and extrinsic overhead weights.
echo "[+] Benchmarking block and extrinsic overheads..."
OUTPUT=$(
  $POLKADOT_BIN benchmark overhead \
  --chain="${runtime}-dev" \
  --wasm-execution=compiled \
  --weight-path="$output_path/runtime/${runtime}/constants/src/weights/" \
  --warmup=10 \
  --repeat=100 \
  --header="$output_path/file_header.txt"
)
if [ $? -ne 0 ]; then
  echo "$OUTPUT" >> "$ERR_FILE"
  echo "[-] Failed to benchmark the block and extrinsic overheads. Error written to $ERR_FILE; continuing..."
fi


# Load all pallet names in an array.
PALLETS=($(
  $POLKADOT_BIN benchmark pallet --list --chain="${runtime}-dev" |\
    tail -n+2 |\
    cut -d',' -f1 |\
    sort |\
    uniq
))

echo "[+] Benchmarking ${#PALLETS[@]} pallets for runtime $runtime"

# Define the error file.
ERR_FILE="${ARTIFACTS_DIR}/benchmarking_errors.txt"
# Delete the error file before each run.
rm -f $ERR_FILE

# Benchmark each pallet.
for PALLET in "${PALLETS[@]}"; do
  echo "[+] Benchmarking $PALLET for $runtime";

  output_file=""
  if [[ $PALLET == *"::"* ]]; then
    # translates e.g. "pallet_foo::bar" to "pallet_foo_bar"
    output_file="${PALLET//::/_}.rs"
  fi

  OUTPUT=$(
    $POLKADOT_BIN benchmark pallet \
    --chain="${runtime}-dev" \
    --steps=50 \
    --repeat=20 \
    --no-storage-info \
    --no-median-slopes \
    --no-min-squares \
    --pallet="$PALLET" \
    --extrinsic="*" \
    --execution=wasm \
    --wasm-execution=compiled \
    --header="$output_path/file_header.txt" \
    --output="$output_path/runtime/${runtime}/src/weights/${output_file}" 2>&1
  )
  if [ $? -ne 0 ]; then
    echo "$OUTPUT" >> "$ERR_FILE"
    echo "[-] Failed to benchmark $PALLET. Error written to $ERR_FILE; continuing..."
  fi
done

# Check if the error file exists.
if [ -f "$ERR_FILE" ]; then
  echo "[-] Some benchmarks failed. See: $ERR_FILE"
else
  echo "[+] All benchmarks passed."
fi
