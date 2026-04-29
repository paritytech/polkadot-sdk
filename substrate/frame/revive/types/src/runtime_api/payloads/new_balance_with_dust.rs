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
use ethereum_types::U256;
use pallet_revive_proc_macro::define_versioned_interface;
use scale_info::TypeInfo;

define_versioned_interface! {
	/// Version 1 of the input payload that asks the runtime to convert a balance expressed in
	/// 18-decimal Ethereum units into the runtime's native `Balance` type plus the leftover dust
	/// that does not fit in it. Used when crossing between Ethereum's 18-decimal world and the
	/// runtime's chosen smaller-precision `Balance` type.
	#[derive(Debug, Clone, Eq, PartialEq, TypeInfo, Encode, Decode)]
	pub struct NewBalanceWithDustVersionedInputPayloadV1 {
		/// Balance to split, expressed in 18-decimal Ethereum units.
		pub balance: U256
	}

	/// Version 1 of the output payload returned for a [`NewBalanceWithDustVersionedInputPayloadV1`]
	/// request, carrying both halves of the split.
	#[derive(Debug, Clone, Eq, PartialEq, TypeInfo, Encode, Decode)]
	pub struct NewBalanceWithDustVersionedOutputPayloadV1<Balance> {
		/// Portion of the input that fits cleanly into the runtime's native `Balance` type.
		pub balance: Balance,
		/// Leftover dust, in the smallest Ethereum-decimal units, that did not fit into the
		/// runtime's native `Balance` and must be carried separately.
		pub dust: u32
	}
}
