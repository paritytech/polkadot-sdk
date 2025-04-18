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

//! `pallet-staking-async`'s main `impl` blocks.

use crate::{
	asset,
	election_size_tracker::StaticTracker,
	log,
	session_rotation::{self, Eras},
	slashing,
	weights::WeightInfo,
	BalanceOf, Exposure, Forcing, LedgerIntegrityState, MaxNominationsOf, Nominations,
	NominationsQuota, PositiveImbalanceOf, RewardDestination, SnapshotStatus, StakingLedger,
	ValidatorPrefs, STAKING_ID,
};
use alloc::{boxed::Box, vec, vec::Vec};
use frame_election_provider_support::{
	bounds::CountBound, data_provider, DataProviderBounds, ElectionDataProvider, ElectionProvider,
	PageIndex, ScoreProvider, SortedListProvider, VoteWeight, VoterOf,
};
use frame_support::{
	defensive,
	dispatch::WithPostDispatchInfo,
	pallet_prelude::*,
	traits::{
		Defensive, DefensiveSaturating, Get, Imbalance, InspectLockableCurrency, LockableCurrency,
		OnUnbalanced,
	},
	weights::Weight,
};
use frame_system::{pallet_prelude::BlockNumberFor, RawOrigin};
use pallet_staking_async_rc_client::{self as rc_client};
use sp_runtime::{
	traits::{CheckedAdd, Saturating, StaticLookup, Zero},
	ArithmeticError, DispatchResult, Perbill,
};
use sp_staking::{
	currency_to_vote::CurrencyToVote,
	EraIndex, OnStakingUpdate, Page, SessionIndex, Stake,
	StakingAccount::{self, Controller, Stash},
	StakingInterface,
};

use super::pallet::*;

use crate::slashing::OffenceRecord;
#[cfg(feature = "try-runtime")]
use frame_support::ensure;
#[cfg(any(test, feature = "try-runtime"))]
use sp_runtime::TryRuntimeError;

/// The maximum number of iterations that we do whilst iterating over `T::VoterList` in
/// `get_npos_voters`.
///
/// In most cases, if we want n items, we iterate exactly n times. In rare cases, if a voter is
/// invalid (for any reason) the iteration continues. With this constant, we iterate at most 2 * n
/// times and then give up.
const NPOS_MAX_ITERATIONS_COEFFICIENT: u32 = 2;

impl<T: Config> Pallet<T> {
	/// Fetches the ledger associated with a controller or stash account, if any.
	pub fn ledger(account: StakingAccount<T::AccountId>) -> Result<StakingLedger<T>, Error<T>> {
		StakingLedger::<T>::get(account)
	}

	pub fn payee(account: StakingAccount<T::AccountId>) -> Option<RewardDestination<T::AccountId>> {
		StakingLedger::<T>::reward_destination(account)
	}

	/// Fetches the controller bonded to a stash account, if any.
	pub fn bonded(stash: &T::AccountId) -> Option<T::AccountId> {
		StakingLedger::<T>::paired_account(Stash(stash.clone()))
	}

	/// Inspects and returns the corruption state of a ledger and direct bond, if any.
	///
	/// Note: all operations in this method access directly the `Bonded` and `Ledger` storage maps
	/// instead of using the [`StakingLedger`] API since the bond and/or ledger may be corrupted.
	/// It is also meant to check state for direct bonds and may not work as expected for virtual
	/// bonds.
	pub(crate) fn inspect_bond_state(
		stash: &T::AccountId,
	) -> Result<LedgerIntegrityState, Error<T>> {
		// look at any old unmigrated lock as well.
		let hold_or_lock = asset::staked::<T>(&stash)
			.max(T::OldCurrency::balance_locked(STAKING_ID, &stash).into());

		let controller = <Bonded<T>>::get(stash).ok_or_else(|| {
			if hold_or_lock == Zero::zero() {
				Error::<T>::NotStash
			} else {
				Error::<T>::BadState
			}
		})?;

		match Ledger::<T>::get(controller) {
			Some(ledger) =>
				if ledger.stash != *stash {
					Ok(LedgerIntegrityState::Corrupted)
				} else {
					if hold_or_lock != ledger.total {
						Ok(LedgerIntegrityState::LockCorrupted)
					} else {
						Ok(LedgerIntegrityState::Ok)
					}
				},
			None => Ok(LedgerIntegrityState::CorruptedKilled),
		}
	}

	/// The total balance that can be slashed from a stash account as of right now.
	pub fn slashable_balance_of(stash: &T::AccountId) -> BalanceOf<T> {
		// Weight note: consider making the stake accessible through stash.
		Self::ledger(Stash(stash.clone())).map(|l| l.active).unwrap_or_default()
	}

	/// Internal impl of [`Self::slashable_balance_of`] that returns [`VoteWeight`].
	pub fn slashable_balance_of_vote_weight(
		stash: &T::AccountId,
		issuance: BalanceOf<T>,
	) -> VoteWeight {
		T::CurrencyToVote::to_vote(Self::slashable_balance_of(stash), issuance)
	}

	/// Returns a closure around `slashable_balance_of_vote_weight` that can be passed around.
	///
	/// This prevents call sites from repeatedly requesting `total_issuance` from backend. But it is
	/// important to be only used while the total issuance is not changing.
	pub fn weight_of_fn() -> Box<dyn Fn(&T::AccountId) -> VoteWeight> {
		// NOTE: changing this to unboxed `impl Fn(..)` return type and the pallet will still
		// compile, while some types in mock fail to resolve.
		let issuance = asset::total_issuance::<T>();
		Box::new(move |who: &T::AccountId| -> VoteWeight {
			Self::slashable_balance_of_vote_weight(who, issuance)
		})
	}

	/// Same as `weight_of_fn`, but made for one time use.
	pub fn weight_of(who: &T::AccountId) -> VoteWeight {
		let issuance = asset::total_issuance::<T>();
		Self::slashable_balance_of_vote_weight(who, issuance)
	}

	pub(super) fn do_bond_extra(stash: &T::AccountId, additional: BalanceOf<T>) -> DispatchResult {
		let mut ledger = Self::ledger(StakingAccount::Stash(stash.clone()))?;

		// for virtual stakers, we don't need to check the balance. Since they are only accessed
		// via low level apis, we can assume that the caller has done the due diligence.
		let extra = if Self::is_virtual_staker(stash) {
			additional
		} else {
			// additional amount or actual balance of stash whichever is lower.
			additional.min(asset::free_to_stake::<T>(stash))
		};

		ledger.total = ledger.total.checked_add(&extra).ok_or(ArithmeticError::Overflow)?;
		ledger.active = ledger.active.checked_add(&extra).ok_or(ArithmeticError::Overflow)?;
		// last check: the new active amount of ledger must be more than ED.
		ensure!(ledger.active >= asset::existential_deposit::<T>(), Error::<T>::InsufficientBond);

		// NOTE: ledger must be updated prior to calling `Self::weight_of`.
		ledger.update()?;
		// update this staker in the sorted list, if they exist in it.
		if T::VoterList::contains(stash) {
			// This might fail if the voter list is locked.
			let _ = T::VoterList::on_update(&stash, Self::weight_of(stash));
		}

		Self::deposit_event(Event::<T>::Bonded { stash: stash.clone(), amount: extra });

		Ok(())
	}

	pub(super) fn do_withdraw_unbonded(
		controller: &T::AccountId,
		num_slashing_spans: u32,
	) -> Result<Weight, DispatchError> {
		let mut ledger = Self::ledger(Controller(controller.clone()))?;
		let (stash, old_total) = (ledger.stash.clone(), ledger.total);
		if let Some(current_era) = CurrentEra::<T>::get() {
			ledger = ledger.consolidate_unlocked(current_era)
		}
		let new_total = ledger.total;

		let ed = asset::existential_deposit::<T>();
		let used_weight =
			if ledger.unlocking.is_empty() && (ledger.active < ed || ledger.active.is_zero()) {
				// This account must have called `unbond()` with some value that caused the active
				// portion to fall below existential deposit + will have no more unlocking chunks
				// left. We can now safely remove all staking-related information.
				Self::kill_stash(&ledger.stash, num_slashing_spans)?;

				T::WeightInfo::withdraw_unbonded_kill(num_slashing_spans)
			} else {
				// This was the consequence of a partial unbond. just update the ledger and move on.
				ledger.update()?;

				// This is only an update, so we use less overall weight.
				T::WeightInfo::withdraw_unbonded_update(num_slashing_spans)
			};

		// `old_total` should never be less than the new total because
		// `consolidate_unlocked` strictly subtracts balance.
		if new_total < old_total {
			// Already checked that this won't overflow by entry condition.
			let value = old_total.defensive_saturating_sub(new_total);
			Self::deposit_event(Event::<T>::Withdrawn { stash, amount: value });

			// notify listeners.
			T::EventListeners::on_withdraw(controller, value);
		}

		Ok(used_weight)
	}

