#!/bin/bash

# A script to remove everything from bridges repository/subtree, except:
#
# - modules/grandpa;
# - modules/messages;
# - modules/parachains;
# - modules/relayers;
# - everything required from primitives folder.

set -eux

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

# let's avoid any restrictions on where this script can be called for - bridges repo may be
# plugged into any other repo folder. So the script (and other stuff that needs to be removed)
# may be located either in call dir, or one of it subdirs.
BRIDGES_FOLDER="$( cd -- "$( dirname -- "${BASH_SOURCE[0]}" )" &> /dev/null && pwd )/.."

# let's leave repository/subtree in its original (clean) state if something fails below
function revert_to_clean_state {
	[[ ! -z "${NO_REVERT}" ]] || { echo "Reverting to clean state..."; git checkout .; }
}
trap revert_to_clean_state EXIT

# remove everything we think is not required for our needs
rm -rf $BRIDGES_FOLDER/.config
rm -rf $BRIDGES_FOLDER/.github
rm -rf $BRIDGES_FOLDER/.maintain
rm -rf $BRIDGES_FOLDER/deployments
rm -f $BRIDGES_FOLDER/docs/dockerhub-*
rm -rf $BRIDGES_FOLDER/fuzz
rm -rf $BRIDGES_FOLDER/modules/beefy
rm -rf $BRIDGES_FOLDER/modules/shift-session-manager
rm -rf $BRIDGES_FOLDER/primitives/beefy
rm -rf $BRIDGES_FOLDER/relays
rm -rf $BRIDGES_FOLDER/relay-clients
rm -rf $BRIDGES_FOLDER/scripts/add_license.sh
rm -rf $BRIDGES_FOLDER/scripts/build-containers.sh
rm -rf $BRIDGES_FOLDER/scripts/ci-cache.sh
rm -rf $BRIDGES_FOLDER/scripts/dump-logs.sh
rm -rf $BRIDGES_FOLDER/scripts/license_header
rm -rf $BRIDGES_FOLDER/scripts/regenerate_runtimes.sh
rm -rf $BRIDGES_FOLDER/scripts/update-weights.sh
rm -rf $BRIDGES_FOLDER/scripts/update-weights-setup.sh
rm -rf $BRIDGES_FOLDER/scripts/update_substrate.sh
rm -rf $BRIDGES_FOLDER/substrate-relay
rm -rf $BRIDGES_FOLDER/tools
rm -f $BRIDGES_FOLDER/.dockerignore
rm -f $BRIDGES_FOLDER/local.Dockerfile.dockerignore
rm -f $BRIDGES_FOLDER/deny.toml
rm -f $BRIDGES_FOLDER/.gitlab-ci.yml
rm -f $BRIDGES_FOLDER/.editorconfig
rm -f $BRIDGES_FOLDER/Cargo.toml
rm -f $BRIDGES_FOLDER/ci.Dockerfile
rm -f $BRIDGES_FOLDER/local.Dockerfile
rm -f $BRIDGES_FOLDER/CODEOWNERS
rm -f $BRIDGES_FOLDER/Dockerfile
rm -f $BRIDGES_FOLDER/rustfmt.toml
rm -f $BRIDGES_FOLDER/RELEASE.md

# let's fix Cargo.toml a bit (it'll be helpful if we are in the bridges repo)
if [[ ! -f "Cargo.toml" ]]; then
	cat > Cargo.toml <<-CARGO_TOML
	[workspace.package]
	authors = ["Parity Technologies <admin@parity.io>"]
	edition = "2021"
	repository = "https://github.com/paritytech/parity-bridges-common.git"
	license = "GPL-3.0-only"

	[workspace]
	resolver = "2"

	members = [
		"bin/runtime-common",
		"modules/*",
		"primitives/*",
	]
	CARGO_TOML
fi

# let's test if everything we need compiles

cargo check -p pallet-bridge-grandpa
cargo check -p pallet-bridge-grandpa --features runtime-benchmarks
cargo check -p pallet-bridge-grandpa --features try-runtime
cargo check -p pallet-bridge-messages
cargo check -p pallet-bridge-messages --features runtime-benchmarks
cargo check -p pallet-bridge-messages --features try-runtime
cargo check -p pallet-bridge-parachains
cargo check -p pallet-bridge-parachains --features runtime-benchmarks
cargo check -p pallet-bridge-parachains --features try-runtime
cargo check -p pallet-bridge-relayers
cargo check -p pallet-bridge-relayers --features runtime-benchmarks
cargo check -p pallet-bridge-relayers --features try-runtime
cargo check -p pallet-xcm-bridge-hub-router
cargo check -p pallet-xcm-bridge-hub-router --features runtime-benchmarks
cargo check -p pallet-xcm-bridge-hub-router --features try-runtime
cargo check -p bridge-runtime-common
cargo check -p bridge-runtime-common --features runtime-benchmarks
cargo check -p bridge-runtime-common --features integrity-test

# we're removing lock file after all checks are done. Otherwise we may use different
# Substrate/Polkadot/Cumulus commits and our checks will fail
rm -f $BRIDGES_FOLDER/Cargo.lock

echo "OK"
