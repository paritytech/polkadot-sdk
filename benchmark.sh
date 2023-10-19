#!/bin/bash

steps=10
repeat=3

binary="./target/release/node-template"
# binary="./target/release/node-sassafras"
# binary="./target/debug/node-template"

export RUST_LOG="sassafras=debug"

pallet='pallet_sassafras'

bench=$1

if [[ $bench == "" ]]; then
    list=$($binary benchmark pallet --list | grep $pallet | cut -d ',' -f 2)

    echo "Usage: $0 <benchmark>"
    echo ""
    echo "Available benchmarks:"
    for bench in $list; do
        echo "- $bench"
    done
    exit
fi

extrinsic=$bench

$binary benchmark pallet \
    --chain dev \
    --pallet $pallet \
    --extrinsic "$extrinsic" \
    --steps $steps \
    --repeat $repeat \
    --output weights.rs \
    --template substrate/.maintain/frame-weight-template.hbs