	pub(super) fn do_payout_stakers(
		validator_stash: T::AccountId,
		era: EraIndex,
	) -> DispatchResultWithPostInfo {
		let page = Eras::<T>::get_next_claimable_page(era, &validator_stash).ok_or_else(|| {
			Error::<T>::AlreadyClaimed.with_weight(T::WeightInfo::payout_stakers_alive_staked(0))
		})?;

		Self::do_payout_stakers_by_page(validator_stash, era, page)
	}

	pub(super) fn do_payout_stakers_by_page(
		validator_stash: T::AccountId,
		era: EraIndex,
		page: Page,
	) -> DispatchResultWithPostInfo {
		// Validate input data
		let current_era = CurrentEra::<T>::get().ok_or_else(|| {
			Error::<T>::InvalidEraToReward
				.with_weight(T::WeightInfo::payout_stakers_alive_staked(0))
		})?;

		let history_depth = T::HistoryDepth::get();

		ensure!(
			era <= current_era && era >= current_era.saturating_sub(history_depth),
			Error::<T>::InvalidEraToReward
				.with_weight(T::WeightInfo::payout_stakers_alive_staked(0))
		);

		ensure!(
			page < Eras::<T>::exposure_page_count(era, &validator_stash),
			Error::<T>::InvalidPage.with_weight(T::WeightInfo::payout_stakers_alive_staked(0))
		);

		// Note: if era has no reward to be claimed, era may be future.
		let era_payout = Eras::<T>::get_validators_reward(era).ok_or_else(|| {
			Error::<T>::InvalidEraToReward
				.with_weight(T::WeightInfo::payout_stakers_alive_staked(0))
		})?;

		let account = StakingAccount::Stash(validator_stash.clone());
		let ledger = Self::ledger(account.clone()).or_else(|_| {
			if StakingLedger::<T>::is_bonded(account) {
				Err(Error::<T>::NotController.into())
			} else {
				Err(Error::<T>::NotStash.with_weight(T::WeightInfo::payout_stakers_alive_staked(0)))
			}
		})?;

		ledger.clone().update()?;

		let stash = ledger.stash.clone();

		if Eras::<T>::is_rewards_claimed(era, &stash, page) {
			return Err(Error::<T>::AlreadyClaimed
				.with_weight(T::WeightInfo::payout_stakers_alive_staked(0)))
		}

		Eras::<T>::set_rewards_as_claimed(era, &stash, page);

		let exposure = Eras::<T>::get_paged_exposure(era, &stash, page).ok_or_else(|| {
			Error::<T>::InvalidEraToReward
				.with_weight(T::WeightInfo::payout_stakers_alive_staked(0))
		})?;

		// Input data seems good, no errors allowed after this point

		// Get Era reward points. It has TOTAL and INDIVIDUAL
		// Find the fraction of the era reward that belongs to the validator
		// Take that fraction of the eras rewards to split to nominator and validator
		//
		// Then look at the validator, figure out the proportion of their reward
		// which goes to them and each of their nominators.

		let era_reward_points = Eras::<T>::get_reward_points(era);
		let total_reward_points = era_reward_points.total;
		let validator_reward_points =
			era_reward_points.individual.get(&stash).copied().unwrap_or_else(Zero::zero);

		// Nothing to do if they have no reward points.
		if validator_reward_points.is_zero() {
			return Ok(Some(T::WeightInfo::payout_stakers_alive_staked(0)).into())
		}

		// This is the fraction of the total reward that the validator and the
		// nominators will get.
		let validator_total_reward_part =
			Perbill::from_rational(validator_reward_points, total_reward_points);

		// This is how much validator + nominators are entitled to.
		let validator_total_payout = validator_total_reward_part * era_payout;

		let validator_commission = Eras::<T>::get_validator_commission(era, &ledger.stash);
		// total commission validator takes across all nominator pages
		let validator_total_commission_payout = validator_commission * validator_total_payout;

		let validator_leftover_payout =
			validator_total_payout.defensive_saturating_sub(validator_total_commission_payout);
		// Now let's calculate how this is split to the validator.
		let validator_exposure_part = Perbill::from_rational(exposure.own(), exposure.total());
		let validator_staking_payout = validator_exposure_part * validator_leftover_payout;
		let page_stake_part = Perbill::from_rational(exposure.page_total(), exposure.total());
		// validator commission is paid out in fraction across pages proportional to the page stake.
		let validator_commission_payout = page_stake_part * validator_total_commission_payout;

		Self::deposit_event(Event::<T>::PayoutStarted {
			era_index: era,
			validator_stash: stash.clone(),
			page,
			next: Eras::<T>::get_next_claimable_page(era, &stash),
		});

		let mut total_imbalance = PositiveImbalanceOf::<T>::zero();
		// We can now make total validator payout:
		if let Some((imbalance, dest)) =
			Self::make_payout(&stash, validator_staking_payout + validator_commission_payout)
		{
			Self::deposit_event(Event::<T>::Rewarded { stash, dest, amount: imbalance.peek() });
			total_imbalance.subsume(imbalance);
		}

		// Track the number of payout ops to nominators. Note:
		// `WeightInfo::payout_stakers_alive_staked` always assumes at least a validator is paid
		// out, so we do not need to count their payout op.
		let mut nominator_payout_count: u32 = 0;

		// Lets now calculate how this is split to the nominators.
		// Reward only the clipped exposures. Note this is not necessarily sorted.
		for nominator in exposure.others().iter() {
			let nominator_exposure_part = Perbill::from_rational(nominator.value, exposure.total());

			let nominator_reward: BalanceOf<T> =
				nominator_exposure_part * validator_leftover_payout;
			// We can now make nominator payout:
			if let Some((imbalance, dest)) = Self::make_payout(&nominator.who, nominator_reward) {
				// Note: this logic does not count payouts for `RewardDestination::None`.
				nominator_payout_count += 1;
				let e = Event::<T>::Rewarded {
					stash: nominator.who.clone(),
					dest,
					amount: imbalance.peek(),
				};
				Self::deposit_event(e);
				total_imbalance.subsume(imbalance);
			}
		}

		T::Reward::on_unbalanced(total_imbalance);
		debug_assert!(nominator_payout_count <= T::MaxExposurePageSize::get());

		Ok(Some(T::WeightInfo::payout_stakers_alive_staked(nominator_payout_count)).into())
	}

	/// Chill a stash account.
	pub(crate) fn chill_stash(stash: &T::AccountId) {
		let chilled_as_validator = Self::do_remove_validator(stash);
		let chilled_as_nominator = Self::do_remove_nominator(stash);
		if chilled_as_validator || chilled_as_nominator {
			Self::deposit_event(Event::<T>::Chilled { stash: stash.clone() });
		}
	}

	/// Actually make a payment to a staker. This uses the currency's reward function
	/// to pay the right payee for the given staker account.
	fn make_payout(
		stash: &T::AccountId,
		amount: BalanceOf<T>,
	) -> Option<(PositiveImbalanceOf<T>, RewardDestination<T::AccountId>)> {
		// noop if amount is zero
		if amount.is_zero() {
			return None
		}
		let dest = Self::payee(StakingAccount::Stash(stash.clone()))?;

		let maybe_imbalance = match dest {
			RewardDestination::Stash => asset::mint_into_existing::<T>(stash, amount),
			RewardDestination::Staked => Self::ledger(Stash(stash.clone()))
				.and_then(|mut ledger| {
					ledger.active += amount;
					ledger.total += amount;
					let r = asset::mint_into_existing::<T>(stash, amount);

					let _ = ledger
						.update()
						.defensive_proof("ledger fetched from storage, so it exists; qed.");

					Ok(r)
				})
				.unwrap_or_default(),
			RewardDestination::Account(ref dest_account) =>
				Some(asset::mint_creating::<T>(&dest_account, amount)),
			RewardDestination::None => None,
			#[allow(deprecated)]
			RewardDestination::Controller => Self::bonded(stash)
					.map(|controller| {
						defensive!("Paying out controller as reward destination which is deprecated and should be migrated.");
						// This should never happen once payees with a `Controller` variant have been migrated.
						// But if it does, just pay the controller account.
						asset::mint_creating::<T>(&controller, amount)
		}),
		};
		maybe_imbalance.map(|imbalance| (imbalance, dest))
	}

	/// Remove all associated data of a stash account from the staking system.
	///
	/// Assumes storage is upgraded before calling.
	///
	/// This is called:
	/// - after a `withdraw_unbonded()` call that frees all of a stash's bonded balance.
	/// - through `reap_stash()` if the balance has fallen to zero (through slashing).
	pub(crate) fn kill_stash(stash: &T::AccountId, num_slashing_spans: u32) -> DispatchResult {
		slashing::clear_stash_metadata::<T>(&stash, num_slashing_spans)?;

		// removes controller from `Bonded` and staking ledger from `Ledger`, as well as reward
		// setting of the stash in `Payee`.
		StakingLedger::<T>::kill(&stash)?;

		Self::do_remove_validator(&stash);
		Self::do_remove_nominator(&stash);

		Ok(())
	}

