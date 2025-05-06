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
//! The algorithm implemented in this module tries to balance these 3 difficulties.
//!
//! First, we only slash participants for the _maximum_ slash they receive in some time period,
//! rather than the sum. This ensures a protection from overslashing.
//!
//! Second, we do not want the time period (or "span") that the maximum is computed
//! over to last indefinitely. That would allow participants to begin acting with
//! impunity after some point, fearing no further repercussions. For that reason, we
//! automatically "chill" validators and withdraw a nominator's nomination after a slashing event,
//! requiring them to re-enlist voluntarily (acknowledging the slash) and begin a new
//! slashing span.
//!
//! Typically, you will have a single slashing event per slashing span. Only in the case
//! where a validator releases many misbehaviors at once, or goes "back in time" to misbehave in
//! eras that have already passed, would you encounter situations where a slashing span
//! has multiple misbehaviors. However, accounting for such cases is necessary
//! to deter a class of "rage-quit" attacks.
//!
//! Based on research at <https://research.web3.foundation/en/latest/polkadot/slashing/npos.html>

use crate::{
	asset, log, session_rotation::Eras, BalanceOf, Config, Error, NegativeImbalanceOf,
	NominatorSlashInEra, OffenceQueue, OffenceQueueEras, PagedExposure, Pallet, Perbill,
	ProcessingOffence, SlashRewardFraction, SpanSlash, UnappliedSlash, UnappliedSlashes,
	ValidatorSlashInEra, WeightInfo,
};
use alloc::vec::Vec;
use codec::{Decode, Encode, MaxEncodedLen};
use frame_support::{
	ensure,
	traits::{Defensive, DefensiveSaturating, Get, Imbalance, OnUnbalanced},
};
use scale_info::TypeInfo;
use sp_runtime::{
	traits::{Saturating, Zero},
	DispatchResult, RuntimeDebug, WeakBoundedVec, Weight,
};
use sp_staking::{EraIndex, StakingInterface};

/// The proportion of the slashing reward to be paid out on the first slashing detection.
/// This is f_1 in the paper.
const REWARD_F1: Perbill = Perbill::from_percent(50);

/// The index of a slashing span - unique to each stash.
pub type SpanIndex = u32;

// A range of start..end eras for a slashing span.
#[derive(Encode, Decode, TypeInfo)]
#[cfg_attr(test, derive(Debug, PartialEq))]
pub(crate) struct SlashingSpan {
	pub(crate) index: SpanIndex,
	pub(crate) start: EraIndex,
	pub(crate) length: Option<EraIndex>, // the ongoing slashing span has indeterminate length.
}

impl SlashingSpan {
	fn contains_era(&self, era: EraIndex) -> bool {
		self.start <= era && self.length.map_or(true, |l| self.start.saturating_add(l) > era)
	}
}

/// An encoding of all of a nominator's slashing spans.
#[derive(Encode, Decode, RuntimeDebug, TypeInfo)]
pub struct SlashingSpans {
	// the index of the current slashing span of the nominator. different for
	// every stash, resets when the account hits free balance 0.
	span_index: SpanIndex,
	// the start era of the most recent (ongoing) slashing span.
	last_start: EraIndex,
	// the last era at which a non-zero slash occurred.
	last_nonzero_slash: EraIndex,
	// all prior slashing spans' start indices, in reverse order (most recent first)
	// encoded as offsets relative to the slashing span after it.
	prior: Vec<EraIndex>,
}

impl SlashingSpans {
	// creates a new record of slashing spans for a stash, starting at the beginning
	// of the bonding period, relative to now.
	pub(crate) fn new(window_start: EraIndex) -> Self {
		SlashingSpans {
			span_index: 0,
			last_start: window_start,
			// initialize to zero, as this structure is lazily created until
			// the first slash is applied. setting equal to `window_start` would
			// put a time limit on nominations.
			last_nonzero_slash: 0,
			prior: Vec::new(),
		}
	}

	// update the slashing spans to reflect the start of a new span at the era after `now`
	// returns `true` if a new span was started, `false` otherwise. `false` indicates
	// that internal state is unchanged.
	pub(crate) fn end_span(&mut self, now: EraIndex) -> bool {
		let next_start = now.defensive_saturating_add(1);
		if next_start <= self.last_start {
			return false
		}

		let last_length = next_start.defensive_saturating_sub(self.last_start);
		self.prior.insert(0, last_length);
		self.last_start = next_start;
		self.span_index.defensive_saturating_accrue(1);
		true
	}

