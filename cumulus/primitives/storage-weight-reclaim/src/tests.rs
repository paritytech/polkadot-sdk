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

use super::*;
use core::marker::PhantomData;
use frame_support::{
	assert_ok,
	dispatch::{DispatchClass, PerDispatchClass},
	weights::{Weight, WeightMeter},
};
use frame_system::{BlockWeight, CheckWeight};
use sp_runtime::{traits::DispatchTransaction, AccountId32, BuildStorage};
use sp_trie::proof_size_extension::ProofSizeExt;

type Test = cumulus_test_runtime::Runtime;
const CALL: &<Test as Config>::RuntimeCall =
	&cumulus_test_runtime::RuntimeCall::System(frame_system::Call::set_heap_pages { pages: 0u64 });
const ALICE: AccountId32 = AccountId32::new([1u8; 32]);
const LEN: usize = 150;

fn new_test_ext() -> sp_io::TestExternalities {
	let ext: sp_io::TestExternalities = cumulus_test_runtime::RuntimeGenesisConfig::default()
		.build_storage()
		.unwrap()
		.into();
	ext
}

struct TestRecorder {
	return_values: Box<[usize]>,
	counter: core::sync::atomic::AtomicUsize,
}

impl TestRecorder {
	fn new(values: &[usize]) -> Self {
		TestRecorder { return_values: values.into(), counter: Default::default() }
	}
}

impl sp_trie::ProofSizeProvider for TestRecorder {
	fn estimate_encoded_size(&self) -> usize {
		let counter = self.counter.fetch_add(1, core::sync::atomic::Ordering::Relaxed);
		self.return_values[counter]
	}
}

fn setup_test_externalities(proof_values: &[usize]) -> sp_io::TestExternalities {
	let mut test_ext = new_test_ext();
	let test_recorder = TestRecorder::new(proof_values);
	test_ext.register_extension(ProofSizeExt::new(test_recorder));
	test_ext
}

fn set_current_storage_weight(new_weight: u64) {
	BlockWeight::<Test>::mutate(|current_weight| {
		current_weight.set(Weight::from_parts(0, new_weight), DispatchClass::Normal);
	});
}

fn get_storage_weight() -> PerDispatchClass<Weight> {
	BlockWeight::<Test>::get()
}

#[test]
fn basic_refund() {
	// The real cost will be 100 bytes of storage size
	let mut test_ext = setup_test_externalities(&[0, 100]);

	test_ext.execute_with(|| {
		set_current_storage_weight(1000);

		// Benchmarked storage weight: 500
		let info = DispatchInfo { call_weight: Weight::from_parts(0, 500), ..Default::default() };
		let post_info = PostDispatchInfo::default();

		// Should add 500 + 150 (len) to weight.
		let (_, next_len) = CheckWeight::<Test>::do_validate(&info, LEN).unwrap();
		assert_ok!(CheckWeight::<Test>::do_prepare(&info, LEN, next_len));

		let (pre, _) = StorageWeightReclaim::<Test>(PhantomData)
			.validate_and_prepare(Some(ALICE.clone()).into(), CALL, &info, LEN)
			.unwrap();
		assert_eq!(pre, Some(0));

		assert_ok!(CheckWeight::<Test>::post_dispatch_details((), &info, &post_info, 0, &Ok(()),));
		// We expect a refund of 400
		assert_ok!(StorageWeightReclaim::<Test>::post_dispatch_details(
			pre,
			&info,
			&post_info,
			LEN,
			&Ok(()),
		));

		assert_eq!(get_storage_weight().total().proof_size(), 1250);
	})
}

