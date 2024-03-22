#![allow(dead_code)]
use super::{AccountId32, Test, ALICE, GAS_LIMIT};
use crate::{
	tests::RuntimeOrigin, AccountIdLookupOf, AccountIdOf, BalanceOf, Code, CodeHash, CollectEvents,
	ContractExecResult, ContractInstantiateResult, DebugInfo, Determinism, EventRecordOf,
	ExecReturnValue, OriginFor, Pallet, Weight,
};
use codec::Compact;
use frame_support::pallet_prelude::DispatchResultWithPostInfo;

macro_rules! builder {
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
InstantiateWithCodeBuilder,
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
InstantiateBuilder,
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
	BareInstantiateBuilder,
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
	CallBuilder,
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
	BareCallBuilder,
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
	pub fn build_and_unwrap_result(self) -> crate::InstantiateReturnValue<AccountIdOf<Test>> {
		self.build().result.unwrap()
	}

	pub fn build_and_unwrap_account_id(self) -> AccountIdOf<Test> {
		self.build().result.unwrap().account_id
	}
}

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
	pub fn build_and_unwrap_result(self) -> ExecReturnValue {
		self.build().result.unwrap()
	}
}

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
