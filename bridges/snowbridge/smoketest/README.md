# Smoketests

Smoke tests for a running E2E environment

# Setup

1. First make sure the E2E Stack is running. See [web/packages/test/README.md](../web/packages/test/README.md).

2. Generate Rust bindings for both sides of the bridge

```shell
./make-bindings.sh
```

# Run Tests

## Assets

### Token Registration

Send an ethereum transaction to the Gateway to register a new wrapped token on the AssetHub parachain.

```
cargo test --test register_token -- --nocapture
```
### Send Tokens

Send an ethereum transaction to the Gateway to send tokens to the AssetHub parachain. Must have registered the token previously.

```
cargo test --test send_token -- --nocapture
```

## Governance

### Upgrade the Gateway

Send an upgrade transaction via the relaychain. This operation will brick the bridge as it upgrades the gateway to mock implementation. Please restart the testnet after running the test.

```
cargo test --test upgrade_gateway -- --nocapture
```

# Troubleshooting

Polkadot-JS explorers for local parachains:

* [BridgeHub](https://polkadot.js.org/apps/?rpc=ws%3A%2F%2F127.0.0.1%3A11144#/explorer)
* [AssetHub]((https://polkadot.js.org/apps/?rpc=ws%3A%2F%2F127.0.0.1%3A12144#/explorer))
