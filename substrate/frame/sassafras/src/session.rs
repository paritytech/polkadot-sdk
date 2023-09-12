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

//! Sassafras implementation of traits required by session pallet.

use super::*;
use frame_support::traits::{EstimateNextSessionRotation, Hooks, OneSessionHandler};
use pallet_session::ShouldEndSession;
use sp_runtime::{traits::SaturatedConversion, Permill};

impl<T: Config> ShouldEndSession<BlockNumberFor<T>> for Pallet<T> {
	fn should_end_session(now: BlockNumberFor<T>) -> bool {
		// It might be (and it is in current implementation) that session module is calling
		// `should_end_session` from it's own `on_initialize` handler, in which case it's
		// possible that Sassafras's own `on_initialize` has not run yet, so let's ensure that we
		// have initialized the pallet and updated the current slot.
		Self::on_initialize(now);
		Self::should_end_epoch(now)
	}
}

impl<T: Config> OneSessionHandler<T::AccountId> for Pallet<T> {
	type Key = AuthorityId;

	fn on_genesis_session<'a, I: 'a>(validators: I)
	where
		I: Iterator<Item = (&'a T::AccountId, AuthorityId)>,
	{
		let authorities: Vec<_> = validators.map(|(_, k)| k).collect();
		Self::initialize_genesis_authorities(&authorities);
	}

	fn on_new_session<'a, I: 'a>(_changed: bool, validators: I, queued_validators: I)
	where
		I: Iterator<Item = (&'a T::AccountId, AuthorityId)>,
	{
		let authorities = validators.map(|(_account, k)| k).collect();
		let bounded_authorities = WeakBoundedVec::<_, T::MaxAuthorities>::force_from(
			authorities,
			Some(
				"Warning: The session has more validators than expected. \
				A runtime configuration adjustment may be needed.",
			),
		);

		let next_authorities = queued_validators.map(|(_account, k)| k).collect();
		let next_bounded_authorities = WeakBoundedVec::<_, T::MaxAuthorities>::force_from(
			next_authorities,
			Some(
				"Warning: The session has more queued validators than expected. \
				A runtime configuration adjustment may be needed.",
			),
		);

		Self::enact_epoch_change(bounded_authorities, next_bounded_authorities)
	}

	fn on_disabled(i: u32) {
		Self::deposit_consensus(ConsensusLog::OnDisabled(i))
	}
}

impl<T: Config> EstimateNextSessionRotation<BlockNumberFor<T>> for Pallet<T> {
	fn average_session_length() -> BlockNumberFor<T> {
		T::EpochDuration::get().saturated_into()
	}

	fn estimate_current_session_progress(_now: BlockNumberFor<T>) -> (Option<Permill>, Weight) {
		let elapsed_slots = Self::current_slot_index() + 1;
		let progress = Permill::from_rational(elapsed_slots, T::EpochDuration::get());
		// DB-Reads: CurrentSlot, GenesisSlot, EpochIndex, EpochDuration
		(Some(progress), T::DbWeight::get().reads(4))
	}

	/// Return the best guess block number at which the next epoch change is predicted to happen.
	///
	/// This is only accurate if no slots are missed. Given missed slots, the slot number will grow
	/// while the block number will not. Hence, the result can be interpreted as an upper bound.
	fn estimate_next_session_rotation(
		now: BlockNumberFor<T>,
	) -> (Option<BlockNumberFor<T>>, Weight) {
		let current_slot = Self::current_slot_index();
		let remaining = T::EpochDuration::get().saturating_sub(current_slot);
		let upper_bound: BlockNumberFor<T> = now.saturating_add(remaining.saturated_into());
		// DB-Reads: CurrentSlot, GenesisSlot, EpochIndex, EpochDuration
		(Some(upper_bound), T::DbWeight::get().reads(4))
	}
}
