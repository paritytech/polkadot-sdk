This is a tool for generating the bridge runtime code from metadata.

Example commands:

```
cargo run --bin runtime-codegen -- --from-node-url "http://localhost:20433" > /tmp/rialto_codegen.rs
```

```
cargo run --bin runtime-codegen -- --from-node-url "wss://rococo-bridge-hub-rpc.polkadot.io:443" > /tmp/rococo_codegen.rs
```

```
cargo run --bin runtime-codegen -- --from-wasm ~/workplace/bridge-hub-rococo_runtime-v9360.compact.compressed.wasm > /tmp/rococo_bridge_hub_codegen.rs
```