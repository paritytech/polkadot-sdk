# Chain Spec Builder

Substrate's chain spec builder utility.

A chain-spec is short for `chain-specification`. See the [`sc-chain-spec`](https://crates.io/docs.rs/sc-chain-spec/latest/sc_chain_spec)
for more information.

_Note:_ this binary is a more flexible alternative to the `build-spec` subcommand, contained in typical Substrate-based nodes.
This particular binary is capable of interacting with [`sp-genesis-builder`](https://docs.rs/sp-genesis-builder/latest/sp_genesis_builder/index.html)
implementation of any provided runtime allowing to build chain-spec JSON files.

See [`ChainSpecBuilderCmd`](https://docs.rs/staging-chain-spec-builder/6.0.0/staging_chain_spec_builder/enum.ChainSpecBuilderCmd.html)
for a list of available commands.

## Installation

```bash
cargo install staging-chain-spec-builder
```

_Note:_ `chain-spec-builder` binary is published on [crates.io](https://crates.io) under
[`staging-chain-spec-builder`](https://crates.io/crates/staging-chain-spec-builder) due to a name conflict.

## Usage

Please note that below usage is backed by integration tests. The commands' examples are wrapped
around by the `bash!(...)` macro calls.

### Generate chains-spec using default config from runtime

Query the default genesis config from the provided runtime WASM blob and use it in the chain spec.

<!-- docify::embed!("tests/test.rs", cmd_create_default) -->

_Note:_ [`GenesisBuilder::get_preset`](https://docs.rs/sp-genesis-builder/latest/sp_genesis_builder/trait.GenesisBuilder.html#method.get_preset)
runtime function is called.

### Display the runtime's default `GenesisConfig`

<!-- docify::embed!("tests/test.rs", cmd_display_default_preset) -->

_Note:_ [`GenesisBuilder::get_preset`](https://docs.rs/sp-genesis-builder/latest/sp_genesis_builder/trait.GenesisBuilder.html#method.get_preset)
runtime function is called.

### Display the `GenesisConfig` preset with given name

<!-- docify::embed!("tests/test.rs", cmd_display_preset)-->

_Note:_ [`GenesisBuilder::get_preset`](https://docs.rs/sp-genesis-builder/latest/sp_genesis_builder/trait.GenesisBuilder.html#method.get_preset)
runtime function is called.

### List the names of `GenesisConfig` presets provided by runtime

<!-- docify::embed!("tests/test.rs", cmd_list_presets)-->

_Note:_ [`GenesisBuilder::preset_names`](https://docs.rs/sp-genesis-builder/latest/sp_genesis_builder/trait.GenesisBuilder.html#method.preset_names)
runtime function is called.

### Generate chain spec using runtime provided genesis config preset

Patch the runtime's default genesis config with the named preset provided by the runtime and generate the plain
version of chain spec:

<!-- docify::embed!("tests/test.rs", cmd_create_with_named_preset)-->

_Note:_ [`GenesisBuilder::get_preset`](https://docs.rs/sp-genesis-builder/latest/sp_genesis_builder/trait.GenesisBuilder.html#method.get_preset)
runtime functions are called.

### Generate raw storage chain spec using genesis config patch

Patch the runtime's default genesis config with provided `patch.json` and generate raw
storage (`-s`) version of chain spec:

<!-- docify::embed!("tests/test.rs", cmd_create_with_patch_raw)-->

_Note:_ [`GenesisBuilder::get_preset`](https://docs.rs/sp-genesis-builder/latest/sp_genesis_builder/trait.GenesisBuilder.html#method.get_preset)
and
[`GenesisBuilder::build_state`](https://docs.rs/sp-genesis-builder/latest/sp_genesis_builder/trait.GenesisBuilder.html#method.build_state)
runtime functions are called.

### Generate raw storage chain spec using full genesis config

Build the chain spec using provided full genesis config json file. No defaults will be used:

<!-- docify::embed!("tests/test.rs", cmd_create_full_raw)-->

_Note_: [`GenesisBuilder::build_state`](https://docs.rs/sp-genesis-builder/latest/sp_genesis_builder/trait.GenesisBuilder.html#method.build_state)
runtime function is called.

### Generate human readable chain spec using provided genesis config patch

<!-- docify::embed!("tests/test.rs", cmd_create_with_patch_plain)-->

### Generate human readable chain spec using provided full genesis config

<!-- docify::embed!("tests/test.rs", cmd_create_full_plain)-->

### Extra tools

The `chain-spec-builder` provides also some extra utilities: [`VerifyCmd`](https://docs.rs/staging-chain-spec-builder/latest/staging_chain_spec_builder/struct.VerifyCmd.html),
[`ConvertToRawCmd`](https://docs.rs/staging-chain-spec-builder/latest/staging_chain_spec_builder/struct.ConvertToRawCmd.html),
[`UpdateCodeCmd`](https://docs.rs/staging-chain-spec-builder/latest/staging_chain_spec_builder/struct.UpdateCodeCmd.html).
