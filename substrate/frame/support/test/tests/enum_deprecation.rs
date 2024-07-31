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
#![allow(useless_deprecated, deprecated, clippy::deprecated_semver)]

use std::collections::BTreeMap;

use frame_support::{
	derive_impl,
	dispatch::Parameter,
	dispatch_context::with_context,
	parameter_types,
	traits::{ConstU32, StorageVersion},
	weights::Weight,
	OrdNoBound, PartialOrdNoBound,
};
use scale_info::TypeInfo;

use sp_runtime::DispatchError;

parameter_types! {
	/// Used to control if the storage version should be updated.
	storage UpdateStorageVersion: bool = false;
}

pub struct SomeType1;
impl From<SomeType1> for u64 {
	fn from(_t: SomeType1) -> Self {
		0u64
	}
}

pub struct SomeType2;
impl From<SomeType2> for u64 {
	fn from(_t: SomeType2) -> Self {
		100u64
	}
}

pub struct SomeType3;
impl From<SomeType3> for u64 {
	fn from(_t: SomeType3) -> Self {
		0u64
	}
}

pub struct SomeType4;
impl From<SomeType4> for u64 {
	fn from(_t: SomeType4) -> Self {
		0u64
	}
}

pub trait SomeAssociation1 {
	type _1: Parameter + codec::MaxEncodedLen + TypeInfo;
}
impl SomeAssociation1 for u64 {
	type _1 = u64;
}

pub trait SomeAssociation2 {
	type _2: Parameter + codec::MaxEncodedLen + TypeInfo;
}
impl SomeAssociation2 for u64 {
	type _2 = u64;
}

#[frame_support::pallet]
/// Pallet documentation
// Comments should not be included in the pallet documentation
#[pallet_doc("../../README.md")]
#[doc = include_str!("../../README.md")]
pub mod pallet {
	use super::*;
	use frame_support::pallet_prelude::*;
	use frame_system::pallet_prelude::*;
	use sp_runtime::DispatchResult;

	pub(crate) const STORAGE_VERSION: StorageVersion = StorageVersion::new(10);

	#[pallet::config]
	pub trait Config: frame_system::Config
	where
		<Self as frame_system::Config>::AccountId: From<SomeType1> + SomeAssociation1,
	{
		/// Some comment
		/// Some comment
		#[pallet::constant]
		type MyGetParam: Get<u32>;

		/// Some comment
		/// Some comment
		#[pallet::constant]
		type MyGetParam2: Get<u32>;

		#[pallet::constant]
		type MyGetParam3: Get<<Self::AccountId as SomeAssociation1>::_1>;

		type Balance: Parameter + Default + TypeInfo;

		type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;
	}

	#[pallet::extra_constants]
	impl<T: Config> Pallet<T>
	where
		T::AccountId: From<SomeType1> + SomeAssociation1 + From<SomeType2>,
	{
		/// Some doc
		/// Some doc
		fn some_extra() -> T::AccountId {
			SomeType2.into()
		}

		/// Some doc
		fn some_extra_extra() -> T::AccountId {
			SomeType1.into()
		}

		/// Some doc
		#[pallet::constant_name(SomeExtraRename)]
		fn some_extra_rename() -> T::AccountId {
			SomeType1.into()
		}
	}

	#[pallet::pallet]
	#[pallet::storage_version(STORAGE_VERSION)]
	pub struct Pallet<T>(_);

	#[pallet::hooks]
	impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T>
	where
		T::AccountId: From<SomeType2> + From<SomeType1> + SomeAssociation1,
	{
		fn on_initialize(_: BlockNumberFor<T>) -> Weight {
			let _ = T::AccountId::from(SomeType1); // Test for where clause
			let _ = T::AccountId::from(SomeType2); // Test for where clause
			Self::deposit_event(Event::B);
			Weight::from_parts(10, 0)
		}
		fn on_finalize(_: BlockNumberFor<T>) {
			let _ = T::AccountId::from(SomeType1); // Test for where clause
			let _ = T::AccountId::from(SomeType2); // Test for where clause
			Self::deposit_event(Event::A);
		}
		fn on_runtime_upgrade() -> Weight {
			let _ = T::AccountId::from(SomeType1); // Test for where clause
			let _ = T::AccountId::from(SomeType2); // Test for where clause
			Self::deposit_event(Event::A);
			Weight::from_parts(30, 0)
		}
		fn integrity_test() {
			let _ = T::AccountId::from(SomeType1); // Test for where clause
			let _ = T::AccountId::from(SomeType2); // Test for where clause
		}
	}

