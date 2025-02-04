# Asset Hub Next

## Local Development

In any case, prepare a chain-spec.

```
VALIDATORS=1000 NOMINATORS=20000 cargo build --release -p asset-hub-next-westend-runtime
rm ./target/release/wbuild/asset-hub-next-westend-runtime/asset_hub_next_westend_runtime.wasm
cargo build -p staging-chain-spec-builder
./target/debug/chain-spec-builder create --runtime ./target/release/wbuild/asset-hub-next-westend-runtime/asset_hub_next_westend_runtime.compact.compressed.wasm --relay-chain westend-local --para-id 1100 named-preset genesis
./target/debug/chain-spec-builder convert-to-raw chain_spec.json
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

Single-node, single dev mode. This doesn't check things like PoV limits at all, be careful!

```
polkadot-omni-node --chain ./chain_spec.json --dev-block-time 12000 --tmp
```
