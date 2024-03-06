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
use frame_support::{
	assert_ok,
	dispatch::DispatchClass,
	weights::{Weight, WeightMeter},
};
use frame_system::{BlockWeight, CheckWeight};
use sp_runtime::{traits::DispatchTransaction, AccountId32, BuildStorage};
use sp_std::marker::PhantomData;
use sp_trie::proof_size_extension::ProofSizeExt;

type Test = cumulus_test_runtime::Runtime;
const CALL: &<Test as Config>::RuntimeCall =
	&cumulus_test_runtime::RuntimeCall::System(frame_system::Call::set_heap_pages { pages: 0u64 });
const ALICE: AccountId32 = AccountId32::new([1u8; 32]);
const LEN: usize = 0;

fn new_test_ext() -> sp_io::TestExternalities {
	let ext: sp_io::TestExternalities = cumulus_test_runtime::RuntimeGenesisConfig::default()
		.build_storage()
		.unwrap()
		.into();
	ext
}

struct TestRecorder {
	return_values: Box<[usize]>,
	counter: std::sync::atomic::AtomicUsize,
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

#[test]
fn basic_refund() {
	// The real cost will be 100 bytes of storage size
	let mut test_ext = setup_test_externalities(&[0, 100]);

	test_ext.execute_with(|| {
		set_current_storage_weight(1000);

		// Benchmarked storage weight: 500
		let info = DispatchInfo { weight: Weight::from_parts(0, 500), ..Default::default() };
		let post_info = PostDispatchInfo::default();

		let (pre, _) = StorageWeightReclaim::<Test>(PhantomData)
			.validate_and_prepare(Some(ALICE.clone()).into(), CALL, &info, LEN)
			.unwrap();
		assert_eq!(pre, Some(0));

		assert_ok!(CheckWeight::<Test>::post_dispatch((), &info, &post_info, 0, &Ok(()), &()));
		// We expect a refund of 400
		assert_ok!(StorageWeightReclaim::<Test>::post_dispatch(
			pre,
			&info,
			&post_info,
			LEN,
			&Ok(()),
			&()
		));

		assert_eq!(BlockWeight::<Test>::get().total().proof_size(), 600);
	})
}

#[test]
fn does_nothing_without_extension() {
	let mut test_ext = new_test_ext();

	// Proof size extension not registered
	test_ext.execute_with(|| {
		set_current_storage_weight(1000);

		// Benchmarked storage weight: 500
		let info = DispatchInfo { weight: Weight::from_parts(0, 500), ..Default::default() };
		let post_info = PostDispatchInfo::default();

		let (pre, _) = StorageWeightReclaim::<Test>(PhantomData)
			.validate_and_prepare(Some(ALICE.clone()).into(), CALL, &info, LEN)
			.unwrap();
		assert_eq!(pre, None);

		assert_ok!(CheckWeight::<Test>::post_dispatch((), &info, &post_info, 0, &Ok(()), &()));
		assert_ok!(StorageWeightReclaim::<Test>::post_dispatch(
			pre,
			&info,
			&post_info,
			LEN,
			&Ok(()),
			&()
		));

		assert_eq!(BlockWeight::<Test>::get().total().proof_size(), 1000);
	})
}

#[test]
fn negative_refund_is_added_to_weight() {
	let mut test_ext = setup_test_externalities(&[100, 300]);

	test_ext.execute_with(|| {
		set_current_storage_weight(1000);
		// Benchmarked storage weight: 100
		let info = DispatchInfo { weight: Weight::from_parts(0, 100), ..Default::default() };
		let post_info = PostDispatchInfo::default();

		let (pre, _) = StorageWeightReclaim::<Test>(PhantomData)
			.validate_and_prepare(Some(ALICE.clone()).into(), CALL, &info, LEN)
			.unwrap();
		assert_eq!(pre, Some(100));

		// We expect no refund
		assert_ok!(CheckWeight::<Test>::post_dispatch((), &info, &post_info, 0, &Ok(()), &()));
		assert_ok!(StorageWeightReclaim::<Test>::post_dispatch(
			pre,
			&info,
			&post_info,
			LEN,
			&Ok(()),
			&()
		));

		assert_eq!(BlockWeight::<Test>::get().total().proof_size(), 1100);
	})
}

#[test]
fn test_zero_proof_size() {
	let mut test_ext = setup_test_externalities(&[0, 0]);

	test_ext.execute_with(|| {
		let info = DispatchInfo { weight: Weight::from_parts(0, 500), ..Default::default() };
		let post_info = PostDispatchInfo::default();

		let (pre, _) = StorageWeightReclaim::<Test>(PhantomData)
			.validate_and_prepare(Some(ALICE.clone()).into(), CALL, &info, LEN)
			.unwrap();
		assert_eq!(pre, Some(0));

		assert_ok!(CheckWeight::<Test>::post_dispatch((), &info, &post_info, 0, &Ok(()), &()));
		assert_ok!(StorageWeightReclaim::<Test>::post_dispatch(
			pre,
			&info,
			&post_info,
			LEN,
			&Ok(()),
			&()
		));

		assert_eq!(BlockWeight::<Test>::get().total().proof_size(), 0);
	});
}

#[test]
fn test_larger_pre_dispatch_proof_size() {
	let mut test_ext = setup_test_externalities(&[300, 100]);

	test_ext.execute_with(|| {
		set_current_storage_weight(1300);

		let info = DispatchInfo { weight: Weight::from_parts(0, 500), ..Default::default() };
		let post_info = PostDispatchInfo::default();

		let (pre, _) = StorageWeightReclaim::<Test>(PhantomData)
			.validate_and_prepare(Some(ALICE.clone()).into(), CALL, &info, LEN)
			.unwrap();
		assert_eq!(pre, Some(300));

		assert_ok!(CheckWeight::<Test>::post_dispatch((), &info, &post_info, 0, &Ok(()), &()));
		assert_ok!(StorageWeightReclaim::<Test>::post_dispatch(
			pre,
			&info,
			&post_info,
			LEN,
			&Ok(()),
			&()
		));

		assert_eq!(BlockWeight::<Test>::get().total().proof_size(), 800);
	});
}

#[test]
fn test_incorporates_check_weight_unspent_weight() {
	let mut test_ext = setup_test_externalities(&[100, 300]);

	test_ext.execute_with(|| {
		set_current_storage_weight(1000);

		// Benchmarked storage weight: 300
		let info = DispatchInfo { weight: Weight::from_parts(100, 300), ..Default::default() };

		// Actual weight is 50
		let post_info = PostDispatchInfo {
			actual_weight: Some(Weight::from_parts(50, 250)),
			pays_fee: Default::default(),
		};

		let (pre, _) = StorageWeightReclaim::<Test>(PhantomData)
			.validate_and_prepare(Some(ALICE.clone()).into(), CALL, &info, LEN)
			.unwrap();
		assert_eq!(pre, Some(100));

		// The `CheckWeight` extension will refunt `actual_weight` from `PostDispatchInfo`
		// we always need to call `post_dispatch` to verify that they interoperate correctly.
		assert_ok!(CheckWeight::<Test>::post_dispatch((), &info, &post_info, 0, &Ok(()), &()));
		assert_ok!(StorageWeightReclaim::<Test>::post_dispatch(
			pre,
			&info,
			&post_info,
			LEN,
			&Ok(()),
			&()
		));

		assert_eq!(BlockWeight::<Test>::get().total().proof_size(), 900);
	})
}

#[test]
fn test_incorporates_check_weight_unspent_weight_on_negative() {
	let mut test_ext = setup_test_externalities(&[100, 300]);

	test_ext.execute_with(|| {
		set_current_storage_weight(1000);
		// Benchmarked storage weight: 50
		let info = DispatchInfo { weight: Weight::from_parts(100, 50), ..Default::default() };

		// Actual weight is 25
		let post_info = PostDispatchInfo {
			actual_weight: Some(Weight::from_parts(50, 25)),
			pays_fee: Default::default(),
		};

		let (pre, _) = StorageWeightReclaim::<Test>(PhantomData)
			.validate_and_prepare(Some(ALICE.clone()).into(), CALL, &info, LEN)
			.unwrap();
		assert_eq!(pre, Some(100));

		// The `CheckWeight` extension will refunt `actual_weight` from `PostDispatchInfo`
		// we always need to call `post_dispatch` to verify that they interoperate correctly.
		assert_ok!(CheckWeight::<Test>::post_dispatch((), &info, &post_info, 0, &Ok(()), &()));
		assert_ok!(StorageWeightReclaim::<Test>::post_dispatch(
			pre,
			&info,
			&post_info,
			LEN,
			&Ok(()),
			&()
		));

		assert_eq!(BlockWeight::<Test>::get().total().proof_size(), 1150);
	})
}

#[test]
fn test_incorporates_check_weight_unspent_weight_reverse_order() {
	let mut test_ext = setup_test_externalities(&[100, 300]);

	test_ext.execute_with(|| {
		set_current_storage_weight(1000);

		// Benchmarked storage weight: 300
		let info = DispatchInfo { weight: Weight::from_parts(100, 300), ..Default::default() };

		// Actual weight is 50
		let post_info = PostDispatchInfo {
			actual_weight: Some(Weight::from_parts(50, 250)),
			pays_fee: Default::default(),
		};

		let (pre, _) = StorageWeightReclaim::<Test>(PhantomData)
			.validate_and_prepare(Some(ALICE.clone()).into(), CALL, &info, LEN)
			.unwrap();
		assert_eq!(pre, Some(100));

		assert_ok!(StorageWeightReclaim::<Test>::post_dispatch(
			pre,
			&info,
			&post_info,
			LEN,
			&Ok(()),
			&()
		));
		// `CheckWeight` gets called after `StorageWeightReclaim` this time.
		// The `CheckWeight` extension will refunt `actual_weight` from `PostDispatchInfo`
		// we always need to call `post_dispatch` to verify that they interoperate correctly.
		assert_ok!(CheckWeight::<Test>::post_dispatch((), &info, &post_info, 0, &Ok(()), &()));

		assert_eq!(BlockWeight::<Test>::get().total().proof_size(), 900);
	})
}

#[test]
fn test_incorporates_check_weight_unspent_weight_on_negative_reverse_order() {
	let mut test_ext = setup_test_externalities(&[100, 300]);

	test_ext.execute_with(|| {
		set_current_storage_weight(1000);
		// Benchmarked storage weight: 50
		let info = DispatchInfo { weight: Weight::from_parts(100, 50), ..Default::default() };

		// Actual weight is 25
		let post_info = PostDispatchInfo {
			actual_weight: Some(Weight::from_parts(50, 25)),
			pays_fee: Default::default(),
		};

		let (pre, _) = StorageWeightReclaim::<Test>(PhantomData)
			.validate_and_prepare(Some(ALICE.clone()).into(), CALL, &info, LEN)
			.unwrap();
		assert_eq!(pre, Some(100));

		assert_ok!(StorageWeightReclaim::<Test>::post_dispatch(
			pre,
			&info,
			&post_info,
			LEN,
			&Ok(()),
			&()
		));
		// `CheckWeight` gets called after `StorageWeightReclaim` this time.
		// The `CheckWeight` extension will refunt `actual_weight` from `PostDispatchInfo`
		// we always need to call `post_dispatch` to verify that they interoperate correctly.
		assert_ok!(CheckWeight::<Test>::post_dispatch((), &info, &post_info, 0, &Ok(()), &()));

		assert_eq!(BlockWeight::<Test>::get().total().proof_size(), 1150);
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
		assert_eq!(BlockWeight::<Test>::get().total().proof_size(), 10);
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
