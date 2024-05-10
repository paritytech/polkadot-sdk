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

//! # Stake Tracker Pallet
//!
//! The stake-tracker pallet is responsible to keep track of the voter's stake and target's approval
//! voting in the staking system.
//!
//! ## Overview
//!
//! The stake-tracker pallet listens to staking events through implementing the [`OnStakingUpdate`]
//! trait. Based on the emitted events, the goal of this pallet is to maintain a **strictly**
//! sorted list of targets by approval voting. This pallet may also update a voter list, based on
//! the configurations.
//!
//! For the voter list, the [`crate::VoterUpdateMode`] defines the type of sortition of the list,
//! namely:
//!
//! - [`crate::VoterUpdateMode::Lazy`]: will skip the score update in the voter list.
//! - [`crate::VoterUpdateMode::Strict`]: will ensure that the score updates are kept sorted
//! for the corresponding list. In this case, the [`Config::VoterList`] is *strictly*
//! sorted* by [`SortedListProvider::Score`] (note: from the time the sorting mode is strict).
//!
//! Note that insertions and removals of voter nodes will be executed regardless of the sorting
//! mode.
//!
//! ## Goals
//!
//! Note: these goals are assuming the both target list and sorted lists have
//! [`crate::VoterUpdateMode::Strict`] set.
//!
//! The [`OnStakingUpdate`] implementation (in strict mode) aims to achieve the following goals:
//!
//! * The [`Config::TargetList`] keeps a sorted list of validators, *strictly* sorted by approvals
//! (which include self-vote and nominations' stake).
//! * The [`Config::VoterList`] keeps a list of voters, *stricly* sorted by bonded stake if it has
//! [`crate::VoterUpdateMode::Strict`] mode enabled, otherwise the list is kept lazily sorted.
//! * The [`Config::TargetList`] sorting must be *always* kept up to date, even in the event of new
//! nomination updates, nominator/validator slashes and rewards. This pallet *must* ensure that the
//! scores of the targets and voters are always up to date and thus, that the targets and voters in
//! the lists are sorted by score at all time.
//!
//! Note that from the POV of this pallet, staking actions may result in one or multiple updates to
//! [`Config::VoterList`] and/or [`Config::TargetList`] state. If a set of staking updates require
//! too much weight to execute (e.g. at nominator's rewards payout or at slashes), the event emitter
//! should handle that in some way (e.g. buffering events and implementing a multi-block event
//! emitter).
//!
//! ## Staker status and list invariants
//!
//! Note: these goals are assuming the both target list and sorted lists have
//! [`crate::VoterUpdateMode::Strict`] set.
//!
//! * A [`sp_staking::StakerStatus::Nominator`] is part of the voter list and its self-stake is the
//! voter list's score. In addition, if the `VoterList` is in strict mode, the voters' scores are up
//! to date with the current stake returned by [`T::Staking::stake`].
//! * A [`sp_staking::StakerStatus::Validator`] is part of both voter and target list. In addition,
//! if the `TargetList` is in strict mode, its
//! approvals score (nominations + self-stake) is kept up to date as the target list's score.
//! * A [`sp_staking::StakerStatus::Idle`] may have a target list's score while other stakers
//!   nominate the idle validator.
//! * A "dangling" target, which is not an active staker anymore (i.e. not bonded), may still have
//!   an associated target list score. This may happen when active nominators are still nominating
//!   the target after the validator unbonded. The target list's node and score will automatically
//!   be removed onced all the voters stop nominating the unbonded account (i.e. the target's score
//!   drops to 0).
//!
//! For further details on the target list invariante, refer to [`Self`::do_try_state_approvals`]
//! and [`Self::do_try_state_target_sorting`].
//!
//! ## Event emitter ordering and staking ledger state updates
//!
//! It is important to ensure that the events are emitted from staking (i.e. the calls into
//! [`OnStakingUpdate`]) *after* the staking ledger has been updated by the caller, since the new
//! state will be fetched and used to update the sorted lists accordingly.

#![cfg_attr(not(feature = "std"), no_std)]

pub use pallet::*;

use frame_election_provider_support::SortedListProvider;
use frame_support::{
	defensive,
	pallet_prelude::*,
	traits::{fungible::Inspect as FnInspect, Defensive, DefensiveSaturating},
};
use sp_runtime::traits::{Saturating, Zero};
use sp_staking::{
	currency_to_vote::CurrencyToVote, OnStakingUpdate, Stake, StakerStatus, StakingInterface,
};
use sp_std::{collections::btree_map::BTreeMap, vec, vec::Vec};

