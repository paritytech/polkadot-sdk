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

//! A slashing implementation for NPoS systems.
//!
//! For the purposes of the economic model, it is easiest to think of each validator as a nominator
//! which nominates only its own identity.
//!
//! The act of nomination signals intent to unify economic identity with the validator - to take
//! part in the rewards of a job well done, and to take part in the punishment of a job done badly.
//!
//! There are 3 main difficulties to account for with slashing in NPoS:
//!   - A nominator can nominate multiple validators and be slashed via any of them.
//!   - Until slashed, stake is reused from era to era. Nominating with N coins for E eras in a row
//!     does not mean you have N*E coins to be slashed - you've only ever had N.
//!   - Slashable offences can be found after the fact and out of order.
//!
//! We only slash participants for the _maximum_ slash they receive in some time period (era),
//! rather than the sum. This ensures a protection from overslashing.
//!
//! In most of the cases, thanks to validator disabling, an offender won't be able to commit more
//! than one offence. An exception is the case when the number of offenders reaches the
//! Byzantine threshold. In that case one or more offenders with the smallest offence will be
//! re-enabled and they can commit another offence. But as noted previously, even in this case we
//! slash the offender only for the biggest offence committed within an era.
//!
//! Based on research at <https://research.web3.foundation/Polkadot/security/slashing/npos>

use crate::{
	asset, log, session_rotation::Eras, BalanceOf, Config, NegativeImbalanceOf,
	NominatorSlashInEra, OffenceQueue, OffenceQueueEras, PagedExposure, Pallet, Perbill,
	ProcessingOffence, SlashRewardFraction, UnappliedSlash, UnappliedSlashes, ValidatorSlashInEra,
	WeightInfo,
};
use alloc::vec::Vec;
use codec::{Decode, Encode, MaxEncodedLen};
use frame_support::traits::{Defensive, DefensiveSaturating, Get, Imbalance, OnUnbalanced};
use scale_info::TypeInfo;
use sp_runtime::{
	traits::{Saturating, Zero},
	RuntimeDebug, WeakBoundedVec, Weight,
};
use sp_staking::{EraIndex, StakingInterface};

/// Parameters for performing a slash.
#[derive(Clone)]
pub(crate) struct SlashParams<'a, T: 'a + Config> {
	/// The stash account being slashed.
	pub(crate) stash: &'a T::AccountId,
	/// The proportion of the slash.
	pub(crate) slash: Perbill,
	/// The prior slash proportion of the validator if the validator has been reported multiple
	/// times in the same era, and a new greater slash replaces the old one.
	/// Invariant: slash > prior_slash
	pub(crate) prior_slash: Perbill,
	/// The exposure of the stash and all nominators.
	pub(crate) exposure: &'a PagedExposure<T::AccountId, BalanceOf<T>>,
	/// The era where the offence occurred.
	pub(crate) slash_era: EraIndex,
	/// The maximum percentage of a slash that ever gets paid out.
	/// This is f_inf in the paper.
	pub(crate) reward_proportion: Perbill,
}

/// Represents an offence record within the staking system, capturing details about a slashing
/// event.
#[derive(Clone, Encode, Decode, TypeInfo, MaxEncodedLen, PartialEq, RuntimeDebug)]
pub struct OffenceRecord<AccountId> {
	/// The account ID of the entity that reported the offence.
	pub reporter: Option<AccountId>,

	/// Era at which the offence was reported.
	pub reported_era: EraIndex,

	/// The specific page of the validator's exposure currently being processed.
	///
	/// Since a validator's total exposure can span multiple pages, this field serves as a pointer
	/// to the current page being evaluated. The processing order starts from the last page
	/// and moves backward, decrementing this value with each processed page.
	///
	/// This ensures that all pages are systematically handled, and it helps track when
	/// the entire exposure has been processed.
	pub exposure_page: u32,

	/// The fraction of the validator's stake to be slashed for this offence.
	pub slash_fraction: Perbill,

	/// The previous slash fraction of the validator's stake before being updated.
	/// If a new, higher slash fraction is reported, this field stores the prior fraction
	/// that was overwritten. This helps in tracking changes in slashes across multiple reports for
	/// the same era.
	pub prior_slash_fraction: Perbill,
}

