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

use super::GAS_LIMIT;
use crate::{
	AccountIdLookupOf, AccountIdOf, BalanceOf, Code, CodeHash, CollectEvents, Config,
	ContractExecResult, ContractInstantiateResult, DebugInfo, Determinism, EventRecordOf,
	ExecReturnValue, InstantiateReturnValue, OriginFor, Pallet, Weight,
};
use codec::{Encode, HasCompact};
use core::fmt::Debug;
use frame_support::pallet_prelude::DispatchResultWithPostInfo;
use paste::paste;
use scale_info::TypeInfo;

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
			<BalanceOf<T> as HasCompact>::Type: Clone + Eq + PartialEq + Debug + TypeInfo + Encode,
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

builder!(
	instantiate_with_code(
		origin: OriginFor<T>,
		value: BalanceOf<T>,
		gas_limit: Weight,
		storage_deposit_limit: Option<<BalanceOf<T> as codec::HasCompact>::Type>,
		code: Vec<u8>,
		data: Vec<u8>,
		salt: Vec<u8>,
	) -> DispatchResultWithPostInfo;

	/// Create an [`InstantiateWithCodeBuilder`] with default values.
	pub fn instantiate_with_code(origin: OriginFor<T>, code: Vec<u8>) -> Self {
		Self {
			origin: origin,
			value: 0u32.into(),
			gas_limit: GAS_LIMIT,
			storage_deposit_limit: None,
			code,
			data: vec![],
			salt: vec![],
		}
	}
);

builder!(
	instantiate(
		origin: OriginFor<T>,
		value: BalanceOf<T>,
		gas_limit: Weight,
		storage_deposit_limit: Option<<BalanceOf<T> as codec::HasCompact>::Type>,
		code_hash: CodeHash<T>,
		data: Vec<u8>,
		salt: Vec<u8>,
	) -> DispatchResultWithPostInfo;

	/// Create an [`InstantiateBuilder`] with default values.
	pub fn instantiate(origin: OriginFor<T>, code_hash: CodeHash<T>) -> Self {
		Self {
			origin,
			value: 0u32.into(),
			gas_limit: GAS_LIMIT,
			storage_deposit_limit: None,
			code_hash,
			data: vec![],
			salt: vec![],
		}
	}
);

builder!(
	bare_instantiate(
		origin: AccountIdOf<T>,
		value: BalanceOf<T>,
		gas_limit: Weight,
		storage_deposit_limit: Option<BalanceOf<T>>,
		code: Code<CodeHash<T>>,
		data: Vec<u8>,
		salt: Vec<u8>,
		debug: DebugInfo,
		collect_events: CollectEvents,
	) -> ContractInstantiateResult<AccountIdOf<T>, BalanceOf<T>, EventRecordOf<T>>;

	/// Build the instantiate call and unwrap the result.
	pub fn build_and_unwrap_result(self) -> InstantiateReturnValue<AccountIdOf<T>> {
		self.build().result.unwrap()
	}

	/// Build the instantiate call and unwrap the account id.
	pub fn build_and_unwrap_account_id(self) -> AccountIdOf<T> {
		self.build().result.unwrap().account_id
	}

	pub fn bare_instantiate(origin: AccountIdOf<T>, code: Code<CodeHash<T>>) -> Self {
		Self {
			origin,
			value: 0u32.into(),
			gas_limit: GAS_LIMIT,
			storage_deposit_limit: None,
			code,
			data: vec![],
			salt: vec![],
			debug: DebugInfo::Skip,
			collect_events: CollectEvents::Skip,
		}
	}
);

builder!(
	call(
		origin: OriginFor<T>,
		dest: AccountIdLookupOf<T>,
		value: BalanceOf<T>,
		gas_limit: Weight,
		storage_deposit_limit: Option<<BalanceOf<T> as codec::HasCompact>::Type>,
		data: Vec<u8>,
	) -> DispatchResultWithPostInfo;

	/// Create a [`CallBuilder`] with default values.
	pub fn call(origin: OriginFor<T>, dest: AccountIdLookupOf<T>) -> Self {
		CallBuilder {
			origin,
			dest,
			value: 0u32.into(),
			gas_limit: GAS_LIMIT,
			storage_deposit_limit: None,
			data: vec![],
		}
	}
);

builder!(
	bare_call(
		origin: AccountIdOf<T>,
		dest: AccountIdOf<T>,
		value: BalanceOf<T>,
		gas_limit: Weight,
		storage_deposit_limit: Option<BalanceOf<T>>,
		data: Vec<u8>,
		debug: DebugInfo,
		collect_events: CollectEvents,
		determinism: Determinism,
	) -> ContractExecResult<BalanceOf<T>, EventRecordOf<T>>;

	/// Build the call and unwrap the result.
	pub fn build_and_unwrap_result(self) -> ExecReturnValue {
		self.build().result.unwrap()
	}

	/// Create a [`BareCallBuilder`] with default values.
	pub fn bare_call(origin: AccountIdOf<T>, dest: AccountIdOf<T>) -> Self {
		Self {
			origin,
			dest,
			value: 0u32.into(),
			gas_limit: GAS_LIMIT,
			storage_deposit_limit: None,
			data: vec![],
			debug: DebugInfo::Skip,
			collect_events: CollectEvents::Skip,
			determinism: Determinism::Enforced,
		}
	}
);
