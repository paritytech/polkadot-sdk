#!/bin/bash

THIS_DIR=$(dirname "${BASH_SOURCE[0]}")
. "$THIS_DIR/../command-utils.sh"

bench_pallet_common_args=(
  --
  benchmark
  pallet
  --steps=50
  --repeat=20
  --extrinsic="*"
  --wasm-execution=compiled
  --heap-pages=4096
  --json-file="${ARTIFACTS_DIR}/bench.json"
)
bench_pallet() {
  get_arg required --subcommand "$@"
  local subcommand="${out:-""}"

  get_arg required --runtime "$@"
  local runtime="${out:-""}"

  get_arg required --pallet "$@"
  local pallet="${out:-""}"

  local args
  case "$target_dir" in
    substrate)
      args=(
        --features=runtime-benchmarks
        --manifest-path="$output_path/bin/node/cli/Cargo.toml"
        "${bench_pallet_common_args[@]}"
        --pallet="$pallet"
        --chain="$runtime"
      )

      case "$subcommand" in
        pallet)
          # Translates e.g. "pallet_foo::bar" to "pallet_foo_bar"
          local output_dir="${pallet//::/_}"

          # Substrate benchmarks are output to the "frame" directory but they aren't
          # named exactly after the $pallet argument. For example:
          # - When $pallet == pallet_balances, the output folder is frame/balances
          # - When $pallet == frame_benchmarking, the output folder is frame/benchmarking
          # The common pattern we infer from those examples is that we should remove
          # the prefix
          if [[ "$output_dir" =~ ^[A-Za-z]*[^A-Za-z](.*)$ ]]; then
            output_dir="${BASH_REMATCH[1]}"
          fi

          # We also need to translate '_' to '-' due to the folders' naming
          # conventions
          output_dir="${output_dir//_/-}"

          args+=(
            --header="$output_path/HEADER-APACHE2"
            --output="$output_path/frame/$output_dir/src/weights.rs"
            --template="$output_path/.maintain/frame-weight-template.hbs"
          )
        ;;
        *)
          die "Subcommand $subcommand is not supported for $target_dir in bench_pallet"
        ;;
      esac
    ;;
    polkadot)
      # For backward compatibility: replace "-dev" with ""
      runtime=${runtime/-dev/}

      local weights_dir="$output_path/runtime/${runtime}/src/weights"

      args=(
        --bin=polkadot
        --features=runtime-benchmarks
        "${bench_pallet_common_args[@]}"
        --pallet="$pallet"
        --chain="${runtime}-dev"
      )

      case "$subcommand" in
        pallet)
          args+=(
            --header="$output_path/file_header.txt"
            --output="${weights_dir}/"
          )
        ;;
        xcm)
          args+=(
            --header="$output_path/file_header.txt"
            --template="$output_path/xcm/pallet-xcm-benchmarks/template.hbs"
            --output="${weights_dir}/xcm/"
          )
        ;;
        *)
          die "Subcommand $subcommand is not supported for $target_dir in bench_pallet"
        ;;
      esac
    ;;
    cumulus)
      get_arg required --runtime_dir "$@"
      local runtime_dir="${out:-""}"
      local chain="$runtime"

      # to support specifying parachain id from runtime name (e.g. ["glutton-westend", "glutton-westend-dev-1300"])
      # If runtime ends with "-dev" or "-dev-\d+", leave as it is, otherwise concat "-dev" at the end of $chain
      if [[ ! "$runtime" =~ -dev(-[0-9]+)?$ ]]; then
          chain="${runtime}-dev"
      fi

      # replace "-dev" or "-dev-\d+" with "" for runtime
      runtime=$(echo "$runtime" | sed 's/-dev.*//g')

      args=(
        -p=polkadot-parachain-bin
        --features=runtime-benchmarks
        "${bench_pallet_common_args[@]}"
        --pallet="$pallet"
        --chain="${chain}"
        --header="$output_path/file_header.txt"
      )

      case "$subcommand" in
        pallet)
          args+=(
            --output="$output_path/parachains/runtimes/$runtime_dir/$runtime/src/weights/"
          )
        ;;
        xcm)
          mkdir -p "$output_path/parachains/runtimes/$runtime_dir/$runtime/src/weights/xcm"
          args+=(
            --template="$output_path/templates/xcm-bench-template.hbs"
            --output="$output_path/parachains/runtimes/$runtime_dir/$runtime/src/weights/xcm/"
          )
        ;;
        *)
          die "Subcommand $subcommand is not supported for $target_dir in bench_pallet"
        ;;
      esac
    ;;
    trappist)
      local weights_dir="$output_path/runtime/$runtime/src/weights"

      args=(
        --features=runtime-benchmarks
        "${bench_pallet_common_args[@]}"
        --pallet="$pallet"
        --chain="${runtime}-dev"
        --header="$output_path/templates/file_header.txt"
      )

      case "$subcommand" in
        pallet)
          args+=(
            --output="${weights_dir}/"
          )
        ;;
        xcm)
          args+=(
            --template="$output_path/templates/xcm-bench-template.hbs"
            --output="${weights_dir}/xcm/"
          )
        ;;
        *)
          die "Subcommand $subcommand is not supported for $target_dir in bench_pallet"
        ;;
      esac
    ;;
    *)
      die "Repository $target_dir is not supported in bench_pallet"
    ;;
  esac

  cargo_run "${args[@]}"
}

bench_pallet "$@"
