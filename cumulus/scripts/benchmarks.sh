#!/bin/bash

steps=50
repeat=20

statemineOutput=./polkadot-parachains/statemine/src/weights
statemintOutput=./polkadot-parachains/statemint/src/weights
westmintOutput=./polkadot-parachains/westmint/src/weights

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
    frame_system
)

for p in ${pallets[@]}
do
	./target/release/polkadot-collator benchmark \
		--chain=$statemineChain \
		--execution=wasm \
		--wasm-execution=compiled \
		--pallet=$p  \
		--extrinsic='*' \
		--steps=$steps  \
		--repeat=$repeat \
		--raw  \
        --header=./file_header.txt \
		--output=$statemineOutput

	./target/release/polkadot-collator benchmark \
		--chain=$statemintChain \
		--execution=wasm \
		--wasm-execution=compiled \
		--pallet=$p  \
		--extrinsic='*' \
		--steps=$steps  \
		--repeat=$repeat \
		--raw  \
        --header=./file_header.txt \
		--output=$statemintOutput

	./target/release/polkadot-collator benchmark \
		--chain=$westmintChain \
		--execution=wasm \
		--wasm-execution=compiled \
		--pallet=$p  \
		--extrinsic='*' \
		--steps=$steps  \
		--repeat=$repeat \
		--raw  \
        --header=./file_header.txt \
		--output=$westmintOutput
done