#[test]
fn underestimating_refund() {
	// We fixed a bug where `pre dispatch info weight > consumed weight > post info weight`
	// resulted in error.

	// The real cost will be 100 bytes of storage size
	let mut test_ext = setup_test_externalities(&[0, 100]);

	test_ext.execute_with(|| {
		set_current_storage_weight(1000);

		// Benchmarked storage weight: 500
		let info = DispatchInfo { call_weight: Weight::from_parts(0, 101), ..Default::default() };
		let post_info = PostDispatchInfo {
			actual_weight: Some(Weight::from_parts(0, 99)),
			pays_fee: Default::default(),
		};

		let (_, next_len) = CheckWeight::<Test>::do_validate(&info, LEN).unwrap();
		assert_ok!(CheckWeight::<Test>::do_prepare(&info, LEN, next_len));

		let (pre, _) = StorageWeightReclaim::<Test>(PhantomData)
			.validate_and_prepare(Some(ALICE.clone()).into(), CALL, &info, LEN)
			.unwrap();
		assert_eq!(pre, Some(0));

		assert_ok!(CheckWeight::<Test>::post_dispatch_details((), &info, &post_info, 0, &Ok(())));
		// We expect an accrue of 1
		assert_ok!(StorageWeightReclaim::<Test>::post_dispatch_details(
			pre,
			&info,
			&post_info,
			LEN,
			&Ok(())
		));

		assert_eq!(get_storage_weight().total().proof_size(), 1250);
	})
}

#[test]
fn sets_to_node_storage_proof_if_higher() {
	// The storage proof reported by the proof recorder is higher than what is stored on
	// the runtime side.
	{
		let mut test_ext = setup_test_externalities(&[1000, 1005]);

		test_ext.execute_with(|| {
			// Stored in BlockWeight is 5
			set_current_storage_weight(5);

			// Benchmarked storage weight: 10
			let info =
				DispatchInfo { call_weight: Weight::from_parts(0, 10), ..Default::default() };
			let post_info = PostDispatchInfo::default();

			let (_, next_len) = CheckWeight::<Test>::do_validate(&info, LEN).unwrap();
			assert_ok!(CheckWeight::<Test>::do_prepare(&info, LEN, next_len));

			let (pre, _) = StorageWeightReclaim::<Test>(PhantomData)
				.validate_and_prepare(Some(ALICE.clone()).into(), CALL, &info, LEN)
				.unwrap();
			assert_eq!(pre, Some(1000));

			assert_ok!(CheckWeight::<Test>::post_dispatch_details(
				(),
				&info,
				&post_info,
				0,
				&Ok(())
			));
			assert_ok!(StorageWeightReclaim::<Test>::post_dispatch_details(
				pre,
				&info,
				&post_info,
				LEN,
				&Ok(())
			));

			// We expect that the storage weight was set to the node-side proof size (1005) +
			// extrinsics length (150)
			assert_eq!(get_storage_weight().total().proof_size(), 1155);
		})
	}

	// In this second scenario the proof size on the node side is only lower
	// after reclaim happened.
	{
		let mut test_ext = setup_test_externalities(&[175, 180]);
		test_ext.execute_with(|| {
			set_current_storage_weight(85);

			// Benchmarked storage weight: 100
			let info =
				DispatchInfo { call_weight: Weight::from_parts(0, 100), ..Default::default() };
			let post_info = PostDispatchInfo::default();

			// After this pre_dispatch, the BlockWeight proof size will be
			// 85 (initial) + 100 (benched) + 150 (tx length) = 335
			let (_, next_len) = CheckWeight::<Test>::do_validate(&info, LEN).unwrap();
			assert_ok!(CheckWeight::<Test>::do_prepare(&info, LEN, next_len));

			let (pre, _) = StorageWeightReclaim::<Test>(PhantomData)
				.validate_and_prepare(Some(ALICE.clone()).into(), CALL, &info, LEN)
				.unwrap();
			assert_eq!(pre, Some(175));

			assert_ok!(CheckWeight::<Test>::post_dispatch_details(
				(),
				&info,
				&post_info,
				0,
				&Ok(())
			));

			// First we will reclaim 95, which leaves us with 240 BlockWeight. This is lower
			// than 180 (proof size hf) + 150 (length), so we expect it to be set to 330.
			assert_ok!(StorageWeightReclaim::<Test>::post_dispatch_details(
				pre,
				&info,
				&post_info,
				LEN,
				&Ok(())
			));

			// We expect that the storage weight was set to the node-side proof weight
			assert_eq!(get_storage_weight().total().proof_size(), 330);
		})
	}
}