#[cfg(test)]
pub(crate) mod mock;
#[cfg(test)]
mod tests;

/// The balance type of this pallet.
pub type BalanceOf<T> = <<T as Config>::Staking as StakingInterface>::Balance;
/// The account ID of this pallet.
pub type AccountIdOf<T> = <T as frame_system::Config>::AccountId;

/// Represents a stake imbalance to be applied to a staker's score.
#[derive(Copy, Clone, Debug)]
pub enum StakeImbalance<Score> {
	/// Represents the reduction of stake by `Score`.
	Negative(Score),
	/// Represents the increase of stake by `Score`.
	Positive(Score),
}

impl<Score: PartialOrd + DefensiveSaturating> StakeImbalance<Score> {
	/// Constructor for a stake imbalance instance based on the previous and next score.
	fn from(prev: Score, new: Score) -> Self {
		if prev > new {
			StakeImbalance::Negative(prev.defensive_saturating_sub(new))
		} else {
			StakeImbalance::Positive(new.defensive_saturating_sub(prev))
		}
	}
}

/// Defines the sorting mode of sorted list providers.
#[derive(Copy, Clone, Debug)]
pub enum VoterUpdateMode {
	/// All score update events will be automatically reflected in the sorted list.
	Strict,
	/// Score update events are *not* be automatically reflected in the sorted list. Howeber, node
	/// insertion and removals are reflected in the list.
	Lazy,
}

impl VoterUpdateMode {
	fn is_strict_mode(&self) -> bool {
		matches!(self, Self::Strict)
	}
}

#[frame_support::pallet]
pub mod pallet {
	use crate::*;
	use frame_election_provider_support::{ExtendedBalance, VoteWeight};
	use frame_system::pallet_prelude::BlockNumberFor;

	/// The current storage version.
	const STORAGE_VERSION: StorageVersion = StorageVersion::new(0);

	#[pallet::pallet]
	#[pallet::storage_version(STORAGE_VERSION)]
	pub struct Pallet<T>(_);

	#[pallet::config]
	pub trait Config: frame_system::Config {
		/// The stake balance.
		type Currency: FnInspect<Self::AccountId, Balance = BalanceOf<Self>>;

		/// The staking interface.
		type Staking: StakingInterface<AccountId = Self::AccountId>;

		/// A sorted list provider for staking voters that is kept up to date by this pallet. The
		/// sorting by score depends on the sorting mode set by [`Self::VoterUpdateMode`].
		type VoterList: SortedListProvider<Self::AccountId, Score = VoteWeight>;

		/// A sorted list provider for staking targets that is ketp *always* sorted by the target's
		/// stake approvals.
		type TargetList: SortedListProvider<Self::AccountId, Score = ExtendedBalance>;

		/// The voter list update mode.
		type VoterUpdateMode: Get<VoterUpdateMode>;
	}

