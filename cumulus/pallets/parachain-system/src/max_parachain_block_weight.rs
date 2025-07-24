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

//! Utilities for calculating maximum parachain block weight based on core assignments.

use crate::Config;
use codec::{Decode, DecodeWithMemTracking, Encode};
use cumulus_primitives_core::CumulusDigestItem;
use frame_support::{
	dispatch::{DispatchInfo, PostDispatchInfo},
	pallet_prelude::{TransactionSource, TransactionValidityError, ValidTransaction},
	weights::{constants::WEIGHT_REF_TIME_PER_SECOND, Weight},
};
use polkadot_primitives::MAX_POV_SIZE;
use scale_info::TypeInfo;
use sp_runtime::{
	traits::{DispatchInfoOf, Dispatchable, Implication, PostDispatchInfoOf, TransactionExtension},
	DispatchResult,
};

/// A utility type for calculating the maximum block weight for a parachain based on
/// the number of relay chain cores assigned and the target number of blocks.
pub struct MaxParachainBlockWeight;

impl MaxParachainBlockWeight {
	/// Calculate the maximum block weight based on target blocks and core assignments.
	///
	/// This function examines the current block's digest from `frame_system::Digests` storage
	/// to find `CumulusDigestItem::CoreInfo` entries, which contain information about the
	/// number of relay chain cores assigned to the parachain. Each core has a maximum
	/// reference time of 2 seconds and the total maximum PoV size of `MAX_POV_SIZE` is
	/// shared across all target blocks.
	///
	/// # Parameters
	/// - `target_blocks`: The target number of blocks to be produced
	///
	/// # Returns
	/// Returns the calculated maximum weight, or a conservative default if no core info is found
	/// or if an error occurs during calculation.
	pub fn get<T: frame_system::Config>(target_blocks: u32) -> Weight {
		// Maximum ref time per core
		const MAX_REF_TIME_PER_CORE_NS: u64 = 2 * WEIGHT_REF_TIME_PER_SECOND;

		let digest = frame_system::Pallet::<T>::digest();

		let Some(core_info) = CumulusDigestItem::find_core_info(&digest) else {
			return Weight::from_parts(MAX_REF_TIME_PER_CORE_NS, MAX_POV_SIZE as u64);
		};

		let number_of_cores = core_info.number_of_cores.0 as u32;

		// Ensure we have at least one core and valid target blocks
		if number_of_cores == 0 || target_blocks == 0 {
			return Weight::from_parts(MAX_REF_TIME_PER_CORE_NS, MAX_POV_SIZE as u64);
		}

		let total_ref_time = (number_of_cores as u64).saturating_mul(MAX_REF_TIME_PER_CORE_NS);
		let ref_time_per_block = total_ref_time
			.saturating_div(target_blocks as u64)
			.min(MAX_REF_TIME_PER_CORE_NS);

		let total_pov_size = (number_of_cores as u64).saturating_mul(MAX_POV_SIZE as u64);
		let proof_size_per_block = total_pov_size.saturating_div(target_blocks as u64);

		Weight::from_parts(ref_time_per_block, proof_size_per_block)
	}
}

#[derive(Encode, Decode, DecodeWithMemTracking, TypeInfo)]
#[derive_where::derive_where(Clone, Eq, PartialEq, Default; S)]
#[scale_info(skip_type_params(T))]
pub struct DynamicMaxBlockWeight<T, S>(pub S, core::marker::PhantomData<T>);

impl<T, S> DynamicMaxBlockWeight<T, S> {
	/// Create a new `StorageWeightReclaim` instance.
	pub fn new(s: S) -> Self {
		Self(s, Default::default())
	}
}

impl<T, S> From<S> for DynamicMaxBlockWeight<T, S> {
	fn from(s: S) -> Self {
		Self::new(s)
	}
}

impl<T, S: core::fmt::Debug> core::fmt::Debug for DynamicMaxBlockWeight<T, S> {
	fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> Result<(), core::fmt::Error> {
		write!(f, "DynamicMaxBlockWeight<{:?}>", self.0)
	}
}

impl<T: Config + Send + Sync, S: TransactionExtension<T::RuntimeCall>>
	TransactionExtension<T::RuntimeCall> for DynamicMaxBlockWeight<T, S>
