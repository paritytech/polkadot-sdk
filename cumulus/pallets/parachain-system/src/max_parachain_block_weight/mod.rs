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

//! Provides functionality to dynamically calculate the max block weight for a parachain.
//!
//! With block bundling, parachains are relative free to choose whatever block interval they want.
//! The block interval is the time between individual blocks. The available resources per block (max
//! block weight) depend on the number of cores allocated to the parachain on the relay chain. Each
//! relay chain cores provides an execution time of `2s` and a storage size of `10MiB`. Depending on
//! the desired number of blocks to produce, the resources need to be divided for
//!
//!
//! This means they will run under normal conditions with blocks that have a small block weight.
//! These small blocks may prevent certain transactions to be applied, e.g. a runtime upgrade. But
//! it is not only about transactions, also certain block logic may requires more weight from time
//! to time. To serve these needs [`MaxParachainBlockWeight`], [`DynamicMaxBlockWeight`] and
//! [`DynamicMaxBlockWeightHooks`] exist.
//!
//! - [`MaxParachainBlockWeight`]:

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
	PotentialFullCore {
		/// The index of the first transaction.
		first_transaction_index: Option<u32>,
		/// The target weight that was used to determine that the extrinsic is above this limit.
		target_weight: Weight,
	},
	/// The block is only allowed to consume its fraction of the core.
	///
	/// How much each block is allowed to consume, depends on the target number of blocks and the
	/// available cores on the relay chain.
	FractionOfCore {
		/// The index of the first transaction.
		first_transaction_index: Option<u32>,
	},
}

/// Calculates the maximum block weight for a parachain.
///
/// Based on the available cores and the number of desired blocks a block weight is calculated.
///
/// The max block weight is partly dynamic and controlled via the [`DynamicMaxBlockWeight`]
/// transaction extension. The transaction extension is communicating the desired max block weight
/// using the [`BlockWeightMode`].
pub struct MaxParachainBlockWeight<T>(PhantomData<T>);

impl<T: Config> MaxParachainBlockWeight<T> {
	// Maximum ref time per core
	const MAX_REF_TIME_PER_CORE_NS: u64 = 2 * WEIGHT_REF_TIME_PER_SECOND;
	const FULL_CORE_WEIGHT: Weight =
		Weight::from_parts(Self::MAX_REF_TIME_PER_CORE_NS, MAX_POV_SIZE as u64);

	/// Calculate the maximum block weight based on target blocks and available cores.
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