/// Loads next offence in the processing offence and returns the offense record to be processed.
///
/// Note: this can mutate the following storage
/// - `ProcessingOffence`
/// - `OffenceQueue`
/// - `OffenceQueueEras`
fn next_offence<T: Config>() -> Option<(EraIndex, T::AccountId, OffenceRecord<T::AccountId>)> {
	let maybe_processing_offence = ProcessingOffence::<T>::get();

	if let Some((offence_era, offender, offence_record)) = maybe_processing_offence {
		// If the exposure page is 0, then the offence has been processed.
		if offence_record.exposure_page == 0 {
			ProcessingOffence::<T>::kill();
			return Some((offence_era, offender, offence_record))
		}

		// Update the next page.
		ProcessingOffence::<T>::put((
			offence_era,
			&offender,
			OffenceRecord {
				// decrement the page index.
				exposure_page: offence_record.exposure_page.defensive_saturating_sub(1),
				..offence_record.clone()
			},
		));

		return Some((offence_era, offender, offence_record))
	}

	// Nothing in processing offence. Try to enqueue the next offence.
	let Some(mut eras) = OffenceQueueEras::<T>::get() else { return None };
	let Some(&oldest_era) = eras.first() else { return None };

	let mut offence_iter = OffenceQueue::<T>::iter_prefix(oldest_era);
	let next_offence = offence_iter.next();

	if let Some((ref validator, ref offence_record)) = next_offence {
		// Update the processing offence if the offence is multi-page.
		if offence_record.exposure_page > 0 {
			// update processing offence with the next page.
			ProcessingOffence::<T>::put((
				oldest_era,
				validator.clone(),
				OffenceRecord {
					exposure_page: offence_record.exposure_page.defensive_saturating_sub(1),
					..offence_record.clone()
				},
			));
		}

		// Remove from `OffenceQueue`
		OffenceQueue::<T>::remove(oldest_era, &validator);
	}

	// If there are no offences left for the era, remove the era from `OffenceQueueEras`.
	if offence_iter.next().is_none() {
		if eras.len() == 1 {
			// If there is only one era left, remove the entire queue.
			OffenceQueueEras::<T>::kill();
		} else {
			// Remove the oldest era
			eras.remove(0);
			OffenceQueueEras::<T>::put(eras);
		}
	}

	next_offence.map(|(v, o)| (oldest_era, v, o))
}

/// Infallible function to process an offence.
pub(crate) fn process_offence<T: Config>() -> Weight {
	// We do manual weight racking for early-returns, and use benchmarks for the final two branches.
	let mut incomplete_consumed_weight = Weight::from_parts(0, 0);
	let mut add_db_reads_writes = |reads, writes| {
		incomplete_consumed_weight += T::DbWeight::get().reads_writes(reads, writes);
	};

	add_db_reads_writes(1, 1);
	let Some((offence_era, offender, offence_record)) = next_offence::<T>() else {
		return incomplete_consumed_weight
	};

	log!(
		debug,
		"🦹 Processing offence for {:?} in era {:?} with slash fraction {:?}",
		offender,
		offence_era,
		offence_record.slash_fraction,
	);

	add_db_reads_writes(1, 0);
	let reward_proportion = SlashRewardFraction::<T>::get();

	add_db_reads_writes(2, 0);
	let Some(exposure) =
		Eras::<T>::get_paged_exposure(offence_era, &offender, offence_record.exposure_page)
	else {
		// this can only happen if the offence was valid at the time of reporting but became too old
		// at the time of computing and should be discarded.
		return incomplete_consumed_weight
	};

	let slash_page = offence_record.exposure_page;
	let slash_defer_duration = T::SlashDeferDuration::get();
	let slash_era = offence_era.saturating_add(slash_defer_duration);

	add_db_reads_writes(3, 3);
	let Some(mut unapplied) = compute_slash::<T>(SlashParams {
		stash: &offender,
		slash: offence_record.slash_fraction,
		prior_slash: offence_record.prior_slash_fraction,
		exposure: &exposure,
		slash_era: offence_era,
		reward_proportion,
	}) else {
		log!(
			debug,
			"🦹 Slash of {:?}% happened in {:?} (reported in {:?}) is discarded, as could not compute slash",
			offence_record.slash_fraction,
			offence_era,
			offence_record.reported_era,
		);
		// No slash to apply. Discard.
		return incomplete_consumed_weight
	};

	<Pallet<T>>::deposit_event(super::Event::<T>::SlashComputed {
		offence_era,
		slash_era,
		offender: offender.clone(),
		page: slash_page,
	});

	log!(
		debug,
		"🦹 Slash of {:?}% happened in {:?} (reported in {:?}) is computed",
		offence_record.slash_fraction,
		offence_era,
		offence_record.reported_era,
	);

	// add the reporter to the unapplied slash.
	unapplied.reporter = offence_record.reporter;

	if slash_defer_duration == 0 {
		// Apply right away.
		log!(
			debug,
			"🦹 applying slash instantly of {:?} happened in {:?} (reported in {:?}) to {:?}",
			offence_record.slash_fraction,
			offence_era,
			offence_record.reported_era,
			offender,
		);

		apply_slash::<T>(unapplied, offence_era);
		T::WeightInfo::apply_slash().saturating_add(T::WeightInfo::process_offence_queue())
	} else {
		// Historical Note: Previously, with BondingDuration = 28 and SlashDeferDuration = 27,
		// slashes were applied at the start of the 28th era from `offence_era`.
		// However, with paged slashing, applying slashes now takes multiple blocks.
		// To account for this delay, slashes are now applied at the start of the 27th era from
		// `offence_era`.
		log!(
			debug,
			"🦹 deferring slash of {:?}% happened in {:?} (reported in {:?}) to {:?}",
			offence_record.slash_fraction,
			offence_era,
			offence_record.reported_era,
			slash_era,
		);
		UnappliedSlashes::<T>::insert(
			slash_era,
			(offender, offence_record.slash_fraction, slash_page),
			unapplied,
		);
		T::WeightInfo::process_offence_queue()
	}
}

