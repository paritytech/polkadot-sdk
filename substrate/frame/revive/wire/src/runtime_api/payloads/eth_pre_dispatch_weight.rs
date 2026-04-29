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

use alloc::vec::Vec;
use codec::{Decode, Encode};
use pallet_revive_proc_macro::define_versioned_interface;
use scale_info::TypeInfo;
use sp_weights::Weight;

define_versioned_interface! {
	/// Version 1 of the input payload that asks the runtime for the weight an already-encoded
	/// Ethereum transaction will consume before it is dispatched. Used by the inclusion fee
	/// machinery to charge the right amount before execution.
	#[derive(Debug, Clone, Eq, PartialEq, TypeInfo, Encode, Decode)]
	pub struct EthPreDispatchWeightVersionedInputPayloadV1 {
		/// Encoded Ethereum transaction whose pre-dispatch weight should be computed.
		pub tx: Vec<u8>
	}

	/// Version 1 of the output payload returned for an
	/// [`EthPreDispatchWeightVersionedInputPayloadV1`] request, carrying the computed weight.
	#[derive(Debug, Clone, Eq, PartialEq, TypeInfo, Encode, Decode)]
	pub struct EthPreDispatchWeightVersionedOutputPayloadV1 {
		/// Weight the transaction would consume to be dispatched.
		pub weight: Weight
	}
}
