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
use frame_support::dispatch::{DispatchInfo, PostDispatchInfo};
use frame_system::Config;
use scale_info::TypeInfo;
use sp_runtime::{
	traits::{DispatchInfoOf, Dispatchable, PostDispatchInfoOf, SignedExtension},
	transaction_validity::TransactionValidityError,
	DispatchResult,
};
use sp_std::marker::PhantomData;

const LOG_TARGET: &'static str = "runtime::storage_reclaim";

/// Block storage weight reclaim mechanism.
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
		let proof_size = crate::impls::get_storage_size();
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
			let Some(post_dispatch_proof_size) = crate::impls::get_storage_size() else {
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

	#[test]
	fn basic_refund() {
		let mut test_ext = new_test_ext();

		// Storage weight cost: 200 - 100 = 100
		let test_recorder = TestRecorder::new(&[100, 200]);
		test_ext.register_extension(ProofSizeExt::new(test_recorder));

		test_ext.execute_with(|| {
			// Benchmarked storage weight: 500
			let info = DispatchInfo { weight: Weight::from_parts(0, 500), ..Default::default() };
			let post_info = PostDispatchInfo {
				actual_weight: Some(Weight::zero()),
				pays_fee: Default::default(),
			};

			BlockWeight::<Test>::mutate(|current_weight| {
				current_weight.set(Weight::from_parts(0, 1000), DispatchClass::Normal);
			});

			let len = 0_usize;
			let pre = StorageWeightReclaim::<Test>(PhantomData)
				.pre_dispatch(&1, CALL, &info, len)
				.unwrap();
			assert_eq!(pre, 100);

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

			BlockWeight::<Test>::mutate(|current_weight| {
				current_weight.set(Weight::from_parts(0, 1000), DispatchClass::Normal);
			});

			let len = 0_usize;
			let pre = StorageWeightReclaim::<Test>(PhantomData)
				.pre_dispatch(&1, CALL, &info, len)
				.unwrap();
			assert_eq!(pre, 0);

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
		let mut test_ext = new_test_ext();

		// Storage weight cost: 300 - 100 = 200
		let test_recorder = TestRecorder::new(&[100, 300]);

		test_ext.register_extension(ProofSizeExt::new(test_recorder));

		test_ext.execute_with(|| {
			// Benchmarked storage weight: 100
			let info = DispatchInfo { weight: Weight::from_parts(0, 100), ..Default::default() };
			let post_info = PostDispatchInfo {
				actual_weight: Some(Weight::zero()),
				pays_fee: Default::default(),
			};

			BlockWeight::<Test>::mutate(|current_weight| {
				current_weight.set(Weight::from_parts(0, 1000), DispatchClass::Normal);
			});

			let len = 0_usize;
			let pre = StorageWeightReclaim::<Test>(PhantomData)
				.pre_dispatch(&1, CALL, &info, len)
				.unwrap();
			assert_eq!(pre, 100);

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
}
