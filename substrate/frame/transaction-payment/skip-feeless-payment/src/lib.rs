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
//! dispatchables marked by
//! [`#[pallet::feeless_if]`](frame_support::pallet_prelude::feeless_if).
//!
//! ## Overview
//!
//! It does this by wrapping an existing [`TransactionExtension`] implementation (e.g.
//! [`pallet-transaction-payment`]) and checking if the dispatchable is feeless before applying the
//! wrapped extension. If the dispatchable is indeed feeless, the extension is skipped and a custom
//! event is emitted instead. Otherwise, the extension is applied as usual.
//!
//!
//! ## Integration
//!
//! This pallet wraps an existing transaction payment pallet. This means you should both pallets
//! in your [`construct_runtime`](frame_support::construct_runtime) macro and
//! include this pallet's [`TransactionExtension`] ([`SkipCheckIfFeeless`]) that would accept the
//! existing one as an argument.

#![cfg_attr(not(feature = "std"), no_std)]

use codec::{Decode, Encode};
use frame_support::{
	dispatch::{CheckIfFeeless, DispatchResult},
	traits::{IsType, OriginTrait},
	weights::Weight,
};
use scale_info::{StaticTypeInfo, TypeInfo};
use sp_runtime::{
	traits::{
		DispatchInfoOf, DispatchOriginOf, PostDispatchInfoOf, TransactionExtension, ValidateResult,
	},
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
		FeeSkipped { origin: <T::RuntimeOrigin as OriginTrait>::PalletsOrigin },
	}
}

/// A [`TransactionExtension`] that skips the wrapped extension if the dispatchable is feeless.
#[derive(Encode, Decode, Clone, Eq, PartialEq)]
pub struct SkipCheckIfFeeless<T, S>(pub S, core::marker::PhantomData<T>);

// Make this extension "invisible" from the outside (ie metadata type information)
impl<T, S: StaticTypeInfo> TypeInfo for SkipCheckIfFeeless<T, S> {
	type Identity = S;
	fn type_info() -> scale_info::Type {
		S::type_info()
	}
}

impl<T, S: Encode> core::fmt::Debug for SkipCheckIfFeeless<T, S> {
	#[cfg(feature = "std")]
	fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
		write!(f, "SkipCheckIfFeeless<{:?}>", self.0.encode())
	}
	#[cfg(not(feature = "std"))]
	fn fmt(&self, _: &mut core::fmt::Formatter) -> core::fmt::Result {
		Ok(())
	}
}

impl<T, S> From<S> for SkipCheckIfFeeless<T, S> {
	fn from(s: S) -> Self {
		Self(s, core::marker::PhantomData)
	}
}

pub enum Intermediate<T, O> {
	/// The wrapped extension should be applied.
	Apply(T),
	/// The wrapped extension should be skipped.
	Skip(O),
}
use Intermediate::*;

impl<T: Config + Send + Sync, S: TransactionExtension<T::RuntimeCall>>
	TransactionExtension<T::RuntimeCall> for SkipCheckIfFeeless<T, S>
where
	T::RuntimeCall: CheckIfFeeless<Origin = frame_system::pallet_prelude::OriginFor<T>>,
{
	// From the outside this extension should be "invisible", because it just extends the wrapped
	// extension with an extra check in `pre_dispatch` and `post_dispatch`. Thus, we should forward
	// the identifier of the wrapped extension to let wallets see this extension as it would only be
	// the wrapped extension itself.
	const IDENTIFIER: &'static str = S::IDENTIFIER;
	type Implicit = S::Implicit;

	fn implicit(&self) -> Result<Self::Implicit, TransactionValidityError> {
		self.0.implicit()
	}
	type Val =
		Intermediate<S::Val, <DispatchOriginOf<T::RuntimeCall> as OriginTrait>::PalletsOrigin>;
	type Pre =
		Intermediate<S::Pre, <DispatchOriginOf<T::RuntimeCall> as OriginTrait>::PalletsOrigin>;

	fn weight(&self, call: &T::RuntimeCall) -> frame_support::weights::Weight {
		self.0.weight(call)
	}

	fn validate(
		&self,
		origin: DispatchOriginOf<T::RuntimeCall>,
		call: &T::RuntimeCall,
		info: &DispatchInfoOf<T::RuntimeCall>,
		len: usize,
		self_implicit: S::Implicit,
		inherited_implication: &impl Encode,
	) -> ValidateResult<Self::Val, T::RuntimeCall> {
		if call.is_feeless(&origin) {
			Ok((Default::default(), Skip(origin.caller().clone()), origin))
		} else {
			let (x, y, z) =
				self.0.validate(origin, call, info, len, self_implicit, inherited_implication)?;
			Ok((x, Apply(y), z))
		}
	}

	fn prepare(
		self,
		val: Self::Val,
		origin: &DispatchOriginOf<T::RuntimeCall>,
		call: &T::RuntimeCall,
		info: &DispatchInfoOf<T::RuntimeCall>,
		len: usize,
	) -> Result<Self::Pre, TransactionValidityError> {
		match val {
			Apply(val) => self.0.prepare(val, origin, call, info, len).map(Apply),
			Skip(origin) => Ok(Skip(origin)),
		}
	}

	fn post_dispatch_details(
		pre: Self::Pre,
		info: &DispatchInfoOf<T::RuntimeCall>,
		post_info: &PostDispatchInfoOf<T::RuntimeCall>,
		len: usize,
		result: &DispatchResult,
	) -> Result<Weight, TransactionValidityError> {
		match pre {
			Apply(pre) => S::post_dispatch_details(pre, info, post_info, len, result),
			Skip(origin) => {
				Pallet::<T>::deposit_event(Event::<T>::FeeSkipped { origin });
				Ok(Weight::zero())
			},
		}
	}
}
