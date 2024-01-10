#!/bin/bash

# A script to remove everything from snowbridge repository/subtree, except:
#
# - parachain
# - readme
# - license

set -eu

# show CLI help
function show_help() {
  set +x
  echo " "
  echo Error: $1
  echo "Usage:"
  echo "  ./scripts/verify-pallets-build.sh          Exit with code 0 if pallets code is well decoupled from the other code in the repo"
  echo "Options:"
  echo "  --no-revert                                Leaves only runtime code on exit"
  echo "  --ignore-git-state                         Ignores git actual state"
  exit 1
}

# parse CLI args
NO_REVERT=
IGNORE_GIT_STATE=
for i in "$@"
do
	case $i in
		--no-revert)
			NO_REVERT=true
			shift
			;;
		--ignore-git-state)
			IGNORE_GIT_STATE=true
			shift
			;;
		*)
			show_help "Unknown option: $i"
			;;
	esac
done

# the script is able to work only on clean git copy, unless we want to ignore this check
[[ ! -z "${IGNORE_GIT_STATE}" ]] || [[ -z "$(git status --porcelain)" ]] || { echo >&2 "The git copy must be clean"; exit 1; }

# let's avoid any restrictions on where this script can be called for - snowbridge repo may be
# plugged into any other repo folder. So the script (and other stuff that needs to be removed)
# may be located either in call dir, or one of it subdirs.
SNOWBRIDGE_FOLDER="$( cd -- "$( dirname -- "${BASH_SOURCE[0]}" )" &> /dev/null && pwd )/../.."

# remove everything we think is not required for our needs
rm -rf $SNOWBRIDGE_FOLDER/.cargo
rm -rf $SNOWBRIDGE_FOLDER/.github
rm -rf $SNOWBRIDGE_FOLDER/contracts
rm -rf $SNOWBRIDGE_FOLDER/codecov.yml
rm -rf $SNOWBRIDGE_FOLDER/docs
rm -rf $SNOWBRIDGE_FOLDER/hooks
rm -rf $SNOWBRIDGE_FOLDER/relayer
rm -rf $SNOWBRIDGE_FOLDER/scripts
rm -rf $SNOWBRIDGE_FOLDER/SECURITY.md
rm -rf $SNOWBRIDGE_FOLDER/smoketest
rm -rf $SNOWBRIDGE_FOLDER/web
rm -rf $SNOWBRIDGE_FOLDER/.envrc-example
rm -rf $SNOWBRIDGE_FOLDER/.gitbook.yaml
rm -rf $SNOWBRIDGE_FOLDER/.gitignore
rm -rf $SNOWBRIDGE_FOLDER/.gitmodules
rm -rf $SNOWBRIDGE_FOLDER/_typos.toml
rm -rf $SNOWBRIDGE_FOLDER/_codecov.yml
rm -rf $SNOWBRIDGE_FOLDER/flake.lock
rm -rf $SNOWBRIDGE_FOLDER/flake.nix
rm -rf $SNOWBRIDGE_FOLDER/go.work
rm -rf $SNOWBRIDGE_FOLDER/go.work.sum
rm -rf $SNOWBRIDGE_FOLDER/polkadot-sdk
rm -rf $SNOWBRIDGE_FOLDER/rust-toolchain.toml
rm -rf $SNOWBRIDGE_FOLDER/parachain/rustfmt.toml
rm -rf $SNOWBRIDGE_FOLDER/parachain/.gitignore
rm -rf $SNOWBRIDGE_FOLDER/parachain/templates
rm -rf $SNOWBRIDGE_FOLDER/parachain/.cargo
rm -rf $SNOWBRIDGE_FOLDER/parachain/.config
rm -rf $SNOWBRIDGE_FOLDER/parachain/pallets/ethereum-client/fuzz

cd bridges/snowbridge/parachain

# fix polkadot-sdk paths in Cargo.toml files
find "." -name 'Cargo.toml' | while read -r file; do
    replace=$(printf '../../' )
    if [[ "$(uname)" = "Darwin" ]] || [[ "$(uname)" = *BSD ]]; then
        sed -i '' "s|polkadot-sdk/|$replace|g" "$file"
    else
        sed -i "s|polkadot-sdk/|$replace|g" "$file"
    fi
done

# let's test if everything we need compiles
cargo check -p snowbridge-pallet-ethereum-client
cargo check -p snowbridge-pallet-ethereum-client --features runtime-benchmarks
cargo check -p snowbridge-pallet-ethereum-client --features try-runtime
cargo check -p snowbridge-pallet-inbound-queue
cargo check -p snowbridge-pallet-inbound-queue --features runtime-benchmarks
cargo check -p snowbridge-pallet-inbound-queue --features try-runtime
cargo check -p snowbridge-pallet-outbound-queue
cargo check -p snowbridge-pallet-outbound-queue --features runtime-benchmarks
cargo check -p snowbridge-pallet-outbound-queue --features try-runtime
cargo check -p snowbridge-pallet-system
cargo check -p snowbridge-pallet-system --features runtime-benchmarks
cargo check -p snowbridge-pallet-system --features try-runtime

cd -

# we're removing lock file after all checks are done. Otherwise we may use different
# Substrate/Polkadot/Cumulus commits and our checks will fail
rm -f $SNOWBRIDGE_FOLDER/parachain/Cargo.toml
rm -f $SNOWBRIDGE_FOLDER/parachain/Cargo.lock

echo "OK"
