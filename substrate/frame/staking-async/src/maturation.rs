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

//! Pure maturation math for held, vesting validator incentive.
//!
//! Incentive is placed on hold and matures linearly over a fixed window, sliced into "bonding
//! periods" (`period_index = era / bonding_duration`). Incentive delivered in a period accumulates
//! into that period's [`IncentiveBucket`], which matures over `vesting_periods × bonding_duration`
//! eras from its period start and is pruned once fully released. The maturation logic is
//! parametrized so that it can be used for both idle and bonded incentive buckets.

use codec::{Decode, DecodeWithMemTracking, Encode, HasCompact, MaxEncodedLen};
use scale_info::TypeInfo;
use sp_runtime::{
	traits::{AtLeast32BitUnsigned, Zero},
	Perbill,
};
use sp_staking::EraIndex;

/// Index of a bonding period, i.e. `era / bonding_duration`.
pub type PeriodIndex = EraIndex;

/// The fraction of a period's incentive matured by `current_era`.
///
/// The window spans `vesting_periods × bonding_duration` eras from `period_index ×
/// bonding_duration` (the period's first era): `0` at or before the start, saturating to `1` once
/// fully elapsed. A zero-length window is treated as fully matured (no lock).
pub fn matured_fraction(
	period_index: PeriodIndex,
	current_era: EraIndex,
	vesting_periods: u32,
	bonding_duration: EraIndex,
) -> Perbill {
	let window = vesting_periods.saturating_mul(bonding_duration);
	if window.is_zero() {
		return Perbill::one();
	}

	let period_start_era = period_index.saturating_mul(bonding_duration);
	let elapsed = current_era.saturating_sub(period_start_era);

	Perbill::from_rational(elapsed.min(window), window)
}

/// Incentive delivered into a single bonding period, and how much has been released.
///
/// Keyed externally by [`PeriodIndex`]; tracks only the two running totals needed
/// to compute what is releasable at a given era.
#[derive(
	PartialEq, Eq, Clone, Encode, Decode, DecodeWithMemTracking, Debug, TypeInfo, MaxEncodedLen,
)]
pub struct IncentiveBucket<Balance: HasCompact + MaxEncodedLen> {
	/// Total incentive delivered into this period (grows via [`Self::merge`]).
	#[codec(compact)]
	pub total: Balance,
	/// Amount already released (idle) or already vested when bonded (bonded).
	#[codec(compact)]
	pub released: Balance,
}

