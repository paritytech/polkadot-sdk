#!/bin/bash

THIS_DIR=$(dirname "${BASH_SOURCE[0]}")
. "$THIS_DIR/../command-utils.sh"

bench_overhead_common_args=(
  --
  benchmark
  overhead
  --wasm-execution=compiled
  --warmup=10
  --repeat=100
)
bench_overhead() {
  local args
  case "$target_dir" in
    substrate)
      args=(
        --bin=substrate
        "${bench_overhead_common_args[@]}"
        --header="$output_path/HEADER-APACHE2"
        --weight-path="$output_path/frame/support/src/weights"
        --chain="dev"
      )
    ;;
    polkadot)
      get_arg required --runtime "$@"
      local runtime="${out:-""}"
      args=(
        --bin=polkadot
        "${bench_overhead_common_args[@]}"
        --header="$output_path/file_header.txt"
        --weight-path="$output_path/runtime/$runtime/constants/src/weights"
        --chain="$runtime-dev"
      )
    ;;
    cumulus)
      get_arg required --runtime "$@"
      local runtime="${out:-""}"
      args=(
        -p=polkadot-parachain-bin
        "${bench_overhead_common_args[@]}"
        --header="$output_path/file_header.txt"
        --weight-path="$output_path/parachains/runtimes/assets/$runtime/src/weights"
        --chain="$runtime"
      )
    ;;
    trappist)
      get_arg required --runtime "$@"
      local runtime="${out:-""}"
      args=(
        "${bench_overhead_common_args[@]}"
        --header="$output_path/templates/file_header.txt"
        --weight-path="$output_path/runtime/$runtime/src/weights"
        --chain="$runtime-dev"
      )
    ;;
    *)
      die "Target Dir \"$target_dir\" is not supported in bench_overhead"
    ;;
  esac

  cargo_run "${args[@]}"
}

bench_overhead "$@"
