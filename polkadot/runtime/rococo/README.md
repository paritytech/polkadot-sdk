# Rococo: v2.1

Rococo is a testnet runtime with no stability guarantees.

## How to build `rococo` runtime
`EpochDurationInBlocks` parameter is configurable via `ROCOCO_EPOCH_DURATION` environment variable. To build wasm
runtime blob with customized epoch duration the following command shall be exectuted:
```bash
ROCOCO_EPOCH_DURATION=10 ./polkadot/scripts/build-only-wasm.sh rococo-runtime /path/to/output/directory/
```

## How to run `rococo-local`

The [Cumulus Tutorial](https://docs.substrate.io/tutorials/v3/cumulus/start-relay/) details building, starting, and
testing `rococo-local` and parachains connecting to it.

## How to register a parachain on the Rococo testnet

The [parachain registration process](https://docs.substrate.io/tutorials/v3/cumulus/rococo/) on the public Rococo
testnet is also outlined.
