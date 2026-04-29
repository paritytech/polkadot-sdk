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

use codec::{Decode, Encode};
use ethereum_types::{H256, U256};
use pallet_revive_proc_macro::define_versioned_interface;
use scale_info::TypeInfo;

define_versioned_interface! {
	/// Version 1 of the input payload that asks the runtime for the Ethereum hash of a block,
	/// identified by its block number.
	#[derive(Debug, Clone, Eq, PartialEq, TypeInfo, Encode, Decode)]
	pub struct EthBlockHashVersionedInputPayloadV1 {
		/// Block number whose Ethereum hash should be returned.
		pub number: U256
	}

	/// Version 1 of the output payload returned for an [`EthBlockHashVersionedInputPayloadV1`]
	/// request, carrying the requested block hash if it is known to the runtime.
	#[derive(Debug, Clone, Eq, PartialEq, TypeInfo, Encode, Decode)]
	pub struct EthBlockHashVersionedOutputPayloadV1 {
		/// Ethereum hash of the requested block, or `None` if the block number does not exist on
		/// the current chain (e.g. it is in the future or has been pruned).
		pub block_hash: Option<H256>
	}
}
