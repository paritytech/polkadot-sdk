echo "✅ building chain-spec-builder and pallet-staking-async-rc-runtime and pallet-staking-async-parachain-runtime"

LOG="runtime::multiblock-election=info,runtime::staking=info"

if [ "$1" != "no-compile" ]; then
    RUST_LOG=${LOG} cargo build --release -p pallet-staking-async-rc-runtime -p pallet-staking-async-parachain-runtime -p staging-chain-spec-builder
else
    echo "Skipping compilation because 'no-compile' argument was provided."
fi

echo "✅ removing any old chain-spec file"
rm ./parachain.json
rm ./rc.json

echo "✅ creating parachain chain specs"
RUST_LOG=${LOG} ../../../../../target/release/chain-spec-builder \
    create \
    -t development \
    --runtime ../../../../../target/release/wbuild/pallet-staking-async-parachain-runtime/pallet_staking_async_parachain_runtime.compact.compressed.wasm \
    --relay-chain rococo-local \
    --para-id 1100 \
    named-preset dot_size
    # named-preset ksm_size
    # named-preset development
    # change this as per your needs ^^^
mv ./chain_spec.json ./parachain.json

echo "✅ creating rc chain specs"
RUST_LOG=${LOG} ../../../../../target/release/chain-spec-builder \
    create \
    -t development \
    --runtime ../../../../../target/release/wbuild/pallet-staking-async-rc-runtime/fast_runtime_binary.rs.wasm \
    named-preset local_testnet
mv ./chain_spec.json ./rc.json

echo "✅ launching ZN"
zombienet --provider native -l text spawn zombienet-staking-runtimes.toml
