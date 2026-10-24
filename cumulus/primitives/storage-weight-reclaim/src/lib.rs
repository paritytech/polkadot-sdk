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

//! Utilities to reclaim PoV proof size weight.
//!
//! Provides [`StorageWeightReclaimer`] to manually reclaim storage weight and [`get_proof_size`]
//! to read the current storage proof size from the host.
//!
//! To reclaim storage weight after an extrinsic has been applied, use the `StorageWeightReclaim`
//! transaction extension from the `cumulus-pallet-weight-reclaim` crate.

#![cfg_attr(not(feature = "std"), no_std)]

use cumulus_primitives_core::Weight;
use cumulus_primitives_proof_size_hostfunction::{
	storage_proof_size::storage_proof_size, PROOF_RECORDING_DISABLED,
};
use frame_support::weights::WeightMeter;

#[cfg(test)]
mod tests;

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
/// This function also triggers the computation of PoV size for storage root changes,
/// recording trie nodes that would be accessed during storage root calculation.
///
/// Returns `None` if proof recording is disabled on the host.
pub fn get_proof_size() -> Option<u64> {
	let proof_size = storage_proof_size();
	(proof_size != PROOF_RECORDING_DISABLED).then_some(proof_size)
}
