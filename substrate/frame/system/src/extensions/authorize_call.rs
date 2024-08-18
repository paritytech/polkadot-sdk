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

use crate::Config;
use frame_support::{
	dispatch::DispatchInfo,
	pallet_prelude::{Decode, DispatchResult, Encode, TypeInfo, Weight},
	traits::{Authorize, OriginTrait},
	CloneNoBound, EqNoBound, PartialEqNoBound, RuntimeDebugNoBound,
};
use sp_runtime::{
	traits::{
		transaction_extension::TransactionExtensionBase, Dispatchable, PostDispatchInfoOf,
		TransactionExtension, ValidateResult,
	},
	transaction_validity::TransactionValidityError,
};

#[derive(
	Encode, Decode, CloneNoBound, EqNoBound, PartialEqNoBound, TypeInfo, RuntimeDebugNoBound,
)]
#[scale_info(skip_type_params(T))]
pub struct AuthorizeCall<T>(core::marker::PhantomData<T>);

impl<T> AuthorizeCall<T> {
	pub fn new() -> Self {
		Self(Default::default())
	}
}

impl<T: Config + Send + Sync> TransactionExtensionBase for AuthorizeCall<T> {
	const IDENTIFIER: &'static str = "AuthorizeCall";
	type Implicit = ();
	fn weight() -> Weight {
		<T::RuntimeCall as Authorize>::weight_of_authorize()
	}
}

impl<T: Config + Send + Sync> TransactionExtension<T::RuntimeCall> for AuthorizeCall<T>
where
	T::RuntimeCall: Dispatchable<Info = DispatchInfo>,
{
	type Val = ();
	type Pre = Weight;

	fn validate(
		&self,
		origin: T::RuntimeOrigin,
		call: &T::RuntimeCall,
		_info: &DispatchInfo,
		_len: usize,
		_self_implicit: Self::Implicit,
		_inherited_implication: &impl Encode,
	) -> ValidateResult<Self::Val, T::RuntimeCall> {
		if origin.as_system_ref().map_or(false, |system_origin| system_origin.is_none()) {
			if let Some(authorize) = call.authorize() {
				return authorize.map(|(validity, result_origin)| (validity, (), result_origin))
			}
		}

		Ok((Default::default(), (), origin))
	}

	fn prepare(
		self,
		_val: Self::Val,
		_origin: &T::RuntimeOrigin,
		call: &T::RuntimeCall,
		_info: &DispatchInfo,
		_len: usize,
	) -> Result<Self::Pre, TransactionValidityError> {
		Ok(call.accurate_weight_of_authorize())
	}

	fn post_dispatch_details(
		pre: Self::Pre,
		_info: &DispatchInfo,
		_post_info: &PostDispatchInfoOf<T::RuntimeCall>,
		_len: usize,
		_result: &DispatchResult,
	) -> Result<Option<Weight>, TransactionValidityError> {
		Ok(Some(pre))
	}
}

// #[cfg(test)]
// mod tests {
// 	use codec::Encode;
// 	use frame_support::{derive_impl, dispatch::GetDispatchInfo, pallet_prelude::{TransactionSource,
// Weight}, }; 	use sp_runtime::traits::Checkable;
// 	use sp_runtime::traits::Applyable;
// 	use sp_runtime::BuildStorage;
// 	use sp_runtime::transaction_validity::TransactionValidity;
// 	use sp_runtime::transaction_validity::ValidTransaction;
// 	use sp_runtime::transaction_validity::TransactionValidityError;
// 	use sp_runtime::transaction_validity::InvalidTransaction;
// 	use sp_runtime::DispatchError;
// 	use crate as frame_system;

// 	#[frame_support::pallet]
// 	pub mod pallet1 {
// 		use crate as frame_system;
// 		use frame_support::pallet_prelude::*;
// 		use frame_system::pallet_prelude::*;

// 		pub const AUTH_WEIGHT: Weight = Weight::from_all(5);
// 		pub const AUTH_ACCURATE_WEIGHT: Weight = Weight::from_all(3);

// 		pub const VALID_TRANSACTION: ValidTransaction = ValidTransaction {
// 			priority: 10,
// 			provides: vec![10u8.encode()],
// 			requires: vec![],
// 			longevity: 1000,
// 			propagate: true,
// 		};

// 		#[pallet::pallet]
// 		pub struct Pallet<T, I = ()>(_);

// 		#[pallet::config]
// 		pub trait Config<I: 'static = ()>: frame_system::Config {}

// 		#[pallet::call]
// 		impl<T: Config<I>, I: 'static> Pallet<T, I> {
// 			#[pallet::weight(Weight::from_all(1010))]
// 			#[pallet::call_index(0)]
// 			pub fn call1(origin: OriginFor<T>, valid: bool) -> DispatchResult {
// 				Ok(())
// 			}
// 		}

