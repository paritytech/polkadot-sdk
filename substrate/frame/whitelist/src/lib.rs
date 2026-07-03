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

//! # Whitelist Pallet
//!
//! - [`Config`]
//! - [`Call`]
//!
//! ## Overview
//!
//! Allow some configurable origin: [`Config::WhitelistOrigin`] to whitelist some hash of a call,
//! and allow another configurable origin: [`Config::DispatchWhitelistedOrigin`] to dispatch them
//! with the root origin.
//!
//! In the meantime the call corresponding to the hash must have been submitted to the pre-image
//! handler [`pallet::Config::Preimages`].

#![cfg_attr(not(feature = "std"), no_std)]

#[cfg(feature = "runtime-benchmarks")]
mod benchmarking;
#[cfg(test)]
mod mock;
#[cfg(test)]
mod tests;
pub mod weights;
pub use weights::WeightInfo;

extern crate alloc;

use alloc::{boxed::Box, vec};
use codec::{DecodeLimit, Encode, FullCodec};
use frame::{
	deps::sp_runtime::transaction_validity::{
		TransactionSource, TransactionValidityError, TransactionValidityWithRefund,
	},
	prelude::*,
	traits::{QueryPreimage, StorePreimage},
};
use scale_info::TypeInfo;

pub use pallet::*;

#[frame::pallet]
pub mod pallet {
	use super::*;

	#[pallet::config]
	pub trait Config: frame_system::Config {
		/// The overarching event type.
		#[allow(deprecated)]
		type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;

		/// The overarching call type.
		type RuntimeCall: IsType<<Self as frame_system::Config>::RuntimeCall>
			+ Dispatchable<RuntimeOrigin = Self::RuntimeOrigin, PostInfo = PostDispatchInfo>
			+ GetDispatchInfo
			+ FullCodec
			+ TypeInfo
			+ From<frame_system::Call<Self>>
			+ Parameter;

		/// Required origin for whitelisting a call.
		type WhitelistOrigin: EnsureOrigin<Self::RuntimeOrigin>;

		/// Origin allowed to dispatch an already-whitelisted call (the inner call runs with
		/// `Root`).
		///
		/// Conventionally a privileged origin. Accepting the `Authorized` system origin (e.g.
		/// via `frame_system::EnsureAuthorized`) additionally enables permissionless dispatch:
		/// the `#[pallet::authorize]` callbacks probe `try_origin(Authorized)` at pool admission
		/// and admit unsigned, fee-free submissions of whitelisted calls, with
		/// `frame_system::AuthorizeCall` converting the `None` origin into `Authorized`.
		///
		/// When only `Authorized` is accepted, the dispatch calls have no privileged entrypoint
		/// and are reachable solely through this permissionless flow.
		type DispatchWhitelistedOrigin: EnsureOrigin<Self::RuntimeOrigin>;

		/// The handler of pre-images.
		type Preimages: QueryPreimage<H = Self::Hashing> + StorePreimage;

		/// The weight information for this pallet.
		type WeightInfo: WeightInfo;
	}

	#[pallet::pallet]
	pub struct Pallet<T>(_);

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {
		CallWhitelisted { call_hash: T::Hash },
		WhitelistedCallRemoved { call_hash: T::Hash },
		WhitelistedCallDispatched { call_hash: T::Hash, result: DispatchResultWithPostInfo },
	}

	#[pallet::error]
	pub enum Error<T> {
		/// The preimage of the call hash could not be loaded.
		UnavailablePreImage,
		/// The call could not be decoded.
		UndecodableCall,
		/// The weight of the decoded call was higher than the witness.
		InvalidCallWeightWitness,
		/// The call was not whitelisted.
		CallIsNotWhitelisted,
		/// The call was already whitelisted; No-Op.
		CallAlreadyWhitelisted,
	}

	#[pallet::storage]
	pub type WhitelistedCall<T: Config> = StorageMap<_, Twox64Concat, T::Hash, (), OptionQuery>;

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		#[pallet::call_index(0)]
		#[pallet::weight(T::WeightInfo::whitelist_call())]
		pub fn whitelist_call(origin: OriginFor<T>, call_hash: T::Hash) -> DispatchResult {
			T::WhitelistOrigin::ensure_origin(origin)?;

			ensure!(
				!WhitelistedCall::<T>::contains_key(call_hash),
				Error::<T>::CallAlreadyWhitelisted,
			);

			WhitelistedCall::<T>::insert(call_hash, ());
			T::Preimages::request(&call_hash);

			Self::deposit_event(Event::<T>::CallWhitelisted { call_hash });

			Ok(())
		}

		#[pallet::call_index(1)]
		#[pallet::weight(T::WeightInfo::remove_whitelisted_call())]
		pub fn remove_whitelisted_call(origin: OriginFor<T>, call_hash: T::Hash) -> DispatchResult {
			T::WhitelistOrigin::ensure_origin(origin)?;

			WhitelistedCall::<T>::take(call_hash).ok_or(Error::<T>::CallIsNotWhitelisted)?;

			T::Preimages::unrequest(&call_hash);

			Self::deposit_event(Event::<T>::WhitelistedCallRemoved { call_hash });

			Ok(())
		}

