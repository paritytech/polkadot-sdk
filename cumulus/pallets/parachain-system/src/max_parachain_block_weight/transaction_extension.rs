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

use super::{
	block_weight_over_target_block_weight, is_first_block_in_core, BlockWeightMode,
	MaxParachainBlockWeight, LOG_TARGET,
};
use crate::Config;
use alloc::vec::Vec;
use codec::{Decode, DecodeWithMemTracking, Encode};
use cumulus_primitives_core::CumulusDigestItem;
use frame_support::{
	dispatch::{DispatchInfo, PostDispatchInfo},
	pallet_prelude::{
		InvalidTransaction, TransactionSource, TransactionValidityError, ValidTransaction,
	},
	weights::Weight,
};
use scale_info::TypeInfo;
use sp_core::Get;
use sp_runtime::{
	traits::{DispatchInfoOf, Dispatchable, Implication, PostDispatchInfoOf, TransactionExtension},
	DispatchResult,
};

/// Transaction extension that dynamically changes the max block weight.
///
/// With block bundling, parachains are running with block weights that may not allow certain
/// transactions to be applied, e.g. a runtime upgrade. To ensure that these transactions can still
/// be applied, this transaction extension can change the max block weight as required. There are
/// multiple requirements for it to change the block weight:
///
/// 1. Only the first block of a core is allowed to change its block weight.
///
/// 2. Any `inherent` or any transaction up to `MAX_TRANSACTION_TO_CONSIDER` requires more block
///    weight than the target block weight. Target block weight is the max weight for the respective
///    extrinsic class.
///
/// Because the node is tracking the wall clock time while building a block to abort block
/// production if it takes too long, we do not allow any block to change the block weight. The node
/// knows that the first block of a core may runs longer. So, the node allows this block to take up
/// to `2s` of wall clock time. `2s` is the time each `PoV` gets on the relay chain for its
/// validation or in other words the maximum core execution time. The extension sets the
/// [`CumulusDigestItem::UseFullCore`] digest when the block should occupy the entire core.
///
/// # Generic parameters
///
/// - `TargetBlockRate`: The target block rate the parachain should be running with. Or in other
///   words, the number of blocks the parachain should produce in `6s`(relay chain slot duration).
///
/// - `MAX_TRANSACTION`: The maximum number of transactions to consider before giving up to change
///   the max block weight.
///
/// - `ONLY_OPERATIONAL`: Should only operational transactions be allowed to change the max block
///   weight?
#[derive(Encode, Decode, DecodeWithMemTracking, TypeInfo)]
#[derive_where::derive_where(Clone, Eq, PartialEq, Default; S)]
#[scale_info(skip_type_params(T, TargetBlockRate))]
pub struct DynamicMaxBlockWeight<
	T,
	S,
	TargetBlockRate,
	const MAX_TRANSACTION_TO_CONSIDER: u32 = 10,
	const ONLY_OPERATIONAL: bool = false,
>(pub S, core::marker::PhantomData<(T, TargetBlockRate)>);

impl<T, S, TargetBlockRate> DynamicMaxBlockWeight<T, S, TargetBlockRate> {
	/// Create a new [`DynamicMaxBlockWeight`] instance.
	pub fn new(s: S) -> Self {
		Self(s, Default::default())
	}
}

impl<
		T,
		S,
		TargetBlockRate,
		const MAX_TRANSACTION_TO_CONSIDER: u32,
		const ONLY_OPERATIONAL: bool,
	> DynamicMaxBlockWeight<T, S, TargetBlockRate, MAX_TRANSACTION_TO_CONSIDER, ONLY_OPERATIONAL>
