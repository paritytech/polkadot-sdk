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

//! Tests for `#[pallet::as_account(...)]`, `#[pallet::nonce_provider]`, and
//! `#[pallet::fee_payer]` on pallet origin enum variants.

use frame_support::{
	derive_impl,
	traits::AccountLike,
};
use sp_runtime::{generic, traits::BlakeTwo256};

/// Pallet 1: Generic enum origin with `as_account` + `nonce_provider` on some variants.
#[frame_support::pallet(dev_mode)]
pub mod pallet1 {
	use frame_support::pallet_prelude::*;
	use frame_system::pallet_prelude::*;

	#[pallet::pallet]
	pub struct Pallet<T>(_);

	#[pallet::config]
	pub trait Config: frame_system::Config {}

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		pub fn noop(_origin: OriginFor<T>) -> DispatchResult {
			Ok(())
		}
	}

	#[pallet::origin]
	#[derive(
		Clone, PartialEq, Eq, Debug, Encode, Decode, DecodeWithMemTracking, MaxEncodedLen, TypeInfo,
	)]
	pub enum Origin<T: Config> {
		#[pallet::as_account(|who| Some(who.clone()))]
		#[pallet::nonce_provider]
		Member(T::AccountId),
		Admin,
	}
}

/// Pallet 2: Generic enum origin with multiple fields.
#[frame_support::pallet(dev_mode)]
pub mod pallet2 {
	use frame_support::pallet_prelude::*;
	use frame_system::pallet_prelude::*;

	#[pallet::pallet]
	pub struct Pallet<T>(_);

	#[pallet::config]
	pub trait Config: frame_system::Config {}

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		pub fn noop(_origin: OriginFor<T>) -> DispatchResult {
			Ok(())
		}
	}

	#[pallet::origin]
	#[derive(
		Clone, PartialEq, Eq, Debug, Encode, Decode, DecodeWithMemTracking, MaxEncodedLen, TypeInfo,
	)]
	pub enum Origin<T: Config> {
		#[pallet::as_account(|account, _data| Some(account.clone()))]
		#[pallet::nonce_provider]
		#[pallet::fee_payer]
		WithData(T::AccountId, u32),
		#[pallet::as_account(|_a, _b| None)]
		NoAccount(u32, u64),
	}
}

/// Pallet 3: Non-generic enum origin (governance-style, no `as_account`).
#[frame_support::pallet(dev_mode)]
pub mod pallet3 {
	use frame_support::pallet_prelude::*;
	use frame_system::pallet_prelude::*;

	#[pallet::pallet]
	pub struct Pallet<T>(_);

	#[pallet::config]
	pub trait Config: frame_system::Config {}

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		pub fn noop(_origin: OriginFor<T>) -> DispatchResult {
			Ok(())
		}
	}

	#[pallet::origin]
	#[derive(
		Clone, PartialEq, Eq, Debug, Encode, Decode, DecodeWithMemTracking, MaxEncodedLen, TypeInfo,
	)]
	pub enum Origin {
		StakingAdmin,
		Treasurer,
	}
}

/// Pallet 4: Struct origin (no `as_account` possible).
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
		pub fn noop(_origin: OriginFor<T>) -> DispatchResult {
			Ok(())
		}
	}

	#[pallet::origin]
	#[derive(
		Clone, PartialEq, Eq, Debug, Encode, Decode, DecodeWithMemTracking, MaxEncodedLen, TypeInfo,
	)]
	pub struct Origin<T>(pub PhantomData<T>);
}

/// Pallet 5: Function reference instead of closure.
#[frame_support::pallet(dev_mode)]
pub mod pallet5 {
	use frame_support::pallet_prelude::*;
	use frame_system::pallet_prelude::*;

	#[pallet::pallet]
	pub struct Pallet<T>(_);

	#[pallet::config]
	pub trait Config: frame_system::Config {}

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		pub fn noop(_origin: OriginFor<T>) -> DispatchResult {
			Ok(())
		}
	}

	impl<T: Config> Pallet<T> {
		fn nonce_for_member(who: &T::AccountId) -> Option<T::AccountId> {
			Some(who.clone())
		}
	}

	#[pallet::origin]
	#[derive(
		Clone, PartialEq, Eq, Debug, Encode, Decode, DecodeWithMemTracking, MaxEncodedLen, TypeInfo,
	)]
	pub enum Origin<T: Config> {
		#[pallet::as_account(Self::nonce_for_member)]
		#[pallet::nonce_provider]
		#[pallet::fee_payer]
		Member(T::AccountId),
	}
}

