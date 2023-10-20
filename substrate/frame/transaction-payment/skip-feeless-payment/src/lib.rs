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

#![cfg_attr(not(feature = "std"), no_std)]

use codec::{Decode, Encode};
use frame_support::{
	dispatch::{CheckIfFeeless, DispatchResult},
	traits::IsType,
};
use scale_info::TypeInfo;
use sp_runtime::{
	traits::{DispatchInfoOf, PostDispatchInfoOf, SignedExtension},
	transaction_validity::TransactionValidityError,
};

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

#[derive(Encode, Decode, Clone, Eq, PartialEq, TypeInfo)]
#[scale_info(skip_type_params(T))]
pub struct SkipCheckIfFeeless<T: Config, S: SignedExtension>(pub S, sp_std::marker::PhantomData<T>);

impl<T: Config, S: SignedExtension> sp_std::fmt::Debug for SkipCheckIfFeeless<T, S> {
	#[cfg(feature = "std")]
	fn fmt(&self, f: &mut sp_std::fmt::Formatter) -> sp_std::fmt::Result {
		write!(f, "SkipCheckIfFeeless<{:?}>", self.0.encode())
	}
	#[cfg(not(feature = "std"))]
	fn fmt(&self, _: &mut sp_std::fmt::Formatter) -> sp_std::fmt::Result {
		Ok(())
	}
}

impl<T: Config + Send + Sync, S: SignedExtension> SkipCheckIfFeeless<T, S> {
	/// utility constructor. Used only in client/factory code.
	pub fn from(s: S) -> Self {
		Self(s, sp_std::marker::PhantomData)
	}
}

impl<T: Config + Send + Sync, S: SignedExtension<AccountId = T::AccountId>> SignedExtension
	for SkipCheckIfFeeless<T, S>
where
	S::Call: CheckIfFeeless<AccountId = T::AccountId>,
{
	type AccountId = T::AccountId;
	type Call = S::Call;
	type AdditionalSigned = ();
	type Pre = (Self::AccountId, Option<<S as SignedExtension>::Pre>);
	const IDENTIFIER: &'static str = "SkipCheckIfFeeless";

	fn additional_signed(&self) -> Result<(), TransactionValidityError> {
		Ok(())
	}

	fn pre_dispatch(
		self,
		who: &Self::AccountId,
		call: &Self::Call,
		info: &DispatchInfoOf<Self::Call>,
		len: usize,
	) -> Result<Self::Pre, TransactionValidityError> {
		if call.is_feeless(who) {
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
