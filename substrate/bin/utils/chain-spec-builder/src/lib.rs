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
#![doc = include_str!("../README.md")]
#[cfg(feature = "generate-readme")]
docify::compile_markdown!("README.docify.md", "README.md");

use clap::{Parser, Subcommand};
use sc_chain_spec::{
	json_patch, set_code_substitute_in_json_chain_spec, update_code_in_json_chain_spec, ChainType,
	GenericChainSpec, GenesisConfigBuilderRuntimeCaller, MultiaddrWithPeerId,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
	borrow::Cow,
	fs,
	io::{self, BufRead, Write},
	path::{Path, PathBuf},
	str::FromStr,
};

/// A utility to easily create a chain spec definition.
#[derive(Debug, Parser)]
#[command(rename_all = "kebab-case", version, about)]
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
	AddCodeSubstitute(AddCodeSubstituteCmd),
	/// Manages the boot nodes of an existing chain spec.
	#[command(subcommand, alias = "bootnode")]
	Bootnodes(BootnodesCmd),
}

/// Manages the boot nodes stored in the `bootNodes` field of an existing chain spec.
///
/// All these operations support both plain and raw formats. The `add` and `remove` commands take
/// the addresses to operate on from the command line, and prompt for them interactively when none
/// is given.
#[derive(Debug, Subcommand)]
pub enum BootnodesCmd {
	Add(AddBootnodesCmd),
	Remove(RemoveBootnodesCmd),
	List(ListBootnodesCmd),
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
	/// The chain type.
	#[arg(value_enum, short = 't', default_value = "live")]
	chain_type: ChainType,
	/// The para ID for your chain.
	#[arg(long, value_enum, short = 'p', requires = "relay_chain")]
	pub para_id: Option<u32>,
	/// The relay chain you wish to connect to.
	#[arg(long, value_enum, short = 'c')]
	pub relay_chain: Option<String>,
	/// The path to runtime wasm blob.
	#[arg(long, short, alias = "runtime-wasm-path")]
	runtime: PathBuf,
	/// Export chainspec as raw storage.
	#[arg(long, short = 's')]
	raw_storage: bool,
	/// Verify the genesis config. This silently generates the raw storage from genesis config. Any
	/// errors will be reported.
	#[arg(long, short = 'v')]
	verify: bool,
	/// Chain properties in `KEY=VALUE` format.
	///
	/// Multiple `KEY=VALUE` entries can be specified and separated by a comma.
	///
	/// Example: `--properties tokenSymbol=UNIT,tokenDecimals=12,ss58Format=42,isEthereum=false`
	/// Or: `--properties tokenSymbol=UNIT --properties tokenDecimals=12 --properties ss58Format=42
	/// --properties=isEthereum=false`
	///
	/// The first uses comma as separation and the second passes the argument multiple times. Both
	/// styles can also be mixed.
	#[arg(long, default_value = "tokenSymbol=UNIT,tokenDecimals=12")]
	pub properties: Vec<String>,
	/// The boot nodes to be stored in the chain spec.
	///
	/// Each boot node is a multiaddress that contains the peer id of the node. Multiple addresses
	/// can be separated by a comma, or the argument can be passed multiple times.
	///
	/// Example: `--bootnodes /dns/node-0.example.com/tcp/30333/p2p/12D3KooW...`
	#[arg(long, short = 'b', value_delimiter = ',')]
	pub bootnodes: Vec<MultiaddrWithPeerId>,
	#[command(subcommand)]
	action: GenesisBuildAction,

	/// Allows to provide the runtime code blob, instead of reading it from the provided file path.
	#[clap(skip)]
	code: Option<Cow<'static, [u8]>>,
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
	#[arg(alias = "runtime-wasm-path")]
	pub runtime: PathBuf,
}

/// Add a code substitute in the chain spec.
///
/// The `codeSubstitute` object of the chain spec will be updated with the block height as key and
/// runtime code as value. This operation supports both plain and raw formats. The `codeSubstitute`
/// field instructs the node to use the provided runtime code at the given block height. This is
/// useful when the chain can not progress on its own due to a bug that prevents block-building.
///
/// Note: For parachains, the validation function on the relaychain needs to be adjusted too,
/// otherwise blocks built using the substituted parachain runtime will be rejected.
#[derive(Parser, Debug, Clone)]
pub struct AddCodeSubstituteCmd {
	/// Chain spec to be updated.
	pub input_chain_spec: PathBuf,
	/// New runtime wasm blob that should replace the existing code.
	#[arg(alias = "runtime-wasm-path")]
	pub runtime: PathBuf,
	/// The block height at which the code should be substituted.
	pub block_height: u64,
}