/// Computes a slash of a validator and nominators. It returns an unapplied
/// record to be applied at some later point. Slashing metadata is updated in storage,
/// since unapplied records are only rarely intended to be dropped.
///
/// The pending slash record returned does not have initialized reporters. Those have
/// to be set at a higher level, if any.
///
/// If `nomintors_only` is set to `true`, only the nominator slashes will be computed.
pub(crate) fn compute_slash<T: Config>(params: SlashParams<T>) -> Option<UnappliedSlash<T>> {
	let (val_slashed, mut reward_payout) = slash_validator::<T>(params.clone());

	let mut nominators_slashed = Vec::new();
	let (nom_slashed, nom_reward_payout) =
		slash_nominators::<T>(params.clone(), &mut nominators_slashed);
	reward_payout += nom_reward_payout;

	(nom_slashed + val_slashed > Zero::zero()).then_some(UnappliedSlash {
		validator: params.stash.clone(),
		own: val_slashed,
		others: WeakBoundedVec::force_from(
			nominators_slashed,
			Some("slashed nominators not expected to be larger than the bounds"),
		),
		reporter: None,
		payout: reward_payout,
	})
}

/// Compute the slash for a validator. Returns the amount slashed and the reward payout.
fn slash_validator<T: Config>(params: SlashParams<T>) -> (BalanceOf<T>, BalanceOf<T>) {
	let own_stake = params.exposure.exposure_metadata.own;
	let prior_slashed = params.prior_slash * own_stake;
	let new_total_slash = params.slash * own_stake;

	let slash_due = new_total_slash.saturating_sub(prior_slashed);
	// Audit Note: Previously, each repeated slash reduced the reward by 50% (e.g., 50% × 50% for
	// two offences). Since repeat offences in the same era are discarded unless the new slash is
	// higher, this reduction logic was unnecessary and removed.
	let reward_due = params.reward_proportion * slash_due;
	log!(
		warn,
		"🦹 slashing validator {:?} of stake: {:?} for {:?} in era {:?}",
		params.stash,
		own_stake,
		slash_due,
		params.slash_era,
	);

	(slash_due, reward_due)
}

/// Slash nominators. Accepts general parameters and the prior slash percentage of the validator.
///
/// Returns the total amount slashed and amount of reward to pay out.
fn slash_nominators<T: Config>(
	params: SlashParams<T>,
	nominators_slashed: &mut Vec<(T::AccountId, BalanceOf<T>)>,
) -> (BalanceOf<T>, BalanceOf<T>) {
	let mut reward_payout = BalanceOf::<T>::zero();
	let mut total_slashed = BalanceOf::<T>::zero();

	nominators_slashed.reserve(params.exposure.exposure_page.others.len());
	for nominator in &params.exposure.exposure_page.others {
		let stash = &nominator.who;
		let prior_slashed =
			NominatorSlashInEra::<T>::get(&params.slash_era, stash).unwrap_or_else(Zero::zero);
		let new_slash = params.slash * nominator.value;
		let slash_due = new_slash.saturating_sub(prior_slashed);

		if new_slash == Zero::zero() {
			// nothing to do
			continue
		}

		log!(
			debug,
			"🦹 slashing nominator {:?} of stake: {:?} for {:?} in era {:?}",
			stash,
			nominator.value,
			slash_due,
			params.slash_era,
		);

		// the era slash of a nominator always grows, if the validator had a new max slash for the
		// era.
		NominatorSlashInEra::<T>::insert(
			&params.slash_era,
			stash,
			prior_slashed.saturating_add(slash_due),
		);

		nominators_slashed.push((stash.clone(), slash_due));
		total_slashed.saturating_accrue(slash_due);
		reward_payout.saturating_accrue(params.reward_proportion * slash_due);
	}

	(total_slashed, reward_payout)
}

