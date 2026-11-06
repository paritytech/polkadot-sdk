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

//! Various pieces of common functionality.

use crate::*;
use alloc::vec::Vec;
use frame_support::{
	pallet_prelude::*,
	traits::tokens::fungible::{InspectHold, MutateHold},
};

impl<T: Config<I>, I: 'static> Pallet<T, I> {
	/// The hold reason used for all native-token deposits taken by this pallet.
	pub(crate) fn deposit_reason() -> T::RuntimeHoldReason {
		HoldReason::<I>::Deposit.into()
	}

	/// Take a native-token deposit of `amount` from `who` as a hold.
	///
	/// A zero amount is a no-op (mirrors `reserve`), so callers that may pass a zero deposit (e.g.
	/// force-created collections) do not fail on provider-less accounts.
	pub(crate) fn hold_deposit(
		who: &T::AccountId,
		amount: DepositBalanceOf<T, I>,
	) -> DispatchResult {
		if amount.is_zero() {
			return Ok(());
		}
		T::Currency::hold(&Self::deposit_reason(), who, amount)
	}

	/// Release a native-token deposit of `amount` previously taken from `who`.
	///
	/// Drains the new hold first, then unreserves any remainder from a pre-migration legacy
	/// reserve. This lazily migrates old reserves to holds: new deposits are taken as holds, and
	/// pre-existing reserves are released here as the entities holding them are torn down.
	pub(crate) fn release_deposit(who: &T::AccountId, amount: DepositBalanceOf<T, I>) {
		let reason = Self::deposit_reason();
		let from_hold = amount.min(T::Currency::balance_on_hold(&reason, who));
		if !from_hold.is_zero() {
			let r = T::Currency::release(&reason, who, from_hold, BestEffort);
			debug_assert!(r.is_ok());
		}
		let from_reserve = amount.saturating_sub(from_hold);
		if !from_reserve.is_zero() {
			let remaining = T::OldCurrency::unreserve(who, from_reserve);
			debug_assert!(remaining.is_zero());
		}
	}

	/// Move a native-token deposit of `amount` from `from` to `to`, keeping it on deposit.
	///
	/// Mirrors [`Self::release_deposit`]'s lazy migration: the held portion is moved as a hold and
	/// any legacy-reserved remainder is repatriated as a reserve on `to`.
	pub(crate) fn repatriate_deposit(
		from: &T::AccountId,
		to: &T::AccountId,
		amount: DepositBalanceOf<T, I>,
	) -> DispatchResult {
		let reason = Self::deposit_reason();
		let from_hold = amount.min(T::Currency::balance_on_hold(&reason, from));
		if !from_hold.is_zero() {
			T::Currency::transfer_on_hold(
				&reason,
				from,
				to,
				from_hold,
				Exact,
				OnHold,
				Fortitude::Polite,
			)?;
		}
		let from_reserve = amount.saturating_sub(from_hold);
		if !from_reserve.is_zero() {
			T::OldCurrency::repatriate_reserved(from, to, from_reserve, Reserved)?;
		}
		Ok(())
	}

	/// Get the owner of the item, if the item exists.
	pub fn owner(collection: T::CollectionId, item: T::ItemId) -> Option<T::AccountId> {
		Item::<T, I>::get(collection, item).map(|i| i.owner)
	}

	/// Get the owner of the collection, if the collection exists.
	pub fn collection_owner(collection: T::CollectionId) -> Option<T::AccountId> {
		Collection::<T, I>::get(collection).map(|i| i.owner)
	}

	/// Validates the signature of the given data with the provided signer's account ID.
	///
	/// # Errors
	///
	/// This function returns a [`WrongSignature`](crate::Error::WrongSignature) error if the
	/// signature is invalid or the verification process fails.
	pub fn validate_signature(
		data: &Vec<u8>,
		signature: &T::OffchainSignature,
		signer: &T::AccountId,
	) -> DispatchResult {
		if signature.verify(&**data, &signer) {
			return Ok(());
		}

		// NOTE: for security reasons modern UIs implicitly wrap the data requested to sign into
		// <Bytes></Bytes>, that's why we support both wrapped and raw versions.
		let prefix = b"<Bytes>";
		let suffix = b"</Bytes>";
		let mut wrapped: Vec<u8> = Vec::with_capacity(data.len() + prefix.len() + suffix.len());
		wrapped.extend(prefix);
		wrapped.extend(data);
		wrapped.extend(suffix);

		ensure!(signature.verify(&*wrapped, &signer), Error::<T, I>::WrongSignature);

		Ok(())
	}

	pub(crate) fn set_next_collection_id(collection: T::CollectionId) {
		let next_id = collection.increment();
		NextCollectionId::<T, I>::set(next_id);
		Self::deposit_event(Event::NextCollectionIdIncremented { next_id });
	}

	#[cfg(any(test, feature = "runtime-benchmarks"))]
	pub fn set_next_id(id: T::CollectionId) {
		NextCollectionId::<T, I>::set(Some(id));
	}

	#[cfg(test)]
	pub fn get_next_id() -> T::CollectionId {
		NextCollectionId::<T, I>::get()
			.or(T::CollectionId::initial_value())
			.expect("Failed to get next collection ID")
	}
}
