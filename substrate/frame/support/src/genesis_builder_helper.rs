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

//! Helper functions for implementing [`sp_genesis_builder::GenesisBuilder`] for runtimes.
//!
//! Provides common logic. For more info refer to [`sp_genesis_builder::GenesisBuilder`].

extern crate alloc;

use alloc::vec::Vec;
use frame_support::traits::BuildGenesisConfig;
use sp_genesis_builder::{PresetId, Result as BuildResult};
use sp_runtime::format_runtime_string;

/// Build `GenesisConfig` from a JSON blob not using any defaults and store it in the storage. For
/// more info refer to [`sp_genesis_builder::GenesisBuilder::build_state`].
pub fn build_state<GC: BuildGenesisConfig>(json: Vec<u8>) -> BuildResult {
	let gc = serde_json::from_slice::<GC>(&json)
		.map_err(|e| format_runtime_string!("Invalid JSON blob: {}", e))?;
	<GC as BuildGenesisConfig>::build(&gc);
	Ok(())
}

/// Get the default `GenesisConfig` as a JSON blob if `name` is None.
///
/// Query of named presets is delegetaed to provided `preset_for_name` closure. For more info refer
/// to [`sp_genesis_builder::GenesisBuilder::get_preset`].
pub fn get_preset<GC>(
	name: &Option<PresetId>,
	preset_for_name: impl FnOnce(&sp_genesis_builder::PresetId) -> Option<sp_std::vec::Vec<u8>>,
) -> Option<Vec<u8>>
where
	GC: BuildGenesisConfig + Default,
{
	name.as_ref().map_or(
		Some(
			serde_json::to_string(&GC::default())
				.expect("serialization to json is expected to work. qed.")
				.into_bytes(),
		),
		preset_for_name,
	)
}
