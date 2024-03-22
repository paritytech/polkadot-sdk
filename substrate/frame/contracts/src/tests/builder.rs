#![allow(dead_code)]
use super::{AccountId32, Test, ALICE, GAS_LIMIT};
use crate::{
	BalanceOf, Code, CodeHash, CollectEvents, Config, ContractExecResult,
	ContractInstantiateResult, DebugInfo, Determinism, EventRecordOf, ExecReturnValue, Pallet,
	Weight,
};

macro_rules! builder {
	(
		$name:ident,
		$method:ident(
			$($field:ident: $type:ty,)*
		) -> $result:ty
	) => {
		#[doc = concat!("A builder to construct a ", stringify!($method), " call")]
		pub struct $name<T: Config> {
			$($field: $type,)*
		}

		impl<T: Config> $name<T> {
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
		}
	}
}

builder!(
	BareInstantiateBuilder,
	bare_instantiate(
		origin: T::AccountId,
		value: BalanceOf<T>,
		gas_limit: Weight,
		storage_deposit_limit: Option<BalanceOf<T>>,
		code: Code<CodeHash<T>>,
		data: Vec<u8>,
		salt: Vec<u8>,
		debug: DebugInfo,
		collect_events: CollectEvents,
	) -> ContractInstantiateResult<T::AccountId, BalanceOf<T>, EventRecordOf<T>>
);

builder!(
	BareCallBuilder,
	bare_call(
		origin: T::AccountId,
		dest: T::AccountId,
		value: BalanceOf<T>,
		gas_limit: Weight,
		storage_deposit_limit: Option<BalanceOf<T>>,
		data: Vec<u8>,
		debug: DebugInfo,
		collect_events: CollectEvents,
		determinism: Determinism,
	) -> ContractExecResult<BalanceOf<T>, EventRecordOf<T>>
);

/// Create a new instantiate builder.
pub fn instantiate(code: Code<CodeHash<Test>>) -> BareInstantiateBuilder<Test> {
	BareInstantiateBuilder::<Test> {
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

impl<T: Config> BareInstantiateBuilder<T> {
	pub fn build_and_unwrap_result(self) -> crate::InstantiateReturnValue<T::AccountId> {
		self.build().result.unwrap()
	}

	pub fn build_and_unwrap_account_id(self) -> T::AccountId {
		self.build().result.unwrap().account_id
	}
}

/// Create a new call builder.
pub fn bare_call(dest: AccountId32) -> BareCallBuilder<Test> {
	BareCallBuilder::<Test> {
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
impl<T: Config> BareCallBuilder<T> {
	pub fn build_and_unwrap_result(self) -> ExecReturnValue {
		self.build().result.unwrap()
	}
}