#[test]
fn does_nothing_without_extension() {
	let mut test_ext = new_test_ext();

	// Proof size extension not registered
	test_ext.execute_with(|| {
		set_current_storage_weight(1000);

		// Benchmarked storage weight: 500
		let info = DispatchInfo { call_weight: Weight::from_parts(0, 500), ..Default::default() };
		let post_info = PostDispatchInfo::default();

		// Adds 500 + 150 (len) weight
		let (_, next_len) = CheckWeight::<Test>::do_validate(&info, LEN).unwrap();
		assert_ok!(CheckWeight::<Test>::do_prepare(&info, LEN, next_len));

		let (pre, _) = StorageWeightReclaim::<Test>(PhantomData)
			.validate_and_prepare(Some(ALICE.clone()).into(), CALL, &info, LEN)
			.unwrap();
		assert_eq!(pre, None);

		assert_ok!(CheckWeight::<Test>::post_dispatch_details((), &info, &post_info, 0, &Ok(()),));
		assert_ok!(StorageWeightReclaim::<Test>::post_dispatch_details(
			pre,
			&info,
			&post_info,
			LEN,
			&Ok(()),
		));

		assert_eq!(get_storage_weight().total().proof_size(), 1650);
	})
}

#[test]
fn negative_refund_is_added_to_weight() {
	let mut test_ext = setup_test_externalities(&[100, 300]);

	test_ext.execute_with(|| {
		set_current_storage_weight(1000);
		// Benchmarked storage weight: 100
		let info = DispatchInfo { call_weight: Weight::from_parts(0, 100), ..Default::default() };
		let post_info = PostDispatchInfo::default();

		// Weight added should be 100 + 150 (len)
		let (_, next_len) = CheckWeight::<Test>::do_validate(&info, LEN).unwrap();
		assert_ok!(CheckWeight::<Test>::do_prepare(&info, LEN, next_len));

		let (pre, _) = StorageWeightReclaim::<Test>(PhantomData)
			.validate_and_prepare(Some(ALICE.clone()).into(), CALL, &info, LEN)
			.unwrap();
		assert_eq!(pre, Some(100));

		// We expect no refund
		assert_ok!(CheckWeight::<Test>::post_dispatch_details((), &info, &post_info, 0, &Ok(()),));
		assert_ok!(StorageWeightReclaim::<Test>::post_dispatch_details(
			pre,
			&info,
			&post_info,
			LEN,
			&Ok(()),
		));

		assert_eq!(
			get_storage_weight().total().proof_size(),
			1100 + LEN as u64 + info.total_weight().proof_size()
		);
	})
}

#[test]
fn test_zero_proof_size() {
	let mut test_ext = setup_test_externalities(&[0, 0]);

	test_ext.execute_with(|| {
		let info = DispatchInfo { call_weight: Weight::from_parts(0, 500), ..Default::default() };
		let post_info = PostDispatchInfo::default();

		let (_, next_len) = CheckWeight::<Test>::do_validate(&info, LEN).unwrap();
		assert_ok!(CheckWeight::<Test>::do_prepare(&info, LEN, next_len));

		let (pre, _) = StorageWeightReclaim::<Test>(PhantomData)
			.validate_and_prepare(Some(ALICE.clone()).into(), CALL, &info, LEN)
			.unwrap();
		assert_eq!(pre, Some(0));

		assert_ok!(CheckWeight::<Test>::post_dispatch_details((), &info, &post_info, 0, &Ok(()),));
		assert_ok!(StorageWeightReclaim::<Test>::post_dispatch_details(
			pre,
			&info,
			&post_info,
			LEN,
			&Ok(()),
		));

		// Proof size should be exactly equal to extrinsic length
		assert_eq!(get_storage_weight().total().proof_size(), LEN as u64);
	});
}

