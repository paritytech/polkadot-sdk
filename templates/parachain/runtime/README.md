# Runtime

ℹ️ The runtime (in other words, a state transition function), refers to the core logic of the parachain that is
responsible for validating blocks and executing the state changes they define.

💁 The runtime in this template is constructed using ready-made FRAME pallets that ship with
[Polkadot SDK](https://github.com/paritytech/polkadot-sdk), and a [template for a custom pallet](../pallets/README.md).

👉 Learn more about FRAME
[here](https://paritytech.github.io/polkadot-sdk/master/polkadot_sdk_docs/polkadot_sdk/frame_runtime/index.html).

## Cross-chain delivery fees (XCM)

This template preconfigures delivery fees:

- HRMP (sibling parachains): `PriceForSiblingDelivery` uses an exponential price model in `runtime/src/configs/mod.rs`.
- UMP (to relay chain): a constant delivery fee via `ParentAsUmp` in `runtime/src/configs/xcm_config.rs`.

Tune the following constants to match your network economics:

- `BaseDeliveryFee` and `TransactionByteFee` in `runtime/src/configs/mod.rs`.
- `FeeAssetId` (asset used to pay) in `runtime/src/configs/mod.rs`.
- `UmpDeliveryFeeAssets` in `runtime/src/configs/xcm_config.rs`.