/// Pallet 6: Instanced pallet origin.
#[frame_support::pallet(dev_mode)]
pub mod pallet6 {
	use frame_support::pallet_prelude::*;
	use frame_system::pallet_prelude::*;

	#[pallet::pallet]
	pub struct Pallet<T, I = ()>(_);

	#[pallet::config]
	pub trait Config<I: 'static = ()>: frame_system::Config {}

	#[pallet::call]
	impl<T: Config<I>, I: 'static> Pallet<T, I> {
		pub fn noop(_origin: OriginFor<T>) -> DispatchResult {
			Ok(())
		}
	}

	#[pallet::origin]
	#[derive(
		Clone, PartialEq, Eq, Debug, Encode, Decode, DecodeWithMemTracking, MaxEncodedLen, TypeInfo,
	)]
	#[scale_info(skip_type_params(T, I))]
	pub enum Origin<T: Config<I>, I: 'static = ()> {
		#[pallet::as_account(|who| Some(who.clone()))]
		#[pallet::nonce_provider]
		Member(T::AccountId),
		#[allow(dead_code)]
		_Phantom(PhantomData<(T, I)>),
	}
}

/// Pallet 7: Generic enum origin with `as_account` on ALL variants.
#[frame_support::pallet(dev_mode)]
pub mod pallet7 {
	use frame_support::pallet_prelude::*;
	use frame_system::pallet_prelude::*;

	#[pallet::pallet]
	pub struct Pallet<T>(_);

	#[pallet::config]
	pub trait Config: frame_system::Config {}

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		pub fn noop(_origin: OriginFor<T>) -> DispatchResult {
			Ok(())
		}
	}

	#[pallet::origin]
	#[derive(
		Clone, PartialEq, Eq, Debug, Encode, Decode, DecodeWithMemTracking, MaxEncodedLen, TypeInfo,
	)]
	pub enum Origin<T: Config> {
		#[pallet::as_account(|who| Some(who.clone()))]
		#[pallet::nonce_provider]
		#[pallet::fee_payer]
		Member(T::AccountId),
		#[pallet::as_account(|who, _data| Some(who.clone()))]
		#[pallet::nonce_provider]
		MemberWithData(T::AccountId, u32),
		#[pallet::as_account(|_a, _b| None)]
		Anonymous(u32, u64),
	}
}

/// Pallet 8: Non-generic enum origin with `as_account` on multiple variants.
/// Closures access `Self` (= `Pallet<T>`) to derive AccountId from concrete fields.
#[frame_support::pallet(dev_mode)]
pub mod pallet8 {
	use frame_support::pallet_prelude::*;
	use frame_system::pallet_prelude::*;

	#[pallet::pallet]
	pub struct Pallet<T>(_);

	#[pallet::config]
	pub trait Config: frame_system::Config {}

	/// Storage mapping from u32 council IDs to AccountIds.
	#[pallet::storage]
	pub type CouncilAccounts<T: Config> =
		StorageMap<_, Twox64Concat, u32, T::AccountId>;

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		pub fn noop(_origin: OriginFor<T>) -> DispatchResult {
			Ok(())
		}
	}

	impl<T: Config> Pallet<T> {
		pub fn account_for_council(id: &u32) -> Option<T::AccountId> {
			CouncilAccounts::<T>::get(id)
		}
	}

	#[pallet::origin]
	#[derive(
		Clone, PartialEq, Eq, Debug, Encode, Decode, DecodeWithMemTracking, MaxEncodedLen, TypeInfo,
	)]
	pub enum Origin {
		#[pallet::as_account(|id| Self::account_for_council(id))]
		#[pallet::nonce_provider]
		Council(u32),
		#[pallet::as_account(|_count, _threshold| None)]
		Members(u32, u32),
		TechCommittee,
	}
}

/// Pallet 9: Instanced pallet with a type alias origin (mirrors pallet_collective).
/// The aliased `RawOrigin` implements `AccountLike`, so the type alias delegation
/// returns `Some(account)` for the `Member` variant.
#[frame_support::pallet(dev_mode)]
pub mod pallet9 {
	use core::marker::PhantomData;
	use frame_support::pallet_prelude::*;
	use frame_system::pallet_prelude::*;

	#[pallet::pallet]
	pub struct Pallet<T, I = ()>(_);

	#[pallet::config]
	pub trait Config<I: 'static = ()>: frame_system::Config {}

