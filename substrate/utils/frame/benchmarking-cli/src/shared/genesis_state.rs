// This file is part of Substrate.

// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
// 	http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

use crate::overhead::command::ParachainExtension;
use sc_chain_spec::{ChainSpec, GenericChainSpec, GenesisConfigBuilderRuntimeCaller};
use sc_cli::Result;
use serde_json::Value;
use sp_storage::{well_known_keys::CODE, Storage};
use sp_wasm_interface::HostFunctions;
use std::{borrow::Cow, path::PathBuf};

/// When the runtime could not build the genesis storage.
const ERROR_CANNOT_BUILD_GENESIS: &str = "The runtime returned \
an error when trying to build the genesis storage. Please ensure that all pallets \
define a genesis config that can be built. This can be tested with: \
https://github.com/paritytech/polkadot-sdk/pull/3412";

/// Warn when using the chain spec to generate the genesis state.
pub const WARN_SPEC_GENESIS_CTOR: &'static str = "Using the chain spec instead of the runtime to \
generate the genesis state is deprecated. Please remove the `--chain`/`--dev`/`--local` argument, \
point `--runtime` to your runtime blob and set `--genesis-builder=runtime`. This warning may \
become a hard error any time after December 2024.";

/// Defines how the chain specification shall be used to build the genesis storage.
pub enum SpecGenesisSource {
	/// Use preset provided by the runtime embedded in the chain specification.
	Runtime(String),
	/// Use provided chain-specification JSON file.
	SpecJson,
	/// Use default storage.
	None,
}

/// Defines how the genesis storage shall be built.
pub enum GenesisStateHandler {
	ChainSpec(Box<dyn ChainSpec>, SpecGenesisSource),
	Runtime(Vec<u8>, Option<String>),
}

impl GenesisStateHandler {
	/// Populate the genesis storage.
	///
	/// If the raw storage is derived from a named genesis preset, `json_patcher` is can be used to
	/// inject values into the preset.
	pub fn build_storage<HF: HostFunctions>(
		&self,
		json_patcher: Option<Box<dyn FnOnce(Value) -> Value + 'static>>,
	) -> Result<Storage> {
		match self {
			GenesisStateHandler::ChainSpec(chain_spec, source) => match source {
				SpecGenesisSource::Runtime(preset) => {
					let mut storage = chain_spec.build_storage()?;
					let code_bytes = storage
						.top
						.remove(CODE)
						.ok_or("chain spec genesis does not contain code")?;
					genesis_from_code::<HF>(code_bytes.as_slice(), preset, json_patcher)
				},
				SpecGenesisSource::SpecJson => chain_spec
					.build_storage()
					.map_err(|e| format!("{ERROR_CANNOT_BUILD_GENESIS}\nError: {e}").into()),
				SpecGenesisSource::None => Ok(Storage::default()),
			},
			GenesisStateHandler::Runtime(code_bytes, Some(preset)) =>
				genesis_from_code::<HF>(code_bytes.as_slice(), preset, json_patcher),
			GenesisStateHandler::Runtime(_, None) => Ok(Storage::default()),
		}
	}

	/// Get the runtime code blob.
	pub fn get_code_bytes(&self) -> Result<Cow<'_, [u8]>> {
		match self {
			GenesisStateHandler::ChainSpec(chain_spec, _) => {
				let mut storage = chain_spec.build_storage()?;
				storage
					.top
					.remove(CODE)
					.map(|code| Cow::from(code))
					.ok_or("chain spec genesis does not contain code".into())
			},
			GenesisStateHandler::Runtime(code_bytes, _) => Ok(code_bytes.into()),
		}
	}
}

pub fn chain_spec_from_path<HF: HostFunctions>(
	chain: PathBuf,
) -> Result<(Box<dyn ChainSpec>, Option<u32>)> {
	let spec = GenericChainSpec::<ParachainExtension, HF>::from_json_file(chain)
		.map_err(|e| format!("Unable to load chain spec: {:?}", e))?;

	let para_id_from_chain_spec = spec.extensions().para_id;
	Ok((Box::new(spec), para_id_from_chain_spec))
}

fn genesis_from_code<EHF: HostFunctions>(
	code: &[u8],
	genesis_builder_preset: &String,
	storage_patcher: Option<Box<dyn FnOnce(Value) -> Value>>,
) -> Result<Storage> {
	let genesis_config_caller = GenesisConfigBuilderRuntimeCaller::<(
		sp_io::SubstrateHostFunctions,
		frame_benchmarking::benchmarking::HostFunctions,
		EHF,
	)>::new(code);

	let mut preset_json = genesis_config_caller.get_named_preset(Some(genesis_builder_preset))?;
	if let Some(patcher) = storage_patcher {
		preset_json = patcher(preset_json);
	}

	let mut storage =
		genesis_config_caller.get_storage_for_patch(preset_json).inspect_err(|e| {
			let presets = genesis_config_caller.preset_names().unwrap_or_default();
			log::error!(
				"Please pick one of the available presets with \
        `--genesis-builder-preset=<PRESET>`. Available presets ({}): {:?}. Error: {:?}",
				presets.len(),
				presets,
				e
			);
		})?;

	storage.top.insert(CODE.into(), code.into());

	Ok(storage)
}