	#[cfg(test)]
	pub(crate) fn reward_by_ids(validators_points: impl IntoIterator<Item = (T::AccountId, u32)>) {
		Eras::<T>::reward_active_era(validators_points)
	}

	/// Helper to set a new `ForceEra` mode.
	pub(crate) fn set_force_era(mode: Forcing) {
		log!(info, "Setting force era mode {:?}.", mode);
		ForceEra::<T>::put(mode);
		Self::deposit_event(Event::<T>::ForceEra { mode });
	}

	#[cfg(feature = "runtime-benchmarks")]
	pub fn add_era_stakers(
		current_era: EraIndex,
		stash: T::AccountId,
		exposure: Exposure<T::AccountId, BalanceOf<T>>,
	) {
		Eras::<T>::upsert_exposure(current_era, &stash, exposure);
	}

	#[cfg(feature = "runtime-benchmarks")]
	pub fn set_slash_reward_fraction(fraction: Perbill) {
		SlashRewardFraction::<T>::put(fraction);
	}

	/// Get all the voters associated with `page` that are eligible for the npos election.
	///
	/// `bounds` can impose a cap on the number of voters returned per page.
	///
	/// Sets `MinimumActiveStake` to the minimum active nominator stake in the returned set of
	/// nominators.
	///
	/// Note: in the context of the multi-page snapshot, we expect the *order* of `VoterList` and
	/// `TargetList` not to change while the pages are being processed.
	pub(crate) fn get_npos_voters(
		bounds: DataProviderBounds,
		status: &SnapshotStatus<T::AccountId>,
	) -> Vec<VoterOf<Self>> {
		let mut voters_size_tracker: StaticTracker<Self> = StaticTracker::default();

		let page_len_prediction = {
			let all_voter_count = T::VoterList::count();
			bounds.count.unwrap_or(all_voter_count.into()).min(all_voter_count.into()).0
		};

		let mut all_voters = Vec::<_>::with_capacity(page_len_prediction as usize);

		// cache a few things.
		let weight_of = Self::weight_of_fn();

		let mut voters_seen = 0u32;
		let mut validators_taken = 0u32;
		let mut nominators_taken = 0u32;
		let mut min_active_stake = u64::MAX;

		let mut sorted_voters = match status {
			// start the snapshot processing from the beginning.
			SnapshotStatus::Waiting => T::VoterList::iter(),
			// snapshot continues, start from the last iterated voter in the list.
			SnapshotStatus::Ongoing(account_id) => T::VoterList::iter_from(&account_id)
				.defensive_unwrap_or(Box::new(vec![].into_iter())),
			// all voters have been consumed already, return an empty iterator.
			SnapshotStatus::Consumed => Box::new(vec![].into_iter()),
		};

		while all_voters.len() < page_len_prediction as usize &&
			voters_seen < (NPOS_MAX_ITERATIONS_COEFFICIENT * page_len_prediction as u32)
		{
			let voter = match sorted_voters.next() {
				Some(voter) => {
					voters_seen.saturating_inc();
					voter
				},
				None => break,
			};

			let voter_weight = weight_of(&voter);
			// if voter weight is zero, do not consider this voter for the snapshot.
			if voter_weight.is_zero() {
				log!(debug, "voter's active balance is 0. skip this voter.");
				continue
			}

			if let Some(Nominations { targets, .. }) = <Nominators<T>>::get(&voter) {
				if !targets.is_empty() {
					// Note on lazy nomination quota: we do not check the nomination quota of the
					// voter at this point and accept all the current nominations. The nomination
					// quota is only enforced at `nominate` time.

					let voter = (voter, voter_weight, targets);
					if voters_size_tracker.try_register_voter(&voter, &bounds).is_err() {
						// no more space left for the election result, stop iterating.
						Self::deposit_event(Event::<T>::SnapshotVotersSizeExceeded {
							size: voters_size_tracker.size as u32,
						});
						break
					}

					all_voters.push(voter);
					nominators_taken.saturating_inc();
				} else {
					defensive!("non-nominator fetched from voter list: {:?}", voter);
					// technically should never happen, but not much we can do about it.
				}
				min_active_stake =
					if voter_weight < min_active_stake { voter_weight } else { min_active_stake };
			} else if Validators::<T>::contains_key(&voter) {
				// if this voter is a validator:
				let self_vote = (
					voter.clone(),
					voter_weight,
					vec![voter.clone()]
						.try_into()
						.expect("`MaxVotesPerVoter` must be greater than or equal to 1"),
				);

				if voters_size_tracker.try_register_voter(&self_vote, &bounds).is_err() {
					// no more space left for the election snapshot, stop iterating.
					Self::deposit_event(Event::<T>::SnapshotVotersSizeExceeded {
						size: voters_size_tracker.size as u32,
					});
					break
				}
				all_voters.push(self_vote);
				validators_taken.saturating_inc();
			} else {
				// this can only happen if: 1. there a bug in the bags-list (or whatever is the
				// sorted list) logic and the state of the two pallets is no longer compatible, or
				// because the nominators is not decodable since they have more nomination than
				// `T::NominationsQuota::get_quota`. The latter can rarely happen, and is not
				// really an emergency or bug if it does.
				defensive!(
				    "invalid item in `VoterList`: {:?}, this nominator probably has too many nominations now",
                    voter,
                );
			}
		}

		// all_voters should have not re-allocated.
		debug_assert!(all_voters.capacity() == page_len_prediction as usize);

		let min_active_stake: T::CurrencyBalance =
			if all_voters.is_empty() { Zero::zero() } else { min_active_stake.into() };

		MinimumActiveStake::<T>::put(min_active_stake);

		all_voters
	}

	/// Get all the targets associated are eligible for the npos election.
	///
	/// The target snapshot is *always* single paged.
	///
	/// This function is self-weighing as [`DispatchClass::Mandatory`].
	pub fn get_npos_targets(bounds: DataProviderBounds) -> Vec<T::AccountId> {
		let mut targets_size_tracker: StaticTracker<Self> = StaticTracker::default();

		let final_predicted_len = {
			let all_target_count = T::TargetList::count();
			bounds.count.unwrap_or(all_target_count.into()).min(all_target_count.into()).0
		};

		let mut all_targets = Vec::<T::AccountId>::with_capacity(final_predicted_len as usize);
		let mut targets_seen = 0;

		let mut targets_iter = T::TargetList::iter();
		while all_targets.len() < final_predicted_len as usize &&
			targets_seen < (NPOS_MAX_ITERATIONS_COEFFICIENT * final_predicted_len as u32)
		{
			let target = match targets_iter.next() {
				Some(target) => {
					targets_seen.saturating_inc();
					target
				},
				None => break,
			};

			if targets_size_tracker.try_register_target(target.clone(), &bounds).is_err() {
				// no more space left for the election snapshot, stop iterating.
				log!(warn, "npos targets size exceeded, stopping iteration.");
				Self::deposit_event(Event::<T>::SnapshotTargetsSizeExceeded {
					size: targets_size_tracker.size as u32,
				});
				break
			}

			if Validators::<T>::contains_key(&target) {
				all_targets.push(target);
			}
		}

		log!(info, "[bounds {:?}] generated {} npos targets", bounds, all_targets.len());

		all_targets
	}

	/// This function will add a nominator to the `Nominators` storage map,
	/// and `VoterList`.
	///
	/// If the nominator already exists, their nominations will be updated.
	///
	/// NOTE: you must ALWAYS use this function to add nominator or update their targets. Any access
	/// to `Nominators` or `VoterList` outside of this function is almost certainly
	/// wrong.
	pub fn do_add_nominator(who: &T::AccountId, nominations: Nominations<T>) {
		if !Nominators::<T>::contains_key(who) {
			// maybe update sorted list.
			let _ = T::VoterList::on_insert(who.clone(), Self::weight_of(who))
				.defensive_unwrap_or_default();
		}
		Nominators::<T>::insert(who, nominations);

		debug_assert_eq!(
			Nominators::<T>::count() + Validators::<T>::count(),
			T::VoterList::count()
		);
	}

	/// This function will remove a nominator from the `Nominators` storage map,
	/// and `VoterList`.
	///
	/// Returns true if `who` was removed from `Nominators`, otherwise false.
	///
	/// NOTE: you must ALWAYS use this function to remove a nominator from the system. Any access to
	/// `Nominators` or `VoterList` outside of this function is almost certainly
	/// wrong.
	pub fn do_remove_nominator(who: &T::AccountId) -> bool {
		let outcome = if Nominators::<T>::contains_key(who) {
			Nominators::<T>::remove(who);
			let _ = T::VoterList::on_remove(who);
			true
		} else {
			false
		};

		debug_assert_eq!(
			Nominators::<T>::count() + Validators::<T>::count(),
			T::VoterList::count()
		);

		outcome
	}

