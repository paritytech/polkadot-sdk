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

//! Manages all era rotation logic based on session increments.
//!
//! # Lifecycle:
//!
//! When a session ends in RC, a session report is sent to AH with the ending session index. Given
//! there are 6 sessions per Era, and we configure the PlanningEraOffset to be 1, the following
//! happens.
//!
//! ## Idle Sessions
//! In the happy path, first 3 sessions are idle. Nothing much happens in these sessions.
//!
//!
//! ## Planning New Era Session
//! In the happy path, `planning new era` session is initiated when 3rd session ends and the 4th
//! starts in the active era.
//!
//! **Triggers**
//! 1. `SessionProgress == SessionsPerEra - PlanningEraOffset`
//! 2. Forcing is set to `ForceNew` or `ForceAlways`
//!
//! **Actions**
//! 1. Triggers the election process,
//! 2. Updates the CurrentEra.
//!
//! **SkipIf**
//! CurrentEra = ActiveEra + 1 // this implies planning session has already been triggered.
//!
//! **FollowUp**
//! When the election process is over, we send the new validator set, with the CurrentEra index
//! as the id of the validator set.
//!
//!
//! ## Era Rotation Session
//! In the happy path, this happens when the 5th session ends and the 6th starts in the active era.
//!
//! **Triggers**
//! When we receive an activation timestamp from RC.
//!
//! **Assertions**
//! 1. CurrentEra must be ActiveEra + 1.
//! 2. Id of the activation timestamp same as CurrentEra.
//!
//! **Actions**
//! - Finalize the currently active era.
//! - Increment ActiveEra by 1.
//! - Cleanup the old era information.
//! - Set ErasStartSessionIndex with the activating era index and starting session index.
//!
//! **Exceptional Scenarios**
//! - Delay in exporting validator set: Triggered in a session later than 7th.
//! - Forcing Era: May triggered in a session earlier than 7th.
//!
//! ## Example Flow of a happy path
//!
//! * end 0, start 1, plan 2
//! * end 1, start 2, plan 3
//! * end 2, start 3, plan 4
//! * end 3, start 4, plan 5 // `Plan new era` session. Current Era++. Trigger Election.
//! * **** Somewhere here: Election set is sent to RC, keyed with Current Era
//! * end 4, start 5, plan 6 // RC::session receives and queues this set.
//! * end 5, start 6, plan 7 // Session report contains activation timestamp with Current Era.

use crate::{
	asset, log, slashing, ActiveEra, ActiveEraInfo, BalanceOf, BondedEras, Config, CurrentEra,
	EraIndex, EraPayout, EraRewardPoints, ErasClaimedRewards, ErasRewardPoints,
	ErasStakersOverview, ErasStakersPaged, ErasStartSessionIndex, ErasTotalStake,
	ErasValidatorPrefs, ErasValidatorReward, Event, ForceEra, Forcing, MaxStakedRewards,
	PagedExposure, Pallet, ValidatorPrefs,
};
use alloc::vec::Vec;
use frame_election_provider_support::ElectionProvider;
use frame_support::{
	pallet_prelude::*,
	traits::{Defensive, DefensiveMax, DefensiveSaturating, OnUnbalanced},
};
use sp_runtime::{Perbill, Percent, Saturating};
use sp_staking::{Exposure, Page, PagedExposureMetadata, SessionIndex};

/// A handler for all era-based storage items.
///
/// All of the following storage items must be controlled by this type:
///
/// [`ErasValidatorPrefs`]
/// [`ErasClaimedRewards`]
/// [`ErasStakersPaged`]
/// [`ErasStakersOverview`]
/// [`ErasValidatorReward`]
/// [`ErasRewardPoints`]
/// [`ErasTotalStake`]
/// [`ErasStartSessionIndex`]
///
/// All of the invariants are expressed in [`Eras::sanity_check`].
pub struct Eras<T: Config>(core::marker::PhantomData<T>);

impl<T: Config> Eras<T> {
	pub(crate) fn sanity_check() {
		todo!();
	}

