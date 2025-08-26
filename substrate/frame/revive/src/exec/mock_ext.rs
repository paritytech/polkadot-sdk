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

#![cfg(test)]

use crate::{
	exec::{AccountIdOf, ExecError, Ext, Key, Origin, PrecompileExt, PrecompileWithInfoExt},
	gas::GasMeter,
	precompiles::Diff,
	storage::{ContractInfo, WriteOutcome},
	transient_storage::TransientStorage,
	Config, ExecReturnValue, ImmutableData,
};
use alloc::vec::Vec;
use core::marker::PhantomData;
use frame_support::{dispatch::DispatchResult, weights::Weight};
use sp_core::{H160, H256, U256};
use sp_runtime::DispatchError;

/// Mock implementation of the Ext trait that panics for all methods
pub struct MockExt<T: Config> {
	gas_meter: GasMeter<T>,
	_phantom: PhantomData<T>,
}

impl<T: Config> PrecompileExt for MockExt<T> {
	type T = T;

	fn call(
		&mut self,
		_gas_limit: Weight,
		_deposit_limit: U256,
		_to: &H160,
		_value: U256,
		_input_data: Vec<u8>,
		_allows_reentry: bool,
		_read_only: bool,
	) -> Result<(), ExecError> {
		panic!("MockExt::call")
	}

	fn get_transient_storage(&self, _key: &Key) -> Option<Vec<u8>> {
		panic!("MockExt::get_transient_storage")
	}

	fn get_transient_storage_size(&self, _key: &Key) -> Option<u32> {
		panic!("MockExt::get_transient_storage_size")
	}

	fn set_transient_storage(
		&mut self,
		_key: &Key,
		_value: Option<Vec<u8>>,
		_take_old: bool,
	) -> Result<WriteOutcome, DispatchError> {
		panic!("MockExt::set_transient_storage")
	}

	fn caller(&self) -> Origin<Self::T> {
		panic!("MockExt::caller")
	}

	fn origin(&self) -> &Origin<Self::T> {
		panic!("MockExt::origin")
	}

	fn code_hash(&self, _address: &H160) -> H256 {
		panic!("MockExt::code_hash")
	}

	fn code_size(&self, _address: &H160) -> u64 {
		panic!("MockExt::code_size")
	}

	fn caller_is_origin(&self) -> bool {
		panic!("MockExt::caller_is_origin")
	}

	fn caller_is_root(&self) -> bool {
		panic!("MockExt::caller_is_root")
	}

	fn account_id(&self) -> &AccountIdOf<Self::T> {
		panic!("MockExt::account_id")
	}

	fn balance(&self) -> U256 {
		panic!("MockExt::balance")
	}

	fn balance_of(&self, _address: &H160) -> U256 {
		panic!("MockExt::balance_of")
	}

	fn value_transferred(&self) -> U256 {
		panic!("MockExt::value_transferred")
	}

	fn now(&self) -> U256 {
		panic!("MockExt::now")
	}

	fn minimum_balance(&self) -> U256 {
		panic!("MockExt::minimum_balance")
	}

	fn deposit_event(&mut self, _topics: Vec<H256>, _data: Vec<u8>) {
		panic!("MockExt::deposit_event")
	}

	fn block_number(&self) -> U256 {
		panic!("MockExt::block_number")
	}

	fn block_hash(&self, _block_number: U256) -> Option<H256> {
		panic!("MockExt::block_hash")
	}

	fn block_author(&self) -> Option<H160> {
		panic!("MockExt::block_author")
	}

	fn max_value_size(&self) -> u32 {
		panic!("MockExt::max_value_size")
	}

	fn get_weight_price(&self, _weight: Weight) -> U256 {
		panic!("MockExt::get_weight_price")
	}

	fn gas_meter(&self) -> &GasMeter<Self::T> {
		&self.gas_meter
	}

	fn gas_meter_mut(&mut self) -> &mut GasMeter<Self::T> {
		&mut self.gas_meter
	}

	fn ecdsa_recover(
		&self,
		_signature: &[u8; 65],
		_message_hash: &[u8; 32],
	) -> Result<[u8; 33], ()> {
		panic!("MockExt::ecdsa_recover")
	}

	fn sr25519_verify(&self, _signature: &[u8; 64], _message: &[u8], _pub_key: &[u8; 32]) -> bool {
		panic!("MockExt::sr25519_verify")
	}

	fn ecdsa_to_eth_address(&self, _pk: &[u8; 33]) -> Result<[u8; 20], ()> {
		panic!("MockExt::ecdsa_to_eth_address")
	}

	#[cfg(any(test, feature = "runtime-benchmarks"))]
	fn contract_info(&mut self) -> &mut ContractInfo<Self::T> {
		panic!("MockExt::contract_info")
	}

	#[cfg(any(feature = "runtime-benchmarks", test))]
	fn transient_storage(&mut self) -> &mut TransientStorage<Self::T> {
		panic!("MockExt::transient_storage")
	}

	fn is_read_only(&self) -> bool {
		panic!("MockExt::is_read_only")
	}

	fn last_frame_output(&self) -> &ExecReturnValue {
		panic!("MockExt::last_frame_output")
	}

	fn last_frame_output_mut(&mut self) -> &mut ExecReturnValue {
		panic!("MockExt::last_frame_output_mut")
	}
}

impl<T: Config> PrecompileWithInfoExt for MockExt<T> {
	fn get_storage(&mut self, _key: &Key) -> Option<Vec<u8>> {
		panic!("MockExt::get_storage")
	}

	fn get_storage_size(&mut self, _key: &Key) -> Option<u32> {
		panic!("MockExt::get_storage_size")
	}

	fn set_storage(
		&mut self,
		_key: &Key,
		_value: Option<Vec<u8>>,
		_take_old: bool,
	) -> Result<WriteOutcome, DispatchError> {
		panic!("MockExt::set_storage")
	}

	fn charge_storage(&mut self, _diff: &Diff) {}

	fn instantiate(
		&mut self,
		_gas_limit: Weight,
		_deposit_limit: U256,
		_code: H256,
		_value: U256,
		_input_data: Vec<u8>,
		_salt: Option<&[u8; 32]>,
	) -> Result<H160, ExecError> {
		panic!("MockExt::instantiate")
	}
}

impl<T: Config> Ext for MockExt<T> {
	fn delegate_call(
		&mut self,
		_gas_limit: Weight,
		_deposit_limit: U256,
		_address: H160,
		_input_data: Vec<u8>,
	) -> Result<(), ExecError> {
		panic!("MockExt::delegate_call")
	}

	fn terminate(&mut self, _beneficiary: &H160) -> DispatchResult {
		panic!("MockExt::terminate")
	}

	fn own_code_hash(&mut self) -> &H256 {
		panic!("MockExt::own_code_hash")
	}

	fn set_code_hash(&mut self, _hash: H256) -> DispatchResult {
		panic!("MockExt::set_code_hash")
	}

	fn immutable_data_len(&mut self) -> u32 {
		panic!("MockExt::immutable_data_len")
	}

	fn get_immutable_data(&mut self) -> Result<ImmutableData, DispatchError> {
		panic!("MockExt::get_immutable_data")
	}

	fn set_immutable_data(&mut self, _data: ImmutableData) -> Result<(), DispatchError> {
		panic!("MockExt::set_immutable_data")
	}
}
