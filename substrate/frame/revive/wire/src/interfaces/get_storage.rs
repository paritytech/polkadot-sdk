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
use ethereum_types::H160;
use pallet_revive_proc_macro::define_versioned_interface;
use scale_info::TypeInfo;

use crate::StorageKeyV1;

define_versioned_interface! {
	/// Version 1 of the input payload that asks the runtime for the value stored at a given storage
	/// key inside a contract.
	#[derive(Debug, Clone, Eq, PartialEq, TypeInfo, Encode, Decode)]
	pub struct GetStorageVersionedInputPayloadV1 {
		/// Ethereum address of the contract whose storage is being queried.
		pub address: H160,
		/// Storage key inside the contract whose value should be returned.
		pub key: StorageKeyV1
	}

	/// Version 1 of the output payload returned for a [`GetStorageVersionedInputPayloadV1`]
	/// request, carrying the value at the requested storage key.
	#[derive(Debug, Clone, Eq, PartialEq, TypeInfo, Encode, Decode)]
	pub struct GetStorageVersionedOutputPayloadV1 {
		/// Raw bytes stored at the requested key, or `None` if the slot is empty (which is distinct
		/// from a slot that explicitly stores an empty byte sequence).
		pub value: Option<Vec<u8>>
	}
}