where
	T::RuntimeCall: Dispatchable<Info = DispatchInfo, PostInfo = PostDispatchInfo>,
{
	const IDENTIFIER: &'static str = "DynamicMaxBlockWeight<Use `metadata()`!>";

	type Implicit = S::Implicit;

	type Val = S::Val;

	type Pre = S::Pre;

	fn implicit(&self) -> Result<Self::Implicit, TransactionValidityError> {
		self.0.implicit()
	}

	fn metadata() -> Vec<sp_runtime::traits::TransactionExtensionMetadata> {
		let mut inner = S::metadata();
		inner.push(sp_runtime::traits::TransactionExtensionMetadata {
			identifier: "DynamicMaxBlockWeight",
			ty: scale_info::meta_type::<()>(),
			implicit: scale_info::meta_type::<()>(),
		});
		inner
	}

	fn weight(&self, _: &T::RuntimeCall) -> Weight {
		Weight::zero()
	}

	fn validate(
		&self,
		origin: T::RuntimeOrigin,
		call: &T::RuntimeCall,
		info: &DispatchInfoOf<T::RuntimeCall>,
		len: usize,
		self_implicit: Self::Implicit,
		inherited_implication: &impl Implication,
		source: TransactionSource,
	) -> Result<(ValidTransaction, Self::Val, T::RuntimeOrigin), TransactionValidityError> {
		self.0
			.validate(origin, call, info, len, self_implicit, inherited_implication, source)
	}

	fn prepare(
		self,
		val: Self::Val,
		origin: &T::RuntimeOrigin,
		call: &T::RuntimeCall,
		info: &DispatchInfoOf<T::RuntimeCall>,
		len: usize,
	) -> Result<Self::Pre, TransactionValidityError> {
		// TODO: Check the weight of the call
		// Store in some storage item the current block number + the mode that we allow
		// There should be the default mode of not allowing to overshoot, then the mode we allow to
		// overshoot if the weight of the call is below the weight of one core but above one of the
		// axis of the actual block weight. So, if we are above the max storage proof size or the
		// ref time, we allow it to above. Use the digest to check if we are in the first block.
		self.0.prepare(val, origin, call, info, len)
	}

	fn post_dispatch(
		pre: Self::Pre,
		info: &DispatchInfoOf<T::RuntimeCall>,
		post_info: &mut PostDispatchInfo,
		len: usize,
		result: &DispatchResult,
	) -> Result<(), TransactionValidityError> {
		S::post_dispatch(pre, info, post_info, len, result)
	}

	fn bare_validate(
		call: &T::RuntimeCall,
		info: &DispatchInfoOf<T::RuntimeCall>,
		len: usize,
	) -> frame_support::pallet_prelude::TransactionValidity {
		S::bare_validate(call, info, len)
	}

	fn bare_validate_and_prepare(
		call: &T::RuntimeCall,
		info: &DispatchInfoOf<T::RuntimeCall>,
		len: usize,
	) -> Result<(), TransactionValidityError> {
		S::bare_validate_and_prepare(call, info, len)
	}

	fn bare_post_dispatch(
		info: &DispatchInfoOf<T::RuntimeCall>,
		post_info: &mut PostDispatchInfoOf<T::RuntimeCall>,
		len: usize,
		result: &DispatchResult,
	) -> Result<(), TransactionValidityError> {
		S::bare_post_dispatch(info, post_info, len, result)?;

		frame_system::Pallet::<T>::reclaim_weight(info, post_info)
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use codec::Compact;
	use cumulus_primitives_core::{ClaimQueueOffset, CoreInfo, CoreSelector};
	use frame_support::{construct_runtime, derive_impl};
	use sp_io;
	use sp_runtime::{traits::IdentityLookup, BuildStorage};

	type Block = frame_system::mocking::MockBlock<Test>;

	// Configure a mock runtime to test the functionality
	#[derive_impl(frame_system::config_preludes::TestDefaultConfig)]
	impl frame_system::Config for Test {
		type Block = Block;
		type AccountId = u64;
		type AccountData = ();
		type Lookup = IdentityLookup<Self::AccountId>;
	}

	construct_runtime!(
		pub enum Test {
			System: frame_system,
		}
	);

	fn new_test_ext_with_digest(num_cores: Option<u16>) -> sp_io::TestExternalities {
		let storage = frame_system::GenesisConfig::<Test>::default().build_storage().unwrap();

		let mut ext = sp_io::TestExternalities::from(storage);

		ext.execute_with(|| {
			if let Some(num_cores) = num_cores {
				let core_info = CoreInfo {
					selector: CoreSelector(0),
					claim_queue_offset: ClaimQueueOffset(0),
					number_of_cores: Compact(num_cores),
				};

				let digest = CumulusDigestItem::CoreInfo(core_info).to_digest_item();

				frame_system::Pallet::<Test>::deposit_log(digest);
			}
		});

		ext
	}

	#[test]
	fn test_single_core_single_block() {
		new_test_ext_with_digest(Some(1)).execute_with(|| {
			let weight = MaxParachainBlockWeight::get::<Test>(1);

			// With 1 core and 1 target block, should get full 2s ref time and full PoV size
			assert_eq!(weight.ref_time(), 2 * WEIGHT_REF_TIME_PER_SECOND);
			assert_eq!(weight.proof_size(), MAX_POV_SIZE as u64);
		});
	}

	#[test]
	fn test_single_core_multiple_blocks() {
		new_test_ext_with_digest(Some(1)).execute_with(|| {
			let weight = MaxParachainBlockWeight::get::<Test>(4);

			// With 1 core and 4 target blocks, should get 0.5s ref time and 1/4 PoV size per block
			assert_eq!(weight.ref_time(), 2 * WEIGHT_REF_TIME_PER_SECOND / 4);
			assert_eq!(weight.proof_size(), (1 * MAX_POV_SIZE as u64) / 4);
		});
	}

	#[test]
	fn test_multiple_cores_single_block() {
		new_test_ext_with_digest(Some(3)).execute_with(|| {
			let weight = MaxParachainBlockWeight::get::<Test>(1);

			// With 3 cores and 1 target block, should get max 2s ref time (capped per core) and 3x
			// PoV size
			assert_eq!(weight.ref_time(), 2 * WEIGHT_REF_TIME_PER_SECOND);
			assert_eq!(weight.proof_size(), 3 * MAX_POV_SIZE as u64);
		});
	}

	#[test]
	fn test_multiple_cores_multiple_blocks() {
		new_test_ext_with_digest(Some(2)).execute_with(|| {
			let weight = MaxParachainBlockWeight::get::<Test>(4);

			// With 2 cores and 4 target blocks, should get 1s ref time and 2x PoV size / 4 per
			// block
			assert_eq!(weight.ref_time(), 2 * 2 * WEIGHT_REF_TIME_PER_SECOND / 4);
			assert_eq!(weight.proof_size(), (2 * MAX_POV_SIZE as u64) / 4);
		});
	}

	#[test]
	fn test_no_core_info() {
		new_test_ext_with_digest(None).execute_with(|| {
			let weight = MaxParachainBlockWeight::get::<Test>(1);

			// Without core info, should return conservative default
			assert_eq!(weight.ref_time(), 2 * WEIGHT_REF_TIME_PER_SECOND);
			assert_eq!(weight.proof_size(), MAX_POV_SIZE as u64);
		});
	}

	#[test]
	fn test_zero_cores() {
		new_test_ext_with_digest(Some(0)).execute_with(|| {
			let weight = MaxParachainBlockWeight::get::<Test>(1);

			// With 0 cores, should return conservative default
			assert_eq!(weight.ref_time(), 2 * WEIGHT_REF_TIME_PER_SECOND);
			assert_eq!(weight.proof_size(), MAX_POV_SIZE as u64);
		});
	}

	#[test]
	fn test_zero_target_blocks() {
		new_test_ext_with_digest(Some(2)).execute_with(|| {
			let weight = MaxParachainBlockWeight::get::<Test>(0);

			// With 0 target blocks, should return conservative default
			assert_eq!(weight.ref_time(), 2 * WEIGHT_REF_TIME_PER_SECOND);
			assert_eq!(weight.proof_size(), MAX_POV_SIZE as u64);
		});
	}
}
