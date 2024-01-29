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

//! Mechanism to reclaim PoV weight after an extrinsic has been applied.

use codec::{Decode, Encode};
use cumulus_primitives_core::Weight;
use cumulus_primitives_proof_size_hostfunction::storage_proof_size::storage_proof_size;
use frame_support::dispatch::{DispatchClass, DispatchInfo, PostDispatchInfo};
use frame_system::{BlockWeight, Config};
use scale_info::TypeInfo;
use sp_runtime::{
	traits::{DispatchInfoOf, Dispatchable, PostDispatchInfoOf, SignedExtension},
	transaction_validity::TransactionValidityError,
	DispatchResult,
};
use sp_std::marker::PhantomData;

const LOG_TARGET: &'static str = "runtime::storage_reclaim";

/// Indicates that proof recording was disabled on the node side.
const PROOF_RECORDING_DISABLED: u64 = u64::MAX;

/// StorageWeightReclaimer is a mechanism for manually reclaiming storage weight.
///
/// It internally keeps track of the proof size and storage weight at initialization time. At
/// reclaim  it computes the real consumed storage weight and refunds excess weight.
///
///
/// # Example
///
/// ```rust
/// use parachains_common::storage_weight_reclaim::StorageWeightReclaimer;
///
/// // Initialize a StorageWeightReclaimer instance.
/// let mut storage_reclaim = StorageWeightReclaimer::<Runtime>::start();
///
/// // Perform some operations that consume storage weight.
///
/// // Reclaim unused storage weight.
/// if let Some(reclaimed_weight) = storage_reclaim.reclaim() {
///     log::info!("Reclaimed {} weight", reclaimed_weight);
/// }
/// ```
pub struct StorageWeightReclaimer<T: Config> {
	previous_proof_weight: u64,
	previous_reported_proof_size: Option<u64>,
	_phantom: PhantomData<T>,
}

impl<T: Config> StorageWeightReclaimer<T> {
	/// Creates a new `StorageWeightReclaimer` instance and initializes it with the current storage
	/// weight and reported proof size from the node.
	pub fn start() -> StorageWeightReclaimer<T> {
		let previous_proof_weight = BlockWeight::<T>::get().total().proof_size();
		let previous_reported_proof_size = get_proof_size();
		Self { previous_proof_weight, previous_reported_proof_size, _phantom: Default::default() }
	}

	/// Check the consumed storage weight and reclaim any consumed excess weight.
	pub fn reclaim(&mut self) -> Option<Weight> {
		let current_weight = BlockWeight::<T>::get().total().proof_size();
		let (Some(current_storage_proof_size), Some(initial_proof_size)) =
			(get_proof_size(), self.previous_reported_proof_size)
		else {
			return None;
		};
		let used_weight = current_weight.saturating_sub(self.previous_proof_weight);
		let used_storage_proof = current_storage_proof_size.saturating_sub(initial_proof_size);
		let reclaimable = used_weight.saturating_sub(used_storage_proof);
		log::trace!(target: LOG_TARGET, "Reclaiming storage weight. benchmarked_weight: {used_weight}, consumed_weight: {used_storage_proof}, reclaimable: {reclaimable}");
		frame_system::BlockWeight::<T>::mutate(|current| {
			current.reduce(Weight::from_parts(0, reclaimable), DispatchClass::Normal);
		});

		self.previous_proof_weight = current_weight.saturating_sub(reclaimable);
		self.previous_reported_proof_size = Some(current_storage_proof_size);
		Some(Weight::from_parts(0, reclaimable))
	}
}

/// Returns the current storage proof size from the host side.
///
/// Returns `None` if proof recording is disabled on the host.
pub fn get_proof_size() -> Option<u64> {
	let proof_size = storage_proof_size();
	(proof_size != PROOF_RECORDING_DISABLED).then_some(proof_size)
}

/// Storage weight reclaim mechanism.
///
/// This extension checks the size of the node-side storage proof
/// before and after executing a given extrinsic. The difference between
/// benchmarked and spent weight can be reclaimed.
#[derive(Encode, Decode, Clone, Eq, PartialEq, Default, TypeInfo)]
#[scale_info(skip_type_params(T))]
pub struct StorageWeightReclaim<T: Config + Send + Sync>(PhantomData<T>);

impl<T: Config + Send + Sync> core::fmt::Debug for StorageWeightReclaim<T> {
	fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> Result<(), core::fmt::Error> {
		let _ = write!(f, "StorageWeightReclaim");
		Ok(())
	}
}