/// Appends boot nodes to the provided input chain spec.
///
/// The addresses are appended to the end of the `bootNodes` field of the chain spec, in the given
/// order. Addresses that are already present are skipped, so the command can be safely repeated.
///
/// When no address is given on the command line the existing boot nodes are displayed and the
/// addresses to append are read from the standard input, one per line, until an empty line is
/// entered.
///
/// This command does not update chain-spec file in-place. The result of this command will be stored
/// in a file given as `-c/--chain-spec-path` command line argument.
#[derive(Parser, Debug, Clone)]
pub struct AddBootnodesCmd {
	/// Chain spec to be updated.
	///
	/// Please note that the file will not be updated in-place.
	pub input_chain_spec: PathBuf,
	/// The boot nodes to be appended. Read from the standard input when omitted.
	///
	/// Each boot node is a multiaddress that contains the peer id of the node, e.g.
	/// `/dns/node-0.example.com/tcp/30333/p2p/12D3KooW...`.
	#[arg(num_args = 1..)]
	pub bootnodes: Vec<MultiaddrWithPeerId>,
}

/// Removes boot nodes from the provided input chain spec.
///
/// The addresses given in the command line are removed from the `bootNodes` field of the chain
/// spec. Addresses that are not present are ignored, so the command can be safely repeated.
///
/// When no address is given on the command line the existing boot nodes are displayed as a numbered
/// list and the ones to remove are selected by number on the standard input.
///
/// This command does not update chain-spec file in-place. The result of this command will be stored
/// in a file given as `-c/--chain-spec-path` command line argument.
#[derive(Parser, Debug, Clone)]
pub struct RemoveBootnodesCmd {
	/// Chain spec to be updated.
	///
	/// Please note that the file will not be updated in-place.
	pub input_chain_spec: PathBuf,
	/// The boot nodes to be removed. Selected on the standard input when omitted.
	///
	/// Each boot node is a multiaddress that contains the peer id of the node, e.g.
	/// `/dns/node-0.example.com/tcp/30333/p2p/12D3KooW...`.
	#[arg(conflicts_with = "all", num_args = 1..)]
	pub bootnodes: Vec<MultiaddrWithPeerId>,
	/// Remove all the boot nodes of the chain spec, without prompting.
	#[arg(long)]
	pub all: bool,
}

/// Lists the boot nodes of the provided input chain spec.
#[derive(Parser, Debug, Clone)]
pub struct ListBootnodesCmd {
	/// Chain spec to be inspected.
	pub input_chain_spec: PathBuf,
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
	#[arg(long, short, alias = "runtime-wasm-path")]
	pub runtime: PathBuf,
}

