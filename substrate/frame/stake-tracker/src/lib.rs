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

//! # StakeTracker
//!
//! FRAME stake tracker pallet
//!
//! ## Overview
//!
//! The stake-tracker pallet listens to staking events through implementing the
//! [`OnStakingUpdate`] trait and forwards those events to one or multiple types (e.g. pallets) that
//! must be kept track of the stake and staker's state. The pallet does not expose any
//! callables and acts as a multiplexer of staking events.
//!
//! Currently, the stake tracker pallet is used to update a voter and target sorted target list
//! implemented through the bags lists pallet.
//!
//! ## Goals
//!
//! The [`OnStakingUpdate`] implementation aims at achieving the following goals:
//!
//! * The [`Config::TargetList`] keeps a *lazily* sorted list of validators, sorted by approvals
//! (which include self-vote and nominations).
//! * The [`Config::VoterList`] keeps a sorted list of voters, sorted by bonded stake.
//! * The [`Config::TargetList`] sorting must be *always* kept up to date, even in the event of new
//! nomination updates, nominator/validator slashes and rewards.
//!
//! Note that from the POV of this pallet, all events will result in one or multiple
//! updates to the [`Config::VoterList`] and/or [`Config::TargetList`] state. If a update or set of
//! updates require too much weight to process (e.g. at nominator's rewards payout or at nominator's
//! slashes), the event emitter should handle that in some way (e.g. buffering events).
//!
//! ## Event emitter ordering and staking ledger state updates
//!
//! It is important to ensure that the events are emitted from staking (i.e. the calls into
//! [`OnStakingUpdate`]) *after* the caller ensures that the state of the staking ledger is up to
//! date, since the new state will be fetched and used to update the sorted lists accordingly.

#![cfg_attr(not(feature = "std"), no_std)]

pub use pallet::*;

use frame_election_provider_support::SortedListProvider;
use frame_support::traits::{Currency, Defensive};
use sp_staking::{
	currency_to_vote::CurrencyToVote, OnStakingUpdate, StakerStatus, StakingInterface,
};
use sp_std::collections::btree_map::BTreeMap;

#[cfg(test)]
mod mock;
#[cfg(test)]
mod tests;

/// The balance type of this pallet.
pub type BalanceOf<T> = <<T as Config>::Staking as StakingInterface>::Balance;
/// The account ID of this pallet.
pub type AccountIdOf<T> = <T as frame_system::Config>::AccountId;

/// Represents a stake imbalance to be applied to a staker's score.
#[derive(Copy, Clone, Debug)]
pub enum StakeImbalance<Balance> {
	// Represents the reduction of stake by `Balance`.
	Negative(Balance),
	// Represents the increase of stake by `Balance`.
	Positive(Balance),
}

#[frame_support::pallet]
pub mod pallet {
	use crate::*;
	use frame_election_provider_support::VoteWeight;
	use frame_support::pallet_prelude::*;

	/// The current storage version.
	const STORAGE_VERSION: StorageVersion = StorageVersion::new(0);

	#[pallet::pallet]
	#[pallet::storage_version(STORAGE_VERSION)]
	pub struct Pallet<T>(_);

	#[pallet::config]
	pub trait Config: frame_system::Config {
		type Currency: Currency<Self::AccountId, Balance = BalanceOf<Self>>;

		/// The staking interface.
		type Staking: StakingInterface<AccountId = Self::AccountId>;

		/// Something that provides a best-effort sorted list of voters.
		///
		/// To keep the load off the chain as much as possible, changes made to the staked amount
		/// via rewards and slashes are dropped and thus need to be manually updated through
		/// extrinsics. In case of `bags-list`, this always means using `rebag` and `putInFrontOf`.
		type VoterList: SortedListProvider<Self::AccountId, Score = VoteWeight>;

		/// Something that provides a best-effort sorted list of targets.
		///
		/// Note that while at the time of nomination all targets are checked to be real
		/// validators, they can chill at any point, and their approval stakes will still be
		/// recorded. This implies that what comes out of iterating this list MIGHT NOT BE AN ACTIVE
		/// VALIDATOR.
		type TargetList: SortedListProvider<Self::AccountId, Score = VoteWeight>;
	}