/// Clear slashing metadata for an obsolete era.
pub(crate) fn clear_era_metadata<T: Config>(obsolete_era: EraIndex) {
	#[allow(deprecated)]
	ValidatorSlashInEra::<T>::remove_prefix(&obsolete_era, None);
	#[allow(deprecated)]
	NominatorSlashInEra::<T>::remove_prefix(&obsolete_era, None);
}

// apply the slash to a stash account, deducting any missing funds from the reward
// payout, saturating at 0. this is mildly unfair but also an edge-case that
// can only occur when overlapping locked funds have been slashed.
pub fn do_slash<T: Config>(
	stash: &T::AccountId,
	value: BalanceOf<T>,
	reward_payout: &mut BalanceOf<T>,
	slashed_imbalance: &mut NegativeImbalanceOf<T>,
	slash_era: EraIndex,
) {
	let mut ledger =
		match Pallet::<T>::ledger(sp_staking::StakingAccount::Stash(stash.clone())).defensive() {
			Ok(ledger) => ledger,
			Err(_) => return, // nothing to do.
		};

	let value = ledger.slash(value, asset::existential_deposit::<T>(), slash_era);
	if value.is_zero() {
		// nothing to do
		return
	}

	// Skip slashing for virtual stakers. The pallets managing them should handle the slashing.
	if !Pallet::<T>::is_virtual_staker(stash) {
		let (imbalance, missing) = asset::slash::<T>(stash, value);
		slashed_imbalance.subsume(imbalance);

		if !missing.is_zero() {
			// deduct overslash from the reward payout
			*reward_payout = reward_payout.saturating_sub(missing);
		}
	}

	let _ = ledger
		.update()
		.defensive_proof("ledger fetched from storage so it exists in storage; qed.");

	// trigger the event
	<Pallet<T>>::deposit_event(super::Event::<T>::Slashed { staker: stash.clone(), amount: value });
}

/// Apply a previously-unapplied slash.
pub(crate) fn apply_slash<T: Config>(unapplied_slash: UnappliedSlash<T>, slash_era: EraIndex) {
	let mut slashed_imbalance = NegativeImbalanceOf::<T>::zero();
	let mut reward_payout = unapplied_slash.payout;

	if unapplied_slash.own > Zero::zero() {
		do_slash::<T>(
			&unapplied_slash.validator,
			unapplied_slash.own,
			&mut reward_payout,
			&mut slashed_imbalance,
			slash_era,
		);
	}

	for &(ref nominator, nominator_slash) in &unapplied_slash.others {
		if nominator_slash.is_zero() {
			continue
		}

		do_slash::<T>(
			nominator,
			nominator_slash,
			&mut reward_payout,
			&mut slashed_imbalance,
			slash_era,
		);
	}

	pay_reporters::<T>(
		reward_payout,
		slashed_imbalance,
		&unapplied_slash.reporter.map(|v| crate::vec![v]).unwrap_or_default(),
	);
}

/// Apply a reward payout to some reporters, paying the rewards out of the slashed imbalance.
fn pay_reporters<T: Config>(
	reward_payout: BalanceOf<T>,
	slashed_imbalance: NegativeImbalanceOf<T>,
	reporters: &[T::AccountId],
) {
	if reward_payout.is_zero() || reporters.is_empty() {
		// nobody to pay out to or nothing to pay;
		// just treat the whole value as slashed.
		T::Slash::on_unbalanced(slashed_imbalance);
		return
	}

	// take rewards out of the slashed imbalance.
	let reward_payout = reward_payout.min(slashed_imbalance.peek());
	let (mut reward_payout, mut value_slashed) = slashed_imbalance.split(reward_payout);

	let per_reporter = reward_payout.peek() / (reporters.len() as u32).into();
	for reporter in reporters {
		let (reporter_reward, rest) = reward_payout.split(per_reporter);
		reward_payout = rest;

		// this cancels out the reporter reward imbalance internally, leading
		// to no change in total issuance.
		asset::deposit_slashed::<T>(reporter, reporter_reward);
	}

	// the rest goes to the on-slash imbalance handler (e.g. treasury)
	value_slashed.subsume(reward_payout); // remainder of reward division remains.
	T::Slash::on_unbalanced(value_slashed);
}
