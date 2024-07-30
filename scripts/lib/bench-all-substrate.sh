#!/usr/bin/env bash

# This file is part of Substrate.
# Copyright (C) 2022 Parity Technologies (UK) Ltd.
# SPDX-License-Identifier: Apache-2.0
# Licensed under the Apache License, Version 2.0 (the "License");
# you may not use this file except in compliance with the License.
# You may obtain a copy of the License at
#
# http://www.apache.org/licenses/LICENSE-2.0
#
# Unless required by applicable law or agreed to in writing, software
# distributed under the License is distributed on an "AS IS" BASIS,
# WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
# See the License for the specific language governing permissions and
# limitations under the License.

# This script has three parts which all use the Substrate runtime:
# - Pallet benchmarking to update the pallet weights
# - Overhead benchmarking for the Extrinsic and Block weights
# - Machine benchmarking
#
# Should be run on a reference machine to gain accurate benchmarks
# current reference machine: https://github.com/paritytech/substrate/pull/5848

# Original source: https://github.com/paritytech/substrate/blob/ff9921a260a67e3a71f25c8b402cd5c7da787a96/scripts/run_all_benchmarks.sh
# Fail if any sub-command in a pipe fails, not just the last one.
set -o pipefail
# Fail on undeclared variables.
set -u
# Fail if any sub-command fails.
set -e
# Fail on traps.
# set -E

# default RUST_LOG is warn, but could be overridden
export RUST_LOG="${RUST_LOG:-error}"

echo "[+] Compiling Substrate benchmarks..."
cargo build --profile=$profile --locked --features=runtime-benchmarks -p staging-node-cli

# The executable to use.
SUBSTRATE="./target/$profile/substrate-node"

# Manually exclude some pallets.
EXCLUDED_PALLETS=(
  # Helper pallets
  "pallet_election_provider_support_benchmarking"
  # Pallets without automatic benchmarking
  "pallet_babe"
  "pallet_grandpa"
  "pallet_mmr"
  "pallet_offences"
  # Only used for testing, does not need real weights.
  "frame_benchmarking_pallet_pov"
  "pallet_example_tasks"
  "pallet_example_basic"
  "pallet_example_split"
  "pallet_example_kitchensink"
  "pallet_example_mbm"
  "tasks_example"
)

# Load all pallet names in an array.
ALL_PALLETS=($(
  $SUBSTRATE benchmark pallet --list --chain=dev |\
    tail -n+2 |\
    cut -d',' -f1 |\
    sort |\
    uniq
))

# Define the error file.
ERR_FILE="${ARTIFACTS_DIR}/benchmarking_errors.txt"

# Delete the error file before each run.
rm -f "$ERR_FILE"

mkdir -p "$(dirname "$ERR_FILE")"

# Update the block and extrinsic overhead weights.
echo "[+] Benchmarking block and extrinsic overheads..."
OUTPUT=$(
  $SUBSTRATE benchmark overhead \
  --chain=dev \
  --wasm-execution=compiled \
  --weight-path="$output_path/frame/support/src/weights/" \
  --header="$output_path/HEADER-APACHE2" \
  --warmup=10 \
  --repeat=100 2>&1
)
if [ $? -ne 0 ]; then
  echo "$OUTPUT" >> "$ERR_FILE"
  echo "[-] Failed to benchmark the block and extrinsic overheads. Error written to $ERR_FILE; continuing..."
fi

echo "[+] Benchmarking ${#ALL_PALLETS[@]} Substrate pallets and excluding ${#EXCLUDED_PALLETS[@]}."

echo "[+] Excluded pallets ${EXCLUDED_PALLETS[@]}"
echo "[+] ------ "
echo "[+] Whole list pallets ${ALL_PALLETS[@]}"

# Benchmark each pallet.
for PALLET in "${ALL_PALLETS[@]}"; do
  FOLDER="$(echo "${PALLET#*_}" | tr '_' '-')";
  WEIGHT_FILE="$output_path/frame/${FOLDER}/src/weights.rs"

   # Skip the pallet if it is in the excluded list.

  if [[ " ${EXCLUDED_PALLETS[@]} " =~ " ${PALLET} " ]]; then
    echo "[+] Skipping $PALLET as it is in the excluded list."
    continue
  fi

  echo "[+] Benchmarking $PALLET with weight file $WEIGHT_FILE";

  set +e # Disable exit on error for the benchmarking of the pallets
  OUTPUT=$(
    $SUBSTRATE benchmark pallet \
    --chain=dev \
    --steps=50 \
    --repeat=20 \
    --pallet="$PALLET" \
    --no-storage-info \
    --no-median-slopes \
    --no-min-squares \
    --extrinsic="*" \
    --wasm-execution=compiled \
    --heap-pages=4096 \
    --output="$WEIGHT_FILE" \
    --header="$output_path/HEADER-APACHE2" \
    --template="$output_path/.maintain/frame-weight-template.hbs" 2>&1
  )
  if [ $? -ne 0 ]; then
    echo -e "$PALLET: $OUTPUT\n" >> "$ERR_FILE"
    echo "[-] Failed to benchmark $PALLET. Error written to $ERR_FILE; continuing..."
  fi
  set -e # Re-enable exit on error
done


# Check if the error file exists.
if [ -s "$ERR_FILE" ]; then
  echo "[-] Some benchmarks failed. See: $ERR_FILE"
  exit 1
else
  echo "[+] All benchmarks passed."
fi