/// Displays given preset
#[derive(Parser, Debug, Clone)]
pub struct DisplayPresetCmd {
	/// The path to runtime wasm blob.
	#[arg(long, short, alias = "runtime-wasm-path")]
	pub runtime: PathBuf,
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

#[derive(Deserialize, Serialize, Clone)]
pub struct ParachainExtension {
	/// The relay chain of the Parachain.
	pub relay_chain: String,
	/// The id of the Parachain.
	pub para_id: Option<u32>,
}

type ChainSpec = GenericChainSpec<()>;

impl ChainSpecBuilder {
	/// Executes the internal command.
	pub fn run(&self) -> Result<(), String> {
		let chain_spec_path = self.chain_spec_path.to_path_buf();

		match &self.command {
			ChainSpecBuilderCmd::Create(cmd) => {
				let chain_spec_json = generate_chain_spec_for_runtime(&cmd)?;
				fs::write(chain_spec_path, chain_spec_json).map_err(|err| err.to_string())?;
			},
			ChainSpecBuilderCmd::UpdateCode(UpdateCodeCmd {
				ref input_chain_spec,
				ref runtime,
			}) => {
				let mut chain_spec_json = extract_chain_spec_json(input_chain_spec.as_path())?;

				update_code_in_json_chain_spec(
					&mut chain_spec_json,
					&fs::read(runtime.as_path())
						.map_err(|e| format!("Wasm blob file could not be read: {e}"))?[..],
				);

				let chain_spec_json = serde_json::to_string_pretty(&chain_spec_json)
					.map_err(|e| format!("to pretty failed: {e}"))?;
				fs::write(chain_spec_path, chain_spec_json).map_err(|err| err.to_string())?;
			},
			ChainSpecBuilderCmd::AddCodeSubstitute(AddCodeSubstituteCmd {
				ref input_chain_spec,
				ref runtime,
				block_height,
			}) => {
				let mut chain_spec_json = extract_chain_spec_json(input_chain_spec.as_path())?;

				set_code_substitute_in_json_chain_spec(
					&mut chain_spec_json,
					&fs::read(runtime.as_path())
						.map_err(|e| format!("Wasm blob file could not be read: {e}"))?[..],
					*block_height,
				);
				let chain_spec_json = serde_json::to_string_pretty(&chain_spec_json)
					.map_err(|e| format!("to pretty failed: {e}"))?;
				fs::write(chain_spec_path, chain_spec_json).map_err(|err| err.to_string())?;
			},
			ChainSpecBuilderCmd::Bootnodes(BootnodesCmd::Add(AddBootnodesCmd {
				ref input_chain_spec,
				ref bootnodes,
			})) => {
				let mut chain_spec_json = extract_chain_spec_json(input_chain_spec.as_path())?;
				let existing_bootnodes = extract_bootnodes(&chain_spec_json)?;
				let bootnodes_to_add = if bootnodes.is_empty() {
					match prompt_bootnodes_to_add(
						&existing_bootnodes,
						&mut io::stdin().lock(),
						&mut io::stderr(),
					)
					.map_err(|e| format!("Failed to prompt for the boot nodes: {e}"))?
					{
						Some(bootnodes) => bootnodes,
						None => return Ok(()),
					}
				} else {
					bootnodes.clone()
				};

				let mut updated_bootnodes = existing_bootnodes;
				for bootnode in bootnodes_to_add {
					if !updated_bootnodes.contains(&bootnode) {
						updated_bootnodes.push(bootnode);
					}
				}

				set_bootnodes(&mut chain_spec_json, updated_bootnodes)?;
				write_chain_spec_json(&chain_spec_json, chain_spec_path.as_path())?;
			},
			ChainSpecBuilderCmd::Bootnodes(BootnodesCmd::Remove(RemoveBootnodesCmd {
				ref input_chain_spec,
				ref bootnodes,
				all,
			})) => {
				let mut chain_spec_json = extract_chain_spec_json(input_chain_spec.as_path())?;
				// When all the boot nodes are removed the existing ones are not inspected, which
				// allows to fix a chain spec containing an invalid address.
				let remaining_bootnodes = if *all {
					Vec::new()
				} else {
					let existing_bootnodes = extract_bootnodes(&chain_spec_json)?;
					let bootnodes_to_remove = if bootnodes.is_empty() {
						match prompt_bootnodes_to_remove(
							&existing_bootnodes,
							&mut io::stdin().lock(),
							&mut io::stderr(),
						)
						.map_err(|e| format!("Failed to prompt for the boot nodes: {e}"))?
						{
							Some(bootnodes) => bootnodes,
							None => return Ok(()),
						}
					} else {
						bootnodes.clone()
					};

					existing_bootnodes
						.into_iter()
						.filter(|bootnode| !bootnodes_to_remove.contains(bootnode))
						.collect()
				};

				set_bootnodes(&mut chain_spec_json, remaining_bootnodes)?;
				write_chain_spec_json(&chain_spec_json, chain_spec_path.as_path())?;
			},
			ChainSpecBuilderCmd::Bootnodes(BootnodesCmd::List(ListBootnodesCmd {
				ref input_chain_spec,
			})) => {
				let chain_spec_json = extract_chain_spec_json(input_chain_spec.as_path())?;
				let bootnodes = extract_bootnodes(&chain_spec_json)?;
				println!("{}", serde_json::json!({ "bootNodes": bootnodes }).to_string());
			},
			ChainSpecBuilderCmd::ConvertToRaw(ConvertToRawCmd { ref input_chain_spec }) => {
				let chain_spec = ChainSpec::from_json_file(input_chain_spec.clone())?;

				let mut genesis_json =
					serde_json::from_str::<serde_json::Value>(&chain_spec.as_json(true)?)
						.map_err(|e| format!("Conversion to json failed: {e}"))?;

				// We want to extract only raw genesis ("genesis::raw" key), and apply it as a patch
				// for the original json file.
				genesis_json.as_object_mut().map(|map| {
					map.retain(|key, _| key == "genesis");
				});

				let mut org_chain_spec_json = extract_chain_spec_json(input_chain_spec.as_path())?;

				// The original plain genesis ("genesis::runtimeGenesis") is no longer needed, so
				// just remove it:
				org_chain_spec_json
					.get_mut("genesis")
					.and_then(|genesis| genesis.as_object_mut())
					.and_then(|genesis| genesis.remove("runtimeGenesis"));
				json_patch::merge(&mut org_chain_spec_json, genesis_json);

				let chain_spec_json = serde_json::to_string_pretty(&org_chain_spec_json)
					.map_err(|e| format!("Conversion to pretty failed: {e}"))?;
				fs::write(chain_spec_path, chain_spec_json).map_err(|err| err.to_string())?;
			},
			ChainSpecBuilderCmd::Verify(VerifyCmd { ref input_chain_spec }) => {
				let chain_spec = ChainSpec::from_json_file(input_chain_spec.clone())?;
				serde_json::from_str::<serde_json::Value>(&chain_spec.as_json(true)?)
					.map_err(|e| format!("Conversion to json failed: {e}"))?;
			},
			ChainSpecBuilderCmd::ListPresets(ListPresetsCmd { runtime }) => {
				let code = fs::read(runtime.as_path())
					.map_err(|e| format!("wasm blob shall be readable {e}"))?;
				let caller: GenesisConfigBuilderRuntimeCaller =
					GenesisConfigBuilderRuntimeCaller::new(&code[..]);
				let presets = caller
					.preset_names()
					.map_err(|e| format!("getting default config from runtime should work: {e}"))?;
				println!("{}", serde_json::json!({"presets":presets}).to_string());
			},
			ChainSpecBuilderCmd::DisplayPreset(DisplayPresetCmd { runtime, preset_name }) => {
				let code = fs::read(runtime.as_path())
					.map_err(|e| format!("wasm blob shall be readable {e}"))?;
				let caller: GenesisConfigBuilderRuntimeCaller =
					GenesisConfigBuilderRuntimeCaller::new(&code[..]);
				let preset = caller
					.get_named_preset(preset_name.as_ref())
					.map_err(|e| format!("getting default config from runtime should work: {e}"))?;
				println!("{preset}");
			},
		}
		Ok(())
	}

