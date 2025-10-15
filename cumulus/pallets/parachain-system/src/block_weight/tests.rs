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

use super::{mock::*, *};
use codec::Compact;
use cumulus_primitives_core::{
	BundleInfo, ClaimQueueOffset, CoreInfo, CoreSelector, CumulusDigestItem,
};
use frame_support::weights::constants::WEIGHT_REF_TIME_PER_SECOND;
use polkadot_primitives::MAX_POV_SIZE;
use sp_core::ConstU32;
use sp_runtime::Digest;

#[test]
fn test_single_core_single_block() {
	new_test_ext_with_digest(Some(1)).execute_with(|| {
		let weight = MaxParachainBlockWeight::<Runtime, ConstU32<1>>::get();

		assert_eq!(weight.ref_time(), 2 * WEIGHT_REF_TIME_PER_SECOND);
		assert_eq!(weight.proof_size(), MAX_POV_SIZE as u64);
	});
}

#[test]
fn test_single_core_multiple_blocks() {
	new_test_ext_with_digest(Some(1)).execute_with(|| {
		let weight = MaxParachainBlockWeight::<Runtime, ConstU32<4>>::get();

		// With 1 core and 4 target blocks, should get 0.5s ref time and 1/4 PoV size per block
		assert_eq!(weight.ref_time(), 2 * WEIGHT_REF_TIME_PER_SECOND / 4);
		assert_eq!(weight.proof_size(), (1 * MAX_POV_SIZE as u64) / 4);
	});
}

#[test]
fn test_multiple_cores_single_block() {
	new_test_ext_with_digest(Some(3)).execute_with(|| {
		let weight = MaxParachainBlockWeight::<Runtime, ConstU32<1>>::get();

		// With 3 cores and 1 target blocks, should get 2s ref time and 1 PoV size
		assert_eq!(weight.ref_time(), 2 * WEIGHT_REF_TIME_PER_SECOND);
		assert_eq!(weight.proof_size(), MAX_POV_SIZE as u64);
	});
}

#[test]
fn test_multiple_cores_multiple_blocks() {
	new_test_ext_with_digest(Some(2)).execute_with(|| {
		let weight = MaxParachainBlockWeight::<Runtime, ConstU32<4>>::get();

		// With 2 cores and 4 target blocks, should get 1s ref time and 2x PoV size / 4 per
		// block
		assert_eq!(weight.ref_time(), 2 * 2 * WEIGHT_REF_TIME_PER_SECOND / 4);
		assert_eq!(weight.proof_size(), (2 * MAX_POV_SIZE as u64) / 4);
	});
}

#[test]
fn test_no_core_info() {
	new_test_ext_with_digest(None).execute_with(|| {
		let weight = MaxParachainBlockWeight::<Runtime, ConstU32<4>>::get();

		// Without core info, should return conservative default
		assert_eq!(weight.ref_time(), 2 * WEIGHT_REF_TIME_PER_SECOND);
		assert_eq!(weight.proof_size(), MAX_POV_SIZE as u64);
	});
}

#[test]
fn test_zero_cores() {
	new_test_ext_with_digest(Some(0)).execute_with(|| {
		let weight = MaxParachainBlockWeight::<Runtime, ConstU32<4>>::get();

		// With 0 cores, should return conservative default
		assert_eq!(weight.ref_time(), 2 * WEIGHT_REF_TIME_PER_SECOND);
		assert_eq!(weight.proof_size(), MAX_POV_SIZE as u64);
	});
}

#[test]
fn test_zero_target_blocks() {
	new_test_ext_with_digest(Some(2)).execute_with(|| {
		let weight = MaxParachainBlockWeight::<Runtime, ConstU32<0>>::get();
		assert_eq!(weight.ref_time(), 2 * WEIGHT_REF_TIME_PER_SECOND);
		assert_eq!(weight.proof_size(), MAX_POV_SIZE as u64);
	});
}

#[test]
fn test_target_block_weight_calculation() {
	new_test_ext_with_digest(Some(4)).execute_with(|| {
		// Test target_block_weight function directly
		// Both calls return the same since ConstU32<4> is fixed at compile time
		let weight = MaxParachainBlockWeight::<Runtime, ConstU32<4>>::target_block_weight();

		// With 4 cores and 4 target blocks, should get 2s per block (8s / 4)
		assert_eq!(weight.ref_time(), 4 * 2 * WEIGHT_REF_TIME_PER_SECOND / 4);
		assert_eq!(weight.proof_size(), (4 * MAX_POV_SIZE as u64) / 4);
	});
}

#[test]
fn test_max_ref_time_per_core_cap() {
	new_test_ext_with_digest(Some(8)).execute_with(|| {
		// With 8 cores and 4 target blocks, ref time per block should be capped at 2s per core
		let weight = MaxParachainBlockWeight::<Runtime, ConstU32<4>>::get();

		// 8 cores * 2s = 16s total, divided by 4 blocks = 4s, but capped at 6s for all blocks in
		// total
		assert_eq!(weight.ref_time(), 2 * WEIGHT_REF_TIME_PER_SECOND * 3 / 4);
		assert_eq!(weight.proof_size(), 4 * MAX_POV_SIZE as u64);
	});
}

#[test]
fn test_target_block_weight_with_digest_edge_cases() {
	// Test with empty digest
	let empty_digest = Digest::default();
	let weight = MaxParachainBlockWeight::<Runtime, ConstU32<4>>::target_block_weight_with_digest(
		&empty_digest,
	);
	assert_eq!(weight, MaxParachainBlockWeight::<Runtime, ConstU32<4>>::FULL_CORE_WEIGHT);

	// Test with digest containing core info
	let core_info = CoreInfo {
		selector: CoreSelector(0),
		claim_queue_offset: ClaimQueueOffset(0),
		number_of_cores: Compact(2u16),
	};

	let digest = Digest { logs: vec![CumulusDigestItem::CoreInfo(core_info).to_digest_item()] };

	// With 2 cores and 4 target blocks: (2 cores * 2s) / 4 blocks = 1s
	let weight =
		MaxParachainBlockWeight::<Runtime, ConstU32<4>>::target_block_weight_with_digest(&digest);
	assert_eq!(weight.ref_time(), 2 * 2 * WEIGHT_REF_TIME_PER_SECOND / 4);
	assert_eq!(weight.proof_size(), (2 * MAX_POV_SIZE as u64) / 4);
}

#[test]
fn test_is_first_block_in_core_functions() {
	new_test_ext_with_digest(Some(1)).execute_with(|| {
		// Test without bundle info - should return false
		let empty_digest = Digest::default();
		assert!(!super::is_first_block_in_core_with_digest(&empty_digest));

		// Test with bundle info index = 0 - should return true
		let bundle_info_first = BundleInfo { index: 0, maybe_last: false };
		let digest_item_first = CumulusDigestItem::BundleInfo(bundle_info_first).to_digest_item();
		let mut digest_first = Digest::default();
		digest_first.push(digest_item_first);
		assert!(super::is_first_block_in_core_with_digest(&digest_first));

		// Test with bundle info index > 0 - should return false
		let bundle_info_not_first = BundleInfo { index: 5, maybe_last: true };
		let digest_item_not_first =
			CumulusDigestItem::BundleInfo(bundle_info_not_first).to_digest_item();
		let mut digest_not_first = Digest::default();
		digest_not_first.push(digest_item_not_first);
		assert!(!super::is_first_block_in_core_with_digest(&digest_not_first));
	});
}
