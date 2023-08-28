#!/usr/bin/env bash

category=$1
runtimeName=$2
artifactsDir=$3
steps=${4:-50}
repeat=${5:-20}

benchmarkOutput=./parachains/runtimes/$category/$runtimeName/src/weights
benchmarkRuntimeName="$runtimeName-dev"

if [ $category = "glutton" ]; then
    benchmarkRuntimeName="$runtimeName-dev-1300"
fi

# Load all pallet names in an array.
pallets=($(
  ${artifactsDir}/polkadot-parachain benchmark pallet --list --chain="${benchmarkRuntimeName}" |\
    tail -n+2 |\
    cut -d',' -f1 |\
    sort |\
    uniq
))

if [ ${#pallets[@]} -ne 0 ]; then
	echo "[+] Benchmarking ${#pallets[@]} pallets for runtime $runtime"
else
	echo "$runtimeName pallet list not found in benchmarks-ci.sh"
	exit 1
fi

for pallet in ${pallets[@]}
do
	output_dir=""
	extra_args=""
	# a little hack for pallet_xcm_benchmarks - we want to force custom implementation for XcmWeightInfo
	if [[ "$pallet" == "pallet_xcm_benchmarks::generic" ]] || [[ "$pallet" == "pallet_xcm_benchmarks::fungible" ]]; then
		output_dir="xcm/"
		extra_args="--template=./templates/xcm-bench-template.hbs"
	fi
	$artifactsDir/polkadot-parachain benchmark pallet \
		$extra_args \
		--chain=$benchmarkRuntimeName \
		--wasm-execution=compiled \
		--pallet=$pallet  \
		--extrinsic='*' \
		--steps=$steps  \
		--repeat=$repeat \
		--json \
		--header=./file_header.txt \
		--output="${benchmarkOutput}/${output_dir}" >> $artifactsDir/${pallet}_benchmark.json
done
