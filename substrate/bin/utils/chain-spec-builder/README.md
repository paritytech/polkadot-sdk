# Chain Spec Builder

Substrate's chain spec builder utility.

A chain-spec is short for `chain-configuration`. See the [sc-chain-spec](https://crates.io/crates/sc-chain-spec)
for more information.

**Note**: this binary is analogous to the `build-spec` subcommand, contained in typical Substrate-based nodes.
This particular binary is capable of interacting with [sp-genesis-builder](https://docs.rs/sp-genesis-builder/latest/sp_genesis_builder/index.html)
implementation of any provided runtime allowing to build chain-spec JSON files.

See [ChainSpecBuilderCmd](https://docs.rs/staging-chain-spec-builder/6.0.0/staging_chain_spec_builder/enum.ChainSpecBuilderCmd.html)
for a list of available commands.

## Installation

```bash
cargo install staging-chain-spec-builder
```

**Note**: `chain-spec-builder` binary is published on [crates.io](https://crates.io) under
[staging-chain-spec-builder](https://crates.io/crates/staging-chain-spec-builder) due to a name conflict.

## Usage

### Generate chains-spec using default config from runtime

Query the default genesis config from the provided `runtime.wasm` and use it in the chain
spec.

```bash
chain-spec-builder create -r runtime.wasm default
```

_Note:_ [GenesisBuilder::get_preset](https://docs.rs/sp-genesis-builder/latest/sp_genesis_builder/trait.GenesisBuilder.html#method.get_preset)
runtime function is called.

### Display the runtime's default `GenesisConfig`

```bash
chain-spec-builder display-preset -r runtime.wasm
```

_Note:_ [GenesisBuilder::get_preset](https://docs.rs/sp-genesis-builder/latest/sp_genesis_builder/trait.GenesisBuilder.html#method.get_preset)
runtime function is called.

### Display the `GenesisConfig` preset with given name

```bash
chain-spec-builder display-preset -r runtime.wasm -p "staging"
```

_Note:_ [GenesisBuilder::get_preset](https://docs.rs/sp-genesis-builder/latest/sp_genesis_builder/trait.GenesisBuilder.html#method.get_preset)
runtime function is called.

### List the names of `GenesisConfig` presets provided by runtime

```bash
chain-spec-builder list-presets -r runtime.wasm
```

_Note:_ [GenesisBuilder::preset_names](https://docs.rs/sp-genesis-builder/latest/sp_genesis_builder/trait.GenesisBuilder.html#method.preset_names)
runtime function is called.

### Generate chain spec using runtime provided genesis config preset

Patch the runtime's default genesis config with the named preset provided by the runtime and generate the plain
version of chain spec:

```bash
chain-spec-builder create -r runtime.wasm named-preset "staging"
```

_Note:_ [GenesisBuilder::get_preset](https://docs.rs/sp-genesis-builder/latest/sp_genesis_builder/trait.GenesisBuilder.html#method.get_preset)
and
[GenesisBuilder::build_state](https://docs.rs/sp-genesis-builder/latest/sp_genesis_builder/trait.GenesisBuilder.html#method.build_state)
runtime functions are called.

### Generate raw storage chain spec using genesis config patch

Patch the runtime's default genesis config with provided `patch.json` and generate raw
storage (`-s`) version of chain spec:

```bash
chain-spec-builder create -s -r runtime.wasm patch patch.json
```

_Note:_ [GenesisBuilder::build_state](https://docs.rs/sp-genesis-builder/latest/sp_genesis_builder/trait.GenesisBuilder.html#method.build_state)
runtime function is called.

### Generate raw storage chain spec using full genesis config

Build the chain spec using provided full genesis config json file. No defaults will be used:

```bash
chain-spec-builder create -s -r runtime.wasm full full-genesis-config.json
```

_Note_: [GenesisBuilder::build_state](https://docs.rs/sp-genesis-builder/latest/sp_genesis_builder/trait.GenesisBuilder.html#method.build_state)
runtime function is called.

### Generate human readable chain spec using provided genesis config patch

```bash
chain-spec-builder create -r runtime.wasm patch patch.json
```

### Generate human readable chain spec using provided full genesis config

```bash
chain-spec-builder create -r runtime.wasm full full-genesis-config.json
```

### Extra tools

The `chain-spec-builder` provides also some extra utilities: `VerifyCmd`, `ConvertToRawCmd`,
`UpdateCodeCmd`.