#[test]
fn test_larger_pre_dispatch_proof_size() {
	let mut test_ext = setup_test_externalities(&[300, 100]);

	test_ext.execute_with(|| {
		set_current_storage_weight(1300);

		let info = DispatchInfo { call_weight: Weight::from_parts(0, 500), ..Default::default() };
		let post_info = PostDispatchInfo::default();

		// Adds 500 + 150 (len) weight, total weight is 1950
		let (_, next_len) = CheckWeight::<Test>::do_validate(&info, LEN).unwrap();
		assert_ok!(CheckWeight::<Test>::do_prepare(&info, LEN, next_len));

		let (pre, _) = StorageWeightReclaim::<Test>(PhantomData)
			.validate_and_prepare(Some(ALICE.clone()).into(), CALL, &info, LEN)
			.unwrap();
		assert_eq!(pre, Some(300));

		// Refund 500 unspent weight according to `post_info`, total weight is now 1650
		assert_ok!(CheckWeight::<Test>::post_dispatch_details((), &info, &post_info, 0, &Ok(()),));
		// Recorded proof size is negative -200, total weight is now 1450
		assert_ok!(StorageWeightReclaim::<Test>::post_dispatch_details(
			pre,
			&info,
			&post_info,
			LEN,
			&Ok(()),
		));

		assert_eq!(get_storage_weight().total().proof_size(), 1450);
	});
}

#[test]
fn test_incorporates_check_weight_unspent_weight() {
	let mut test_ext = setup_test_externalities(&[100, 300]);

	test_ext.execute_with(|| {
		set_current_storage_weight(1000);

		// Benchmarked storage weight: 300
		let info = DispatchInfo { call_weight: Weight::from_parts(100, 300), ..Default::default() };

		// Actual weight is 50
		let post_info = PostDispatchInfo {
			actual_weight: Some(Weight::from_parts(50, 250)),
			pays_fee: Default::default(),
		};

		// Should add 300 + 150 (len) of weight
		let (_, next_len) = CheckWeight::<Test>::do_validate(&info, LEN).unwrap();
		assert_ok!(CheckWeight::<Test>::do_prepare(&info, LEN, next_len));

		let (pre, _) = StorageWeightReclaim::<Test>(PhantomData)
			.validate_and_prepare(Some(ALICE.clone()).into(), CALL, &info, LEN)
			.unwrap();
		assert_eq!(pre, Some(100));

		// The `CheckWeight` extension will refunt `actual_weight` from `PostDispatchInfo`
		// we always need to call `post_dispatch` to verify that they interoperate correctly.
		assert_ok!(CheckWeight::<Test>::post_dispatch_details((), &info, &post_info, 0, &Ok(()),));
		assert_ok!(StorageWeightReclaim::<Test>::post_dispatch_details(
			pre,
			&info,
			&post_info,
			LEN,
			&Ok(()),
		));

		// Reclaimed 100
		assert_eq!(get_storage_weight().total().proof_size(), 1350);
	})
}

