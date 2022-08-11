#!/usr/bin/env bash

steps=50
repeat=20
category=$1
runtimeName=$2
artifactsDir=$3

benchmarkOutput=./parachains/runtimes/$category/$runtimeName/src/weights
benchmarkRuntimeName="$runtimeName-dev"

if [[ $runtimeName == "statemint" ]] || [[ $runtimeName == "statemine" ]] || [[ $runtimeName == "westmint" ]]; then
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
elif [[ $runtimeName == "collectives-polkadot" ]]; then
		pallets=(
			pallet_alliance
			pallet_balances
			pallet_collator_selection
			pallet_collective
			pallet_multisig
			pallet_proxy
			pallet_session
			pallet_timestamp
			pallet_utility
			cumulus_pallet_xcmp_queue
			frame_system
		)
else
	echo "$runtimeName pallet list not found in benchmarks-ci.sh"
	exit 1
fi

for pallet in ${pallets[@]}
do
	$artifactsDir/polkadot-parachain benchmark pallet \
		--chain=$benchmarkRuntimeName \
		--execution=wasm \
		--wasm-execution=compiled \
		--pallet=$pallet  \
		--extrinsic='*' \
		--steps=$steps  \
		--repeat=$repeat \
		--json \
        --header=./file_header.txt \
		--output=$benchmarkOutput >> $artifactsDir/${pallet}_benchmark.json

done
