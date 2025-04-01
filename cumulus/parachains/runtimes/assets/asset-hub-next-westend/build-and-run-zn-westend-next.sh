echo "✅ building chain-spec-builder and ah-next-runtime and westend-next-runtime"

LOG="runtime::multiblock-election=info,runtime::staking=info"

if [ "$1" != "no-compile" ]; then
    RUST_LOG=${LOG} cargo build --release -p asset-hub-next-westend-runtime -p westend-next-runtime -p staging-chain-spec-builder
else
    echo "Skipping compilation because 'no-compile' argument was provided."
fi

echo "✅ removing any old chain-spec file"
rm ./asset_hub_westend_next.json
rm ./westend_next.json

echo "✅ creating ah-next chain specs"
RUST_LOG=${LOG} ../../../../../target/release/chain-spec-builder \
    create \
    -t development \
    --runtime ../../../../../target/release/wbuild/asset-hub-next-westend-runtime/asset_hub_next_westend_runtime.compact.compressed.wasm \
    --relay-chain rococo-local \
    --para-id 1100 \
    named-preset development
mv ./chain_spec.json ./asset_hub_westend_next.json

echo "✅ creating westend-next chain specs"
RUST_LOG=${LOG} ../../../../../target/release/chain-spec-builder \
    create \
    -t development \
    --runtime ../../../../../target/release/wbuild/westend-next-runtime/fast_runtime_binary.rs.wasm \
    named-preset local_testnet
mv ./chain_spec.json ./westend_next.json

echo "✅ launching ZN"
zombienet --provider native -l text spawn zombienet-westend-next.toml
