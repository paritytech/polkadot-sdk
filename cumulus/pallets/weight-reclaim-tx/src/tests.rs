// This file is part of Substrate.

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

#![cfg(test)]

use super::*;
use cumulus_primitives_proof_size_hostfunction::PROOF_RECORDING_DISABLED;
use frame_support::{assert_ok, derive_impl, pallet_prelude::DispatchClass};
use sp_runtime::{
	generic,
	traits::{BlakeTwo256, DispatchTransaction},
	BuildStorage,
};
use sp_trie::proof_size_extension::ProofSizeExt;

pub type Tx = crate::StorageWeightReclaim<Test, frame_system::CheckWeight<Test>>;
type AccountId = u64;
type Extrinsic = generic::UncheckedExtrinsic<AccountId, RuntimeCall, (), Tx>;
type Block = generic::Block<generic::Header<AccountId, BlakeTwo256>, Extrinsic>;

#[frame_support::runtime]
mod runtime {
	#[runtime::runtime]
	#[runtime::derive(
		RuntimeCall,
		RuntimeEvent,
		RuntimeError,
		RuntimeOrigin,
		RuntimeFreezeReason,
		RuntimeHoldReason,
		RuntimeSlashReason,
		RuntimeLockId,
		RuntimeTask
	)]
	pub struct Test;

	#[runtime::pallet_index(0)]
	pub type System = frame_system::Pallet<Test>;

	#[runtime::pallet_index(1)]
	pub type WeightReclaimTx = crate::Pallet<Test>;
}

#[derive_impl(frame_system::config_preludes::TestDefaultConfig)]
impl frame_system::Config for Test {
	type Block = Block;
	type AccountData = ();
	type MaxConsumers = frame_support::traits::ConstU32<3>;
}

impl crate::Config for Test {
	type WeightInfo = ();
}

pub fn new_test_ext() -> sp_io::TestExternalities {
	RuntimeGenesisConfig::default().build_storage().unwrap().into()
}

pub struct TestRecorder {
	return_values: Box<[usize]>,
	counter: core::sync::atomic::AtomicUsize,
}

impl TestRecorder {
	pub fn new(values: &[usize]) -> Self {
		TestRecorder { return_values: values.into(), counter: Default::default() }
	}
}

impl sp_trie::ProofSizeProvider for TestRecorder {
	fn estimate_encoded_size(&self) -> usize {
		let counter = self.counter.fetch_add(1, core::sync::atomic::Ordering::Relaxed);
		self.return_values[counter]
	}
}

pub fn setup_test_externalities(proof_values: &[usize]) -> sp_io::TestExternalities {
	let mut test_ext = new_test_ext();
	let test_recorder = TestRecorder::new(proof_values);
	test_ext.register_extension(ProofSizeExt::new(test_recorder));
	test_ext
}

pub fn setup_test_ext_default() -> sp_io::TestExternalities {
	setup_test_externalities(&[0; 32])
}

pub fn set_current_storage_weight(new_weight: u64) {
	frame_system::BlockWeight::<Test>::mutate(|current_weight| {
		current_weight.set(Weight::from_parts(0, new_weight), DispatchClass::Normal);
	});
}

pub fn get_storage_weight() -> Weight {
	frame_system::BlockWeight::<Test>::get().get(DispatchClass::Normal).clone()
}

const CALL: &<Test as frame_system::Config>::RuntimeCall =
	&RuntimeCall::System(frame_system::Call::set_heap_pages { pages: 0u64 });
const ALICE_ORIGIN: frame_system::Origin<Test> = frame_system::Origin::<Test>::Signed(99);
const LEN: usize = 150;

mod doc {
	type Runtime = super::Test;
	use crate::StorageWeightReclaim;
	use Tx as _;

	#[docify::export(Tx)]
	type Tx = StorageWeightReclaim<
		Runtime,
		(
			frame_system::CheckNonce<Runtime>,
			frame_system::CheckWeight<Runtime>,
			// ... all other extensions
		),
	>;
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

		let tx = Tx::new(frame_system::CheckWeight::new());

		// Check weight should add 500 + 150 (len) to weight.
		let (pre, _) =
			tx.validate_and_prepare(ALICE_ORIGIN.clone().into(), CALL, &info, LEN).unwrap();

		assert_eq!(pre.0, Some(0));

