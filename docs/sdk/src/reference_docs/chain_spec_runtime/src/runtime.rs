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

//! A minimal runtime that shows runtime genesis state.

// Make the WASM binary available.
#[cfg(feature = "std")]
include!(concat!(env!("OUT_DIR"), "/wasm_binary.rs"));

use crate::{
	pallets::{pallet_bar, pallet_foo},
	presets::*,
};
use alloc::{vec, vec::Vec};
use frame::{
	deps::frame_support::{
		genesis_builder_helper::{build_state, get_preset},
		runtime,
	},
	prelude::*,
	runtime::{apis, prelude::*},
};
use sp_genesis_builder::PresetId;

/// The runtime version.
#[runtime_version]
pub const VERSION: RuntimeVersion = RuntimeVersion {
	spec_name: alloc::borrow::Cow::Borrowed("minimal-template-runtime"),
	impl_name: alloc::borrow::Cow::Borrowed("minimal-template-runtime"),
	authoring_version: 1,
	spec_version: 0,
	impl_version: 1,
	apis: RUNTIME_API_VERSIONS,
	transaction_version: 1,
	system_version: 1,
};

/// The signed extensions that are added to the runtime.
type SignedExtra = ();

// Composes the runtime by adding all the used pallets and deriving necessary types.
#[runtime]
mod runtime {
	/// The main runtime type.
	#[runtime::runtime]
	#[runtime::derive(
		RuntimeCall,
		RuntimeEvent,
		RuntimeError,
		RuntimeOrigin,
		RuntimeTask,
		RuntimeViewFunction
	)]
	pub struct Runtime;

	/// Mandatory system pallet that should always be included in a FRAME runtime.
	#[runtime::pallet_index(0)]
	pub type System = frame_system;

	/// Sample pallet 1
	#[runtime::pallet_index(1)]
	pub type Bar = pallet_bar;

	/// Sample pallet 2
	#[runtime::pallet_index(2)]
	pub type Foo = pallet_foo;
}

parameter_types! {
	pub const Version: RuntimeVersion = VERSION;
}

/// Implements the types required for the system pallet.
#[derive_impl(frame_system::config_preludes::SolochainDefaultConfig)]
impl frame_system::Config for Runtime {
	type Block = Block;
	type Version = Version;
}

impl pallet_bar::Config for Runtime {}
impl pallet_foo::Config for Runtime {}

type Block = frame::runtime::types_common::BlockOf<Runtime, SignedExtra>;
type Header = HeaderFor<Runtime>;

#[docify::export(runtime_impl)]
impl_runtime_apis! {
	impl sp_genesis_builder::GenesisBuilder<Block> for Runtime {
		fn build_state(config: Vec<u8>) -> sp_genesis_builder::Result {
			build_state::<RuntimeGenesisConfig>(config)
		}

		fn get_preset(id: &Option<sp_genesis_builder::PresetId>) -> Option<Vec<u8>> {
			get_preset::<RuntimeGenesisConfig>(id, get_builtin_preset)
		}

		fn preset_names() -> Vec<sp_genesis_builder::PresetId> {
			vec![
				PresetId::from(PRESET_1),
				PresetId::from(PRESET_2),
				PresetId::from(PRESET_3),
				PresetId::from(PRESET_4),
				PresetId::from(PRESET_INVALID)
			]
		}
	}

	impl apis::Core<Block> for Runtime {
		fn version() -> RuntimeVersion { VERSION }
		fn execute_block(_: Block) { }
		fn initialize_block(_: &Header) -> ExtrinsicInclusionMode { ExtrinsicInclusionMode::default() }
	}
}
