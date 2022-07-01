#!/bin/bash

steps=50
repeat=20
category=$1
runtimeName=$2

benchmarkOutput=./parachains/runtimes/$category/$runtimeName/src/weights
benchmarkRuntimeName="$runtimeName-dev"

pallets=(
    pallet_assets
	pallet_balances
	pallet_collator_selection
	pallet_multisig
	pallet_proxy
	pallet_session
	pallet_timestamp
	pallet_utility
	pallet_uniques
	cumulus_pallet_xcmp_queue
	frame_system
)

for pallet in ${pallets[@]}
do
	./artifacts/polkadot-parachain benchmark pallet \
		--chain=$benchmarkRuntimeName \
		--execution=wasm \
		--wasm-execution=compiled \
		--pallet=$pallet  \
		--extrinsic='*' \
		--steps=$steps  \
		--repeat=$repeat \
		--json \
        --header=./file_header.txt \
		--output=$benchmarkOutput >> ./artifacts/${pallet}_benchmark.json
done
