# Asset Hub Next

## Local Development

In any case, prepare a chain-spec.

### Custom Polkadot Node

Until https://github.com/paritytech/polkadot-sdk/issues/7664#issuecomment-2678983053 is resolved, we have to create a custom `polkadot`+`polkadot-execution-worker`+`polkadot-prepare-worker`. You can use this branch:
https://github.com/paritytech/polkadot-sdk/pull/new/kiz-larger-PVF

Build/install binaries from above.

Then:

```
cargo build --release -p asset-hub-next-westend-runtime -p staging-chain-spec-builder
../../../../../target/release/chain-spec-builder create --runtime ../../../../../target/release/wbuild/asset-hub-next-westend-runtime/asset_hub_next_westend_runtime.compact.compressed.wasm --relay-chain rococo-local --para-id 1100 named-preset genesis
```

Note that the para-id is set in the chain-spec too and must be 1100 to match.

### Chopsticks quickstart
```
npx @acala-network/chopsticks@latest -c ./cumulus/parachains/runtimes/assets/asset-hub-next-westend/ah-next-chopsticks.yml --genesis chain_spec.json
```
Access it via localhost:8000 in [pjs apps](https://polkadot.js.org/apps/?rpc=ws://127.0.0.1:8000) or programatically with PAPI etc.

### Real setup with Zombienet

```
zombienet --provider native spawn zombienet-omni-node.toml
```

> Or just use `build-and-run-zn.sh` .

Single-node, single dev mode. This doesn't check things like PoV limits at all, be careful!

```
polkadot-omni-node --chain ./chain_spec.json --dev-block-time 12000 --tmp
```
