// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Cumulus.
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
//! This crate provides:
//! * [`StorageWeightReclaim`] transaction extension: it must wrap the whole transaction extension
//!   pipeline.
//! * The pallet required for the transaction extensions weight information and benchmarks.

#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;
#[cfg(not(feature = "std"))]
use alloc::vec::Vec;
use codec::{Decode, DecodeWithMemTracking, Encode};
use cumulus_primitives_storage_weight_reclaim::get_proof_size;
use derive_where::derive_where;
use frame_support::{
	dispatch::{DispatchInfo, PostDispatchInfo},
	pallet_prelude::Weight,
	traits::Defensive,
};
use scale_info::TypeInfo;
use sp_runtime::{
	traits::{DispatchInfoOf, Dispatchable, Implication, PostDispatchInfoOf, TransactionExtension},
	transaction_validity::{TransactionSource, TransactionValidityError, ValidTransaction},
	DispatchResult,
};

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
/// This extension checks the size of the node-side storage proof before and after executing a given
/// extrinsic using the proof size host function. The difference between benchmarked and used weight
/// is reclaimed.
///
/// If the benchmark was underestimating the proof size, then it is added to the block weight.
///
/// For the time part of the weight, it does same as system `WeightReclaim` extension, it
/// calculates the unused weight using the post information and reclaim the unused weight.
/// So this extension can be used as a drop-in replacement for `WeightReclaim` extension for
/// parachains.
#[derive(Encode, Decode, DecodeWithMemTracking, TypeInfo)]
#[derive_where(Clone, Eq, PartialEq, Default; S)]
#[scale_info(skip_type_params(T))]
pub struct StorageWeightReclaim<T, S>(pub S, core::marker::PhantomData<T>);

impl<T, S> StorageWeightReclaim<T, S> {
	/// Create a new `StorageWeightReclaim` instance.
	pub fn new(s: S) -> Self {
		Self(s, Default::default())
	}
}

impl<T, S> From<S> for StorageWeightReclaim<T, S> {
	fn from(s: S) -> Self {
		Self::new(s)
	}
}

