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

use super::{deposit_limit, GAS_LIMIT};
use crate::{
	address::AddressMapper, AccountIdOf, BalanceOf, BumpNonce, Code, Config, ContractResult,
	DepositLimit, ExecReturnValue, InstantiateReturnValue, OriginFor, Pallet, Weight, U256,
};
use alloc::{vec, vec::Vec};
use frame_support::pallet_prelude::DispatchResultWithPostInfo;
use paste::paste;
use sp_core::H160;

/// Helper macro to generate a builder for contract API calls.
macro_rules! builder {
	// Entry point to generate a builder for the given method.
	(
		$method:ident($($field:ident: $type:ty,)*) -> $result:ty;
        $($extra:item)*
	) => {
		paste!{
			builder!([< $method:camel Builder >], $method($($field: $type,)* ) -> $result; $($extra)*);
		}
	};
	// Generate the builder struct and its methods.
	(
		$name:ident,
		$method:ident($($field:ident: $type:ty,)*) -> $result:ty;
        $($extra:item)*
	) => {
		#[doc = concat!("A builder to construct a ", stringify!($method), " call")]
		pub struct $name<T: Config> {
			$($field: $type,)*
		}

		#[allow(dead_code)]
		impl<T: Config> $name<T>
		where
			BalanceOf<T>: Into<sp_core::U256> + TryFrom<sp_core::U256>,
			crate::MomentOf<T>: Into<sp_core::U256>,
			T::Hash: frame_support::traits::IsType<sp_core::H256>,
		{
			$(
				#[doc = concat!("Set the ", stringify!($field))]
				pub fn $field(mut self, value: $type) -> Self {
					self.$field = value;
					self
				}
			)*

			#[doc = concat!("Build the ", stringify!($method), " call")]
			pub fn build(self) -> $result {
				Pallet::<T>::$method(
					$(self.$field,)*
				)
			}

            $($extra)*
		}
	}
}

pub struct Contract<T: Config> {
	pub account_id: AccountIdOf<T>,
	pub addr: H160,
}

builder!(
	instantiate_with_code(
		origin: OriginFor<T>,
		value: BalanceOf<T>,
		gas_limit: Weight,
		storage_deposit_limit: BalanceOf<T>,
		code: Vec<u8>,
		data: Vec<u8>,
		salt: Option<[u8; 32]>,
	) -> DispatchResultWithPostInfo;

	/// Create an [`InstantiateWithCodeBuilder`] with default values.
	pub fn instantiate_with_code(origin: OriginFor<T>, code: Vec<u8>) -> Self {
		Self {
			origin,
			value: 0u32.into(),
			gas_limit: GAS_LIMIT,
			storage_deposit_limit: deposit_limit::<T>(),
			code,
			data: vec![],
			salt: Some([0; 32]),
		}
	}
);

builder!(
	instantiate(
		origin: OriginFor<T>,
		value: BalanceOf<T>,
		gas_limit: Weight,
		storage_deposit_limit: BalanceOf<T>,
		code_hash: sp_core::H256,
		data: Vec<u8>,
		salt: Option<[u8; 32]>,
	) -> DispatchResultWithPostInfo;

	/// Create an [`InstantiateBuilder`] with default values.
	pub fn instantiate(origin: OriginFor<T>, code_hash: sp_core::H256) -> Self {
		Self {
			origin,
			value: 0u32.into(),
			gas_limit: GAS_LIMIT,
			storage_deposit_limit: deposit_limit::<T>(),
			code_hash,
			data: vec![],
			salt: Some([0; 32]),
		}
	}
);

builder!(
	bare_instantiate(
		origin: OriginFor<T>,
		evm_value: U256,
		gas_limit: Weight,
		storage_deposit_limit: DepositLimit<BalanceOf<T>>,
		code: Code,
		data: Vec<u8>,
		salt: Option<[u8; 32]>,
		bump_nonce: BumpNonce,
	) -> ContractResult<InstantiateReturnValue, BalanceOf<T>>;

	/// Set the call's evm_value using a native_value amount.
	pub fn native_value(mut self, value: BalanceOf<T>) -> Self {
		self.evm_value = Pallet::<T>::convert_native_to_evm(value);
		self
	}

	/// Build the instantiate call and unwrap the result.
	pub fn build_and_unwrap_result(self) -> InstantiateReturnValue {
		self.build().result.unwrap()
	}

	/// Build the instantiate call and unwrap the account id.
	pub fn build_and_unwrap_contract(self) -> Contract<T> {
		let result = self.build().result.unwrap();
		assert!(!result.result.did_revert(), "instantiation did revert");

		let addr = result.addr;
		let account_id = T::AddressMapper::to_account_id(&addr);
		Contract{ account_id,  addr }
	}

	/// Create a [`BareInstantiateBuilder`] with default values.
	pub fn bare_instantiate(origin: OriginFor<T>, code: Code) -> Self {
		Self {
			origin,
			evm_value: Default::default(),
			gas_limit: GAS_LIMIT,
			storage_deposit_limit: DepositLimit::Balance(deposit_limit::<T>()),
			code,
			data: vec![],
			salt: Some([0; 32]),
			bump_nonce: BumpNonce::Yes,
		}
	}
);

builder!(
	call(
		origin: OriginFor<T>,
		dest: H160,
		value: BalanceOf<T>,
		gas_limit: Weight,
		storage_deposit_limit: BalanceOf<T>,
		data: Vec<u8>,
	) -> DispatchResultWithPostInfo;

	/// Create a [`CallBuilder`] with default values.
	pub fn call(origin: OriginFor<T>, dest: H160) -> Self {
		CallBuilder {
			origin,
			dest,
			value: 0u32.into(),
			gas_limit: GAS_LIMIT,
			storage_deposit_limit: deposit_limit::<T>(),
			data: vec![],
		}
	}
);

builder!(
	bare_call(
		origin: OriginFor<T>,
		dest: H160,
		evm_value: U256,
		gas_limit: Weight,
		storage_deposit_limit: DepositLimit<BalanceOf<T>>,
		data: Vec<u8>,
	) -> ContractResult<ExecReturnValue, BalanceOf<T>>;

	/// Set the call's evm_value using a native_value amount.
	pub fn native_value(mut self, value: BalanceOf<T>) -> Self {
		self.evm_value = Pallet::<T>::convert_native_to_evm(value);
		self
	}

	/// Build the call and unwrap the result.
	pub fn build_and_unwrap_result(self) -> ExecReturnValue {
		self.build().result.unwrap()
	}

	/// Create a [`BareCallBuilder`] with default values.
	pub fn bare_call(origin: OriginFor<T>, dest: H160) -> Self {
		Self {
			origin,
			dest,
			evm_value: Default::default(),
			gas_limit: GAS_LIMIT,
			storage_deposit_limit: DepositLimit::Balance(deposit_limit::<T>()),
			data: vec![],
		}
	}
);

builder!(
	eth_call(
		origin: OriginFor<T>,
		dest: H160,
		value: U256,
		gas_limit: Weight,
		storage_deposit_limit: BalanceOf<T>,
		data: Vec<u8>,
	) -> DispatchResultWithPostInfo;

	/// Create a [`EthCallBuilder`] with default values.
	pub fn eth_call(origin: OriginFor<T>, dest: H160) -> Self {
		Self {
			origin,
			dest,
			value: 0u32.into(),
			gas_limit: GAS_LIMIT,
			storage_deposit_limit: deposit_limit::<T>(),
			data: vec![],
		}
	}
);
