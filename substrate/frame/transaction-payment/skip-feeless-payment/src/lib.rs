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
//
//! # Skip Feeless Payment Pallet
//!
//! This pallet allows runtimes that include it to skip payment of transaction fees for
//! dispatchables marked by [`#[pallet::feeless_if]`](`macro@
//! frame_support::pallet_prelude::feeless_if`).
//!
//! ## Overview
//!
//! It does this by wrapping an existing [`SignedExtension`] implementation (e.g.
//! [`pallet-transaction-payment`]) and checking if the dispatchable is feeless before applying the
//! wrapped extension. If the dispatchable is indeed feeless, the extension is skipped and a custom
//! event is emitted instead. Otherwise, the extension is applied as usual.
//!
//!
//! ## Integration
//!
//! This pallet wraps an existing transaction payment pallet. This means you should both pallets
//! in your `construct_runtime` macro and include this pallet's
//! [`SignedExtension`] ([`SkipCheckIfFeeless`]) that would accept the existing one as an argument.

#![cfg_attr(not(feature = "std"), no_std)]

use codec::{Decode, Encode};
use frame_support::{
	dispatch::{CheckIfFeeless, DispatchResult},
	traits::{IsType, OriginTrait},
};
use scale_info::TypeInfo;
use sp_runtime::{
	traits::{DispatchInfoOf, PostDispatchInfoOf, SignedExtension},
	transaction_validity::TransactionValidityError,
};

#[cfg(test)]
mod mock;
#[cfg(test)]
mod tests;

pub use pallet::*;

#[frame_support::pallet]
pub mod pallet {
	use super::*;

	#[pallet::config]
	pub trait Config: frame_system::Config {
		/// The overarching event type.
		type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;
	}

	#[pallet::pallet]
	pub struct Pallet<T>(_);

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {
		/// A transaction fee was skipped.
		FeeSkipped { who: T::AccountId },
	}
}

/// A [`SignedExtension`] that skips the wrapped extension if the dispatchable is feeless.
#[derive(Encode, Decode, Clone, Eq, PartialEq, TypeInfo)]
#[scale_info(skip_type_params(T))]
pub struct SkipCheckIfFeeless<T, S>(pub S, sp_std::marker::PhantomData<T>);

impl<T, S: Encode> sp_std::fmt::Debug for SkipCheckIfFeeless<T, S> {
	#[cfg(feature = "std")]
	fn fmt(&self, f: &mut sp_std::fmt::Formatter) -> sp_std::fmt::Result {
		write!(f, "SkipCheckIfFeeless<{:?}>", self.0.encode())
	}
	#[cfg(not(feature = "std"))]
	fn fmt(&self, _: &mut sp_std::fmt::Formatter) -> sp_std::fmt::Result {
		Ok(())
	}
}

impl<T, S> From<S> for SkipCheckIfFeeless<T, S> {
	fn from(s: S) -> Self {
		Self(s, sp_std::marker::PhantomData)
	}
}

impl<T: Config + Send + Sync, S: SignedExtension<AccountId = T::AccountId>> SignedExtension
	for SkipCheckIfFeeless<T, S>
where
	S::Call: CheckIfFeeless<Origin = frame_system::pallet_prelude::OriginFor<T>>,
{
	type AccountId = T::AccountId;
	type Call = S::Call;
	type AdditionalSigned = S::AdditionalSigned;
	type Pre = (Self::AccountId, Option<<S as SignedExtension>::Pre>);
	// From the outside this extension should be "invisible", because it just extends the wrapped
	// extension with an extra check in `pre_dispatch` and `post_dispatch`. Thus, we should forward
	// the identifier of the wrapped extension to let wallets see this extension as it would only be
	// the wrapped extension itself.
	const IDENTIFIER: &'static str = S::IDENTIFIER;

	fn additional_signed(&self) -> Result<Self::AdditionalSigned, TransactionValidityError> {
		self.0.additional_signed()
	}

	fn pre_dispatch(
		self,
		who: &Self::AccountId,
		call: &Self::Call,
		info: &DispatchInfoOf<Self::Call>,
		len: usize,
	) -> Result<Self::Pre, TransactionValidityError> {
		if call.is_feeless(&<T as frame_system::Config>::RuntimeOrigin::signed(who.clone())) {
			Ok((who.clone(), None))
		} else {
			Ok((who.clone(), Some(self.0.pre_dispatch(who, call, info, len)?)))
		}
	}

	fn post_dispatch(
		pre: Option<Self::Pre>,
		info: &DispatchInfoOf<Self::Call>,
		post_info: &PostDispatchInfoOf<Self::Call>,
		len: usize,
		result: &DispatchResult,
	) -> Result<(), TransactionValidityError> {
		if let Some(pre) = pre {
			if let Some(pre) = pre.1 {
				S::post_dispatch(Some(pre), info, post_info, len, result)?;
			} else {
				Pallet::<T>::deposit_event(Event::<T>::FeeSkipped { who: pre.0 });
			}
		}
		Ok(())
	}
}
