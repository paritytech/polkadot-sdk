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
use ethereum_types::U256;
use pallet_revive_proc_macro::define_versioned_interface;
use scale_info::TypeInfo;

define_versioned_interface! {
	/// Version 1 of the input payload that asks the runtime for the gas limit configured for the
	/// current block, expressed in Ethereum gas units so that callers can populate the `gasLimit`
	/// field of an Ethereum block.
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
	pub struct BlockGasLimitVersionedInputPayloadV1 {}

	/// Version 1 of the output payload returned for a [`BlockGasLimitVersionedInputPayloadV1`]
	/// request, carrying the configured block gas limit.
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
	pub struct BlockGasLimitVersionedOutputPayloadV1 {
		/// Block gas limit, in Ethereum gas units.
		pub gas_limit: U256
	}
}
