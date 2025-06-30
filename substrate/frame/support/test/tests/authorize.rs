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

use codec::Encode;
use frame_support::{
	derive_impl,
	dispatch::GetDispatchInfo,
	pallet_prelude::{TransactionSource, Weight},
};
use sp_runtime::{
	testing::UintAuthorityId,
	traits::{Applyable, Checkable},
	transaction_validity::{
		InvalidTransaction, TransactionValidity, TransactionValidityError, ValidTransaction,
	},
	BuildStorage, DispatchError,
};

// test for instance
#[frame_support::pallet]
pub mod pallet1 {
	use frame_support::pallet_prelude::*;
	use frame_system::pallet_prelude::*;

	pub const CALL_1_AUTH_WEIGHT: Weight = Weight::from_all(1);
	pub const CALL_1_WEIGHT: Weight = Weight::from_all(2);
	pub const CALL_2_AUTH_WEIGHT: Weight = Weight::from_all(3);
	pub const CALL_2_WEIGHT: Weight = Weight::from_all(5);
	pub const CALL_2_REFUND: Weight = Weight::from_all(4);
	pub const CALL_3_AUTH_WEIGHT: Weight = Weight::from_all(6);
	pub const CALL_3_WEIGHT: Weight = Weight::from_all(7);
	pub const CALL_3_REFUND: Weight = Weight::from_all(6);
	pub const CALL_4_AUTH_WEIGHT: Weight = Weight::from_all(10);
	pub const CALL_4_WEIGHT: Weight = Weight::from_all(11);

	#[pallet::pallet]
	pub struct Pallet<T, I = ()>(_);

	pub trait WeightInfo {
		fn call1() -> Weight;
		fn call2() -> Weight;
		fn authorize_call2() -> Weight;
		fn call3() -> Weight;
		fn authorize_call3() -> Weight;
	}

	impl WeightInfo for () {
		fn call1() -> Weight {
			CALL_1_WEIGHT
		}
		fn call2() -> Weight {
			CALL_2_WEIGHT
		}
		fn authorize_call2() -> Weight {
			CALL_2_AUTH_WEIGHT
		}
		fn call3() -> Weight {
			CALL_3_WEIGHT
		}
		fn authorize_call3() -> Weight {
			CALL_3_AUTH_WEIGHT
		}
	}

	#[pallet::config]
	pub trait Config<I: 'static = ()>: frame_system::Config {
		type SomeGeneric: Parameter;
		type WeightInfo: WeightInfo;
	}

	#[pallet::call(weight = T::WeightInfo)]
	impl<T: Config<I>, I: 'static> Pallet<T, I> {
		#[pallet::authorize(|_source| Ok((ValidTransaction::default(), Weight::zero())))]
		#[pallet::weight_of_authorize(CALL_1_AUTH_WEIGHT)]
		#[pallet::call_index(0)]
		pub fn call1(origin: OriginFor<T>) -> DispatchResult {
			ensure_authorized(origin)?;

			Ok(())
		}

		#[pallet::authorize(|_source, a, b, c, d, e, f, authorize_refund|
			if *a {
				let valid = ValidTransaction {
					priority: *b,
					requires: vec![c.encode()],
					provides: vec![d.encode()],
					longevity: *e,
					propagate: *f,
				};
				Ok((valid, *authorize_refund))
			} else {
				Err(TransactionValidityError::Invalid(InvalidTransaction::Call))
			}
		)]
		#[pallet::call_index(1)]
		pub fn call2(
			origin: OriginFor<T>,
			a: bool,
			b: u64,
			c: u8,
			d: u8,
			e: u64,
			f: bool,
			authorize_refund: Weight,
		) -> DispatchResultWithPostInfo {
			ensure_authorized(origin)?;

			let _ = (a, b, c, d, e, f, authorize_refund);

			Ok(Some(CALL_2_REFUND).into())
		}

		#[pallet::authorize(Self::authorize_call3)]
		#[pallet::call_index(2)]
		pub fn call3(
			origin: OriginFor<T>,
			valid: bool,
			_some_gen: T::SomeGeneric,
		) -> DispatchResultWithPostInfo {
			ensure_authorized(origin)?;

			let _ = valid;

			Err(sp_runtime::DispatchErrorWithPostInfo {
				post_info: Some(CALL_3_REFUND).into(),
				error: DispatchError::Other("Call3 failed"),
			})
		}

		#[cfg(feature = "frame-feature-testing")]
		#[pallet::call_index(3)]
		#[pallet::authorize(|_source| Ok((ValidTransaction::default(), Weight::zero())))]
		#[pallet::weight_of_authorize(CALL_4_AUTH_WEIGHT)]
		#[pallet::weight(CALL_4_WEIGHT)]
		pub fn call4(origin: OriginFor<T>) -> DispatchResult {
			ensure_authorized(origin)?;

			Ok(())
		}
	}

	impl<T: Config<I>, I: 'static> Pallet<T, I> {
		fn authorize_call3(
			_source: TransactionSource,
			valid: &bool,
			_some_gen: &T::SomeGeneric,
		) -> TransactionValidityWithRefund {
			if *valid {
				Ok(Default::default())
			} else {
				Err(TransactionValidityError::Invalid(InvalidTransaction::Call))
			}
		}
	}
}

