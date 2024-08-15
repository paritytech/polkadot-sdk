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

// no origin + simple supertrait bound + instance
#[frame_support::pallet]
pub mod pallet1 {
	use frame_support::pallet_prelude::*;
	use frame_system::pallet_prelude::*;

	const CALL_1_WEIGHT: Weight = Weight::from_all(1);

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
		fn call1() -> Weight { Weight::from_all(1) }
		fn call2() -> Weight { Weight::from_all(2) }
		fn authorize_call2() -> Weight { Weight::from_all(3) }
		fn call3() -> Weight { Weight::from_all(4) }
		fn authorize_call3() -> Weight { Weight::from_all(5) }
	}

	#[pallet::config]
	pub trait Config<I: 'static = ()>: frame_system::Config {
		type WeightInfo: WeightInfo;
	}

	#[pallet::call(weight = T::WeightInfo)]
	impl<T: Config<I>, I: 'static> Pallet<T, I> {
		#[pallet::authorize(|| Ok(ValidTransaction::default()))]
		#[pallet::weight_of_authorize(CALL_1_WEIGHT)]
		#[pallet::call_index(0)]
		pub fn call1(origin: OriginFor<T>) -> DispatchResult {
			ensure_authorized_origin!(origin);

			Ok(())
		}

		#[pallet::authorize(|a, b, c, d, e, f|
			if *a && !Reject::<T, I>::get() {
				Ok(ValidTransaction {
					priority: *b,
					requires: vec![c.encode()],
					provides: vec![d.encode()],
					longevity: *e,
					propagate: *f,
				})
			} else {
				Err(TransactionValidityError::Invalid(InvalidTransaction::Call))
			}
		)]
		#[pallet::call_index(1)]
		pub fn call2(origin: OriginFor<T>, a: bool, b: u64, c: u8, d: u8, e: u64, f: bool) -> DispatchResult {
			ensure_authorized_origin!(origin);

			let _ = (a, b, c, d, e, f);

			Ok(())
		}

		#[pallet::authorize(Pallet::<T, I>::authorize_call3)]
		#[pallet::call_index(2)]
		pub fn call3(origin: OriginFor<T>, valid: bool) -> DispatchResult {
			ensure_authorized_origin!(origin);

			let _ = valid;

			Ok(())
		}
	}

	impl<T: Config<I>, I: 'static> Pallet<T, I> {
		fn authorize_call3(valid: &bool) -> TransactionValidity {
			if *valid {
				Ok(Default::default())
			} else {
				Err(TransactionValidityError::Invalid(InvalidTransaction::Call))
			}
		}
	}

	#[pallet::storage]
	pub type Reject<T, I = ()> = StorageValue<_, bool, ValueQuery>;
}

// dev mode + system supertrait bound with arg
#[frame_support::pallet(dev_mode)]
pub mod pallet2 {
	use frame_support::pallet_prelude::*;
	use frame_system::pallet_prelude::*;

	#[pallet::pallet]
	pub struct Pallet<T>(_);

	pub trait SomeTrait {}

	#[pallet::config]
	pub trait Config: crate::pallet1::Config + frame_system::Config<RuntimeOrigin: SomeTrait> {}

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		#[pallet::authorize(|| Ok(ValidTransaction::default()))]
		pub fn call1(origin: OriginFor<T>) -> DispatchResult {
			ensure_authorized_origin!(origin);
			Ok(())
		}
	}
}

// explicit generic origin
#[frame_support::pallet]
pub mod pallet3 {
	use frame_support::pallet_prelude::*;
	use frame_system::pallet_prelude::*;

	#[pallet::pallet]
	pub struct Pallet<T>(_);

	#[pallet::config]
	pub trait Config: crate::pallet1::Config + frame_system::Config {}

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		#[pallet::authorize(|| Ok(ValidTransaction::default()))]
		#[pallet::weight_of_authorize(Weight::from_all(1))]
		#[pallet::weight(Weight::from_all(1))]
		#[pallet::call_index(0)]
		pub fn call1(origin: OriginFor<T>) -> DispatchResult {
			ensure_authorized_origin!(origin);
			Ok(())
		}
	}

	#[pallet::origin]
	#[derive(
		frame_support::CloneNoBound,
		frame_support::PartialEqNoBound,
		frame_support::EqNoBound,
		frame_support::RuntimeDebugNoBound,
		codec::Encode,
		codec::MaxEncodedLen,
		codec::Decode,
		scale_info::TypeInfo,
	)]
	#[scale_info(skip_type_params(T))]
	pub enum Origin<T> {
		#[pallet::authorized_call]
		AuthorizedCall(_),
		// #[codec(skip)]
		// __Ignore(PhantomData<T>, frame_support::Never),
		__Ignore(PhantomData<T>),
	}
}

// explicit generic origin + instance
#[frame_support::pallet]
pub mod pallet4 {
	use frame_support::pallet_prelude::*;
	use frame_system::pallet_prelude::*;

	#[pallet::pallet]
	pub struct Pallet<T, I = ()>(_);

	#[pallet::config]
	pub trait Config<I: 'static = ()>: crate::pallet1::Config + frame_system::Config {}