	// an iterator over all slashing spans in _reverse_ order - most recent first.
	pub(crate) fn iter(&'_ self) -> impl Iterator<Item = SlashingSpan> + '_ {
		let mut last_start = self.last_start;
		let mut index = self.span_index;
		let last = SlashingSpan { index, start: last_start, length: None };
		let prior = self.prior.iter().cloned().map(move |length| {
			let start = last_start.defensive_saturating_sub(length);
			last_start = start;
			index.defensive_saturating_reduce(1);

			SlashingSpan { index, start, length: Some(length) }
		});

		core::iter::once(last).chain(prior)
	}

	/// Yields the era index where the most recent non-zero slash occurred.
	pub fn last_nonzero_slash(&self) -> EraIndex {
		self.last_nonzero_slash
	}

	// prune the slashing spans against a window, whose start era index is given.
	//
	// If this returns `Some`, then it includes a range start..end of all the span
	// indices which were pruned.
	fn prune(&mut self, window_start: EraIndex) -> Option<(SpanIndex, SpanIndex)> {
		let old_idx = self
			.iter()
			.skip(1) // skip ongoing span.
			.position(|span| {
				span.length
					.map_or(false, |len| span.start.defensive_saturating_add(len) <= window_start)
			});

		let earliest_span_index =
			self.span_index.defensive_saturating_sub(self.prior.len() as SpanIndex);
		let pruned = match old_idx {
			Some(o) => {
				self.prior.truncate(o);
				let new_earliest =
					self.span_index.defensive_saturating_sub(self.prior.len() as SpanIndex);
				Some((earliest_span_index, new_earliest))
			},
			None => None,
		};

		// readjust the ongoing span, if it started before the beginning of the window.
		self.last_start = core::cmp::max(self.last_start, window_start);
		pruned
	}
}

/// A slashing-span record for a particular stash.
#[derive(Encode, Decode, Default, TypeInfo, MaxEncodedLen)]
pub(crate) struct SpanRecord<Balance> {
	slashed: Balance,
	paid_out: Balance,
}

impl<Balance> SpanRecord<Balance> {
	/// The value of stash balance slashed in this span.
	#[cfg(test)]
	pub(crate) fn amount(&self) -> &Balance {
		&self.slashed
	}
}

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
	/// The first era in the current bonding period.
	pub(crate) window_start: EraIndex,
	/// The current era.
	pub(crate) now: EraIndex,
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
	let window_start = offence_record.reported_era.saturating_sub(T::BondingDuration::get());

