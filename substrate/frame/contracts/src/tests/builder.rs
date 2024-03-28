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

use super::{AccountId32, Test, ALICE, GAS_LIMIT};
use crate::{
	tests::RuntimeOrigin, AccountIdLookupOf, AccountIdOf, BalanceOf, Code, CodeHash, CollectEvents,
	ContractExecResult, ContractInstantiateResult, DebugInfo, Determinism, EventRecordOf,
	ExecReturnValue, OriginFor, Pallet, Weight,
};
use codec::Compact;
use frame_support::pallet_prelude::DispatchResultWithPostInfo;
use paste::paste;

/// Helper macro to generate a builder for contract API calls.
macro_rules! builder {
	// Entry point to generate a builder for the given method.
	(
		$method:ident($($field:ident: $type:ty,)*) -> $result:ty
	) => {
		paste!{
			builder!([< $method:camel Builder >], $method($($field: $type,)* ) -> $result);
		}
	};
	// Generate the builder struct and its methods.
	(
		$name:ident,
		$method:ident(
			$($field:ident: $type:ty,)*
		) -> $result:ty
	) => {
		#[doc = concat!("A builder to construct a ", stringify!($method), " call")]
		pub struct $name {
			$($field: $type,)*
		}

		#[allow(dead_code)]
		impl $name
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
				Pallet::<Test>::$method(
					$(self.$field,)*
				)
			}
		}
	}
}

builder!(
	instantiate_with_code(
		origin: OriginFor<Test>,
		value: BalanceOf<Test>,
		gas_limit: Weight,
		storage_deposit_limit: Option<Compact<BalanceOf<Test>>>,
		code: Vec<u8>,
		data: Vec<u8>,
		salt: Vec<u8>,
	) -> DispatchResultWithPostInfo
);

builder!(
	instantiate(
		origin: OriginFor<Test>,
		value: BalanceOf<Test>,
		gas_limit: Weight,
		storage_deposit_limit: Option<Compact<BalanceOf<Test>>>,
		code_hash: CodeHash<Test>,
		data: Vec<u8>,
		salt: Vec<u8>,
	) -> DispatchResultWithPostInfo
);

builder!(
	bare_instantiate(
		origin: AccountIdOf<Test>,
		value: BalanceOf<Test>,
		gas_limit: Weight,
		storage_deposit_limit: Option<BalanceOf<Test>>,
		code: Code<CodeHash<Test>>,
		data: Vec<u8>,
		salt: Vec<u8>,
		debug: DebugInfo,
		collect_events: CollectEvents,
	) -> ContractInstantiateResult<AccountIdOf<Test>, BalanceOf<Test>, EventRecordOf<Test>>
);

builder!(
	call(
		origin: OriginFor<Test>,
		dest: AccountIdLookupOf<Test>,
		value: BalanceOf<Test>,
		gas_limit: Weight,
		storage_deposit_limit: Option<Compact<BalanceOf<Test>>>,
		data: Vec<u8>,
	) -> DispatchResultWithPostInfo
);

builder!(
	bare_call(
		origin: AccountIdOf<Test>,
		dest: AccountIdOf<Test>,
		value: BalanceOf<Test>,
		gas_limit: Weight,
		storage_deposit_limit: Option<BalanceOf<Test>>,
		data: Vec<u8>,
		debug: DebugInfo,
		collect_events: CollectEvents,
		determinism: Determinism,
	) -> ContractExecResult<BalanceOf<Test>, EventRecordOf<Test>>
);

/// Create a [`BareInstantiateBuilder`] with default values.
pub fn bare_instantiate(code: Code<CodeHash<Test>>) -> BareInstantiateBuilder {
	BareInstantiateBuilder {
		origin: ALICE,
		value: 0,
		gas_limit: GAS_LIMIT,
		storage_deposit_limit: None,
		code,
		data: vec![],
		salt: vec![],
		debug: DebugInfo::Skip,
		collect_events: CollectEvents::Skip,
	}
}

impl BareInstantiateBuilder {
	/// Build the instantiate call and unwrap the result.
	pub fn build_and_unwrap_result(self) -> crate::InstantiateReturnValue<AccountIdOf<Test>> {
		self.build().result.unwrap()
	}

	/// Build the instantiate call and unwrap the account id.
	pub fn build_and_unwrap_account_id(self) -> AccountIdOf<Test> {
		self.build().result.unwrap().account_id
	}
}

/// Create a [`BareCallBuilder`] with default values.
pub fn bare_call(dest: AccountId32) -> BareCallBuilder {
	BareCallBuilder {
		origin: ALICE,
		dest,
		value: 0,
		gas_limit: GAS_LIMIT,
		storage_deposit_limit: None,
		data: vec![],
		debug: DebugInfo::Skip,
		collect_events: CollectEvents::Skip,
		determinism: Determinism::Enforced,
	}
}

impl BareCallBuilder {
	/// Build the call and unwrap the result.
	pub fn build_and_unwrap_result(self) -> ExecReturnValue {
		self.build().result.unwrap()
	}
}

/// Create an [`InstantiateWithCodeBuilder`] with default values.
pub fn instantiate_with_code(code: Vec<u8>) -> InstantiateWithCodeBuilder {
	InstantiateWithCodeBuilder {
		origin: RuntimeOrigin::signed(ALICE),
		value: 0,
		gas_limit: GAS_LIMIT,
		storage_deposit_limit: None,
		code,
		data: vec![],
		salt: vec![],
	}
}

/// Create an [`InstantiateBuilder`] with default values.
pub fn instantiate(code_hash: CodeHash<Test>) -> InstantiateBuilder {
	InstantiateBuilder {
		origin: RuntimeOrigin::signed(ALICE),
		value: 0,
		gas_limit: GAS_LIMIT,
		storage_deposit_limit: None,
		code_hash,
		data: vec![],
		salt: vec![],
	}
}

/// Create a [`CallBuilder`] with default values.
pub fn call(dest: AccountIdLookupOf<Test>) -> CallBuilder {
	CallBuilder {
		origin: RuntimeOrigin::signed(ALICE),
		dest,
		value: 0,
		gas_limit: GAS_LIMIT,
		storage_deposit_limit: None,
		data: vec![],
	}
}
