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

use super::bytes::Bytes;
use alloc::collections::BTreeMap;
use codec::{Decode, Encode};
use ethereum_types::{Address, H256, U256};
use pallet_revive_proc_macro::define_versioned_type;
use scale_info::TypeInfo;
use serde::{Deserialize, Serialize};

define_versioned_type! {
	/// Version 1 of a mapping from account addresses to state overrides.
	#[derive(
		Debug, Default, Clone, Serialize, Deserialize, Eq, PartialEq, TypeInfo, Encode, Decode,
	)]
	pub struct StateOverrideSetV1(
		/// Overrides keyed by account address.
		pub BTreeMap<Address, StateOverrideV1>
	);
}

impl core::ops::Deref for StateOverrideSetV1 {
	type Target = BTreeMap<Address, StateOverrideV1>;

	fn deref(&self) -> &Self::Target {
		&self.0
	}
}

impl core::ops::DerefMut for StateOverrideSetV1 {
	fn deref_mut(&mut self) -> &mut Self::Target {
		&mut self.0
	}
}

define_versioned_type! {
	/// Version 1 of a storage override for one account.
	#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq, TypeInfo, Encode, Decode)]
	#[serde(rename_all = "camelCase")]
	pub enum StorageOverrideV1 {
		/// Completely replaces the account storage with the provided slots.
		State(BTreeMap<H256, H256>),
		/// Patches the provided slots without clearing existing storage.
		StateDiff(BTreeMap<H256, H256>),
	}
}

define_versioned_type! {
	/// Version 1 of the per-account overrides applied during dry-run simulation.
	#[derive(
		Debug, Default, Clone, Serialize, Deserialize, Eq, PartialEq, TypeInfo, Encode, Decode,
	)]
	#[serde(rename_all = "camelCase")]
	pub struct StateOverrideV1 {
		/// Fake balance to set before executing the call.
		#[serde(skip_serializing_if = "Option::is_none")]
		pub balance: Option<U256>,
		/// Fake nonce to set before executing the call.
		#[serde(skip_serializing_if = "Option::is_none")]
		pub nonce: Option<U256>,
		/// Fake EVM bytecode to inject before executing the call.
		#[serde(skip_serializing_if = "Option::is_none")]
		pub code: Option<Bytes>,
		/// Full or differential storage override.
		#[serde(flatten)]
		pub storage: Option<StorageOverrideV1>,
		/// Address to which the existing precompile should be moved.
		#[serde(skip_serializing_if = "Option::is_none")]
		pub move_precompile_to_address: Option<Address>,
	}
}