	#[pallet::hooks]
	impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
		#[cfg(feature = "try-runtime")]
		fn try_state(_n: BlockNumberFor<T>) -> Result<(), sp_runtime::TryRuntimeError> {
			Self::do_try_state()
		}
	}

	impl<T: Config> Pallet<T> {
		/// Updates the stake of a voter.
		///
		/// It must ensure that there are no duplicate nominations to prevent over-counting the
		/// stake approval.
		pub(crate) fn do_stake_update_voter(
			who: &T::AccountId,
			prev_stake: Option<Stake<BalanceOf<T>>>,
			stake: Stake<BalanceOf<T>>,
			nominations: Vec<T::AccountId>,
		) {
			let voter_weight = Self::to_vote(stake.active);

			// if voter list is in strict sorting mode, update the voter score too.
			if T::VoterUpdateMode::get().is_strict_mode() {
				let _ = T::VoterList::on_update(who, voter_weight).defensive_proof(
					"staker should exist in VoterList, as per the contract \
                            with staking.",
				);
			}

			let stake_imbalance = StakeImbalance::from(
				prev_stake.map_or(Default::default(), |s| Self::to_vote(s.active).into()),
				voter_weight.into(),
			);

			// note: this dedup can be removed once the staking pallet ensures no duplicate
			// nominations are allowed <https://github.com/paritytech/polkadot-sdk/issues/4419>.
			// TODO: replace the dedup by a debug_assert once #4419 is resolved.
			let nominations = Self::ensure_dedup(nominations);

			// updates vote weight of nominated targets accordingly. Note: this will
			// update the score of up to `T::MaxNominations` validators.
			for target in nominations.into_iter() {
				Self::update_target_score(&target, stake_imbalance);
			}
		}

		/// Updates the stake of a target.
		pub(crate) fn do_stake_update_target(
			who: &T::AccountId,
			prev_stake: Option<Stake<BalanceOf<T>>>,
			stake: Stake<BalanceOf<T>>,
		) {
			let voter_weight = Self::to_vote(stake.active).into();
			let stake_imbalance = StakeImbalance::from(
				prev_stake.map_or(Default::default(), |s| Self::to_vote(s.active).into()),
				voter_weight,
			);

			Self::update_target_score(who, stake_imbalance);

			// validator is both a target and a voter. update the voter score if the voter list
			// is in strict mode.
			if T::VoterUpdateMode::get().is_strict_mode() {
				let _ = T::VoterList::on_update(who, Self::to_vote(stake.active)).defensive_proof(
					"the staker should exist in VoterList, as per the \
                            contract with staking.",
				);
			}
		}

		/// Updates a target's score by increasing/decreasing an imbalance of the current score in
		/// the target list.
		pub(crate) fn update_target_score(
			who: &T::AccountId,
			imbalance: StakeImbalance<ExtendedBalance>,
		) {
			// if target list does not contain target, add it and proceed.
			if !T::TargetList::contains(who) {
				T::TargetList::on_insert(who.clone(), Zero::zero())
					.expect("staker does not exist in the list as per check above; qed.");
			}

			// update target score.
			match imbalance {
				StakeImbalance::Positive(imbalance) => {
					let _ = T::TargetList::on_increase(who, imbalance).defensive_proof(
						"staker should exist in the list, otherwise returned earlier.",
					);
				},
				StakeImbalance::Negative(imbalance) => {
					if let Ok(current_score) = T::TargetList::get_score(who) {
						let balance = current_score.saturating_sub(imbalance);

						// the target is removed from the list IFF score is 0 and the target is
						// dangling (i.e. not bonded).
						if balance.is_zero() && T::Staking::status(who).is_err() {
							let _ = T::TargetList::on_remove(who).defensive_proof(
								"staker exists in the list as per the check above; qed.",
							);
						} else {
							// update the target score without removing it.
							let _ = T::TargetList::on_update(who, balance).defensive_proof(
								"staker exists in the list as per the check above; qed.",
							);
						}
					} else {
						defensive!("unexpected: unable to fetch score from staking interface of an existent staker");
					}
				},
			};
		}

		// ------ Helpers

		/// Helper to convert the balance of a staker into its vote weight.
		pub(crate) fn to_vote(balance: BalanceOf<T>) -> VoteWeight {
			<T::Staking as StakingInterface>::CurrencyToVote::to_vote(
				balance,
				T::Currency::total_issuance(),
			)
		}

		/// Helper to fetch te active stake of a staker and convert it to vote weight.
		pub(crate) fn vote_of(who: &T::AccountId) -> VoteWeight {
			let active = T::Staking::stake(who).map(|s| s.active).defensive_unwrap_or_default();
			Self::to_vote(active)
		}

		/// Returns a dedup list of accounts.
		///
		/// Note: this dedup can be removed once the staking pallet ensures no duplicate
		/// nominations are allowed <https://github.com/paritytech/polkadot-sdk/issues/4419>.
		///
		/// TODO: replace this helper method by a debug_assert if #4419 ever prevents the nomination
		/// of duplicated target.
		pub(crate) fn ensure_dedup(mut v: Vec<T::AccountId>) -> Vec<T::AccountId> {
			use sp_std::collections::btree_set::BTreeSet;

			v.drain(..).collect::<BTreeSet<_>>().into_iter().collect::<Vec<_>>()
		}
	}
}

#[cfg(any(test, feature = "try-runtime"))]
impl<T: Config> Pallet<T> {
	/// Try-state checks for the stake-tracker pallet.
	///
	/// 1. `do_try_state_approvals`: checks the current approval stake in the target list compared
	///    with the staking state.
	/// 2. `do_try_state_target_sorting`: checks if the target list is sorted by score (approvals).
	/// 3. `do_try_state_voter_sorting`: checks if the voter list is sorted by score (stake).
	pub fn do_try_state() -> Result<(), sp_runtime::TryRuntimeError> {
		Self::do_try_state_approvals()?;
		Self::do_try_state_target_sorting()?;
		Self::do_try_state_voter_sorting()?;

		Ok(())
	}