	#[pallet::call]
	impl<T: Config<I>, I: 'static> Pallet<T, I> {
		#[pallet::authorize(|| Ok(ValidTransaction::default()))]
		#[pallet::weight_of_authorize(Weight::from_all(1))]
		#[pallet::weight(Weight::from_all(1))]
		#[pallet::call_index(0)]
		pub fn call1(origin: OriginFor<T>) -> DispatchResult {
			ensure_authorized_origin!(origin);
			Ok(())
		}
	}

	#[pallet::origin]
	#[derive(
		frame_support::CloneNoBound,
		frame_support::PartialEqNoBound,
		frame_support::EqNoBound,
		frame_support::RuntimeDebugNoBound,
		codec::Encode,
		codec::MaxEncodedLen,
		codec::Decode,
		scale_info::TypeInfo,
	)]
	#[scale_info(skip_type_params(T, I))]
	pub enum Origin<T, I = ()> {
		#[pallet::authorized_call]
		AuthorizedCall(_),
		// #[codec(skip)]
		// __Ignore(PhantomData<(T, I)>, frame_support::Never),
		__Ignore(PhantomData<(T, I)>),
	}
}

// explicit not generic origin
#[frame_support::pallet]
pub mod pallet5 {
	use frame_support::pallet_prelude::*;
	use frame_system::pallet_prelude::*;

	#[pallet::pallet]
	pub struct Pallet<T>(_);

	#[pallet::config]
	pub trait Config: crate::pallet1::Config + frame_system::Config {}

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		#[pallet::authorize(|| Ok(ValidTransaction::default()))]
		#[pallet::weight_of_authorize(Weight::from_all(1))]
		#[pallet::weight(Weight::from_all(1))]
		#[pallet::call_index(0)]
		pub fn call1(origin: OriginFor<T>) -> DispatchResult {
			ensure_authorized_origin!(origin);
			Ok(())
		}
	}

	#[pallet::origin]
	#[derive(
		frame_support::CloneNoBound,
		frame_support::PartialEqNoBound,
		frame_support::EqNoBound,
		frame_support::RuntimeDebugNoBound,
		codec::Encode,
		codec::MaxEncodedLen,
		codec::Decode,
		scale_info::TypeInfo,
	)]
	pub enum Origin {
		#[pallet::authorized_call]
		AuthorizedCall(_),
	}
}

use frame_support::derive_impl;
#[derive_impl(frame_system::config_preludes::TestDefaultConfig)]
impl frame_system::Config for Runtime {
	type BaseCallFilter = frame_support::traits::Everything;
	type RuntimeOrigin = RuntimeOrigin;
	type Nonce = u64;
	type RuntimeCall = RuntimeCall;
	type Hash = sp_runtime::testing::H256;
	type Hashing = sp_runtime::traits::BlakeTwo256;
	type AccountId = u64;
	type Lookup = sp_runtime::traits::IdentityLookup<Self::AccountId>;
	type Block = Block;
	type RuntimeEvent = RuntimeEvent;
	type BlockWeights = ();
	type BlockLength = ();
	type DbWeight = ();
	type Version = ();
	type PalletInfo = PalletInfo;
	type AccountData = ();
	type OnNewAccount = ();
	type OnKilledAccount = ();
	type SystemWeightInfo = ();
	type SS58Prefix = ();
	type OnSetCode = ();
	type MaxConsumers = frame_support::traits::ConstU32<16>;
}

impl pallet2::SomeTrait for RuntimeOrigin {}

impl pallet1::Config for Runtime {
	type WeightInfo = ();
}

impl pallet1::Config<frame_support::instances::Instance2> for Runtime {
	type WeightInfo = ();
}

impl pallet2::Config for Runtime {}

impl pallet3::Config for Runtime {}

impl pallet4::Config for Runtime {}

impl pallet4::Config<frame_support::instances::Instance2> for Runtime {}

impl pallet5::Config for Runtime {}

pub type TransactionExtension = (
	frame_system::AuthorizeCall<Runtime>,
	frame_system::CheckNonZeroSender<Runtime>,
	frame_system::DenyNone<Runtime>,
);

pub type Header = sp_runtime::generic::Header<u32, sp_runtime::traits::BlakeTwo256>;
pub type Block = sp_runtime::generic::Block<Header, UncheckedExtrinsic>;
pub type UncheckedExtrinsic = sp_runtime::generic::UncheckedExtrinsic<
	u64,
	RuntimeCall,
	(),
	TransactionExtension,
>;

#[frame_support::runtime]
mod runtime {
	#[runtime::runtime]
	#[runtime::derive(
		RuntimeCall,
		RuntimeEvent,
		RuntimeError,
		RuntimeOrigin,
		RuntimeFreezeReason,
		RuntimeHoldReason,
		RuntimeSlashReason,
		RuntimeLockId,
		RuntimeTask
	)]
	pub struct Runtime;

	#[runtime::pallet_index(0)]
	pub type System = frame_system::Pallet<Runtime>;

	#[runtime::pallet_index(1)]
	pub type Pallet1 = pallet1::Pallet<Runtime>;

	#[runtime::pallet_index(12)]
	pub type Pallet1Instance2 = pallet1::Pallet<Runtime, Instance2>;

	#[runtime::pallet_index(2)]
	pub type Pallet2 = pallet2::Pallet<Runtime>;

	#[runtime::pallet_index(3)]
	pub type Pallet3 = pallet3::Pallet<Runtime>;

	#[runtime::pallet_index(4)]
	pub type Pallet4 = pallet4::Pallet<Runtime>;

	#[runtime::pallet_index(42)]
	pub type Pallet4Instance2 = pallet4::Pallet<Runtime, Instance2>;

	#[runtime::pallet_index(5)]
	pub type Pallet5 = pallet5::Pallet<Runtime>;
}

// TODO TODO: add tests
