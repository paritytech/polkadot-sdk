# Staking Async Test Parachain

- [Staking Async Test Parachain](#staking-async-test-parachain)
	- [Runtime Overview](#runtime-overview)
		- [Runtime Presets + Other Hacks](#runtime-presets--other-hacks)
			- [`parameter_types! { pub storage }` FTW](#parameter_types--pub-storage--ftw)
			- [Optionally Ignoring New Validators](#optionally-ignoring-new-validators)
			- [Presets](#presets)
	- [Running](#running)
		- [PAPI Typegen](#papi-typegen)
		- [Runing a Preset](#runing-a-preset)
	- [Test](#test)
	- [How To Write Tests](#how-to-write-tests)

This folder contains a Node+PAPI+Bun setup to:

1. run the `pallet-staking-async-runtimes` parachain and relay-chain. It uses Zombienet under the hood.
2. Contains integration tests, based on ZN as well, that spawns a particular test, submits transactions, and inspects the chain state (notably events) for verification.

## Runtime Overview

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

### Runtime Presets + Other Hacks

The above runtimes contain a number of hacks, and `genesis presets` that allow them to be quickly adapted to a specific scenario. This section covers these topics.

#### `parameter_types! { pub storage }` FTW

In both runtimes, we extensively use `parameter_types! { pub storage }` as a shorthand for `pallet-parameters`. This allows us to stick values that are fed into different pallets' `trait Config`, such as `type SignedPhase` (the duration of a phase) into an orphan storage item. This has two benefits:

1. In manual testing, we can update them on the fly via `sudo(system.set_storage)` calls. [This](https://paritytech.github.io/polkadot-sdk/master/src/frame_support/lib.rs.html#357) is how the key for these is generated.
2. We can easily tweak them based on startup.

#### Optionally Ignoring New Validators

The rely chain runtime contains an important hack. A type called `MaybeUsePreviousValidatorsElse`. This type looks into `parameter_types! { pub storage UsePreviousValidators: bool = true }`, and

* If set to `true`, it will ignore the new validators coming from AH, and use the previous ones. **Why is this needed**? Because in ZN, our test relay chain is running with usually a set of known validators run by ZN (often alice and bob). If AH sends us back a validator set that contains a large new validator set, the setup will break. As seen in the next section, a number of runtime presets are designed to generate large validator/nominator sets to mimic the behavior of Polkadot and Kusama. We thereofre must use this hack in such cases.
* If set to `false`, it will use the new validator set.

#### Presets

The runtime presets are generally divided into few categories:

* `real-s` / `real-m`: imply that the realy chain will not use `MaybeUsePreviousValidatorsElse`. Consequently, AH will NOT generate random validators, and instead use 2 or 4 well know keys (alice, bob, dave, eve) as validator candidates. This setup is useful for slashing testing. `real-s` uses 2 validators, while `real-m` uses 4 validators. The latter is useful for testing disabling. Note that we need at least 2 non-disabled validators to run a parachain.
* `fake-x`: these presets instruct asset-hub to generate various number of fake validators and nominators. Useful for testing large elections. `MaybeUsePreviousValidatorsElse` is used in the relay runtime to ignore the new validators, and stick to alice and bob.

More concretely, the presets are:

* Parachain:
  * `fake-dev`: 4 page, small number of fake validators and nominators.
  * `fake-dot`: 32 pages, large number of fake validators and nominators.
  * `fake-ksm`: 16 pages, large number of fake validators and nominators.
  * `real-s`: 4 pages, alice and bob as validators, 500 fake nominators
  * `real-m`: 4 pages, alice, bob, dave, eve as validators, 2000 fake nominators.
* Relay Chain
  * `fake-s`: alice and bob as relay validators, `UsePreviousValidators` set to true. Should be used with all 3 `fake-x` presets in the parachain.
  * `real-s`: alice and bob as relay validators, `UsePreviousValidators` set to false. Should be used with `real-s` presets in the parachain.
  * `real-m`: alice, bob, dave, eve as relay validators, `UsePreviousValidators` set to false. Should be used with `real-m` presets in the parachain.

See `genesis_config_presets.rs`, and `fn build_state` in each runtime for more details.

## Running

This section describes how to run this code. Make sure to have the latest version of node+bun installed. Moreover, you are expected to have `zombienet`, `polkadot`, `polkadot-parachain`, `polkadot-prepare-worker` and `polkadot-execution-worker` in your `PATH` already. Rest of the binaries (`chain-spec-builder`) are compiled from the sdk

### PAPI Typegen

First, install the dependencies:

```bash
bun install
```

First, we will need to instruct PAPI to generate the right types for our runtimes. Use the lagacy `build-and-run-zn.sh` setup to run both chains. Then run:

```bash
npx papi add -w ws://localhost:9945 rc
npx papi add -w ws://localhost:9946 parachain
npx papi
```

Then you should be ready to go.

### Runing a Preset

> This is merely a wrapper around generating chain-specs, compiling runtimes, and running ZN. You could do all of it manually as well. You can also use `build-and-run-zn.sh`.

```bash
bun run src/index.ts run --para-preset <preset>
```

You only provide the parachain preset, one of the above. The relay chain preset is automatically selected. The correct ZN file to use is also selected based on the preset.

## Test

```bash
bun run test
```

## How To Write Tests
