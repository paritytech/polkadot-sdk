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
cargo install staging-chain-spec-builder --locked
```

_Note:_ `chain-spec-builder` binary is published on [crates.io](https://crates.io) under
[`staging-chain-spec-builder`](https://crates.io/crates/staging-chain-spec-builder) due to a name conflict.

## Usage

Please note that below usage is backed by integration tests. The commands' examples are wrapped
around by the `bash!(...)` macro calls.

### Note for `CreateCmd`'s `para-id` flag

<!-- TODO: https://github.com/paritytech/polkadot-sdk/issues/8747 -->
Runtimes relying on generating the chain specification with this tool should
implement `cumulus_primitives_core::GetParachainInfo` trait, a new runtime API
designed to provide the parachain ID from the `parachain-info`
pallet. The `para-id` flag can be used though if the runtime does not implement
the runtime API, and the parachain id will be fetched by the node from chain
specification. This can be especially useful when syncing a node from a state
where the runtime does not implement `cumulus_primitives_core::GetParachainInfo`.

For reference, generating a chain specification with a `para_id` field can be
done like below:

```bash
chain-spec-builder -c "/dev/stdout" create --relay-chain "dev" --para-id 1000 -r $runtime_path named-preset "staging"
```

### Generate chains-spec using default config from runtime

Query the default genesis config from the provided runtime WASM blob and use it in the chain spec.

```rust,ignore
bash!(
	chain-spec-builder -c "/dev/stdout" create -r $runtime_path default
)
```

_Note:_ [`GenesisBuilder::get_preset`](https://docs.rs/sp-genesis-builder/latest/sp_genesis_builder/trait.GenesisBuilder.html#method.get_preset)
runtime function is called.

### Display the runtime's default `GenesisConfig`

```rust,ignore
bash!(
	chain-spec-builder display-preset -r $runtime_path
)
```

_Note:_ [`GenesisBuilder::get_preset`](https://docs.rs/sp-genesis-builder/latest/sp_genesis_builder/trait.GenesisBuilder.html#method.get_preset)
runtime function is called.

### Display the `GenesisConfig` preset with given name

```rust,ignore
fn cmd_display_preset(runtime_path: &str) -> String {
	bash!(
		chain-spec-builder display-preset -r $runtime_path -p "staging"
	)
}
```

_Note:_ [`GenesisBuilder::get_preset`](https://docs.rs/sp-genesis-builder/latest/sp_genesis_builder/trait.GenesisBuilder.html#method.get_preset)
runtime function is called.

### List the names of `GenesisConfig` presets provided by runtime

```rust,ignore
bash!(
	chain-spec-builder list-presets -r $runtime_path
)
```

_Note:_ [`GenesisBuilder::preset_names`](https://docs.rs/sp-genesis-builder/latest/sp_genesis_builder/trait.GenesisBuilder.html#method.preset_names)
runtime function is called.

### Generate chain spec using runtime provided genesis config preset

Patch the runtime's default genesis config with the named preset provided by the runtime and generate the plain
version of chain spec:

```rust,ignore
bash!(
	chain-spec-builder -c "/dev/stdout" create --relay-chain "dev" -r $runtime_path named-preset "staging"
)
```

_Note:_ [`GenesisBuilder::get_preset`](https://docs.rs/sp-genesis-builder/latest/sp_genesis_builder/trait.GenesisBuilder.html#method.get_preset)
runtime functions are called.

### Generate raw storage chain spec using genesis config patch

Patch the runtime's default genesis config with provided `patch.json` and generate raw
storage (`-s`) version of chain spec:

```rust,ignore
bash!(
	chain-spec-builder -c "/dev/stdout" create -s -r $runtime_path patch "tests/input/patch.json"
)
```

Refer to [_patch file_](#patch-file) for some details on the patch file format.

_Note:_ [`GenesisBuilder::get_preset`](https://docs.rs/sp-genesis-builder/latest/sp_genesis_builder/trait.GenesisBuilder.html#method.get_preset)
and
[`GenesisBuilder::build_state`](https://docs.rs/sp-genesis-builder/latest/sp_genesis_builder/trait.GenesisBuilder.html#method.build_state)
runtime functions are called.

### Generate raw storage chain spec using full genesis config

Build the chain spec using provided full genesis config json file. No defaults will be used:

```rust,ignore
bash!(
	chain-spec-builder -c "/dev/stdout" create -s -r $runtime_path full "tests/input/full.json"
)
```

Refer to [_full config file_](#full-genesis-config-file) for some details on the full file format.

_Note_: [`GenesisBuilder::build_state`](https://docs.rs/sp-genesis-builder/latest/sp_genesis_builder/trait.GenesisBuilder.html#method.build_state)
runtime function is called.

### Generate human readable chain spec using provided genesis config patch

```rust,ignore
bash!(
	chain-spec-builder -c "/dev/stdout" create -r $runtime_path patch "tests/input/patch.json"
)
```

Refer to [_patch file_](#patch-file) for some details on the patch file format.

### Generate human readable chain spec using provided full genesis config

```rust,ignore
bash!(
	chain-spec-builder -c "/dev/stdout" create -r $runtime_path full "tests/input/full.json"
)
```

Refer to [_full config file_](#full-genesis-config-file) for some details on the full file format.

### Manage the boot nodes of a chain spec

The `bootnodes` command group manages the boot nodes of an existing chain spec, through its `add`,
`remove` and `list` subcommands. Plain and raw chain specs are both supported.

Append boot nodes to the chain spec. The addresses that are already stored are skipped, so the
command can be safely repeated:

```rust,ignore
bash!(
	chain-spec-builder -c "/dev/stdout" bootnodes add $chain_spec_path "/dns/node-0.example.com/tcp/30333/p2p/12D3KooW9vw7UNUYQtPWK3RS8eyhjJgp4qBwbbiirYQcWLw5bCsf"
)
```

Remove boot nodes from the chain spec. The addresses that are not stored are ignored, and
`bootnodes remove <CHAIN_SPEC> --all` removes all of them at once:

```rust,ignore
bash!(
	chain-spec-builder -c "/dev/stdout" bootnodes remove $chain_spec_path "/dns/node-0.example.com/tcp/30333/p2p/12D3KooW9vw7UNUYQtPWK3RS8eyhjJgp4qBwbbiirYQcWLw5bCsf"
)
```

List the boot nodes stored in a chain spec:

```rust,ignore
bash!(
	chain-spec-builder bootnodes list $chain_spec_path
)
```

The boot nodes of a newly created chain spec can be given with the `--bootnodes` argument of the
`create` command.

## Patch and full genesis config files

This section provides details on the files that can be used with `create patch` or `create full` subcommands.

### Patch file

The patch file for genesis config contains the key-value pairs valid for given runtime, that needs to be customized,
 e.g:

```ignore
{
 "balances": {
  "balances": [
   [
    "5FHneW46xGXgs5mUiveU4sbTyGBzmstUspZC92UhjJM694ty",
       1000000000000000
   ],
   [
    "5FLSigC9HGRKVhB9FiEo4Y3koPsNmBmLJbpXg2mp1hXcS59Y",
        1000000000000000
   ],
   [
    "5CcjiSgG2KLuKAsqkE2Nak1S2FbAcMr5SxRASUuwR3zSNV2b",
       5000000000000000
   ]
  ]
 },
 "sudo": {
  "key": "5Ff3iXP75ruzroPWRP2FYBHWnmGGBSb63857BgnzCoXNxfPo"
 }
}
```

The rest of genesis config keys will be initialized with default values.

### Full genesis config file

The full genesis config file must contain values for _all_ the keys present in the genesis config for given runtime. The
format of the file is similar to patch format. Example is not provided here as it heavily depends on the runtime.

### Extra tools

The `chain-spec-builder` provides also some extra utilities: [`VerifyCmd`](https://docs.rs/staging-chain-spec-builder/latest/staging_chain_spec_builder/struct.VerifyCmd.html),
[`ConvertToRawCmd`](https://docs.rs/staging-chain-spec-builder/latest/staging_chain_spec_builder/struct.ConvertToRawCmd.html),
[`UpdateCodeCmd`](https://docs.rs/staging-chain-spec-builder/latest/staging_chain_spec_builder/struct.UpdateCodeCmd.html).