	#[pallet::call]
	impl<T: Config<I>, I: 'static> Pallet<T, I> {
		pub fn noop(_origin: OriginFor<T>) -> DispatchResult {
			Ok(())
		}
	}

	/// A collective-style RawOrigin with an instance parameter.
	#[derive(
		Clone, PartialEq, Eq, Debug, Encode, Decode, DecodeWithMemTracking, MaxEncodedLen, TypeInfo,
	)]
	#[scale_info(skip_type_params(I))]
	#[codec(mel_bound(AccountId: MaxEncodedLen))]
	pub enum RawOrigin<AccountId, I> {
		Members(u32, u32),
		Member(AccountId),
		_Phantom(PhantomData<I>),
	}

	impl<AccountId: Clone, I> frame_support::traits::AccountLike<AccountId>
		for RawOrigin<AccountId, I>
	{
		fn as_account(&self) -> Option<AccountId> {
			match self {
				RawOrigin::Member(who) => Some(who.clone()),
				_ => None,
			}
		}

		fn nonce_provider(&self) -> Option<AccountId> {
			match self {
				RawOrigin::Member(who) => Some(who.clone()),
				_ => None,
			}
		}
	}

	/// Type alias origin, just like pallet_collective.
	#[pallet::origin]
	pub type Origin<T, I = ()> = RawOrigin<<T as frame_system::Config>::AccountId, I>;
}

/// Pallet 10: Type alias origin with a custom type that manually implements `AccountLike`.
/// Verifies that `__as_account_for_origin` delegates to the trait impl on the aliased type.
#[frame_support::pallet(dev_mode)]
pub mod pallet10 {
	use frame_support::pallet_prelude::*;
	use frame_system::pallet_prelude::*;

	#[pallet::pallet]
	pub struct Pallet<T>(_);

	#[pallet::config]
	pub trait Config: frame_system::Config {}

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		pub fn noop(_origin: OriginFor<T>) -> DispatchResult {
			Ok(())
		}
	}

	/// A custom origin type with a manual `AccountLike` implementation.
	#[derive(
		Clone, PartialEq, Eq, Debug, Encode, Decode, DecodeWithMemTracking, MaxEncodedLen, TypeInfo,
	)]
	pub enum CustomOrigin<AccountId> {
		Admin(AccountId),
		Root,
	}

	impl<AccountId: Clone> frame_support::traits::AccountLike<AccountId>
		for CustomOrigin<AccountId>
	{
		fn as_account(&self) -> Option<AccountId> {
			match self {
				CustomOrigin::Admin(who) => Some(who.clone()),
				CustomOrigin::Root => None,
			}
		}

		fn nonce_provider(&self) -> Option<AccountId> {
			match self {
				CustomOrigin::Admin(who) => Some(who.clone()),
				CustomOrigin::Root => None,
			}
		}

		fn fee_payer(&self) -> Option<AccountId> {
			match self {
				CustomOrigin::Admin(who) => Some(who.clone()),
				CustomOrigin::Root => None,
			}
		}
	}

	#[pallet::origin]
	pub type Origin<T> = CustomOrigin<<T as frame_system::Config>::AccountId>;
}

/// Pallet 11: as_account only (no nonce_provider, no fee_payer).
#[frame_support::pallet(dev_mode)]
pub mod pallet11 {
	use frame_support::pallet_prelude::*;
	use frame_system::pallet_prelude::*;

	#[pallet::pallet]
	pub struct Pallet<T>(_);

	#[pallet::config]
	pub trait Config: frame_system::Config {}

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		pub fn noop(_origin: OriginFor<T>) -> DispatchResult {
			Ok(())
		}
	}

	#[pallet::origin]
	#[derive(
		Clone, PartialEq, Eq, Debug, Encode, Decode, DecodeWithMemTracking, MaxEncodedLen, TypeInfo,
	)]
	pub enum Origin<T: Config> {
		#[pallet::as_account(|who| Some(who.clone()))]
		Member(T::AccountId),
		Admin,
	}
}

/// Pallet 12: as_account + fee_payer only (no nonce_provider).
#[frame_support::pallet(dev_mode)]
pub mod pallet12 {
	use frame_support::pallet_prelude::*;
	use frame_system::pallet_prelude::*;

	#[pallet::pallet]
	pub struct Pallet<T>(_);

	#[pallet::config]
	pub trait Config: frame_system::Config {}

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		pub fn noop(_origin: OriginFor<T>) -> DispatchResult {
			Ok(())
		}
	}

	#[pallet::origin]
	#[derive(
		Clone, PartialEq, Eq, Debug, Encode, Decode, DecodeWithMemTracking, MaxEncodedLen, TypeInfo,
	)]
	pub enum Origin<T: Config> {
		#[pallet::as_account(|who| Some(who.clone()))]
		#[pallet::fee_payer]
		Member(T::AccountId),
		Admin,
	}
}

