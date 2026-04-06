//! Tests for legacy reward mode (EraPayout-based minting).
//!
//! Legacy mode is used on Kusama where inflation depends on the staking ratio.
//! These tests verify that the old mint-on-payout path works correctly when
//! `DisableMinting = false`.

use super::*;
use crate::mock::*;
use frame_support::assert_ok;

#[test]
fn legacy_end_era_computes_inflation_and_emits_era_paid() {
	ExtBuilder::default().legacy_reward_mode().build_and_execute(|| {
		let era_duration = time_per_era();
		let total = era_duration as Balance;
		let expected_remainder = RemainderRatio::get() * total;
		let expected_stakers = total - expected_remainder;

		Session::roll_until_active_era(2);

		// Legacy mode emits EraPaid with real remainder.
		assert!(staking_events_since_last_call().contains(&Event::EraPaid {
			era_index: 1,
			validator_payout: expected_stakers,
			remainder: expected_remainder,
		}));

		// Era reward is stored for later payout.
		assert_eq!(ErasValidatorReward::<Test>::get(1).unwrap(), expected_stakers);
	});
}

#[test]
fn legacy_reward_remainder_handler_called() {
	ExtBuilder::default().legacy_reward_mode().build_and_execute(|| {
		RewardRemainderUnbalanced::set(0);

		Session::roll_until_active_era(2);

		// RewardRemainder handler should have received the treasury portion.
		assert!(RewardRemainderUnbalanced::get() > 0);

		let total = time_per_era() as Balance;
		let expected_remainder = RemainderRatio::get() * total;
		assert_eq!(RewardRemainderUnbalanced::get(), expected_remainder);
	});
}

#[test]
fn legacy_payout_mints_tokens() {
	ExtBuilder::default().legacy_reward_mode().build_and_execute(|| {
		Staking::reward_by_ids(vec![(11, 1)]);
		Session::roll_until_active_era(2);

		let pre_payout_issuance = pallet_balances::TotalIssuance::<Test>::get();

		// Payout should mint (increase total issuance).
		assert_ok!(Staking::payout_stakers(RuntimeOrigin::signed(1337), 11, 1));

		assert!(pallet_balances::TotalIssuance::<Test>::get() > pre_payout_issuance);
	});
}

#[test]
fn legacy_max_staked_rewards_caps_staker_payout() {
	ExtBuilder::default().legacy_reward_mode().build_and_execute(|| {
		// Set MaxStakedRewards to 10%.
		MaxStakedRewards::<Test>::set(Some(Percent::from_percent(10)));

		Staking::reward_by_ids(vec![(11, 1)]);
		Session::roll_until_active_era(2);

		let total = time_per_era() as Balance;
		let expected_stakers = Percent::from_percent(10) * total;
		let expected_remainder = total - expected_stakers;

		assert!(staking_events_since_last_call().contains(&Event::EraPaid {
			era_index: 1,
			validator_payout: expected_stakers,
			remainder: expected_remainder,
		}));

		assert_eq!(ErasValidatorReward::<Test>::get(1).unwrap(), expected_stakers);
	});
}

#[test]
fn legacy_max_era_duration_caps_payout() {
	ExtBuilder::default().legacy_reward_mode().build_and_execute(|| {
		// Set MaxEraDuration to half of time_per_era.
		let half = time_per_era() / 2;
		MaxEraDuration::set(half);

		Session::roll_until_active_era(2);

		let capped_total = half as Balance;
		let expected_remainder = RemainderRatio::get() * capped_total;
		let expected_stakers = capped_total - expected_remainder;

		let events = staking_events_since_last_call();
		assert!(events.contains(&Event::Unexpected(UnexpectedKind::EraDurationBoundExceeded)));
		assert!(events.contains(&Event::EraPaid {
			era_index: 1,
			validator_payout: expected_stakers,
			remainder: expected_remainder,
		}));
	});
}

#[test]
fn legacy_disable_minting_guard_not_set() {
	ExtBuilder::default().legacy_reward_mode().build_and_execute(|| {
		// In legacy mode, the guard should never be set.
		assert_eq!(DisableMintingGuard::<Test>::get(), None);

		Session::roll_until_active_era(5);

		// Still not set after multiple eras.
		assert_eq!(DisableMintingGuard::<Test>::get(), None);
	});
}

#[test]
fn legacy_no_era_pots_created() {
	ExtBuilder::default().legacy_reward_mode().build_and_execute(|| {
		Staking::reward_by_ids(vec![(11, 1)]);
		Session::roll_until_active_era(2);

		// No reward pot should exist for this era.
		assert!(!crate::reward::EraRewardManager::<Test>::has_staker_rewards_pot(1));

		// Payout still works (via legacy mint).
		assert_ok!(Staking::payout_stakers(RuntimeOrigin::signed(1337), 11, 1));
	});
}
