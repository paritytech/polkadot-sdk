// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Cumulus.

// Cumulus is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Cumulus is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Cumulus.  If not, see <http://www.gnu.org/licenses/>.

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
use scale_info::TypeInfo;
use cumulus_primitives_storage_weight_reclaim::get_proof_size;
use derivative::Derivative;
use frame_support::{
	dispatch::{DispatchInfo, PostDispatchInfo},
	pallet_prelude::Weight,
};
use sp_runtime::{
	traits::{
		DispatchInfoOf, Dispatchable, PostDispatchInfoOf, TransactionExtension,
	},
	transaction_validity::{TransactionValidityError, ValidTransaction},
	DispatchResult,
};
#[cfg(not(feature = "std"))]
use alloc::vec::Vec;

#[cfg(feature = "runtime-benchmarks")]
pub mod benchmarks;
#[cfg(test)]
mod tests;
mod weights;

pub use pallet::*;
pub use weights::WeightInfo;

const LOG_TARGET: &'static str = "runtime::storage_reclaim_pallet";

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

/// Storage weight reclaim mechanism.
///
/// This extension must wrap all the transaction extensions:
#[doc = docify::embed!("./src/tests.rs", Tx)]
///
/// This extension checks the size of the node-side storage proof
/// before and after executing a given extrinsic. The difference between
/// benchmarked and spent weight can be reclaimed.
#[derive(Encode, Decode, TypeInfo, Derivative)]
#[derivative(
	Clone(bound = "S: Clone"),
	Eq(bound = "S: Eq"),
	PartialEq(bound = "S: PartialEq"),
	Default(bound = "S: Default")
)]
#[scale_info(skip_type_params(T))]
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

		#[cfg(not(feature = "std"))]
		let _ = f;

		Ok(())
	}
}

impl<T: Config + Send + Sync, S: TransactionExtension<T::RuntimeCall>>
	TransactionExtension<T::RuntimeCall> for StorageWeightReclaim<T, S>
where
	T::RuntimeCall: Dispatchable<Info = DispatchInfo, PostInfo = PostDispatchInfo>,
{
	const IDENTIFIER: &'static str = "StorageWeightReclaim<Use `metadata()`!>";
	type Implicit = S::Implicit;
	type Val = (Option<u64>, S::Val);
	type Pre = (Option<u64>, S::Pre);

	fn implicit(&self) -> Result<Self::Implicit, TransactionValidityError> {
		self.0.implicit()
	}

	fn metadata() -> Vec<sp_runtime::traits::TransactionExtensionMetadata> {
		S::metadata()
	}

	fn weight(&self, call: &T::RuntimeCall) -> Weight {
		T::WeightInfo::storage_weight_reclaim().saturating_add(self.0.weight(call))
	}

	fn validate(
		&self,
		origin: T::RuntimeOrigin,
		call: &T::RuntimeCall,
		info: &DispatchInfoOf<T::RuntimeCall>,
		len: usize,
		self_implicit: Self::Implicit,
		inherited_implication: &impl Encode,
	) -> Result<(ValidTransaction, Self::Val, T::RuntimeOrigin), TransactionValidityError> {
		// Trade-off: we could move it to `prepare` but better be accurate on reclaim than fast on
		// `validate`
		let proof_size = get_proof_size();

		self.0
			.validate(origin, call, info, len, self_implicit, inherited_implication)
			.map(|(validity, val, origin)| (validity, (proof_size, val), origin))
	}

	fn prepare(
		self,
		val: Self::Val,
		origin: &T::RuntimeOrigin,
		call: &T::RuntimeCall,
		info: &DispatchInfoOf<T::RuntimeCall>,
		len: usize,
	) -> Result<Self::Pre, TransactionValidityError> {
		let (proof_size, inner_val) = val;
		self.0.prepare(inner_val, origin, call, info, len).map(|pre| (proof_size, pre))
	}

	fn post_dispatch_details(
		pre: Self::Pre,
		info: &DispatchInfoOf<T::RuntimeCall>,
		post_info: &PostDispatchInfoOf<T::RuntimeCall>,
		len: usize,
		result: &DispatchResult,
	) -> Result<Weight, TransactionValidityError> {
		let (pre_dispatch_proof_size, inner_pre) = pre;

		let mut post_info_with_inner = *post_info;
		S::post_dispatch(inner_pre, info, &mut post_info_with_inner, len, result)?;

		let inner_refund = if let (Some(before_weight), Some(after_weight)) = (post_info.actual_weight, post_info_with_inner.actual_weight) {
			before_weight.saturating_sub(after_weight)
		} else {
			Weight::zero()
		};

		let Some(pre_dispatch_proof_size) = pre_dispatch_proof_size else {
			// We have no proof size information, there is nothing we can do.
			return Ok(inner_refund);
		};

		let Some(post_dispatch_proof_size) = get_proof_size() else {
			log::debug!(
				target: LOG_TARGET,
				"Proof recording enabled during prepare, now disabled. This should not happen."
			);
			return Ok(inner_refund)
		};

		// Unspent weight according to the `actual_weight` from `PostDispatchInfo`
		// This unspent weight will be refunded by the `CheckWeight` extension, so we need to
		// account for that.
		//
		// Also we don't need to include the unspent weight of this transaction extension because:
		// 1 - There is no unspent weight for this extension.
		// 2 - `CheckWeight` computes the unspent weight wihout seeing the unspent weight of this
		// extension even if there was any. Thus `post_info_with_inner` is what we want.
		let unspent = post_info_with_inner.calc_unspent(info).proof_size();
		let benchmarked_weight = info.total_weight().proof_size().saturating_sub(unspent);
		let consumed_weight = post_dispatch_proof_size.saturating_sub(pre_dispatch_proof_size);

		let storage_size_diff = benchmarked_weight.abs_diff(consumed_weight as u64);

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

		Ok(inner_refund)
	}
}