	add_db_reads_writes(3, 3);
	let Some(mut unapplied) = compute_slash::<T>(SlashParams {
		stash: &offender,
		slash: offence_record.slash_fraction,
		prior_slash: offence_record.prior_slash_fraction,
		exposure: &exposure,
		slash_era: offence_era,
		window_start,
		now: offence_record.reported_era,
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
			"🦹 applying slash instantly of {:?}% happened in {:?} (reported in {:?}) to {:?}",
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

// doesn't apply any slash, but kicks out the validator if the misbehavior is from
// the most recent slashing span.
fn kick_out_if_recent<T: Config>(params: SlashParams<T>) {
	// these are not updated by era-span or end-span.
	let mut reward_payout = Zero::zero();
	let mut val_slashed = Zero::zero();
	let mut spans = fetch_spans::<T>(
		params.stash,
		params.window_start,
		&mut reward_payout,
		&mut val_slashed,
		params.reward_proportion,
	);

	if spans.era_span(params.slash_era).map(|s| s.index) == Some(spans.span_index()) {
		// Check https://github.com/paritytech/polkadot-sdk/issues/2650 for details
		spans.end_span(params.now);
	}
}

/// Compute the slash for a validator. Returns the amount slashed and the reward payout.
fn slash_validator<T: Config>(params: SlashParams<T>) -> (BalanceOf<T>, BalanceOf<T>) {
	let own_slash = params.slash * params.exposure.exposure_metadata.own;
	log!(
		warn,
		"🦹 slashing validator {:?} of stake: {:?} with {:?}% for {:?} in era {:?}",
		params.stash,
		params.exposure.exposure_metadata.own,
		params.slash,
		own_slash,
		params.slash_era,
	);

	if own_slash == Zero::zero() {
		// kick out the validator even if they won't be slashed,
		// as long as the misbehavior is from their most recent slashing span.
		kick_out_if_recent::<T>(params);
		return (Zero::zero(), Zero::zero())
	}

	// apply slash to validator.
	let mut reward_payout = Zero::zero();
	let mut val_slashed = Zero::zero();

	{
		let mut spans = fetch_spans::<T>(
			params.stash,
			params.window_start,
			&mut reward_payout,
			&mut val_slashed,
			params.reward_proportion,
		);

		let target_span = spans.compare_and_update_span_slash(params.slash_era, own_slash);

		if target_span == Some(spans.span_index()) {
			// misbehavior occurred within the current slashing span - end current span.
			// Check <https://github.com/paritytech/polkadot-sdk/issues/2650> for details.
			spans.end_span(params.now);
		}
	}

	(val_slashed, reward_payout)
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
		let mut nom_slashed = Zero::zero();

		// the era slash of a nominator always grows, if the validator had a new max slash for the
		// era.
		let era_slash = {
			let own_slash_prior = params.prior_slash * nominator.value;
			let own_slash_by_validator = params.slash * nominator.value;
			let own_slash_difference = own_slash_by_validator.saturating_sub(own_slash_prior);

			let mut era_slash =
				NominatorSlashInEra::<T>::get(&params.slash_era, stash).unwrap_or_else(Zero::zero);
			era_slash += own_slash_difference;
			NominatorSlashInEra::<T>::insert(&params.slash_era, stash, &era_slash);

			era_slash
		};

		// compare the era slash against other eras in the same span.
		{
			let mut spans = fetch_spans::<T>(
				stash,
				params.window_start,
				&mut reward_payout,
				&mut nom_slashed,
				params.reward_proportion,
			);

			let target_span = spans.compare_and_update_span_slash(params.slash_era, era_slash);

			if target_span == Some(spans.span_index()) {
				// end the span, but don't chill the nominator.
				spans.end_span(params.now);
			}
		}
		nominators_slashed.push((stash.clone(), nom_slashed));
		total_slashed.saturating_accrue(nom_slashed);
	}

	(total_slashed, reward_payout)
}

// helper struct for managing a set of spans we are currently inspecting.
// writes alterations to disk on drop, but only if a slash has been carried out.
//
// NOTE: alterations to slashing metadata should not be done after this is dropped.
// dropping this struct applies any necessary slashes, which can lead to free balance
// being 0, and the account being garbage-collected -- a dead account should get no new
// metadata.
struct InspectingSpans<'a, T: Config + 'a> {
	dirty: bool,
	window_start: EraIndex,
	stash: &'a T::AccountId,
	spans: SlashingSpans,
	paid_out: &'a mut BalanceOf<T>,
	slash_of: &'a mut BalanceOf<T>,
	reward_proportion: Perbill,
	_marker: core::marker::PhantomData<T>,
}

// fetches the slashing spans record for a stash account, initializing it if necessary.
fn fetch_spans<'a, T: Config + 'a>(
	stash: &'a T::AccountId,
	window_start: EraIndex,
	paid_out: &'a mut BalanceOf<T>,
	slash_of: &'a mut BalanceOf<T>,
	reward_proportion: Perbill,
) -> InspectingSpans<'a, T> {
	let spans = crate::SlashingSpans::<T>::get(stash).unwrap_or_else(|| {
		let spans = SlashingSpans::new(window_start);
		crate::SlashingSpans::<T>::insert(stash, &spans);
		spans
	});

	InspectingSpans {
		dirty: false,
		window_start,
		stash,
		spans,
		slash_of,
		paid_out,
		reward_proportion,
		_marker: core::marker::PhantomData,
	}
}

impl<'a, T: 'a + Config> InspectingSpans<'a, T> {
	fn span_index(&self) -> SpanIndex {
		self.spans.span_index
	}

	fn end_span(&mut self, now: EraIndex) {
		self.dirty = self.spans.end_span(now) || self.dirty;
	}

	// add some value to the slash of the staker.
	// invariant: the staker is being slashed for non-zero value here
	// although `amount` may be zero, as it is only a difference.
	fn add_slash(&mut self, amount: BalanceOf<T>, slash_era: EraIndex) {
		*self.slash_of += amount;
		self.spans.last_nonzero_slash = core::cmp::max(self.spans.last_nonzero_slash, slash_era);
	}

