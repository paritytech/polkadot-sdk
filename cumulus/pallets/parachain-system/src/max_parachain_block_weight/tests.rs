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

use super::{mock::*, transaction_extension::DynamicMaxBlockWeight, *};
use crate as parachain_system;
use codec::Compact;
use cumulus_primitives_core::{
	BundleInfo, ClaimQueueOffset, CoreInfo, CoreSelector, CumulusDigestItem,
};
use frame_support::{
	construct_runtime, derive_impl,
	dispatch::{DispatchClass, DispatchInfo, Pays},
	traits::Hooks,
	weights::{constants::WEIGHT_REF_TIME_PER_SECOND, Weight},
};
use frame_system::mocking::MockBlock;
use polkadot_primitives::MAX_POV_SIZE;
use sp_core::ConstU32;
use sp_io;
use sp_runtime::{
	generic::Header,
	testing::{TestXt, UintAuthorityId},
	traits::{
		BlakeTwo256, Block as BlockT, Dispatchable, Header as HeaderT, IdentityLookup,
		TransactionExtension,
	},
	transaction_validity::TransactionSource,
	BuildStorage, Perbill,
};

#[test]
fn test_single_core_single_block() {
	new_test_ext_with_digest(Some(1)).execute_with(|| {
		let weight = MaxParachainBlockWeight::<Test>::get(1);

		// With 1 core and 1 target block, should get full 2s ref time and full PoV size
		assert_eq!(weight.ref_time(), 2 * WEIGHT_REF_TIME_PER_SECOND);
		assert_eq!(weight.proof_size(), MAX_POV_SIZE as u64);
	});
}

#[test]
fn test_single_core_multiple_blocks() {
	new_test_ext_with_digest(Some(1)).execute_with(|| {
		let weight = MaxParachainBlockWeight::<Test>::get(4);

		// With 1 core and 4 target blocks, should get 0.5s ref time and 1/4 PoV size per block
		assert_eq!(weight.ref_time(), 2 * WEIGHT_REF_TIME_PER_SECOND / 4);
		assert_eq!(weight.proof_size(), (1 * MAX_POV_SIZE as u64) / 4);
	});
}

#[test]
fn test_multiple_cores_single_block() {
	new_test_ext_with_digest(Some(3)).execute_with(|| {
		let weight = MaxParachainBlockWeight::<Test>::get(1);

		// With 3 cores and 1 target block, should get max 2s ref time (capped per core) and 3x
		// PoV size
		assert_eq!(weight.ref_time(), 2 * WEIGHT_REF_TIME_PER_SECOND);
		assert_eq!(weight.proof_size(), 3 * MAX_POV_SIZE as u64);
	});
}

#[test]
fn test_multiple_cores_multiple_blocks() {
	new_test_ext_with_digest(Some(2)).execute_with(|| {
		let weight = MaxParachainBlockWeight::<Test>::get(4);

		// With 2 cores and 4 target blocks, should get 1s ref time and 2x PoV size / 4 per
		// block
		assert_eq!(weight.ref_time(), 2 * 2 * WEIGHT_REF_TIME_PER_SECOND / 4);
		assert_eq!(weight.proof_size(), (2 * MAX_POV_SIZE as u64) / 4);
	});
}

#[test]
fn test_no_core_info() {
	new_test_ext_with_digest(None).execute_with(|| {
		let weight = MaxParachainBlockWeight::<Test>::get(1);

		// Without core info, should return conservative default
		assert_eq!(weight.ref_time(), 2 * WEIGHT_REF_TIME_PER_SECOND);
		assert_eq!(weight.proof_size(), MAX_POV_SIZE as u64);
	});
}

#[test]
fn test_zero_cores() {
	new_test_ext_with_digest(Some(0)).execute_with(|| {
		let weight = MaxParachainBlockWeight::<Test>::get(1);

		// With 0 cores, should return conservative default
		assert_eq!(weight.ref_time(), 2 * WEIGHT_REF_TIME_PER_SECOND);
		assert_eq!(weight.proof_size(), MAX_POV_SIZE as u64);
	});
}

#[test]
fn test_zero_target_blocks() {
	new_test_ext_with_digest(Some(2)).execute_with(|| {
		let weight = MaxParachainBlockWeight::<Test>::get(0);

		// With 0 target blocks, should return conservative default
		assert_eq!(weight.ref_time(), 2 * WEIGHT_REF_TIME_PER_SECOND);
		assert_eq!(weight.proof_size(), MAX_POV_SIZE as u64);
	});
}

#[test]
fn test_target_block_weight_calculation() {
	new_test_ext_with_digest(Some(4)).execute_with(|| {
		// Test target_block_weight function directly
		let weight_2_blocks = MaxParachainBlockWeight::<Test>::target_block_weight(2);
		let weight_8_blocks = MaxParachainBlockWeight::<Test>::target_block_weight(8);

		// With 4 cores and 2 target blocks, should get 2s per block
		assert_eq!(weight_2_blocks.ref_time(), 2 * WEIGHT_REF_TIME_PER_SECOND);
		assert_eq!(weight_2_blocks.proof_size(), (4 * MAX_POV_SIZE as u64) / 2);

		// With 4 cores and 8 target blocks, should get 1s per block
		assert_eq!(weight_8_blocks.ref_time(), 2 * 4 * WEIGHT_REF_TIME_PER_SECOND / 8);
		assert_eq!(weight_8_blocks.proof_size(), (4 * MAX_POV_SIZE as u64) / 8);
	});
}