	/// Try-state: checks if the approvals stake of the targets in the target list are correct.
	///
	/// These try-state checks generate a map with approval stake of all the targets based on
	/// the staking state of stakers in the voter and target lists. In doing so, we are able to
	/// verify that the current voter and target lists and scores are in sync with the staking
	/// data and perform other sanity checks as the approvals map is calculated.
	///
	/// NOTE: this is an expensive state check since it iterates over all the nodes in the
	/// target and voter list providers.
	///
	/// Invariants:
	///
	/// * Target List:
	///   * The sum of the calculated approvals stake is the same as the current approvals in
	///   the target list per target.
	///   * The target score of an active validator is the sum of all of its nominators' stake
	///   and the self-stake;
	///   * The target score of an idle validator (i.e. chilled) is the sum of its nominator's
	///   stake. An idle target may not be part of the target list, if it has no nominations.
	///   * The target score of a "dangling" target (ie. idle AND unbonded validator) must
	///   always be > 0. We expect the stake-tracker to have cleaned up dangling targets with 0
	///   score.
	///   * The number of target nodes in the target list matches the number of
	///   (active_validators + idle_validators + dangling_targets_score_with_score).
	///
	/// * Voter List:
	///  * The voter score is the same as the active stake of the corresponding stash.
	///  * An active validator should also be part of the voter list.
	///  * An idle validator should not be part of the voter list.
	///  * A dangling target shoud not be part of the voter list.
	pub(crate) fn do_try_state_approvals() -> Result<(), sp_runtime::TryRuntimeError> {
		let mut approvals_map: BTreeMap<AccountIdOf<T>, sp_npos_elections::ExtendedBalance> =
			BTreeMap::new();

		// build map of approvals stakes from the `VoterList` POV.
		for voter in T::VoterList::iter() {
			if let Some(nominations) = <T::Staking as StakingInterface>::nominations(&voter) {
				// sanity check.
				let active_stake = T::Staking::stake(&voter)
					.map(|s| Self::to_vote(s.active))
					.expect("active voter has bonded stake; qed.");

				// if the voter list is in strict mode, we expect the stake of the voter to match
				// the score in the list at all times. The approvals calculation also depends on
				// the sorting of the voter list:
				// * if the voter list is strictly sorted, use the nominator's scores to calculate
				// the approvals.
				// * if the voter list is lazily sorted, use the active stake of the nominator to
				// calculat the approvals.
				let stake = if T::VoterUpdateMode::get().is_strict_mode() {
					// voter list is strictly sorted, use the voter list score to calculate the
					// target's approvals.
					let score =
						<T::VoterList as SortedListProvider<AccountIdOf<T>>>::get_score(&voter)
							.map_err(|_| "nominator score must exist in voter bags list")?;

					frame_support::ensure!(
						active_stake == score,
						"voter score must be the same as its active stake"
					);

					score
				} else {
					active_stake
				};

				// update the approvals map with the voter's active stake.
				// note: must remove the dedup nominations, which is also done by the
				// stake-tracker.
				let nominations = Self::ensure_dedup(nominations);

				for nomination in nominations {
					*approvals_map.entry(nomination).or_default() +=
						stake as sp_npos_elections::ExtendedBalance;
				}
			} else {
				// if it is in the voter list but it's not a nominator, it should be a validator
				// and part of the target list.
				frame_support::ensure!(
					T::Staking::status(&voter) == Ok(StakerStatus::Validator) &&
						T::TargetList::contains(&voter),
					"wrong state of voter"
				);
				frame_support::ensure!(
					T::TargetList::contains(&voter),
					"if voter is in voter list and it's not a nominator, it must be a target"
				);
			}
		}

		// add self-vote of active targets to calculated approvals from the `TargetList` POV.
		for target in T::TargetList::iter() {
			// also checks invariant: all active targets are also voters.
			let maybe_self_stake = match T::Staking::status(&target) {
				Err(_) => {
					// if target is "dangling" (i.e unbonded but still in the `TargetList`), it
					// should NOT be part of the voter list.
					frame_support::ensure!(
						!T::VoterList::contains(&target),
						"dangling target (i.e. unbonded) should not be part of the voter list"
					);

					// if target is dangling, its target score should > 0 (otherwise it should
					// have been removed from the list).
					frame_support::ensure!(
                            T::TargetList::get_score(&target).expect("target must have score") > Zero::zero(),
                            "dangling target (i.e. unbonded) is part of the `TargetList` IFF it's approval voting > 0"
                        );
					// no self-stake and it should not be part of the target list.
					None
				},
				Ok(StakerStatus::Idle) => {
					// target is idle and not part of the voter list.
					frame_support::ensure!(
						!T::VoterList::contains(&target),
						"chilled validator (idle target) should not be part of the voter list"
					);

					// no sef-stake but since it's chilling, it should be part of the TL even
					// with score = 0.
					Some(0)
				},
				Ok(StakerStatus::Validator) => {
					// active target should be part of the voter list.
					frame_support::ensure!(
						T::VoterList::contains(&target),
						"bonded and active validator should also be part of the voter list"
					);
					// return self-stake (ie. active bonded).
					T::Staking::stake(&target).map(|s| Self::to_vote(s.active)).ok()
				},
				Ok(StakerStatus::Nominator(_)) => {
					// an idle/dangling target may become a nominator.
					None
				},
			};

			if let Some(score) = maybe_self_stake {
				if let Some(stake) = approvals_map.get_mut(&target) {
					*stake += score as sp_npos_elections::ExtendedBalance;
				} else {
					approvals_map.insert(target, score.into());
				}
			} else {
				// unbonded target: it does not have self-stake.
			}
		}

		// compare calculated approvals per target with target list state.
		for (target, calculated_stake) in approvals_map.iter() {
			let stake_in_list = T::TargetList::get_score(target).expect("target must exist; qed.");

			if *calculated_stake != stake_in_list {
				log::error!(
					target: "runtime::stake-tracker",
					"try-runtime: score of {:?} in `TargetList` list: {:?}, calculated sum of all stake: {:?}",
					target,
					stake_in_list,
					calculated_stake,
				);

				return Err("target score in the target list is different than the expected".into())
			}
		}

		println!(
			"in approval: {:?}, in TL: {:?}",
			approvals_map.keys().count(),
			T::TargetList::iter().count()
		);
		println!("> TL: {:?}", T::TargetList::iter().map(|t| t).collect::<Vec<_>>());
		println!("> Apps: {:?}", approvals_map.keys());

		frame_support::ensure!(
			approvals_map.keys().count() == T::TargetList::iter().count(),
			"calculated approvals count is different from total of target list.",
		);

		Ok(())
	}