// test for dev mode.
#[frame_support::pallet(dev_mode)]
pub mod pallet2 {
	use frame_support::pallet_prelude::*;
	use frame_system::pallet_prelude::*;

	#[pallet::pallet]
	pub struct Pallet<T>(_);

	pub trait SomeTrait {}

	#[pallet::config]
	pub trait Config: frame_system::Config {}

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		#[pallet::authorize(|_source| Ok(Default::default()))]
		pub fn call1(origin: OriginFor<T>) -> DispatchResult {
			ensure_authorized(origin)?;
			Ok(())
		}
	}
}

// test for no pallet info.
#[frame_support::pallet]
pub mod pallet3 {
	use frame_support::pallet_prelude::*;
	use frame_system::pallet_prelude::*;

	pub const CALL_1_AUTH_WEIGHT: Weight = Weight::from_all(1);
	pub const CALL_1_WEIGHT: Weight = Weight::from_all(1);

	#[pallet::pallet]
	pub struct Pallet<T>(_);

	#[pallet::config]
	pub trait Config: crate::pallet1::Config + frame_system::Config {}

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		#[pallet::authorize(|_source| Ok(Default::default()))]
		#[pallet::weight_of_authorize(CALL_1_AUTH_WEIGHT)]
		#[pallet::weight(CALL_1_WEIGHT)]
		#[pallet::call_index(0)]
		pub fn call1(origin: OriginFor<T>) -> DispatchResult {
			ensure_authorized(origin)?;
			Ok(())
		}
	}
}

// test for pallet with no authorized call
#[frame_support::pallet(dev_mode)]
pub mod pallet4 {
	use frame_support::pallet_prelude::*;
	use frame_system::pallet_prelude::*;

	#[pallet::pallet]
	pub struct Pallet<T>(_);

	#[pallet::config]
	pub trait Config: frame_system::Config {}

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		#[pallet::call_index(0)]
		pub fn call1(origin: OriginFor<T>) -> DispatchResult {
			ensure_authorized(origin)?;
			Ok(())
		}
	}
}

#[derive_impl(frame_system::config_preludes::TestDefaultConfig)]
impl frame_system::Config for Runtime {
	type Block = Block;
}

impl pallet2::SomeTrait for RuntimeOrigin {}

impl pallet1::Config for Runtime {
	type SomeGeneric = u32;
	type WeightInfo = ();
}

impl pallet1::Config<frame_support::instances::Instance2> for Runtime {
	type SomeGeneric = u32;
	type WeightInfo = ();
}

#[cfg(feature = "frame-feature-testing")]
impl pallet1::Config<frame_support::instances::Instance3> for Runtime {
	type SomeGeneric = u32;
	type WeightInfo = ();
}

impl pallet2::Config for Runtime {}

impl pallet3::Config for Runtime {}

impl pallet4::Config for Runtime {}

pub type TransactionExtension = frame_system::AuthorizeCall<Runtime>;

pub type Header = sp_runtime::generic::Header<u32, sp_runtime::traits::BlakeTwo256>;
pub type Block = sp_runtime::generic::Block<Header, UncheckedExtrinsic>;
pub type UncheckedExtrinsic = sp_runtime::generic::UncheckedExtrinsic<
	u64,
	RuntimeCall,
	UintAuthorityId,
	TransactionExtension,
>;

frame_support::construct_runtime!(
	pub enum Runtime {
		System: frame_system,
		Pallet1: pallet1,
		Pallet1Instance2: pallet1::<Instance2>,
		Pallet2: pallet2,
		Pallet3: pallet3,
		#[cfg(feature = "frame-feature-testing")]
		Pallet1Instance3: pallet1::<Instance3>,
		Pallet4: pallet4,
	}
);

pub fn new_test_ext() -> sp_io::TestExternalities {
	let t = RuntimeGenesisConfig { ..Default::default() }.build_storage().unwrap();
	t.into()
}

