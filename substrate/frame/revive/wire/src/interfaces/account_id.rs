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
use ethereum_types::H160;
use pallet_revive_proc_macro::define_versioned_interface;
use scale_info::TypeInfo;

define_versioned_interface! {
	/// Version 1 of the input payload that asks the runtime for the substrate `AccountId`
	/// associated with a given Ethereum H160 address, applying whatever address-to-account
	/// derivation scheme `pallet-revive` currently uses.
	#[derive(Debug, Clone, Eq, PartialEq, TypeInfo, Encode, Decode)]
	pub struct AccountIdVersionedInputPayloadV1 {
		/// Ethereum address whose corresponding `AccountId` should be returned.
		pub address: H160
	}

	/// Version 1 of the output payload returned for an [`AccountIdVersionedInputPayloadV1`]
	/// request, carrying the resolved `AccountId`.
	#[derive(Debug, Clone, Eq, PartialEq, TypeInfo, Encode, Decode)]
	pub struct AccountIdVersionedOutputPayloadV1<AccountId> {
		/// `AccountId` that the runtime associates with the requested Ethereum address.
		pub account_id: AccountId
	}
}
