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

#![cfg(test)]

use super::*;
use cumulus_primitives_proof_size_hostfunction::PROOF_RECORDING_DISABLED;
use frame_support::{
	assert_ok, derive_impl, dispatch::GetDispatchInfo, pallet_prelude::DispatchClass,
};
use sp_runtime::{
	generic,
	traits::{Applyable, BlakeTwo256, DispatchTransaction, Get},
	BuildStorage,
};
use sp_trie::proof_size_extension::ProofSizeExt;

thread_local! {
	static CHECK_WEIGHT_WEIGHT: core::cell::RefCell<Weight> = Default::default();
	static STORAGE_WEIGHT_RECLAIM_WEIGHT: core::cell::RefCell<Weight> = Default::default();
	static MOCK_EXT_WEIGHT: core::cell::RefCell<Weight> = Default::default();
	static MOCK_EXT_REFUND: core::cell::RefCell<Weight> = Default::default();
}

/// An extension which has some proof_size weight and some proof_size refund.
#[derive(
	Encode, Decode, DecodeWithMemTracking, Debug, Clone, PartialEq, Eq, scale_info::TypeInfo,
)]
pub struct MockExtensionWithRefund;

impl TransactionExtension<RuntimeCall> for MockExtensionWithRefund {
	const IDENTIFIER: &'static str = "mock_extension_with_refund";
	type Implicit = ();
	type Val = ();
	type Pre = ();
	fn weight(&self, _: &RuntimeCall) -> Weight {
		MOCK_EXT_WEIGHT.with_borrow(|v| *v)
	}
	fn post_dispatch_details(
		_pre: Self::Pre,
		_info: &DispatchInfoOf<RuntimeCall>,
		_post_info: &PostDispatchInfoOf<RuntimeCall>,
		_len: usize,
		_result: &DispatchResult,
	) -> Result<Weight, TransactionValidityError> {
		Ok(MOCK_EXT_REFUND.with_borrow(|v| *v))
	}
	fn bare_post_dispatch(
		_info: &DispatchInfoOf<RuntimeCall>,
		post_info: &mut PostDispatchInfoOf<RuntimeCall>,
		_len: usize,
		_result: &DispatchResult,
	) -> Result<(), TransactionValidityError> {
		if let Some(ref mut w) = post_info.actual_weight {
			*w -= MOCK_EXT_REFUND.with_borrow(|v| *v);
		}
		Ok(())
	}

	sp_runtime::impl_tx_ext_default!(RuntimeCall; validate prepare);
}

pub type Tx =
	crate::StorageWeightReclaim<Test, (frame_system::CheckWeight<Test>, MockExtensionWithRefund)>;
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
		RuntimeTask,
		RuntimeViewFunction
	)]
	pub struct Test;

	#[runtime::pallet_index(0)]
	pub type System = frame_system::Pallet<Test>;

	#[runtime::pallet_index(1)]
	pub type WeightReclaim = crate::Pallet<Test>;
}

pub struct MockWeightInfo;

impl frame_system::ExtensionsWeightInfo for MockWeightInfo {
	fn check_genesis() -> Weight {
		Default::default()
	}
	fn check_mortality_mortal_transaction() -> Weight {
		Default::default()
	}
	fn check_mortality_immortal_transaction() -> Weight {
		Default::default()
	}
	fn check_non_zero_sender() -> Weight {
		Default::default()
	}
	fn check_nonce() -> Weight {
		Default::default()
	}
	fn check_spec_version() -> Weight {
		Default::default()
	}
	fn check_tx_version() -> Weight {
		Default::default()
	}
	fn check_weight() -> Weight {
		CHECK_WEIGHT_WEIGHT.with_borrow(|v| *v)
	}
	fn weight_reclaim() -> Weight {
		Default::default()
	}
}

impl frame_system::WeightInfo for MockWeightInfo {
	fn remark(_b: u32) -> Weight {
		Weight::from_parts(400, 0)
	}
	fn set_code() -> Weight {
		Weight::zero()
	}
	fn set_storage(_i: u32) -> Weight {
		Weight::zero()
	}
	fn kill_prefix(_p: u32) -> Weight {
		Weight::zero()
	}
	fn kill_storage(_i: u32) -> Weight {
		Weight::zero()
	}
	fn set_heap_pages() -> Weight {
		Weight::zero()
	}
	fn remark_with_event(_b: u32) -> Weight {
		Weight::zero()
	}
	fn authorize_upgrade() -> Weight {
		Weight::zero()
	}
	fn apply_authorized_upgrade() -> Weight {
		Weight::zero()
	}
}

