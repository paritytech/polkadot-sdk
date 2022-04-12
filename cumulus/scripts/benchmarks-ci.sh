#!/bin/bash

steps=50
repeat=20
chainName=$1

benhcmarkOutput=./polkadot-parachains/$chainName/src/weights
benhcmarkChainName="$chainName-dev"

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

for p in ${pallets[@]}
do
	./artifacts/polkadot-collator benchmark \
		--chain=$benhcmarkChainName \
		--execution=wasm \
		--wasm-execution=compiled \
		--pallet=$p  \
		--extrinsic='*' \
		--steps=$steps  \
		--repeat=$repeat \
		--json \
        --header=./file_header.txt \
		--output=$benhcmarkOutput
done
