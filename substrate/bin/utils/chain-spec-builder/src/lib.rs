// This file is part of Substrate.

// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.

//! Substrate's chain spec builder utility.
//!
//! A chain-spec is short for `chain-configuration`. See the [`sc-chain-spec`] for more information.
//!
//! Note that this binary is analogous to the `build-spec` subcommand, contained in typical
//! substrate-based nodes. This particular binary is capable of interacting with
//! [`sp-genesis-builder`] implementation of any provided runtime allowing to build chain-spec JSON
//! files.
//!
//! See [`ChainSpecBuilderCmd`] for a list of available commands.
//!
//! ## Typical use-cases.
//! ##### Generate chains-spec using default config from runtime.
//!
//!	Query the default genesis config from the provided `runtime.wasm` and use it in the chain
//! spec.
//!	```bash
//! chain-spec-builder create -r runtime.wasm default
//! ```
//! 
//! _Note:_ [`GenesisBuilder::get_preset`][sp-genesis-builder-get-preset] runtime function is
//! called.
//!
//!
//! ##### Display the runtime's default `GenesisConfig`
//!
//! Displays the content of the runtime's default `GenesisConfig`
//! ```bash
//! chain-spec-builder display-preset -r runtime.wasm
//! ```
//! 
//! _Note:_ [`GenesisBuilder::get_preset`][sp-genesis-builder-get-preset] runtime function is called.
//!
//! ##### Display the `GenesisConfig` preset with given name
//!
//! Displays the content of the `GenesisConfig` preset for given name
//! ```bash
//! chain-spec-builder display-preset -r runtime.wasm -p "staging"
//! ```
//! 
//! _Note:_ [`GenesisBuilder::get_preset`][sp-genesis-builder-get-preset] runtime function is called.
//!
//! ##### List the names of `GenesisConfig` presets provided by runtime.
//!
//! Displays the names of the presets of `GenesisConfigs` provided by runtime.
//! ```bash
//! chain-spec-builder list-presets -r runtime.wasm
//! ```
//! 
//! _Note:_ [`GenesisBuilder::preset_names`][sp-genesis-builder-list] runtime function is called.
//!
//! ##### Generate chain spec using runtime provided genesis config preset.
//!
//! Patch the runtime's default genesis config with the named preset provided by the runtime and generate the plain
//! version of chain spec:
//! ```bash
//! chain-spec-builder create -r runtime.wasm named-preset "staging"
//! ```
//! 
//! _Note:_ [`GenesisBuilder::get_preset`][sp-genesis-builder-get-preset] and [`GenesisBuilder::build_state`][sp-genesis-builder-build] runtime functions are called.
//!
//! ##### Generate raw storage chain spec using genesis config patch.
//!
//! Patch the runtime's default genesis config with provided `patch.json` and generate raw
//! storage (`-s`) version of chain spec:
//! ```bash
//! chain-spec-builder create -s -r runtime.wasm patch patch.json
//! ```
//! 
//! _Note:_ [`GenesisBuilder::build_state`][sp-genesis-builder-build] runtime function is called.
//!
//! ##### Generate raw storage chain spec using full genesis config.
//!
//! Build the chain spec using provided full genesis config json file. No defaults will be used:
//! ```bash
//! chain-spec-builder create -s -r runtime.wasm full full-genesis-config.json
//! ```
//! 
//! _Note_: [`GenesisBuilder::build_state`][sp-genesis-builder-build] runtime function is called.
//!
//! ##### Generate human readable chain spec using provided genesis config patch.
//! ```bash
//! chain-spec-builder create -r runtime.wasm patch patch.json
//! ```
//! 
//! ##### Generate human readable chain spec using provided full genesis config.
//! ```bash
//! chain-spec-builder create -r runtime.wasm full full-genesis-config.json
//! ```
//! 
//! ##### Extra tools.
//! The `chain-spec-builder` provides also some extra utilities: [`VerifyCmd`], [`ConvertToRawCmd`],
//! [`UpdateCodeCmd`].
//!
//! [`sc-chain-spec`]: ../sc_chain_spec/index.html
//! [`node-cli`]: ../node_cli/index.html
//! [`sp-genesis-builder`]: ../sp_genesis_builder/index.html
//! [sp-genesis-builder-build]: ../sp_genesis_builder/trait.GenesisBuilder.html#method.build_state
//! [sp-genesis-builder-list]: ../sp_genesis_builder/trait.GenesisBuilder.html#method.preset_names
//! [sp-genesis-builder-get-preset]: ../sp_genesis_builder/trait.GenesisBuilder.html#method.get_preset

