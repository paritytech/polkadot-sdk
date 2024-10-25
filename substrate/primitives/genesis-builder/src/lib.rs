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

//! # Substrate genesis config builder.
//!
//! This crate contains [`GenesisBuilder`], a runtime-api to be implemented by runtimes, in order to
//! express their genesis state.
//!
//! The overall flow of the methods in [`GenesisBuilder`] is as follows:
//!
//! 1. [`GenesisBuilder::preset_names`]: A runtime exposes a number of different
//!    `RuntimeGenesisConfig` variations, each of which is called a `preset`, and is identified by a
//!    [`PresetId`]. All runtimes are encouraged to expose at least [`DEV_RUNTIME_PRESET`] and
//!    [`LOCAL_TESTNET_RUNTIME_PRESET`] presets for consistency.
//! 2. [`GenesisBuilder::get_preset`]: Given a `PresetId`, this the runtime returns the JSON blob
//!    representation of the `RuntimeGenesisConfig` for that preset. This JSON blob is often mixed
//!    into the broader `chain_spec`. If `None` is given, [`GenesisBuilder::get_preset`] provides a
//!    JSON represention of the default `RuntimeGenesisConfig` (by simply serializing the
//!    `RuntimeGenesisConfig::default()` value into JSON format). This is used as a base for
//!    applying patches / presets.

//! 3. [`GenesisBuilder::build_state`]: Given a JSON blob, this method should deserialize it and
//!    enact it (using `frame_support::traits::BuildGenesisConfig` for Frame-based runtime),
//!    essentially writing it to the state.
//!
//! The first two flows are often done in between a runtime, and the `chain_spec_builder` binary.
//! The latter is used when a new blockchain is launched to enact and store the genesis state. See
//! the documentation of `chain_spec_builder` for more info.
//!
//! ## Patching
//!
//! The runtime may provide a number of partial predefined `RuntimeGenesisConfig` configurations in
//! the form of patches which shall be applied on top of the default `RuntimeGenesisConfig`. The
//! patch is a JSON blob, which essentially comprises the list of key-value pairs that are to be
//! customized in the default runtime genesis config. These predefined configurations are referred
//! to as presets.
//!
//! This allows the runtime to provide a number of predefined configs (e.g. for different testnets
//! or development) without necessarily to leak the runtime types outside itself (e.g. node or
//! chain-spec related tools).
//!
//! ## FRAME vs. non-FRAME
//!
//! For FRAME based runtimes [`GenesisBuilder`] provides means to interact with
//! `RuntimeGenesisConfig`.
//!
//! For non-FRAME runtimes this interface is intended to build genesis state of the runtime basing
//! on some input arbitrary bytes array. This documentation uses term `RuntimeGenesisConfig`, which
//! for non-FRAME runtimes may be understood as the "runtime-side entity representing initial
//! runtime genesis configuration". The representation of the preset is an arbitrary `Vec<u8>` and
//! does not necessarily have to represent a JSON blob.
//!
//! ## Genesis Block State
//!
//! Providing externalities with an empty storage and putting `RuntimeGenesisConfig` into storage
//! (by calling `build_state`) allows to construct the raw storage of `RuntimeGenesisConfig`
//! which is the foundation for genesis block.

extern crate alloc;
use alloc::vec::Vec;

/// The result type alias, used in build methods. `Err` contains formatted error message.
pub type Result = core::result::Result<(), sp_runtime::RuntimeString>;

/// The type representing preset ID.
pub type PresetId = sp_runtime::RuntimeString;

/// The default `development` preset used to communicate with the runtime via
/// [`GenesisBuilder`] interface.
///
/// (Recommended for testing with a single node, e.g., for benchmarking)
pub const DEV_RUNTIME_PRESET: &'static str = "development";

/// The default `local_testnet` preset used to communicate with the runtime via
/// [`GenesisBuilder`] interface.
///
/// (Recommended for local testing with multiple nodes)
pub const LOCAL_TESTNET_RUNTIME_PRESET: &'static str = "local_testnet";

sp_api::decl_runtime_apis! {
	/// API to interact with `RuntimeGenesisConfig` for the runtime
	pub trait GenesisBuilder {
		/// Build `RuntimeGenesisConfig` from a JSON blob not using any defaults and store it in the
		/// storage.
		///
		/// In the case of a FRAME-based runtime, this function deserializes the full
		/// `RuntimeGenesisConfig` from the given JSON blob and puts it into the storage. If the
		/// provided JSON blob is incorrect or incomplete or the deserialization fails, an error
		/// is returned.
		///
		/// Please note that provided JSON blob must contain all `RuntimeGenesisConfig` fields, no
		/// defaults will be used.
		fn build_state(json: Vec<u8>) -> Result;

		/// Returns a JSON blob representation of the built-in `RuntimeGenesisConfig` identified by
		/// `id`.
		///
		/// If `id` is `None` the function should return JSON blob representation of the default
		/// `RuntimeGenesisConfig` struct of the runtime. Implementation must provide default
		/// `RuntimeGenesisConfig`.
		///
		/// Otherwise function returns a JSON representation of the built-in, named
		/// `RuntimeGenesisConfig` preset identified by `id`, or `None` if such preset does not
		/// exist. Returned `Vec<u8>` contains bytes of JSON blob (patch) which comprises a list of
		/// (potentially nested) key-value pairs that are intended for customizing the default
		/// runtime genesis config. The patch shall be merged (rfc7386) with the JSON representation
		/// of the default `RuntimeGenesisConfig` to create a comprehensive genesis config that can
		/// be used in `build_state` method.
		fn get_preset(id: &Option<PresetId>) -> Option<Vec<u8>>;

		/// Returns a list of identifiers for available builtin `RuntimeGenesisConfig` presets.
		///
		/// The presets from the list can be queried with [`GenesisBuilder::get_preset`] method. If
		/// no named presets are provided by the runtime the list is empty.
		fn preset_names() -> Vec<PresetId>;
	}
}
