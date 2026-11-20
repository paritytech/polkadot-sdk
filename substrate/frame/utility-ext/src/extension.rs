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

//! The pallet's transaction extension.

use crate::{Config, Pallet, RefundableShare};
use codec::{Decode, DecodeWithMemTracking, Encode};
use core::marker::PhantomData;
use frame_support::{
	dispatch::{DispatchInfo, PostDispatchInfo},
	pallet_prelude::TransactionSource,
};
use scale_info::{StaticTypeInfo, TypeInfo};
use sp_runtime::{
	traits::{Implication, TransactionExtension, TransactionExtensionMetadata, ValidateResult},
	transaction_validity::{TransactionValidity, TransactionValidityError},
	DispatchResult, Weight,
};

type RuntimeCallOf<T> = <T as frame_system::Config>::RuntimeCall;
type RuntimeOriginOf<T> = <T as frame_system::Config>::RuntimeOrigin;

/// The pallet's transaction extension, wrapping the runtime's fee payment extension `S` in its
/// slot of the pipeline. For a multi-origin batch, `S` settles the fee on a `post_info` and a
/// length reduced by the [`RefundableShare`]. The real `post_info` is not changed, so the rest of
/// the pipeline accounts for that weight. Other transactions go through to `S` unchanged.
#[derive(Encode, Decode, DecodeWithMemTracking, Clone, Eq, PartialEq)]
pub struct MultiOriginBatch<T, S>(pub S, PhantomData<T>);

// Type info, metadata, identifier and implicit are `S`'s, so the wrapper is invisible from the
// outside.
impl<T, S: StaticTypeInfo> TypeInfo for MultiOriginBatch<T, S> {
	type Identity = S;
	fn type_info() -> scale_info::Type {
		S::type_info()
	}
}

impl<T, S: core::fmt::Debug> core::fmt::Debug for MultiOriginBatch<T, S> {
	fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
		self.0.fmt(f)
	}
}

impl<T, S> From<S> for MultiOriginBatch<T, S> {
	fn from(s: S) -> Self {
		Self(s, PhantomData)
	}
}

impl<T: Config + Send + Sync, S: TransactionExtension<RuntimeCallOf<T>>>
	TransactionExtension<RuntimeCallOf<T>> for MultiOriginBatch<T, S>
{
	const IDENTIFIER: &'static str = S::IDENTIFIER;
	type Implicit = S::Implicit;
	type Val = S::Val;
	/// The wrapped extension's and whether the transaction is a batch.
	type Pre = (S::Pre, bool);

	fn metadata() -> alloc::vec::Vec<TransactionExtensionMetadata> {
		S::metadata()
	}

	fn implicit(&self) -> Result<Self::Implicit, TransactionValidityError> {
		self.0.implicit()
	}

	fn weight(&self, call: &RuntimeCallOf<T>) -> Weight {
		self.0.weight(call)
	}

	fn validate(
		&self,
		origin: RuntimeOriginOf<T>,
		call: &RuntimeCallOf<T>,
		info: &DispatchInfo,
		len: usize,
		self_implicit: Self::Implicit,
		inherited_implication: &impl Implication,
		source: TransactionSource,
	) -> ValidateResult<Self::Val, RuntimeCallOf<T>> {
		self.0
			.validate(origin, call, info, len, self_implicit, inherited_implication, source)
	}

	fn prepare(
		self,
		val: Self::Val,
		origin: &RuntimeOriginOf<T>,
		call: &RuntimeCallOf<T>,
		info: &DispatchInfo,
		len: usize,
	) -> Result<Self::Pre, TransactionValidityError> {
		let batch = Pallet::<T>::is_batch(call);
		if batch {
			RefundableShare::<T>::kill();
		}
		Ok((self.0.prepare(val, origin, call, info, len)?, batch))
	}

	fn post_dispatch_details(
		(pre, batch): Self::Pre,
		info: &DispatchInfo,
		post_info: &PostDispatchInfo,
		len: usize,
		result: &DispatchResult,
	) -> Result<Weight, TransactionValidityError> {
		if !batch {
			return S::post_dispatch_details(pre, info, post_info, len, result);
		}
		let share = RefundableShare::<T>::take().unwrap_or_default();
		let charged = PostDispatchInfo {
			actual_weight: Some(post_info.calc_actual_weight(info).saturating_sub(share.weight)),
			pays_fee: post_info.pays_fee,
		};
		let len = len.saturating_sub(share.len as usize);
		S::post_dispatch_details(pre, info, &charged, len, result)
	}

	fn bare_validate(
		call: &RuntimeCallOf<T>,
		info: &DispatchInfo,
		len: usize,
	) -> TransactionValidity {
		S::bare_validate(call, info, len)
	}

	fn bare_validate_and_prepare(
		call: &RuntimeCallOf<T>,
		info: &DispatchInfo,
		len: usize,
	) -> Result<(), TransactionValidityError> {
		S::bare_validate_and_prepare(call, info, len)
	}

	fn bare_post_dispatch(
		info: &DispatchInfo,
		post_info: &mut PostDispatchInfo,
		len: usize,
		result: &DispatchResult,
	) -> Result<(), TransactionValidityError> {
		S::bare_post_dispatch(info, post_info, len, result)
	}
}