#[test]
fn test_max_ref_time_per_core_cap() {
	new_test_ext_with_digest(Some(8)).execute_with(|| {
		// Even with many cores, ref time per block should be capped at MAX_REF_TIME_PER_CORE_NS
		let weight = MaxParachainBlockWeight::<Test>::get(1);

		// Should be capped at 2s ref time per core
		assert_eq!(weight.ref_time(), 2 * WEIGHT_REF_TIME_PER_SECOND);
		// But proof size should scale with number of cores
		assert_eq!(weight.proof_size(), 8 * MAX_POV_SIZE as u64);
	});
}

#[test]
fn test_target_block_weight_with_digest_edge_cases() {
	use cumulus_primitives_core::CumulusDigestItem;
	use sp_runtime::Digest;

	// Test with empty digest
	let empty_digest = Digest::default();
	let weight = MaxParachainBlockWeight::<Test>::target_block_weight_with_digest(1, &empty_digest);
	assert_eq!(weight, MaxParachainBlockWeight::<Test>::FULL_CORE_WEIGHT);

	// Test with digest containing core info
	let core_info = CoreInfo {
		selector: CoreSelector(0),
		claim_queue_offset: ClaimQueueOffset(0),
		number_of_cores: Compact(2u16),
	};
	let digest_item = CumulusDigestItem::CoreInfo(core_info).to_digest_item();
	let mut digest = Digest::default();
	digest.push(digest_item);

	let weight = MaxParachainBlockWeight::<Test>::target_block_weight_with_digest(2, &digest);
	assert_eq!(weight.ref_time(), 2 * 2 * WEIGHT_REF_TIME_PER_SECOND / 2);
	assert_eq!(weight.proof_size(), (2 * MAX_POV_SIZE as u64) / 2);
}

#[test]
fn test_is_first_block_in_core_functions() {
	use cumulus_primitives_core::{BundleInfo, CumulusDigestItem};
	use sp_runtime::Digest;

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

#[test]
fn test_dynamic_max_block_weight_creation() {
	use super::transaction_extension::DynamicMaxBlockWeight;

	// Test creating DynamicMaxBlockWeight with new()
	let inner = ();
	let dynamic_weight = DynamicMaxBlockWeight::<Test, (), ()>::new(inner);
	assert_eq!(dynamic_weight.0, ());

	// Test creating DynamicMaxBlockWeight with From trait
	let dynamic_weight_from: DynamicMaxBlockWeight<Test, (), ()> = ().into();
	assert_eq!(dynamic_weight_from.0, ());

	// Test Debug formatting
	let debug_string = format!("{:?}", dynamic_weight);
	assert!(debug_string.contains("DynamicMaxBlockWeight"));
}

#[test]
fn test_max_block_weight_hooks_type() {
	use super::pre_inherents_hook::DynamicMaxBlockWeightHooks;
	use sp_core::ConstU32;

	// Ensure the type can be instantiated (compile-time test)
	let _hooks: DynamicMaxBlockWeightHooks<Test, ConstU32<2>> =
		DynamicMaxBlockWeightHooks(core::marker::PhantomData);
}

#[test]
fn test_block_weight_mode_with_different_transaction_indices() {
	// Test BlockWeightMode with None transaction indices
	let mode_with_none = BlockWeightMode::PotentialFullCore {
		first_transaction_index: None,
		target_weight: Weight::zero(),
	};
	let mode_with_some = BlockWeightMode::FractionOfCore { first_transaction_index: Some(42) };

	// Test encoding/decoding
	use codec::{Decode, Encode};
	let encoded_none = mode_with_none.encode();
	let decoded_none = BlockWeightMode::decode(&mut &encoded_none[..]).unwrap();
	assert!(matches!(
		decoded_none,
		BlockWeightMode::PotentialFullCore {
			first_transaction_index: None,
			target_weight: Weight::Zero
		}
	));

	let encoded_some = mode_with_some.encode();
	let decoded_some = BlockWeightMode::decode(&mut &encoded_some[..]).unwrap();
	assert!(matches!(
		decoded_some,
		BlockWeightMode::FractionOfCore { first_transaction_index: Some(42) }
	));
}

#[test]
fn test_saturation_arithmetic() {
	new_test_ext_with_digest(Some(u16::MAX)).execute_with(|| {
		// Test with maximum number of cores to ensure no overflow
		let weight = MaxParachainBlockWeight::<Test>::get(1);

		// Should be capped at 2s ref time per core even with max cores
		assert_eq!(weight.ref_time(), 2 * WEIGHT_REF_TIME_PER_SECOND);
		// Proof size should saturate properly
		assert!(weight.proof_size() > 0);
	});
}

#[test]
fn test_large_target_blocks() {
	new_test_ext_with_digest(Some(4)).execute_with(|| {
		// Test with very large number of target blocks
		let weight = MaxParachainBlockWeight::<Test>::get(u32::MAX);

		// Should not panic and should return minimal weights
		assert!(weight.ref_time() > 0);
		assert!(weight.proof_size() > 0);
	});
}