	#[pallet::call]
	impl<T: Config> Pallet<T>
	where
		T::AccountId: From<SomeType1> + From<SomeType2> + SomeAssociation1,
	{
		/// call foo_storage_layer doc comment put in metadata
		#[pallet::call_index(1)]
		#[pallet::weight({1})]
		pub fn foo_storage_layer(
			_origin: OriginFor<T>,
			#[pallet::compact] foo: u32,
		) -> DispatchResultWithPostInfo {
			Self::deposit_event(Event::B);
			if foo == 0 {
				Err(Error::<T>::InsufficientProposersBalance)?;
			}

			Ok(().into())
		}

		#[pallet::call_index(4)]
		#[pallet::weight({1})]
		pub fn foo_index_out_of_order(_origin: OriginFor<T>) -> DispatchResult {
			Ok(())
		}

		// Test for DispatchResult return type
		#[pallet::call_index(2)]
		#[pallet::weight({1})]
		pub fn foo_no_post_info(_origin: OriginFor<T>) -> DispatchResult {
			Ok(())
		}

		#[pallet::call_index(3)]
		#[pallet::weight({1})]
		pub fn check_for_dispatch_context(_origin: OriginFor<T>) -> DispatchResult {
			with_context::<(), _>(|_| ()).ok_or_else(|| DispatchError::Unavailable)
		}
	}

	#[pallet::error]
	#[derive(PartialEq, Eq)]
	pub enum Error<T> {
		/// error doc comment put in metadata
		InsufficientProposersBalance,
		NonExistentStorageValue,
		Code(u8),
		#[codec(skip)]
		Skipped(u128),
		CompactU8(#[codec(compact)] u8),
	}

	#[pallet::event]
	#[pallet::generate_deposit(fn deposit_event)]
	pub enum Event<T: Config>
	where
		T::AccountId: SomeAssociation1 + From<SomeType1>,
	{
		#[deprecated = "second"]
		A,
		#[deprecated = "first"]
		#[codec(index = 0)]
		B,
	}

	#[pallet::genesis_config]
	#[derive(frame_support::DefaultNoBound)]
	pub struct GenesisConfig<T: Config>
	where
		T::AccountId: From<SomeType1> + SomeAssociation1 + From<SomeType4>,
	{
		#[serde(skip)]
		_config: sp_std::marker::PhantomData<T>,
		_myfield: u32,
	}

	#[pallet::genesis_build]
	impl<T: Config> BuildGenesisConfig for GenesisConfig<T>
	where
		T::AccountId: From<SomeType1> + SomeAssociation1 + From<SomeType4>,
	{
		fn build(&self) {
			let _ = T::AccountId::from(SomeType1); // Test for where clause
			let _ = T::AccountId::from(SomeType4); // Test for where clause
		}
	}

	#[pallet::origin]
	#[derive(
		EqNoBound,
		RuntimeDebugNoBound,
		CloneNoBound,
		PartialEqNoBound,
		PartialOrdNoBound,
		OrdNoBound,
		Encode,
		Decode,
		TypeInfo,
		MaxEncodedLen,
	)]
	pub struct Origin<T>(PhantomData<T>);
}

frame_support::parameter_types!(
	pub const MyGetParam3: u32 = 12;
);

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
	type MaxConsumers = ConstU32<16>;
}
impl pallet::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type MyGetParam = ConstU32<10>;
	type MyGetParam2 = ConstU32<11>;
	type MyGetParam3 = MyGetParam3;
	type Balance = u64;
}

pub type Header = sp_runtime::generic::Header<u32, sp_runtime::traits::BlakeTwo256>;
pub type Block = sp_runtime::generic::Block<Header, UncheckedExtrinsic>;
pub type UncheckedExtrinsic =
	sp_runtime::testing::TestXt<RuntimeCall, frame_system::CheckNonZeroSender<Runtime>>;

frame_support::construct_runtime!(
	pub struct Runtime {
		// Exclude part `Storage` in order not to check its metadata in tests.
		System: frame_system exclude_parts { Pallet, Storage },
		Example: pallet,

	}
);

#[test]
fn pallet_metadata() {
	use sp_metadata_ir::{DeprecationInfoIR, DeprecationStatusIR};
	let pallets = Runtime::metadata_ir().pallets;
	let example = pallets[0].clone();
	{
		// Example pallet events are partially and fully deprecated
		let meta = example.event.unwrap();
		assert_eq!(
			// Result should be this, but instead we get the result below
			// see: https://github.com/paritytech/parity-scale-codec/issues/507
			//
			// DeprecationInfoIR::PartiallyDeprecated(BTreeMap::from([
			// 	(codec::Compact(0), DeprecationStatusIR::Deprecated { note: "first", since: None
			// }), 	(
			// 		codec::Compact(1),
			// 		DeprecationStatusIR::Deprecated { note: "second", since: None }
			// 	)
			// ])),
			DeprecationInfoIR::PartiallyDeprecated(BTreeMap::from([(
				codec::Compact(0),
				DeprecationStatusIR::Deprecated { note: "first", since: None }
			),])),
			meta.deprecation_info
		);
	}
}
