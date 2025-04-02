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

use codec::{Decode, DecodeWithMemTracking, Encode};
use frame_support::{
	dispatch::DispatchResult,
	pallet_prelude::TransactionSource,
	traits::{ContainsPair, OriginTrait},
	weights::Weight,
};
use scale_info::{StaticTypeInfo, TypeInfo};
use sp_runtime::{
	traits::{
		DispatchInfoOf, DispatchOriginOf, Implication, PostDispatchInfoOf, TransactionExtension,
		ValidateResult,
	},
	transaction_validity::TransactionValidityError,
};

/// A [`TransactionExtension`] that skips the wrapped extension `E` if the `Filter` returns `true`.
#[derive(Encode, Decode, DecodeWithMemTracking, Clone, Eq, PartialEq)]
pub struct SkipCheckIf<T, E, Filter>(pub E, core::marker::PhantomData<(T, Filter)>);

// Make this extension "invisible" from the outside (ie metadata type information)
impl<T, E: StaticTypeInfo, Filter> TypeInfo for SkipCheckIf<T, E, Filter> {
	type Identity = E;
	fn type_info() -> scale_info::Type {
		E::type_info()
	}
}

impl<T, E: Encode, Filter> core::fmt::Debug for SkipCheckIf<T, E, Filter> {
	#[cfg(feature = "std")]
	fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
		write!(f, "SkipCheckIf<{:?}>", self.0.encode())
	}
	#[cfg(not(feature = "std"))]
	fn fmt(&self, _: &mut core::fmt::Formatter) -> core::fmt::Result {
		Ok(())
	}
}

impl<T, E, Filter> From<E> for SkipCheckIf<T, E, Filter> {
	fn from(e: E) -> Self {
		Self(e, core::marker::PhantomData)
	}
}

pub enum Intermediate<T, O> {
	/// The wrapped extension should be applied.
	Apply(T),
	/// The wrapped extension should be skipped.
	Skip(O),
}
use Intermediate::*;

impl<
		T: crate::Config + Send + Sync,
		E: TransactionExtension<T::RuntimeCall>,
		Filter: ContainsPair<T::RuntimeCall, DispatchOriginOf<T::RuntimeCall>>
			+ 'static
			+ Send
			+ Sync
			+ Clone
			+ Eq,
	> TransactionExtension<T::RuntimeCall> for SkipCheckIf<T, E, Filter>
{
	const IDENTIFIER: &'static str = E::IDENTIFIER;
	type Implicit = E::Implicit;

	fn implicit(&self) -> Result<Self::Implicit, TransactionValidityError> {
		self.0.implicit()
	}

	fn metadata() -> alloc::vec::Vec<sp_runtime::traits::TransactionExtensionMetadata> {
		E::metadata()
	}
	type Val =
		Intermediate<E::Val, <DispatchOriginOf<T::RuntimeCall> as OriginTrait>::PalletsOrigin>;
	type Pre =
		Intermediate<E::Pre, <DispatchOriginOf<T::RuntimeCall> as OriginTrait>::PalletsOrigin>;

	fn weight(&self, call: &T::RuntimeCall) -> Weight {
		self.0.weight(call)
	}

	fn validate(
		&self,
		origin: DispatchOriginOf<T::RuntimeCall>,
		call: &T::RuntimeCall,
		info: &DispatchInfoOf<T::RuntimeCall>,
		len: usize,
		self_implicit: E::Implicit,
		inherited_implication: &impl Implication,
		source: TransactionSource,
	) -> ValidateResult<Self::Val, T::RuntimeCall> {
		if Filter::contains(call, &origin) {
			Ok((Default::default(), Skip(origin.caller().clone()), origin))
		} else {
			let (x, y, z) = self.0.validate(
				origin,
				call,
				info,
				len,
				self_implicit,
				inherited_implication,
				source,
			)?;
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
			Apply(pre) => E::post_dispatch_details(pre, info, post_info, len, result),
			Skip(_) => {
				// TODO: FAIL-CI - maybe return some weight consumed by `Filter::contains(call, &origin)`?
				Ok(Weight::zero())
			},
		}
	}
}