		assert_ok!(Tx::post_dispatch_details(pre, &info, &post_info, LEN, &Ok(()), &()));

		assert_eq!(get_storage_weight().proof_size(), 1250);
	});
}

#[test]
fn does_nothing_without_extension() {
	// Proof size extension not registered
	let mut test_ext = new_test_ext();

	test_ext.execute_with(|| {
		set_current_storage_weight(1000);

		// Benchmarked storage weight: 500
		let info = DispatchInfo { call_weight: Weight::from_parts(0, 500), ..Default::default() };
		let post_info = PostDispatchInfo::default();

		let tx = Tx::new(frame_system::CheckWeight::new());

		// Check weight should add 500 + 150 (len) to weight.
		let (pre, _) =
			tx.validate_and_prepare(ALICE_ORIGIN.clone().into(), CALL, &info, LEN).unwrap();

		assert_eq!(pre.0, None);

		assert_ok!(Tx::post_dispatch_details(pre, &info, &post_info, LEN, &Ok(()), &()));

		assert_eq!(get_storage_weight().proof_size(), 1650);
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

		let tx = Tx::new(frame_system::CheckWeight::new());

		// Weight added should be 100 + 150 (len)
		let (pre, _) =
			tx.validate_and_prepare(ALICE_ORIGIN.clone().into(), CALL, &info, LEN).unwrap();

		assert_eq!(pre.0, Some(100));

		// We expect no refund
		assert_ok!(Tx::post_dispatch_details(pre, &info, &post_info, LEN, &Ok(()), &()));

		assert_eq!(
			get_storage_weight().proof_size(),
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

		let tx = Tx::new(frame_system::CheckWeight::new());

		let (pre, _) =
			tx.validate_and_prepare(ALICE_ORIGIN.clone().into(), CALL, &info, LEN).unwrap();

		assert_eq!(pre.0, Some(0));

		assert_ok!(Tx::post_dispatch_details(pre, &info, &post_info, LEN, &Ok(()), &()));

		// Proof size should be exactly equal to extrinsic length
		assert_eq!(get_storage_weight().proof_size(), LEN as u64);
	});
}

#[test]
fn test_larger_pre_dispatch_proof_size() {
	let mut test_ext = setup_test_externalities(&[300, 100]);

	test_ext.execute_with(|| {
		set_current_storage_weight(1300);

		let info = DispatchInfo { call_weight: Weight::from_parts(0, 500), ..Default::default() };
		let post_info = PostDispatchInfo::default();

		let tx = Tx::new(frame_system::CheckWeight::new());

		// Adds 500 + 150 (len) weight, total weight is 1950
		let (pre, _) =
			tx.validate_and_prepare(ALICE_ORIGIN.clone().into(), CALL, &info, LEN).unwrap();

		assert_eq!(pre.0, Some(300));

		// check weight:
		// Refund 500 unspent weight according to `post_info`, total weight is now 1650
		//
		// storage reclaim:
		// Recorded proof size is negative -200, total weight is now 1450
		assert_ok!(Tx::post_dispatch_details(pre, &info, &post_info, LEN, &Ok(()), &()));

		assert_eq!(get_storage_weight().proof_size(), 1450);
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

		let tx = Tx::new(frame_system::CheckWeight::new());

		// Check weight should add 300 + 150 (len) of weight
		let (pre, _) =
			tx.validate_and_prepare(ALICE_ORIGIN.clone().into(), CALL, &info, LEN).unwrap();

		assert_eq!(pre.0, Some(100));

		// The `CheckWeight` extension will refunt `actual_weight` from `PostDispatchInfo`
		// we always need to call `post_dispatch` to verify that they interoperate correctly.
		assert_ok!(Tx::post_dispatch_details(pre, &info, &post_info, LEN, &Ok(()), &()));

		// Reclaimed 100
		assert_eq!(get_storage_weight().proof_size(), 1350);
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

		let tx = Tx::new(frame_system::CheckWeight::new());

		// Adds 50 + 150 (len) weight, total weight 1200
		let (pre, _) =
			tx.validate_and_prepare(ALICE_ORIGIN.clone().into(), CALL, &info, LEN).unwrap();
		assert_eq!(pre.0, Some(100));

		// The `CheckWeight` extension will refund `actual_weight` from `PostDispatchInfo`
		// we always need to call `post_dispatch` to verify that they interoperate correctly.
		// Refunds unspent 25 weight according to `post_info`, 1175
		//
		// storage reclaim:
		// Adds 200 - 25 (unspent) == 175 weight, total weight 1350
		assert_ok!(Tx::post_dispatch_details(pre, &info, &post_info, LEN, &Ok(()), &()));

		assert_eq!(get_storage_weight().proof_size(), 1350);
	})
}

#[test]
fn test_nothing_reclaimed() {
	let mut test_ext = setup_test_externalities(&[100, 200]);

	test_ext.execute_with(|| {
		set_current_storage_weight(0);
		// Benchmarked storage weight: 100
		let info = DispatchInfo { call_weight: Weight::from_parts(100, 100), ..Default::default() };

		// Actual proof size is 100
		let post_info = PostDispatchInfo {
			actual_weight: Some(Weight::from_parts(50, 100)),
			pays_fee: Default::default(),
		};

		let tx = Tx::new(frame_system::CheckWeight::new());

		// Adds benchmarked weight 100 + 150 (len), total weight is now 250
		let (pre, _) =
			tx.validate_and_prepare(ALICE_ORIGIN.clone().into(), CALL, &info, LEN).unwrap();

		// Weight should go up by 150 len + 100 proof size weight, total weight 250
		assert_eq!(get_storage_weight().proof_size(), 250);

		// Should return `setup_test_externalities` proof recorder value: 100.
		assert_eq!(pre.0, Some(100));

		// The `CheckWeight` extension will refund `actual_weight` from `PostDispatchInfo`
		// we always need to call `post_dispatch` to verify that they interoperate correctly.
		// Nothing to refund, unspent is 0, total weight 250
		//
		// weight reclaim:
		// `setup_test_externalities` proof recorder value: 200, so this means the extrinsic
		// actually used 100 proof size.
		// Nothing to refund or add, weight matches proof recorder
		assert_ok!(Tx::post_dispatch_details(pre, &info, &post_info, LEN, &Ok(()), &()));

		// Check block len weight was not reclaimed:
		// 100 weight + 150 extrinsic len == 250 proof size
		assert_eq!(get_storage_weight().proof_size(), 250);
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

		type WrongTx = (crate::StorageWeightReclaim<Test, ()>, frame_system::CheckWeight<Test>);
		let tx: WrongTx = (
			crate::StorageWeightReclaim::new(()),
			frame_system::CheckWeight::new(),
		);

		// Adds 300 + 150 (len) weight, total weight 1450
		let (pre, _) =
			tx.validate_and_prepare(ALICE_ORIGIN.clone().into(), CALL, &info, LEN).unwrap();

		assert_eq!(pre.0.0, Some(100));

		// storage weight reclaim:
		// This refunds 100 - 50(unspent), total weight is now 1400
		//
		// `CheckWeight` gets called after `StorageWeightReclaim` this time.
		// The `CheckWeight` extension will refund `actual_weight` from `PostDispatchInfo`
		// we always need to call `post_dispatch` to verify that they interoperate correctly.
		assert_ok!(WrongTx::post_dispatch_details(pre, &info, &post_info, LEN, &Ok(()), &()));

		// Above call refunds 50 (unspent), total weight is 1350 now
		assert_eq!(get_storage_weight().proof_size(), 1350);
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

		type WrongTx = (crate::StorageWeightReclaim<Test, ()>, frame_system::CheckWeight<Test>);
		let tx: WrongTx = (
			crate::StorageWeightReclaim::new(()),
			frame_system::CheckWeight::new(),
		);

		// Adds 50 + 150 (len) weight, total weight is 1200
		let (pre, _) =
			tx.validate_and_prepare(ALICE_ORIGIN.clone().into(), CALL, &info, LEN).unwrap();

		assert_eq!(pre.0.0, Some(100));

		// storage weight reclaim:
		// Adds additional 150 weight recorded
		//
		// check weight:
		// `CheckWeight` gets called after `StorageWeightReclaim` this time.
		// The `CheckWeight` extension will refunt `actual_weight` from `PostDispatchInfo`
		// we always need to call `post_dispatch` to verify that they interoperate correctly.
		assert_ok!(WrongTx::post_dispatch_details(pre, &info, &post_info, LEN, &Ok(()), &()));

		assert_eq!(get_storage_weight().proof_size(), 1350);
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