	pub(crate) fn prune_era(era: EraIndex) {
		// TODO: lazy deletion -- after an era is marked is delete-able, all of the info associated
		// with it can be removed.
		let mut cursor = <ErasValidatorPrefs<T>>::clear_prefix(era, u32::MAX, None);
		debug_assert!(cursor.maybe_cursor.is_none());
		cursor = <ErasClaimedRewards<T>>::clear_prefix(era, u32::MAX, None);
		debug_assert!(cursor.maybe_cursor.is_none());
		cursor = <ErasStakersPaged<T>>::clear_prefix((era,), u32::MAX, None);
		debug_assert!(cursor.maybe_cursor.is_none());
		cursor = <ErasStakersOverview<T>>::clear_prefix(era, u32::MAX, None);
		debug_assert!(cursor.maybe_cursor.is_none());

		<ErasValidatorReward<T>>::remove(era);
		<ErasRewardPoints<T>>::remove(era);
		<ErasTotalStake<T>>::remove(era);
		ErasStartSessionIndex::<T>::remove(era);
	}

	pub(crate) fn set_validator_prefs(era: EraIndex, stash: &T::AccountId, prefs: ValidatorPrefs) {
		debug_assert_eq!(era, Rotator::<T>::planning_era(), "we only set prefs for planning era");
		<ErasValidatorPrefs<T>>::insert(era, stash, prefs);
	}

	pub(crate) fn get_validator_prefs(era: EraIndex, stash: &T::AccountId) -> ValidatorPrefs {
		<ErasValidatorPrefs<T>>::get(era, stash)
	}

	/// Returns validator commission for this era and page.
	pub(crate) fn get_validator_commission(era: EraIndex, stash: &T::AccountId) -> Perbill {
		Self::get_validator_prefs(era, stash).commission
	}

	/// Returns true if validator has one or more page of era rewards not claimed yet.
	pub(crate) fn pending_rewards(era: EraIndex, validator: &T::AccountId) -> bool {
		<ErasStakersOverview<T>>::get(&era, validator)
			.map(|overview| {
				ErasClaimedRewards::<T>::get(era, validator).len() < overview.page_count as usize
			})
			.unwrap_or(false)
	}

	/// Get exposure for a validator at a given era and page.
	///
	/// This builds a paged exposure from `PagedExposureMetadata` and `ExposurePage` of the
	/// validator. For older non-paged exposure, it returns the clipped exposure directly.
	pub(crate) fn get_paged_exposure(
		era: EraIndex,
		validator: &T::AccountId,
		page: Page,
	) -> Option<PagedExposure<T::AccountId, BalanceOf<T>>> {
		let overview = <ErasStakersOverview<T>>::get(&era, validator)?;

		// validator stake is added only in page zero
		let validator_stake = if page == 0 { overview.own } else { Zero::zero() };

		// since overview is present, paged exposure will always be present except when a
		// validator has only own stake and no nominator stake.
		let exposure_page = <ErasStakersPaged<T>>::get((era, validator, page)).unwrap_or_default();

		// build the exposure
		Some(PagedExposure {
			exposure_metadata: PagedExposureMetadata { own: validator_stake, ..overview },
			exposure_page,
		})
	}

	/// Get full exposure of the validator at a given era.
	pub(crate) fn get_full_exposure(
		era: EraIndex,
		validator: &T::AccountId,
	) -> Exposure<T::AccountId, BalanceOf<T>> {
		let Some(overview) = <ErasStakersOverview<T>>::get(&era, validator) else {
			return Exposure::default();
		};

		let mut others = Vec::with_capacity(overview.nominator_count as usize);
		for page in 0..overview.page_count {
			let nominators = <ErasStakersPaged<T>>::get((era, validator, page));
			others.append(&mut nominators.map(|n| n.others).defensive_unwrap_or_default());
		}

		Exposure { total: overview.total, own: overview.own, others }
	}