/// Pallet 13: as_account on a unit variant (zero fields).
#[frame_support::pallet(dev_mode)]
pub mod pallet13 {
	use frame_support::pallet_prelude::*;
	use frame_system::pallet_prelude::*;

	#[pallet::pallet]
	pub struct Pallet<T>(_);

	#[pallet::config]
	pub trait Config: frame_system::Config {}

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		pub fn noop(_origin: OriginFor<T>) -> DispatchResult {
			Ok(())
		}
	}

	#[pallet::origin]
	#[derive(
		Clone, PartialEq, Eq, Debug, Encode, Decode, DecodeWithMemTracking, MaxEncodedLen, TypeInfo,
	)]
	pub enum Origin<T: Config> {
		#[pallet::as_account(|who| Some(who.clone()))]
		#[pallet::nonce_provider]
		Member(T::AccountId),
		#[pallet::as_account(|| None)]
		Admin,
	}
}

/// Pallet 14: as_account on a named-fields variant (struct-like).
#[frame_support::pallet(dev_mode)]
pub mod pallet14 {
	use frame_support::pallet_prelude::*;
	use frame_system::pallet_prelude::*;

	#[pallet::pallet]
	pub struct Pallet<T>(_);

	#[pallet::config]
	pub trait Config: frame_system::Config {}

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		pub fn noop(_origin: OriginFor<T>) -> DispatchResult {
			Ok(())
		}
	}

	#[pallet::origin]
	#[derive(
		Clone, PartialEq, Eq, Debug, Encode, Decode, DecodeWithMemTracking, MaxEncodedLen, TypeInfo,
	)]
	pub enum Origin<T: Config> {
		#[pallet::as_account(|who, _rank| Some(who.clone()))]
		#[pallet::nonce_provider]
		#[pallet::fee_payer]
		Member { who: T::AccountId, rank: u32 },
		Admin,
	}
}

/// Pallet 15: Has an Origin enum with variant `Delegate(AccountId, u32)`.
/// The `as_account` closure returns Some(account) — uses the first field.
#[frame_support::pallet(dev_mode)]
pub mod pallet15 {
	use frame_support::pallet_prelude::*;
	use frame_system::pallet_prelude::*;

	#[pallet::pallet]
	pub struct Pallet<T>(_);

	#[pallet::config]
	pub trait Config: frame_system::Config {}

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		pub fn noop(_origin: OriginFor<T>) -> DispatchResult {
			Ok(())
		}
	}

	#[pallet::origin]
	#[derive(
		Clone, PartialEq, Eq, Debug, Encode, Decode, DecodeWithMemTracking, MaxEncodedLen, TypeInfo,
	)]
	pub enum Origin<T: Config> {
		/// Shared variant name with pallet16, but different closure logic:
		/// returns the account from field 0, ignoring the u32 weight field.
		#[pallet::as_account(|who, _weight| Some(who.clone()))]
		#[pallet::nonce_provider]
		Delegate(T::AccountId, u32),
		/// A variant unique to pallet15.
		#[pallet::as_account(|who| Some(who.clone()))]
		#[pallet::fee_payer]
		Treasurer(T::AccountId),
	}
}

/// Pallet 16: Also has an Origin enum with variant `Delegate(AccountId, u32)`.
/// The `as_account` closure returns None when the second field is 0 (disabled delegate).
#[frame_support::pallet(dev_mode)]
pub mod pallet16 {
	use frame_support::pallet_prelude::*;
	use frame_system::pallet_prelude::*;

	#[pallet::pallet]
	pub struct Pallet<T>(_);

	#[pallet::config]
	pub trait Config: frame_system::Config {}

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		pub fn noop(_origin: OriginFor<T>) -> DispatchResult {
			Ok(())
		}
	}

	#[pallet::origin]
	#[derive(
		Clone, PartialEq, Eq, Debug, Encode, Decode, DecodeWithMemTracking, MaxEncodedLen, TypeInfo,
	)]
	pub enum Origin<T: Config> {
		/// Shared variant name with pallet15, but different closure logic:
		/// returns None when the power field is 0 (disabled delegate).
		#[pallet::as_account(|who, power| if *power > 0 { Some(who.clone()) } else { None })]
		#[pallet::nonce_provider]
		#[pallet::fee_payer]
		Delegate(T::AccountId, u32),
		/// A variant unique to pallet16.
		Spectator,
	}
}

pub type AccountId = u64;
pub type Header = generic::Header<u32, BlakeTwo256>;
pub type UncheckedExtrinsic = generic::UncheckedExtrinsic<u64, RuntimeCall, (), ()>;
pub type Block = generic::Block<Header, UncheckedExtrinsic>;

