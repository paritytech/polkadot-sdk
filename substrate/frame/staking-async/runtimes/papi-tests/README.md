# Staking Async Test Parachain

- [Staking Async Test Parachain](#staking-async-test-parachain)
  - [Runtime Overview](#runtime-overview)
    - [Runtime Presets + Other Hacks](#runtime-presets--other-hacks)
      - [`parameter_types! { pub storage }` FTW](#parameter_types--pub-storage--ftw)
      - [Optionally Ignoring New Validators](#optionally-ignoring-new-validators)
      - [Presets](#presets)
  - [Setup](#setup)
    - [Quick Start](#quick-start)
    - [Development Workflow](#development-workflow)
  - [How To Write Tests](#how-to-write-tests)
  - [Log Formatting](#log-formatting)

This folder contains a Node+PAPI+Bun setup to:

1. run the `pallet-staking-async-runtimes` parachain and relay-chain. It uses Zombienet under the
   hood.
2. Contains integration tests, based on ZN as well, that spawns a particular test, submits
   transactions, and inspects the chain state (notably events) for verification.

The [next section](#runtime-overview) describes the runtimes and further details. To jump right into
the setup, see [Setup](#setup).

## Runtime Overview

This parachain runtime is a fake fork of the asset-hub next (created originally by Dónal). It is here
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

The above runtimes contain a number of hacks, and `genesis presets` that allow them to be quickly
adapted to a specific scenario. This section covers these topics.

#### `parameter_types! { pub storage }` FTW

In both runtimes, we extensively use `parameter_types! { pub storage }` as a shorthand for
`pallet-parameters`. This allows us to stick values that are fed into different pallets' `trait
Config`, such as `type SignedPhase` (the duration of a phase) into an orphan storage item. This has
two benefits:

1. In manual testing, we can update them on the fly via `sudo(system.set_storage)` calls.
   [This](https://paritytech.github.io/polkadot-sdk/master/src/frame_support/lib.rs.html#357) is how
   the key for these is generated.
2. We can easily tweak them based on startup.

#### Optionally Ignoring New Validators

The relay chain runtime contains an important hack. A type called `MaybeUsePreviousValidatorsElse`.
This type looks into `parameter_types! { pub storage UsePreviousValidators: bool = true }`, and

- If set to `true`, it will ignore the new validators coming from AH, and use the previous ones.
  **Why is this needed**? Because in ZN, our test relay chain is running with usually a set of known
  validators run by ZN (often alice and bob). If AH sends us back a validator set that contains a
  large new validator set, the setup will break. As seen in the next section, a number of runtime
  presets are designed to generate large validator/nominator sets to mimic the behavior of Polkadot
  and Kusama. We thereofre must use this hack in such cases.
- If set to `false`, it will use the new validator set.

#### Presets

The runtime presets are generally divided into few categories:

- `real-s` / `real-m`: imply that the relay chain will not use `MaybeUsePreviousValidatorsElse`.
  Consequently, AH will NOT generate random validators, and instead use 2 or 4 well know keys
  (alice, bob, dave, eve) as validator candidates. This setup is useful for slashing testing.
  `real-s` uses 2 validators, while `real-m` uses 4 validators. The latter is useful for testing
  disabling. Note that we need at least 2 non-disabled validators to run a parachain.
- `fake-x`: these presets instruct asset-hub to generate various number of fake validators and
  nominators. Useful for testing large elections. `MaybeUsePreviousValidatorsElse` is used in the
  relay runtime to ignore the new validators, and stick to alice and bob.

More concretely, the presets are:

- Parachain:
    - `fake-dev`: 4 page, small number of fake validators and nominators.
    - `fake-dot`: 32 pages, large number of fake validators and nominators.
    - `fake-ksm`: 16 pages, large number of fake validators and nominators.
    - `real-s`: 4 pages, alice and bob as validators, 500 fake nominators
    - `real-m`: 4 pages, alice, bob, dave, eve as validators, 2000 fake nominators.
- Relay Chain
    - `fake-s`: alice and bob as relay validators, `UsePreviousValidators` set to true. Should be
      used with all 3 `fake-x` presets in the parachain.
    - `real-s`: alice and bob as relay validators, `UsePreviousValidators` set to false. Should be
      used with `real-s` presets in the parachain.
    - `real-m`: alice, bob, dave, eve as relay validators, `UsePreviousValidators` set to false.
      Should be used with `real-m` presets in the parachain.

See `genesis_config_presets.rs`, and `fn build_state` in each runtime for more details.

## Setup

This section describes how to set up and run this code. Make sure to have the latest version of
node, bun and [just](https://github.com/casey/just) installed. Moreover, you are expected to have
`zombienet`, `polkadot`, `polkadot-parachain`, `polkadot-prepare-worker` and
`polkadot-execution-worker` in your `PATH` already. Rest of the binaries (`chain-spec-builder`) are
compiled from the sdk.

> verified compatible zombienet version: 1.3.126

### Quick Start

For first-time setup, run:

```bash
just setup
```

This single command will:

- Start the chains and generate fresh metadata
- Generate PAPI descriptors from the running chains
- Install all dependencies including the generated descriptors
- Clean up chain processes

After this, you can use regular `just install` (or `bun install`) commands without issues.

### Development Workflow

```bash
# First time setup. Compiles the binaries, spawns ZN, generates PAPI types against it.
just setup

# Regular development - install dependencies
just  install # or equivalently: bun install

# Run tests
just test

# Running specific tests
bun test tests/example.test.ts
bun test tests/unsigned-dev.ts
# and so on..
```

Further useful commands:

```bash
# Generate fresh descriptors (when chains are running)
just generate-descriptors

# Run with a specific preset
just run <preset>

# See available presets
just presets

# Clean everything and start fresh
just reset

# See all available commands
just help
```


## How To Write Tests

See `tests/example.test.ts`.

## Log Formatting

The tests, for each block which contains an event in which we are interested in, will emit a log like this:

```
verbose: [Rely#91][⛓ 2,039ms / 777 kb] Processing event: ...
verbose: [Para#71][⛓ 38ms / 852 kb][✍️ hd=0.22, xt=4.07, st=6.82, sum=11.11, cmp=9.74, time=2ms] Processing event: ...
```

- `Rely` indicates the relay chain (truncated to be 4 chars), `Para` indicates the parachain.
- Both chains' logs contain onchain (⛓) weight information, obtained from `frame-system`.
- `Para` logs contain more information from the collator/author's logs (✍️). They are:
  - `hd` header size,
  - `xt` extrinsics siz
  - `st` storage proof size
  - `sum` total PoV,
  - `cmp` compressed PoV
  - and `time`, authoring time in the collator.