#[test]
fn test_incorporates_check_weight_unspent_weight_on_negative() {
	let mut test_ext = setup_test_externalities(&[100, 300]);

	test_ext.execute_with(|| {
		set_current_storage_weight(1000);
		// Benchmarked storage weight: 50
		let info = DispatchInfo { call_weight: Weight::from_parts(100, 50), ..Default::default() };

		// Actual weight is 25
		let post_info = PostDispatchInfo {
			actual_weight: Some(Weight::from_parts(50, 25)),
			pays_fee: Default::default(),
		};

		// Adds 50 + 150 (len) weight, total weight 1200
		let (_, next_len) = CheckWeight::<Test>::do_validate(&info, LEN).unwrap();
		assert_ok!(CheckWeight::<Test>::do_prepare(&info, LEN, next_len));

		let (pre, _) = StorageWeightReclaim::<Test>(PhantomData)
			.validate_and_prepare(Some(ALICE.clone()).into(), CALL, &info, LEN)
			.unwrap();
		assert_eq!(pre, Some(100));

		// The `CheckWeight` extension will refunt `actual_weight` from `PostDispatchInfo`
		// we always need to call `post_dispatch` to verify that they interoperate correctly.
		// Refunds unspent 25 weight according to `post_info`, 1175
		assert_ok!(CheckWeight::<Test>::post_dispatch_details((), &info, &post_info, 0, &Ok(()),));
		// Adds 200 - 25 (unspent) == 175 weight, total weight 1350
		assert_ok!(StorageWeightReclaim::<Test>::post_dispatch_details(
			pre,
			&info,
			&post_info,
			LEN,
			&Ok(()),
		));

		assert_eq!(get_storage_weight().total().proof_size(), 1350);
	})
}

#[test]
fn test_nothing_relcaimed() {
	let mut test_ext = setup_test_externalities(&[0, 100]);

	test_ext.execute_with(|| {
		set_current_storage_weight(0);
		// Benchmarked storage weight: 100
		let info = DispatchInfo { call_weight: Weight::from_parts(100, 100), ..Default::default() };

		// Actual proof size is 100
		let post_info = PostDispatchInfo {
			actual_weight: Some(Weight::from_parts(50, 100)),
			pays_fee: Default::default(),
		};

		// Adds benchmarked weight 100 + 150 (len), total weight is now 250
		let (_, next_len) = CheckWeight::<Test>::do_validate(&info, LEN).unwrap();
		assert_ok!(CheckWeight::<Test>::do_prepare(&info, LEN, next_len));

		// Weight should go up by 150 len + 100 proof size weight, total weight 250
		assert_eq!(get_storage_weight().total().proof_size(), 250);

		let (pre, _) = StorageWeightReclaim::<Test>(PhantomData)
			.validate_and_prepare(Some(ALICE.clone()).into(), CALL, &info, LEN)
			.unwrap();
		// Should return `setup_test_externalities` proof recorder value: 100.
		assert_eq!(pre, Some(0));

		// The `CheckWeight` extension will refund `actual_weight` from `PostDispatchInfo`
		// we always need to call `post_dispatch` to verify that they interoperate correctly.
		// Nothing to refund, unspent is 0, total weight 250
		assert_ok!(CheckWeight::<Test>::post_dispatch_details((), &info, &post_info, LEN, &Ok(())));
		// `setup_test_externalities` proof recorder value: 200, so this means the extrinsic
		// actually used 100 proof size.
		// Nothing to refund or add, weight matches proof recorder
		assert_ok!(StorageWeightReclaim::<Test>::post_dispatch_details(
			pre,
			&info,
			&post_info,
			LEN,
			&Ok(())
		));

		// Check block len weight was not reclaimed:
		// 100 weight + 150 extrinsic len == 250 proof size
		assert_eq!(get_storage_weight().total().proof_size(), 250);
	})
}

