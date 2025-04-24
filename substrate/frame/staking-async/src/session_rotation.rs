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

use crate::*;
use alloc::vec::Vec;
use frame_election_provider_support::{BoundedSupportsOf, ElectionProvider, PageIndex};
use frame_support::{
	pallet_prelude::*,
	traits::{Defensive, DefensiveMax, DefensiveSaturating, OnUnbalanced, TryCollect},
};
use sp_runtime::{Perbill, Percent, Saturating};
use sp_staking::{
	currency_to_vote::CurrencyToVote, Exposure, Page, PagedExposureMetadata, SessionIndex,
};

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
pub struct Eras<T: Config>(core::marker::PhantomData<T>);

impl<T: Config> Eras<T> {
	/// Prune all associated information with the given era.
	///
	/// Implementation note: ATM this is deleting all the information in one go, yet it can very
	/// well be done lazily.
	pub(crate) fn prune_era(era: EraIndex) {
		crate::log!(debug, "Pruning era {:?}", era);
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

#[cfg(any(feature = "try-runtime", test))]
impl<T: Config> Eras<T> {
	/// Ensure the given era is present, i.e. has not been pruned yet.
	pub(crate) fn era_present(era: EraIndex) -> Result<(), sp_runtime::TryRuntimeError> {
		// these two are only set if we have some validators in an era.
		let e0 = ErasValidatorPrefs::<T>::iter_prefix_values(era).count() != 0;
		// note: we don't check `ErasStakersPaged` as a validator can have no backers.
		let e1 = ErasStakersOverview::<T>::iter_prefix_values(era).count() != 0;
		assert_eq!(e0, e1, "ErasValidatorPrefs and ErasStakersOverview should be consistent");

		// these two must always be set
		let e2 = ErasTotalStake::<T>::contains_key(era);
		let e3 = ErasStartSessionIndex::<T>::contains_key(era);

		let active_era = Rotator::<T>::active_era();
		let e4 = if era.saturating_sub(1) > 0 &&
			era.saturating_sub(1) > active_era.saturating_sub(T::HistoryDepth::get() + 1)
		{
			// `ErasValidatorReward` is set at active era n for era n-1, and is not set for era 0 in
			// our tests. Moreover, it cannot be checked for presence in the oldest present era
			// (`active_era.saturating_sub(1)`)
			ErasValidatorReward::<T>::contains_key(era.saturating_sub(1))
		} else {
			// ignore
			e3
		};

		assert!(
			vec![e2, e3, e4].windows(2).all(|w| w[0] == w[1]),
			"era info presence not consistent for era {}: {}, {}, {}",
			era,
			e2,
			e3,
			e4,
		);

		if e2 {
			Ok(())
		} else {
			Err("era presence mismatch".into())
		}
	}

	/// Ensure the given era has indeed been already pruned.
	pub(crate) fn era_absent(era: EraIndex) -> Result<(), sp_runtime::TryRuntimeError> {
		// check double+ maps
		let e0 = ErasValidatorPrefs::<T>::iter_prefix_values(era).count() != 0;
		let e1 = ErasStakersPaged::<T>::iter_prefix_values((era,)).count() != 0;
		let e2 = ErasStakersOverview::<T>::iter_prefix_values(era).count() != 0;

		// check maps
		// `ErasValidatorReward` is set at active era n for era n-1
		let e3 = ErasValidatorReward::<T>::contains_key(era);
		let e4 = ErasTotalStake::<T>::contains_key(era);
		let e5 = ErasStartSessionIndex::<T>::contains_key(era);

		// these two are only populated conditionally, so we only check them for lack of existence
		let e6 = ErasClaimedRewards::<T>::iter_prefix_values(era).count() != 0;
		let e7 = ErasRewardPoints::<T>::contains_key(era);

		assert!(
			vec![e0, e1, e2, e3, e4, e5, e6, e7].windows(2).all(|w| w[0] == w[1]),
			"era info absence not consistent for era {}: {}, {}, {}, {}, {}, {}, {}, {}",
			era,
			e0,
			e1,
			e2,
			e3,
			e4,
			e5,
			e6,
			e7
		);

		if !e0 {
			Ok(())
		} else {
			Err("era absence mismatch".into())
		}
	}

	pub(crate) fn do_try_state() -> Result<(), sp_runtime::TryRuntimeError> {
		// pruning window works.
		let active_era = Rotator::<T>::active_era();
		// we max with 1 as in active era 0 we don't do an election and therefore we don't have some
		// of the maps populated.
		let oldest_present_era = active_era.saturating_sub(T::HistoryDepth::get()).max(1);
		let maybe_first_pruned_era =
			active_era.saturating_sub(T::HistoryDepth::get()).checked_sub(One::one());

		for e in oldest_present_era..=active_era {
			Self::era_present(e)?
		}
		if let Some(first_pruned_era) = maybe_first_pruned_era {
			Self::era_absent(first_pruned_era)?;
		}
		Ok(())
	}
}

/// Manages session rotation logic.
///
/// This controls the following storage items in FULL, meaning that they should not be accessed
/// directly from anywhere else in this pallet:
///
/// * `CurrentEra`: The current planning era
/// * `ActiveEra`: The current active era
/// * `ErasStartSessionIndex`: The starting index of the active era
/// * `BondedEras`: the list of eras
pub struct Rotator<T: Config>(core::marker::PhantomData<T>);

impl<T: Config> Rotator<T> {
	#[cfg(feature = "runtime-benchmarks")]
	pub(crate) fn legacy_insta_plan_era() -> Vec<T::AccountId> {
		// Plan the era,
		Self::plan_new_era();
		// signal that we are about to call into elect asap.
		<<T as Config>::ElectionProvider as ElectionProvider>::asap();
		// immediately call into the election provider to fetch and process the results. We assume
		// we are using an instant, onchain election here.
		let msp = <T::ElectionProvider as ElectionProvider>::msp();
		let lsp = 0;
		for p in (lsp..=msp).rev() {
			EraElectionPlanner::<T>::do_elect_paged(p);
		}

		crate::ElectableStashes::<T>::take().into_iter().collect()
	}

	#[cfg(any(feature = "try-runtime", test))]
	pub(crate) fn do_try_state() -> Result<(), sp_runtime::TryRuntimeError> {
		// planned era can always be at most one more than active era
		let planned = Self::planning_era();
		let active = Self::active_era();
		ensure!(
			planned == active || planned == active + 1,
			"planned era is always equal or one more than active"
		);
		Ok(())
	}

	pub fn planning_era() -> EraIndex {
		CurrentEra::<T>::get().unwrap_or(0)
	}

	pub fn active_era() -> EraIndex {
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
				Self::start_era(active_era, starting, time);
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

		let active_era = Self::active_era();
		// check if we should plan new era.
		let should_plan_era = match ForceEra::<T>::get() {
			// see if it's good time to plan a new era.
			Forcing::NotForcing => Self::is_plan_era_deadline(starting, active_era),
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

		let has_pending_era = active_era < current_planned_era;
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
					debug,
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

	pub(crate) fn start_era(
		ending_era: ActiveEraInfo,
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
		let _ = CurrentEra::<T>::try_mutate(|x| {
			log!(debug, "Planning new era: {:?}, sending election start signal", x.unwrap_or(0));
			let could_start_election = EraElectionPlanner::<T>::plan_new_election();
			*x = Some(x.unwrap_or(0) + 1);
			could_start_election
		});
	}

	/// Returns whether we are at the session where we should plan the new era.
	fn is_plan_era_deadline(start_session: SessionIndex, active_era: EraIndex) -> bool {
		let planning_era_offset = T::PlanningEraOffset::get().min(T::SessionsPerEra::get());
		// session at which we should plan the new era.
		let target_plan_era_session = T::SessionsPerEra::get().saturating_sub(planning_era_offset);
		let era_start_session = ErasStartSessionIndex::<T>::get(&active_era).unwrap_or(0);

		// progress of the active era in sessions.
		let session_progress =
			start_session.saturating_add(1).defensive_saturating_sub(era_start_session);

		log!(
			debug,
			"Session progress within era: {:?}, target_plan_era_session: {:?}",
			session_progress,
			target_plan_era_session
		);
		session_progress >= target_plan_era_session
	}

	fn cleanup_old_era(starting_era: EraIndex) {
		EraElectionPlanner::<T>::cleanup();

		// discard the ancient era info.
		if let Some(old_era) = starting_era.checked_sub(T::HistoryDepth::get() + 1) {
			log!(debug, "Removing era information for {:?}", old_era);
			Eras::<T>::prune_era(old_era);
		}
	}
}

/// Manager type which collects the election results from [`Config::ElectionProvider`] and
/// finalizes the planning of a new era.
///
/// This type managed 3 storage items:
///
/// * [`crate::VoterSnapshotStatus`]
/// * [`crate::NextElectionPage`]
/// * [`crate::ElectableStashes`]
///
/// A new election is fetched over multiple pages, and finalized upon fetching the last page.
///
/// * The intermediate state of fetching the election result is kept in [`NextElectionPage`]. If
///   `Some(_)` something is ongoing, otherwise not.
/// * We fully trust [`Config::ElectionProvider`] to give us a full set of validators, with enough
///   backing after all calls to `maybe_fetch_election_results` are done. Note that older versions
///   of this pallet had a `MinimumValidatorCount` to double-check this, but we don't check it
///   anymore.
/// * `maybe_fetch_election_results` returns no weight. Its weight should be taken account in the
///   e2e benchmarking of the [`Config::ElectionProvider`].
///
/// TODOs:
///
/// * Add a try-state check based on the 3 storage items
/// * Move snapshot creation functions here as well.
pub(crate) struct EraElectionPlanner<T: Config>(PhantomData<T>);
impl<T: Config> EraElectionPlanner<T> {
	/// Cleanup all associated storage items.
	pub(crate) fn cleanup() {
		VoterSnapshotStatus::<T>::kill();
		NextElectionPage::<T>::kill();
		ElectableStashes::<T>::kill();
		Pallet::<T>::register_weight(T::DbWeight::get().writes(3));
	}

	/// Fetches the number of pages configured by the election provider.
	pub(crate) fn election_pages() -> u32 {
		<<T as Config>::ElectionProvider as ElectionProvider>::Pages::get()
	}

	/// Plan a new election
	pub(crate) fn plan_new_election() -> Result<(), <T::ElectionProvider as ElectionProvider>::Error>
	{
		T::ElectionProvider::start()
			.inspect_err(|e| log!(warn, "Election provider failed to start: {:?}", e))
	}

	/// Hook to be used in the pallet's on-initialize.
	pub(crate) fn maybe_fetch_election_results() {
		if let Ok(true) = T::ElectionProvider::status() {
			crate::log!(
				debug,
				"Election provider is ready, our status is {:?}",
				NextElectionPage::<T>::get()
			);

			debug_assert!(
				CurrentEra::<T>::get().unwrap_or(0) ==
					ActiveEra::<T>::get().map_or(0, |a| a.index) + 1,
				"Next era must be already planned."
			);

			let current_page = NextElectionPage::<T>::get()
				.unwrap_or(Self::election_pages().defensive_saturating_sub(1));
			let maybe_next_page = current_page.checked_sub(1);
			crate::log!(debug, "fetching page {:?}, next {:?}", current_page, maybe_next_page);

			Self::do_elect_paged(current_page);
			NextElectionPage::<T>::set(maybe_next_page);

			// if current page was `Some`, and next is `None`, we have finished an election and
			// we can report it now.
			if maybe_next_page.is_none() {
				use pallet_staking_async_rc_client::RcClientInterface;
				let id = CurrentEra::<T>::get().defensive_unwrap_or(0);
				let prune_up_to = Self::get_prune_up_to();

				crate::log!(
					info,
					"Send new validator set to RC. ID: {:?}, prune_up_to: {:?}",
					id,
					prune_up_to
				);

				T::RcClientInterface::validator_set(
					ElectableStashes::<T>::take().into_iter().collect(),
					id,
					prune_up_to,
				);
			}
		}
	}

	/// Get the right value of the first session that needs to be pruned on the RC's historical
	/// session pallet.
	fn get_prune_up_to() -> Option<SessionIndex> {
		let bonded_eras = BondedEras::<T>::get();

		// get the first session of the oldest era in the bonded eras.
		if (bonded_eras.len() as u32) < T::BondingDuration::get() {
			None
		} else {
			Some(bonded_eras.first().map(|(_, first_session)| *first_session).unwrap_or(0))
		}
	}

	/// Paginated elect.
	///
	/// Fetches the election page with index `page` from the election provider.
	///
	/// The results from the elect call should be stored in the `ElectableStashes` storage. In
	/// addition, it stores stakers' information for next planned era based on the paged
	/// solution data returned.
	///
	/// If any new election winner does not fit in the electable stashes storage, it truncates
	/// the result of the election. We ensure that only the winners that are part of the
	/// electable stashes have exposures collected for the next era.
	pub(crate) fn do_elect_paged(page: PageIndex) {
		let election_result = T::ElectionProvider::elect(page);
		match election_result {
			Ok(supports) => {
				let inner_processing_results = Self::do_elect_paged_inner(supports);
				if let Err(not_included) = inner_processing_results {
					defensive!(
						"electable stashes exceeded limit, unexpected but election proceeds.\
                		{} stashes from election result discarded",
						not_included
					);
				};

				Pallet::<T>::deposit_event(Event::PagedElectionProceeded {
					page,
					result: inner_processing_results.map(|x| x as u32).map_err(|x| x as u32),
				});
			},
			Err(e) => {
				log!(warn, "election provider page failed due to {:?} (page: {})", e, page);
				Pallet::<T>::deposit_event(Event::PagedElectionProceeded { page, result: Err(0) });
			},
		}
	}

	/// Inner implementation of [`Self::do_elect_paged`].
	///
	/// Returns an error if adding election winners to the electable stashes storage fails due
	/// to exceeded bounds. In case of error, it returns the index of the first stash that
	/// failed to be included.
	pub(crate) fn do_elect_paged_inner(
		mut supports: BoundedSupportsOf<T::ElectionProvider>,
	) -> Result<usize, usize> {
		let planning_era = Rotator::<T>::planning_era();

		match Self::add_electables(supports.iter().map(|(s, _)| s.clone())) {
			Ok(added) => {
				let exposures = Self::collect_exposures(supports);
				let _ = Self::store_stakers_info(exposures, planning_era);
				Ok(added)
			},
			Err(not_included_idx) => {
				let not_included = supports.len().saturating_sub(not_included_idx);

				log!(
					warn,
					"not all winners fit within the electable stashes, excluding {:?} accounts from solution.",
					not_included,
				);

				// filter out supports of stashes that do not fit within the electable stashes
				// storage bounds to prevent collecting their exposures.
				supports.truncate(not_included_idx);
				let exposures = Self::collect_exposures(supports);
				let _ = Self::store_stakers_info(exposures, planning_era);

				Err(not_included)
			},
		}
	}

	/// Process the output of a paged election.
	///
	/// Store staking information for the new planned era of a single election page.
	pub(crate) fn store_stakers_info(
		exposures: BoundedExposuresOf<T>,
		new_planned_era: EraIndex,
	) -> BoundedVec<T::AccountId, MaxWinnersPerPageOf<T::ElectionProvider>> {
		// populate elected stash, stakers, exposures, and the snapshot of validator prefs.
		let mut total_stake_page: BalanceOf<T> = Zero::zero();
		let mut elected_stashes_page = Vec::with_capacity(exposures.len());
		let mut total_backers = 0u32;

		exposures.into_iter().for_each(|(stash, exposure)| {
			log!(
				trace,
				"stored exposure for stash {:?} and {:?} backers",
				stash,
				exposure.others.len()
			);
			// build elected stash.
			elected_stashes_page.push(stash.clone());
			// accumulate total stake.
			total_stake_page = total_stake_page.saturating_add(exposure.total);
			// set or update staker exposure for this era.
			total_backers += exposure.others.len() as u32;
			Eras::<T>::upsert_exposure(new_planned_era, &stash, exposure);
		});

		let elected_stashes: BoundedVec<_, MaxWinnersPerPageOf<T::ElectionProvider>> =
			elected_stashes_page
				.try_into()
				.expect("both types are bounded by MaxWinnersPerPageOf; qed");

		// adds to total stake in this era.
		Eras::<T>::add_total_stake(new_planned_era, total_stake_page);

		// collect or update the pref of all winners.
		for stash in &elected_stashes {
			let pref = Validators::<T>::get(stash);
			Eras::<T>::set_validator_prefs(new_planned_era, stash, pref);
		}

		log!(
			info,
			"stored a page of stakers with {:?} validators and {:?} total backers for era {:?}",
			elected_stashes.len(),
			total_backers,
			new_planned_era,
		);

		elected_stashes
	}

	/// Consume a set of [`BoundedSupports`] from [`sp_npos_elections`] and collect them into a
	/// [`Exposure`].
	///
	/// Returns vec of all the exposures of a validator in `paged_supports`, bounded by the
	/// number of max winners per page returned by the election provider.
	fn collect_exposures(
		supports: BoundedSupportsOf<T::ElectionProvider>,
	) -> BoundedExposuresOf<T> {
		let total_issuance = asset::total_issuance::<T>();
		let to_currency = |e: frame_election_provider_support::ExtendedBalance| {
			T::CurrencyToVote::to_currency(e, total_issuance)
		};

		supports
			.into_iter()
			.map(|(validator, support)| {
				// Build `struct exposure` from `support`.
				let mut others = Vec::with_capacity(support.voters.len());
				let mut own: BalanceOf<T> = Zero::zero();
				let mut total: BalanceOf<T> = Zero::zero();
				support
					.voters
					.into_iter()
					.map(|(nominator, weight)| (nominator, to_currency(weight)))
					.for_each(|(nominator, stake)| {
						if nominator == validator {
							defensive_assert!(own == Zero::zero(), "own stake should be unique");
							own = own.saturating_add(stake);
						} else {
							others.push(IndividualExposure { who: nominator, value: stake });
						}
						total = total.saturating_add(stake);
					});

				let exposure = Exposure { own, others, total };
				(validator, exposure)
			})
			.try_collect()
			.expect("we only map through support vector which cannot change the size; qed")
	}

	/// Adds a new set of stashes to the electable stashes.
	///
	/// Returns:
	///
	/// `Ok(newly_added)` if all stashes were added successfully.
	/// `Err(first_un_included)` if some stashes cannot be added due to bounds.
	pub(crate) fn add_electables(
		new_stashes: impl Iterator<Item = T::AccountId>,
	) -> Result<usize, usize> {
		ElectableStashes::<T>::mutate(|electable| {
			let pre_size = electable.len();

			for (idx, stash) in new_stashes.enumerate() {
				if electable.try_insert(stash).is_err() {
					return Err(idx);
				}
			}

			Ok(electable.len() - pre_size)
		})
	}
}