	/// Returns the number of pages of exposure a validator has for the given era.
	///
	/// For eras where paged exposure does not exist, this returns 1 to keep backward compatibility.
	pub(crate) fn exposure_page_count(era: EraIndex, validator: &T::AccountId) -> Page {
		<ErasStakersOverview<T>>::get(&era, validator)
			.map(|overview| {
				if overview.page_count == 0 && overview.own > Zero::zero() {
					// Even though there are no nominator pages, there is still validator's own
					// stake exposed which needs to be paid out in a page.
					1
				} else {
					overview.page_count
				}
			})
			// Always returns 1 page for older non-paged exposure.
			// FIXME: Can be cleaned up with issue #13034.
			.unwrap_or(1)
	}

	/// Returns the next page that can be claimed or `None` if nothing to claim.
	pub(crate) fn get_next_claimable_page(era: EraIndex, validator: &T::AccountId) -> Option<Page> {
		// Find next claimable page of paged exposure.
		let page_count = Self::exposure_page_count(era, validator);
		let all_claimable_pages: Vec<Page> = (0..page_count).collect();
		let claimed_pages = ErasClaimedRewards::<T>::get(era, validator);

		all_claimable_pages.into_iter().find(|p| !claimed_pages.contains(p))
	}

	/// Creates an entry to track validator reward has been claimed for a given era and page.
	/// Noop if already claimed.
	pub(crate) fn set_rewards_as_claimed(era: EraIndex, validator: &T::AccountId, page: Page) {
		let mut claimed_pages = ErasClaimedRewards::<T>::get(era, validator);

		// this should never be called if the reward has already been claimed
		if claimed_pages.contains(&page) {
			defensive!("Trying to set an already claimed reward");
			// nevertheless don't do anything since the page already exist in claimed rewards.
			return
		}

		// add page to claimed entries
		claimed_pages.push(page);
		ErasClaimedRewards::<T>::insert(era, validator, claimed_pages);
	}

	/// Store exposure for elected validators at start of an era.
	///
	/// If the exposure does not exist yet for the tuple (era, validator), it sets it. Otherwise,
	/// it updates the existing record by ensuring *intermediate* exposure pages are filled up with
	/// `T::MaxExposurePageSize` number of backers per page and the remaining exposures are added
	/// to new exposure pages.
	pub fn upsert_exposure(
		era: EraIndex,
		validator: &T::AccountId,
		mut exposure: Exposure<T::AccountId, BalanceOf<T>>,
	) {
		let page_size = T::MaxExposurePageSize::get().defensive_max(1);

		if let Some(stored_overview) = ErasStakersOverview::<T>::get(era, &validator) {
			let last_page_idx = stored_overview.page_count.saturating_sub(1);

			let mut last_page =
				ErasStakersPaged::<T>::get((era, validator, last_page_idx)).unwrap_or_default();
			let last_page_empty_slots =
				T::MaxExposurePageSize::get().saturating_sub(last_page.others.len() as u32);

			// splits the exposure so that `exposures_append` will fit within the last exposure
			// page, up to the max exposure page size. The remaining individual exposures in
			// `exposure` will be added to new pages.
			let exposures_append = exposure.split_others(last_page_empty_slots);

			ErasStakersOverview::<T>::mutate(era, &validator, |stored| {
				// new metadata is updated based on 3 different set of exposures: the
				// current one, the exposure split to be "fitted" into the current last page and
				// the exposure set that will be appended from the new page onwards.
				let new_metadata =
					stored.defensive_unwrap_or_default().update_with::<T::MaxExposurePageSize>(
						[&exposures_append, &exposure]
							.iter()
							.fold(Default::default(), |total, expo| {
								total.saturating_add(expo.total.saturating_sub(expo.own))
							}),
						[&exposures_append, &exposure]
							.iter()
							.fold(Default::default(), |count, expo| {
								count.saturating_add(expo.others.len() as u32)
							}),
					);
				*stored = new_metadata.into();
			});

			// fill up last page with exposures.
			last_page.page_total = last_page
				.page_total
				.saturating_add(exposures_append.total)
				.saturating_sub(exposures_append.own);
			last_page.others.extend(exposures_append.others);
			ErasStakersPaged::<T>::insert((era, &validator, last_page_idx), last_page);

			// now handle the remaining exposures and append the exposure pages. The metadata update
			// has been already handled above.
			let (_, exposure_pages) = exposure.into_pages(page_size);

			exposure_pages.iter().enumerate().for_each(|(idx, paged_exposure)| {
				let append_at =
					(last_page_idx.saturating_add(1).saturating_add(idx as u32)) as Page;
				<ErasStakersPaged<T>>::insert((era, &validator, append_at), &paged_exposure);
			});
		} else {
			// expected page count is the number of nominators divided by the page size, rounded up.
			let expected_page_count = exposure
				.others
				.len()
				.defensive_saturating_add((page_size as usize).defensive_saturating_sub(1))
				.saturating_div(page_size as usize);

			// no exposures yet for this (era, validator) tuple, calculate paged exposure pages and
			// metadata from a blank slate.
			let (exposure_metadata, exposure_pages) = exposure.into_pages(page_size);
			defensive_assert!(exposure_pages.len() == expected_page_count, "unexpected page count");

			// insert metadata.
			ErasStakersOverview::<T>::insert(era, &validator, exposure_metadata);

			// insert validator's overview.
			exposure_pages.iter().enumerate().for_each(|(idx, paged_exposure)| {
				let append_at = idx as Page;
				<ErasStakersPaged<T>>::insert((era, &validator, append_at), &paged_exposure);
			});
		};
	}

