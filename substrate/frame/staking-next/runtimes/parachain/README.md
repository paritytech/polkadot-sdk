# Asset Hub Next

## Local Development

In any case, prepare a chain-spec.

### Custom Polkadot Node

Until https://github.com/paritytech/polkadot-sdk/issues/7664#issuecomment-2678983053 is resolved, we have to create a custom `polkadot`+`polkadot-execution-worker`+`polkadot-prepare-worker`. You can use this branch:
https://github.com/paritytech/polkadot-sdk/pull/new/kiz-larger-PVF

Build/install binaries from above.

Then:

From the `polkadot-sdk` project root dir:

```bash
cargo build --release -p asset-hub-next-westend-runtime -p staging-chain-spec-builder
./target/release/chain-spec-builder create --runtime ./target/release/wbuild/asset-hub-next-westend-runtime/asset_hub_next_westend_runtime.compact.compressed.wasm --relay-chain westend-local --para-id 1100 named-preset development
./target/release/chain-spec-builder convert-to-raw ./chain_spec.json
```

Note that the para-id is set in the chain-spec too and must be 1100 to match.

If errors like

```bash
$ npx @acala-network/chopsticks@latest -c ./cumulus/parachains/runtimes/assets/asset-hub-next-westend/ah-next-chopsticks.yml --genesis chain_spec.json

ZodError: [
  {
    "code": "invalid_type",
    "expected": "object",
    "received": "undefined",
    "path": [
      "genesis",
      "raw"
    ],
    "message": "Required"
  }
]
```

it is likely that the third step above `chain-spec-builder convert-to-raw` was forgotten.

### Chopsticks quickstart
```bash
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


### Starting the Election

As it stands now, the election process is dormant. In the future, it will be kickstarted by the rc-client pallet.
For local testing, do the following:

Start the chain. When ready, submit the following extrinsic:

```
Multiblock::manage(ForceSetPhase(Phase::Snapshot(64)))
```

This extrinsic is gated by Sudo, or `EnsureSigned`. See `impl multiblock::Config for Runtime { type AdminOrigin = .. }` in `staking.rs`.
