echo "✅ building chain-spec-builder and pallet-staking-next-rc-runtime and pallet-staking-next-parachain-runtime"

LOG="runtime::multiblock-election=info,runtime::staking=info"

if [ "$1" != "no-compile" ]; then
    RUST_LOG=${LOG} cargo build --release -p pallet-staking-next-rc-runtime -p pallet-staking-next-parachain-runtime -p staging-chain-spec-builder
else
    echo "Skipping compilation because 'no-compile' argument was provided."
fi

echo "✅ removing any old chain-spec file"
rm ./parachain.json
rm ./rc.json

echo "✅ creating ah-next chain specs"
RUST_LOG=${LOG} ../../../../../target/release/chain-spec-builder \
    create \
    -t development \
    --runtime ../../../../../target/release/wbuild/pallet-staking-next-parachain-runtime/pallet_staking_next_parachain_runtime.compact.compressed.wasm \
    --relay-chain rococo-local \
    --para-id 1100 \
    named-preset development
mv ./chain_spec.json ./parachain.json

echo "✅ creating westend-next chain specs"
RUST_LOG=${LOG} ../../../../../target/release/chain-spec-builder \
    create \
    -t development \
    --runtime ../../../../../target/release/wbuild/pallet-staking-next-rc-runtime/fast_runtime_binary.rs.wasm \
    named-preset local_testnet
mv ./chain_spec.json ./rc.json

echo "✅ launching ZN"
zombienet --provider native -l text spawn zombienet-staking-runtimes.toml
