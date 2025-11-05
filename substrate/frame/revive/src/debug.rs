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

use crate::{Config, DebugSettingsOf};
use codec::{Decode, Encode, MaxEncodedLen};
use scale_info::TypeInfo;
use serde::{Deserialize, Serialize};
use sp_core::Get;
use sp_runtime::RuntimeDebug;

/// Debugging settings that can be configured when DebugEnabled config is true.
#[derive(
	Encode,
	Decode,
	Default,
	Clone,
	PartialEq,
	RuntimeDebug,
	TypeInfo,
	MaxEncodedLen,
	Serialize,
	Deserialize,
)]
pub struct DebugSettings {
	/// Whether to allow unlimited contract size.
	allow_unlimited_contract_size: bool,
	allow_eth_substrate_call: bool,
}

impl DebugSettings {
	pub fn new(allow_unlimited_contract_size: bool, allow_eth_substrate_call: bool) -> Self {
		Self { allow_unlimited_contract_size, allow_eth_substrate_call }
	}

	/// Returns true if unlimited contract size is allowed.
	pub fn is_unlimited_contract_size_allowed<T: Config>() -> bool {
		T::DebugEnabled::get() && DebugSettingsOf::<T>::get().allow_unlimited_contract_size
	}

	/// Returns true if eth_substrate_call is allowed.
	pub fn is_eth_substrate_call_allowed<T: Config>() -> bool {
		// TODO:: Temporarily disable the DebugEnabled check
		//T::DebugEnabled::get() &&
		DebugSettingsOf::<T>::get().allow_eth_substrate_call
	}

	/// Write the debug settings to storage.
	pub fn write_to_storage<T: Config>(&self) {
		DebugSettingsOf::<T>::put(self);
		if !T::DebugEnabled::get() {
			log::warn!(
				target: crate::LOG_TARGET,
				"Debug settings changed, but debug features are disabled in the runtime configuration."
			);
		}
	}
}
