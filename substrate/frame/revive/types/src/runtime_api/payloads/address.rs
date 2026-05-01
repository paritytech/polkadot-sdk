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
use ethereum_types::H160;
use pallet_revive_proc_macro::define_versioned_interface;
use scale_info::TypeInfo;

define_versioned_interface! {
	/// Version 1 of the input payload that asks the runtime for the Ethereum H160 address
	/// associated with a given substrate `AccountId`. This is the inverse of
	/// [`AccountIdVersionedInputPayloadV1`](crate::runtime_api::AccountIdVersionedInputPayloadV1).
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
	pub struct AddressVersionedInputPayloadV1<AccountId> {
		/// `AccountId` whose Ethereum H160 address should be returned.
		pub account_id: AccountId
	}

	/// Version 1 of the output payload returned for an [`AddressVersionedInputPayloadV1`] request,
	/// carrying the Ethereum address derived from the supplied `AccountId`.
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
	pub struct AddressVersionedOutputPayloadV1 {
		/// Ethereum address that the runtime associates with the requested `AccountId`.
		pub address: H160
	}
}
