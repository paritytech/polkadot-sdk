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

//! Mechanism to reclaim PoV proof size weight after an extrinsic has been applied.

#![cfg_attr(not(feature = "std"), no_std)]

use codec::{Decode, Encode};
use cumulus_primitives_core::Weight;
use cumulus_primitives_proof_size_hostfunction::{
	storage_proof_size::storage_proof_size, PROOF_RECORDING_DISABLED,
};
use frame_support::{
	dispatch::{DispatchInfo, PostDispatchInfo},
	weights::WeightMeter,
};
use frame_system::Config;
use scale_info::TypeInfo;
use sp_runtime::{
	impl_tx_ext_default,
	traits::{
		DispatchInfoOf, Dispatchable, PostDispatchInfoOf, TransactionExtension,
		TransactionExtensionBase,
	},
	transaction_validity::TransactionValidityError,
	DispatchResult,
};
use sp_std::marker::PhantomData;

#[cfg(test)]
mod tests;

#[cfg(feature = "runtime-benchmarks")]
mod benchmarking;

const LOG_TARGET: &'static str = "runtime::storage_reclaim";

/// `StorageWeightReclaimer` is a mechanism for manually reclaiming storage weight.
///
/// It internally keeps track of the proof size and storage weight at initialization time. At
/// reclaim  it computes the real consumed storage weight and refunds excess weight.
///
/// # Example
#[doc = docify::embed!("src/tests.rs", simple_reclaimer_example)]
pub struct StorageWeightReclaimer {
	previous_remaining_proof_size: u64,
	previous_reported_proof_size: Option<u64>,
}

impl StorageWeightReclaimer {
	/// Creates a new `StorageWeightReclaimer` instance and initializes it with the storage
	/// size provided by `weight_meter` and reported proof size from the node.
	#[must_use = "Must call `reclaim_with_meter` to reclaim the weight"]
	pub fn new(weight_meter: &WeightMeter) -> StorageWeightReclaimer {
		let previous_remaining_proof_size = weight_meter.remaining().proof_size();
		let previous_reported_proof_size = get_proof_size();
		Self { previous_remaining_proof_size, previous_reported_proof_size }
	}

	/// Check the consumed storage weight and calculate the consumed excess weight.
	fn reclaim(&mut self, remaining_weight: Weight) -> Option<Weight> {
		let current_remaining_weight = remaining_weight.proof_size();
		let current_storage_proof_size = get_proof_size()?;
		let previous_storage_proof_size = self.previous_reported_proof_size?;
		let used_weight =
			self.previous_remaining_proof_size.saturating_sub(current_remaining_weight);
		let reported_used_size =
			current_storage_proof_size.saturating_sub(previous_storage_proof_size);
		let reclaimable = used_weight.saturating_sub(reported_used_size);
		log::trace!(
			target: LOG_TARGET,
			"Found reclaimable storage weight. benchmarked: {used_weight}, consumed: {reported_used_size}"
		);

		self.previous_remaining_proof_size = current_remaining_weight.saturating_add(reclaimable);
		self.previous_reported_proof_size = Some(current_storage_proof_size);
		Some(Weight::from_parts(0, reclaimable))
	}

	/// Check the consumed storage weight and add the reclaimed
	/// weight budget back to `weight_meter`.
	pub fn reclaim_with_meter(&mut self, weight_meter: &mut WeightMeter) -> Option<Weight> {
		let reclaimed = self.reclaim(weight_meter.remaining())?;
		weight_meter.reclaim_proof_size(reclaimed.proof_size());
		Some(reclaimed)
	}
}

/// Returns the current storage proof size from the host side.
///
/// Returns `None` if proof recording is disabled on the host.
pub fn get_proof_size() -> Option<u64> {
	let proof_size = storage_proof_size();
	(proof_size != PROOF_RECORDING_DISABLED).then_some(proof_size)
}

/// Storage weight reclaim mechanism.
///
/// This extension checks the size of the node-side storage proof
/// before and after executing a given extrinsic. The difference between
/// benchmarked and spent weight can be reclaimed.
#[derive(Encode, Decode, Clone, Eq, PartialEq, Default, TypeInfo)]
#[scale_info(skip_type_params(T))]
pub struct StorageWeightReclaim<T: Config + Send + Sync>(PhantomData<T>);

impl<T: Config + Send + Sync> StorageWeightReclaim<T> {
	/// Create a new `StorageWeightReclaim` instance.
	pub fn new() -> Self {
		Self(Default::default())
	}
}

impl<T: Config + Send + Sync> core::fmt::Debug for StorageWeightReclaim<T> {
	fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> Result<(), core::fmt::Error> {
		let _ = write!(f, "StorageWeightReclaim");
		Ok(())
	}
}

impl<T: Config + Send + Sync> TransactionExtensionBase for StorageWeightReclaim<T> {
	const IDENTIFIER: &'static str = "StorageWeightReclaim";
	type Implicit = ();
}

impl<T: Config + Send + Sync, Context> TransactionExtension<T::RuntimeCall, Context>
	for StorageWeightReclaim<T>
where
	T::RuntimeCall: Dispatchable<Info = DispatchInfo, PostInfo = PostDispatchInfo>,
{
	type Val = ();
	type Pre = Option<u64>;

	fn prepare(
		self,
		_val: Self::Val,
		_origin: &T::RuntimeOrigin,
		_call: &T::RuntimeCall,
		_info: &DispatchInfoOf<T::RuntimeCall>,
		_len: usize,
		_context: &Context,
	) -> Result<Self::Pre, TransactionValidityError> {
		Ok(get_proof_size())
	}

	fn post_dispatch(
		pre: Self::Pre,
		info: &DispatchInfoOf<T::RuntimeCall>,
		post_info: &PostDispatchInfoOf<T::RuntimeCall>,
		_len: usize,
		_result: &DispatchResult,
		_context: &Context,
	) -> Result<(), TransactionValidityError> {
		let Some(pre_dispatch_proof_size) = pre else {
			return Ok(());
		};

		let Some(post_dispatch_proof_size) = get_proof_size() else {
			log::debug!(
				target: LOG_TARGET,
				"Proof recording enabled during pre-dispatch, now disabled. This should not happen."
			);
			return Ok(())
		};
		let benchmarked_weight = info.weight.proof_size();
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
					"Benchmarked storage weight smaller than consumed storage weight. benchmarked: {benchmarked_weight} consumed: {consumed_weight} unspent: {unspent}"
				);
				current.accrue(Weight::from_parts(0, storage_size_diff), info.class)
			} else {
				log::trace!(
					target: LOG_TARGET,
					"Reclaiming storage weight. benchmarked: {benchmarked_weight}, consumed: {consumed_weight} unspent: {unspent}"
				);
				current.reduce(Weight::from_parts(0, storage_size_diff), info.class)
			}
		});
		Ok(())
	}

	impl_tx_ext_default!(T::RuntimeCall; Context; validate);
}