	/// Sets the code used by [`CreateCmd`]
	///
	/// The file pointed by `CreateCmd::runtime` field will not be read. Provided blob will used
	/// instead for chain spec generation.
	pub fn set_create_cmd_runtime_code(&mut self, code: Cow<'static, [u8]>) {
		match &mut self.command {
			ChainSpecBuilderCmd::Create(cmd) => {
				cmd.code = Some(code);
			},
			_ => {
				panic!("Overwriting code blob is only supported for CreateCmd");
			},
		};
	}
}

fn process_action<T: Serialize + Clone + Sync + 'static>(
	cmd: &CreateCmd,
	code: &[u8],
	builder: sc_chain_spec::ChainSpecBuilder<T>,
) -> Result<String, String> {
	let builder = match cmd.action {
		GenesisBuildAction::NamedPreset(NamedPresetCmd { ref preset_name }) => {
			builder.with_genesis_config_preset_name(&preset_name)
		},
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
				GenesisConfigBuilderRuntimeCaller::new(&code);
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

impl CreateCmd {
	/// Returns the associated runtime code.
	///
	/// If the code blob was previously set, returns it. Otherwise reads the file.
	fn get_runtime_code(&self) -> Result<Cow<'static, [u8]>, String> {
		Ok(if let Some(code) = self.code.clone() {
			code
		} else {
			fs::read(self.runtime.as_path())
				.map_err(|e| format!("wasm blob shall be readable {e}"))?
				.into()
		})
	}
}

/// Parses chain properties passed as a comma-separated KEY=VALUE pairs.
fn parse_properties(raw: &String, props: &mut sc_chain_spec::Properties) -> Result<(), String> {
	for pair in raw.split(',') {
		let mut iter = pair.splitn(2, '=');
		let key = iter
			.next()
			.ok_or_else(|| format!("Invalid chain property key: {pair}"))?
			.trim()
			.to_owned();
		let value_str = iter
			.next()
			.ok_or_else(|| format!("Invalid chain property value for key: {key}"))?
			.trim();

		// Try to parse as bool, number, or fallback to String
		let value = match value_str.parse::<bool>() {
			Ok(b) => Value::Bool(b),
			Err(_) => match value_str.parse::<u32>() {
				Ok(i) => Value::Number(i.into()),
				Err(_) => Value::String(value_str.to_string()),
			},
		};

		props.insert(key, value);
	}
	Ok(())
}

/// Processes `CreateCmd` and returns string representation of JSON version of `ChainSpec`.
pub fn generate_chain_spec_for_runtime(cmd: &CreateCmd) -> Result<String, String> {
	let code = cmd.get_runtime_code()?;

	let chain_type = &cmd.chain_type;

	let mut properties = sc_chain_spec::Properties::new();
	for raw in &cmd.properties {
		parse_properties(raw, &mut properties)?;
	}

	let builder = ChainSpec::builder(&code[..], Default::default())
		.with_name(&cmd.chain_name[..])
		.with_id(&cmd.chain_id[..])
		.with_properties(properties)
		.with_chain_type(chain_type.clone())
		.with_boot_nodes(cmd.bootnodes.clone());

	let chain_spec_json_string = process_action(&cmd, &code[..], builder)?;
	let parachain_properties = cmd.relay_chain.as_ref().map(|rc| {
		cmd.para_id
			.map(|para_id| {
				serde_json::json!({
					"relay_chain": rc,
					"para_id": para_id,
				})
			})
			.unwrap_or(serde_json::json!({
				"relay_chain": rc,
			}))
	});

	let chain_spec = parachain_properties
		.map(|props| {
			let chain_spec_json_blob = serde_json::from_str(chain_spec_json_string.as_str())
				.map_err(|e| format!("deserialization a json failed {e}"));
			chain_spec_json_blob.and_then(|mut cs| {
				json_patch::merge(&mut cs, props);
				serde_json::to_string_pretty(&cs).map_err(|e| format!("to pretty failed: {e}"))
			})
		})
		.unwrap_or(Ok(chain_spec_json_string));
	chain_spec
}

/// The key of the chain spec json field holding the boot nodes.
const BOOT_NODES_KEY: &str = "bootNodes";

/// Extracts the boot nodes stored in the given chain spec json.
///
/// An empty list is returned if the chain spec does not contain the boot nodes field.
fn extract_bootnodes(chain_spec_json: &Value) -> Result<Vec<MultiaddrWithPeerId>, String> {
	let Some(bootnodes) = chain_spec_json.get(BOOT_NODES_KEY) else { return Ok(Vec::new()) };

	serde_json::from_value(bootnodes.clone())
		.map_err(|e| format!("`{BOOT_NODES_KEY}` field of the chain spec is invalid: {e}"))
}

/// Stores the given boot nodes in the given chain spec json, replacing the existing ones.
fn set_bootnodes(
	chain_spec_json: &mut Value,
	bootnodes: Vec<MultiaddrWithPeerId>,
) -> Result<(), String> {
	let bootnodes = serde_json::to_value(bootnodes)
		.map_err(|e| format!("Conversion of the boot nodes to json failed: {e}"))?;

	chain_spec_json
		.as_object_mut()
		.ok_or_else(|| "Provided chain spec is not a json object".to_string())?
		.insert(BOOT_NODES_KEY.to_string(), bootnodes);
	Ok(())
}

/// Stores the given chain spec json in the file at the given path.
fn write_chain_spec_json(chain_spec_json: &Value, chain_spec_path: &Path) -> Result<(), String> {
	let chain_spec_json = serde_json::to_string_pretty(chain_spec_json)
		.map_err(|e| format!("to pretty failed: {e}"))?;
	fs::write(chain_spec_path, chain_spec_json).map_err(|err| err.to_string())
}

/// Formats the given number of boot nodes, e.g. `1 boot node` or `2 boot nodes`.
fn count_bootnodes(count: usize) -> String {
	format!("{count} boot node{}", if count == 1 { "" } else { "s" })
}

/// Displays the given boot nodes, each one preceded by the number it is paired with.
fn display_bootnodes<'a>(
	bootnodes: impl IntoIterator<Item = (usize, &'a MultiaddrWithPeerId)>,
	output: &mut impl Write,
) -> io::Result<()> {
	for (number, bootnode) in bootnodes {
		writeln!(output, "  {number}. {bootnode}")?;
	}
	Ok(())
}

/// Pairs the given boot nodes with the 1-based numbers they are displayed with.
fn number_bootnodes(
	bootnodes: &[MultiaddrWithPeerId],
) -> impl Iterator<Item = (usize, &MultiaddrWithPeerId)> {
	bootnodes.iter().enumerate().map(|(index, bootnode)| (index + 1, bootnode))
}

/// Reads a line from the given input, returning `None` at the end of the input.
fn read_line(input: &mut impl BufRead) -> io::Result<Option<String>> {
	let mut line = String::new();
	if input.read_line(&mut line)? == 0 {
		return Ok(None);
	}
	Ok(Some(line))
}

/// Prompts for the confirmation of an operation.
///
/// The operation is confirmed by an empty answer, and the end of the input is treated as one.
fn prompt_confirmation(
	question: &str,
	input: &mut impl BufRead,
	output: &mut impl Write,
) -> io::Result<bool> {
	write!(output, "{question} [Y/n]: ")?;
	output.flush()?;

	let Some(answer) = read_line(input)? else { return Ok(true) };
	Ok(!matches!(answer.trim().chars().next(), Some('n') | Some('N')))
}

/// Prompts for the boot nodes to append to the given existing ones.
///
/// The addresses are read one per line until an empty line is entered. Invalid and already stored
/// addresses are reported and the prompt is repeated, so that a typo does not discard the
/// addresses that were entered before it.
///
/// `None` is returned when the operation is discarded, in which case the chain spec shall be left
/// untouched.
fn prompt_bootnodes_to_add(
	existing_bootnodes: &[MultiaddrWithPeerId],
	input: &mut impl BufRead,
	output: &mut impl Write,
) -> io::Result<Option<Vec<MultiaddrWithPeerId>>> {
	if existing_bootnodes.is_empty() {
		writeln!(output, "The chain spec contains no boot nodes.")?;
	} else {
		writeln!(output, "The chain spec contains {}:", count_bootnodes(existing_bootnodes.len()))?;
		display_bootnodes(number_bootnodes(existing_bootnodes), output)?;
	}
	writeln!(
		output,
		"\nEnter the boot nodes to append, one per line, and an empty line once you are done."
	)?;

	let mut bootnodes = Vec::new();
	loop {
		write!(output, "  {}. > ", existing_bootnodes.len() + bootnodes.len() + 1)?;
		output.flush()?;

		let Some(line) = read_line(input)? else { break };
		let line = line.trim();
		if line.is_empty() {
			break;
		}

		match MultiaddrWithPeerId::from_str(line) {
			Ok(bootnode) if existing_bootnodes.contains(&bootnode) => {
				writeln!(output, "     Already stored in the chain spec, skipped.")?
			},
			Ok(bootnode) if bootnodes.contains(&bootnode) => {
				writeln!(output, "     Already entered, skipped.")?
			},
			Ok(bootnode) => bootnodes.push(bootnode),
			Err(e) => writeln!(output, "     Invalid boot node address: {e}")?,
		}
	}

	if bootnodes.is_empty() {
		writeln!(output, "\nNo boot nodes added to the chainspec.")?;
		return Ok(None);
	}

	writeln!(output, "\nAppending {}:", count_bootnodes(bootnodes.len()))?;
	display_bootnodes(
		bootnodes
			.iter()
			.enumerate()
			.map(|(index, bootnode)| (existing_bootnodes.len() + index + 1, bootnode)),
		output,
	)?;

	if !prompt_confirmation("Update the chain spec?", input, output)? {
		writeln!(output, "Aborted.")?;
		return Ok(None);
	}
	Ok(Some(bootnodes))
}

/// Prompts for the boot nodes to remove among the given existing ones.
///
/// The existing addresses are displayed as a numbered list and the ones to remove are selected by
/// number. An invalid selection is reported and the prompt is repeated.
///
/// `None` is returned when the operation is discarded, in which case the chain spec shall be left
/// untouched.
fn prompt_bootnodes_to_remove(
	existing_bootnodes: &[MultiaddrWithPeerId],
	input: &mut impl BufRead,
	output: &mut impl Write,
) -> io::Result<Option<Vec<MultiaddrWithPeerId>>> {
	if existing_bootnodes.is_empty() {
		writeln!(output, "The chain spec contains no boot nodes.")?;
		return Ok(None);
	}

	writeln!(output, "The chain spec contains {}:", count_bootnodes(existing_bootnodes.len()))?;
	display_bootnodes(number_bootnodes(existing_bootnodes), output)?;
	writeln!(
		output,
		"\nEnter the numbers of the boot nodes to remove, e.g. `1`, `1,3` or `2-4`. Enter `all` to \
		 remove all of them, or an empty line to cancel."
	)?;

	let selection = loop {
		write!(output, "> ")?;
		output.flush()?;

		let Some(line) = read_line(input)? else { return Ok(None) };
		if line.trim().is_empty() {
			writeln!(output, "Cancelled.")?;
			return Ok(None);
		}

		match parse_bootnodes_selection(&line, existing_bootnodes.len()) {
			Ok(selection) if selection.is_empty() => writeln!(output, "  No boot node selected.")?,
			Ok(selection) => break selection,
			Err(e) => writeln!(output, "  {e}")?,
		}
	};

	writeln!(output, "\nRemoving {}:", count_bootnodes(selection.len()))?;
	display_bootnodes(
		selection.iter().map(|&index| (index + 1, &existing_bootnodes[index])),
		output,
	)?;

	if !prompt_confirmation("Update the chain spec?", input, output)? {
		writeln!(output, "Aborted.")?;
		return Ok(None);
	}
	Ok(Some(selection.into_iter().map(|index| existing_bootnodes[index].clone()).collect()))
}

/// Parses a selection of boot nodes, as entered at the prompt of the `remove` command.
///
/// The selection holds 1-based boot node numbers separated by commas or spaces, and ranges such as
/// `2-4`. The single token `all` selects every boot node. The returned indices are 0-based, sorted
/// and deduplicated, so that the same boot node selected twice is removed once.
fn parse_bootnodes_selection(selection: &str, count: usize) -> Result<Vec<usize>, String> {
	if selection.trim().eq_ignore_ascii_case("all") {
		return Ok((0..count).collect());
	}

	let mut indices = Vec::new();
	for token in selection.split([',', ' ', '\t', '\n', '\r']).filter(|token| !token.is_empty()) {
		let range = match token.split_once('-') {
			Some((first, last)) => {
				parse_bootnode_number(first, count)?..=parse_bootnode_number(last, count)?
			},
			None => {
				let index = parse_bootnode_number(token, count)?;
				index..=index
			},
		};
		if range.is_empty() {
			return Err(format!("`{token}` is not a valid range."));
		}
		indices.extend(range);
	}

	indices.sort_unstable();
	indices.dedup();
	Ok(indices)
}

/// Parses the 1-based number of a boot node into its index, checking that it is in range.
fn parse_bootnode_number(number: &str, count: usize) -> Result<usize, String> {
	let number = number.trim();
	match number.parse::<usize>() {
		Ok(number) if (1..=count).contains(&number) => Ok(number - 1),
		Ok(_) => Err(format!("`{number}` is not in the 1..={count} range.")),
		Err(_) => Err(format!("`{number}` is not a boot node number.")),
	}
}

/// Extract any chain spec and convert it to JSON
fn extract_chain_spec_json(input_chain_spec: &Path) -> Result<serde_json::Value, String> {
	let chain_spec = &fs::read(input_chain_spec)
		.map_err(|e| format!("Provided chain spec could not be read: {e}"))?;

	serde_json::from_slice(&chain_spec).map_err(|e| format!("Conversion to json failed: {e}"))
}

#[cfg(test)]
mod tests {
	use super::*;
	use std::io::Cursor;

	const BOOTNODE_0: &str =
		"/dns/node-0.example.com/tcp/30333/p2p/12D3KooW9vw7UNUYQtPWK3RS8eyhjJgp4qBwbbiirYQcWLw5bCsf";
	const BOOTNODE_1: &str =
		"/dns/node-1.example.com/tcp/30333/p2p/12D3KooWAb5MyC1UJiEQJk4Hg4B2Vi3AJdqSUhTGYUqSnEqCFMFg";
	const BOOTNODE_2: &str =
		"/ip4/198.51.100.19/tcp/30333/p2p/12D3KooWAdyiVAaeGdtBt6vn5zVetwA4z4qfm9Fi2QCSykN1wTBJ";

	fn bootnodes(addresses: &[&str]) -> Vec<MultiaddrWithPeerId> {
		addresses
			.iter()
			.map(|address| MultiaddrWithPeerId::from_str(address).expect("a valid address. qed"))
			.collect()
	}

	/// Drives [`prompt_bootnodes_to_add`] with the given input, discarding what it displays.
	fn prompt_to_add(existing: &[&str], input: &str) -> Option<Vec<MultiaddrWithPeerId>> {
		prompt_bootnodes_to_add(
			&bootnodes(existing),
			&mut Cursor::new(input.as_bytes()),
			&mut Vec::new(),
		)
		.expect("the prompt to succeed. qed")
	}

	/// Drives [`prompt_bootnodes_to_remove`] with the given input, discarding what it displays.
	fn prompt_to_remove(existing: &[&str], input: &str) -> Option<Vec<MultiaddrWithPeerId>> {
		prompt_bootnodes_to_remove(
			&bootnodes(existing),
			&mut Cursor::new(input.as_bytes()),
			&mut Vec::new(),
		)
		.expect("the prompt to succeed. qed")
	}

	#[test]
	fn prompt_to_add_reads_addresses_until_an_empty_line() {
		assert_eq!(
			prompt_to_add(&[BOOTNODE_0], &format!("{BOOTNODE_1}\n{BOOTNODE_2}\n\n\n")),
			Some(bootnodes(&[BOOTNODE_1, BOOTNODE_2]))
		);
	}

	#[test]
	fn prompt_to_add_keeps_prompting_after_an_invalid_address() {
		assert_eq!(
			prompt_to_add(&[], &format!("/dns/node-0.example.com/tcp/30333\n{BOOTNODE_1}\n\n\n")),
			Some(bootnodes(&[BOOTNODE_1]))
		);
	}

	#[test]
	fn prompt_to_add_skips_the_duplicated_addresses() {
		assert_eq!(
			prompt_to_add(
				&[BOOTNODE_0],
				&format!("{BOOTNODE_0}\n{BOOTNODE_1}\n{BOOTNODE_1}\n\n\n")
			),
			Some(bootnodes(&[BOOTNODE_1]))
		);
	}

	#[test]
	fn prompt_to_add_discards_an_empty_selection() {
		assert_eq!(prompt_to_add(&[BOOTNODE_0], "\n"), None);
	}

	#[test]
	fn prompt_to_add_discards_a_rejected_confirmation() {
		assert_eq!(prompt_to_add(&[], &format!("{BOOTNODE_0}\n\nn\n")), None);
	}

	#[test]
	fn prompt_to_add_stops_at_the_end_of_the_input() {
		assert_eq!(prompt_to_add(&[], &format!("{BOOTNODE_0}\n")), Some(bootnodes(&[BOOTNODE_0])));
	}

	#[test]
	fn prompt_to_remove_selects_by_number() {
		assert_eq!(
			prompt_to_remove(&[BOOTNODE_0, BOOTNODE_1, BOOTNODE_2], "1,3\n\n"),
			Some(bootnodes(&[BOOTNODE_0, BOOTNODE_2]))
		);
	}

	#[test]
	fn prompt_to_remove_selects_all() {
		assert_eq!(
			prompt_to_remove(&[BOOTNODE_0, BOOTNODE_1], "all\n\n"),
			Some(bootnodes(&[BOOTNODE_0, BOOTNODE_1]))
		);
	}

	#[test]
	fn prompt_to_remove_keeps_prompting_after_an_invalid_selection() {
		assert_eq!(
			prompt_to_remove(&[BOOTNODE_0, BOOTNODE_1], "3\nnope\n2\n\n"),
			Some(bootnodes(&[BOOTNODE_1]))
		);
	}

	#[test]
	fn prompt_to_remove_cancels_on_an_empty_selection() {
		assert_eq!(prompt_to_remove(&[BOOTNODE_0], "\n"), None);
	}

	#[test]
	fn prompt_to_remove_discards_a_rejected_confirmation() {
		assert_eq!(prompt_to_remove(&[BOOTNODE_0], "1\nn\n"), None);
	}

	#[test]
	fn prompt_to_remove_does_nothing_without_boot_nodes() {
		assert_eq!(prompt_to_remove(&[], "all\n\n"), None);
	}

	#[test]
	fn bootnodes_selection_accepts_numbers_ranges_and_all() {
		assert_eq!(parse_bootnodes_selection("2", 3), Ok(vec![1]));
		assert_eq!(parse_bootnodes_selection("1,3", 3), Ok(vec![0, 2]));
		assert_eq!(parse_bootnodes_selection(" 3  1 ", 3), Ok(vec![0, 2]));
		assert_eq!(parse_bootnodes_selection("1-3", 3), Ok(vec![0, 1, 2]));
		assert_eq!(parse_bootnodes_selection("1-2, 2", 3), Ok(vec![0, 1]));
		assert_eq!(parse_bootnodes_selection("all", 3), Ok(vec![0, 1, 2]));
		assert_eq!(parse_bootnodes_selection("ALL\n", 2), Ok(vec![0, 1]));
	}

	#[test]
	fn bootnodes_selection_rejects_the_invalid_entries() {
		assert!(parse_bootnodes_selection("0", 3).is_err());
		assert!(parse_bootnodes_selection("4", 3).is_err());
		assert!(parse_bootnodes_selection("-1", 3).is_err());
		assert!(parse_bootnodes_selection("3-1", 3).is_err());
		assert!(parse_bootnodes_selection("one", 3).is_err());
		assert!(parse_bootnodes_selection("all,1", 3).is_err());
	}

	#[test]
	fn boot_nodes_are_counted_in_the_singular_and_the_plural() {
		assert_eq!(count_bootnodes(0), "0 boot nodes");
		assert_eq!(count_bootnodes(1), "1 boot node");
		assert_eq!(count_bootnodes(2), "2 boot nodes");
	}
}
