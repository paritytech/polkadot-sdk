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

//! Tests for the nomination-staleness mechanism.
//!
//! Covers:
//! - The pure decay curve (`LinearStalenessCurve`) at boundaries.
//! - Which staking actions reset `submitted_in` (only `nominate`; not `bond_extra`, `chill`,
//!   `payout_stakers`, etc.).
//! - The election snapshot applying the multiplier to voter weight, and zero-weight voters being
//!   excluded.
//! - The migration helper that resets `submitted_in` on upgrade.

use super::*;
use crate::{
	migrations::nomination_staleness::reset_all_nomination_submitted_in,
	mock::{Session, StalenessDecayPeriod, StalenessFloor, StalenessGracePeriod},
	LinearStalenessCurve, NoNominationStaleness,
};
use frame_election_provider_support::ElectionDataProvider;
use sp_staking::NominationStalenessCurve as _;

fn set_curve(grace: EraIndex, decay: EraIndex, floor: Perbill) {
	StalenessGracePeriod::set(grace);
	StalenessDecayPeriod::set(decay);
	StalenessFloor::set(floor);
}

type Curve = LinearStalenessCurve<StalenessGracePeriod, StalenessDecayPeriod, StalenessFloor>;

// ----- Pure curve tests (no ExtBuilder needed). -----

#[test]
fn no_staleness_curve_is_always_one() {
	assert_eq!(NoNominationStaleness::multiplier(0), Perbill::one());
	assert_eq!(NoNominationStaleness::multiplier(1_000), Perbill::one());
	assert_eq!(NoNominationStaleness::multiplier(u32::MAX), Perbill::one());
}

#[test]
fn linear_curve_full_weight_in_grace_period() {
	set_curve(28, 140, Perbill::zero());
	for s in 0..=28 {
		assert_eq!(Curve::multiplier(s), Perbill::one(), "multiplier should be 1 at s = {}", s);
	}
}

#[test]
fn linear_curve_decays_linearly_to_floor_zero() {
	set_curve(28, 140, Perbill::zero());

	let m_just_past = Curve::multiplier(29);
	assert!(m_just_past < Perbill::one() && m_just_past > Perbill::from_percent(99));

	let m_halfway = Curve::multiplier(28 + 70);
	assert_eq!(m_halfway, Perbill::from_rational(70u32, 140u32));

	let m_almost_done = Curve::multiplier(28 + 139);
	assert!(m_almost_done > Perbill::zero() && m_almost_done < Perbill::from_percent(1));

	assert_eq!(Curve::multiplier(28 + 140), Perbill::zero());
	assert_eq!(Curve::multiplier(1_000), Perbill::zero());
}

#[test]
fn linear_curve_decays_linearly_to_nonzero_floor() {
	let floor = Perbill::from_percent(25);
	set_curve(10, 100, floor);

	assert_eq!(Curve::multiplier(10), Perbill::one());

	let m_halfway = Curve::multiplier(10 + 50);
	// active_share = 50%, one_minus_floor = 75%, product = 37.5%, +floor (25%) = 62.5%
	assert_eq!(m_halfway, floor + Perbill::from_rational(50u32, 100u32) * (Perbill::one() - floor));

	assert_eq!(Curve::multiplier(10 + 100), floor);
	assert_eq!(Curve::multiplier(10 + 1_000), floor);
}

#[test]
fn linear_curve_disabled_by_max_grace_period() {
	set_curve(u32::MAX, 0, Perbill::zero());
	assert_eq!(Curve::multiplier(0), Perbill::one());
	assert_eq!(Curve::multiplier(1_000), Perbill::one());
	assert_eq!(Curve::multiplier(u32::MAX - 1), Perbill::one());
}

#[test]
fn linear_curve_zero_decay_period_clamps_immediately() {
	set_curve(5, 0, Perbill::from_percent(40));
	assert_eq!(Curve::multiplier(5), Perbill::one());
	assert_eq!(Curve::multiplier(6), Perbill::from_percent(40));
	assert_eq!(Curve::multiplier(1_000), Perbill::from_percent(40));
}

// ----- Refresh-trigger tests. -----

#[test]
fn nominate_resets_submitted_in_even_with_same_targets() {
	ExtBuilder::default().has_stakers(true).nominate(true).build_and_execute(|| {
		Session::roll_until_active_era(5);
		let before = Nominators::<Test>::get(101).unwrap();
		assert_eq!(before.targets.to_vec(), vec![11, 21]);
		let stale_era = before.submitted_in;
		assert!(stale_era < 5);

		assert_ok!(Staking::nominate(RuntimeOrigin::signed(101), vec![11, 21]));

		let after = Nominators::<Test>::get(101).unwrap();
		assert_eq!(after.targets.to_vec(), vec![11, 21]);
		assert_eq!(after.submitted_in, current_era());
	});
}