	impl<T: Config> Pallet<T> {
		/// Returns the vote weight of a staker based on its current *active* stake, as returned by
		/// the staking interface.
		pub(crate) fn active_vote_of(who: &T::AccountId) -> VoteWeight {
			T::Staking::stake(who)
				.map(|s| Self::to_vote(s.active))
				.defensive_unwrap_or_default()
		}

		/// Converts a staker's balance to its vote weight.
		pub(crate) fn to_vote(balance: BalanceOf<T>) -> VoteWeight {
			<T::Staking as StakingInterface>::CurrencyToVote::to_vote(
				balance,
				T::Currency::total_issuance(),
			)
		}

		/// Updates a staker's score by increasing/decreasing an imbalance of the current score in
		/// the list.
		pub(crate) fn update_score<L>(who: &T::AccountId, imbalance: StakeImbalance<VoteWeight>)
		where
			L: SortedListProvider<AccountIdOf<T>, Score = VoteWeight>,
		{
			// there may be nominators who nominate a non-existant validator. if that's the case,
			// move on.
			if !L::contains(who) {
				return
			}

			match imbalance {
				StakeImbalance::Positive(imbalance) => {
					let _ = L::on_increase(who, imbalance).defensive_proof(
						"staker should exist in the list, otherwise returned earlier.",
					);
				},
				StakeImbalance::Negative(imbalance) => {
					let current_score = L::get_score(who)
						.expect("staker exists in the list as per the check above; qed.");

					// if decreasing the imbalance makes the score lower than 0, the node will be
					// removed from the list when calling `L::on_decrease`, which is not expected.
					// Instead, we call `L::on_update` to set the score as 0. The node will be
					// removed when `on_*_removed` is called.
					if current_score.saturating_sub(imbalance) == 0 {
						let _ = L::on_update(who, 0).defensive_proof(
							"staker exists in the list, otherwise returned earlier.",
						);
					} else {
						let _ = L::on_decrease(who, imbalance)
							.defensive_proof("staker exists in the list as per the check above.");
					}
				},
			}
		}
	}
}

impl<T: Config> OnStakingUpdate<T::AccountId, BalanceOf<T>> for Pallet<T> {
	// Fired when the stake amount of someone updates.
	//
	// When a nominator's stake is updated, all the nominated targets must be updated accordingly.
	//
	// Note: it is assumed that who's staking state is updated *before* this method is called.
	fn on_stake_update(who: &T::AccountId, prev_stake: Option<sp_staking::Stake<BalanceOf<T>>>) {
		if let Ok(stake) = T::Staking::stake(who) {
			let voter_weight = Self::to_vote(stake.active);

			match T::Staking::status(who).defensive_unwrap_or(StakerStatus::Idle) {
				StakerStatus::Nominator(nominations) => {
					let _ = T::VoterList::on_update(who, voter_weight).defensive_proof(
						"staker should exist in VoterList, as per the contract \
                            with staking.",
					);

					// calculate imbalace to update the score of nominated targets.
					let stake_imbalance = if let Some(prev_stake) = prev_stake {
						let prev_voter_weight = Self::to_vote(prev_stake.active);

						if prev_voter_weight > voter_weight {
							StakeImbalance::Negative(prev_voter_weight - voter_weight)
						} else {
							StakeImbalance::Positive(voter_weight - prev_voter_weight)
						}
					} else {
						// if nominator had no stake before update, then add all the voter weight
						// to the target's score.
						StakeImbalance::Positive(voter_weight)
					};

					// updates vote weight of nominated targets accordingly. Note: this will update
					// the score of up to `T::MaxNominations` validators.
					for target in nominations.into_iter() {
						// target may be chilling due to a recent slash, verify if it is active
						// before updating the score.
						if <T::Staking as StakingInterface>::is_validator(&target) {
							Self::update_score::<T::TargetList>(&target, stake_imbalance);
						}
					}
				},
				StakerStatus::Validator => {
					// validator is both a target and a voter.
					let _ = T::TargetList::on_update(who, voter_weight).defensive_proof(
						"staker should exist in TargetList, as per the contract \
                            with staking.",
					);
					let _ = T::VoterList::on_update(who, voter_weight).defensive_proof(
						"the staker should exit in VoterList, as per the \
                            contract with staking.",
					);
				},
				StakerStatus::Idle => (), // nothing to see here.
			}
		}
	}