#[test]
fn test_incorporates_check_weight_unspent_weight_reverse_order() {
	let mut test_ext = setup_test_externalities(&[100, 300]);

	test_ext.execute_with(|| {
		set_current_storage_weight(1000);

		// Benchmarked storage weight: 300
		let info = DispatchInfo { call_weight: Weight::from_parts(100, 300), ..Default::default() };

		// Actual weight is 50
		let post_info = PostDispatchInfo {
			actual_weight: Some(Weight::from_parts(50, 250)),
			pays_fee: Default::default(),
		};

		// Adds 300 + 150 (len) weight, total weight 1450
		let (_, next_len) = CheckWeight::<Test>::do_validate(&info, LEN).unwrap();
		assert_ok!(CheckWeight::<Test>::do_prepare(&info, LEN, next_len));

		let (pre, _) = StorageWeightReclaim::<Test>(PhantomData)
			.validate_and_prepare(Some(ALICE.clone()).into(), CALL, &info, LEN)
			.unwrap();
		assert_eq!(pre, Some(100));

		// This refunds 100 - 50(unspent), total weight is now 1400
		assert_ok!(StorageWeightReclaim::<Test>::post_dispatch_details(
			pre,
			&info,
			&post_info,
			LEN,
			&Ok(()),
		));
		// `CheckWeight` gets called after `StorageWeightReclaim` this time.
		// The `CheckWeight` extension will refunt `actual_weight` from `PostDispatchInfo`
		// we always need to call `post_dispatch` to verify that they interoperate correctly.
		assert_ok!(CheckWeight::<Test>::post_dispatch_details((), &info, &post_info, 0, &Ok(()),));

		// Above call refunds 50 (unspent), total weight is 1350 now
		assert_eq!(get_storage_weight().total().proof_size(), 1350);
	})
}

#[test]
fn test_incorporates_check_weight_unspent_weight_on_negative_reverse_order() {
	let mut test_ext = setup_test_externalities(&[100, 300]);

	test_ext.execute_with(|| {
		set_current_storage_weight(1000);
		// Benchmarked storage weight: 50
		let info = DispatchInfo { call_weight: Weight::from_parts(100, 50), ..Default::default() };

		// Actual weight is 25
		let post_info = PostDispatchInfo {
			actual_weight: Some(Weight::from_parts(50, 25)),
			pays_fee: Default::default(),
		};

		// Adds 50 + 150 (len) weight, total weight is 1200
		let (_, next_len) = CheckWeight::<Test>::do_validate(&info, LEN).unwrap();
		assert_ok!(CheckWeight::<Test>::do_prepare(&info, LEN, next_len));

		let (pre, _) = StorageWeightReclaim::<Test>(PhantomData)
			.validate_and_prepare(Some(ALICE.clone()).into(), CALL, &info, LEN)
			.unwrap();
		assert_eq!(pre, Some(100));

		// Adds additional 150 weight recorded
		assert_ok!(StorageWeightReclaim::<Test>::post_dispatch_details(
			pre,
			&info,
			&post_info,
			LEN,
			&Ok(()),
		));
		// `CheckWeight` gets called after `StorageWeightReclaim` this time.
		// The `CheckWeight` extension will refunt `actual_weight` from `PostDispatchInfo`
		// we always need to call `post_dispatch` to verify that they interoperate correctly.
		assert_ok!(CheckWeight::<Test>::post_dispatch_details((), &info, &post_info, 0, &Ok(()),));

		assert_eq!(get_storage_weight().total().proof_size(), 1350);
	})
}

#[test]
fn storage_size_reported_correctly() {
	let mut test_ext = setup_test_externalities(&[1000]);
	test_ext.execute_with(|| {
		assert_eq!(get_proof_size(), Some(1000));
	});

	let mut test_ext = new_test_ext();

	let test_recorder = TestRecorder::new(&[0]);

	test_ext.register_extension(ProofSizeExt::new(test_recorder));

	test_ext.execute_with(|| {
		assert_eq!(get_proof_size(), Some(0));
	});
}

#[test]
fn storage_size_disabled_reported_correctly() {
	let mut test_ext = setup_test_externalities(&[PROOF_RECORDING_DISABLED as usize]);

	test_ext.execute_with(|| {
		assert_eq!(get_proof_size(), None);
	});
}