	/// Try-state: checks if targets in the target list are sorted by score.
	///
	/// Invariant
	///  * All targets in the target list are sorted by their score (approvals).
	///
	///  NOTE: unfortunatelly, it is not trivial to check if the sort correctness of the list if
	///  the `SortedListProvider` is implemented by bags list due to score bucketing. Thus, we
	///  leverage the [`SortedListProvider::in_position`] to verify if the target is in the
	/// correct  position in the list (bag or otherwise), given its score.
	pub fn do_try_state_target_sorting() -> Result<(), sp_runtime::TryRuntimeError> {
		for t in T::TargetList::iter() {
			frame_support::ensure!(
				T::TargetList::in_position(&t).expect("target exists"),
				"target list is not sorted"
			);
		}

		Ok(())
	}

	/// Try-state: checks if voters in the voter list are sorted by score (stake).
	///
	/// Invariant
	///  * All voters in the voter list are sorted by their score.
	///
	///  NOTE: unfortunatelly, it is not trivial to check if the sort correctness of the list if
	///  the `SortedListProvider` is implemented by bags list due to score bucketing. Thus, we
	///  leverage the [`SortedListProvider::in_position`] to verify if the target is in the
	/// correct  position in the list (bag or otherwise), given its score.
	pub fn do_try_state_voter_sorting() -> Result<(), sp_runtime::TryRuntimeError> {
		// if the voter list is in lazy mode, we don't expect the nodes to be sorted at all times.
		// skip checks.
		if T::VoterUpdateMode::get().is_strict_mode() {
			return Ok(())
		}

		for t in T::VoterList::iter() {
			frame_support::ensure!(
				T::VoterList::in_position(&t).expect("voter exists"),
				"voter list is not sorted"
			);
		}

		Ok(())
	}
}

