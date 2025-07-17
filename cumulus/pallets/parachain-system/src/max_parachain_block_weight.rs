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

use cumulus_primitives_core::CumulusDigestItem;
use frame_support::weights::{constants::WEIGHT_REF_TIME_PER_SECOND, Weight};
use polkadot_primitives::MAX_POV_SIZE;

/// A utility type for calculating the maximum block weight for a parachain based on
/// the number of relay chain cores assigned and the target number of blocks.
pub struct MaxParachainBlockWeight;

impl MaxParachainBlockWeight {
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
	pub fn get<T: frame_system::Config>(target_blocks: u32) -> Weight {
		// Maximum ref time per core
		const MAX_REF_TIME_PER_CORE_NS: u64 = 2 * WEIGHT_REF_TIME_PER_SECOND;

		let digest = frame_system::Pallet::<T>::digest();

		let Some(core_info) = CumulusDigestItem::find_core_info(&digest) else {
			return Weight::from_parts(MAX_REF_TIME_PER_CORE_NS, MAX_POV_SIZE as u64);
		};

		let number_of_cores = core_info.number_of_cores.0 as u32;

		// Ensure we have at least one core and valid target blocks
		if number_of_cores == 0 || target_blocks == 0 {
			return Weight::from_parts(MAX_REF_TIME_PER_CORE_NS, MAX_POV_SIZE as u64);
		}

		let ref_time_per_block = MAX_REF_TIME_PER_CORE_NS.saturating_div(target_blocks as u64);

		let proof_size_per_block = (MAX_POV_SIZE as u64).saturating_div(target_blocks as u64);

		Weight::from_parts(ref_time_per_block, proof_size_per_block)
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use codec::Compact;
	use cumulus_primitives_core::{ClaimQueueOffset, CoreInfo, CoreSelector};
	use frame_support::{construct_runtime, derive_impl};
	use sp_io;
	use sp_runtime::{traits::IdentityLookup, BuildStorage};

	type Block = frame_system::mocking::MockBlock<Test>;

	// Configure a mock runtime to test the functionality
	#[derive_impl(frame_system::config_preludes::TestDefaultConfig)]
	impl frame_system::Config for Test {
		type Block = Block;
		type AccountId = u64;
		type AccountData = ();
		type Lookup = IdentityLookup<Self::AccountId>;
	}

	construct_runtime!(
		pub enum Test {
			System: frame_system,
		}
	);

	fn new_test_ext_with_digest(num_cores: Option<u16>) -> sp_io::TestExternalities {
		let storage = frame_system::GenesisConfig::<Test>::default().build_storage().unwrap();

		let mut ext = sp_io::TestExternalities::from(storage);

		ext.execute_with(|| {
			if let Some(num_cores) = num_cores {
				let core_info = CoreInfo {
					selector: CoreSelector(0),
					claim_queue_offset: ClaimQueueOffset(0),
					number_of_cores: Compact(num_cores),
				};

				let digest = CumulusDigestItem::CoreInfo(core_info).to_digest_item();

				frame_system::Pallet::<Test>::deposit_log(digest);
			}
		});

		ext
	}

	#[test]
	fn test_single_core_single_block() {
		new_test_ext_with_digest(Some(1)).execute_with(|| {
			let weight = MaxParachainBlockWeight::get::<Test>(1);

			// With 1 core and 1 target block, should get full 2s ref time and full PoV size
			assert_eq!(weight.ref_time(), 2_000_000_000);
			assert_eq!(weight.proof_size(), MAX_POV_SIZE as u64);
		});
	}

	#[test]
	fn test_single_core_multiple_blocks() {
		new_test_ext_with_digest(Some(1)).execute_with(|| {
			let weight = MaxParachainBlockWeight::get::<Test>(4);

			// With 1 core and 4 target blocks, should get 0.5s ref time and 1/4 PoV size per block
			assert_eq!(weight.ref_time(), 500_000_000);
			assert_eq!(weight.proof_size(), (MAX_POV_SIZE as u64) / 4);
		});
	}

	#[test]
	fn test_multiple_cores_single_block() {
		new_test_ext_with_digest(Some(3)).execute_with(|| {
			let weight = MaxParachainBlockWeight::get::<Test>(1);

			// With 3 cores and 1 target block, should get 6s ref time total and full PoV size
			assert_eq!(weight.ref_time(), 6_000_000_000);
			assert_eq!(weight.proof_size(), MAX_POV_SIZE as u64);
		});
	}

	#[test]
	fn test_multiple_cores_multiple_blocks() {
		new_test_ext_with_digest(Some(2)).execute_with(|| {
			let weight = MaxParachainBlockWeight::get::<Test>(4);

			// With 2 cores and 4 target blocks, should get 1s ref time and 1/4 PoV size per block
			assert_eq!(weight.ref_time(), 1_000_000_000);
			assert_eq!(weight.proof_size(), (MAX_POV_SIZE as u64) / 4);
		});
	}

	#[test]
	fn test_no_core_info() {
		new_test_ext_with_digest(None).execute_with(|| {
			let weight = MaxParachainBlockWeight::get::<Test>(1);

			// Without core info, should return conservative default
			assert_eq!(weight.ref_time(), 2_000_000_000);
			assert_eq!(weight.proof_size(), MAX_POV_SIZE as u64);
		});
	}

	#[test]
	fn test_zero_cores() {
		new_test_ext_with_digest(Some(0)).execute_with(|| {
			let weight = MaxParachainBlockWeight::get::<Test>(1);

			// With 0 cores, should return conservative default
			assert_eq!(weight.ref_time(), 2_000_000_000);
			assert_eq!(weight.proof_size(), MAX_POV_SIZE as u64);
		});
	}

	#[test]
	fn test_zero_target_blocks() {
		new_test_ext_with_digest(Some(2)).execute_with(|| {
			let weight = MaxParachainBlockWeight::get::<Test>(0);

			// With 0 target blocks, should return conservative default
			assert_eq!(weight.ref_time(), 2_000_000_000);
			assert_eq!(weight.proof_size(), MAX_POV_SIZE as u64);
		});
	}
}
