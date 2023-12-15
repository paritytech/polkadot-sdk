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

#![cfg_attr(not(feature = "std"), no_std)]

//! Substrate genesis config builder
//!
//! The runtime may provide a number of partial `RuntimeGenesisConfig` configurations in form of
//! patches which shall be applied on top of the default `RuntimeGenesisConfig`. Thus presets are
//! sometimes refered to as patches. This allows the runtime to provide a number of predefined
//! configuration (e.g. for different testnets) without leaking the runtime types outside the
//! runtime.
//!
//! This Runtime API allows to interact with `RuntimeGenesisConfig`, in particular:
//! - provide the list of available preset names,
//! - provide a number of named, built-in, presets of `RuntimeGenesisConfig`,
//! - serialize the default `RuntimeGenesisConfig` struct into json format,
//! - deserialize the `RuntimeGenesisConfig` from given json blob and put the resulting
//!   `RuntimeGenesisConfig` into the state storage creating the initial runtime's state. Allows to
//!   build customized genesis. This operation internally calls `GenesisBuild::build` function for
//!   all runtime pallets.
//!
//! Providing externalities with empty storage and putting `RuntimeGenesisConfig` into storage
//! allows to catch and build the raw storage of `RuntimeGenesisConfig` which is the foundation for
//! genesis block.

/// The result type alias, used in build methods. `Err` contains formatted error message.
pub type Result = core::result::Result<(), sp_runtime::RuntimeString>;

sp_api::decl_runtime_apis! {
	/// API to interact with RuntimeGenesisConfig for the runtime
	#[api_version(2)]
	pub trait GenesisBuilder {
		/// Creates the default `RuntimeGenesisConfig` and returns it as a JSON blob.
		///
		/// This function instantiates the default `RuntimeGenesisConfig` struct for the runtime and
		/// serializes it into a JSON blob. It returns a `Vec<u8>` containing the JSON
		/// representation of the default `RuntimeGenesisConfig`.
		fn create_default_config() -> sp_std::vec::Vec<u8>;

		/// Build `RuntimeGenesisConfig` from a JSON blob not using any defaults and store it in the
		/// storage.
		///
		/// This function deserializes the full `RuntimeGenesisConfig` from the given JSON blob and
		/// puts it into the storage. If the provided JSON blob is incorrect or incomplete or the
		/// deserialization fails, an error is returned.
		///
		/// Please note that provided json blob must contain all `RuntimeGenesisConfig` fields, no
		/// defaults will be used.
		#[renamed("build_config", 2)]
		fn build_state(json: sp_std::vec::Vec<u8>) -> Result;

		/// Returns a JSON blob representation of the built-in `RuntimeGenesisConfig` identified by
		/// `id`.
		///
		/// If `id` is `None` the function returns JSON blob representation of the default
		/// `RuntimeGenesisConfig` struct of the runtime. Implementation must provide default
		/// `RuntimeGenesisConfig`.
		///
		/// Otherwise function returns a JSON representation of the built-in, named
		/// `RuntimeGenesisConfig` preset identified by `id`, or `None` if such preset does not
		/// exists. Returned `Vec<u8>` contains bytes of JSON blob.
		#[api_version(2)]
		fn get_preset(id: Option<sp_runtime::RuntimeString>) -> Option<sp_std::vec::Vec<u8>>;

		/// Returns a list of names for available builtin `RuntimeGenesisConfig` presets.
		///
		/// The presets from the list can be queried with [`GenesisBuilder::get_preset`] method. If
		/// no named presets are provided by the runtime the list is empty.
		#[api_version(2)]
		fn preset_names() -> sp_std::vec::Vec<sp_runtime::RuntimeString>;
	}
}