impl<Balance> IncentiveBucket<Balance>
where
	Balance: AtLeast32BitUnsigned + MaxEncodedLen + Copy,
{
	/// New bucket seeded with an initial amount.
	pub fn new(amount: Balance) -> Self {
		Self { total: amount, released: Balance::zero() }
	}

	/// Add a same-period delivery to this bucket.
	pub fn merge(&mut self, amount: Balance) {
		self.total = self.total.saturating_add(amount);
	}

	/// Add another bucket's `total` and `released` (e.g. a proportional slice drawn from an idle
	/// bucket).
	pub fn absorb(&mut self, total: Balance, released: Balance) {
		self.total = self.total.saturating_add(total);
		self.released = self.released.saturating_add(released);
	}

	/// Amount releasable now: cumulative matured (see [`matured_fraction`]) minus already released.
	pub fn releasable_at(
		&self,
		period_index: PeriodIndex,
		current_era: EraIndex,
		vesting_periods: u32,
		bonding_duration: EraIndex,
	) -> Balance {
		let cumulative_matured =
			matured_fraction(period_index, current_era, vesting_periods, bonding_duration)
				.mul_floor(self.total);
		cumulative_matured.saturating_sub(self.released)
	}

	/// Whether the bucket is fully released and can be pruned.
	pub fn is_exhausted(&self) -> bool {
		self.released >= self.total
	}

	/// Still-restricted amount at `current_era`: `total` minus the larger of `released` (already
	/// vested at bond time) and the cumulative matured amount. Used by bonded buckets.
	pub fn restricted_at(
		&self,
		period_index: PeriodIndex,
		current_era: EraIndex,
		vesting_periods: u32,
		bonding_duration: EraIndex,
	) -> Balance {
		let matured =
			matured_fraction(period_index, current_era, vesting_periods, bonding_duration)
				.mul_floor(self.total);
		self.total.saturating_sub(matured.max(self.released))
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use sp_runtime::Saturating;

	type Balance = u128;

	// A small, easy-to-reason-about window: 4 periods of 5 eras each = 20 eras total.
	const VESTING_PERIODS: u32 = 4;
	const BONDING_DURATION: EraIndex = 5;
	const WINDOW: EraIndex = VESTING_PERIODS * BONDING_DURATION;

	#[test]
	fn matured_fraction_zero_at_period_boundary() {
		// Period 0 starts at era 0; period 2 starts at era 10.
		assert_eq!(matured_fraction(0, 0, VESTING_PERIODS, BONDING_DURATION), Perbill::zero());
		assert_eq!(
			matured_fraction(2, 2 * BONDING_DURATION, VESTING_PERIODS, BONDING_DURATION),
			Perbill::zero()
		);
	}

	#[test]
	fn matured_fraction_before_period_start_saturates_to_zero() {
		// current_era before the period even starts: elapsed saturates to 0 via saturating_sub.
		assert_eq!(matured_fraction(3, 1, VESTING_PERIODS, BONDING_DURATION), Perbill::zero());
	}

	#[test]
	fn matured_fraction_full_at_window_end() {
		assert_eq!(matured_fraction(0, WINDOW, VESTING_PERIODS, BONDING_DURATION), Perbill::one());
	}

	#[test]
	fn matured_fraction_saturates_after_window_end() {
		assert_eq!(
			matured_fraction(0, WINDOW + 100, VESTING_PERIODS, BONDING_DURATION),
			Perbill::one()
		);
	}

	#[test]
	fn matured_fraction_linear_per_era_steps() {
		// With a 20-era window, each elapsed era should add exactly 1/20 = 5%.
		for elapsed in 0..=WINDOW {
			let expected = Perbill::from_rational(elapsed, WINDOW);
			assert_eq!(matured_fraction(0, elapsed, VESTING_PERIODS, BONDING_DURATION), expected);
		}
	}

	#[test]
	fn matured_fraction_respects_nonzero_period_start() {
		// Period 1 starts at era 5 (bonding_duration = 5); halfway through its own window is era
		// 5 + 10 = 15.
		let period_index = 1;
		let period_start = period_index * BONDING_DURATION;
		assert_eq!(
			matured_fraction(period_index, period_start, VESTING_PERIODS, BONDING_DURATION),
			Perbill::zero()
		);
		assert_eq!(
			matured_fraction(
				period_index,
				period_start + WINDOW / 2,
				VESTING_PERIODS,
				BONDING_DURATION
			),
			Perbill::from_rational(1u32, 2u32)
		);
		assert_eq!(
			matured_fraction(
				period_index,
				period_start + WINDOW,
				VESTING_PERIODS,
				BONDING_DURATION
			),
			Perbill::one()
		);
	}

	#[test]
	fn matured_fraction_zero_window_is_fully_matured() {
		// vesting_periods == 0.
		assert_eq!(matured_fraction(0, 1_000, 0, BONDING_DURATION), Perbill::one());
		// bonding_duration == 0.
		assert_eq!(matured_fraction(0, 1_000, VESTING_PERIODS, 0), Perbill::one());
		// both zero.
		assert_eq!(matured_fraction(0, 1_000, 0, 0), Perbill::one());
	}

	#[test]
	fn bucket_new_starts_with_nothing_released() {
		let bucket = IncentiveBucket::<Balance>::new(1_000);
		assert_eq!(bucket.total, 1_000);
		assert_eq!(bucket.released, 0);
		assert!(!bucket.is_exhausted());
	}

	#[test]
	fn bucket_merge_accumulates_same_period() {
		let mut bucket = IncentiveBucket::<Balance>::new(1_000);
		bucket.merge(500);
		assert_eq!(bucket.total, 1_500);
		bucket.merge(0);
		assert_eq!(bucket.total, 1_500);
	}

	#[test]
	fn bucket_releasable_linear_across_window() {
		let bucket = IncentiveBucket::<Balance>::new(WINDOW as Balance * 10);
		// At the period boundary, nothing is releasable.
		assert_eq!(bucket.releasable_at(0, 0, VESTING_PERIODS, BONDING_DURATION), 0);
		// Halfway through the window, half should be releasable.
		assert_eq!(
			bucket.releasable_at(0, WINDOW / 2, VESTING_PERIODS, BONDING_DURATION),
			(WINDOW as Balance * 10) / 2
		);
		// At and after full maturation, the entire amount is releasable.
		assert_eq!(
			bucket.releasable_at(0, WINDOW, VESTING_PERIODS, BONDING_DURATION),
			WINDOW as Balance * 10
		);
		assert_eq!(
			bucket.releasable_at(0, WINDOW + 50, VESTING_PERIODS, BONDING_DURATION),
			WINDOW as Balance * 10
		);
	}

	#[test]
	fn bucket_releasable_never_exceeds_total() {
		let total = 1_000_000u128;
		let bucket = IncentiveBucket::<Balance>::new(total);
		for era in 0..=(WINDOW + 25) {
			let releasable = bucket.releasable_at(0, era, VESTING_PERIODS, BONDING_DURATION);
			assert!(releasable <= total);
		}
	}

	#[test]
	fn bucket_releasable_accounts_for_already_released() {
		let mut bucket = IncentiveBucket::<Balance>::new(1_000);
		// Simulate having released the first-quarter's worth already.
		bucket.released = 250;
		// At the quarter mark, nothing further should be releasable yet.
		assert_eq!(bucket.releasable_at(0, WINDOW / 4, VESTING_PERIODS, BONDING_DURATION), 0);
		// Halfway through, only the remaining unreleased matured amount should show up.
		assert_eq!(bucket.releasable_at(0, WINDOW / 2, VESTING_PERIODS, BONDING_DURATION), 250);
	}

	#[test]
	fn bucket_is_exhausted_only_when_fully_released() {
		let mut bucket = IncentiveBucket::<Balance>::new(1_000);
		assert!(!bucket.is_exhausted());
		bucket.released = 999;
		assert!(!bucket.is_exhausted());
		bucket.released = 1_000;
		assert!(bucket.is_exhausted());
	}

	#[test]
	fn bucket_prune_triggers_exactly_at_full_release_over_time() {
		let mut bucket = IncentiveBucket::<Balance>::new(2_000);
		for era in 0..=WINDOW {
			let releasable = bucket.releasable_at(0, era, VESTING_PERIODS, BONDING_DURATION);
			bucket.released.saturating_accrue(releasable);
			if era < WINDOW {
				assert!(!bucket.is_exhausted(), "must not be exhausted before window end");
			}
		}
		assert!(bucket.is_exhausted(), "must be exhausted once the full window has elapsed");
	}

	#[test]
	fn bucket_zero_window_releases_everything() {
		let bucket = IncentiveBucket::<Balance>::new(1_000);
		assert_eq!(bucket.releasable_at(0, 0, 0, BONDING_DURATION), 1_000);
		assert_eq!(bucket.restricted_at(0, 0, 0, BONDING_DURATION), 0);
	}

	#[test]
	fn bucket_restricted_respects_prevested_released() {
		// Bonded slice with 50 already vested at bond time: restriction starts at 50, not 100.
		let bucket = IncentiveBucket::<Balance> { total: 100, released: 50 };
		assert_eq!(bucket.restricted_at(0, 0, VESTING_PERIODS, BONDING_DURATION), 50);
		// Decays linearly on the total: at 3/4 of the window 25 remains restricted.
		assert_eq!(bucket.restricted_at(0, 3 * WINDOW / 4, VESTING_PERIODS, BONDING_DURATION), 25);
		assert_eq!(bucket.restricted_at(0, WINDOW, VESTING_PERIODS, BONDING_DURATION), 0);
	}

	#[test]
	fn bucket_restricted_linear_across_window() {
		let bucket = IncentiveBucket::<Balance>::new(WINDOW as Balance * 10);
		assert_eq!(
			bucket.restricted_at(0, 0, VESTING_PERIODS, BONDING_DURATION),
			WINDOW as Balance * 10
		);
		assert_eq!(
			bucket.restricted_at(0, WINDOW / 2, VESTING_PERIODS, BONDING_DURATION),
			(WINDOW as Balance * 10) / 2
		);
		assert_eq!(bucket.restricted_at(0, WINDOW, VESTING_PERIODS, BONDING_DURATION), 0);
	}

	#[test]
	fn bucket_restricted_zero_once_slashed_to_zero() {
		let bucket = IncentiveBucket::<Balance>::new(0);
		assert_eq!(bucket.restricted_at(0, 0, VESTING_PERIODS, BONDING_DURATION), 0);
	}
}
