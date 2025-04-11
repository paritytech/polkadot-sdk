# Staking Async test

To run locally:

1. Ensure you have got chainspecs for RC and AH-next in the crate root.
2. Ensure you have got `polkadot`, `polkadot-execute-worker`, `polkadot-prepare-worker` and
   `polkadot-parachain` in your `PATH`.
3. Run `ZOMBIE_PROVIDER="native" cargo test happy_case`

## How to build the chainspecs

Courtesy to @kianenigma for these instructions.

1. Run `cargo build --release -p pallet-staking-async-rc-runtime -p
   pallet-staking-async-parachain-runtime -p staging-chain-spec-builder`
2. For AH-Next run

```
chain-spec-builder \
    create \
    -t development \
    --runtime ../../target/release/wbuild/pallet-staking-async-parachain-runtime/pallet_staking_async_parachain_runtime.compact.compressed.wasm \
    --relay-chain rococo-local \
    --para-id 1100 \
    named-preset development
mv ./chain_spec.json ./parachain.json
```

3. For RC run

```
chain-spec-builder \
    create \
    -t development \
    --runtime ../../target/release/wbuild/pallet-staking-async-rc-runtime/fast_runtime_binary.rs.wasm \
    named-preset local_testnet
mv ./chain_spec.json ./rc.json
```
