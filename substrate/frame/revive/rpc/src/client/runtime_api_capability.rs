// This file is part of Substrate.

// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//  http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

use subxt::{OnlineClient, ext::frame_metadata::v16::RuntimeMetadataV16, runtime_api::RuntimeApi};

use crate::subxt_client::{self, SrcChainConfig};

/// Stores the capabilities of pallet-revive's runtime API.
///
/// New methods were added to pallet-revive over time without making proper use of frame's API
/// version. Therefore, there is no clean mapping of "in API version X, function A was added" or
/// anything of this sort. All such information needs to be obtained by analyzing the metadata at
/// a particular block to deduce such information.
///
/// This structure provides precisely this information and is used to answer the question "is method
/// X available on the runtime API for this block or not".
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct ReviveRuntimeApiCapabilities {
	pub eth_block: MethodStatus,
	pub eth_block_hash: MethodStatus,
	pub eth_receipt_data: MethodStatus,
	pub block_gas_limit: MethodStatus,
	pub max_extrinsic_weight_in_gas: MethodStatus,
	pub balance: MethodStatus,
	pub gas_price: MethodStatus,
	pub nonce: MethodStatus,
	pub call: MethodStatus,
	pub instantiate: MethodStatus,
	pub eth_transact: MethodStatus,
	pub eth_estimate_gas: MethodStatus,
	pub eth_pre_dispatch_weight: MethodStatus,
	pub upload_code: MethodStatus,
	pub get_storage: MethodStatus,
	pub runtime_pallets_address: MethodStatus,
	pub code: MethodStatus,
	pub account_id: MethodStatus,
	pub new_balance_with_dust: MethodStatus,
	pub block_author: MethodStatus,
	pub address: MethodStatus,
	pub trace_block: MethodStatus,
	pub trace_tx: MethodStatus,
	pub trace_call: MethodStatus,
}

impl ReviveRuntimeApiCapabilities {
	/// Constructs the revive runtime API capabilities.
	///
	/// This requires the metadata and the runtime API at the same block in order to be constructed.
	/// If they come from different blocks then the object created might end up with corrupted state
	/// that is not representative of any real block on the network.
	pub async fn new(
		metadata: &RuntimeMetadataV16,
		runtime_api: RuntimeApi<SrcChainConfig, OnlineClient<SrcChainConfig>>,
	) -> Self {
		let mut this = Self::default();

		let versioned_methods = runtime_api
			.call(subxt_client::apis().revive_api().version_declarations())
			.await
			.into_iter()
			.flat_map(|declarations| declarations.0.into_iter());
		for (method_name, method_version) in versioned_methods {
			if let Some(method_status) = this.get_method_status_ref_mut(&method_name) {
				*method_status =
					MethodStatus::Available(MethodVersioningStatus::Versioned(method_version));
			}
		}

		let unversioned_methods = metadata
			.apis
			.iter()
			.filter(|runtime_api| runtime_api.name == "ReviveApi")
			.max_by(|a, b| a.version.0.cmp(&b.version.0))
			.into_iter()
			.flat_map(|api| api.methods.iter())
			.filter(|method| !method.name.ends_with("_versioned"));
		for method in unversioned_methods {
			if let Some(method_status @ MethodStatus::Unavailable) =
				this.get_method_status_ref_mut(&method.name)
			{
				*method_status = MethodStatus::Available(MethodVersioningStatus::Unversioned);
			}
		}

		this
	}

	fn get_method_status_ref_mut(&mut self, key: impl AsRef<str>) -> Option<&mut MethodStatus> {
		let key = key.as_ref();
		let key = key.strip_suffix("_versioned").unwrap_or(key);
		match key {
			"eth_block" => Some(&mut self.eth_block),
			"eth_block_hash" => Some(&mut self.eth_block_hash),
			"eth_receipt_data" => Some(&mut self.eth_receipt_data),
			"block_gas_limit" => Some(&mut self.block_gas_limit),
			"max_extrinsic_weight_in_gas" => Some(&mut self.max_extrinsic_weight_in_gas),
			"balance" => Some(&mut self.balance),
			"gas_price" => Some(&mut self.gas_price),
			"nonce" => Some(&mut self.nonce),
			"call" => Some(&mut self.call),
			"instantiate" => Some(&mut self.instantiate),
			"eth_transact" => Some(&mut self.eth_transact),
			"eth_estimate_gas" => Some(&mut self.eth_estimate_gas),
			"eth_pre_dispatch_weight" => Some(&mut self.eth_pre_dispatch_weight),
			"upload_code" => Some(&mut self.upload_code),
			"get_storage" => Some(&mut self.get_storage),
			"runtime_pallets_address" => Some(&mut self.runtime_pallets_address),
			"code" => Some(&mut self.code),
			"account_id" => Some(&mut self.account_id),
			"new_balance_with_dust" => Some(&mut self.new_balance_with_dust),
			"block_author" => Some(&mut self.block_author),
			"address" => Some(&mut self.address),
			"trace_block" => Some(&mut self.trace_block),
			"trace_tx" => Some(&mut self.trace_tx),
			"trace_call" => Some(&mut self.trace_call),
			_ => None,
		}
	}
}

/// Defines the status of a runtime API function in pallet-revive and whether it's available or not.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub enum MethodStatus {
	/// The runtime API method is not available on the runtime API of the selected block (e.g., the
	/// `estimate_gas` runtime API function which was added later on).
	#[default]
	Unavailable,

	/// The runtime API method is available on the runtime API, and may either be versioned or
	/// unversioned.
	Available(MethodVersioningStatus),
}

/// Defines the status of a runtime API function's versioning.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum MethodVersioningStatus {
	/// The method is available on the runtime API and is not versioned (e.g., the specified block
	/// has a pre-versioning runtime).
	Unversioned,

	/// The method is available on the runtime API and is versioned. The provided value is the
	/// highest version supported by the runtime.
	Versioned(u8),
}
