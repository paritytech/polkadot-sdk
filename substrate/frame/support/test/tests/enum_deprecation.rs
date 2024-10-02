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
	parameter_types,
	traits::{ConstU32, StorageVersion},
	OrdNoBound, PartialOrdNoBound,
};
use scale_info::TypeInfo;

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

pub trait SomeAssociation1 {
	type _1: Parameter + codec::MaxEncodedLen + TypeInfo;
}
impl SomeAssociation1 for u64 {
	type _1 = u64;
}

#[frame_support::pallet]
pub mod pallet {
	use super::*;
	use frame_support::pallet_prelude::*;

	pub(crate) const STORAGE_VERSION: StorageVersion = StorageVersion::new(10);

	#[pallet::config]
	pub trait Config: frame_system::Config
	where
		<Self as frame_system::Config>::AccountId: From<SomeType1> + SomeAssociation1,
	{
		type Balance: Parameter + Default + TypeInfo;

		type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;
	}

	#[pallet::pallet]
	#[pallet::storage_version(STORAGE_VERSION)]
	pub struct Pallet<T>(_);

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
	type MaxConsumers = ConstU32<16>;
}
impl pallet::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type Balance = u64;
}

pub type Header = sp_runtime::generic::Header<u32, sp_runtime::traits::BlakeTwo256>;
pub type Block = sp_runtime::generic::Block<Header, UncheckedExtrinsic>;
pub type UncheckedExtrinsic = sp_runtime::generic::UncheckedExtrinsic<
	u64,
	RuntimeCall,
	sp_runtime::testing::UintAuthorityId,
	frame_system::CheckNonZeroSender<Runtime>,
>;

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
			// DeprecationInfoIR::VariantsDeprecated(BTreeMap::from([
			// 	(codec::Compact(0), DeprecationStatusIR::Deprecated { note: "first", since: None
			// }), 	(
			// 		codec::Compact(1),
			// 		DeprecationStatusIR::Deprecated { note: "second", since: None }
			// 	)
			// ])),
			DeprecationInfoIR::VariantsDeprecated(BTreeMap::from([(
				codec::Compact(0),
				DeprecationStatusIR::Deprecated { note: "first", since: None }
			),])),
			meta.deprecation_info
		);
	}
}
