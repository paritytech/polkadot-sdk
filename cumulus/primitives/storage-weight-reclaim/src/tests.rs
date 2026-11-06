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
	dispatch::{DispatchClass, PerDispatchClass},
	weights::{Weight, WeightMeter},
};
use frame_system::BlockWeight;
use sp_runtime::BuildStorage;
use sp_trie::proof_size_extension::ProofSizeExt;

type Test = cumulus_test_runtime::Runtime;

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