impl<T, S: core::fmt::Debug> core::fmt::Debug for StorageWeightReclaim<T, S> {
	fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> Result<(), core::fmt::Error> {
		#[cfg(feature = "std")]
		let _ = write!(f, "StorageWeightReclaim<{:?}>", self.0);

		#[cfg(not(feature = "std"))]
		let _ = write!(f, "StorageWeightReclaim<wasm-stripped>");

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

	// Initial proof size and inner extension value.
	type Val = (Option<u64>, S::Val);

	// Initial proof size and inner extension pre.
	type Pre = (Option<u64>, S::Pre);

	fn implicit(&self) -> Result<Self::Implicit, TransactionValidityError> {
		self.0.implicit()
	}

	fn metadata() -> Vec<sp_runtime::traits::TransactionExtensionMetadata> {
		let mut inner = S::metadata();
		inner.push(sp_runtime::traits::TransactionExtensionMetadata {
			identifier: "StorageWeightReclaim",
			ty: scale_info::meta_type::<()>(),
			implicit: scale_info::meta_type::<()>(),
		});
		inner
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
		inherited_implication: &impl Implication,
		source: TransactionSource,
	) -> Result<(ValidTransaction, Self::Val, T::RuntimeOrigin), TransactionValidityError> {
		let proof_size = get_proof_size();

		self.0
			.validate(origin, call, info, len, self_implicit, inherited_implication, source)
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
		let (proof_size_before_dispatch, inner_pre) = pre;

		let mut post_info_with_inner = *post_info;
		S::post_dispatch(inner_pre, info, &mut post_info_with_inner, len, result)?;

		let inner_refund = if let (Some(before_weight), Some(after_weight)) =
			(post_info.actual_weight, post_info_with_inner.actual_weight)
		{
			before_weight.saturating_sub(after_weight)
		} else {
			Weight::zero()
		};

		let Some(proof_size_before_dispatch) = proof_size_before_dispatch else {
			// We have no proof size information, there is nothing we can do.
			return Ok(inner_refund);
		};

		let Some(proof_size_after_dispatch) = get_proof_size().defensive_proof(
			"Proof recording enabled during prepare, now disabled. This should not happen.",
		) else {
			return Ok(inner_refund)
		};

		// The consumed proof size as measured by the host.
		let measured_proof_size =
			proof_size_after_dispatch.saturating_sub(proof_size_before_dispatch);

		// The consumed weight as benchmarked. Calculated from post info and info.
		// NOTE: `calc_actual_weight` will take the minimum of `post_info` and `info` weights.
		// This means any underestimation of compute time in the pre dispatch info will not be
		// taken into account.
		let benchmarked_actual_weight = post_info_with_inner.calc_actual_weight(info);

		let benchmarked_actual_proof_size = benchmarked_actual_weight.proof_size();
		if benchmarked_actual_proof_size < measured_proof_size {
			log::error!(
				target: LOG_TARGET,
				"Benchmarked storage weight smaller than consumed storage weight. \
				benchmarked: {benchmarked_actual_proof_size} consumed: {measured_proof_size}"
			);
		} else {
			log::trace!(
				target: LOG_TARGET,
				"Reclaiming storage weight. benchmarked: {benchmarked_actual_proof_size},
				consumed: {measured_proof_size}"
			);
		}

		let accurate_weight = benchmarked_actual_weight.set_proof_size(measured_proof_size);

		let pov_size_missing_from_node = frame_system::BlockWeight::<T>::mutate(|current_weight| {
			let already_reclaimed = frame_system::ExtrinsicWeightReclaimed::<T>::get();
			current_weight.accrue(already_reclaimed, info.class);
			current_weight.reduce(info.total_weight(), info.class);
			current_weight.accrue(accurate_weight, info.class);

			// If we encounter a situation where the node-side proof size is already higher than
			// what we have in the runtime bookkeeping, we add the difference to the `BlockWeight`.
			// This prevents that the proof size grows faster than the runtime proof size.
			let extrinsic_len = frame_system::AllExtrinsicsLen::<T>::get().unwrap_or(0);
			let node_side_pov_size = proof_size_after_dispatch.saturating_add(extrinsic_len.into());
			let block_weight_proof_size = current_weight.total().proof_size();
			let pov_size_missing_from_node =
				node_side_pov_size.saturating_sub(block_weight_proof_size);
			if pov_size_missing_from_node > 0 {
				log::warn!(
					target: LOG_TARGET,
					"Node-side PoV size higher than runtime proof size weight. node-side: \
					{node_side_pov_size} extrinsic_len: {extrinsic_len} runtime: \
					{block_weight_proof_size}, missing: {pov_size_missing_from_node}. Setting to \
					node-side proof size."
				);
				current_weight
					.accrue(Weight::from_parts(0, pov_size_missing_from_node), info.class);
			}

			pov_size_missing_from_node
		});

		// The saturation will happen if the pre-dispatch weight is underestimating the proof
		// size or if the node-side proof size is higher than expected.
		// In this case the extrinsic proof size weight reclaimed is 0 and not a negative reclaim.
		let accurate_unspent = info
			.total_weight()
			.saturating_sub(accurate_weight)
			.saturating_sub(Weight::from_parts(0, pov_size_missing_from_node));
		frame_system::ExtrinsicWeightReclaimed::<T>::put(accurate_unspent);

		// Call have already returned their unspent amount.
		// (also transaction extension prior in the pipeline, but there shouldn't be any.)
		let already_unspent_in_tx_ext_pipeline = post_info.calc_unspent(info);
		Ok(accurate_unspent.saturating_sub(already_unspent_in_tx_ext_pipeline))
	}

	fn bare_validate(
		call: &T::RuntimeCall,
		info: &DispatchInfoOf<T::RuntimeCall>,
		len: usize,
	) -> frame_support::pallet_prelude::TransactionValidity {
		S::bare_validate(call, info, len)
	}

	fn bare_validate_and_prepare(
		call: &T::RuntimeCall,
		info: &DispatchInfoOf<T::RuntimeCall>,
		len: usize,
	) -> Result<(), TransactionValidityError> {
		S::bare_validate_and_prepare(call, info, len)
	}

	fn bare_post_dispatch(
		info: &DispatchInfoOf<T::RuntimeCall>,
		post_info: &mut PostDispatchInfoOf<T::RuntimeCall>,
		len: usize,
		result: &DispatchResult,
	) -> Result<(), TransactionValidityError> {
		S::bare_post_dispatch(info, post_info, len, result)?;

		frame_system::Pallet::<T>::reclaim_weight(info, post_info)
	}
}
