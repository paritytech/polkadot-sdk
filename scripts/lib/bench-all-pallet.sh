#!/usr/bin/env bash

set -eu -o pipefail
shopt -s inherit_errexit
shopt -s globstar

. "$(dirname "${BASH_SOURCE[0]}")/../command-utils.sh"

get_arg required --pallet "$@"
PALLET="${out:-""}"

REPO_NAME="$(basename "$PWD")"
BASE_COMMAND="$(dirname "${BASH_SOURCE[0]}")/../../bench/bench.sh --noexit=true --subcommand=pallet"

WEIGHT_FILE_PATHS=( $(find . -type f -name "${PALLET}.rs" -path "**/weights/*" | sed 's|^\./||g') )

# convert pallet_ranked_collective to ranked-collective
CLEAN_PALLET=$(echo $PALLET | sed 's/pallet_//g' | sed 's/_/-/g')

# add substrate pallet weights to a list
SUBSTRATE_PALLET_PATH=$(ls substrate/frame/$CLEAN_PALLET/src/weights.rs || :)
if [ ! -z "${SUBSTRATE_PALLET_PATH}" ]; then
  WEIGHT_FILE_PATHS+=("$SUBSTRATE_PALLET_PATH")
fi

# add trappist pallet weights to a list
TRAPPIST_PALLET_PATH=$(ls pallet/$CLEAN_PALLET/src/weights.rs || :)
if [ ! -z "${TRAPPIST_PALLET_PATH}" ]; then
  WEIGHT_FILE_PATHS+=("$TRAPPIST_PALLET_PATH")
fi

COMMANDS=()

if [ "${#WEIGHT_FILE_PATHS[@]}" -eq 0 ]; then
  echo "No weights files found for pallet: $PALLET"
  exit 1
else
  echo "Found weights files for pallet: $PALLET"
fi

for f in ${WEIGHT_FILE_PATHS[@]}; do
  echo "- $f"
  # f examples:
  # cumulus/parachains/runtimes/assets/asset-hub-rococo/src/weights/pallet_balances.rs
  # polkadot/runtime/rococo/src/weights/pallet_balances.rs
  # runtime/trappist/src/weights/pallet_assets.rs
  TARGET_DIR=$(echo $f | cut -d'/' -f 1)

  if [ "$REPO_NAME" == "polkadot-sdk" ]; then
    case $TARGET_DIR in
      cumulus)
        TYPE=$(echo $f | cut -d'/' -f 2)
        # Example: cumulus/parachains/runtimes/assets/asset-hub-rococo/src/weights/pallet_balances.rs
        if [ "$TYPE" == "parachains" ]; then
          RUNTIME=$(echo $f | cut -d'/' -f 5)
          RUNTIME_DIR=$(echo $f | cut -d'/' -f 4)
          COMMANDS+=("$BASE_COMMAND --runtime=$RUNTIME --runtime_dir=$RUNTIME_DIR --target_dir=$TARGET_DIR --pallet=$PALLET")
        fi
        ;;
      polkadot)
        # Example: polkadot/runtime/rococo/src/weights/pallet_balances.rs
        RUNTIME=$(echo $f | cut -d'/' -f 3)
        COMMANDS+=("$BASE_COMMAND --runtime=$RUNTIME --target_dir=$TARGET_DIR --pallet=$PALLET")
        ;;
      substrate)
        # Example: substrate/frame/contracts/src/weights.rs
        COMMANDS+=("$BASE_COMMAND --target_dir=$TARGET_DIR --runtime=dev --pallet=$PALLET")
        ;;
      *)
        echo "Unknown dir: $TARGET_DIR"
        exit 1
        ;;
    esac
  fi

  if [ "$REPO_NAME" == "trappist" ]; then
    case $TARGET_DIR in
      runtime)
        TYPE=$(echo $f | cut -d'/' -f 2)
        if [ "$TYPE" == "trappist" || "$TYPE" == "stout" ]; then
          # Example: runtime/trappist/src/weights/pallet_assets.rs
          COMMANDS+=("$BASE_COMMAND --target_dir=trappist --runtime=$TYPE --pallet=$PALLET")
        fi
        ;;
      *)
        echo "Unknown dir: $TARGET_DIR"
        exit 1
        ;;
    esac
  fi
done

for cmd in "${COMMANDS[@]}"; do
  echo "Running command: $cmd"
  . $cmd
done