	pub(crate) fn set_validators_reward(era: EraIndex, amount: BalanceOf<T>) {
		ErasValidatorReward::<T>::insert(era, amount);
	}

	pub(crate) fn get_validators_reward(era: EraIndex) -> Option<BalanceOf<T>> {
		ErasValidatorReward::<T>::get(era)
	}

	/// Update the total exposure for all the elected validators in the era.
	pub(crate) fn add_total_stake(era: EraIndex, stake: BalanceOf<T>) {
		<ErasTotalStake<T>>::mutate(era, |total_stake| {
			*total_stake += stake;
		});
	}

	/// Check if the rewards for the given era and page index have been claimed.
	pub(crate) fn is_rewards_claimed(era: EraIndex, validator: &T::AccountId, page: Page) -> bool {
		ErasClaimedRewards::<T>::get(era, validator).contains(&page)
	}

	/// Add reward points to validators using their stash account ID.
	pub(crate) fn reward_active_era(
		validators_points: impl IntoIterator<Item = (T::AccountId, u32)>,
	) {
		if let Some(active_era) = ActiveEra::<T>::get() {
			<ErasRewardPoints<T>>::mutate(active_era.index, |era_rewards| {
				for (validator, points) in validators_points.into_iter() {
					*era_rewards.individual.entry(validator).or_default() += points;
					era_rewards.total += points;
				}
			});
		}
	}

	pub(crate) fn get_reward_points(era: EraIndex) -> EraRewardPoints<T::AccountId> {
		ErasRewardPoints::<T>::get(era)
	}
}

/// Manages session rotation logic.
///
/// This controls the following storage items in FULL, meaning that they should not be accessed
/// directly from anywhere else in this pallet:
///
/// * [`CurrentEra`]: The current planning era
/// * [`ActiveEra`]: The current active era
/// * [`ErasStartSessionIndex`]: The starting index of the active era
/// * [`BondedEras`]: the list of eras
pub struct Rotator<T: Config>(core::marker::PhantomData<T>);

impl<T: Config> Rotator<T> {
	#[cfg(feature = "runtime-benchmarks")]
	pub(crate) fn legacy_try_plan_era() -> Vec<T::AccountId> {
		// Plan the era,
		Self::plan_new_era();
		// immediately call into the election provider to fetch and process the results. We assume
		// we are using an instant, onchain election here.
		let msp = <T::ElectionProvider as ElectionProvider>::msp();
		let lsp = 0;
		for p in (lsp..=msp).rev() {
			Pallet::<T>::do_elect_paged(p);
		}

		crate::ElectableStashes::<T>::get().into_iter().collect()
	}

