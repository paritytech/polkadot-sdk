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
use sp_runtime::DispatchError;
use sp_weights::Weight;

use crate::{CodeV1, InstantiateReturnValueV1, StorageDepositV1};

define_versioned_interface! {
	#[derive(Debug, Clone, Eq, PartialEq, TypeInfo, Encode, Decode)]
	pub struct InstantiateVersionedInputPayloadV1<AccountId, Balance> {
		pub origin: AccountId,
		pub value: Balance,
		pub gas_limit: Option<Weight>,
		pub storage_deposit_limit: Option<Balance>,
		pub code: CodeV1,
		pub data: Vec<u8>,
		pub salt: Option<[u8; 32]>
	}

	#[derive(Debug, Clone, Eq, PartialEq, TypeInfo, Encode, Decode)]
	pub struct InstantiateVersionedOutputPayloadV1<Balance> {
		pub weight_consumed: Weight,
		pub weight_required: Weight,
		pub storage_deposit: StorageDepositV1<Balance>,
		pub max_storage_deposit: StorageDepositV1<Balance>,
		pub gas_consumed: Balance,
		pub result: Result<InstantiateReturnValueV1, DispatchError>
	}
}