	/// This function will add a validator to the `Validators` storage map.
	///
	/// If the validator already exists, their preferences will be updated.
	///
	/// NOTE: you must ALWAYS use this function to add a validator to the system. Any access to
	/// `Validators` or `VoterList` outside of this function is almost certainly
	/// wrong.
	pub fn do_add_validator(who: &T::AccountId, prefs: ValidatorPrefs) {
		if !Validators::<T>::contains_key(who) {
			// maybe update sorted list.
			let _ = T::VoterList::on_insert(who.clone(), Self::weight_of(who));
		}
		Validators::<T>::insert(who, prefs);

		debug_assert_eq!(
			Nominators::<T>::count() + Validators::<T>::count(),
			T::VoterList::count()
		);
	}

	/// This function will remove a validator from the `Validators` storage map.
	///
	/// Returns true if `who` was removed from `Validators`, otherwise false.
	///
	/// NOTE: you must ALWAYS use this function to remove a validator from the system. Any access to
	/// `Validators` or `VoterList` outside of this function is almost certainly
	/// wrong.
	pub fn do_remove_validator(who: &T::AccountId) -> bool {
		let outcome = if Validators::<T>::contains_key(who) {
			Validators::<T>::remove(who);
			let _ = T::VoterList::on_remove(who);
			true
		} else {
			false
		};

		debug_assert_eq!(
			Nominators::<T>::count() + Validators::<T>::count(),
			T::VoterList::count()
		);

		outcome
	}

	/// Register some amount of weight directly with the system pallet.
	///
	/// This is always mandatory weight.
	pub(crate) fn register_weight(weight: Weight) {
		<frame_system::Pallet<T>>::register_extra_weight_unchecked(
			weight,
			DispatchClass::Mandatory,
		);
	}

	/// Returns full exposure of a validator for a given era.
	///
	/// History note: This used to be a getter for old storage item `ErasStakers` deprecated in v14
	/// and deleted in v17. Since this function is used in the codebase at various places, we kept
	/// it as a custom getter that takes care of getting the full exposure of the validator in a
	/// backward compatible way.
	pub fn eras_stakers(
		era: EraIndex,
		account: &T::AccountId,
	) -> Exposure<T::AccountId, BalanceOf<T>> {
		Eras::<T>::get_full_exposure(era, account)
	}

	pub(super) fn do_migrate_currency(stash: &T::AccountId) -> DispatchResult {
		if Self::is_virtual_staker(stash) {
			return Self::do_migrate_virtual_staker(stash);
		}

		let ledger = Self::ledger(Stash(stash.clone()))?;
		let staked: BalanceOf<T> = T::OldCurrency::balance_locked(STAKING_ID, stash).into();
		ensure!(!staked.is_zero(), Error::<T>::AlreadyMigrated);
		ensure!(ledger.total == staked, Error::<T>::BadState);

		// remove old staking lock
		T::OldCurrency::remove_lock(STAKING_ID, &stash);

		// check if we can hold all stake.
		let max_hold = asset::free_to_stake::<T>(&stash);
		let force_withdraw = if max_hold >= staked {
			// this means we can hold all stake. yay!
			asset::update_stake::<T>(&stash, staked)?;
			Zero::zero()
		} else {
			// if we are here, it means we cannot hold all user stake. We will do a force withdraw
			// from ledger, but that's okay since anyways user do not have funds for it.
			let force_withdraw = staked.saturating_sub(max_hold);

			// we ignore if active is 0. It implies the locked amount is not actively staked. The
			// account can still get away from potential slash but we can't do much better here.
			StakingLedger {
				total: max_hold,
				active: ledger.active.saturating_sub(force_withdraw),
				// we are not changing the stash, so we can keep the stash.
				..ledger
			}
			.update()?;
			force_withdraw
		};

		// Get rid of the extra consumer we used to have with OldCurrency.
		frame_system::Pallet::<T>::dec_consumers(&stash);

		Self::deposit_event(Event::<T>::CurrencyMigrated { stash: stash.clone(), force_withdraw });
		Ok(())
	}

	fn do_migrate_virtual_staker(stash: &T::AccountId) -> DispatchResult {
		// Funds for virtual stakers not managed/held by this pallet. We only need to clear
		// the extra consumer we used to have with OldCurrency.
		frame_system::Pallet::<T>::dec_consumers(&stash);

		// The delegation system that manages the virtual staker needed to increment provider
		// previously because of the consumer needed by this pallet. In reality, this stash
		// is just a key for managing the ledger and the account does not need to hold any
		// balance or exist. We decrement this provider.
		let actual_providers = frame_system::Pallet::<T>::providers(stash);

		let expected_providers =
			// provider is expected to be 1 but someone can always transfer some free funds to
			// these accounts, increasing the provider.
			if asset::free_to_stake::<T>(&stash) >= asset::existential_deposit::<T>() {
				2
			} else {
				1
			};

		// We should never have more than expected providers.
		ensure!(actual_providers <= expected_providers, Error::<T>::BadState);

		// if actual provider is less than expected, it is already migrated.
		ensure!(actual_providers == expected_providers, Error::<T>::AlreadyMigrated);

		// dec provider
		let _ = frame_system::Pallet::<T>::dec_providers(&stash)?;

		return Ok(())
	}
}

impl<T: Config> Pallet<T> {
	/// Returns the current nominations quota for nominators.
	///
	/// Used by the runtime API.
	pub fn api_nominations_quota(balance: BalanceOf<T>) -> u32 {
		T::NominationsQuota::get_quota(balance)
	}

	pub fn api_eras_stakers(
		era: EraIndex,
		account: T::AccountId,
	) -> Exposure<T::AccountId, BalanceOf<T>> {
		Self::eras_stakers(era, &account)
	}

	pub fn api_eras_stakers_page_count(era: EraIndex, account: T::AccountId) -> Page {
		Eras::<T>::exposure_page_count(era, &account)
	}

	pub fn api_pending_rewards(era: EraIndex, account: T::AccountId) -> bool {
		Eras::<T>::pending_rewards(era, &account)
	}
}

impl<T: Config> ElectionDataProvider for Pallet<T> {
	type AccountId = T::AccountId;
	type BlockNumber = BlockNumberFor<T>;
	type MaxVotesPerVoter = MaxNominationsOf<T>;

	fn desired_targets() -> data_provider::Result<u32> {
		Self::register_weight(T::DbWeight::get().reads(1));
		Ok(ValidatorCount::<T>::get())
	}

	fn electing_voters(
		bounds: DataProviderBounds,
		page: PageIndex,
	) -> data_provider::Result<Vec<VoterOf<Self>>> {
		let mut status = VoterSnapshotStatus::<T>::get();
		let voters = Self::get_npos_voters(bounds, &status);

		// update the voter snapshot status.
		match (page, &status) {
			// last page, reset status for next round.
			(0, _) => status = SnapshotStatus::Waiting,

			(_, SnapshotStatus::Waiting) | (_, SnapshotStatus::Ongoing(_)) => {
				let maybe_last = voters.last().map(|(x, _, _)| x).cloned();

				if let Some(ref last) = maybe_last {
					let has_next =
						T::VoterList::iter_from(last).ok().and_then(|mut i| i.next()).is_some();
					if has_next {
						status = SnapshotStatus::Ongoing(last.clone());
					} else {
						status = SnapshotStatus::Consumed;
					}
				}
			},
			// do nothing.
			(_, SnapshotStatus::Consumed) => (),
		}

		log!(
			debug,
			"[page {}, status {:?} (stake?: {:?}), bounds {:?}] generated {} npos voters",
			page,
			VoterSnapshotStatus::<T>::get(),
			if let SnapshotStatus::Ongoing(x) = VoterSnapshotStatus::<T>::get() {
				Self::weight_of(&x)
			} else {
				Zero::zero()
			},
			bounds,
			voters.len(),
		);

		match status {
			SnapshotStatus::Ongoing(_) => T::VoterList::lock(),
			_ => T::VoterList::unlock(),
		}

		VoterSnapshotStatus::<T>::put(status);

		debug_assert!(!bounds.slice_exhausted(&voters));

		Ok(voters)
	}

	fn electing_voters_stateless(
		bounds: DataProviderBounds,
	) -> data_provider::Result<Vec<VoterOf<Self>>> {
		let voters = Self::get_npos_voters(bounds, &SnapshotStatus::Waiting);
		log!(
			debug,
			"[stateless, status {:?}, bounds {:?}] generated {} npos voters",
			VoterSnapshotStatus::<T>::get(),
			bounds,
			voters.len(),
		);
		Ok(voters)
	}