#[test]
fn bond_extra_does_not_reset_submitted_in() {
	ExtBuilder::default().has_stakers(true).nominate(true).build_and_execute(|| {
		Session::roll_until_active_era(7);
		let before = Nominators::<Test>::get(101).unwrap();
		let original_submitted_in = before.submitted_in;

		assert_ok!(Staking::bond_extra(RuntimeOrigin::signed(101), 100));

		let after = Nominators::<Test>::get(101).unwrap();
		assert_eq!(after.submitted_in, original_submitted_in);
	});
}

#[test]
fn chill_removes_nomination_and_renominate_creates_fresh_entry() {
	ExtBuilder::default().has_stakers(true).nominate(true).build_and_execute(|| {
		Session::roll_until_active_era(3);
		assert!(Nominators::<Test>::get(101).is_some());

		assert_ok!(Staking::chill(RuntimeOrigin::signed(101)));
		assert!(Nominators::<Test>::get(101).is_none());

		Session::roll_until_active_era(9);
		assert_ok!(Staking::nominate(RuntimeOrigin::signed(101), vec![11, 21]));

		let after = Nominators::<Test>::get(101).unwrap();
		assert_eq!(after.submitted_in, current_era());
	});
}

// ----- Snapshot-behavior tests. -----

#[test]
fn snapshot_voter_weight_is_unchanged_with_default_no_op_curve() {
	ExtBuilder::default().has_stakers(true).nominate(true).build_and_execute(|| {
		Session::roll_until_active_era(20);

		let voters =
			<Staking as ElectionDataProvider>::electing_voters(DataProviderBounds::default(), 0)
				.unwrap();

		let entry = voters.iter().find(|(who, _, _)| who == &101).expect("101 in snapshot");
		// Active stake of nominator 101 in the genesis mock setup.
		assert_eq!(entry.1, 500);
	});
}

#[test]
fn snapshot_voter_weight_is_reduced_when_stale() {
	ExtBuilder::default().has_stakers(true).nominate(true).build_and_execute(|| {
		// Force `submitted_in` to era 0 so we can drive staleness deterministically.
		Nominators::<Test>::mutate(101, |n| {
			n.as_mut().unwrap().submitted_in = 0;
		});
		set_curve(2, 4, Perbill::zero());

		Session::roll_until_active_era(4);
		// 4 eras since `submitted_in = 0`. Past grace (2), 2 eras into decay.
		// multiplier = (4 - 2) / 4 = 50%.
		let voters =
			<Staking as ElectionDataProvider>::electing_voters(DataProviderBounds::default(), 0)
				.unwrap();
		let entry = voters.iter().find(|(who, _, _)| who == &101).expect("101 in snapshot");
		assert_eq!(entry.1, 250);
	});
}

#[test]
fn fully_stale_voter_with_zero_floor_is_excluded_from_snapshot() {
	ExtBuilder::default().has_stakers(true).nominate(true).build_and_execute(|| {
		Nominators::<Test>::mutate(101, |n| {
			n.as_mut().unwrap().submitted_in = 0;
		});
		set_curve(2, 4, Perbill::zero());

		Session::roll_until_active_era(6);
		// 6 eras since `submitted_in = 0` >= grace + decay. multiplier = 0.
		let voters =
			<Staking as ElectionDataProvider>::electing_voters(DataProviderBounds::default(), 0)
				.unwrap();
		assert!(voters.iter().all(|(who, _, _)| who != &101));
	});
}

#[test]
fn re_nominating_restores_full_weight() {
	ExtBuilder::default().has_stakers(true).nominate(true).build_and_execute(|| {
		Nominators::<Test>::mutate(101, |n| {
			n.as_mut().unwrap().submitted_in = 0;
		});
		set_curve(2, 4, Perbill::zero());

		Session::roll_until_active_era(4);
		// Re-affirm in era 4.
		assert_ok!(Staking::nominate(RuntimeOrigin::signed(101), vec![11, 21]));

		let voters =
			<Staking as ElectionDataProvider>::electing_voters(DataProviderBounds::default(), 0)
				.unwrap();
		let entry = voters.iter().find(|(who, _, _)| who == &101).expect("101 in snapshot");
		assert_eq!(entry.1, 500);
	});
}

// ----- Migration helper test. -----

#[test]
fn reset_all_nomination_submitted_in_resets_every_nominator() {
	ExtBuilder::default().has_stakers(true).nominate(true).build_and_execute(|| {
		Nominators::<Test>::mutate(101, |n| {
			n.as_mut().unwrap().submitted_in = 0;
		});

		Session::roll_until_active_era(42);
		let era = current_era();
		assert_eq!(Nominators::<Test>::get(101).unwrap().submitted_in, 0);

		let _w = reset_all_nomination_submitted_in::<Test>();

		for (_, nomination) in Nominators::<Test>::iter() {
			assert_eq!(nomination.submitted_in, era);
		}
	});
}

// ----- Downstream payout / slashing behaviour. -----

