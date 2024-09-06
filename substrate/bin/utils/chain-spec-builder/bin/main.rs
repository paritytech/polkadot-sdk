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

use chain_spec_builder::{
	generate_chain_spec_for_runtime, AddCodeSubstituteCmd, ChainSpecBuilder, ChainSpecBuilderCmd,
	ConvertToRawCmd, DisplayPresetCmd, ListPresetsCmd, UpdateCodeCmd, VerifyCmd,
};
use clap::Parser;
use remote_externalities::RemoteExternalities;
use sc_chain_spec::{
	set_code_substitute_in_json_chain_spec, update_code_in_json_chain_spec, GenericChainSpec,
	GenesisConfigBuilderRuntimeCaller,
};
use sp_core::{storage::well_known_keys::is_default_child_storage_key, twox_64};
use staging_chain_spec_builder as chain_spec_builder;
use std::fs;

type ChainSpec = GenericChainSpec<sc_chain_spec::NoExtension, ()>;

//avoid error message escaping
fn main() {
	match inner_main() {
		Err(e) => eprintln!("{}", format!("{e}")),
		_ => {},
	}
}

fn inner_main() -> Result<(), String> {
	sp_tracing::try_init_simple();

	let builder = ChainSpecBuilder::parse();
	let chain_spec_path = builder.chain_spec_path.to_path_buf();

	match builder.command {
		ChainSpecBuilderCmd::Create(cmd) => {
			let chain_spec_json = generate_chain_spec_for_runtime(&cmd)?;
			fs::write(chain_spec_path, chain_spec_json).map_err(|err| err.to_string())?;
		},
		ChainSpecBuilderCmd::UpdateCode(UpdateCodeCmd {
			ref input_chain_spec,
			ref runtime_wasm_path,
		}) => {
			let chain_spec = ChainSpec::from_json_file(input_chain_spec.clone())?;

			let mut chain_spec_json =
				serde_json::from_str::<serde_json::Value>(&chain_spec.as_json(false)?)
					.map_err(|e| format!("Conversion to json failed: {e}"))?;
			update_code_in_json_chain_spec(
				&mut chain_spec_json,
				&fs::read(runtime_wasm_path.as_path())
					.map_err(|e| format!("Wasm blob file could not be read: {e}"))?[..],
			);

			let chain_spec_json = serde_json::to_string_pretty(&chain_spec_json)
				.map_err(|e| format!("to pretty failed: {e}"))?;
			fs::write(chain_spec_path, chain_spec_json).map_err(|err| err.to_string())?;
		},
		ChainSpecBuilderCmd::AddCodeSubstitute(AddCodeSubstituteCmd {
			ref input_chain_spec,
			ref runtime_wasm_path,
			block_height,
		}) => {
			let chain_spec = ChainSpec::from_json_file(input_chain_spec.clone())?;

			let mut chain_spec_json =
				serde_json::from_str::<serde_json::Value>(&chain_spec.as_json(false)?)
					.map_err(|e| format!("Conversion to json failed: {e}"))?;

			set_code_substitute_in_json_chain_spec(
				&mut chain_spec_json,
				&fs::read(runtime_wasm_path.as_path())
					.map_err(|e| format!("Wasm blob file could not be read: {e}"))?[..],
				block_height,
			);
			let chain_spec_json = serde_json::to_string_pretty(&chain_spec_json)
				.map_err(|e| format!("to pretty failed: {e}"))?;
			fs::write(chain_spec_path, chain_spec_json).map_err(|err| err.to_string())?;
		},
		ChainSpecBuilderCmd::ConvertToRaw(ConvertToRawCmd { ref input_chain_spec }) => {
			let chain_spec = ChainSpec::from_json_file(input_chain_spec.clone())?;

			let chain_spec_json =
				serde_json::from_str::<serde_json::Value>(&chain_spec.as_json(true)?)
					.map_err(|e| format!("Conversion to json failed: {e}"))?;

			let chain_spec_json = serde_json::to_string_pretty(&chain_spec_json)
				.map_err(|e| format!("Conversion to pretty failed: {e}"))?;
			fs::write(chain_spec_path, chain_spec_json).map_err(|err| err.to_string())?;
		},
		ChainSpecBuilderCmd::Verify(VerifyCmd { ref input_chain_spec }) => {
			let chain_spec = ChainSpec::from_json_file(input_chain_spec.clone())?;
			let _ = serde_json::from_str::<serde_json::Value>(&chain_spec.as_json(true)?)
				.map_err(|e| format!("Conversion to json failed: {e}"))?;
		},
		ChainSpecBuilderCmd::ListPresets(ListPresetsCmd { runtime_wasm_path }) => {
			let code = fs::read(runtime_wasm_path.as_path())
				.map_err(|e| format!("wasm blob shall be readable {e}"))?;
			let caller: GenesisConfigBuilderRuntimeCaller =
				GenesisConfigBuilderRuntimeCaller::new(&code[..]);
			let presets = caller
				.preset_names()
				.map_err(|e| format!("getting default config from runtime should work: {e}"))?;
			let presets: Vec<String> = presets
				.into_iter()
				.map(|preset| {
					String::from(
						TryInto::<&str>::try_into(&preset)
							.unwrap_or_else(|_| "cannot display preset id")
							.to_string(),
					)
				})
				.collect();
			println!("{}", serde_json::json!({"presets":presets}).to_string());
		},
		ChainSpecBuilderCmd::DisplayPreset(DisplayPresetCmd { runtime_wasm_path, preset_name }) => {
			let code = fs::read(runtime_wasm_path.as_path())
				.map_err(|e| format!("wasm blob shall be readable {e}"))?;
			let caller: GenesisConfigBuilderRuntimeCaller =
				GenesisConfigBuilderRuntimeCaller::new(&code[..]);
			let preset = caller
				.get_named_preset(preset_name.as_ref())
				.map_err(|e| format!("getting default config from runtime should work: {e}"))?;
			println!("{preset}");
		},
		ChainSpecBuilderCmd::FromLive(cmd) => {
			use sp_runtime::testing::{Block as RawBlock, ExtrinsicWrapper, H256 as Hash};
			type Block = RawBlock<ExtrinsicWrapper<Hash>>;
			use remote_externalities::{Mode, OnlineConfig, SnapshotConfig, Transport};

			let rt = tokio::runtime::Runtime::new().unwrap();
			let ext: RemoteExternalities<Block> = rt.block_on(async {
				remote_externalities::Builder::<Block>::new()
					.mode(Mode::Online(OnlineConfig {
						transport: Transport::Uri(cmd.uri),
						state_snapshot: Some(SnapshotConfig { path: "./try-runtime.snap".into() }),
						child_trie: true,
						..Default::default()
					}))
					.blacklist_hashed_key(&twox_64(b"Babe"))
					.blacklist_hashed_key(&twox_64(b"Grandpa"))
					.blacklist_hashed_key(&twox_64(b"Session"))
					.blacklist_hashed_key(&twox_64(b"Beefy"))
					.blacklist_hashed_key(&twox_64(b"Authorship"))
					.build()
					.await
					.unwrap()
			});
			let storage = ext.inner_ext.into_storage();

			println!(
				"{} top keys, {} child keys",
				storage.top.keys().len(),
				storage.children_default.keys().len()
			);
			println!(
				"has code? {}",
				storage.top.keys().any(|k| k == sp_core::storage::well_known_keys::CODE)
			);
			let maybe_child = storage
				.top
				.keys()
				.filter(|k| is_default_child_storage_key(k))
				.collect::<Vec<_>>();
			println!(
				"{}, {:?} child keys",
				maybe_child.len(),
				maybe_child.iter().take(5).collect::<Vec<_>>()
			);

			// parse cmd.chain_spec as a json file
			let chain_spec = ChainSpec::from_json_file(cmd.chain_spec.clone()).unwrap();

			// TODO: assuming original is not raw
			let mut original_spec_json = serde_json::from_str::<serde_json::Value>(
				&chain_spec.clone().as_json(false).unwrap(),
			)
			.unwrap();
			let mut new_spec = chain_spec;

			<ChainSpec as sc_chain_spec::ChainSpec>::set_storage(&mut new_spec, storage);
			let new_spec_json =
				serde_json::from_str::<serde_json::Value>(&new_spec.as_json(false).unwrap())
					.unwrap();
			sc_chain_spec::json_patch::merge(&mut original_spec_json, new_spec_json);

			fs::write("./out.json", serde_json::to_string_pretty(&original_spec_json).unwrap())
				.map_err(|err| err.to_string())?;
		},
	};
	Ok(())
}