	fn electable_targets(
		bounds: DataProviderBounds,
		page: PageIndex,
	) -> data_provider::Result<Vec<T::AccountId>> {
		if page > 0 {
			log!(warn, "multi-page target snapshot not supported, returning page 0.");
		}

		let targets = Self::get_npos_targets(bounds);
		// We can't handle this case yet -- return an error. WIP to improve handling this case in
		// <https://github.com/paritytech/substrate/pull/13195>.
		if bounds.exhausted(None, CountBound(targets.len() as u32).into()) {
			return Err("Target snapshot too big")
		}

		debug_assert!(!bounds.slice_exhausted(&targets));

		Ok(targets)
	}

	fn next_election_prediction(_: BlockNumberFor<T>) -> BlockNumberFor<T> {
		debug_assert!(false, "this is deprecated and not used anymore");
		sp_runtime::traits::Bounded::max_value()
	}

	#[cfg(feature = "runtime-benchmarks")]
	fn fetch_page(page: PageIndex) {
		session_rotation::EraElectionPlanner::<T>::do_elect_paged(page);
	}

	#[cfg(feature = "runtime-benchmarks")]
	fn add_voter(
		voter: T::AccountId,
		weight: VoteWeight,
		targets: BoundedVec<T::AccountId, Self::MaxVotesPerVoter>,
	) {
		let stake = <BalanceOf<T>>::try_from(weight).unwrap_or_else(|_| {
			panic!("cannot convert a VoteWeight into BalanceOf, benchmark needs reconfiguring.")
		});
		<Bonded<T>>::insert(voter.clone(), voter.clone());
		<Ledger<T>>::insert(voter.clone(), StakingLedger::<T>::new(voter.clone(), stake));

		Self::do_add_nominator(&voter, Nominations { targets, submitted_in: 0, suppressed: false });
	}

	#[cfg(feature = "runtime-benchmarks")]
	fn add_target(target: T::AccountId) {
		let stake = (MinValidatorBond::<T>::get() + 1u32.into()) * 100u32.into();
		<Bonded<T>>::insert(target.clone(), target.clone());
		<Ledger<T>>::insert(target.clone(), StakingLedger::<T>::new(target.clone(), stake));
		Self::do_add_validator(
			&target,
			ValidatorPrefs { commission: Perbill::zero(), blocked: false },
		);
	}

	#[cfg(feature = "runtime-benchmarks")]
	fn clear() {
		#[allow(deprecated)]
		<Bonded<T>>::remove_all(None);
		#[allow(deprecated)]
		<Ledger<T>>::remove_all(None);
		#[allow(deprecated)]
		<Validators<T>>::remove_all();
		#[allow(deprecated)]
		<Nominators<T>>::remove_all();

		T::VoterList::unsafe_clear();
	}

	#[cfg(feature = "runtime-benchmarks")]
	fn put_snapshot(
		voters: Vec<VoterOf<Self>>,
		targets: Vec<T::AccountId>,
		target_stake: Option<VoteWeight>,
	) {
		targets.into_iter().for_each(|v| {
			let stake: BalanceOf<T> = target_stake
				.and_then(|w| <BalanceOf<T>>::try_from(w).ok())
				.unwrap_or_else(|| MinNominatorBond::<T>::get() * 100u32.into());
			<Bonded<T>>::insert(v.clone(), v.clone());
			<Ledger<T>>::insert(v.clone(), StakingLedger::<T>::new(v.clone(), stake));
			Self::do_add_validator(
				&v,
				ValidatorPrefs { commission: Perbill::zero(), blocked: false },
			);
		});

		voters.into_iter().for_each(|(v, s, t)| {
			let stake = <BalanceOf<T>>::try_from(s).unwrap_or_else(|_| {
				panic!("cannot convert a VoteWeight into BalanceOf, benchmark needs reconfiguring.")
			});
			<Bonded<T>>::insert(v.clone(), v.clone());
			<Ledger<T>>::insert(v.clone(), StakingLedger::<T>::new(v.clone(), stake));
			Self::do_add_nominator(
				&v,
				Nominations { targets: t, submitted_in: 0, suppressed: false },
			);
		});
	}

	#[cfg(feature = "runtime-benchmarks")]
	fn set_desired_targets(count: u32) {
		ValidatorCount::<T>::put(count);
	}
}

impl<T: Config> rc_client::AHStakingInterface for Pallet<T> {
	type AccountId = T::AccountId;
	type MaxValidatorSet = T::MaxValidatorSet;

	/// When we receive a session report from the relay chain, it kicks off the next session.
	///
	/// There are three special types of things we can do in a session:
	/// 1. Plan a new era: We do this one session before the expected era rotation.
	/// 2. Kick off election: We do this based on the [`Config::PlanningEraOffset`] configuration.
	/// 3. Activate Next Era: When we receive an activation timestamp in the session report, it
	/// implies a new validator set has been applied, and we must increment the active era to keep
	/// the systems in sync.
	fn on_relay_session_report(report: rc_client::SessionReport<Self::AccountId>) {
		log!(debug, "session report received\n{:?}", report,);
		let consumed_weight = T::WeightInfo::rc_on_session_report();

		let rc_client::SessionReport {
			end_index,
			activation_timestamp,
			validator_points,
			leftover,
		} = report;
		debug_assert!(!leftover);

		Eras::<T>::reward_active_era(validator_points.into_iter());
		session_rotation::Rotator::<T>::end_session(end_index, activation_timestamp);
		// NOTE: we might want to either return these weights so that they are registered in the
		// rc-client pallet, or directly benchmarked there, such that we can use them in the
		// "pre-dispatch" fashion. That said, since these are all `Mandatory` weights, it doesn't
		// make that big of a difference.
		Self::register_weight(consumed_weight);
	}

	fn on_new_offences(
		slash_session: SessionIndex,
		offences: Vec<rc_client::Offence<T::AccountId>>,
	) {
		log!(debug, "🦹 on_new_offences: {:?}", offences);
		let consumed_weight = T::WeightInfo::rc_on_offence(offences.len() as u32);

		// Find the era to which offence belongs.
		let Some(active_era) = ActiveEra::<T>::get() else {
			log!(warn, "🦹 on_new_offences: no active era; ignoring offence");
			return
		};

		let active_era_start_session =
			ErasStartSessionIndex::<T>::get(active_era.index).unwrap_or(0);

		// Fast path for active-era report - most likely.
		// `slash_session` cannot be in a future active era. It must be in `active_era` or before.
		let offence_era = if slash_session >= active_era_start_session {
			active_era.index
		} else {
			match BondedEras::<T>::get()
				.iter()
				// Reverse because it's more likely to find reports from recent eras.
				.rev()
				.find(|&(_, sesh)| sesh <= &slash_session)
				.map(|(era, _)| *era)
			{
				Some(era) => era,
				None => {
					// defensive: this implies offence is for a discarded era, and should already be
					// filtered out.
					log!(warn, "🦹 on_offence: no era found for slash_session; ignoring offence");
					return
				},
			}
		};

		let invulnerables = Invulnerables::<T>::get();

		for o in offences {
			let slash_fraction = o.slash_fraction;
			let validator: <T as frame_system::Config>::AccountId = o.offender.into();
			// Skip if the validator is invulnerable.
			if invulnerables.contains(&validator) {
				log!(debug, "🦹 on_offence: {:?} is invulnerable; ignoring offence", validator);
				continue
			}

			let Some(exposure_overview) = <ErasStakersOverview<T>>::get(&offence_era, &validator)
			else {
				// defensive: this implies offence is for a discarded era, and should already be
				// filtered out.
				log!(
					warn,
					"🦹 on_offence: no exposure found for {:?} in era {}; ignoring offence",
					validator,
					offence_era
				);
				continue;
			};

			Self::deposit_event(Event::<T>::OffenceReported {
				validator: validator.clone(),
				fraction: slash_fraction,
				offence_era,
			});

			let prior_slash_fraction = ValidatorSlashInEra::<T>::get(offence_era, &validator)
				.map_or(Zero::zero(), |(f, _)| f);

			if let Some(existing) = OffenceQueue::<T>::get(offence_era, &validator) {
				if slash_fraction.deconstruct() > existing.slash_fraction.deconstruct() {
					OffenceQueue::<T>::insert(
						offence_era,
						&validator,
						OffenceRecord {
							reporter: o.reporters.first().cloned(),
							reported_era: active_era.index,
							slash_fraction,
							..existing
						},
					);

					// update the slash fraction in the `ValidatorSlashInEra` storage.
					ValidatorSlashInEra::<T>::insert(
						offence_era,
						&validator,
						(slash_fraction, exposure_overview.own),
					);

					log!(
						debug,
						"🦹 updated slash for {:?}: {:?} (prior: {:?})",
						validator,
						slash_fraction,
						prior_slash_fraction,
					);
				} else {
					log!(
						debug,
						"🦹 ignored slash for {:?}: {:?} (existing prior is larger: {:?})",
						validator,
						slash_fraction,
						prior_slash_fraction,
					);
				}
			} else if slash_fraction.deconstruct() > prior_slash_fraction.deconstruct() {
				ValidatorSlashInEra::<T>::insert(
					offence_era,
					&validator,
					(slash_fraction, exposure_overview.own),
				);

				OffenceQueue::<T>::insert(
					offence_era,
					&validator,
					OffenceRecord {
						reporter: o.reporters.first().cloned(),
						reported_era: active_era.index,
						// there are cases of validator with no exposure, hence 0 page, so we
						// saturate to avoid underflow.
						exposure_page: exposure_overview.page_count.saturating_sub(1),
						slash_fraction,
						prior_slash_fraction,
					},
				);

				OffenceQueueEras::<T>::mutate(|q| {
					if let Some(eras) = q {
						log!(debug, "🦹 inserting offence era {} into existing queue", offence_era);
						eras.binary_search(&offence_era).err().map(|idx| {
							eras.try_insert(idx, offence_era).defensive_proof(
								"Offence era must be present in the existing queue",
							)
						});
					} else {
						let mut eras = BoundedVec::default();
						log!(debug, "🦹 inserting offence era {} into empty queue", offence_era);
						let _ = eras
							.try_push(offence_era)
							.defensive_proof("Failed to push offence era into empty queue");
						*q = Some(eras);
					}
				});

				log!(
					debug,
					"🦹 queued slash for {:?}: {:?} (prior: {:?})",
					validator,
					slash_fraction,
					prior_slash_fraction,
				);
			} else {
				log!(
					debug,
					"🦹 ignored slash for {:?}: {:?} (already slashed in era with prior: {:?})",
					validator,
					slash_fraction,
					prior_slash_fraction,
				);
			}
		}

		Self::register_weight(consumed_weight);
	}
}