	// find the span index of the given era, if covered.
	fn era_span(&self, era: EraIndex) -> Option<SlashingSpan> {
		self.spans.iter().find(|span| span.contains_era(era))
	}

	// compares the slash in an era to the overall current span slash.
	// if it's higher, applies the difference of the slashes and then updates the span on disk.
	//
	// returns the span index of the era where the slash occurred, if any.
	fn compare_and_update_span_slash(
		&mut self,
		slash_era: EraIndex,
		slash: BalanceOf<T>,
	) -> Option<SpanIndex> {
		let target_span = self.era_span(slash_era)?;
		let span_slash_key = (self.stash.clone(), target_span.index);
		let mut span_record = SpanSlash::<T>::get(&span_slash_key);
		let mut changed = false;

		let reward = if span_record.slashed < slash {
			// new maximum span slash. apply the difference.
			let difference = slash.defensive_saturating_sub(span_record.slashed);
			span_record.slashed = slash;

			// compute reward.
			let reward =
				REWARD_F1 * (self.reward_proportion * slash).saturating_sub(span_record.paid_out);

			self.add_slash(difference, slash_era);
			changed = true;

			reward
		} else if span_record.slashed == slash {
			// compute reward. no slash difference to apply.
			REWARD_F1 * (self.reward_proportion * slash).saturating_sub(span_record.paid_out)
		} else {
			Zero::zero()
		};

		if !reward.is_zero() {
			changed = true;
			span_record.paid_out += reward;
			*self.paid_out += reward;
		}

		if changed {
			self.dirty = true;
			SpanSlash::<T>::insert(&span_slash_key, &span_record);
		}

		Some(target_span.index)
	}
}

impl<'a, T: 'a + Config> Drop for InspectingSpans<'a, T> {
	fn drop(&mut self) {
		// only update on disk if we slashed this account.
		if !self.dirty {
			return
		}

		if let Some((start, end)) = self.spans.prune(self.window_start) {
			for span_index in start..end {
				SpanSlash::<T>::remove(&(self.stash.clone(), span_index));
			}
		}

		crate::SlashingSpans::<T>::insert(self.stash, &self.spans);
	}
}

/// Clear slashing metadata for an obsolete era.
pub(crate) fn clear_era_metadata<T: Config>(obsolete_era: EraIndex) {
	#[allow(deprecated)]
	ValidatorSlashInEra::<T>::remove_prefix(&obsolete_era, None);
	#[allow(deprecated)]
	NominatorSlashInEra::<T>::remove_prefix(&obsolete_era, None);
}

