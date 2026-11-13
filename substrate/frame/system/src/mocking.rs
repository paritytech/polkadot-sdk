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

//! Provide types to help defining a mock environment when testing pallets.

use sp_runtime::generic;

/// An unchecked extrinsic type to be used in tests.
pub type MockUncheckedExtrinsic<T, Signature = (), Extra = ()> = generic::UncheckedExtrinsic<
	<T as crate::Config>::AccountId,
	<T as crate::Config>::RuntimeCall,
	Signature,
	Extra,
>;

/// An implementation of `sp_runtime::traits::Block` to be used in tests.
pub type MockBlock<T, Signature = (), Extra = ()> = generic::Block<
	generic::Header<u64, sp_runtime::traits::BlakeTwo256>,
	MockUncheckedExtrinsic<T, Signature, Extra>,
>;

/// An implementation of `sp_runtime::traits::Block` to be used in tests with u32 BlockNumber type.
pub type MockBlockU32<T, Signature = (), Extra = ()> = generic::Block<
	generic::Header<u32, sp_runtime::traits::BlakeTwo256>,
	MockUncheckedExtrinsic<T, Signature, Extra>,
>;

/// An implementation of `sp_runtime::traits::Block` to be used in tests with u128 BlockNumber
/// type.
pub type MockBlockU128<T, Signature = (), Extra = ()> = generic::Block<
	generic::Header<u128, sp_runtime::traits::BlakeTwo256>,
	MockUncheckedExtrinsic<T, Signature, Extra>,
>;

/// A minimal pallet with a custom origin for testing `AccountLike` integration
/// with `CheckNonce` and transaction payment extensions.
///
/// Variants cover every combination of `as_account`, `nonce_provider`, and `fee_payer`:
/// - `Member(AccountId)`: `as_account` + `nonce_provider` + `fee_payer`
/// - `NonceOnly(AccountId)`: `as_account` + `nonce_provider`
/// - `FeeOnly(AccountId)`: `as_account` + `fee_payer`
/// - `NonPaying(AccountId)`: `as_account` only (no nonce/fee)
/// - `Council`: no account mapping at all
#[frame_support::pallet(dev_mode)]
pub mod pallet_with_custom_origin {
	use crate as frame_system;
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
		/// A member with full account integration (nonce + fee payment).
		#[pallet::as_account(|who| Some(who.clone()))]
		#[pallet::nonce_provider]
		#[pallet::fee_payer]
		Member(T::AccountId),
		/// An account origin that only participates in nonce tracking (no fee payment).
		#[pallet::as_account(|who| Some(who.clone()))]
		#[pallet::nonce_provider]
		NonceOnly(T::AccountId),
		/// An account origin that only pays fees (no nonce tracking).
		#[pallet::as_account(|who| Some(who.clone()))]
		#[pallet::fee_payer]
		FeeOnly(T::AccountId),
		/// An account origin that does NOT participate in nonce/fee tracking.
		#[pallet::as_account(|who| Some(who.clone()))]
		NonPaying(T::AccountId),
		/// A governance-style origin with no account mapping.
		Council,
	}
}