	pub fn sanity_check() {
		let planned = Self::planning_era();
		let active = Self::active_era();
		assert!(
			planned == active || planned == active + 1,
			"planned era is always equal or one more than active"
		);
	}

	pub(crate) fn planning_era() -> EraIndex {
		CurrentEra::<T>::get().unwrap_or(0)
	}

	pub(crate) fn active_era() -> EraIndex {
		ActiveEra::<T>::get().map(|a| a.index).defensive_unwrap_or(0)
	}

	/// End the session and start the next one.
	pub(crate) fn end_session(end_index: SessionIndex, activation_timestamp: Option<(u64, u32)>) {
		let Some(active_era) = ActiveEra::<T>::get() else {
			defensive!("Active era must always be available.");
			return;
		};
		let current_planned_era = Self::planning_era();
		let starting = end_index + 1;
		// the session after the starting session.
		let planning = starting + 1;

		log!(
			info,
			"Session: end {:?}, start {:?} (ts: {:?}), plan {:?}",
			end_index,
			starting,
			activation_timestamp,
			planning
		);
		log!(info, "Era: active {:?}, planned {:?}", active_era.index, current_planned_era);

		match activation_timestamp {
			Some((time, id)) if id == current_planned_era => {
				// We rotate the era if we have the activation timestamp.
				Self::start_era(&active_era, starting, time);
			},
			Some((_time, id)) => {
				// RC has done something wrong -- we received the wrong ID. Don't start a new era.
				crate::log!(
					warn,
					"received wrong ID with activation timestamp. Got {}, expected {}",
					id,
					current_planned_era
				);
			},
			None => (),
		}

		// check if we should plan new era.
		let should_plan_era = match ForceEra::<T>::get() {
			// see if it's good time to plan a new era.
			Forcing::NotForcing => Self::is_plan_era_deadline(starting, active_era.index),
			// Force plan new era only once.
			Forcing::ForceNew => {
				ForceEra::<T>::put(Forcing::NotForcing);
				true
			},
			// always plan the new era.
			Forcing::ForceAlways => true,
			// never force.
			Forcing::ForceNone => false,
		};

		let has_pending_era = active_era.index < current_planned_era;
		match (should_plan_era, has_pending_era) {
			(false, _) => {
				// nothing to consider
			},
			(true, false) => {
				// happy path
				Self::plan_new_era();
			},
			(true, true) => {
				// we are waiting for to start the previously planned era, we cannot plan a new era
				// now.
				crate::log!(
					warn,
					"time to plan a new era {}, but waiting for the activation of the previous.",
					current_planned_era
				);
			},
		}

		Pallet::<T>::deposit_event(Event::SessionRotated {
			starting_session: starting,
			active_era: Self::active_era(),
			planned_era: Self::planning_era(),
		});
	}

	fn start_era(
		ending_era: &ActiveEraInfo,
		starting_session: SessionIndex,
		new_era_start_timestamp: u64,
	) {
		// verify that a new era was planned
		debug_assert!(CurrentEra::<T>::get().unwrap_or(0) == ending_era.index + 1);

		let starting_era = ending_era.index + 1;

		// finalize the ending era.
		Self::end_era(&ending_era, new_era_start_timestamp);

		// start the next era.
		Self::start_era_inc_active_era(new_era_start_timestamp);
		Self::start_era_update_bonded_eras(starting_era, starting_session);

		// add the index to starting session so later we can compute the era duration in sessions.
		ErasStartSessionIndex::<T>::insert(starting_era, starting_session);

		// discard old era information that is no longer needed.
		Self::cleanup_old_era(starting_era);
	}

