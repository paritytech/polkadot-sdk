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
//! It does this by wrapping an existing [`TransactionExtension`] implementation (e.g.
//! [`pallet-transaction-payment`]) and checking if the dispatchable is feeless before applying the
//! wrapped extension. If the dispatchable is indeed feeless, the extension is skipped and a custom
//! event is emitted instead. Otherwise, the extension is applied as usual.
//!
//!
//! ## Integration
//!
//! This pallet wraps an existing transaction payment pallet. This means you should both pallets
//! in your `construct_runtime` macro and include this pallet's
//! [`TransactionExtension`] ([`SkipCheckIfFeeless`]) that would accept the existing one as an
//! argument.

#![cfg_attr(not(feature = "std"), no_std)]

use codec::{Decode, Encode};
use frame_support::{
	dispatch::{CheckIfFeeless, DispatchResult},
	traits::{IsType, OriginTrait},
};
use scale_info::{StaticTypeInfo, TypeInfo};
use sp_runtime::{
	traits::{
		DispatchInfoOf, OriginOf, PostDispatchInfoOf, TransactionExtension,
		TransactionExtensionBase, ValidateResult,
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
pub struct SkipCheckIfFeeless<T, S>(pub S, sp_std::marker::PhantomData<T>);

// Make this extension "invisible" from the outside (ie metadata type information)
impl<T, S: StaticTypeInfo> TypeInfo for SkipCheckIfFeeless<T, S> {
	type Identity = S;
	fn type_info() -> scale_info::Type {
		S::type_info()
	}
}

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

pub enum Intermediate<T, O> {
	/// The wrapped extension should be applied.
	Apply(T),
	/// The wrapped extension should be skipped.
	Skip(O),
}
use Intermediate::*;

impl<T: Config + Send + Sync, S: TransactionExtensionBase> TransactionExtensionBase
	for SkipCheckIfFeeless<T, S>
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

	fn weight(&self) -> frame_support::weights::Weight {
		self.0.weight()
	}
}

impl<T: Config + Send + Sync, Context, S: TransactionExtension<T::RuntimeCall, Context>>
	TransactionExtension<T::RuntimeCall, Context> for SkipCheckIfFeeless<T, S>
where
	T::RuntimeCall: CheckIfFeeless<Origin = frame_system::pallet_prelude::OriginFor<T>>,
{
	type Val = Intermediate<S::Val, <OriginOf<T::RuntimeCall> as OriginTrait>::PalletsOrigin>;
	type Pre = Intermediate<S::Pre, <OriginOf<T::RuntimeCall> as OriginTrait>::PalletsOrigin>;

	fn validate(
		&self,
		origin: OriginOf<T::RuntimeCall>,
		call: &T::RuntimeCall,
		info: &DispatchInfoOf<T::RuntimeCall>,
		len: usize,
		context: &mut Context,
		self_implicit: S::Implicit,
		inherited_implication: &impl Encode,
	) -> ValidateResult<Self::Val, T::RuntimeCall> {
		if call.is_feeless(&origin) {
			Ok((Default::default(), Skip(origin.caller().clone()), origin))
		} else {
			let (x, y, z) = self.0.validate(
				origin,
				call,
				info,
				len,
				context,
				self_implicit,
				inherited_implication,
			)?;
			Ok((x, Apply(y), z))
		}
	}

	fn prepare(
		self,
		val: Self::Val,
		origin: &OriginOf<T::RuntimeCall>,
		call: &T::RuntimeCall,
		info: &DispatchInfoOf<T::RuntimeCall>,
		len: usize,
		context: &Context,
	) -> Result<Self::Pre, TransactionValidityError> {
		match val {
			Apply(val) => self.0.prepare(val, origin, call, info, len, context).map(Apply),
			Skip(origin) => Ok(Skip(origin)),
		}
	}

	fn post_dispatch(
		pre: Self::Pre,
		info: &DispatchInfoOf<T::RuntimeCall>,
		post_info: &PostDispatchInfoOf<T::RuntimeCall>,
		len: usize,
		result: &DispatchResult,
		context: &Context,
	) -> Result<(), TransactionValidityError> {
		match pre {
			Apply(pre) => S::post_dispatch(pre, info, post_info, len, result, context),
			Skip(origin) => {
				Pallet::<T>::deposit_event(Event::<T>::FeeSkipped { origin });
				Ok(())
			},
		}
	}
}