#[test]
fn test_reclaim_helper() {
	let mut test_ext = setup_test_externalities(&[1000, 1300, 1800]);

	test_ext.execute_with(|| {
		let mut remaining_weight_meter = WeightMeter::with_limit(Weight::from_parts(0, 2000));
		let mut reclaim_helper = StorageWeightReclaimer::new(&remaining_weight_meter);
		remaining_weight_meter.consume(Weight::from_parts(0, 500));
		let reclaimed = reclaim_helper.reclaim_with_meter(&mut remaining_weight_meter);

		assert_eq!(reclaimed, Some(Weight::from_parts(0, 200)));

		remaining_weight_meter.consume(Weight::from_parts(0, 800));
		let reclaimed = reclaim_helper.reclaim_with_meter(&mut remaining_weight_meter);
		assert_eq!(reclaimed, Some(Weight::from_parts(0, 300)));
		assert_eq!(remaining_weight_meter.remaining(), Weight::from_parts(0, 1200));
	});
}

#[test]
fn test_reclaim_helper_does_not_reclaim_negative() {
	// Benchmarked weight does not change at all
	let mut test_ext = setup_test_externalities(&[1000, 1300]);

	test_ext.execute_with(|| {
		let mut remaining_weight_meter = WeightMeter::with_limit(Weight::from_parts(0, 1000));
		let mut reclaim_helper = StorageWeightReclaimer::new(&remaining_weight_meter);
		let reclaimed = reclaim_helper.reclaim_with_meter(&mut remaining_weight_meter);

		assert_eq!(reclaimed, Some(Weight::from_parts(0, 0)));
		assert_eq!(remaining_weight_meter.remaining(), Weight::from_parts(0, 1000));
	});

	// Benchmarked weight increases less than storage proof consumes
	let mut test_ext = setup_test_externalities(&[1000, 1300]);

	test_ext.execute_with(|| {
		let mut remaining_weight_meter = WeightMeter::with_limit(Weight::from_parts(0, 1000));
		let mut reclaim_helper = StorageWeightReclaimer::new(&remaining_weight_meter);
		remaining_weight_meter.consume(Weight::from_parts(0, 0));
		let reclaimed = reclaim_helper.reclaim_with_meter(&mut remaining_weight_meter);

		assert_eq!(reclaimed, Some(Weight::from_parts(0, 0)));
	});
}

/// Just here for doc purposes
fn get_benched_weight() -> Weight {
	Weight::from_parts(0, 5)
}

/// Just here for doc purposes
fn do_work() {}

#[docify::export_content(simple_reclaimer_example)]
fn reclaim_with_weight_meter() {
	let mut remaining_weight_meter = WeightMeter::with_limit(Weight::from_parts(10, 10));

	let benched_weight = get_benched_weight();

	// It is important to instantiate the `StorageWeightReclaimer` before we consume the weight
	// for a piece of work from the weight meter.
	let mut reclaim_helper = StorageWeightReclaimer::new(&remaining_weight_meter);

	if remaining_weight_meter.try_consume(benched_weight).is_ok() {
		// Perform some work that takes has `benched_weight` storage weight.
		do_work();

		// Reclaimer will detect that we only consumed 2 bytes, so 3 bytes are reclaimed.
		let reclaimed = reclaim_helper.reclaim_with_meter(&mut remaining_weight_meter);

		// We reclaimed 3 bytes of storage size!
		assert_eq!(reclaimed, Some(Weight::from_parts(0, 3)));
		assert_eq!(get_storage_weight().total().proof_size(), 10);
		assert_eq!(remaining_weight_meter.remaining(), Weight::from_parts(10, 8));
	}
}

#[test]
fn test_reclaim_helper_works_with_meter() {
	// The node will report 12 - 10 = 2 consumed storage size between the calls.
	let mut test_ext = setup_test_externalities(&[10, 12]);

	test_ext.execute_with(|| {
		// Initial storage size is 10.
		set_current_storage_weight(10);
		reclaim_with_weight_meter();
	});
}