frame_support::construct_runtime!(
	pub enum Runtime {
		System: frame_system,
		Pallet1: pallet1,
		Pallet2: pallet2,
		Pallet3: pallet3,
		Pallet4: pallet4,
		Pallet5: pallet5,
		Pallet6: pallet6,
		Pallet6Instance2: pallet6::<Instance2>,
		Pallet7: pallet7,
		Pallet8: pallet8,
		Pallet9: pallet9,
		Pallet9Instance2: pallet9::<Instance2>,
		Pallet10: pallet10,
		Pallet11: pallet11,
		Pallet12: pallet12,
		Pallet13: pallet13,
		Pallet14: pallet14,
		Pallet15: pallet15,
		Pallet16: pallet16,
	}
);

#[derive_impl(frame_system::config_preludes::TestDefaultConfig)]
impl frame_system::Config for Runtime {
	type Block = Block;
}

impl pallet1::Config for Runtime {}
impl pallet2::Config for Runtime {}
impl pallet3::Config for Runtime {}
impl pallet4::Config for Runtime {}
impl pallet5::Config for Runtime {}
impl pallet6::Config for Runtime {}
impl pallet6::Config<frame_support::instances::Instance2> for Runtime {}
impl pallet7::Config for Runtime {}
impl pallet8::Config for Runtime {}
impl pallet9::Config for Runtime {}
impl pallet9::Config<frame_support::instances::Instance2> for Runtime {}
impl pallet10::Config for Runtime {}
impl pallet11::Config for Runtime {}
impl pallet12::Config for Runtime {}
impl pallet13::Config for Runtime {}
impl pallet14::Config for Runtime {}
impl pallet15::Config for Runtime {}
impl pallet16::Config for Runtime {}

// =============================================================================
// Tests for as_account
// =============================================================================

#[test]
fn test_system_origin_as_account() {
	use frame_system::RawOrigin;

	assert_eq!(OriginCaller::system(RawOrigin::Signed(42)).as_account(), Some(42u64));
	assert_eq!(OriginCaller::system(RawOrigin::Root).as_account(), None);
	assert_eq!(OriginCaller::system(RawOrigin::None).as_account(), None);
}

#[test]
fn test_system_origin_nonce_provider() {
	use frame_system::RawOrigin;

	assert_eq!(OriginCaller::system(RawOrigin::Signed(42)).nonce_provider(), Some(42u64));
	assert_eq!(OriginCaller::system(RawOrigin::Root).nonce_provider(), None);
	assert_eq!(OriginCaller::system(RawOrigin::None).nonce_provider(), None);
}

#[test]
fn test_system_origin_fee_payer() {
	use frame_system::RawOrigin;

	assert_eq!(OriginCaller::system(RawOrigin::Signed(42)).fee_payer(), Some(42u64));
	assert_eq!(OriginCaller::system(RawOrigin::Root).fee_payer(), None);
	assert_eq!(OriginCaller::system(RawOrigin::None).fee_payer(), None);
}

#[test]
fn test_enum_origin_with_as_account() {
	// Pallet1: Member has as_account + nonce_provider
	assert_eq!(
		OriginCaller::Pallet1(pallet1::Origin::Member(42)).as_account(),
		Some(42u64)
	);
	assert_eq!(OriginCaller::Pallet1(pallet1::Origin::Admin).as_account(), None);

	assert_eq!(
		OriginCaller::Pallet1(pallet1::Origin::Member(42)).nonce_provider(),
		Some(42u64)
	);
	assert_eq!(OriginCaller::Pallet1(pallet1::Origin::Admin).nonce_provider(), None);

	// No fee_payer on pallet1
	assert_eq!(
		OriginCaller::Pallet1(pallet1::Origin::Member(42)).fee_payer(),
		None
	);
}

#[test]
fn test_enum_origin_multiple_fields() {
	// Pallet2: WithData has as_account + nonce_provider + fee_payer
	assert_eq!(
		OriginCaller::Pallet2(pallet2::Origin::WithData(42, 100)).as_account(),
		Some(42u64)
	);
	assert_eq!(
		OriginCaller::Pallet2(pallet2::Origin::WithData(42, 100)).nonce_provider(),
		Some(42u64)
	);
	assert_eq!(
		OriginCaller::Pallet2(pallet2::Origin::WithData(42, 100)).fee_payer(),
		Some(42u64)
	);

	// NoAccount has as_account (returning None) but no nonce_provider/fee_payer flags
	assert_eq!(
		OriginCaller::Pallet2(pallet2::Origin::NoAccount(1, 2)).as_account(),
		None
	);
	assert_eq!(
		OriginCaller::Pallet2(pallet2::Origin::NoAccount(1, 2)).nonce_provider(),
		None
	);
	assert_eq!(
		OriginCaller::Pallet2(pallet2::Origin::NoAccount(1, 2)).fee_payer(),
		None
	);
}

