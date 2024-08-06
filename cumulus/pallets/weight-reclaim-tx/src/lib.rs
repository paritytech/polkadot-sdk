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

//! Pallet and transaction extensions to reclaim PoV proof size weight after an extrinsic has been
//! applied.
//!
//! This crate provides a transaction extensions and a pallet.
//! * [`StorageWeightReclaim`] transaction extension: it must wrap the whole transaction extension
//!   pipeline.
//! * The pallet required for the transaction extensions weight information and benchmarks.

#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;
use codec::{Decode, Encode};
use cumulus_primitives_storage_weight_reclaim::get_proof_size;
use derivative::Derivative;
use frame_support::{
	dispatch::{DispatchInfo, PostDispatchInfo},
	pallet_prelude::Weight,
};
use sp_runtime::{
	traits::{
		AccrueWeight, DispatchInfoOf, Dispatchable, PostDispatchInfoOf, TransactionExtension,
		TransactionExtensionBase,
	},
	transaction_validity::{TransactionValidityError, ValidTransaction},
	DispatchResult,
};

#[cfg(feature = "runtime-benchmarks")]
pub mod benchmarks;
#[cfg(test)]
mod tests;

const LOG_TARGET: &'static str = "runtime::storage_reclaim_pallet";

pub use pallet::*;

/// Pallet to use alongside the transaction extension [`StorageWeightReclaim`], the pallet provides
/// weight information and benchmarks.
#[frame_support::pallet]
pub mod pallet {
	use super::*;

	#[pallet::pallet]
	pub struct Pallet<T>(_);

	#[pallet::config]
	pub trait Config: frame_system::Config {
		type WeightInfo: WeightInfo;
	}
}

// TODO: generate from cli
pub trait WeightInfo {
	fn storage_weight_reclaim() -> Weight;
}

impl WeightInfo for () {
	fn storage_weight_reclaim() -> Weight {
		Weight::zero()
	}
}

/// Storage weight reclaim mechanism.
///
/// This extension must wrap all the transaction extensions:
#[doc = docify::embed!("./src/tests.rs", Tx)]
///
/// This extension checks the size of the node-side storage proof
/// before and after executing a given extrinsic. The difference between
/// benchmarked and spent weight can be reclaimed.
#[derive(Encode, Decode, Derivative)]
#[derivative(
	Clone(bound = "S: Clone"),
	Eq(bound = "S: Eq"),
	PartialEq(bound = "S: PartialEq"),
	Default(bound = "S: Default")
)]
pub struct StorageWeightReclaim<T, S>(pub S, core::marker::PhantomData<T>);

impl<T, S> StorageWeightReclaim<T, S> {
	/// Create a new `StorageWeightReclaim` instance.
	pub fn new(s: S) -> Self {
		Self(s, Default::default())
	}
}

impl<T, S: core::fmt::Debug> core::fmt::Debug for StorageWeightReclaim<T, S> {
	fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> Result<(), core::fmt::Error> {
		#[cfg(feature = "std")]
		let _ = write!(f, "StorageWeightReclaim<{:?}>", self.0);

		Ok(())
	}
}

// Make this extension "invisible" from the outside (ie metadata type information)
impl<T, S: scale_info::StaticTypeInfo> scale_info::TypeInfo for StorageWeightReclaim<T, S> {
	type Identity = S;
	fn type_info() -> scale_info::Type {
		S::type_info()
	}
}

impl<T: Config + Send + Sync, S: TransactionExtensionBase> TransactionExtensionBase
	for StorageWeightReclaim<T, S>
{
	const IDENTIFIER: &'static str = S::IDENTIFIER;
	type Implicit = S::Implicit;

	fn weight() -> Weight {
		T::WeightInfo::storage_weight_reclaim().saturating_add(S::weight())
	}
}

impl<T: Config + Send + Sync, S: TransactionExtension<T::RuntimeCall, Context>, Context>
	TransactionExtension<T::RuntimeCall, Context> for StorageWeightReclaim<T, S>