impl<T: Config> OnStakingUpdate<T::AccountId, BalanceOf<T>> for Pallet<T> {
	/// When a nominator's stake is updated, all the nominated targets must be updated
	/// accordingly.
	///
	/// The score of the node associated with `who` in the *VoterList* will be updated if the
	/// the mode is [`VoterUpdateMode::Strict`]. The approvals of the nominated targets (by `who`)
	/// are always updated.
	fn on_stake_update(
		who: &T::AccountId,
		prev_stake: Option<Stake<BalanceOf<T>>>,
		stake: Stake<BalanceOf<T>>,
	) {
		match T::Staking::status(who) {
			Ok(StakerStatus::Nominator(nominations)) =>
				Self::do_stake_update_voter(who, prev_stake, stake, nominations),
			Ok(StakerStatus::Validator) => Self::do_stake_update_target(who, prev_stake, stake),
			Ok(StakerStatus::Idle) => (), // nothing to see here.
			Err(_) => {
				defensive!(
					"staker should exist when calling `on_stake_update` and have a valid status"
				);
			},
		}
	}

	/// A validator is also considered a voter with self-vote and should also be added to
	/// [`Config::VoterList`].
	//
	/// Note: it is assumed that `who`'s ledger staking state is updated *before* calling this
	/// method.
	fn on_validator_add(who: &T::AccountId, self_stake: Option<Stake<BalanceOf<T>>>) {
		let self_stake = Self::to_vote(self_stake.unwrap_or_default().active).into();

		match T::TargetList::on_insert(who.clone(), self_stake) {
			Ok(_) => (),
			Err(_) => {
				// if the target already exists in the list, it means that the target is idle
				// and/or is dangling.
				debug_assert!(
					T::Staking::status(who) == Ok(StakerStatus::Idle) ||
						T::Staking::status(who).is_err()
				);

				Self::update_target_score(who, StakeImbalance::Positive(self_stake));
			},
		}

		// a validator is also a nominator.
		Self::on_nominator_add(who, vec![])
	}

	/// A validator has been chilled. The target node remains in the target list.
	///
	/// While idling, the target node is not removed from the target list but its score is
	/// updated.
	fn on_validator_idle(who: &T::AccountId) {
		let self_stake = Self::vote_of(who);
		Self::update_target_score(who, StakeImbalance::Negative(self_stake.into()));

		// validator is a nominator too.
		Self::on_nominator_idle(who, vec![]);
	}

	/// A validator has been set as inactive/removed from the staking POV.
	///
	/// The target node is removed from the target list IFF its score is 0. Otherwise, its score
	/// should be kept up to date as if the validator was active.
	fn on_validator_remove(who: &T::AccountId) {
		// validator must be idle before removing completely. Perform some sanity checks too.
		match T::Staking::status(who) {
			Ok(StakerStatus::Idle) => (), // proceed
			Ok(StakerStatus::Validator) => Self::on_validator_idle(who),
			Ok(StakerStatus::Nominator(_)) => {
				defensive!("on_validator_remove called on a nominator, unexpected.");
				return
			},
			Err(_) => {
				defensive!("on_validator_remove called on a non-existing target.");
				return
			},
		};

		if let Ok(score) = T::TargetList::get_score(who) {
			// remove from target list IIF score is zero. If `score != 0`, the target still has
			// active nominations, thus we keep it in the target list with corresponding approval
			// stake.
			if score.is_zero() {
				T::TargetList::on_remove(who).expect("target exists as per above; qed");
			}
		} else {
			// target is not part of the list. Given the contract with staking and the checks above,
			// this may actually be called. So do nothing and skip defensive warns.
		};
	}

	/// A nominator has been added to the system.
	///
	/// Even in lazy mode, inserting voter list nodes on new nominator must be done.
	///
	/// Note: it is assumed that `who`'s ledger staking state is updated *before* this method is
	/// called.
	fn on_nominator_add(who: &T::AccountId, nominations: Vec<AccountIdOf<T>>) {
		let nominator_vote = Self::vote_of(who);
		let nominations = Self::ensure_dedup(nominations);

		// the new voter node will be added even if the voter is in lazy mode. In lazy mode, we
		// ensure that the nodes exist in the voter list, even though they may not have the updated
		// score at all times.
		let _ = T::VoterList::on_insert(who.clone(), nominator_vote).defensive_proof(
			"the nominator must not exist in the list as per the contract with staking.",
		);

		// if `who` is a nominator, update the vote weight of the nominations if they exist. Note:
		// this will update the score of up to `T::MaxNominations` validators.
		match T::Staking::status(who).defensive() {
			Ok(StakerStatus::Nominator(_)) =>
				for t in nominations {
					Self::update_target_score(&t, StakeImbalance::Positive(nominator_vote.into()))
				},
			Ok(StakerStatus::Idle) | Ok(StakerStatus::Validator) | Err(_) => (), // nada.
		};
	}