impl<T: Config + Send + Sync> SignedExtension for StorageWeightReclaim<T>
where
	T::RuntimeCall: Dispatchable<Info = DispatchInfo, PostInfo = PostDispatchInfo>,
{
	const IDENTIFIER: &'static str = "StorageWeightReclaim";

	type AccountId = T::AccountId;
	type Call = T::RuntimeCall;
	type AdditionalSigned = ();
	type Pre = Option<u64>;

	fn additional_signed(
		&self,
	) -> Result<Self::AdditionalSigned, sp_runtime::transaction_validity::TransactionValidityError>
	{
		Ok(())
	}

	fn pre_dispatch(
		self,
		_who: &Self::AccountId,
		_call: &Self::Call,
		_info: &sp_runtime::traits::DispatchInfoOf<Self::Call>,
		_len: usize,
	) -> Result<Self::Pre, sp_runtime::transaction_validity::TransactionValidityError> {
		let proof_size = get_proof_size();
		Ok(proof_size)
	}

	fn post_dispatch(
		pre: Option<Self::Pre>,
		info: &DispatchInfoOf<Self::Call>,
		_post_info: &PostDispatchInfoOf<Self::Call>,
		_len: usize,
		_result: &DispatchResult,
	) -> Result<(), TransactionValidityError> {
		if let Some(Some(pre_dispatch_proof_size)) = pre {
			let Some(post_dispatch_proof_size) = get_proof_size() else {
				log::debug!(target: LOG_TARGET, "Proof recording enabled during pre-dispatch, now disabled. This should not happen.");
				return Ok(())
			};
			let benchmarked_weight = info.weight.proof_size();
			let consumed_weight = post_dispatch_proof_size.saturating_sub(pre_dispatch_proof_size);

			if consumed_weight > benchmarked_weight {
				log::debug!(target: LOG_TARGET, "Benchmarked storage weight smaller than consumed storage weight. benchmarked_weight: {benchmarked_weight} consumed_weight: {consumed_weight}");
				return Ok(())
			}

			let reclaimable_storage_part =
				benchmarked_weight.saturating_sub(consumed_weight as u64);
			log::trace!(target: LOG_TARGET,"Reclaiming storage weight. benchmarked_weight: {benchmarked_weight}, consumed_weight: {consumed_weight}, reclaimable: {reclaimable_storage_part}");
			frame_system::BlockWeight::<T>::mutate(|current| {
				current.reduce(Weight::from_parts(0, reclaimable_storage_part), info.class)
			});
		}
		Ok(())
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use frame_support::{assert_ok, dispatch::DispatchClass, weights::Weight};
	use frame_system::{
		mock::{new_test_ext, Test, CALL},
		BlockWeight,
	};
	use sp_std::marker::PhantomData;
	use sp_trie::proof_size_extension::ProofSizeExt;

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

	fn base_block_weight() -> Weight {
		<Test as frame_system::Config>::BlockWeights::get().base_block
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
		let mut test_ext = setup_test_externalities(&[100, 200]);

		test_ext.execute_with(|| {
			// Benchmarked storage weight: 500
			let info = DispatchInfo { weight: Weight::from_parts(0, 500), ..Default::default() };
			let post_info = PostDispatchInfo {
				actual_weight: Some(Weight::zero()),
				pays_fee: Default::default(),
			};

			set_current_storage_weight(1000);

			let len = 0_usize;
			let pre = StorageWeightReclaim::<Test>(PhantomData)
				.pre_dispatch(&1, CALL, &info, len)
				.unwrap();
			assert_eq!(pre, Some(100));

			// We expect a refund of 400
			assert_ok!(StorageWeightReclaim::<Test>::post_dispatch(
				Some(pre),
				&info,
				&post_info,
				len,
				&Ok(())
			));

			assert_eq!(
				BlockWeight::<Test>::get().total(),
				Weight::from_parts(base_block_weight().ref_time(), 600)
			);
		})
	}

	#[test]
	fn does_nothing_without_extension() {
		let mut test_ext = new_test_ext();

		// Proof size extension not registered
		test_ext.execute_with(|| {
			// Benchmarked storage weight: 500
			let info = DispatchInfo { weight: Weight::from_parts(0, 500), ..Default::default() };
			let post_info = PostDispatchInfo {
				actual_weight: Some(Weight::zero()),
				pays_fee: Default::default(),
			};

			set_current_storage_weight(1000);

			let len = 0_usize;
			let pre = StorageWeightReclaim::<Test>(PhantomData)
				.pre_dispatch(&1, CALL, &info, len)
				.unwrap();
			assert_eq!(pre, None);

			// We expect a refund of 400
			assert_ok!(StorageWeightReclaim::<Test>::post_dispatch(
				Some(pre),
				&info,
				&post_info,
				len,
				&Ok(())
			));

			assert_eq!(
				BlockWeight::<Test>::get().total(),
				Weight::from_parts(base_block_weight().ref_time(), 1000)
			);
		})
	}

	#[test]
	fn negative_refund_is_ignored() {
		let mut test_ext = setup_test_externalities(&[100, 300]);

		test_ext.execute_with(|| {
			// Benchmarked storage weight: 100
			let info = DispatchInfo { weight: Weight::from_parts(0, 100), ..Default::default() };
			let post_info = PostDispatchInfo {
				actual_weight: Some(Weight::zero()),
				pays_fee: Default::default(),
			};

			set_current_storage_weight(1000);

			let len = 0_usize;
			let pre = StorageWeightReclaim::<Test>(PhantomData)
				.pre_dispatch(&1, CALL, &info, len)
				.unwrap();
			assert_eq!(pre, Some(100));

			// We expect no refund
			assert_ok!(StorageWeightReclaim::<Test>::post_dispatch(
				Some(pre),
				&info,
				&post_info,
				len,
				&Ok(())
			));

			assert_eq!(
				BlockWeight::<Test>::get().total(),
				Weight::from_parts(base_block_weight().ref_time(), 1000)
			);
		})
	}

	#[test]
	fn test_zero_proof_size() {
		let mut test_ext = setup_test_externalities(&[0, 0]);

		test_ext.execute_with(|| {
			let info = DispatchInfo { weight: Weight::from_parts(0, 500), ..Default::default() };
			let post_info = PostDispatchInfo::default();

			let len = 0_usize;
			let pre = StorageWeightReclaim::<Test>(PhantomData)
				.pre_dispatch(&1, CALL, &info, len)
				.unwrap();
			assert_eq!(pre, Some(0));

			assert_ok!(StorageWeightReclaim::<Test>::post_dispatch(
				Some(pre),
				&info,
				&post_info,
				len,
				&Ok(())
			));

			assert_eq!(BlockWeight::<Test>::get().total(), base_block_weight());
		});
	}

	#[test]
	fn test_larger_pre_dispatch_proof_size() {
		let mut test_ext = setup_test_externalities(&[300, 100]);

		test_ext.execute_with(|| {
			let info = DispatchInfo { weight: Weight::from_parts(0, 500), ..Default::default() };
			let post_info = PostDispatchInfo::default();

			set_current_storage_weight(1313);

			let len = 0_usize;
			let pre = StorageWeightReclaim::<Test>(PhantomData)
				.pre_dispatch(&1, CALL, &info, len)
				.unwrap();
			assert_eq!(pre, Some(300));

			assert_ok!(StorageWeightReclaim::<Test>::post_dispatch(
				Some(pre),
				&info,
				&post_info,
				len,
				&Ok(())
			));

			assert_eq!(
				BlockWeight::<Test>::get().total(),
				Weight::from_parts(base_block_weight().ref_time(), 813)
			);
		});
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
			set_current_storage_weight(1000);
			let mut reclaim_helper = StorageWeightReclaimer::<Test>::start();
			set_current_storage_weight(1500);
			let reclaimed = reclaim_helper.reclaim();

			assert_eq!(reclaimed, Some(Weight::from_parts(0, 200)));
			assert_eq!(
				BlockWeight::<Test>::get().total(),
				Weight::from_parts(base_block_weight().ref_time(), 1300)
			);

			set_current_storage_weight(2100);
			let reclaimed = reclaim_helper.reclaim();
			assert_eq!(reclaimed, Some(Weight::from_parts(0, 300)));
			assert_eq!(
				BlockWeight::<Test>::get().total(),
				Weight::from_parts(base_block_weight().ref_time(), 1800)
			);
		});
	}

	#[test]
	fn test_reclaim_helper_does_not_reclaim_negative() {
		// Benchmarked weight does not change at all
		let mut test_ext = setup_test_externalities(&[1000, 1300]);

		test_ext.execute_with(|| {
			set_current_storage_weight(1000);
			let mut reclaim_helper = StorageWeightReclaimer::<Test>::start();
			let reclaimed = reclaim_helper.reclaim();

			assert_eq!(reclaimed, Some(Weight::from_parts(0, 0)));
			assert_eq!(
				BlockWeight::<Test>::get().total(),
				Weight::from_parts(base_block_weight().ref_time(), 1000)
			);
		});

		// Benchmarked weight increases less than storage proof consumes
		let mut test_ext = setup_test_externalities(&[1000, 1300]);

		test_ext.execute_with(|| {
			set_current_storage_weight(1000);
			let mut reclaim_helper = StorageWeightReclaimer::<Test>::start();
			set_current_storage_weight(1200);
			let reclaimed = reclaim_helper.reclaim();

			assert_eq!(reclaimed, Some(Weight::from_parts(0, 0)));
			assert_eq!(
				BlockWeight::<Test>::get().total(),
				Weight::from_parts(base_block_weight().ref_time(), 1200)
			);
		});
	}
}
