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

Refer to [_patch file_](#patch-file) for some details on the patch file format.

_Note:_ [`GenesisBuilder::get_preset`](https://docs.rs/sp-genesis-builder/latest/sp_genesis_builder/trait.GenesisBuilder.html#method.get_preset)
and
[`GenesisBuilder::build_state`](https://docs.rs/sp-genesis-builder/latest/sp_genesis_builder/trait.GenesisBuilder.html#method.build_state)
runtime functions are called.

### Generate raw storage chain spec using full genesis config

Build the chain spec using provided full genesis config json file. No defaults will be used:

<!-- docify::embed!("tests/test.rs", cmd_create_full_raw)-->

Refer to [_full config file_](#full-genesis-config-file) for some details on the full file format.

_Note_: [`GenesisBuilder::build_state`](https://docs.rs/sp-genesis-builder/latest/sp_genesis_builder/trait.GenesisBuilder.html#method.build_state)
runtime function is called.

### Generate human readable chain spec using provided genesis config patch

<!-- docify::embed!("tests/test.rs", cmd_create_with_patch_plain)-->

Refer to [_patch file_](#patch-file) for some details on the patch file format.

### Generate human readable chain spec using provided full genesis config

<!-- docify::embed!("tests/test.rs", cmd_create_full_plain)-->

Refer to [_full config file_](#full-genesis-config-file) for some details on the full file format.

### Manage the boot nodes of a chain spec

The `bootnodes` command group manages the boot nodes of an existing chain spec, through its `add`,
`remove` and `list` subcommands. Plain and raw chain specs are both supported.

Append boot nodes to the chain spec. The addresses that are already stored are skipped, so the
command can be safely repeated:

<!-- docify::embed!("tests/test.rs", cmd_add_bootnodes) -->

Remove boot nodes from the chain spec. The addresses that are not stored are ignored, and
`bootnodes remove <CHAIN_SPEC> --all` removes all of them at once:

<!-- docify::embed!("tests/test.rs", cmd_remove_bootnodes) -->

List the boot nodes stored in a chain spec:

<!-- docify::embed!("tests/test.rs", cmd_list_bootnodes) -->

The boot nodes of a newly created chain spec can be given with the `--bootnodes` argument of the
`create` command.

#### Interactive mode

`bootnodes add` and `bootnodes remove` prompt for the addresses to operate on when none is given in
the command line. The prompts are written to the standard error, so that the standard output stays
usable as a data channel.

`bootnodes add` displays the boot nodes that are already stored and reads the ones to append, one
per line, until an empty line is entered:

```sh
$ chain-spec-builder -c updated_chain_spec.json bootnodes add chain_spec.json
The chain spec contains 1 boot node:
  1. /dns/node-0.example.com/tcp/30333/p2p/12D3KooW9vw7UNUYQtPWK3RS8eyhjJgp4qBwbbiirYQcWLw5bCsf

Enter the boot nodes to append, one per line, and an empty line once you are done.
  2. > /dns/node-1.example.com/tcp/30333/p2p/12D3KooWAb5MyC1UJiEQJk4Hg4B2Vi3AJdqSUhTGYUqSnEqCFMFg
  3. > /dns/node-2.example.com/tcp/30333
     Invalid boot node address: Peer id is missing from the address
  3. >

Appending 1 boot node:
  2. /dns/node-1.example.com/tcp/30333/p2p/12D3KooWAb5MyC1UJiEQJk4Hg4B2Vi3AJdqSUhTGYUqSnEqCFMFg
Update the chain spec? [Y/n]:
```

`bootnodes remove` displays the boot nodes that are stored as a numbered list, and the ones to
remove are selected by number:

```sh
$ chain-spec-builder -c updated_chain_spec.json bootnodes remove chain_spec.json
The chain spec contains 3 boot nodes:
  1. /dns/node-0.example.com/tcp/30333/p2p/12D3KooW9vw7UNUYQtPWK3RS8eyhjJgp4qBwbbiirYQcWLw5bCsf
  2. /dns/node-1.example.com/tcp/30333/p2p/12D3KooWAb5MyC1UJiEQJk4Hg4B2Vi3AJdqSUhTGYUqSnEqCFMFg
  3. /ip4/198.51.100.19/tcp/30333/p2p/12D3KooWAdyiVAaeGdtBt6vn5zVetwA4z4qfm9Fi2QCSykN1wTBJ

Enter the numbers of the boot nodes to remove, e.g. `1`, `1,3` or `2-4`. Enter `all` to remove all
of them, or an empty line to cancel.
> 1,3

Removing 2 boot nodes:
  1. /dns/node-0.example.com/tcp/30333/p2p/12D3KooW9vw7UNUYQtPWK3RS8eyhjJgp4qBwbbiirYQcWLw5bCsf
  3. /ip4/198.51.100.19/tcp/30333/p2p/12D3KooWAdyiVAaeGdtBt6vn5zVetwA4z4qfm9Fi2QCSykN1wTBJ
Update the chain spec? [Y/n]:
```

The chain spec is left untouched when the prompt is cancelled or the confirmation is rejected.

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