#[test]
fn payout_diminished() {
	// A stale nominator's slice of validator rewards is reduced in proportion to their reduced
	// exposure. The mechanism is applied at the election snapshot only; the `payout_stakers`
	// math is unchanged and naturally distributes the "lost" share to the validator's
	// non-stale stakers via standard exposure-weighted payout (RFC #104).
	ExtBuilder::default().has_stakers(true).nominate(true).build_and_execute(|| {
		// Pin 101 to a deterministic staleness. With `submitted_in = 0` and curve
		// `(grace = 0, decay = 0, floor = 50%)`, the multiplier is `1` while in grace
		// (`s == 0`) and clamps to the floor (`50%`) for any later era.
		Nominators::<Test>::mutate(101, |n| {
			n.as_mut().unwrap().submitted_in = 0;
		});
		set_curve(0, 0, Perbill::from_percent(50));

		// Pay rewards into free balance so we can read deltas via `total_balance`.
		Payee::<Test>::insert(11, RewardDestination::Account(11));
		Payee::<Test>::insert(101, RewardDestination::Account(101));

		// Advance past the initial (curve-less) election so the multiplier takes effect on a
		// fresh snapshot.
		Session::roll_until_active_era(2);

		// 101's distributed exposure across all validators sums to their reduced voter weight:
		// `bonded (500) * 50% = 250` (vs. the non-stale baseline of 500, see
		// `nominators_no_slashing::nominators_are_not_slashed`).
		let exp_11 = Staking::eras_stakers(active_era(), &11);
		let exp_21 = Staking::eras_stakers(active_era(), &21);
		let total_exposure_101: Balance = exp_11
			.others
			.iter()
			.chain(exp_21.others.iter())
			.filter(|i| i.who == 101)
			.map(|i| i.value)
			.sum();
		assert_eq!(total_exposure_101, 250);

		let exposed_101_in_11 = exp_11
			.others
			.iter()
			.find(|i| i.who == 101)
			.expect("101 still in 11's exposure under 50% multiplier")
			.value;
		assert!(exposed_101_in_11 > 0 && exposed_101_in_11 < 250);

		// Reward validator 11 and pay out era 2.
		Pallet::<Test>::reward_by_ids(vec![(11, 1)]);
		let payout = validator_payout_for(time_per_era());
		Session::roll_until_active_era(3);

		let init_11 = asset::total_balance::<Test>(&11);
		let init_101 = asset::total_balance::<Test>(&101);
		mock::make_all_reward_payment(2);
		let recv_11 = asset::total_balance::<Test>(&11) - init_11;
		let recv_101 = asset::total_balance::<Test>(&101) - init_101;

		// 101's share equals their (reduced) exposure value over 11's total exposure —
		// strictly less than without staleness. The validator's slice (`own / total`) is
		// correspondingly larger.
		let expected_101 = Perbill::from_rational(exposed_101_in_11, exp_11.total) * payout;
		let expected_11 = Perbill::from_rational(exp_11.own, exp_11.total) * payout;
		assert_eq_error_rate!(recv_101, expected_101, 2);
		assert_eq_error_rate!(recv_11, expected_11, 2);
	});
}

#[test]
fn slashing_unaffected() {
	// The slashing path is unchanged by the staleness mechanism: it operates on
	// `exposure.own` and `IndividualExposure.value` directly, with no separate staleness
	// adjustment. Concretely:
	//   - The validator's own slash equals `slash_pct * exposure.own`, which is the
	//     validator's self-stake and is independent of nominator staleness.
	//   - A still-exposed stale nominator is slashed proportional to their (reduced)
	//     exposure value — they are NOT made immune to slashing.
	ExtBuilder::default().has_stakers(true).nominate(true).build_and_execute(|| {
		Nominators::<Test>::mutate(101, |n| {
			n.as_mut().unwrap().submitted_in = 0;
		});
		// Permanent 50% floor: 101 always has multiplier == 50% in any election after we
		// set the curve.
		set_curve(0, 0, Perbill::from_percent(50));

		// Advance past the initial (curve-less) election so the multiplier takes effect.
		Session::roll_until_active_era(2);

		// 11's `own` is unaffected by staleness; 101 is still in the exposure with a
		// strictly reduced (non-zero) value (vs. the 250 baseline at full weight).
		let exp = Staking::eras_stakers(active_era(), &11);
		assert_eq!(exp.own, 1000);
		let exposed_101 = exp
			.others
			.iter()
			.find(|i| i.who == 101)
			.expect("101 still exposed under 50% multiplier")
			.value;
		assert!(exposed_101 > 0 && exposed_101 < 250);

		let pre_11 = asset::stakeable_balance::<Test>(&11);
		let pre_101 = asset::stakeable_balance::<Test>(&101);

		// Slash 11 by 50%. With the default `SlashDeferDuration = 0`, the slash is computed
		// and applied on the next block.
		add_slash_with_percent(11, 50);
		Session::roll_next();

		// Validator slash = `slash_pct * own = 500`, identical to the non-stale case.
		assert_eq!(asset::stakeable_balance::<Test>(&11), pre_11 - 500);
		// Nominator slash = `slash_pct * exposed_value`. The stale nominator is hit on
		// their (reduced) exposure, not made immune.
		assert_eq!(
			asset::stakeable_balance::<Test>(&101),
			pre_101 - (Perbill::from_percent(50) * exposed_101),
		);
	});
}
