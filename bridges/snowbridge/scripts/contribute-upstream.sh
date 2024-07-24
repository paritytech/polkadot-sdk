#!/bin/bash

# A script to cleanup the Snowfork fork of the polkadot-sdk to contribute it upstream back to parity/polkadot-sdk
# ./bridges/snowbridge/scripts/contribute-upstream.sh <branchname>

# show CLI help
function show_help() {
  set +x
  echo " "
  echo Error: $1
  echo "Usage:"
  echo "  ./bridges/snowbridge/scripts/contribute-upstream.sh <branchname>         Exit with code 0 if pallets code is well decoupled from the other code in the repo"
  exit 1
}

if [[ -z "$1" ]]; then
    echo "Please provide a branch name you would like your upstream branch to be named"
    exit 1
fi

branch_name=$1

set -eux

# let's avoid any restrictions on where this script can be called for - snowbridge repo may be
# plugged into any other repo folder. So the script (and other stuff that needs to be removed)
# may be located either in call dir, or one of it subdirs.
SNOWBRIDGE_FOLDER="$( cd -- "$( dirname -- "${BASH_SOURCE[0]}" )" &> /dev/null && pwd )/../"

# Get the current Git branch name
current_branch=$(git rev-parse --abbrev-ref HEAD)

if [ "$current_branch" = "$branch_name" ] || git branch | grep -q "$branch_name"; then
    echo "Already on requested branch or branch exists, not creating."
else
    git branch "$branch_name"
fi

git checkout "$branch_name"

# remove everything we think is not required for our needs
rm -rf rust-toolchain.toml
rm -rf codecov.yml
rm -rf $SNOWBRIDGE_FOLDER/.cargo
rm -rf $SNOWBRIDGE_FOLDER/.github
rm -rf $SNOWBRIDGE_FOLDER/SECURITY.md
rm -rf $SNOWBRIDGE_FOLDER/.gitignore
rm -rf $SNOWBRIDGE_FOLDER/rustfmt.toml
rm -rf $SNOWBRIDGE_FOLDER/templates
rm -rf $SNOWBRIDGE_FOLDER/pallets/ethereum-client/fuzz

pushd $SNOWBRIDGE_FOLDER

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

# we're removing lock file after all checks are done. Otherwise we may use different
# Substrate/Polkadot/Cumulus commits and our checks will fail
rm -f $SNOWBRIDGE_FOLDER/Cargo.toml
rm -f $SNOWBRIDGE_FOLDER/Cargo.lock

popd

# Replace Parity's CI files, that we have overwritten in our fork, to run our own CI
rm -rf .github
git remote -v | grep -w parity || git remote add parity https://github.com/paritytech/polkadot-sdk
git fetch parity master
git checkout parity/master -- .github
git add -- .github

git commit -m "cleanup branch"

# Fetch the latest from parity master
echo "Fetching latest from Parity master. Resolve merge conflicts, if there are any."
git fetch parity master
git merge parity/master
echo "OK"
