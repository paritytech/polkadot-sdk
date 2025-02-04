# Asset Hub Next

## Local Development

In any case, prepare a chain-spec.

```
# small election
RUST_LOG=runtime=debug VALIDATORS=100 NOMINATORS=2000 cargo build --release -p asset-hub-next-westend-runtime --features fast-runtime
# large election
RUST_LOG=runtime=debug VALIDATORS=1000 NOMINATORS=20000 cargo build --release -p asset-hub-next-westend-runtime --features fast-runtime

chain-spec-builder create --runtime target/release/wbuild/asset-hub-next-westend-runtime/asset_hub_next_westend_runtime.wasm --relay-chain rococo-local --para-id 1100 named-preset genesis
```

Note that the para-id is set in the chain-spec too and must match this one.

### OmniNode

Single-node, dev mode. This doesn't check things like PoV limits at all, be careful!

```
cargo install polkadot-omni-node
RUST_LOG=runtime=debug,runtime::multiblock-election=trace polkadot-omni-node --chain ./chain_spec.json --dev-block-time 1000 --tmp --offchain-worker=always
```


### Zombienet

Real setup with Zombienet

```
zombienet --provider native spawn cumulus/parachains/runtimes/assets/asset-hub-next-westend/zombienet-omni-node.toml
```