// 		impl<T: Config> Authorize for Call<T> {
// 			type RuntimeOrigin = frame_system::OriginFor<T>;

// 			fn authorize(
// 				&self,
// 			) -> Option<Result<(ValidTransaction, Self::RuntimeOrigin), TransactionValidityError>> {
// 				match Call {
// 					Call::call1 { valid, .. } => {
// 						if valid {
// 							Some(Ok((VALID_TRANSACTION, crate::Origin::<T>::signed(42).into())))
// 						} else {
// 							Some(Err(TransactionValidityError::Invalid(InvalidTransaction::Call)))
// 						}
// 					}
// 				}
// 			}

// 			fn weight_of_authorize() -> Weight {
// 				AUTH_WEIGHT
// 			}

// 			fn accurate_weight_of_authorize(&self) -> Weight {
// 				AUTH_ACCURATE_WEIGHT
// 			}
// 		}
// 	}

// 	#[frame_support::runtime]
// 	mod runtime {
// 		#[runtime::runtime]
// 		#[runtime::derive(
// 			RuntimeCall,
// 			RuntimeEvent,
// 			RuntimeError,
// 			RuntimeOrigin,
// 			RuntimeFreezeReason,
// 			RuntimeHoldReason,
// 			RuntimeSlashReason,
// 			RuntimeLockId,
// 			RuntimeTask
// 		)]
// 		pub struct Runtime;

// 		#[runtime::pallet_index(0)]
// 		pub type System = frame_system::Pallet<Runtime>;

// 		#[runtime::pallet_index(1)]
// 		pub type Pallet1 = pallet1::Pallet<Runtime>;
// 	}

// 	pub type TransactionExtension = (
// 		frame_system::AuthorizeCall<Runtime>,
// 	);

// 	pub type Header = sp_runtime::generic::Header<u32, sp_runtime::traits::BlakeTwo256>;
// 	pub type Block = sp_runtime::generic::Block<Header, UncheckedExtrinsic>;
// 	pub type UncheckedExtrinsic = sp_runtime::generic::UncheckedExtrinsic<
// 		u64,
// 		RuntimeCall,
// 		sp_runtime::testing::MockU64Signature,
// 		TransactionExtension,
// 	>;

// 	#[derive_impl(frame_system::config_preludes::TestDefaultConfig)]
// 	impl frame_system::Config for Runtime {
// 		type BaseCallFilter = frame_support::traits::Everything;
// 		type RuntimeOrigin = RuntimeOrigin;
// 		type Nonce = u64;
// 		type RuntimeCall = RuntimeCall;
// 		type Hash = sp_runtime::testing::H256;
// 		type Hashing = sp_runtime::traits::BlakeTwo256;
// 		type AccountId = u64;
// 		type Lookup = sp_runtime::traits::IdentityLookup<Self::AccountId>;
// 		type Block = Block;
// 		type RuntimeEvent = RuntimeEvent;
// 		type BlockWeights = ();
// 		type BlockLength = ();
// 		type DbWeight = ();
// 		type Version = ();
// 		type PalletInfo = PalletInfo;
// 		type AccountData = ();
// 		type OnNewAccount = ();
// 		type OnKilledAccount = ();
// 		type SystemWeightInfo = ();
// 		type SS58Prefix = ();
// 		type OnSetCode = ();
// 		type MaxConsumers = frame_support::traits::ConstU32<16>;
// 	}

// 	impl pallet1::Config for Runtime {}

// 	pub fn new_test_ext() -> sp_io::TestExternalities {
// 		let t = RuntimeGenesisConfig {
// 			..Default::default()
// 		}
// 			.build_storage()
// 			.unwrap();
// 		t.into()
// 	}

// 	#[test]
// 	fn test() {
// 		// TODO TODO
// 		// new_test_ext().execute_with(|| {
// 		// 	let tx_ext = (
// 		// 		frame_system::AuthorizeCall::<Runtime>::new(),
// 		// 	);

// // 			let tx = UncheckedExtrinsic::new_transaction(call, tx_ext);

// // 			let info = tx.get_dispatch_info();
// // 			let len = tx.using_encoded(|e| e.len());

// // 			let checked = Checkable::check(tx, &frame_system::ChainContext::<Runtime>::default())
// // 				.expect("Transaction is general so signature is good");

// // 			checked.validate::<Runtime>(TransactionSource::External, &info, len)
// // 				.expect("call1 is always valid");

// // 			let dispatch_result = checked.apply::<Runtime>(&info, len)
// // 				.expect("Transaction is valid");

// // 			assert_eq!(dispatch_result.is_ok(), dispatch_success);

// // 			let post_info = dispatch_result
// // 				.unwrap_or_else(|e| e.post_info);

// // 			assert_eq!(info.call_weight, call_weight);
// // 			assert_eq!(info.extension_weight, ext_weight);
// // 			assert_eq!(post_info.actual_weight, Some(actual_weight));
// 		// });
// 	}
// }
