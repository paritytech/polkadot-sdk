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

//! Utilities for calculating maximum parachain block weight based on core assignments.

use crate::Config;
use codec::{Decode, Encode};
use core::marker::PhantomData;
use cumulus_primitives_core::CumulusDigestItem;
use frame_support::weights::{constants::WEIGHT_REF_TIME_PER_SECOND, Weight};
use polkadot_primitives::MAX_POV_SIZE;
use scale_info::TypeInfo;
use sp_core::Get;
use sp_runtime::Digest;

#[cfg(test)]
pub(crate) mod mock;
pub mod pre_inherents_hook;
#[cfg(test)]
mod tests;
pub mod transaction_extension;

pub use pre_inherents_hook::DynamicMaxBlockWeightHooks;
pub use transaction_extension::DynamicMaxBlockWeight;

const LOG_TARGET: &str = "runtime::parachain-system::block-weight";

/// The current block weight mode.
///
/// Based on this mode [`MaxParachainBlockWeight`] determines the current allowed block weight.
#[derive(Debug, Encode, Decode, Clone, Copy, TypeInfo)]
pub enum BlockWeightMode {
	/// The block is allowed to use the weight of a full core.
	FullCore,
	/// The current active transaction is allowed to use the weight of a full core.
	PotentialFullCore { first_transaction_index: Option<u32> },
	/// The block is only allowed to consume its fraction of the core.
	///
	/// How much each block is allowed to consume, depends on the target number of blocks and the
	/// available cores on the relay chain.
	FractionOfCore { first_transaction_index: Option<u32> },
}

/// A utility type for calculating the maximum block weight for a parachain based on
/// the number of relay chain cores assigned and the target number of blocks.
pub struct MaxParachainBlockWeight<T>(PhantomData<T>);

impl<T: Config> MaxParachainBlockWeight<T> {
	// Maximum ref time per core
	const MAX_REF_TIME_PER_CORE_NS: u64 = 2 * WEIGHT_REF_TIME_PER_SECOND;
	const FULL_CORE_WEIGHT: Weight =
		Weight::from_parts(Self::MAX_REF_TIME_PER_CORE_NS, MAX_POV_SIZE as u64);

	/// Calculate the maximum block weight based on target blocks and core assignments.
	///
	/// This function examines the current block's digest from `frame_system::Digests` storage
	/// to find `CumulusDigestItem::CoreInfo` entries, which contain information about the
	/// number of relay chain cores assigned to the parachain. Each core has a maximum
	/// reference time of 2 seconds and the total maximum PoV size of `MAX_POV_SIZE` is
	/// shared across all target blocks.
	///
	/// # Parameters
	/// - `target_blocks`: The target number of blocks to be produced
	///
	/// # Returns
	/// Returns the calculated maximum weight, or a conservative default if no core info is found
	/// or if an error occurs during calculation.
	pub fn get(target_blocks: u32) -> Weight {
		let digest = frame_system::Pallet::<T>::digest();
		let target_block_weight = Self::target_block_weight_with_digest(target_blocks, &digest);

		let maybe_full_core_weight = if is_first_block_in_core_with_digest(&digest) {
			Self::FULL_CORE_WEIGHT
		} else {
			target_block_weight
		};

		// If we are in `on_initialize` or at applying the inherents, we allow the maximum block
		// weight as allowed by the current context.
		if !frame_system::Pallet::<T>::inherents_applied() {
			return maybe_full_core_weight
		}

		match crate::BlockWeightMode::<T>::get() {
			// We allow the full core.
			Some(BlockWeightMode::FullCore | BlockWeightMode::PotentialFullCore { .. }) =>
				Self::FULL_CORE_WEIGHT,
			// Let's calculate below how much weight we can use.
			Some(BlockWeightMode::FractionOfCore { .. }) => target_block_weight,
			// Either the runtime is not using the `DynamicMaxBlockWeight` extension or there is a
			// bug. The value should be set before applying the first extrinsic.
			None => maybe_full_core_weight,
		}
	}

	/// Returns the target block weight for one block.
	fn target_block_weight(target_blocks: u32) -> Weight {
		let digest = frame_system::Pallet::<T>::digest();
		Self::target_block_weight_with_digest(target_blocks, &digest)
	}

	/// Same as [`Self::target_block_weight`], but takes the `digests` directly.
	fn target_block_weight_with_digest(target_blocks: u32, digest: &Digest) -> Weight {
		let Some(core_info) = CumulusDigestItem::find_core_info(&digest) else {
			return Self::FULL_CORE_WEIGHT;
		};

		let number_of_cores = core_info.number_of_cores.0 as u32;

		// Ensure we have at least one core and valid target blocks
		if number_of_cores == 0 || target_blocks == 0 {
			return Self::FULL_CORE_WEIGHT;
		}

		let total_ref_time =
			(number_of_cores as u64).saturating_mul(Self::MAX_REF_TIME_PER_CORE_NS);
		let ref_time_per_block = total_ref_time
			.saturating_div(target_blocks as u64)
			.min(Self::MAX_REF_TIME_PER_CORE_NS);

		let total_pov_size = (number_of_cores as u64).saturating_mul(MAX_POV_SIZE as u64);
		let proof_size_per_block = total_pov_size.saturating_div(target_blocks as u64);

		Weight::from_parts(ref_time_per_block, proof_size_per_block)
	}
}

/// Is this the first block in a core?
fn is_first_block_in_core<T: Config>() -> bool {
	let digest = frame_system::Pallet::<T>::digest();
	is_first_block_in_core_with_digest(&digest)
}

/// Is this the first block in a core? (takes digest as parameter)
fn is_first_block_in_core_with_digest(digest: &Digest) -> bool {
	CumulusDigestItem::find_bundle_info(digest).map_or(false, |bi| bi.index == 0)
}

/// Is the `BlockWeight` already above the target block weight?
fn block_weight_over_target_block_weight<T: Config, TargetBlockRate: Get<u32>>() -> bool {
	let target_block_weight =
		MaxParachainBlockWeight::<T>::target_block_weight(TargetBlockRate::get());

	frame_system::Pallet::<T>::remaining_block_weight()
		.consumed()
		.any_gt(target_block_weight)
}