where
	T::RuntimeCall: Dispatchable<Info = DispatchInfo, PostInfo = PostDispatchInfo>,
{
	type Val = (Option<u64>, S::Val);
	type Pre = (Option<u64>, S::Pre);

	fn validate(
		&self,
		origin: T::RuntimeOrigin,
		call: &T::RuntimeCall,
		info: &DispatchInfoOf<T::RuntimeCall>,
		len: usize,
		context: &mut Context,
		self_implicit: Self::Implicit,
		inherited_implication: &impl Encode,
	) -> Result<(ValidTransaction, Self::Val, T::RuntimeOrigin), TransactionValidityError> {
		// Trade-off: we could move it to `prepare` but better be accurate on reclaim than fast on
		// `validate`
		let proof_size = get_proof_size();

		self.0
			.validate(origin, call, info, len, context, self_implicit, inherited_implication)
			.map(|(validity, val, origin)| (validity, (proof_size, val), origin))
	}

	fn prepare(
		self,
		val: Self::Val,
		origin: &T::RuntimeOrigin,
		call: &T::RuntimeCall,
		info: &DispatchInfoOf<T::RuntimeCall>,
		len: usize,
		context: &Context,
	) -> Result<Self::Pre, TransactionValidityError> {
		let (proof_size, inner_val) = val;
		self.0
			.prepare(inner_val, origin, call, info, len, context)
			.map(|pre| (proof_size, pre))
	}

	fn post_dispatch_details(
		pre: Self::Pre,
		info: &DispatchInfoOf<T::RuntimeCall>,
		post_info: &PostDispatchInfoOf<T::RuntimeCall>,
		len: usize,
		result: &DispatchResult,
		context: &Context,
	) -> Result<Option<Weight>, TransactionValidityError> {
		log::error!(
			target: LOG_TARGET,
			"Calling the post dispatch details of an aggregating transaction extensions is \
			invalid. No information can sensibly be returned for `pays_fee`."
		);

		let mut post_info_copy = *post_info;

		Self::post_dispatch(pre, info, &mut post_info_copy, len, result, context)?;
		post_info_copy.accrue(T::WeightInfo::storage_weight_reclaim());

		Ok(post_info_copy.actual_weight)
	}

	fn post_dispatch(
		pre: Self::Pre,
		info: &DispatchInfoOf<T::RuntimeCall>,
		post_info: &mut PostDispatchInfoOf<T::RuntimeCall>,
		len: usize,
		result: &DispatchResult,
		context: &Context,
	) -> Result<(), TransactionValidityError> {
		let (pre_dispatch_proof_size, inner_pre) = pre;

		S::post_dispatch(inner_pre, info, post_info, len, result, context)?;
		post_info.accrue(T::WeightInfo::storage_weight_reclaim());

		let Some(pre_dispatch_proof_size) = pre_dispatch_proof_size else {
			// No information
			return Ok(());
		};

		let Some(post_dispatch_proof_size) = get_proof_size() else {
			log::debug!(
				target: LOG_TARGET,
				"Proof recording enabled during prepare, now disabled. This should not happen."
			);
			return Ok(())
		};

		let benchmarked_weight = info.total_weight().proof_size();
		let consumed_weight = post_dispatch_proof_size.saturating_sub(pre_dispatch_proof_size);

		// Unspent weight according to the `actual_weight` from `PostDispatchInfo`
		// This unspent weight will be refunded by the `CheckWeight` extension, so we need to
		// account for that.
		let unspent = post_info.calc_unspent(info).proof_size();
		let storage_size_diff =
			benchmarked_weight.saturating_sub(unspent).abs_diff(consumed_weight as u64);

		// This value will be reclaimed by [`frame_system::CheckWeight`], so we need to calculate
		// that in.
		frame_system::BlockWeight::<T>::mutate(|current| {
			if consumed_weight > benchmarked_weight {
				log::error!(
					target: LOG_TARGET,
					"Benchmarked storage weight smaller than consumed storage weight. \
					benchmarked: {benchmarked_weight} consumed: {consumed_weight} unspent: \
					{unspent}"
				);
				current.accrue(Weight::from_parts(0, storage_size_diff), info.class)
			} else {
				log::trace!(
					target: LOG_TARGET,
					"Reclaiming storage weight. benchmarked: {benchmarked_weight}, consumed: \
					{consumed_weight} unspent: {unspent}"
				);
				current.reduce(Weight::from_parts(0, storage_size_diff), info.class)
			}
		});

		Ok(())
	}
}
