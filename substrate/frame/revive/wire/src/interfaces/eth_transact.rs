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
use ethereum_types::U256;
use pallet_revive_proc_macro::define_versioned_interface;
use scale_info::TypeInfo;
use sp_weights::Weight;

use crate::{GenericTransactionV1, StateOverrideSetV1};

define_versioned_interface! {
	#[derive(Debug, Clone, Eq, PartialEq, TypeInfo, Encode, Decode)]
	pub struct EthTransactVersionedInputPayloadV1<Moment> {
		pub tx: GenericTransactionV1,
		pub timestamp_override: Option<Moment>,
		pub state_overrides: StateOverrideSetV1
	}

	#[derive(Debug, Clone, Eq, PartialEq, TypeInfo, Encode, Decode)]
	pub struct EthTransactVersionedOutputPayloadV1<Balance> {
		pub weight_required: Weight,
		pub storage_deposit: Balance,
		pub max_storage_deposit: Balance,
		pub eth_gas: U256,
		pub data: Vec<u8>
	}
}