#[test]
fn valid_call_weight_test() {
	// Tests for valid successful calls and assert their weight

	struct Test {
		call: RuntimeCall,
		dispatch_success: bool,
		call_weight: Weight,
		ext_weight: Weight,
		actual_weight: Weight,
	}

	let call_2_auth_weight_refund = Weight::from_all(2);

	let tests = vec![
		Test {
			call: RuntimeCall::Pallet1(pallet1::Call::call1 {}),
			dispatch_success: true,
			call_weight: pallet1::CALL_1_WEIGHT,
			ext_weight: pallet1::CALL_1_AUTH_WEIGHT,
			actual_weight: pallet1::CALL_1_WEIGHT + pallet1::CALL_1_AUTH_WEIGHT,
		},
		Test {
			call: RuntimeCall::Pallet1(pallet1::Call::call2 {
				a: true,
				b: 1,
				c: 2,
				d: 3,
				e: 4,
				f: true,
				authorize_refund: Weight::zero(),
			}),
			dispatch_success: true,
			call_weight: pallet1::CALL_2_WEIGHT,
			ext_weight: pallet1::CALL_2_AUTH_WEIGHT,
			actual_weight: pallet1::CALL_2_REFUND + pallet1::CALL_2_AUTH_WEIGHT,
		},
		Test {
			call: RuntimeCall::Pallet1(pallet1::Call::call3 { valid: true, some_gen: 1 }),
			dispatch_success: false,
			call_weight: pallet1::CALL_3_WEIGHT,
			ext_weight: pallet1::CALL_3_AUTH_WEIGHT,
			actual_weight: pallet1::CALL_3_REFUND + pallet1::CALL_3_AUTH_WEIGHT,
		},
		Test {
			call: RuntimeCall::Pallet1Instance2(pallet1::Call::call1 {}),
			dispatch_success: true,
			call_weight: pallet1::CALL_1_WEIGHT,
			ext_weight: pallet1::CALL_1_AUTH_WEIGHT,
			actual_weight: pallet1::CALL_1_WEIGHT + pallet1::CALL_1_AUTH_WEIGHT,
		},
		Test {
			call: RuntimeCall::Pallet1Instance2(pallet1::Call::call2 {
				a: true,
				b: 1,
				c: 2,
				d: 3,
				e: 4,
				f: true,
				authorize_refund: call_2_auth_weight_refund,
			}),
			dispatch_success: true,
			call_weight: pallet1::CALL_2_WEIGHT,
			ext_weight: pallet1::CALL_2_AUTH_WEIGHT,
			actual_weight: pallet1::CALL_2_REFUND + pallet1::CALL_2_AUTH_WEIGHT -
				call_2_auth_weight_refund,
		},
		Test {
			call: RuntimeCall::Pallet1Instance2(pallet1::Call::call3 { valid: true, some_gen: 1 }),
			dispatch_success: false,
			call_weight: pallet1::CALL_3_WEIGHT,
			ext_weight: pallet1::CALL_3_AUTH_WEIGHT,
			actual_weight: pallet1::CALL_3_REFUND + pallet1::CALL_3_AUTH_WEIGHT,
		},
		Test {
			call: RuntimeCall::Pallet2(pallet2::Call::call1 {}),
			dispatch_success: true,
			call_weight: Weight::zero(),
			ext_weight: Weight::zero(),
			actual_weight: Weight::zero(),
		},
		Test {
			call: RuntimeCall::Pallet3(pallet3::Call::call1 {}),
			dispatch_success: true,
			call_weight: pallet3::CALL_1_WEIGHT,
			ext_weight: pallet3::CALL_1_AUTH_WEIGHT,
			actual_weight: pallet3::CALL_1_WEIGHT + pallet3::CALL_1_AUTH_WEIGHT,
		},
		#[cfg(feature = "frame-feature-testing")]
		Test {
			call: RuntimeCall::Pallet1Instance3(pallet1::Call::call4 {}),
			dispatch_success: true,
			call_weight: pallet1::CALL_4_WEIGHT,
			ext_weight: pallet1::CALL_4_AUTH_WEIGHT,
			actual_weight: pallet1::CALL_4_WEIGHT + pallet1::CALL_4_AUTH_WEIGHT,
		},
	];

	for (index, test) in tests.into_iter().enumerate() {
		let Test { call, dispatch_success, call_weight, ext_weight, actual_weight } = test;

		println!("Running test {}", index);

		new_test_ext().execute_with(|| {
			let tx_ext = frame_system::AuthorizeCall::<Runtime>::new();

			let tx = UncheckedExtrinsic::new_transaction(call, tx_ext);

			let info = tx.get_dispatch_info();
			let len = tx.using_encoded(|e| e.len());

			let checked = Checkable::check(tx, &frame_system::ChainContext::<Runtime>::default())
				.expect("Transaction is general so signature is good");

			checked
				.validate::<Runtime>(TransactionSource::External, &info, len)
				.expect("call1 is always valid");

			let dispatch_result =
				checked.apply::<Runtime>(&info, len).expect("Transaction is valid");

			assert_eq!(dispatch_result.is_ok(), dispatch_success);

			let post_info = dispatch_result.unwrap_or_else(|e| e.post_info);

			assert_eq!(info.call_weight, call_weight);
			assert_eq!(info.extension_weight, ext_weight);
			assert_eq!(post_info.actual_weight, Some(actual_weight));
		});
	}
}