where
	T: Config,
	TargetBlockRate: Get<u32>,
{
	fn pre_validate_extrinsic(
		info: &DispatchInfo,
		len: usize,
	) -> Result<(), TransactionValidityError> {
		let is_not_inherent = frame_system::Pallet::<T>::inherents_applied();
		let extrinsic_index = is_not_inherent
			.then(|| frame_system::Pallet::<T>::extrinsic_index().unwrap_or_default());

		crate::BlockWeightMode::<T>::mutate(|mode| {
			let current_mode = *mode.get_or_insert_with(|| BlockWeightMode::FractionOfCore {
				first_transaction_index: extrinsic_index,
			});

			match current_mode {
				// We are already allowing the full core, not that much more to do here.
				BlockWeightMode::FullCore => {},
				BlockWeightMode::PotentialFullCore { first_transaction_index, .. } |
				BlockWeightMode::FractionOfCore { first_transaction_index } => {
					let is_potential =
						matches!(current_mode, BlockWeightMode::PotentialFullCore { .. });
					debug_assert!(
						!is_potential,
						"`PotentialFullCore` should resolve to `FullCore` or `FractionOfCore` after applying a transaction.",
					);

					let block_weight_over_limit = first_transaction_index == extrinsic_index
						&& block_weight_over_target_block_weight::<T, TargetBlockRate>();

					let block_weights = T::BlockWeights::get();
					let target_weight = block_weights.get(info.class).max_total.unwrap_or_else(
						|| MaxParachainBlockWeight::<T>::target_block_weight(TargetBlockRate::get()).saturating_sub(block_weights.base_block)
					);

					// Protection against a misconfiguration as this should be detected by the pre-inherent hook.
					if block_weight_over_limit {
						*mode = Some(BlockWeightMode::FullCore);

						// Inform the node that this block uses the full core.
						frame_system::Pallet::<T>::deposit_log(
							CumulusDigestItem::UseFullCore.to_digest_item(),
						);

						log::error!(
							target: LOG_TARGET,
							"Inherent block logic took longer than the target block weight, \
							`MaxBlockWeightHooks` not registered as `PreInherents` hook!",
						);
					} else if info
						.total_weight()
						// The extrinsic lengths counts towards the POV size
						.saturating_add(Weight::from_parts(0, len as u64))
						.any_gt(target_weight) && is_first_block_in_core::<T>()
					{
						if extrinsic_index.unwrap_or_default().saturating_sub(first_transaction_index.unwrap_or_default()) < MAX_TRANSACTION_TO_CONSIDER {
							*mode = Some(BlockWeightMode::PotentialFullCore {
								target_weight,
								// While applying inherents `extrinsic_index` and `first_transaction_index` will be `None`.
								// When the first transaction is applied, we want to store the index.
								first_transaction_index: first_transaction_index.or(extrinsic_index),
							});
						} else {
							return Err(InvalidTransaction::ExhaustsResources)
						}
					} else if is_potential {
						*mode =
							Some(BlockWeightMode::FractionOfCore { first_transaction_index });
					}
				},
			};

			Ok(())
		}).map_err(Into::into)
	}

	fn post_dispatch_extrinsic(info: &DispatchInfo) {
		crate::BlockWeightMode::<T>::mutate(|weight_mode| {
			let Some(mode) = *weight_mode else { return };

			match mode {
				// If the previous mode was already `FullCore`, we are fine.
				BlockWeightMode::FullCore => {},
				BlockWeightMode::FractionOfCore { .. } => {
					let target_block_weight =
						MaxParachainBlockWeight::<T>::target_block_weight(TargetBlockRate::get());

					let is_above_limit = frame_system::Pallet::<T>::remaining_block_weight()
						.consumed()
						.any_gt(target_block_weight);

					// If we are above the limit, it means the transaction used more weight than
					// what it had announced, which should not happen.
					if is_above_limit {
						log::error!(
							target: LOG_TARGET,
							"Extrinsic ({}) used more weight than what it had announced and pushed the \
							block above the allowed weight limit!",
							frame_system::Pallet::<T>::extrinsic_index().unwrap_or_default()
						);

						// If this isn't the first block in a core, we register the full core weight
						// to ensure that we don't include any other transactions. Because we don't
						// know how many weight of the core was already used by the blocks before.
						if !is_first_block_in_core::<T>() {
							log::error!(
								target: LOG_TARGET,
								"Registering `FULL_CORE_WEIGHT` to ensure no other transaction is included \
								in this block, because this isn't the first block in the core!",
							);

							frame_system::Pallet::<T>::register_extra_weight_unchecked(
								MaxParachainBlockWeight::<T>::FULL_CORE_WEIGHT,
								frame_support::dispatch::DispatchClass::Mandatory,
							);
						}

						*weight_mode = Some(BlockWeightMode::FullCore);

						// Inform the node that this block uses the full core.
						frame_system::Pallet::<T>::deposit_log(
							CumulusDigestItem::UseFullCore.to_digest_item(),
						);
					}
				},
				// Now we need to check if the transaction required more weight than a fraction of a
				// core block.
				BlockWeightMode::PotentialFullCore { first_transaction_index, target_weight } => {
					let block_weight = frame_system::BlockWeight::<T>::get();

					if block_weight.get(info.class).any_gt(target_weight) {
						*weight_mode = Some(BlockWeightMode::FullCore);

						// Inform the node that this block uses the full core.
						frame_system::Pallet::<T>::deposit_log(
							CumulusDigestItem::UseFullCore.to_digest_item(),
						);
					} else {
						*weight_mode =
							Some(BlockWeightMode::FractionOfCore { first_transaction_index });
					}
				},
			}
		});
	}
}

impl<T, S, TargetBlockRate> From<S> for DynamicMaxBlockWeight<T, S, TargetBlockRate> {
	fn from(s: S) -> Self {
		Self::new(s)
	}
}

impl<T, S: core::fmt::Debug, TargetBlockRate> core::fmt::Debug
	for DynamicMaxBlockWeight<T, S, TargetBlockRate>
{
	fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> Result<(), core::fmt::Error> {
		write!(f, "DynamicMaxBlockWeight<{:?}>", self.0)
	}
}

impl<
		T: Config + Send + Sync,
		S: TransactionExtension<T::RuntimeCall>,
		TargetBlockRate: Get<u32> + Send + Sync + 'static,
	> TransactionExtension<T::RuntimeCall> for DynamicMaxBlockWeight<T, S, TargetBlockRate>
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
		Self::pre_validate_extrinsic(info, len)?;

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
		self.0.prepare(val, origin, call, info, len)
	}

	fn post_dispatch(
		pre: Self::Pre,
		info: &DispatchInfoOf<T::RuntimeCall>,
		post_info: &mut PostDispatchInfo,
		len: usize,
		result: &DispatchResult,
	) -> Result<(), TransactionValidityError> {
		S::post_dispatch(pre, info, post_info, len, result)?;

		Self::post_dispatch_extrinsic(info);

		Ok(())
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
		S::bare_validate_and_prepare(call, info, len)?;

		Self::pre_validate_extrinsic(info, len)?;

		Ok(())
	}

	fn bare_post_dispatch(
		info: &DispatchInfoOf<T::RuntimeCall>,
		post_info: &mut PostDispatchInfoOf<T::RuntimeCall>,
		len: usize,
		result: &DispatchResult,
	) -> Result<(), TransactionValidityError> {
		S::bare_post_dispatch(info, post_info, len, result)?;

		Self::post_dispatch_extrinsic(info);

		Ok(())
	}
}