use std::{fs, path::PathBuf};

use clap::{Parser, Subcommand};
use sc_chain_spec::{GenericChainSpec, GenesisConfigBuilderRuntimeCaller};
use serde_json::Value;

/// A utility to easily create a chain spec definition.
#[derive(Debug, Parser)]
#[command(rename_all = "kebab-case")]
pub struct ChainSpecBuilder {
	#[command(subcommand)]
	pub command: ChainSpecBuilderCmd,
	/// The path where the chain spec should be saved.
	#[arg(long, short, default_value = "./chain_spec.json")]
	pub chain_spec_path: PathBuf,
}

#[derive(Debug, Subcommand)]
#[command(rename_all = "kebab-case")]
pub enum ChainSpecBuilderCmd {
	Create(CreateCmd),
	Verify(VerifyCmd),
	UpdateCode(UpdateCodeCmd),
	ConvertToRaw(ConvertToRawCmd),
	ListPresets(ListPresetsCmd),
	DisplayPreset(DisplayPresetCmd),
}

/// Create a new chain spec by interacting with the provided runtime wasm blob.
#[derive(Parser, Debug)]
pub struct CreateCmd {
	/// The name of chain.
	#[arg(long, short = 'n', default_value = "Custom")]
	chain_name: String,
	/// The chain id.
	#[arg(long, short = 'i', default_value = "custom")]
	chain_id: String,
	/// The path to runtime wasm blob.
	#[arg(long, short)]
	runtime_wasm_path: PathBuf,
	/// Export chainspec as raw storage.
	#[arg(long, short = 's')]
	raw_storage: bool,
	/// Verify the genesis config. This silently generates the raw storage from genesis config. Any
	/// errors will be reported.
	#[arg(long, short = 'v')]
	verify: bool,
	#[command(subcommand)]
	action: GenesisBuildAction,
}

#[derive(Subcommand, Debug, Clone)]
enum GenesisBuildAction {
	Patch(PatchCmd),
	Full(FullCmd),
	Default(DefaultCmd),
	NamedPreset(NamedPresetCmd),
}

/// Patches the runtime's default genesis config with provided patch.
#[derive(Parser, Debug, Clone)]
struct PatchCmd {
	/// The path to the runtime genesis config patch.
	patch_path: PathBuf,
}

/// Build the genesis config for runtime using provided json file. No defaults will be used.
#[derive(Parser, Debug, Clone)]
struct FullCmd {
	/// The path to the full runtime genesis config json file.
	config_path: PathBuf,
}

/// Gets the default genesis config for the runtime and uses it in ChainSpec. Please note that
/// default genesis config may not be valid. For some runtimes initial values should be added there
/// (e.g. session keys, babe epoch).
#[derive(Parser, Debug, Clone)]
struct DefaultCmd {}

/// Uses named preset provided by runtime to build the chains spec.
#[derive(Parser, Debug, Clone)]
struct NamedPresetCmd {
	preset_name: String,
}