#[test]
fn test_non_generic_enum_origin() {
	assert_eq!(
		OriginCaller::Pallet3(pallet3::Origin::StakingAdmin).as_account(),
		None
	);
	assert_eq!(
		OriginCaller::Pallet3(pallet3::Origin::Treasurer).as_account(),
		None
	);
}

#[test]
fn test_struct_origin() {
	use core::marker::PhantomData;

	assert_eq!(
		OriginCaller::Pallet4(pallet4::Origin(PhantomData)).as_account(),
		None
	);
}

#[test]
fn test_function_reference_as_account() {
	assert_eq!(
		OriginCaller::Pallet5(pallet5::Origin::Member(42)).as_account(),
		Some(42u64)
	);
	assert_eq!(
		OriginCaller::Pallet5(pallet5::Origin::Member(42)).nonce_provider(),
		Some(42u64)
	);
	assert_eq!(
		OriginCaller::Pallet5(pallet5::Origin::Member(42)).fee_payer(),
		Some(42u64)
	);
}

#[test]
fn test_instanced_origin() {
	assert_eq!(
		OriginCaller::Pallet6(pallet6::Origin::Member(42)).as_account(),
		Some(42u64)
	);
	assert_eq!(
		OriginCaller::Pallet6(pallet6::Origin::Member(42)).nonce_provider(),
		Some(42u64)
	);
	assert_eq!(
		OriginCaller::Pallet6Instance2(pallet6::Origin::Member(42)).nonce_provider(),
		Some(42u64)
	);
}

#[test]
fn test_account_like_trait_directly() {
	assert_eq!(
		AccountLike::as_account(&pallet1::Origin::<Runtime>::Member(42)),
		Some(42u64)
	);
	assert_eq!(
		AccountLike::as_account(&pallet1::Origin::<Runtime>::Admin),
		None
	);
	assert_eq!(
		AccountLike::nonce_provider(&pallet1::Origin::<Runtime>::Member(42)),
		Some(42u64)
	);
	assert_eq!(
		AccountLike::fee_payer(&pallet1::Origin::<Runtime>::Member(42)),
		None
	);
}

#[test]
fn test_generic_enum_all_variants_as_account() {
	assert_eq!(
		OriginCaller::Pallet7(pallet7::Origin::Member(42)).as_account(),
		Some(42u64)
	);
	assert_eq!(
		OriginCaller::Pallet7(pallet7::Origin::Member(42)).nonce_provider(),
		Some(42u64)
	);
	assert_eq!(
		OriginCaller::Pallet7(pallet7::Origin::Member(42)).fee_payer(),
		Some(42u64)
	);
	assert_eq!(
		OriginCaller::Pallet7(pallet7::Origin::MemberWithData(99, 7)).as_account(),
		Some(99u64)
	);
	assert_eq!(
		OriginCaller::Pallet7(pallet7::Origin::MemberWithData(99, 7)).nonce_provider(),
		Some(99u64)
	);
	// MemberWithData does NOT have fee_payer
	assert_eq!(
		OriginCaller::Pallet7(pallet7::Origin::MemberWithData(99, 7)).fee_payer(),
		None
	);
	assert_eq!(
		OriginCaller::Pallet7(pallet7::Origin::Anonymous(1, 2)).as_account(),
		None
	);
}

#[test]
fn test_non_generic_enum_with_storage() {
	use sp_runtime::BuildStorage;

	let t = RuntimeGenesisConfig { ..Default::default() }.build_storage().unwrap();
	let mut ext = sp_io::TestExternalities::new(t);
	ext.execute_with(|| {
		pallet8::CouncilAccounts::<Runtime>::insert(1u32, 42u64);

		assert_eq!(
			OriginCaller::Pallet8(pallet8::Origin::Council(1)).as_account(),
			Some(42u64)
		);
		assert_eq!(
			OriginCaller::Pallet8(pallet8::Origin::Council(1)).nonce_provider(),
			Some(42u64)
		);
		assert_eq!(
			OriginCaller::Pallet8(pallet8::Origin::Council(999)).as_account(),
			None
		);
		assert_eq!(
			OriginCaller::Pallet8(pallet8::Origin::Members(3, 5)).as_account(),
			None
		);
		assert_eq!(
			OriginCaller::Pallet8(pallet8::Origin::TechCommittee).as_account(),
			None
		);
	});
}

