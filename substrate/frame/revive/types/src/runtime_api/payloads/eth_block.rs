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

use codec::{Decode, Encode, MaxEncodedLen};
use pallet_revive_proc_macro::define_versioned_interface;
use scale_info::TypeInfo;

use crate::runtime_api::BlockV1;

define_versioned_interface! {
	/// Version 1 of the input payload that asks the runtime to render the current substrate block
	/// in Ethereum block format. The result is suitable for serving back to Ethereum tooling that
	/// expects a JSON-RPC `eth_getBlockByNumber`-shaped response.
	#[derive(
		Debug,
		Clone,
		Eq,
		PartialEq,
		TypeInfo,
		Encode,
		Decode,
		MaxEncodedLen,
	)]
	pub struct EthBlockVersionedInputPayloadV1 {}

	/// Version 1 of the output payload returned for an [`EthBlockVersionedInputPayloadV1`] request,
	/// carrying the current block in Ethereum format.
	#[derive(
		Debug,
		Clone,
		Eq,
		PartialEq,
		TypeInfo,
		Encode,
		Decode,
	)]
	pub struct EthBlockVersionedOutputPayloadV1 {
		/// The current block, rendered as an Ethereum block.
		pub block: BlockV1
	}
}
