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
		pallet_xcm_benchmarks::generic
		pallet_xcm_benchmarks::fungible
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
	output_file="${pallet//::/_}"
	extra_args=""
	# a little hack for pallet_xcm_benchmarks - we want to force custom implementation for XcmWeightInfo
  if [[ "$pallet" == "pallet_xcm_benchmarks::generic" ]] || [[ "$pallet" == "pallet_xcm_benchmarks::fungible" ]]; then
		output_file="xcm/$output_file"
		extra_args="--template=./templates/xcm-bench-template.hbs"
  fi
	$artifactsDir/polkadot-parachain benchmark pallet \
		$extra_args \
		--chain=$benchmarkRuntimeName \
		--execution=wasm \
		--wasm-execution=compiled \
		--pallet=$pallet  \
		--extrinsic='*' \
		--steps=$steps  \
		--repeat=$repeat \
		--json \
		--header=./file_header.txt \
		--output="${benchmarkOutput}/${output_file}.rs" >> $artifactsDir/${pallet}_benchmark.json
done