#[test]
fn test_existing_pallets_unaffected() {
	assert_eq!(
		OriginCaller::Pallet3(pallet3::Origin::StakingAdmin).nonce_provider(),
		None
	);
	assert_eq!(
		OriginCaller::Pallet4(pallet4::Origin(core::marker::PhantomData)).nonce_provider(),
		None
	);
}

#[test]
fn test_type_alias_origin_delegates_to_account_like() {
	use pallet9::RawOrigin;

	assert_eq!(
		OriginCaller::Pallet9(RawOrigin::Member(42)).as_account(),
		Some(42u64)
	);
	assert_eq!(
		OriginCaller::Pallet9(RawOrigin::Member(42)).nonce_provider(),
		Some(42u64)
	);
	assert_eq!(
		OriginCaller::Pallet9(RawOrigin::Members(3, 5)).as_account(),
		None
	);

	assert_eq!(
		OriginCaller::Pallet9Instance2(RawOrigin::Member(99)).nonce_provider(),
		Some(99u64)
	);
}

#[test]
fn test_type_alias_custom_origin_with_manual_account_like() {
	use pallet10::CustomOrigin;

	assert_eq!(
		OriginCaller::Pallet10(CustomOrigin::Admin(42)).as_account(),
		Some(42u64)
	);
	assert_eq!(
		OriginCaller::Pallet10(CustomOrigin::Admin(42)).nonce_provider(),
		Some(42u64)
	);
	assert_eq!(
		OriginCaller::Pallet10(CustomOrigin::Admin(42)).fee_payer(),
		Some(42u64)
	);
	assert_eq!(
		OriginCaller::Pallet10(CustomOrigin::Root).as_account(),
		None
	);
}

// =============================================================================
// Tests for as_account without flags (pallet11)
// =============================================================================

#[test]
fn test_as_account_only_no_flags() {
	// Pallet11: Member has as_account but no nonce_provider or fee_payer
	assert_eq!(
		OriginCaller::Pallet11(pallet11::Origin::Member(42)).as_account(),
		Some(42u64)
	);
	assert_eq!(
		OriginCaller::Pallet11(pallet11::Origin::Member(42)).nonce_provider(),
		None
	);
	assert_eq!(
		OriginCaller::Pallet11(pallet11::Origin::Member(42)).fee_payer(),
		None
	);
}

// =============================================================================
// Tests for as_account + fee_payer only (pallet12)
// =============================================================================

#[test]
fn test_as_account_with_fee_payer_only() {
	// Pallet12: Member has as_account + fee_payer but no nonce_provider
	assert_eq!(
		OriginCaller::Pallet12(pallet12::Origin::Member(42)).as_account(),
		Some(42u64)
	);
	assert_eq!(
		OriginCaller::Pallet12(pallet12::Origin::Member(42)).nonce_provider(),
		None
	);
	assert_eq!(
		OriginCaller::Pallet12(pallet12::Origin::Member(42)).fee_payer(),
		Some(42u64)
	);
	assert_eq!(
		OriginCaller::Pallet12(pallet12::Origin::Admin).fee_payer(),
		None
	);
}

// =============================================================================
// Tests for unit variant with as_account (pallet13)
// =============================================================================

#[test]
fn test_as_account_unit_variant() {
	// Pallet13: Member has as_account + nonce_provider, Admin has as_account returning None
	assert_eq!(
		OriginCaller::Pallet13(pallet13::Origin::Member(42)).as_account(),
		Some(42u64)
	);
	assert_eq!(
		OriginCaller::Pallet13(pallet13::Origin::Member(42)).nonce_provider(),
		Some(42u64)
	);
	// Admin is a unit variant with as_account(|| None)
	assert_eq!(
		OriginCaller::Pallet13(pallet13::Origin::Admin).as_account(),
		None
	);
	assert_eq!(
		OriginCaller::Pallet13(pallet13::Origin::Admin).nonce_provider(),
		None
	);
}

// =============================================================================
// Tests for named-fields variant with as_account (pallet14)
// =============================================================================

#[test]
fn test_as_account_named_fields_variant() {
	// Pallet14: Member { who, rank } has as_account + nonce_provider + fee_payer
	assert_eq!(
		OriginCaller::Pallet14(pallet14::Origin::Member { who: 42, rank: 3 }).as_account(),
		Some(42u64)
	);
	assert_eq!(
		OriginCaller::Pallet14(pallet14::Origin::Member { who: 42, rank: 3 }).nonce_provider(),
		Some(42u64)
	);
	assert_eq!(
		OriginCaller::Pallet14(pallet14::Origin::Member { who: 42, rank: 3 }).fee_payer(),
		Some(42u64)
	);
	// Different rank, same account
	assert_eq!(
		OriginCaller::Pallet14(pallet14::Origin::Member { who: 42, rank: 99 }).as_account(),
		Some(42u64)
	);
	// Admin has no as_account
	assert_eq!(
		OriginCaller::Pallet14(pallet14::Origin::Admin).as_account(),
		None
	);
	assert_eq!(
		OriginCaller::Pallet14(pallet14::Origin::Admin).nonce_provider(),
		None
	);
	assert_eq!(
		OriginCaller::Pallet14(pallet14::Origin::Admin).fee_payer(),
		None
	);
}