#[test]
fn call_validity() {
	struct Test {
		call: RuntimeCall,
		validate_res: TransactionValidity,
	}

	let tests = vec![
		Test {
			call: RuntimeCall::Pallet1(pallet1::Call::call1 {}),
			validate_res: Ok(Default::default()),
		},
		Test {
			call: RuntimeCall::Pallet1(pallet1::Call::call2 {
				a: true,
				b: 1,
				c: 2,
				d: 3,
				e: 4,
				f: true,
				authorize_refund: Weight::zero(),
			}),
			validate_res: Ok(ValidTransaction {
				priority: 1,
				requires: vec![2u8.encode()],
				provides: vec![3u8.encode()],
				longevity: 4,
				propagate: true,
			}),
		},
		Test {
			call: RuntimeCall::Pallet1(pallet1::Call::call2 {
				a: false,
				b: 1,
				c: 2,
				d: 3,
				e: 4,
				f: true,
				authorize_refund: Weight::zero(),
			}),
			validate_res: Err(TransactionValidityError::Invalid(InvalidTransaction::Call)),
		},
		Test {
			call: RuntimeCall::Pallet1(pallet1::Call::call3 { valid: true, some_gen: 1 }),
			validate_res: Ok(Default::default()),
		},
		Test {
			call: RuntimeCall::Pallet1(pallet1::Call::call3 { valid: false, some_gen: 1 }),
			validate_res: Err(TransactionValidityError::Invalid(InvalidTransaction::Call)),
		},
	];

	for (index, test) in tests.into_iter().enumerate() {
		let Test { call, validate_res } = test;

		println!("Running test {}", index);

		new_test_ext().execute_with(|| {
			let tx_ext = frame_system::AuthorizeCall::<Runtime>::new();

			let tx = UncheckedExtrinsic::new_transaction(call, tx_ext);

			let info = tx.get_dispatch_info();
			let len = tx.using_encoded(|e| e.len());

			let checked = Checkable::check(tx, &frame_system::ChainContext::<Runtime>::default())
				.expect("Transaction is general so signature is good");

			let res = checked.validate::<Runtime>(TransactionSource::External, &info, len);
			assert_eq!(res, validate_res);
		});
	}
}

#[test]
fn signed_is_valid_but_dispatch_error() {
	new_test_ext().execute_with(|| {
		let call = RuntimeCall::Pallet1(pallet1::Call::call1 {});
		let tx_ext = frame_system::AuthorizeCall::<Runtime>::new();

		let tx = UncheckedExtrinsic::new_signed(call, 1u64, 1.into(), tx_ext);

		let info = tx.get_dispatch_info();
		let len = tx.using_encoded(|e| e.len());

		let checked = Checkable::check(tx, &frame_system::ChainContext::<Runtime>::default())
			.expect("Signature is good");

		checked
			.validate::<Runtime>(TransactionSource::External, &info, len)
			.expect("origin is signed, transaction is valid");

		let dispatch_err = checked
			.apply::<Runtime>(&info, len)
			.expect("origin is signed, transaction is valid")
			.expect_err("origin is wrong for the dispatched call");

		assert_eq!(dispatch_err.error, DispatchError::BadOrigin);
	});
}

#[test]
fn call_without_authorization() {
	use frame_support::traits::Authorize;

	new_test_ext().execute_with(|| {
		let call = RuntimeCall::Pallet4(pallet4::Call::call1 {});

		// tests for trait implementation
		assert_eq!(call.weight_of_authorize(), Weight::zero());
		assert_eq!(call.authorize(TransactionSource::External), None);
		assert_eq!(call.authorize(TransactionSource::InBlock), None);
		assert_eq!(call.authorize(TransactionSource::Local), None);

		// tests for transaction extension implementation
		let tx_ext = frame_system::AuthorizeCall::<Runtime>::new();

		let tx = UncheckedExtrinsic::new_transaction(call, tx_ext);

		let info = tx.get_dispatch_info();
		let len = tx.using_encoded(|e| e.len());

		let checked = Checkable::check(tx, &frame_system::ChainContext::<Runtime>::default())
			.expect("Transaction is general so signature is good");

		let err = checked
			.validate::<Runtime>(TransactionSource::External, &info, len)
			.expect_err("Call is not authorized, transaction is invalid");

		assert_eq!(err, TransactionValidityError::Invalid(InvalidTransaction::UnknownOrigin));

		let err = checked
			.apply::<Runtime>(&info, len)
			.expect_err("Call is not authorized, transaction is invalid");

		assert_eq!(err, TransactionValidityError::Invalid(InvalidTransaction::UnknownOrigin));
	});
}