impl crate::WeightInfo for MockWeightInfo {
	fn storage_weight_reclaim() -> Weight {
		STORAGE_WEIGHT_RECLAIM_WEIGHT.with_borrow(|v| *v)
	}
}

#[derive_impl(frame_system::config_preludes::TestDefaultConfig)]
impl frame_system::Config for Test {
	type Block = Block;
	type AccountData = ();
	type MaxConsumers = frame_support::traits::ConstU32<3>;
	type ExtensionsWeightInfo = MockWeightInfo;
}

impl crate::Config for Test {
	type WeightInfo = MockWeightInfo;
}

fn new_test_ext() -> sp_io::TestExternalities {
	RuntimeGenesisConfig::default().build_storage().unwrap().into()
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

#[cfg(feature = "runtime-benchmarks")]
pub fn setup_test_ext_default() -> sp_io::TestExternalities {
	setup_test_externalities(&[0; 32])
}

fn set_current_storage_weight(new_weight: u64) {
	frame_system::BlockWeight::<Test>::mutate(|current_weight| {
		current_weight.set(Weight::from_parts(0, new_weight), DispatchClass::Normal);
	});
}

fn get_storage_weight() -> Weight {
	*frame_system::BlockWeight::<Test>::get().get(DispatchClass::Normal)
}

const CALL: &<Test as frame_system::Config>::RuntimeCall =
	&RuntimeCall::System(frame_system::Call::set_heap_pages { pages: 0u64 });
const ALICE_ORIGIN: frame_system::Origin<Test> = frame_system::Origin::<Test>::Signed(99);
const LEN: usize = 150;

fn new_tx_ext() -> Tx {
	Tx::new((frame_system::CheckWeight::new(), MockExtensionWithRefund))
}

fn new_extrinsic() -> generic::CheckedExtrinsic<AccountId, RuntimeCall, Tx> {
	generic::CheckedExtrinsic {
		format: generic::ExtrinsicFormat::Signed(99, new_tx_ext()),
		function: RuntimeCall::System(frame_system::Call::remark { remark: vec![] }),
	}
}

#[allow(unused)]
mod doc {
	type Runtime = super::Test;
	use crate::StorageWeightReclaim;

	#[docify::export(Tx)]
	type Tx = StorageWeightReclaim<
		Runtime,
		(
			frame_system::CheckNonce<Runtime>,
			frame_system::CheckWeight<Runtime>,
			// ... all other extensions
			// No need for `frame_system::WeightReclaim` as the reclaim.
		),
	>;
}

#[test]
fn basic_refund_no_post_info() {
	// The real cost will be 100 bytes of storage size
	let mut test_ext = setup_test_externalities(&[0, 100]);

	test_ext.execute_with(|| {
		set_current_storage_weight(1000);

		// Benchmarked storage weight: 500
		let info = DispatchInfo { call_weight: Weight::from_parts(0, 500), ..Default::default() };
		let mut post_info = PostDispatchInfo::default();

		let tx_ext = new_tx_ext();

		// Check weight should add 500 + 150 (len) to weight.
		let (pre, _) = tx_ext
			.validate_and_prepare(ALICE_ORIGIN.clone().into(), CALL, &info, LEN, 0)
			.unwrap();

		assert_eq!(pre.0, Some(0));

		assert_ok!(Tx::post_dispatch(pre, &info, &mut post_info, LEN, &Ok(())));

		assert_eq!(post_info.actual_weight, None);
		assert_eq!(get_storage_weight().proof_size(), 1250);
	});
}

#[test]
fn basic_refund_some_post_info() {
	// The real cost will be 100 bytes of storage size
	let mut test_ext = setup_test_externalities(&[0, 100]);

	test_ext.execute_with(|| {
		set_current_storage_weight(1000);

		// Benchmarked storage weight: 500
		let info = DispatchInfo { call_weight: Weight::from_parts(0, 500), ..Default::default() };
		let mut post_info = PostDispatchInfo::default();
		post_info.actual_weight = Some(info.total_weight());

		let tx_ext = new_tx_ext();

		// Check weight should add 500 + 150 (len) to weight.
		let (pre, _) = tx_ext
			.validate_and_prepare(ALICE_ORIGIN.clone().into(), CALL, &info, LEN, 0)
			.unwrap();

		assert_eq!(pre.0, Some(0));

		assert_ok!(Tx::post_dispatch(pre, &info, &mut post_info, LEN, &Ok(())));

		assert_eq!(post_info.actual_weight.unwrap(), Weight::from_parts(0, 100));
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
		let mut post_info = PostDispatchInfo::default();
		post_info.actual_weight = Some(info.total_weight());

		let tx_ext = new_tx_ext();

		// Check weight should add 500 + 150 (len) to weight.
		let (pre, _) = tx_ext
			.validate_and_prepare(ALICE_ORIGIN.clone().into(), CALL, &info, LEN, 0)
			.unwrap();

		assert_eq!(pre.0, None);

		assert_ok!(Tx::post_dispatch(pre, &info, &mut post_info, LEN, &Ok(())));

		assert_eq!(post_info.actual_weight.unwrap(), info.total_weight());
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
		let mut post_info = PostDispatchInfo::default();
		post_info.actual_weight = Some(info.total_weight());

		let tx_ext = new_tx_ext();

		// Weight added should be 100 + 150 (len)
		let (pre, _) = tx_ext
			.validate_and_prepare(ALICE_ORIGIN.clone().into(), CALL, &info, LEN, 0)
			.unwrap();

		assert_eq!(pre.0, Some(100));

		// We expect no refund
		assert_ok!(Tx::post_dispatch(pre, &info, &mut post_info, LEN, &Ok(())));

		assert_eq!(post_info.actual_weight.unwrap(), info.total_weight());
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
		let mut post_info = PostDispatchInfo::default();
		post_info.actual_weight = Some(info.total_weight());

		let tx_ext = new_tx_ext();

		let (pre, _) = tx_ext
			.validate_and_prepare(ALICE_ORIGIN.clone().into(), CALL, &info, LEN, 0)
			.unwrap();

		assert_eq!(pre.0, Some(0));

		assert_ok!(Tx::post_dispatch(pre, &info, &mut post_info, LEN, &Ok(())));

		assert_eq!(post_info.actual_weight.unwrap(), Weight::from_parts(0, 0));
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
		let mut post_info = PostDispatchInfo::default();
		post_info.actual_weight = Some(info.total_weight());

		let tx_ext = new_tx_ext();

		// Adds 500 + 150 (len) weight, total weight is 1950
		let (pre, _) = tx_ext
			.validate_and_prepare(ALICE_ORIGIN.clone().into(), CALL, &info, LEN, 0)
			.unwrap();

		assert_eq!(pre.0, Some(300));

		// check weight:
		// Refund 500 unspent weight according to `post_info`, total weight is now 1650
		//
		// storage reclaim:
		// Recorded proof size is negative -200, total weight is now 1450
		assert_ok!(Tx::post_dispatch(pre, &info, &mut post_info, LEN, &Ok(())));

		assert_eq!(post_info.actual_weight.unwrap(), Weight::from_parts(0, 0));
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
		let mut post_info = PostDispatchInfo {
			actual_weight: Some(Weight::from_parts(50, 250)),
			pays_fee: Default::default(),
		};

		let tx_ext = new_tx_ext();

		// Check weight should add 300 + 150 (len) of weight
		let (pre, _) = tx_ext
			.validate_and_prepare(ALICE_ORIGIN.clone().into(), CALL, &info, LEN, 0)
			.unwrap();

		assert_eq!(pre.0, Some(100));

		// The `CheckWeight` extension will refund `actual_weight` from `PostDispatchInfo`
		// we always need to call `post_dispatch` to verify that they interoperate correctly.
		assert_ok!(Tx::post_dispatch(pre, &info, &mut post_info, LEN, &Ok(())));

		assert_eq!(post_info.actual_weight.unwrap(), Weight::from_parts(50, 350 - LEN as u64));
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
		let mut post_info = PostDispatchInfo {
			actual_weight: Some(Weight::from_parts(50, 25)),
			pays_fee: Default::default(),
		};

		let tx_ext = new_tx_ext();

		// Adds 50 + 150 (len) weight, total weight 1200
		let (pre, _) = tx_ext
			.validate_and_prepare(ALICE_ORIGIN.clone().into(), CALL, &info, LEN, 0)
			.unwrap();
		assert_eq!(pre.0, Some(100));

		// The `CheckWeight` extension will refund `actual_weight` from `PostDispatchInfo`
		// CheckWeight: refunds unspent 25 weight according to `post_info`, 1175
		//
		// storage reclaim:
		// Adds 200 - 25 (unspent) == 175 weight, total weight 1350
		assert_ok!(Tx::post_dispatch(pre, &info, &mut post_info, LEN, &Ok(())));

		assert_eq!(post_info.actual_weight.unwrap(), Weight::from_parts(50, 25));
		assert_eq!(get_storage_weight().proof_size(), 1350);
	})
}

#[test]
fn test_nothing_reclaimed() {
	let mut test_ext = setup_test_externalities(&[0, 100]);

	test_ext.execute_with(|| {
		set_current_storage_weight(0);
		// Benchmarked storage weight: 100
		let info = DispatchInfo { call_weight: Weight::from_parts(100, 100), ..Default::default() };

		// Actual proof size is 100
		let mut post_info = PostDispatchInfo {
			actual_weight: Some(Weight::from_parts(50, 100)),
			pays_fee: Default::default(),
		};

		let tx_ext = new_tx_ext();

		// Adds benchmarked weight 100 + 150 (len), total weight is now 250
		let (pre, _) = tx_ext
			.validate_and_prepare(ALICE_ORIGIN.clone().into(), CALL, &info, LEN, 0)
			.unwrap();

		// Weight should go up by 150 len + 100 proof size weight, total weight 250
		assert_eq!(get_storage_weight().proof_size(), 250);

		// Should return `setup_test_externalities` proof recorder value: 100.
		assert_eq!(pre.0, Some(0));

		// The `CheckWeight` extension will refund `actual_weight` from `PostDispatchInfo`
		// we always need to call `post_dispatch` to verify that they interoperate correctly.
		// Nothing to refund, unspent is 0, total weight 250
		//
		// weight reclaim:
		// `setup_test_externalities` proof recorder value: 200, so this means the extrinsic
		// actually used 100 proof size.
		// Nothing to refund or add, weight matches proof recorder
		assert_ok!(Tx::post_dispatch(pre, &info, &mut post_info, LEN, &Ok(())));

		assert_eq!(post_info.actual_weight.unwrap(), Weight::from_parts(50, 100));
		// Check block len weight was not reclaimed:
		// 100 weight + 150 extrinsic len == 250 proof size
		assert_eq!(get_storage_weight().proof_size(), 250);
	})
}

// Test for refund of calls and related proof size
#[test]
fn test_series() {
	struct TestCfg {
		measured_proof_size_pre_dispatch: u64,
		measured_proof_size_post_dispatch: u64,
		info_call_weight: Weight,
		info_extension_weight: Weight,
		post_info_actual_weight: Option<Weight>,
		block_weight_pre_dispatch: Weight,
		mock_ext_refund: Weight,
		assert_post_info_weight: Option<Weight>,
		assert_block_weight_post_dispatch: Weight,
	}

	let base_extrinsic = <<Test as frame_system::Config>::BlockWeights as Get<
		frame_system::limits::BlockWeights,
	>>::get()
	.per_class
	.get(DispatchClass::Normal)
	.base_extrinsic;

	let tests = vec![
		// Info is exact, no post info, no refund.
		TestCfg {
			measured_proof_size_pre_dispatch: 100,
			measured_proof_size_post_dispatch: 400,
			info_call_weight: Weight::from_parts(40, 100),
			info_extension_weight: Weight::from_parts(60, 200),
			post_info_actual_weight: None,
			block_weight_pre_dispatch: Weight::from_parts(1000, 1000),
			mock_ext_refund: Weight::from_parts(0, 0),
			assert_post_info_weight: None,
			assert_block_weight_post_dispatch: base_extrinsic +
				Weight::from_parts(1100, 1300 + LEN as u64),
		},
		// some tx ext refund is ignored, because post info is None.
		TestCfg {
			measured_proof_size_pre_dispatch: 100,
			measured_proof_size_post_dispatch: 400,
			info_call_weight: Weight::from_parts(40, 100),
			info_extension_weight: Weight::from_parts(60, 200),
			post_info_actual_weight: None,
			block_weight_pre_dispatch: Weight::from_parts(1000, 1000),
			mock_ext_refund: Weight::from_parts(20, 20),
			assert_post_info_weight: None,
			assert_block_weight_post_dispatch: base_extrinsic +
				Weight::from_parts(1100, 1300 + LEN as u64),
		},
		// some tx ext refund is ignored on proof size because lower than actual measure.
		TestCfg {
			measured_proof_size_pre_dispatch: 100,
			measured_proof_size_post_dispatch: 400,
			info_call_weight: Weight::from_parts(40, 100),
			info_extension_weight: Weight::from_parts(60, 200),
			post_info_actual_weight: Some(Weight::from_parts(100, 300)),
			block_weight_pre_dispatch: Weight::from_parts(1000, 1000),
			mock_ext_refund: Weight::from_parts(20, 20),
			assert_post_info_weight: Some(Weight::from_parts(80, 300)),
			assert_block_weight_post_dispatch: base_extrinsic +
				Weight::from_parts(1080, 1300 + LEN as u64),
		},
		// post info doesn't double refund the call and is missing some.
		TestCfg {
			measured_proof_size_pre_dispatch: 100,
			measured_proof_size_post_dispatch: 350,
			info_call_weight: Weight::from_parts(40, 100),
			info_extension_weight: Weight::from_parts(60, 200),
			post_info_actual_weight: Some(Weight::from_parts(60, 200)),
			block_weight_pre_dispatch: Weight::from_parts(1000, 1000),
			mock_ext_refund: Weight::from_parts(20, 20),
			// 50 are missed in pov because 100 is unspent in post info but it should be only 50.
			assert_post_info_weight: Some(Weight::from_parts(40, 200)),
			assert_block_weight_post_dispatch: base_extrinsic +
				Weight::from_parts(1040, 1250 + LEN as u64),
		},
		// post info doesn't double refund the call and is accurate.
		TestCfg {
			measured_proof_size_pre_dispatch: 100,
			measured_proof_size_post_dispatch: 250,
			info_call_weight: Weight::from_parts(40, 100),
			info_extension_weight: Weight::from_parts(60, 200),
			post_info_actual_weight: Some(Weight::from_parts(60, 200)),
			block_weight_pre_dispatch: Weight::from_parts(1000, 1000),
			mock_ext_refund: Weight::from_parts(20, 20),
			assert_post_info_weight: Some(Weight::from_parts(40, 150)),
			assert_block_weight_post_dispatch: base_extrinsic +
				Weight::from_parts(1040, 1150 + LEN as u64),
		},
		// post info doesn't double refund the call and is accurate. Even if mock ext is refunding
		// too much.
		TestCfg {
			measured_proof_size_pre_dispatch: 100,
			measured_proof_size_post_dispatch: 250,
			info_call_weight: Weight::from_parts(40, 100),
			info_extension_weight: Weight::from_parts(60, 200),
			post_info_actual_weight: Some(Weight::from_parts(60, 200)),
			block_weight_pre_dispatch: Weight::from_parts(1000, 1000),
			mock_ext_refund: Weight::from_parts(20, 300),
			assert_post_info_weight: Some(Weight::from_parts(40, 150)),
			assert_block_weight_post_dispatch: base_extrinsic +
				Weight::from_parts(1040, 1150 + LEN as u64),
		},
	];

	for (i, test) in tests.into_iter().enumerate() {
		dbg!("test number: ", i);
		MOCK_EXT_REFUND.with_borrow_mut(|v| *v = test.mock_ext_refund);
		let mut test_ext = setup_test_externalities(&[
			test.measured_proof_size_pre_dispatch as usize,
			test.measured_proof_size_post_dispatch as usize,
		]);

		test_ext.execute_with(|| {
			frame_system::BlockWeight::<Test>::mutate(|current_weight| {
				current_weight.set(test.block_weight_pre_dispatch, DispatchClass::Normal);
			});
			// Benchmarked storage weight: 50
			let info = DispatchInfo {
				call_weight: test.info_call_weight,
				extension_weight: test.info_extension_weight,
				..Default::default()
			};
			let mut post_info = PostDispatchInfo {
				actual_weight: test.post_info_actual_weight,
				pays_fee: Default::default(),
			};
			let tx_ext = new_tx_ext();
			let (pre, _) = tx_ext
				.validate_and_prepare(ALICE_ORIGIN.clone().into(), CALL, &info, LEN, 0)
				.unwrap();
			assert_ok!(Tx::post_dispatch(pre, &info, &mut post_info, LEN, &Ok(())));

			assert_eq!(post_info.actual_weight, test.assert_post_info_weight);
			assert_eq!(
				*frame_system::BlockWeight::<Test>::get().get(DispatchClass::Normal),
				test.assert_block_weight_post_dispatch,
			);
		})
	}
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
fn full_basic_refund() {
	// Settings for the test:
	let actual_used_proof_size = 200;
	let check_weight = 100;
	let storage_weight_reclaim = 100;
	let mock_ext = 142;
	let mock_ext_refund = 100;

	// Test execution:
	CHECK_WEIGHT_WEIGHT.with_borrow_mut(|v| *v = Weight::from_parts(1, check_weight));
	STORAGE_WEIGHT_RECLAIM_WEIGHT
		.with_borrow_mut(|v| *v = Weight::from_parts(1, storage_weight_reclaim));
	MOCK_EXT_WEIGHT.with_borrow_mut(|v| *v = Weight::from_parts(36, mock_ext));
	MOCK_EXT_REFUND.with_borrow_mut(|v| *v = Weight::from_parts(35, mock_ext_refund));

	let initial_storage_weight = 1212u64;

	let mut test_ext = setup_test_externalities(&[
		initial_storage_weight as usize,
		initial_storage_weight as usize + actual_used_proof_size,
	]);

	test_ext.execute_with(|| {
		set_current_storage_weight(initial_storage_weight);

		let extrinsic = new_extrinsic();
		let call_info = extrinsic.function.get_dispatch_info();

		let info = extrinsic.get_dispatch_info();
		let post_info = extrinsic.apply::<Test>(&info, LEN).unwrap().unwrap();

		// Assertions:
		assert_eq!(
			post_info.actual_weight.unwrap().ref_time(),
			call_info.call_weight.ref_time() + 3,
		);
		assert_eq!(
			post_info.actual_weight.unwrap().proof_size(),
			// LEN is part of the base extrinsic, not the post info weight actual weight.
			actual_used_proof_size as u64,
		);
		assert_eq!(
			get_storage_weight().proof_size(),
			initial_storage_weight + actual_used_proof_size as u64 + LEN as u64
		);
	});
}

#[test]
fn full_accrue() {
	// Settings for the test:
	let actual_used_proof_size = 400;
	let check_weight = 100;
	let storage_weight_reclaim = 100;
	let mock_ext = 142;
	let mock_ext_refund = 100;

	// Test execution:
	CHECK_WEIGHT_WEIGHT.with_borrow_mut(|v| *v = Weight::from_parts(1, check_weight));
	STORAGE_WEIGHT_RECLAIM_WEIGHT
		.with_borrow_mut(|v| *v = Weight::from_parts(1, storage_weight_reclaim));
	MOCK_EXT_WEIGHT.with_borrow_mut(|v| *v = Weight::from_parts(36, mock_ext));
	MOCK_EXT_REFUND.with_borrow_mut(|v| *v = Weight::from_parts(35, mock_ext_refund));

	let initial_storage_weight = 1212u64;

	let mut test_ext = setup_test_externalities(&[
		initial_storage_weight as usize,
		initial_storage_weight as usize + actual_used_proof_size,
	]);

	test_ext.execute_with(|| {
		set_current_storage_weight(initial_storage_weight);

		let extrinsic = new_extrinsic();
		let call_info = extrinsic.function.get_dispatch_info();

		let info = extrinsic.get_dispatch_info();
		let post_info = extrinsic.apply::<Test>(&info, LEN).unwrap().unwrap();

		// Assertions:
		assert_eq!(
			post_info.actual_weight.unwrap().ref_time(),
			call_info.call_weight.ref_time() + 3,
		);
		assert_eq!(
			post_info.actual_weight.unwrap().proof_size(),
			info.total_weight().proof_size(), // The post info doesn't get the accrue.
		);
		assert_eq!(
			get_storage_weight().proof_size(),
			initial_storage_weight + actual_used_proof_size as u64 + LEN as u64
		);
	});
}

#[test]
fn bare_is_reclaimed() {
	let mut test_ext = setup_test_externalities(&[]);
	test_ext.execute_with(|| {
		let info = DispatchInfo {
			call_weight: Weight::from_parts(100, 100),
			extension_weight: Weight::from_parts(100, 100),
			class: DispatchClass::Normal,
			pays_fee: Default::default(),
		};
		let mut post_info = PostDispatchInfo {
			actual_weight: Some(Weight::from_parts(100, 100)),
			pays_fee: Default::default(),
		};
		MOCK_EXT_REFUND.with_borrow_mut(|v| *v = Weight::from_parts(10, 10));

		frame_system::BlockWeight::<Test>::mutate(|current_weight| {
			current_weight
				.set(Weight::from_parts(45, 45) + info.total_weight(), DispatchClass::Normal);
		});

		StorageWeightReclaim::<Test, MockExtensionWithRefund>::bare_post_dispatch(
			&info,
			&mut post_info,
			0,
			&Ok(()),
		)
		.expect("tx is valid");

		assert_eq!(
			*frame_system::BlockWeight::<Test>::get().get(DispatchClass::Normal),
			Weight::from_parts(45 + 90, 45 + 90),
		);
	});
}

#[test]
fn sets_to_node_storage_proof_if_higher() {
	struct TestCfg {
		initial_proof_size: u64,
		post_dispatch_proof_size: u64,
		mock_ext_proof_size: u64,
		pre_dispatch_block_proof_size: u64,
		assert_final_block_proof_size: u64,
	}

	let tests = vec![
		// The storage proof reported by the proof recorder is higher than what is stored on
		// the runtime side.
		TestCfg {
			initial_proof_size: 1000,
			post_dispatch_proof_size: 1005,
			mock_ext_proof_size: 0,
			pre_dispatch_block_proof_size: 5,
			// We expect that the storage weight was set to the node-side proof size (1005) +
			// extrinsics length (150)
			assert_final_block_proof_size: 1155,
		},
		// In this second scenario the proof size on the node side is only lower
		// after reclaim happened.
		TestCfg {
			initial_proof_size: 175,
			post_dispatch_proof_size: 180,
			mock_ext_proof_size: 100,
			pre_dispatch_block_proof_size: 85,
			// After the pre_dispatch, the BlockWeight proof size will be
			// 85 (initial) + 100 (benched) + 150 (tx length) = 335
			//
			// We expect that the storage weight was set to the node-side proof weight
			// First we will reclaim 95, which leaves us with 240 BlockWeight.
			// This is lower than 180 (proof size hf) + 150 (length).
			// So we expect it to be set to 330.
			assert_final_block_proof_size: 330,
		},
	];

	for test in tests {
		let mut test_ext = setup_test_externalities(&[
			test.initial_proof_size as usize,
			test.post_dispatch_proof_size as usize,
		]);

		CHECK_WEIGHT_WEIGHT.with_borrow_mut(|v| *v = Weight::from_parts(0, 0));
		STORAGE_WEIGHT_RECLAIM_WEIGHT.with_borrow_mut(|v| *v = Weight::from_parts(0, 0));
		MOCK_EXT_WEIGHT.with_borrow_mut(|v| *v = Weight::from_parts(0, test.mock_ext_proof_size));

		test_ext.execute_with(|| {
			set_current_storage_weight(test.pre_dispatch_block_proof_size);

			let extrinsic = new_extrinsic();
			let call_info = extrinsic.function.get_dispatch_info();
			assert_eq!(call_info.call_weight.proof_size(), 0);

			let info = extrinsic.get_dispatch_info();
			let _post_info = extrinsic.apply::<Test>(&info, LEN).unwrap().unwrap();

			assert_eq!(get_storage_weight().proof_size(), test.assert_final_block_proof_size);
		})
	}
}

#[test]
fn test_pov_missing_from_node_reclaim() {
	// Test scenario: after dispatch the pov size from node side is less than block weight.
	// Ensure `pov_size_missing_from_node` is calculated correctly, and `ExtrinsicWeightReclaimed`
	// is updated correctly.

	// Proof size:
	let bench_pre_dispatch_call = 220;
	let bench_post_dispatch_actual = 90;
	let len = 20; // Only one extrinsic in the scenario. So all extrinsics length.
	let block_pre_dispatch = 100;
	let missing_from_node = 50;
	let node_diff = 70;

	let node_pre_dispatch = block_pre_dispatch + missing_from_node;
	let node_post_dispatch = node_pre_dispatch + node_diff;

	// Initialize the test.
	let mut test_ext =
		setup_test_externalities(&[node_pre_dispatch as usize, node_post_dispatch as usize]);

	test_ext.execute_with(|| {
		set_current_storage_weight(block_pre_dispatch);
		let info = DispatchInfo {
			call_weight: Weight::from_parts(0, bench_pre_dispatch_call),
			extension_weight: Weight::from_parts(0, 0),
			..Default::default()
		};
		let post_info = PostDispatchInfo {
			actual_weight: Some(Weight::from_parts(0, bench_post_dispatch_actual)),
			..Default::default()
		};

		// Execute the transaction.
		let tx_ext = StorageWeightReclaim::<Test, frame_system::CheckWeight<Test>>::new(
			frame_system::CheckWeight::new(),
		);
		tx_ext
			.test_run(ALICE_ORIGIN.clone().into(), CALL, &info, len as usize, 0, |_| Ok(post_info))
			.expect("valid")
			.expect("success");

		// Assert the results.
		assert_eq!(
			frame_system::BlockWeight::<Test>::get().get(DispatchClass::Normal).proof_size(),
			node_post_dispatch + len,
		);
		assert_eq!(
			frame_system::ExtrinsicWeightReclaimed::<Test>::get().proof_size(),
			bench_pre_dispatch_call - node_diff - missing_from_node,
		);
	});
}

#[test]
fn test_ref_time_weight_reclaim() {
	// Test scenario: after dispatch the time weight is refunded correctly.

	// Time weight:
	let bench_pre_dispatch_call = 145;
	let bench_post_dispatch_actual = 104;
	let bench_mock_ext_weight = 63;
	let bench_mock_ext_refund = 22;
	let len = 20; // Only one extrinsic in the scenario. So all extrinsics length.
	let block_pre_dispatch = 121;
	let node_pre_dispatch = 0;
	let node_post_dispatch = 0;

	// Initialize the test.
	CHECK_WEIGHT_WEIGHT.with_borrow_mut(|v| *v = Weight::from_parts(0, 0));
	STORAGE_WEIGHT_RECLAIM_WEIGHT.with_borrow_mut(|v| *v = Weight::from_parts(0, 0));
	MOCK_EXT_WEIGHT.with_borrow_mut(|v| *v = Weight::from_parts(bench_mock_ext_weight, 0));
	MOCK_EXT_REFUND.with_borrow_mut(|v| *v = Weight::from_parts(bench_mock_ext_refund, 0));

	let base_extrinsic = <<Test as frame_system::Config>::BlockWeights as Get<
		frame_system::limits::BlockWeights,
	>>::get()
	.per_class
	.get(DispatchClass::Normal)
	.base_extrinsic;

	let mut test_ext =
		setup_test_externalities(&[node_pre_dispatch as usize, node_post_dispatch as usize]);

	test_ext.execute_with(|| {
		frame_system::BlockWeight::<Test>::mutate(|current_weight| {
			current_weight.set(Weight::from_parts(block_pre_dispatch, 0), DispatchClass::Normal);
		});
		let info = DispatchInfo {
			call_weight: Weight::from_parts(bench_pre_dispatch_call, 0),
			extension_weight: Weight::from_parts(bench_mock_ext_weight, 0),
			..Default::default()
		};
		let post_info = PostDispatchInfo {
			actual_weight: Some(Weight::from_parts(bench_post_dispatch_actual, 0)),
			..Default::default()
		};

		type InnerTxExt = (frame_system::CheckWeight<Test>, MockExtensionWithRefund);
		// Execute the transaction.
		let tx_ext = StorageWeightReclaim::<Test, InnerTxExt>::new((
			frame_system::CheckWeight::new(),
			MockExtensionWithRefund,
		));
		tx_ext
			.test_run(ALICE_ORIGIN.clone().into(), CALL, &info, len as usize, 0, |_| Ok(post_info))
			.expect("valid transaction extension pipeline")
			.expect("success");

		// Assert the results.
		assert_eq!(
			frame_system::BlockWeight::<Test>::get().get(DispatchClass::Normal).ref_time(),
			block_pre_dispatch +
				base_extrinsic.ref_time() +
				bench_post_dispatch_actual +
				bench_mock_ext_weight -
				bench_mock_ext_refund,
		);
		assert_eq!(
			frame_system::ExtrinsicWeightReclaimed::<Test>::get().ref_time(),
			bench_pre_dispatch_call - bench_post_dispatch_actual + bench_mock_ext_refund,
		);
	});
}

#[test]
fn test_metadata() {
	assert_eq!(
		StorageWeightReclaim::<Test, frame_system::CheckWeight<Test>>::metadata()
			.iter()
			.map(|m| m.identifier)
			.collect::<Vec<_>>(),
		vec!["CheckWeight", "StorageWeightReclaim"]
	);
}
