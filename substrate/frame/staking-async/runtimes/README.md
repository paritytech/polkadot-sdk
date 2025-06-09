# Staking Async Parachain

## Overview

This parachain runtime is a fake fork of the asset-hub next (created original by Donál). It is here
to test the async-staking pallet in a real environment.

This parachain contains:

- `pallet-staking-async`
- `pallet-staking-async-rc-client`
- `pallet-election-provider-multi-block` and family
- aux staking pallets `pallet-nomination-pools`, `pallet-fast-unstake`, `pallet-bags-list`, and
  `pallet-delegated-staking`.

All of the above are means to stake and select validators for the RELAY-CHAIN, which is eventually
communicated to it via the `pallet-staking-async-rc-client` pallet.

A lot more is in the runtime, and can be eventually removed.

Note that the parachain runtime also contains a `pallet-session` that works with
`pallet-collator-selection` for the PARACHAIN block author selection.

The counterpart `rc` runtime is a relay chain that is meant to host the parachain. It contains:

- `pallet-staking-async-ah-client`
- `pallet-session`
- `pallet-authorship`
- And all of the consensus pallets that feed the authority set from the session, such as
  aura/babe/grandpa/beefy and so on.

## Run

To run this, a one-click script is provided:

```
bash build-and-run-zn.sh
```

This script will generate chain-specs for both runtimes, and run them with zombie-net.

> Make sure you have all Polkadot binaries (`polkadot`, `polkadot-execution-worker` and
> `polkadot-prepare-worker`) and `polkadot-parachain` installed in your PATH. You can usually
> download them from the Polkadot-sdk release page.

You also need `chain-spec-builder`, but the script builds that and uses a fresh one.

## Chain-spec presets

We have tried to move as much of the configuration as possible to different chain-specifications, so
that manually tweaking the code is not needed.

The parachain comes with 3 main chain-spec presets.

- `development`: 100 validator, 2000 nominators, all 2000 nominators in the snapshot, 10 validator
  to be elected, 4 pages
- `dot_size`: 2000 validator, 25_000 nominators, 22_500 nominators in the snapshot, 500 validator to
  be elected, 32 pages
- `ksm_size`: 4000 validator, 20_000 nominators, 12_500 nominators in the snapshot, 1000 validator
  to be elected, 16 pages

Both when running the benchmarks (`bench.sh`) and the chain (`build-and-run-zn.sh`), you can specify
the chain-spec preset. See each file for more info as to how.