impl<T: Config> ScoreProvider<T::AccountId> for Pallet<T> {
	type Score = VoteWeight;

	fn score(who: &T::AccountId) -> Option<Self::Score> {
		Self::ledger(Stash(who.clone()))
			.map(|l| l.active)
			.map(|a| {
				let issuance = asset::total_issuance::<T>();
				T::CurrencyToVote::to_vote(a, issuance)
			})
			.ok()
	}

	#[cfg(feature = "runtime-benchmarks")]
	fn set_score_of(who: &T::AccountId, weight: Self::Score) {
		// this will clearly results in an inconsistent state, but it should not matter for a
		// benchmark.
		let active: BalanceOf<T> = weight.try_into().map_err(|_| ()).unwrap();
		let mut ledger = match Self::ledger(StakingAccount::Stash(who.clone())) {
			Ok(l) => l,
			Err(_) => StakingLedger::default_from(who.clone()),
		};
		ledger.active = active;

		<Ledger<T>>::insert(who, ledger);
		<Bonded<T>>::insert(who, who);

		// also, we play a trick to make sure that a issuance based-`CurrencyToVote` behaves well:
		// This will make sure that total issuance is zero, thus the currency to vote will be a 1-1
		// conversion.
		let imbalance = asset::burn::<T>(asset::total_issuance::<T>());
		// kinda ugly, but gets the job done. The fact that this works here is a HUGE exception.
		// Don't try this pattern in other places.
		core::mem::forget(imbalance);
	}
}

/// A simple sorted list implementation that does not require any additional pallets. Note, this
/// does not provide validators in sorted order. If you desire nominators in a sorted order take
/// a look at [`pallet-bags-list`].
pub struct UseValidatorsMap<T>(core::marker::PhantomData<T>);
impl<T: Config> SortedListProvider<T::AccountId> for UseValidatorsMap<T> {
	type Score = BalanceOf<T>;
	type Error = ();

	/// Returns iterator over voter list, which can have `take` called on it.
	fn iter() -> Box<dyn Iterator<Item = T::AccountId>> {
		Box::new(Validators::<T>::iter().map(|(v, _)| v))
	}
	fn iter_from(
		start: &T::AccountId,
	) -> Result<Box<dyn Iterator<Item = T::AccountId>>, Self::Error> {
		if Validators::<T>::contains_key(start) {
			let start_key = Validators::<T>::hashed_key_for(start);
			Ok(Box::new(Validators::<T>::iter_from(start_key).map(|(n, _)| n)))
		} else {
			Err(())
		}
	}
	fn lock() {}
	fn unlock() {}
	fn count() -> u32 {
		Validators::<T>::count()
	}
	fn contains(id: &T::AccountId) -> bool {
		Validators::<T>::contains_key(id)
	}
	fn on_insert(_: T::AccountId, _weight: Self::Score) -> Result<(), Self::Error> {
		// nothing to do on insert.
		Ok(())
	}
	fn get_score(id: &T::AccountId) -> Result<Self::Score, Self::Error> {
		Ok(Pallet::<T>::weight_of(id).into())
	}
	fn on_update(_: &T::AccountId, _weight: Self::Score) -> Result<(), Self::Error> {
		// nothing to do on update.
		Ok(())
	}
	fn on_remove(_: &T::AccountId) -> Result<(), Self::Error> {
		// nothing to do on remove.
		Ok(())
	}
	fn unsafe_regenerate(
		_: impl IntoIterator<Item = T::AccountId>,
		_: Box<dyn Fn(&T::AccountId) -> Option<Self::Score>>,
	) -> u32 {
		// nothing to do upon regenerate.
		0
	}
	#[cfg(feature = "try-runtime")]
	fn try_state() -> Result<(), TryRuntimeError> {
		Ok(())
	}

	fn unsafe_clear() {
		#[allow(deprecated)]
		Validators::<T>::remove_all();
	}

	#[cfg(feature = "runtime-benchmarks")]
	fn score_update_worst_case(_who: &T::AccountId, _is_increase: bool) -> Self::Score {
		unimplemented!()
	}
}

/// A simple voter list implementation that does not require any additional pallets. Note, this
/// does not provided nominators in sorted ordered. If you desire nominators in a sorted order take
/// a look at [`pallet-bags-list].
pub struct UseNominatorsAndValidatorsMap<T>(core::marker::PhantomData<T>);
impl<T: Config> SortedListProvider<T::AccountId> for UseNominatorsAndValidatorsMap<T> {
	type Error = ();
	type Score = VoteWeight;

	fn iter() -> Box<dyn Iterator<Item = T::AccountId>> {
		Box::new(
			Validators::<T>::iter()
				.map(|(v, _)| v)
				.chain(Nominators::<T>::iter().map(|(n, _)| n)),
		)
	}
	fn iter_from(
		start: &T::AccountId,
	) -> Result<Box<dyn Iterator<Item = T::AccountId>>, Self::Error> {
		if Validators::<T>::contains_key(start) {
			let start_key = Validators::<T>::hashed_key_for(start);
			Ok(Box::new(
				Validators::<T>::iter_from(start_key)
					.map(|(n, _)| n)
					.chain(Nominators::<T>::iter().map(|(x, _)| x)),
			))
		} else if Nominators::<T>::contains_key(start) {
			let start_key = Nominators::<T>::hashed_key_for(start);
			Ok(Box::new(Nominators::<T>::iter_from(start_key).map(|(n, _)| n)))
		} else {
			Err(())
		}
	}
	fn lock() {}
	fn unlock() {}
	fn count() -> u32 {
		Nominators::<T>::count().saturating_add(Validators::<T>::count())
	}
	fn contains(id: &T::AccountId) -> bool {
		Nominators::<T>::contains_key(id) || Validators::<T>::contains_key(id)
	}
	fn on_insert(_: T::AccountId, _weight: Self::Score) -> Result<(), Self::Error> {
		// nothing to do on insert.
		Ok(())
	}
	fn get_score(id: &T::AccountId) -> Result<Self::Score, Self::Error> {
		Ok(Pallet::<T>::weight_of(id))
	}
	fn on_update(_: &T::AccountId, _weight: Self::Score) -> Result<(), Self::Error> {
		// nothing to do on update.
		Ok(())
	}
	fn on_remove(_: &T::AccountId) -> Result<(), Self::Error> {
		// nothing to do on remove.
		Ok(())
	}
	fn unsafe_regenerate(
		_: impl IntoIterator<Item = T::AccountId>,
		_: Box<dyn Fn(&T::AccountId) -> Option<Self::Score>>,
	) -> u32 {
		// nothing to do upon regenerate.
		0
	}

	#[cfg(feature = "try-runtime")]
	fn try_state() -> Result<(), TryRuntimeError> {
		Ok(())
	}

	fn unsafe_clear() {
		// NOTE: Caller must ensure this doesn't lead to too many storage accesses. This is a
		// condition of SortedListProvider::unsafe_clear.
		#[allow(deprecated)]
		Nominators::<T>::remove_all();
		#[allow(deprecated)]
		Validators::<T>::remove_all();
	}

	#[cfg(feature = "runtime-benchmarks")]
	fn score_update_worst_case(_who: &T::AccountId, _is_increase: bool) -> Self::Score {
		unimplemented!()
	}
}

