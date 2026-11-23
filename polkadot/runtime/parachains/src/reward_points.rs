// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Polkadot.

// Polkadot is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Polkadot is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Polkadot.  If not, see <http://www.gnu.org/licenses/>.

//! An implementation of the `RewardValidators` trait used by `inclusion` that employs
//! `pallet-staking` to compute the rewards.
//!
//! Based on <https://research.web3.foundation/en/latest/polkadot/overview/2-token-economics.html>
//! which doesn't currently mention availability bitfields. As such, we don't reward them
//! for the time being, although we will build schemes to do so in the future.

use crate::{session_info, shared};
use alloc::collections::btree_set::BTreeSet;
use frame_support::traits::{Defensive, Get, RewardsReporter, ValidatorSet};
use polkadot_primitives::{SessionIndex, ValidatorIndex};

/// Rewards validators for participating in parachains with era points in pallet-staking.
///
/// `PointsPerWork` is the amount of era points given per unit of work. The unit of work depends on
/// where this type is used:
///
/// - As `inclusion::Config::RewardValidators`: backing a candidate that is included.
/// - As `disputes::Config::RewardValidators`: casting a dispute statement on a candidate.
///
/// Use a separate instance with a different `PointsPerWork` for each pallet to reward them
/// differently.
pub struct RewardValidatorsWithEraPoints<C, R, PointsPerWork>(
	core::marker::PhantomData<(C, R, PointsPerWork)>,
);

impl<C, R, PointsPerWork> RewardValidatorsWithEraPoints<C, R, PointsPerWork>
where
	C: session_info::Config,
	C::ValidatorSet: ValidatorSet<C::AccountId, ValidatorId = C::AccountId>,
	R: RewardsReporter<C::AccountId>,
	PointsPerWork: Get<u32>,
{
	/// Reward validators in session with points, but only if they are in the active set.
	fn reward_only_active(
		session_index: SessionIndex,
		indices: impl IntoIterator<Item = ValidatorIndex>,
	) {
		let points = PointsPerWork::get();
		let validators = session_info::AccountKeys::<C>::get(&session_index);
		let validators = match validators
			.defensive_proof("account_keys are present for dispute_period sessions")
		{
			Some(validators) => validators,
			None => return,
		};
		// limit rewards to the active validator set
		let active_set: BTreeSet<_> = C::ValidatorSet::validators().into_iter().collect();

		let rewards = indices
			.into_iter()
			.filter_map(|i| validators.get(i.0 as usize).cloned())
			.filter(|v| active_set.contains(v))
			.map(|v| (v, points));

		R::reward_by_ids(rewards);
	}
}

impl<C, R, PointsPerWork> crate::inclusion::RewardValidators
	for RewardValidatorsWithEraPoints<C, R, PointsPerWork>
where
	C: shared::Config + session_info::Config,
	C::ValidatorSet: ValidatorSet<C::AccountId, ValidatorId = C::AccountId>,
	R: RewardsReporter<C::AccountId>,
	PointsPerWork: Get<u32>,
{
	fn reward_backing(indices: impl IntoIterator<Item = ValidatorIndex>) {
		let session_index = shared::CurrentSessionIndex::<C>::get();
		Self::reward_only_active(session_index, indices);
	}

	fn reward_bitfields(_validators: impl IntoIterator<Item = ValidatorIndex>) {}
}

impl<C, R, PointsPerWork> crate::disputes::RewardValidators
	for RewardValidatorsWithEraPoints<C, R, PointsPerWork>
where
	C: session_info::Config,
	C::ValidatorSet: ValidatorSet<C::AccountId, ValidatorId = C::AccountId>,
	R: RewardsReporter<C::AccountId>,
	PointsPerWork: Get<u32>,
{
	fn reward_dispute_statement(
		session: SessionIndex,
		validators: impl IntoIterator<Item = ValidatorIndex>,
	) {
		Self::reward_only_active(session, validators);
	}
}
