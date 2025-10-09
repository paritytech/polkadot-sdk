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
	block_weight_over_target_block_weight, is_first_block_in_core, BlockWeightMode, LOG_TARGET,
};
use crate::max_parachain_block_weight::MaxParachainBlockWeight;
use cumulus_primitives_core::CumulusDigestItem;
use frame_support::traits::PreInherents;
use sp_core::Get;

/// A pre-inherent hook that may increases max block weight after `on_initialize`.
///
/// The hook is called before applying the first inherent. It checks the used block weight of
/// `on_initialize`. If the used block weight is above the target block weight, the hook will allow
/// the block to use the weight of a full core. It also sets the [`CumulusDigestItem::UseFullCore`]
/// digest.
pub struct DynamicMaxBlockWeightHooks<Config, TargetBlockRate>(
	pub core::marker::PhantomData<(Config, TargetBlockRate)>,
);

impl<Config, TargetBlockRate> PreInherents for DynamicMaxBlockWeightHooks<Config, TargetBlockRate>
where
	Config: crate::Config,
	TargetBlockRate: Get<u32>,
{
	fn pre_inherents() {
		if !block_weight_over_target_block_weight::<Config, TargetBlockRate>() {
			return
		}

		let is_first_block_in_core = is_first_block_in_core::<Config>();

		if !is_first_block_in_core {
			log::error!(
				target: LOG_TARGET,
				"Inherent block logic took longer than the target block weight, THIS IS A BUG!!!",
			);

			// We are already above the allowed maximum and do not want to accept any more
			// extrinsics.
			frame_system::Pallet::<Config>::register_extra_weight_unchecked(
				MaxParachainBlockWeight::<Config>::FULL_CORE_WEIGHT,
				frame_support::dispatch::DispatchClass::Mandatory,
			);
		} else {
			log::debug!(
				target: LOG_TARGET,
				"Inherent block logic took longer than the target block weight, going to use the full core",
			);
		}

		crate::BlockWeightMode::<Config>::put(BlockWeightMode::FullCore);

		// Inform the node that this block uses the full core.
		frame_system::Pallet::<Config>::deposit_log(
			CumulusDigestItem::UseFullCore.to_digest_item(),
		);
	}
}
