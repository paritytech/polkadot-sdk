# Asset Hub Next

## Local Development

In any case, prepare a chain-spec

```
# in this directory
cargo build --release
chain-spec-builder create --runtime ../../../../../target/release/wbuild/asset-hub-next-westend-runtime/asset_hub_next_westend_runtime.wasm --relay-chain rococo-local --para-id 1234 named-preset genesis
```

Real setup with Zombienet

```
zombienet --provider native spawn zombienet-omni-node.toml
```

Single-node, single dev mode. This doesn't check things like PoV limits at all, be careful!

```
polkadot-omni-node --chain ./chain_spec.json --dev-block-time 1000 --tmp
```

