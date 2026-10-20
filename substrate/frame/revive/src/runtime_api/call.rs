// This file is part of Substrate.

// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//  http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

use alloc::vec::Vec;
use frame_support::traits::{Get, OriginTrait};
use frame_system::pallet_prelude::OriginFor;
use pallet_revive_types::runtime_api::*;
use sp_core::H160;
use sp_runtime::traits::Bounded;

use crate::{
	BalanceOf, Config, ContractResult, ExecConfig, ExecReturnValue, Pallet, TransactionLimits,
	Weight,
};

pub struct CallInputPayload<AccountId, Balance> {
	pub origin: AccountId,
	pub dest: H160,
	pub value: Balance,
	pub gas_limit: Option<Weight>,
	pub storage_deposit_limit: Option<Balance>,
	pub input_data: Vec<u8>,
}

impl<AccountId, Balance> From<CallVersionedInputPayload<AccountId, Balance>>
	for CallInputPayload<AccountId, Balance>
{
	fn from(value: CallVersionedInputPayload<AccountId, Balance>) -> Self {
		match value {
			CallVersionedInputPayload::V1(payload) => payload.into(),
		}
	}
}

impl<AccountId, Balance> From<CallInputPayloadV1<AccountId, Balance>>
	for CallInputPayload<AccountId, Balance>
{
	fn from(value: CallInputPayloadV1<AccountId, Balance>) -> Self {
		Self {
			origin: value.origin,
			dest: value.dest,
			value: value.value,
			gas_limit: value.gas_limit,
			storage_deposit_limit: value.storage_deposit_limit,
			input_data: value.input_data,
		}
	}
}

pub struct CallOutputPayload<Balance> {
	pub contract_result: ContractResult<ExecReturnValue, Balance>,
}

impl<Balance> From<CallOutputPayload<Balance>> for CallOutputPayloadV1<Balance> {
	fn from(value: CallOutputPayload<Balance>) -> Self {
		Self { contract_result: value.contract_result.into() }
	}
}

/// Dry runs a call of the `call` dispatchable.
///
/// A missing limit means the block's maximum weight, or no deposit limit.
pub fn dry_run_call<T: Config>(
	input: CallInputPayload<T::AccountId, BalanceOf<T>>,
) -> CallOutputPayload<BalanceOf<T>> {
	Pallet::<T>::prepare_dry_run(&input.origin);
	let contract_result = Pallet::<T>::bare_call(
		OriginFor::<T>::signed(input.origin),
		input.dest,
		Pallet::<T>::convert_native_to_evm(input.value),
		TransactionLimits::WeightAndDeposit {
			weight_limit: input.gas_limit.unwrap_or(T::BlockWeights::get().max_block),
			deposit_limit: input.storage_deposit_limit.unwrap_or(BalanceOf::<T>::max_value()),
		},
		input.input_data,
		&ExecConfig::new_substrate_tx().with_first_frame_reads_paid().with_dry_run(None),
	);
	CallOutputPayload { contract_result }
}
