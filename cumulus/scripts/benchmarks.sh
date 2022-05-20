#!/bin/bash

steps=50
repeat=20

statemineOutput=./parachains/runtimes/statemine/src/weights
statemintOutput=./parachains/runtimes/statemint/src/weights
westmintOutput=./parachains/runtimes/westmint/src/weights

statemineChain=statemine-dev
statemintChain=statemint-dev
westmintChain=westmint-dev

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
	./target/production/polkadot-parachain benchmark pallet \
		--chain=$statemineChain \
		--execution=wasm \
		--wasm-execution=compiled \
		--pallet=$p  \
		--extrinsic='*' \
		--steps=$steps  \
		--repeat=$repeat \
		--json-file=./bench-statemine.json \
        --header=./file_header.txt \
		--output=$statemineOutput

	./target/production/polkadot-parachain benchmark pallet \
		--chain=$statemintChain \
		--execution=wasm \
		--wasm-execution=compiled \
		--pallet=$p  \
		--extrinsic='*' \
		--steps=$steps  \
		--repeat=$repeat \
		--json-file=./bench-statemint.json \
        --header=./file_header.txt \
		--output=$statemintOutput

	./target/production/polkadot-parachain benchmark pallet \
		--chain=$westmintChain \
		--execution=wasm \
		--wasm-execution=compiled \
		--pallet=$p  \
		--extrinsic='*' \
		--steps=$steps  \
		--repeat=$repeat \
		--json-file=./bench-westmint.json \
        --header=./file_header.txt \
		--output=$westmintOutput
done
