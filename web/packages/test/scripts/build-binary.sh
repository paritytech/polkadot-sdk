#!/usr/bin/env bash
set -eu

source scripts/set-env.sh

build_binaries() {
    pushd $root_dir/polkadot-sdk

    local features=''
    if [[ "$active_spec" != "minimal" ]]; then
        features=--features beacon-spec-mainnet
    fi

    check_local_changes "polkadot"
    check_local_changes "substrate"

  # Check that all 3 binaries are available and no changes made in the polkadot and substrate dirs
    if [[ ! -e "target/release/polkadot" || ! -e "target/release/polkadot-execute-worker" || ! -e "target/release/polkadot-prepare-worker" || "$changes_detected" -eq 1 ]]; then
        echo "Building polkadot binary, due to changes detected in polkadot or substrate, or binaries not found"
        cargo build --release --locked --bin polkadot --bin polkadot-execute-worker --bin polkadot-prepare-worker
    else
        echo "No changes detected in polkadot or substrate and binaries are available, not rebuilding relaychain binaries."
    fi

    cp target/release/polkadot $output_bin_dir/polkadot
    cp target/release/polkadot-execute-worker $output_bin_dir/polkadot-execute-worker
    cp target/release/polkadot-prepare-worker $output_bin_dir/polkadot-prepare-worker

    echo "Building polkadot-parachain binary"
    cargo build --release --locked -p polkadot-parachain-bin --bin polkadot-parachain $features --no-default-features
    cp target/release/polkadot-parachain $output_bin_dir/polkadot-parachain

    popd
}

changes_detected=0

check_local_changes() {
    local dir=$1
    cd "$dir"
    if git status --porcelain | grep .; then
        changes_detected=1
    fi
    cd -
}

build_contracts() {
    echo "Building contracts"
    pushd $root_dir/contracts
    forge build
    popd
}

build_relayer() {
    echo "Building relayer"
    mage -d "$relay_dir" build
    cp $relay_bin "$output_bin_dir"
}

install_binary() {
    echo "Building and installing binaries."
    mkdir -p $output_bin_dir
    build_binaries
    build_contracts
    build_relayer
}

if [ -z "${from_start_services:-}" ]; then
    echo "build binaries only!"
    install_binary
fi