/// Updates the code in the provided input chain spec.
///
/// The code field of the chain spec will be updated with the runtime provided in the
/// command line. This operation supports both plain and raw formats.
///
/// This command does not update chain-spec file in-place. The result of this command will be stored
/// in a file given as `-c/--chain-spec-path` command line argument.
#[derive(Parser, Debug, Clone)]
pub struct UpdateCodeCmd {
	/// Chain spec to be updated.
	///
	/// Please note that the file will not be updated in-place.
	pub input_chain_spec: PathBuf,
	/// The path to new runtime wasm blob to be stored into chain-spec.
	pub runtime_wasm_path: PathBuf,
}

/// Converts the given chain spec into the raw format.
#[derive(Parser, Debug, Clone)]
pub struct ConvertToRawCmd {
	/// Chain spec to be converted.
	pub input_chain_spec: PathBuf,
}

/// Lists available presets
#[derive(Parser, Debug, Clone)]
pub struct ListPresetsCmd {
	/// The path to runtime wasm blob.
	#[arg(long, short)]
	pub runtime_wasm_path: PathBuf,
}

/// Displays given preset
#[derive(Parser, Debug, Clone)]
pub struct DisplayPresetCmd {
	/// The path to runtime wasm blob.
	#[arg(long, short)]
	pub runtime_wasm_path: PathBuf,
	/// Preset to be displayed. If none is given default will be displayed.
	#[arg(long, short)]
	pub preset_name: Option<String>,
}

/// Verifies the provided input chain spec.
///
/// Silently checks if given input chain spec can be converted to raw. It allows to check if all
/// RuntimeGenesisConfig fields are properly initialized and if the json does not contain invalid
/// fields.
#[derive(Parser, Debug, Clone)]
pub struct VerifyCmd {
	/// Chain spec to be verified.
	pub input_chain_spec: PathBuf,
}

/// Processes `CreateCmd` and returns JSON version of `ChainSpec`.
pub fn generate_chain_spec_for_runtime(cmd: &CreateCmd) -> Result<String, String> {
	let code = fs::read(cmd.runtime_wasm_path.as_path())
		.map_err(|e| format!("wasm blob shall be readable {e}"))?;

	let builder = GenericChainSpec::<()>::builder(&code[..], Default::default())
		.with_name(&cmd.chain_name[..])
		.with_id(&cmd.chain_id[..])
		.with_chain_type(sc_chain_spec::ChainType::Live);

	let builder = match cmd.action {
		GenesisBuildAction::NamedPreset(NamedPresetCmd { ref preset_name }) =>
			builder.with_genesis_config_preset_name(&preset_name),
		GenesisBuildAction::Patch(PatchCmd { ref patch_path }) => {
			let patch = fs::read(patch_path.as_path())
				.map_err(|e| format!("patch file {patch_path:?} shall be readable: {e}"))?;
			builder.with_genesis_config_patch(serde_json::from_slice::<Value>(&patch[..]).map_err(
				|e| format!("patch file {patch_path:?} shall contain a valid json: {e}"),
			)?)
		},
		GenesisBuildAction::Full(FullCmd { ref config_path }) => {
			let config = fs::read(config_path.as_path())
				.map_err(|e| format!("config file {config_path:?} shall be readable: {e}"))?;
			builder.with_genesis_config(serde_json::from_slice::<Value>(&config[..]).map_err(
				|e| format!("config file {config_path:?} shall contain a valid json: {e}"),
			)?)
		},
		GenesisBuildAction::Default(DefaultCmd {}) => {
			let caller: GenesisConfigBuilderRuntimeCaller =
				GenesisConfigBuilderRuntimeCaller::new(&code[..]);
			let default_config = caller
				.get_default_config()
				.map_err(|e| format!("getting default config from runtime should work: {e}"))?;
			builder.with_genesis_config(default_config)
		},
	};

	let chain_spec = builder.build();

	match (cmd.verify, cmd.raw_storage) {
		(_, true) => chain_spec.as_json(true),
		(true, false) => {
			chain_spec.as_json(true)?;
			println!("Genesis config verification: OK");
			chain_spec.as_json(false)
		},
		(false, false) => chain_spec.as_json(false),
	}
}