	// Fired when someone sets their intention to nominate.
	//
	// Note: it is assumed that who's staking state is updated *before* this method is called.
	fn on_nominator_add(who: &T::AccountId) {
		let nominator_vote = Self::active_vote_of(who);

		let _ = T::VoterList::on_insert(who.clone(), nominator_vote).defensive_proof(
			"staker should not exist in VoterList, as per the contract with staking.",
		);

		// If who is a nominator, update the vote weight of the nominations if they exist. Note:
		// this will update the score of up to `T::MaxNominations` validators.
		match T::Staking::status(who) {
			Ok(StakerStatus::Nominator(nominations)) =>
				for t in nominations {
					Self::update_score::<T::TargetList>(
						&t,
						StakeImbalance::Positive(nominator_vote),
					)
				},
			Ok(StakerStatus::Idle) | Ok(StakerStatus::Validator) | Err(_) => (), // nada.
		};
	}

	// Fired when someone sets their intention to validate.
	//
	// A validator is also considered a voter with self-vote and should be added to
	// [`Config::VoterList`].
	//
	// Note: it is assumed that who's staking state is updated *before* calling this method.
	fn on_validator_add(who: &T::AccountId) {
		let _ = T::TargetList::on_insert(who.clone(), Self::active_vote_of(who)).defensive_proof(
			"staker should not exist in TargetList, as per the contract with staking.",
		);

		// a validator is also a nominator.
		Self::on_nominator_add(who)
	}

	// Fired when someone removes their intention to nominate, either due to chill or validating.
	//
	// Note: it is assumed that who's staking state is updated *before* the caller calling into
	// this method. Thus, the nominations before the nominator has been removed from staking are
	// passed in, so that the target list can be updated accordingly.
	fn on_nominator_remove(who: &T::AccountId, nominations: Vec<T::AccountId>) {
		let nominator_vote = Self::active_vote_of(who);

		// updates the nominated target's score. Note: this may update the score of up to
		// `T::MaxNominations` validators.
		for t in nominations.iter() {
			Self::update_score::<T::TargetList>(&t, StakeImbalance::Negative(nominator_vote))
		}

		let _ = T::VoterList::on_remove(&who).defensive_proof(
			"the nominator exists in the list as per the contract with staking; qed.",
		);
	}

	// Fired when someone removes their intention to validate, either due to chill or nominating.
	fn on_validator_remove(who: &T::AccountId) {
		let _ = T::TargetList::on_remove(&who).defensive_proof(
			"the validator exists in the list as per the contract with staking; qed.",
		);

		// validator is also a nominator.
		Self::on_nominator_remove(who, vec![]);
	}

	// Fired when an existing nominator updates their nominations.
	//
	// This is called when a nominator updates their nominations. The nominator's stake remains the
	// same (updates to the nominator's stake should emit [`Self::on_stake_update`] instead).
	// However, the score of the nominated targets must be updated accordingly.
	//
	// Note: it is assumed that who's staking state is updated *before* calling this method.
	fn on_nominator_update(who: &T::AccountId, prev_nominations: Vec<T::AccountId>) {
		let nominator_vote = Self::active_vote_of(who);
		let curr_nominations =
			<T::Staking as StakingInterface>::nominations(&who).unwrap_or_default();

		// new nominations
		for target in curr_nominations.iter() {
			if !prev_nominations.contains(target) {
				Self::update_score::<T::TargetList>(
					&target,
					StakeImbalance::Positive(nominator_vote),
				);
			}
		}
		// removed nominations
		for target in prev_nominations.iter() {
			if !curr_nominations.contains(target) {
				Self::update_score::<T::TargetList>(
					&target,
					StakeImbalance::Negative(nominator_vote),
				);
			}
		}
	}

	// noop: the updates to target and voter lists when applying a slash are performed
	// through [`Self::on_nominator_remove`] and [`Self::on_validator_remove`] when the stakers are
	// chilled. When the slash is applied, the ledger is updated, thus the stake is propagated
	// through the `[Self::update::<T::EventListener>]`.
	fn on_slash(
		_stash: &T::AccountId,
		_slashed_active: BalanceOf<T>,
		_slashed_unlocking: &BTreeMap<sp_staking::EraIndex, BalanceOf<T>>,
		_slashed_total: BalanceOf<T>,
	) {
		#[cfg(any(std, no_std))]
		frame_support::defensive!("unexpected call to OnStakingUpdate::on_slash");
	}
}