impl<T: Config> StakingInterface for Pallet<T> {
	type AccountId = T::AccountId;
	type Balance = BalanceOf<T>;
	type CurrencyToVote = T::CurrencyToVote;

	fn minimum_nominator_bond() -> Self::Balance {
		MinNominatorBond::<T>::get()
	}

	fn minimum_validator_bond() -> Self::Balance {
		MinValidatorBond::<T>::get()
	}

	fn stash_by_ctrl(controller: &Self::AccountId) -> Result<Self::AccountId, DispatchError> {
		Self::ledger(Controller(controller.clone()))
			.map(|l| l.stash)
			.map_err(|e| e.into())
	}

	fn bonding_duration() -> EraIndex {
		T::BondingDuration::get()
	}

	fn current_era() -> EraIndex {
		CurrentEra::<T>::get().unwrap_or(Zero::zero())
	}

	fn stake(who: &Self::AccountId) -> Result<Stake<BalanceOf<T>>, DispatchError> {
		Self::ledger(Stash(who.clone()))
			.map(|l| Stake { total: l.total, active: l.active })
			.map_err(|e| e.into())
	}

	fn bond_extra(who: &Self::AccountId, extra: Self::Balance) -> DispatchResult {
		Self::bond_extra(RawOrigin::Signed(who.clone()).into(), extra)
	}

	fn unbond(who: &Self::AccountId, value: Self::Balance) -> DispatchResult {
		let ctrl = Self::bonded(who).ok_or(Error::<T>::NotStash)?;
		Self::unbond(RawOrigin::Signed(ctrl).into(), value)
			.map_err(|with_post| with_post.error)
			.map(|_| ())
	}

	fn set_payee(stash: &Self::AccountId, reward_acc: &Self::AccountId) -> DispatchResult {
		// Since virtual stakers are not allowed to compound their rewards as this pallet does not
		// manage their locks, we do not allow reward account to be set same as stash. For
		// external pallets that manage the virtual bond, they can claim rewards and re-bond them.
		ensure!(
			!Self::is_virtual_staker(stash) || stash != reward_acc,
			Error::<T>::RewardDestinationRestricted
		);

		let ledger = Self::ledger(Stash(stash.clone()))?;
		let _ = ledger
			.set_payee(RewardDestination::Account(reward_acc.clone()))
			.defensive_proof("ledger was retrieved from storage, thus its bonded; qed.")?;

		Ok(())
	}

	fn chill(who: &Self::AccountId) -> DispatchResult {
		// defensive-only: any account bonded via this interface has the stash set as the
		// controller, but we have to be sure. Same comment anywhere else that we read this.
		let ctrl = Self::bonded(who).ok_or(Error::<T>::NotStash)?;
		Self::chill(RawOrigin::Signed(ctrl).into())
	}

	fn withdraw_unbonded(
		who: Self::AccountId,
		num_slashing_spans: u32,
	) -> Result<bool, DispatchError> {
		let ctrl = Self::bonded(&who).ok_or(Error::<T>::NotStash)?;
		Self::withdraw_unbonded(RawOrigin::Signed(ctrl.clone()).into(), num_slashing_spans)
			.map(|_| !StakingLedger::<T>::is_bonded(StakingAccount::Controller(ctrl)))
			.map_err(|with_post| with_post.error)
	}

	fn bond(
		who: &Self::AccountId,
		value: Self::Balance,
		payee: &Self::AccountId,
	) -> DispatchResult {
		Self::bond(
			RawOrigin::Signed(who.clone()).into(),
			value,
			RewardDestination::Account(payee.clone()),
		)
	}

	fn nominate(who: &Self::AccountId, targets: Vec<Self::AccountId>) -> DispatchResult {
		let ctrl = Self::bonded(who).ok_or(Error::<T>::NotStash)?;
		let targets = targets.into_iter().map(T::Lookup::unlookup).collect::<Vec<_>>();
		Self::nominate(RawOrigin::Signed(ctrl).into(), targets)
	}

	fn desired_validator_count() -> u32 {
		ValidatorCount::<T>::get()
	}

	fn election_ongoing() -> bool {
		<T::ElectionProvider as ElectionProvider>::status().is_ok()
	}

	fn force_unstake(who: Self::AccountId) -> sp_runtime::DispatchResult {
		let num_slashing_spans =
			SlashingSpans::<T>::get(&who).map_or(0, |s| s.iter().count() as u32);
		Self::force_unstake(RawOrigin::Root.into(), who.clone(), num_slashing_spans)
	}

	fn is_exposed_in_era(who: &Self::AccountId, era: &EraIndex) -> bool {
		ErasStakersPaged::<T>::iter_prefix((era,)).any(|((validator, _), exposure_page)| {
			validator == *who || exposure_page.others.iter().any(|i| i.who == *who)
		})
	}

	fn status(
		who: &Self::AccountId,
	) -> Result<sp_staking::StakerStatus<Self::AccountId>, DispatchError> {
		if !StakingLedger::<T>::is_bonded(StakingAccount::Stash(who.clone())) {
			return Err(Error::<T>::NotStash.into())
		}

		let is_validator = Validators::<T>::contains_key(&who);
		let is_nominator = Nominators::<T>::get(&who);

		use sp_staking::StakerStatus;
		match (is_validator, is_nominator.is_some()) {
			(false, false) => Ok(StakerStatus::Idle),
			(true, false) => Ok(StakerStatus::Validator),
			(false, true) => Ok(StakerStatus::Nominator(
				is_nominator.expect("is checked above; qed").targets.into_inner(),
			)),
			(true, true) => {
				defensive!("cannot be both validators and nominator");
				Err(Error::<T>::BadState.into())
			},
		}
	}

	/// Whether `who` is a virtual staker whose funds are managed by another pallet.
	///
	/// There is an assumption that, this account is keyless and managed by another pallet in the
	/// runtime. Hence, it can never sign its own transactions.
	fn is_virtual_staker(who: &T::AccountId) -> bool {
		frame_system::Pallet::<T>::account_nonce(who).is_zero() &&
			VirtualStakers::<T>::contains_key(who)
	}

	fn slash_reward_fraction() -> Perbill {
		SlashRewardFraction::<T>::get()
	}

	sp_staking::runtime_benchmarks_enabled! {
		fn nominations(who: &Self::AccountId) -> Option<Vec<T::AccountId>> {
			Nominators::<T>::get(who).map(|n| n.targets.into_inner())
		}

		fn add_era_stakers(
			current_era: &EraIndex,
			stash: &T::AccountId,
			exposures: Vec<(Self::AccountId, Self::Balance)>,
		) {
			let others = exposures
				.iter()
				.map(|(who, value)| crate::IndividualExposure { who: who.clone(), value: *value })
				.collect::<Vec<_>>();
			let exposure = Exposure { total: Default::default(), own: Default::default(), others };
			Eras::<T>::upsert_exposure(*current_era, stash, exposure);
		}

		fn set_current_era(era: EraIndex) {
			CurrentEra::<T>::put(era);
		}

		fn max_exposure_page_size() -> Page {
			T::MaxExposurePageSize::get()
		}
	}
}

impl<T: Config> sp_staking::StakingUnchecked for Pallet<T> {
	fn migrate_to_virtual_staker(who: &Self::AccountId) -> DispatchResult {
		asset::kill_stake::<T>(who)?;
		VirtualStakers::<T>::insert(who, ());
		Ok(())
	}

	/// Virtually bonds `keyless_who` to `payee` with `value`.
	///
	/// The payee must not be the same as the `keyless_who`.
	fn virtual_bond(
		keyless_who: &Self::AccountId,
		value: Self::Balance,
		payee: &Self::AccountId,
	) -> DispatchResult {
		if StakingLedger::<T>::is_bonded(StakingAccount::Stash(keyless_who.clone())) {
			return Err(Error::<T>::AlreadyBonded.into())
		}

		// check if payee not same as who.
		ensure!(keyless_who != payee, Error::<T>::RewardDestinationRestricted);

		// mark who as a virtual staker.
		VirtualStakers::<T>::insert(keyless_who, ());

		Self::deposit_event(Event::<T>::Bonded { stash: keyless_who.clone(), amount: value });
		let ledger = StakingLedger::<T>::new(keyless_who.clone(), value);

		ledger.bond(RewardDestination::Account(payee.clone()))?;

		Ok(())
	}

	/// Only meant to be used in tests.
	#[cfg(feature = "runtime-benchmarks")]
	fn migrate_to_direct_staker(who: &Self::AccountId) {
		assert!(VirtualStakers::<T>::contains_key(who));
		let ledger = StakingLedger::<T>::get(Stash(who.clone())).unwrap();
		let _ = asset::update_stake::<T>(who, ledger.total)
			.expect("funds must be transferred to stash");
		VirtualStakers::<T>::remove(who);
	}
}