		#[pallet::authorize(Self::authorize_dispatch_whitelisted_call)]
		#[pallet::weight_of_authorize(T::DbWeight::get().reads(1))]
		#[pallet::call_index(2)]
		#[pallet::weight(
			T::WeightInfo::dispatch_whitelisted_call(*call_encoded_len)
				.saturating_add(*call_weight_witness)
		)]
		pub fn dispatch_whitelisted_call(
			origin: OriginFor<T>,
			call_hash: T::Hash,
			call_encoded_len: u32,
			call_weight_witness: Weight,
		) -> DispatchResultWithPostInfo {
			T::DispatchWhitelistedOrigin::ensure_origin(origin)?;

			ensure!(
				WhitelistedCall::<T>::contains_key(call_hash),
				Error::<T>::CallIsNotWhitelisted,
			);

			let call = T::Preimages::fetch(&call_hash, Some(call_encoded_len))
				.map_err(|_| Error::<T>::UnavailablePreImage)?;

			let call = <T as Config>::RuntimeCall::decode_all_with_depth_limit(
				frame::deps::frame_support::MAX_EXTRINSIC_DEPTH,
				&mut &call[..],
			)
			.map_err(|_| Error::<T>::UndecodableCall)?;

			ensure!(
				call.get_dispatch_info().call_weight.all_lte(call_weight_witness),
				Error::<T>::InvalidCallWeightWitness
			);

			let actual_weight = Self::clean_and_dispatch(call_hash, call).map(|w| {
				w.saturating_add(T::WeightInfo::dispatch_whitelisted_call(call_encoded_len))
			});

			Ok(actual_weight.into())
		}

		#[pallet::authorize(Self::authorize_dispatch_whitelisted_call_with_preimage)]
		#[pallet::weight_of_authorize(T::DbWeight::get().reads(1))]
		#[pallet::call_index(3)]
		#[pallet::weight({
			let call_weight = call.get_dispatch_info().call_weight;
			let call_len = call.encoded_size() as u32;

			T::WeightInfo::dispatch_whitelisted_call_with_preimage(call_len)
				.saturating_add(call_weight)
		})]
		pub fn dispatch_whitelisted_call_with_preimage(
			origin: OriginFor<T>,
			call: Box<<T as Config>::RuntimeCall>,
		) -> DispatchResultWithPostInfo {
			T::DispatchWhitelistedOrigin::ensure_origin(origin)?;

			let call_hash = T::Hashing::hash_of(&call).into();

			ensure!(
				WhitelistedCall::<T>::contains_key(call_hash),
				Error::<T>::CallIsNotWhitelisted,
			);

			let call_len = call.encoded_size() as u32;
			let actual_weight = Self::clean_and_dispatch(call_hash, *call).map(|w| {
				w.saturating_add(T::WeightInfo::dispatch_whitelisted_call_with_preimage(call_len))
			});

			Ok(actual_weight.into())
		}
	}
}

impl<T: Config> Pallet<T> {
	/// Pool-level authorization shared by both permissionless dispatch calls.
	///
	/// Admits the unsigned submission only when [`Config::DispatchWhitelistedOrigin`] accepts
	/// the `Authorized` system origin and `call_hash` is present in [`WhitelistedCall`].
	fn authorize_whitelisted_dispatch(call_hash: T::Hash) -> TransactionValidityWithRefund {
		// Opt-in probe: reject at the pool unless the dispatch body would accept `Authorized`.
		let authorized: T::RuntimeOrigin =
			frame_system::RawOrigin::<T::AccountId>::Authorized.into();
		T::DispatchWhitelistedOrigin::try_origin(authorized)
			.map_err(|_| TransactionValidityError::Invalid(InvalidTransaction::Call))?;

		if !WhitelistedCall::<T>::contains_key(call_hash) {
			return Err(TransactionValidityError::Invalid(InvalidTransaction::Call));
		}

		Ok((
			ValidTransaction { provides: vec![call_hash.encode()], ..Default::default() },
			Weight::zero(),
		))
	}

	/// [`pallet::authorize`] callback for [`Pallet::dispatch_whitelisted_call`]; the hash is taken
	/// directly from the `call_hash` argument.
	fn authorize_dispatch_whitelisted_call(
		_source: TransactionSource,
		call_hash: &T::Hash,
		_call_encoded_len: &u32,
		_call_weight_witness: &Weight,
	) -> TransactionValidityWithRefund {
		Self::authorize_whitelisted_dispatch(*call_hash)
	}

	/// [`pallet::authorize`] callback for [`Pallet::dispatch_whitelisted_call_with_preimage`]; the
	/// hash is computed from the inline call.
	fn authorize_dispatch_whitelisted_call_with_preimage(
		_source: TransactionSource,
		call: &Box<<T as Config>::RuntimeCall>,
	) -> TransactionValidityWithRefund {
		let call_hash = T::Hashing::hash_of(call).into();
		Self::authorize_whitelisted_dispatch(call_hash)
	}

	/// Clean whitelisting/preimage and dispatch call.
	///
	/// Return the call actual weight of the dispatched call if there is some.
	fn clean_and_dispatch(call_hash: T::Hash, call: <T as Config>::RuntimeCall) -> Option<Weight> {
		WhitelistedCall::<T>::remove(call_hash);

		T::Preimages::unrequest(&call_hash);

		let result = call.dispatch(frame_system::Origin::<T>::Root.into());

		let call_actual_weight = match result {
			Ok(call_post_info) => call_post_info.actual_weight,
			Err(call_err) => call_err.post_info.actual_weight,
		};

		Self::deposit_event(Event::<T>::WhitelistedCallDispatched { call_hash, result });

		call_actual_weight
	}
}