	fn start_era_inc_active_era(start_timestamp: u64) {
		ActiveEra::<T>::mutate(|active_era| {
			let new_index = active_era.as_ref().map(|info| info.index + 1).unwrap_or(0);
			log!(
				debug,
				"starting active era {:?} with RC-provided timestamp {:?}",
				new_index,
				start_timestamp
			);
			*active_era = Some(ActiveEraInfo { index: new_index, start: Some(start_timestamp) });
		});
	}

	fn start_era_update_bonded_eras(starting_era: EraIndex, start_session: SessionIndex) {
		let bonding_duration = T::BondingDuration::get();

		BondedEras::<T>::mutate(|bonded| {
			bonded.push((starting_era, start_session));

			if starting_era > bonding_duration {
				let first_kept = starting_era.defensive_saturating_sub(bonding_duration);

				// Prune out everything that's from before the first-kept index.
				let n_to_prune =
					bonded.iter().take_while(|&&(era_idx, _)| era_idx < first_kept).count();

				// Kill slashing metadata.
				for (pruned_era, _) in bonded.drain(..n_to_prune) {
					slashing::clear_era_metadata::<T>(pruned_era);
				}
			}
		});
	}

	fn end_era(ending_era: &ActiveEraInfo, new_era_start: u64) {
		let previous_era_start = ending_era.start.defensive_unwrap_or(new_era_start);
		let era_duration = new_era_start.saturating_sub(previous_era_start);
		Self::end_era_compute_payout(ending_era, era_duration);
	}

	fn end_era_compute_payout(ending_era: &ActiveEraInfo, era_duration: u64) {
		let staked = ErasTotalStake::<T>::get(ending_era.index);
		let issuance = asset::total_issuance::<T>();

		log!(
			debug,
			"computing inflation for era {:?} with duration {:?}",
			ending_era.index,
			era_duration
		);
		let (validator_payout, remainder) =
			T::EraPayout::era_payout(staked, issuance, era_duration);

		let total_payout = validator_payout.saturating_add(remainder);
		let max_staked_rewards = MaxStakedRewards::<T>::get().unwrap_or(Percent::from_percent(100));

		// apply cap to validators payout and add difference to remainder.
		let validator_payout = validator_payout.min(max_staked_rewards * total_payout);
		let remainder = total_payout.saturating_sub(validator_payout);

		Pallet::<T>::deposit_event(Event::<T>::EraPaid {
			era_index: ending_era.index,
			validator_payout,
			remainder,
		});

		// Set ending era reward.
		Eras::<T>::set_validators_reward(ending_era.index, validator_payout);
		T::RewardRemainder::on_unbalanced(asset::issue::<T>(remainder));
	}

	/// Plans a new era by kicking off the election process.
	///
	/// The newly planned era is targeted to activate in the next session.
	fn plan_new_era() {
		CurrentEra::<T>::mutate(|x| {
			*x = Some(x.unwrap_or(0) + 1);
			log!(debug, "Planning new era: {:?}, sending election start signal", x.unwrap());
			let _ = T::ElectionProvider::start();
		})
	}

	/// Returns whether we are at the session where we should plan the new era.
	fn is_plan_era_deadline(start_session: SessionIndex, active_era: EraIndex) -> bool {
		let planning_era_offset = T::PlanningEraOffset::get().min(T::SessionsPerEra::get());
		// session at which we should plan the new era.
		let plan_era_session = T::SessionsPerEra::get().saturating_sub(planning_era_offset);
		let era_start_session = ErasStartSessionIndex::<T>::get(&active_era).unwrap_or(0);

		// progress of the active era in sessions.
		let session_progress =
			start_session.saturating_add(1).defensive_saturating_sub(era_start_session);

		session_progress == plan_era_session
	}

	fn cleanup_old_era(starting_era: EraIndex) {
		Pallet::<T>::clear_election_metadata();

		// discard the ancient era info.
		if let Some(old_era) = starting_era.checked_sub(T::HistoryDepth::get() + 1) {
			log!(debug, "Removing era information for {:?}", old_era);
			Eras::<T>::prune_era(old_era);
		}
	}
}