#[cfg(any(test, feature = "try-runtime"))]
impl<T: Config> Pallet<T> {
	pub(crate) fn do_try_state(_now: BlockNumberFor<T>) -> Result<(), TryRuntimeError> {
		session_rotation::Rotator::<T>::do_try_state()?;
		session_rotation::Eras::<T>::do_try_state()?;
		Self::check_ledgers()?;
		Self::check_bonded_consistency()?;
		Self::check_payees()?;
		Self::check_paged_exposures()?;
		Self::check_count()?;

		Ok(())
	}

	/// Invariants:
	/// * A controller should not be associated with more than one ledger.
	/// * A bonded (stash, controller) pair should have only one associated ledger. I.e. if the
	///   ledger is bonded by stash, the controller account must not bond a different ledger.
	/// * A bonded (stash, controller) pair must have an associated ledger.
	///
	/// NOTE: these checks result in warnings only. Once
	/// <https://github.com/paritytech/polkadot-sdk/issues/3245> is resolved, turn warns into check
	/// failures.
	fn check_bonded_consistency() -> Result<(), TryRuntimeError> {
		use alloc::collections::btree_set::BTreeSet;

		let mut count_controller_double = 0;
		let mut count_double = 0;
		let mut count_none = 0;
		// sanity check to ensure that each controller in Bonded storage is associated with only one
		// ledger.
		let mut controllers = BTreeSet::new();

		for (stash, controller) in <Bonded<T>>::iter() {
			if !controllers.insert(controller.clone()) {
				count_controller_double += 1;
			}

			match (<Ledger<T>>::get(&stash), <Ledger<T>>::get(&controller)) {
				(Some(_), Some(_)) =>
				// if stash == controller, it means that the ledger has migrated to
				// post-controller. If no migration happened, we expect that the (stash,
				// controller) pair has only one associated ledger.
					if stash != controller {
						count_double += 1;
					},
				(None, None) => {
					count_none += 1;
				},
				_ => {},
			};
		}

		if count_controller_double != 0 {
			log!(
				warn,
				"a controller is associated with more than one ledger ({} occurrences)",
				count_controller_double
			);
		};

		if count_double != 0 {
			log!(warn, "single tuple of (stash, controller) pair bonds more than one ledger ({} occurrences)", count_double);
		}

		if count_none != 0 {
			log!(warn, "inconsistent bonded state: (stash, controller) pair missing associated ledger ({} occurrences)", count_none);
		}

		Ok(())
	}

	/// Invariants:
	/// * A bonded ledger should always have an assigned `Payee`.
	/// * The number of entries in `Payee` and of bonded staking ledgers *must* match.
	/// * The stash account in the ledger must match that of the bonded account.
	fn check_payees() -> Result<(), TryRuntimeError> {
		for (stash, _) in Bonded::<T>::iter() {
			ensure!(Payee::<T>::get(&stash).is_some(), "bonded ledger does not have payee set");
		}

		ensure!(
			(Ledger::<T>::iter().count() == Payee::<T>::iter().count()) &&
				(Ledger::<T>::iter().count() == Bonded::<T>::iter().count()),
			"number of entries in payee storage items does not match the number of bonded ledgers",
		);

		Ok(())
	}

	/// Invariants:
	/// * Number of voters in `VoterList` match that of the number of Nominators and Validators in
	/// the system (validator is both voter and target).
	/// * Number of targets in `TargetList` matches the number of validators in the system.
	/// * Current validator count is bounded by the election provider's max winners.
	fn check_count() -> Result<(), TryRuntimeError> {
		ensure!(
			<T as Config>::VoterList::count() ==
				Nominators::<T>::count() + Validators::<T>::count(),
			"wrong external count"
		);
		ensure!(
			<T as Config>::TargetList::count() == Validators::<T>::count(),
			"wrong external count"
		);
		let max_validators_bound = crate::MaxWinnersOf::<T>::get();
		let max_winners_per_page_bound = crate::MaxWinnersPerPageOf::<T::ElectionProvider>::get();
		ensure!(
			max_validators_bound >= max_winners_per_page_bound,
			"max validators should be higher than per page bounds"
		);
		ensure!(ValidatorCount::<T>::get() <= max_validators_bound, Error::<T>::TooManyValidators);
		Ok(())
	}

	/// Invariants:
	/// * Stake consistency: ledger.total == ledger.active + sum(ledger.unlocking).
	/// * The ledger's controller and stash matches the associated `Bonded` tuple.
	/// * Staking locked funds for every bonded stash (non virtual stakers) should be the same as
	/// its ledger's total.
	/// * For virtual stakers, locked funds should be zero and payee should be non-stash account.
	/// * Staking ledger and bond are not corrupted.
	fn check_ledgers() -> Result<(), TryRuntimeError> {
		Bonded::<T>::iter()
			.map(|(stash, ctrl)| {
				// ensure locks consistency.
				if VirtualStakers::<T>::contains_key(stash.clone()) {
					ensure!(
						asset::staked::<T>(&stash) == Zero::zero(),
						"virtual stakers should not have any staked balance"
					);
					ensure!(
						<Bonded<T>>::get(stash.clone()).unwrap() == stash.clone(),
						"stash and controller should be same"
					);
					ensure!(
						Ledger::<T>::get(stash.clone()).unwrap().stash == stash,
						"ledger corrupted for virtual staker"
					);
					ensure!(
						frame_system::Pallet::<T>::account_nonce(&stash).is_zero(),
						"virtual stakers are keyless and should not have any nonce"
					);
					let reward_destination = <Payee<T>>::get(stash.clone()).unwrap();
					if let RewardDestination::Account(payee) = reward_destination {
						ensure!(
							payee != stash.clone(),
							"reward destination should not be same as stash for virtual staker"
						);
					} else {
						return Err(DispatchError::Other(
							"reward destination must be of account variant for virtual staker",
						));
					}
				} else {
					ensure!(
						Self::inspect_bond_state(&stash) == Ok(LedgerIntegrityState::Ok),
						"bond, ledger and/or staking hold inconsistent for a bonded stash."
					);
				}

				// ensure ledger consistency.
				Self::ensure_ledger_consistent(ctrl)
			})
			.collect::<Result<Vec<_>, _>>()?;
		Ok(())
	}

	/// Invariants:
	/// * For each paged era exposed validator, check if the exposure total is sane (exposure.total
	/// = exposure.own + exposure.own).
	/// * Paged exposures metadata (`ErasStakersOverview`) matches the paged exposures state.
	fn check_paged_exposures() -> Result<(), TryRuntimeError> {
		use alloc::collections::btree_map::BTreeMap;
		use sp_staking::PagedExposureMetadata;

		// Sanity check for the paged exposure of the active era.
		let mut exposures: BTreeMap<T::AccountId, PagedExposureMetadata<BalanceOf<T>>> =
			BTreeMap::new();
		let era = ActiveEra::<T>::get().unwrap().index;
		let accumulator_default = PagedExposureMetadata {
			total: Zero::zero(),
			own: Zero::zero(),
			nominator_count: 0,
			page_count: 0,
		};

		ErasStakersPaged::<T>::iter_prefix((era,))
			.map(|((validator, _page), expo)| {
				ensure!(
					expo.page_total ==
						expo.others.iter().map(|e| e.value).fold(Zero::zero(), |acc, x| acc + x),
					"wrong total exposure for the page.",
				);

				let metadata = exposures.get(&validator).unwrap_or(&accumulator_default);
				exposures.insert(
					validator,
					PagedExposureMetadata {
						total: metadata.total + expo.page_total,
						own: metadata.own,
						nominator_count: metadata.nominator_count + expo.others.len() as u32,
						page_count: metadata.page_count + 1,
					},
				);

				Ok(())
			})
			.collect::<Result<(), TryRuntimeError>>()?;

		exposures
			.iter()
			.map(|(validator, metadata)| {
				let actual_overview = ErasStakersOverview::<T>::get(era, validator);

				ensure!(actual_overview.is_some(), "No overview found for a paged exposure");
				let actual_overview = actual_overview.unwrap();

				ensure!(
					actual_overview.total == metadata.total + actual_overview.own,
					"Exposure metadata does not have correct total exposed stake."
				);
				ensure!(
					actual_overview.nominator_count == metadata.nominator_count,
					"Exposure metadata does not have correct count of nominators."
				);
				ensure!(
					actual_overview.page_count == metadata.page_count,
					"Exposure metadata does not have correct count of pages."
				);

				Ok(())
			})
			.collect::<Result<(), TryRuntimeError>>()
	}

	fn ensure_ledger_consistent(ctrl: T::AccountId) -> Result<(), TryRuntimeError> {
		// ensures ledger.total == ledger.active + sum(ledger.unlocking).
		let ledger = Self::ledger(StakingAccount::Controller(ctrl.clone()))?;

		let real_total: BalanceOf<T> =
			ledger.unlocking.iter().fold(ledger.active, |a, c| a + c.value);
		ensure!(real_total == ledger.total, "ledger.total corrupt");

		Ok(())
	}
}