/// Clear slashing metadata for a dead account.
pub(crate) fn clear_stash_metadata<T: Config>(
	stash: &T::AccountId,
	num_slashing_spans: u32,
) -> DispatchResult {
	let spans = match crate::SlashingSpans::<T>::get(stash) {
		None => return Ok(()),
		Some(s) => s,
	};

	ensure!(
		num_slashing_spans as usize >= spans.iter().count(),
		Error::<T>::IncorrectSlashingSpans
	);

	crate::SlashingSpans::<T>::remove(stash);

	// kill slashing-span metadata for account.
	//
	// this can only happen while the account is staked _if_ they are completely slashed.
	// in that case, they may re-bond, but it would count again as span 0. Further ancient
	// slashes would slash into this new bond, since metadata has now been cleared.
	for span in spans.iter() {
		SpanSlash::<T>::remove(&(stash.clone(), span.index));
	}

	Ok(())
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

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn span_contains_era() {
		// unbounded end
		let span = SlashingSpan { index: 0, start: 1000, length: None };
		assert!(!span.contains_era(0));
		assert!(!span.contains_era(999));

		assert!(span.contains_era(1000));
		assert!(span.contains_era(1001));
		assert!(span.contains_era(10000));

		// bounded end - non-inclusive range.
		let span = SlashingSpan { index: 0, start: 1000, length: Some(10) };
		assert!(!span.contains_era(0));
		assert!(!span.contains_era(999));

		assert!(span.contains_era(1000));
		assert!(span.contains_era(1001));
		assert!(span.contains_era(1009));
		assert!(!span.contains_era(1010));
		assert!(!span.contains_era(1011));
	}

	#[test]
	fn single_slashing_span() {
		let spans = SlashingSpans {
			span_index: 0,
			last_start: 1000,
			last_nonzero_slash: 0,
			prior: Vec::new(),
		};

		assert_eq!(
			spans.iter().collect::<Vec<_>>(),
			vec![SlashingSpan { index: 0, start: 1000, length: None }],
		);
	}

	#[test]
	fn many_prior_spans() {
		let spans = SlashingSpans {
			span_index: 10,
			last_start: 1000,
			last_nonzero_slash: 0,
			prior: vec![10, 9, 8, 10],
		};

		assert_eq!(
			spans.iter().collect::<Vec<_>>(),
			vec![
				SlashingSpan { index: 10, start: 1000, length: None },
				SlashingSpan { index: 9, start: 990, length: Some(10) },
				SlashingSpan { index: 8, start: 981, length: Some(9) },
				SlashingSpan { index: 7, start: 973, length: Some(8) },
				SlashingSpan { index: 6, start: 963, length: Some(10) },
			],
		)
	}

	#[test]
	fn pruning_spans() {
		let mut spans = SlashingSpans {
			span_index: 10,
			last_start: 1000,
			last_nonzero_slash: 0,
			prior: vec![10, 9, 8, 10],
		};

		assert_eq!(spans.prune(981), Some((6, 8)));
		assert_eq!(
			spans.iter().collect::<Vec<_>>(),
			vec![
				SlashingSpan { index: 10, start: 1000, length: None },
				SlashingSpan { index: 9, start: 990, length: Some(10) },
				SlashingSpan { index: 8, start: 981, length: Some(9) },
			],
		);

		assert_eq!(spans.prune(982), None);
		assert_eq!(
			spans.iter().collect::<Vec<_>>(),
			vec![
				SlashingSpan { index: 10, start: 1000, length: None },
				SlashingSpan { index: 9, start: 990, length: Some(10) },
				SlashingSpan { index: 8, start: 981, length: Some(9) },
			],
		);

		assert_eq!(spans.prune(989), None);
		assert_eq!(
			spans.iter().collect::<Vec<_>>(),
			vec![
				SlashingSpan { index: 10, start: 1000, length: None },
				SlashingSpan { index: 9, start: 990, length: Some(10) },
				SlashingSpan { index: 8, start: 981, length: Some(9) },
			],
		);

		assert_eq!(spans.prune(1000), Some((8, 10)));
		assert_eq!(
			spans.iter().collect::<Vec<_>>(),
			vec![SlashingSpan { index: 10, start: 1000, length: None },],
		);

		assert_eq!(spans.prune(2000), None);
		assert_eq!(
			spans.iter().collect::<Vec<_>>(),
			vec![SlashingSpan { index: 10, start: 2000, length: None },],
		);

		// now all in one shot.
		let mut spans = SlashingSpans {
			span_index: 10,
			last_start: 1000,
			last_nonzero_slash: 0,
			prior: vec![10, 9, 8, 10],
		};
		assert_eq!(spans.prune(2000), Some((6, 10)));
		assert_eq!(
			spans.iter().collect::<Vec<_>>(),
			vec![SlashingSpan { index: 10, start: 2000, length: None },],
		);
	}

	#[test]
	fn ending_span() {
		let mut spans = SlashingSpans {
			span_index: 1,
			last_start: 10,
			last_nonzero_slash: 0,
			prior: Vec::new(),
		};

		assert!(spans.end_span(10));

		assert_eq!(
			spans.iter().collect::<Vec<_>>(),
			vec![
				SlashingSpan { index: 2, start: 11, length: None },
				SlashingSpan { index: 1, start: 10, length: Some(1) },
			],
		);

		assert!(spans.end_span(15));
		assert_eq!(
			spans.iter().collect::<Vec<_>>(),
			vec![
				SlashingSpan { index: 3, start: 16, length: None },
				SlashingSpan { index: 2, start: 11, length: Some(5) },
				SlashingSpan { index: 1, start: 10, length: Some(1) },
			],
		);

		// does nothing if not a valid end.
		assert!(!spans.end_span(15));
		assert_eq!(
			spans.iter().collect::<Vec<_>>(),
			vec![
				SlashingSpan { index: 3, start: 16, length: None },
				SlashingSpan { index: 2, start: 11, length: Some(5) },
				SlashingSpan { index: 1, start: 10, length: Some(1) },
			],
		);
	}
}
