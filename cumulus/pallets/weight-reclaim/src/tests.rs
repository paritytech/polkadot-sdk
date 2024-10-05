// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Cumulus.

// Cumulus is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Cumulus is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Cumulus.  If not, see <http://www.gnu.org/licenses/>.

#![cfg(test)]

use super::*;
use cumulus_primitives_proof_size_hostfunction::PROOF_RECORDING_DISABLED;
use frame_support::{
	assert_ok, derive_impl, dispatch::GetDispatchInfo, pallet_prelude::DispatchClass,
};
use sp_runtime::{
	generic,
	traits::{Applyable, BlakeTwo256, DispatchTransaction},
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
#[derive(Encode, Decode, Debug, Clone, PartialEq, Eq, scale_info::TypeInfo)]
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

	sp_runtime::impl_tx_ext_default!(RuntimeCall; validate prepare);
}

pub type Tx =
	crate::StorageWeightReclaim<Test, (MockExtensionWithRefund, frame_system::CheckWeight<Test>)>;
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
	Tx::new((MockExtensionWithRefund, frame_system::CheckWeight::new()))
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
		let mut post_info = PostDispatchInfo::default();

		let tx_ext = new_tx_ext();

		// Check weight should add 500 + 150 (len) to weight.
		let (pre, _) = tx_ext
			.validate_and_prepare(ALICE_ORIGIN.clone().into(), CALL, &info, LEN)
			.unwrap();

		assert_eq!(pre.0, Some(0));

		assert_ok!(Tx::post_dispatch(pre, &info, &mut post_info, LEN, &Ok(())));

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

		let tx_ext = new_tx_ext();

		// Check weight should add 500 + 150 (len) to weight.
		let (pre, _) = tx_ext
			.validate_and_prepare(ALICE_ORIGIN.clone().into(), CALL, &info, LEN)
			.unwrap();

		assert_eq!(pre.0, None);

		assert_ok!(Tx::post_dispatch(pre, &info, &mut post_info, LEN, &Ok(())));

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

		let tx_ext = new_tx_ext();

		// Weight added should be 100 + 150 (len)
		let (pre, _) = tx_ext
			.validate_and_prepare(ALICE_ORIGIN.clone().into(), CALL, &info, LEN)
			.unwrap();

		assert_eq!(pre.0, Some(100));

		// We expect no refund
		assert_ok!(Tx::post_dispatch(pre, &info, &mut post_info, LEN, &Ok(())));

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

		let tx_ext = new_tx_ext();

		let (pre, _) = tx_ext
			.validate_and_prepare(ALICE_ORIGIN.clone().into(), CALL, &info, LEN)
			.unwrap();

		assert_eq!(pre.0, Some(0));

		assert_ok!(Tx::post_dispatch(pre, &info, &mut post_info, LEN, &Ok(())));

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

		let tx_ext = new_tx_ext();

		// Adds 500 + 150 (len) weight, total weight is 1950
		let (pre, _) = tx_ext
			.validate_and_prepare(ALICE_ORIGIN.clone().into(), CALL, &info, LEN)
			.unwrap();

		assert_eq!(pre.0, Some(300));

		// check weight:
		// Refund 500 unspent weight according to `post_info`, total weight is now 1650
		//
		// storage reclaim:
		// Recorded proof size is negative -200, total weight is now 1450
		assert_ok!(Tx::post_dispatch(pre, &info, &mut post_info, LEN, &Ok(())));

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
			.validate_and_prepare(ALICE_ORIGIN.clone().into(), CALL, &info, LEN)
			.unwrap();

		assert_eq!(pre.0, Some(100));

		// The `CheckWeight` extension will refund `actual_weight` from `PostDispatchInfo`
		// we always need to call `post_dispatch` to verify that they interoperate correctly.
		assert_ok!(Tx::post_dispatch(pre, &info, &mut post_info, LEN, &Ok(())));

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
			.validate_and_prepare(ALICE_ORIGIN.clone().into(), CALL, &info, LEN)
			.unwrap();
		assert_eq!(pre.0, Some(100));

		// The `CheckWeight` extension will refund `actual_weight` from `PostDispatchInfo`
		// we always need to call `post_dispatch` to verify that they interoperate correctly.
		// Refunds unspent 25 weight according to `post_info`, 1175
		//
		// storage reclaim:
		// Adds 200 - 25 (unspent) == 175 weight, total weight 1350
		assert_ok!(Tx::post_dispatch(pre, &info, &mut post_info, LEN, &Ok(())));

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
		let mut post_info = PostDispatchInfo {
			actual_weight: Some(Weight::from_parts(50, 100)),
			pays_fee: Default::default(),
		};

		let tx_ext = new_tx_ext();

		// Adds benchmarked weight 100 + 150 (len), total weight is now 250
		let (pre, _) = tx_ext
			.validate_and_prepare(ALICE_ORIGIN.clone().into(), CALL, &info, LEN)
			.unwrap();

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
		assert_ok!(Tx::post_dispatch(pre, &info, &mut post_info, LEN, &Ok(())));

		// Check block len weight was not reclaimed:
		// 100 weight + 150 extrinsic len == 250 proof size
		assert_eq!(get_storage_weight().proof_size(), 250);
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

	let initial_storage_weight = 1212;

	let mut test_ext = setup_test_externalities(&[3232, 3232 + actual_used_proof_size]);

	test_ext.execute_with(|| {
		set_current_storage_weight(initial_storage_weight);

		let extrinsic = new_extrinsic();
		let call_info = extrinsic.function.get_dispatch_info();

		let info = extrinsic.get_dispatch_info();
		let post_info = extrinsic.apply::<Test>(&info, LEN).unwrap().unwrap();

		// Assertions:
		let post_info_tx_proof_size =
			check_weight + storage_weight_reclaim + mock_ext - mock_ext_refund;
		assert_eq!(
			post_info.actual_weight,
			Some(call_info.call_weight + Weight::from_parts(3, post_info_tx_proof_size))
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

	let initial_storage_weight = 1212;

	let mut test_ext = setup_test_externalities(&[3232, 3232 + actual_used_proof_size]);

	test_ext.execute_with(|| {
		set_current_storage_weight(initial_storage_weight);

		let extrinsic = new_extrinsic();
		let call_info = extrinsic.function.get_dispatch_info();

		let info = extrinsic.get_dispatch_info();
		let post_info = extrinsic.apply::<Test>(&info, LEN).unwrap().unwrap();

		// Assertions:
		let post_info_tx_proof_size =
			check_weight + storage_weight_reclaim + mock_ext - mock_ext_refund;
		assert_eq!(
			post_info.actual_weight,
			Some(call_info.call_weight + Weight::from_parts(3, post_info_tx_proof_size))
		);
		assert_eq!(
			get_storage_weight().proof_size(),
			initial_storage_weight + actual_used_proof_size as u64 + LEN as u64
		);
	});
}