// =============================================================================
// Test: two pallets with identically-named Origin enums and shared variant names
// =============================================================================

#[test]
fn test_same_origin_and_variant_names_are_independent() {
	// pallet15::Origin::Delegate always returns Some(account), ignoring the weight field.
	assert_eq!(
		OriginCaller::Pallet15(pallet15::Origin::Delegate(42, 0)).as_account(),
		Some(42u64)
	);
	assert_eq!(
		OriginCaller::Pallet15(pallet15::Origin::Delegate(42, 999)).as_account(),
		Some(42u64)
	);
	// pallet15::Delegate has nonce_provider but NOT fee_payer.
	assert_eq!(
		OriginCaller::Pallet15(pallet15::Origin::Delegate(42, 0)).nonce_provider(),
		Some(42u64)
	);
	assert_eq!(
		OriginCaller::Pallet15(pallet15::Origin::Delegate(42, 0)).fee_payer(),
		None
	);
	// pallet15::Treasurer has fee_payer but NOT nonce_provider.
	assert_eq!(
		OriginCaller::Pallet15(pallet15::Origin::Treasurer(7)).as_account(),
		Some(7u64)
	);
	assert_eq!(
		OriginCaller::Pallet15(pallet15::Origin::Treasurer(7)).nonce_provider(),
		None
	);
	assert_eq!(
		OriginCaller::Pallet15(pallet15::Origin::Treasurer(7)).fee_payer(),
		Some(7u64)
	);

	// pallet16::Origin::Delegate returns None when power == 0 (disabled delegate).
	assert_eq!(
		OriginCaller::Pallet16(pallet16::Origin::Delegate(42, 0)).as_account(),
		None
	);
	// pallet16::Delegate returns Some when power > 0.
	assert_eq!(
		OriginCaller::Pallet16(pallet16::Origin::Delegate(42, 5)).as_account(),
		Some(42u64)
	);
	// pallet16::Delegate has all three flags, but they all use the same closure.
	assert_eq!(
		OriginCaller::Pallet16(pallet16::Origin::Delegate(42, 5)).nonce_provider(),
		Some(42u64)
	);
	assert_eq!(
		OriginCaller::Pallet16(pallet16::Origin::Delegate(42, 5)).fee_payer(),
		Some(42u64)
	);
	// Disabled delegate: all return None.
	assert_eq!(
		OriginCaller::Pallet16(pallet16::Origin::Delegate(42, 0)).nonce_provider(),
		None
	);
	assert_eq!(
		OriginCaller::Pallet16(pallet16::Origin::Delegate(42, 0)).fee_payer(),
		None
	);
	// pallet16::Spectator has no as_account at all.
	assert_eq!(
		OriginCaller::Pallet16(pallet16::Origin::Spectator).as_account(),
		None
	);
}

// =============================================================================
// Test: direct AccountLike trait on non-generic origin type (pallet8)
// =============================================================================

#[test]
fn test_non_generic_origin_account_like_trait_directly() {
	// For non-generic enum origins, the AccountLike trait impl on the origin type
	// itself uses default implementations (returns None), because the closures need
	// access to T. The actual behavior is routed through OriginCaller.
	assert_eq!(AccountLike::<u64>::as_account(&pallet8::Origin::Council(1)), None);
	assert_eq!(AccountLike::<u64>::nonce_provider(&pallet8::Origin::Council(1)), None);
	assert_eq!(AccountLike::<u64>::fee_payer(&pallet8::Origin::Council(1)), None);

	// But through OriginCaller, it works correctly (with storage).
	use sp_runtime::BuildStorage;
	let t = RuntimeGenesisConfig { ..Default::default() }.build_storage().unwrap();
	let mut ext = sp_io::TestExternalities::new(t);
	ext.execute_with(|| {
		pallet8::CouncilAccounts::<Runtime>::insert(1u32, 42u64);
		assert_eq!(
			OriginCaller::Pallet8(pallet8::Origin::Council(1)).as_account(),
			Some(42u64)
		);
	});
}