	/// A nominator has been idle. From the `T::VotertList` PoV, chilling a nominator is the same as
	/// removing it.
	///
	/// Note: it is assumed that `who`'s staking ledger and `nominations` are up to date before
	/// calling this method.
	fn on_nominator_idle(who: &T::AccountId, nominations: Vec<T::AccountId>) {
		Self::on_nominator_remove(who, nominations);
	}

	/// Fired when someone removes their intention to nominate and is completely removed from
	/// the staking state.
	///
	/// Even in lazy mode, removing voter list nodes on nominator remove must be done.
	///
	/// Note: the number of nodes that are updated is bounded by the maximum number of
	/// nominators, which is defined in the staking pallet.
	fn on_nominator_remove(who: &T::AccountId, nominations: Vec<T::AccountId>) {
		let nominator_vote = Self::vote_of(who);
		let nominations = Self::ensure_dedup(nominations);

		// updates the nominated target's score.
		for t in nominations.iter() {
			Self::update_target_score(t, StakeImbalance::Negative(nominator_vote.into()))
		}

		let _ = T::VoterList::on_remove(who).defensive_proof(
			"the nominator must exist in the list as per the contract with staking.",
		);
	}

	/// This is called when a nominator updates their nominations. The nominator's stake remains
	/// the same (updates to the nominator's stake should emit [`Self::on_stake_update`]
	/// instead). However, the score of the nominated targets must be updated accordingly.
	///
	/// Note: it is assumed that `who`'s ledger staking state is updated *before* calling this
	/// method.
	fn on_nominator_update(
		who: &T::AccountId,
		prev_nominations: Vec<T::AccountId>,
		nominations: Vec<T::AccountId>,
	) {
		let nominator_vote = Self::vote_of(who);

		let nominations = Self::ensure_dedup(nominations);
		let prev_nominations = Self::ensure_dedup(prev_nominations);

		// new nominations.
		for target in nominations.iter() {
			if !prev_nominations.contains(target) {
				Self::update_target_score(target, StakeImbalance::Positive(nominator_vote.into()));
			}
		}
		// removed nominations.
		for target in prev_nominations.iter() {
			if !nominations.contains(target) {
				Self::update_target_score(target, StakeImbalance::Negative(nominator_vote.into()));
			}
		}
	}

	/// This is called when a staker is slashed.
	///
	/// From the stake-tracker POV, no direct changes should be made to the target or voter list in
	/// this event handler, since the stake updates from a slash will be indirectly performed
	/// through the call to `on_stake_update`.
	///
	/// However, if a slash of a nominator results on its active stake becoming 0, the stake
	/// tracker *requests* the staking interface to chill the nominator in order to ensure that
	/// their nominations are dropped. This way, we ensure that in the event of a validator and all
	/// its nominators are 100% slashed, the target can be reaped/killed without leaving
	/// nominations behind.
	fn on_slash(
		stash: &T::AccountId,
		_slashed_active: BalanceOf<T>,
		_slashed_unlocking: &BTreeMap<sp_staking::EraIndex, BalanceOf<T>>,
		slashed_total: BalanceOf<T>,
	) {
		let active_after_slash = T::Staking::stake(stash)
			.defensive_unwrap_or_default()
			.active
			.saturating_sub(slashed_total);

		match (active_after_slash.is_zero(), T::Staking::status(stash)) {
			(true, Ok(StakerStatus::Nominator(_))) => {
				let _ = T::Staking::chill(stash).defensive();
			},
			_ => (), // do nothing.
		};
	}

	// no-op events.

	/// The score of the staker `who` is updated through the `on_stake_update` calls following the
	/// full unstake (ledger kill).
	fn on_unstake(_who: &T::AccountId) {}

	/// The score of the staker `who` is updated through the `on_stake_update` calls following the
	/// withdraw.
	fn on_withdraw(_who: &T::AccountId, _amount: BalanceOf<T>) {}
}
