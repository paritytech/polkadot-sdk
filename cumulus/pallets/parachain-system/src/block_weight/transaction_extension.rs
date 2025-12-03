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
	block_weight_over_target_block_weight, inside_pre_validate, is_first_block_in_core_with_digest,
	BlockWeightMode, MaxParachainBlockWeight, FULL_CORE_WEIGHT, LOG_TARGET,
};
use crate::WeightInfo;
use alloc::vec::Vec;
use codec::{Decode, DecodeWithMemTracking, Encode};
use cumulus_primitives_core::CumulusDigestItem;
use frame_support::{
	dispatch::{DispatchClass, DispatchInfo, PostDispatchInfo},
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
///    weight than the target extrinsic weight. Target extrinsic weight is the max weight for the
///    respective extrinsic class. The priority to determine the target e weight is the following, we
///    start checking if
///    [`WeightsPerClass::max_extrinsic`](frame_system::limits::WeightsPerClass::max_extrinsic) is
///    set, after this
///    [`WeightsPerClass::max_total`](frame_system::limits::WeightsPerClass::max_total) and if both
///    of these are `None` we fall back to the actual target block weight.
///
/// Because the node is tracking the wall clock time while building a block to abort block
/// production if it takes too long, we do not allow any block to change the block weight. The node
/// knows that the first block of a core may runs longer. So, the node allows this block to take up
/// to `2s` of wall clock time. `2s` is the time each `PoV` gets on the relay chain for its
/// validation or in other words the maximum core execution time. The extension sets the
/// [`CumulusDigestItem::UseFullCore`] digest when the block should occupy the entire core.
///
/// Before dispatching an extrinsic the extension will check the requirements and set the
/// appropriate [`BlockWeightMode`]. After the extrinsic has finished, the checks from before
/// dispatching the extrinsic are repeated with the post dispatch weights. The [`BlockWeightMode`]
/// is changed properly.
///
/// # Note
///
/// The extension requires that any of the inner extensions sets the
/// [`BlockWeight`](frame_system::BlockWeight). Otherwise the weight tracking is not working
/// properly. Normally this is done by [`CheckWeight`](frame_system::CheckWeight).
///
/// # Generic parameters
///
/// - `Config`: The [`Config`](crate::Config) trait of this pallet.
///
/// - `Inner`: The inner transaction extensions aka the other transaction extensions to be used by
///   the runtime.
///
/// - `TargetBlockRate`: The target block rate the parachain should be running with. Or in other
///   words, the number of blocks the parachain should produce in `6s`(relay chain slot duration).
///
/// - `MAX_TRANSACTION`: The maximum number of transactions to consider before giving up to change
///   the max block weight.
///
/// - `ALLOW_NORMAL`: Should transactions with a dispatch class `Normal` be allowed to change the
///   max block weight?
#[derive(Encode, Decode, DecodeWithMemTracking, TypeInfo)]
#[derive_where::derive_where(Clone, Eq, PartialEq, Default; Inner)]
#[scale_info(skip_type_params(Config, TargetBlockRate))]
pub struct DynamicMaxBlockWeight<
	Config,
	Inner,
	TargetBlockRate,
	const MAX_TRANSACTION_TO_CONSIDER: u32 = 10,
	const ALLOW_NORMAL: bool = true,
>(pub Inner, core::marker::PhantomData<(Config, TargetBlockRate)>);

impl<T, S, TargetBlockRate, const MAX_TRANSACTION_TO_CONSIDER: u32, const ALLOW_NORMAL: bool>
	DynamicMaxBlockWeight<T, S, TargetBlockRate, MAX_TRANSACTION_TO_CONSIDER, ALLOW_NORMAL>
{
	/// Create a new [`DynamicMaxBlockWeight`] instance.
	pub fn new(s: S) -> Self {
		Self(s, Default::default())
	}
}

impl<
		Config,
		Inner,
		TargetBlockRate,
		const MAX_TRANSACTION_TO_CONSIDER: u32,
		const ALLOW_NORMAL: bool,
	> DynamicMaxBlockWeight<Config, Inner, TargetBlockRate, MAX_TRANSACTION_TO_CONSIDER, ALLOW_NORMAL>
where
	Config: crate::Config,
	TargetBlockRate: Get<u32>,
{
	/// Should be executed before `validate` is called for any inner extension.
	fn pre_validate_extrinsic(
		info: &DispatchInfo,
		len: usize,
	) -> Result<(), TransactionValidityError> {
		let is_not_inherent = frame_system::Pallet::<Config>::inherents_applied();
		let extrinsic_index = frame_system::Pallet::<Config>::extrinsic_index().unwrap_or_default();
		let transaction_index = is_not_inherent.then(|| extrinsic_index);

		crate::BlockWeightMode::<Config>::mutate(|mode| {
			let current_mode = mode.get_or_insert_with(|| BlockWeightMode::<Config>::fraction_of_core(transaction_index));

			// If the mode is stale (from previous block), we reset it.
			//
			// This happens for example when running in an offchain context.
			if current_mode.is_stale() {
				*current_mode = BlockWeightMode::fraction_of_core(transaction_index);
			}

			log::trace!(
				target: LOG_TARGET,
				"About to pre-validate an extrinsic. current_mode={current_mode:?}, transaction_index={transaction_index:?}"
			);

			let is_potential =
				matches!(current_mode, &mut BlockWeightMode::PotentialFullCore { .. });

			match current_mode {
				// We are already allowing the full core, not that much more to do here.
				BlockWeightMode::<Config>::FullCore { .. } => {},
				BlockWeightMode::<Config>::PotentialFullCore { first_transaction_index, .. } |
				BlockWeightMode::<Config>::FractionOfCore { first_transaction_index, .. } => {
					debug_assert!(
						!is_potential,
						"`PotentialFullCore` should resolve to `FullCore` or `FractionOfCore` after applying a transaction.",
					);

					let digest = frame_system::Pallet::<Config>::digest();
					let block_weight_over_limit = extrinsic_index == 0
						&& block_weight_over_target_block_weight::<Config, TargetBlockRate>();

					// If `BlockWeights` is configured correctly, it will internally call `MaxParachainBlockWeight::get()`
					// and by setting this variable to `true`, we tell it the context. This is important as we want to get
					// the `target_block_weight` and not the full core weight. Otherwise, we will here get a too huge weight
					// and do not set the `PotentialFullCore` weight, leading to `CheckWeight` rejecting the extrinsic.
					//
					// All of this is only important for extrinsics that will enable the `PotentialFullCore` mode.
					let block_weights = inside_pre_validate::using(&mut true, || Config::BlockWeights::get());
					let class_weights = block_weights.get(info.class);
					let target_block_weight =
						MaxParachainBlockWeight::<Config, TargetBlockRate>::target_block_weight_with_digest(&digest)
							.saturating_sub(block_weights.base_block);

					// `max_extrinsic` determines the maximum weight allowed for one transaction.
					// If that isn't set, we fall back to `max_total` which represents the total allowed weight for
					// this dispatch class. If all previous weights are `None`, we fall back to the target block weight.
					let target_weight = class_weights
						.max_extrinsic
						.or(class_weights.max_total)
						.unwrap_or(target_block_weight);

					// Protection against a misconfiguration as this should be detected by the pre-inherent hook.
					if block_weight_over_limit {
						*mode = Some(BlockWeightMode::<Config>::full_core());

						// Inform the node that this block uses the full core.
						frame_system::Pallet::<Config>::deposit_log(
							CumulusDigestItem::UseFullCore.to_digest_item(),
						);

						if !is_first_block_in_core_with_digest(&digest).unwrap_or(false) {
							// We are already above the allowed maximum and do not want to accept any more
							// extrinsics.
							frame_system::Pallet::<Config>::register_extra_weight_unchecked(
								FULL_CORE_WEIGHT,
								DispatchClass::Mandatory,
							);
						}

						log::error!(
							target: LOG_TARGET,
							"Inherent block logic took longer than the target block weight, \
							`DynamicMaxBlockWeightHooks` not registered as `PreInherents` hook!",
						);
					} else if info
						.total_weight()
						// The extrinsic lengths counts towards the POV size
						.saturating_add(Weight::from_parts(0, len as u64))
						.any_gt(target_weight)
					{
						// When `ALLOW_NORMAL` is `true`, we want to allow all classes of transactions. Inherents are always allowed.
						let class_allowed = if ALLOW_NORMAL { true } else { info.class == DispatchClass::Operational }
							|| info.class == DispatchClass::Mandatory;

						// If the `BundleInfo` digest is not set (function returns `None`), it means we are in some offchain
						// call like `validate_block`. In this case we assume this is the first block, otherwise these big
						// transactions will never be able to enter the tx pool.
						let is_first_block = is_first_block_in_core_with_digest(&digest).unwrap_or(true);

						if transaction_index.unwrap_or_default().saturating_sub(first_transaction_index.unwrap_or_default()) < MAX_TRANSACTION_TO_CONSIDER
							&& is_first_block && class_allowed {
							log::trace!(
								target: LOG_TARGET,
								"Enabling `PotentialFullCore` mode for extrinsic",
							);

							*mode = Some(BlockWeightMode::<Config>::potential_full_core(
								// While applying inherents `extrinsic_index` and `first_transaction_index` will be `None`.
								// When the first transaction is applied, we want to store the index.
								first_transaction_index.or(transaction_index),
								target_weight,
							));
						} else {
							log::trace!(
								target: LOG_TARGET,
								"Transaction is over the block limit, but is either outside of the allowed window or the dispatch class is not allowed.",
							);

							return Err(InvalidTransaction::ExhaustsResources)
						}
					} else if is_potential {
						log::trace!(
							target: LOG_TARGET,
							"Resetting back to `FractionOfCore`"
						);
						*mode =
							Some(BlockWeightMode::<Config>::fraction_of_core(first_transaction_index.or(transaction_index)));
					} else {
						log::trace!(
							target: LOG_TARGET,
							"Not changing block weight mode"
						);

						*mode =
							Some(BlockWeightMode::<Config>::fraction_of_core(first_transaction_index.or(transaction_index)));
					}
				},
			};

			Ok(())
		}).map_err(Into::into)
	}

	/// Should be called after all inner extensions have finished executing their post dispatch
	/// handling.
	///
	/// Returns the weight to refund. Aka the weight that wasn't used by this extension.
	fn post_dispatch_extrinsic(info: &DispatchInfo) -> Weight {
		crate::BlockWeightMode::<Config>::mutate(|weight_mode| {
			let Some(mode) = weight_mode else { return Weight::zero() };

			match mode {
				// If the previous mode was already `FullCore`, we are fine.
				BlockWeightMode::<Config>::FullCore { .. } =>
					Config::WeightInfo::block_weight_tx_extension_max_weight()
						.saturating_sub(Config::WeightInfo::block_weight_tx_extension_full_core()),
				BlockWeightMode::<Config>::FractionOfCore { .. } => {
					let digest = frame_system::Pallet::<Config>::digest();
					let target_block_weight =
						MaxParachainBlockWeight::<Config, TargetBlockRate>::target_block_weight_with_digest(&digest);

					let is_above_limit = frame_system::Pallet::<Config>::remaining_block_weight()
						.consumed()
						.any_gt(target_block_weight);

					// If we are above the limit, it means the transaction used more weight than
					// what it had announced, which should not happen.
					if is_above_limit {
						log::error!(
							target: LOG_TARGET,
							"Extrinsic ({}) used more weight than what it had announced and pushed the \
							block above the allowed weight limit!",
							frame_system::Pallet::<Config>::extrinsic_index().unwrap_or_default()
						);

						// If this isn't the first block in a core, we register the full core weight
						// to ensure that we don't include any other transactions. Because we don't
						// know how many weight of the core was already used by the blocks before.
						if !is_first_block_in_core_with_digest(&digest).unwrap_or(false) {
							log::error!(
								target: LOG_TARGET,
								"Registering `FULL_CORE_WEIGHT` to ensure no other transaction is included \
								in this block, because this isn't the first block in the core!",
							);

							frame_system::Pallet::<Config>::register_extra_weight_unchecked(
								FULL_CORE_WEIGHT,
								DispatchClass::Mandatory,
							);
						}

						*weight_mode = Some(BlockWeightMode::<Config>::full_core());

						// Inform the node that this block uses the full core.
						frame_system::Pallet::<Config>::deposit_log(
							CumulusDigestItem::UseFullCore.to_digest_item(),
						);
					}

					Config::WeightInfo::block_weight_tx_extension_max_weight().saturating_sub(
						Config::WeightInfo::block_weight_tx_extension_stays_fraction_of_core(),
					)
				},
				// Now we need to check if the transaction required more weight than a fraction of a
				// core block.
				BlockWeightMode::<Config>::PotentialFullCore {
					first_transaction_index,
					target_weight,
					..
				} => {
					let block_weight = frame_system::BlockWeight::<Config>::get();
					let extrinsic_class_weight = block_weight.get(info.class);

					if extrinsic_class_weight.any_gt(*target_weight) {
						log::trace!(
							target: LOG_TARGET,
							"Extrinsic class weight {extrinsic_class_weight:?} above target weight {target_weight:?}, enabling `FullCore` mode."
						);

						*weight_mode = Some(BlockWeightMode::<Config>::full_core());

						// Inform the node that this block uses the full core.
						frame_system::Pallet::<Config>::deposit_log(
							CumulusDigestItem::UseFullCore.to_digest_item(),
						);
					} else {
						log::trace!(
							target: LOG_TARGET,
							"Extrinsic class weight {extrinsic_class_weight:?} not above target \
							weight {target_weight:?}, going back to `FractionOfCore` mode."
						);

						*weight_mode = Some(BlockWeightMode::<Config>::fraction_of_core(
							*first_transaction_index,
						));
					}

					// We run into the worst case, so no refund :)
					Weight::zero()
				},
			}
		})
	}
}

impl<
		Config,
		Inner,
		TargetBlockRate,
		const MAX_TRANSACTION_TO_CONSIDER: u32,
		const ALLOW_NORMAL: bool,
	> From<Inner>
	for DynamicMaxBlockWeight<
		Config,
		Inner,
		TargetBlockRate,
		MAX_TRANSACTION_TO_CONSIDER,
		ALLOW_NORMAL,
	>
{
	fn from(s: Inner) -> Self {
		Self::new(s)
	}
}

impl<
		Config,
		Inner: core::fmt::Debug,
		TargetBlockRate,
		const MAX_TRANSACTION_TO_CONSIDER: u32,
		const ALLOW_NORMAL: bool,
	> core::fmt::Debug
	for DynamicMaxBlockWeight<
		Config,
		Inner,
		TargetBlockRate,
		MAX_TRANSACTION_TO_CONSIDER,
		ALLOW_NORMAL,
	>
{
	fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> Result<(), core::fmt::Error> {
		write!(f, "DynamicMaxBlockWeight<{:?}>", self.0)
	}
}

impl<
		Config: crate::Config + Send + Sync,
		Inner: TransactionExtension<Config::RuntimeCall>,
		TargetBlockRate: Get<u32> + Send + Sync + 'static,
		const MAX_TRANSACTION_TO_CONSIDER: u32,
		const ALLOW_NORMAL: bool,
	> TransactionExtension<Config::RuntimeCall>
	for DynamicMaxBlockWeight<
		Config,
		Inner,
		TargetBlockRate,
		MAX_TRANSACTION_TO_CONSIDER,
		ALLOW_NORMAL,
	>
where
	Config::RuntimeCall: Dispatchable<Info = DispatchInfo, PostInfo = PostDispatchInfo>,
{
	const IDENTIFIER: &'static str = "DynamicMaxBlockWeight<Use `metadata()`!>";

	type Implicit = Inner::Implicit;

	type Val = Inner::Val;

	type Pre = Inner::Pre;

	fn implicit(&self) -> Result<Self::Implicit, TransactionValidityError> {
		self.0.implicit()
	}

	fn metadata() -> Vec<sp_runtime::traits::TransactionExtensionMetadata> {
		let mut inner = Inner::metadata();
		inner.push(sp_runtime::traits::TransactionExtensionMetadata {
			identifier: "DynamicMaxBlockWeight",
			ty: scale_info::meta_type::<()>(),
			implicit: scale_info::meta_type::<()>(),
		});
		inner
	}

	fn weight(&self, call: &Config::RuntimeCall) -> Weight {
		Config::WeightInfo::block_weight_tx_extension_max_weight()
			.saturating_add(self.0.weight(call))
	}

	fn validate(
		&self,
		origin: Config::RuntimeOrigin,
		call: &Config::RuntimeCall,
		info: &DispatchInfoOf<Config::RuntimeCall>,
		len: usize,
		self_implicit: Self::Implicit,
		inherited_implication: &impl Implication,
		source: TransactionSource,
	) -> Result<(ValidTransaction, Self::Val, Config::RuntimeOrigin), TransactionValidityError> {
		Self::pre_validate_extrinsic(info, len)?;

		self.0
			.validate(origin, call, info, len, self_implicit, inherited_implication, source)
	}

	fn prepare(
		self,
		val: Self::Val,
		origin: &Config::RuntimeOrigin,
		call: &Config::RuntimeCall,
		info: &DispatchInfoOf<Config::RuntimeCall>,
		len: usize,
	) -> Result<Self::Pre, TransactionValidityError> {
		self.0.prepare(val, origin, call, info, len)
	}

	fn post_dispatch_details(
		pre: Self::Pre,
		info: &DispatchInfoOf<Config::RuntimeCall>,
		post_info: &PostDispatchInfo,
		len: usize,
		result: &DispatchResult,
	) -> Result<Weight, TransactionValidityError> {
		let weight_refund = Inner::post_dispatch_details(pre, info, post_info, len, result)?;

		let extra_refund = Self::post_dispatch_extrinsic(info);

		Ok(weight_refund.saturating_add(extra_refund))
	}

	fn bare_validate(
		call: &Config::RuntimeCall,
		info: &DispatchInfoOf<Config::RuntimeCall>,
		len: usize,
	) -> frame_support::pallet_prelude::TransactionValidity {
		Self::pre_validate_extrinsic(info, len)?;

		Inner::bare_validate(call, info, len)
	}

	fn bare_validate_and_prepare(
		call: &Config::RuntimeCall,
		info: &DispatchInfoOf<Config::RuntimeCall>,
		len: usize,
	) -> Result<(), TransactionValidityError> {
		Self::pre_validate_extrinsic(info, len)?;

		Inner::bare_validate_and_prepare(call, info, len)
	}

	fn bare_post_dispatch(
		info: &DispatchInfoOf<Config::RuntimeCall>,
		post_info: &mut PostDispatchInfoOf<Config::RuntimeCall>,
		len: usize,
		result: &DispatchResult,
	) -> Result<(), TransactionValidityError> {
		Inner::bare_post_dispatch(info, post_info, len, result)?;

		Self::post_dispatch_extrinsic(info);

		Ok(())
	}
}
