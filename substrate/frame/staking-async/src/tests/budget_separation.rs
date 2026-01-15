//! Tests for separated validator and nominator budgets

use crate::{
	mock::{Session, *},
	session_rotation::Eras,
	tests::mock::ExtBuilder,
	*,
};

/// Test that budget separation works correctly with the Test Budget Provider.
/// Validators and nominators are paid from separate budgets.
#[test]
fn test_budget_separation_basic() {
	ExtBuilder::default().build_and_execute(|| {
		// Validator 11 has own stake: 1000
		// Nominator 101 nominates 11 with stake: 500
		// Total for validator 11: 1500
		// Reward points for era 1: validator 11 gets 1 point

		Eras::<T>::reward_active_era(vec![(11, 1)]);
		Session::roll_until_active_era(2);

		let _ = staking_events_since_last_call();

		// Make payout
		mock::make_all_reward_payment(1);

		let events = staking_events_since_last_call();

		// Should get PayoutStarted event and two Rewarded events (validator and nominator)
		assert!(events.contains(&Event::PayoutStarted {
			era_index: 1,
			validator_stash: 11,
			page: 0,
			next: None
		}));

		// Check that both validator and nominator got rewards
		let validator_reward = events.iter().find_map(|e| match e {
			Event::Rewarded { stash: 11, amount, .. } => Some(*amount),
			_ => None,
		}).expect("Validator should be rewarded");

		let nominator_reward = events.iter().find_map(|e| match e {
			Event::Rewarded { stash: 101, amount, .. } => Some(*amount),
			_ => None,
		}).expect("Nominator should be rewarded");

		// Validator gets more than nominator because:
		// 1. Validator has own stake (1000) vs nominator (500)
		// 2. Validator gets rewards from validator budget based on own stake
		// 3. Nominator only gets rewards from nominator budget
		assert!(validator_reward > nominator_reward);

		// Total rewards should be non-zero
		assert!(validator_reward + nominator_reward > 0);
	});
}

/// Test that when there are no nominators, validator still gets paid from validator budget.
#[test]
fn test_validator_only_rewards() {
	// Start with stakers but without nominators
	ExtBuilder::default().nominate(false).build_and_execute(|| {
		// Validator 11 and 21 exist but no nominators
		Eras::<T>::reward_active_era(vec![(11, 1)]);
		Session::roll_until_active_era(2);

		let _ = staking_events_since_last_call();

		mock::make_all_reward_payment(1);

		let events = staking_events_since_last_call();

		// Should get PayoutStarted event and at least one Rewarded event for the validator
		assert!(events.contains(&Event::PayoutStarted {
			era_index: 1,
			validator_stash: 11,
			page: 0,
			next: None
		}));

		// Validator should be rewarded (own stake from validator budget)
		let has_validator_reward = events.iter().any(|e| matches!(
			e,
			Event::Rewarded { stash: 11, .. }
		));
		assert!(has_validator_reward, "Validator should receive rewards from validator budget");
	});
}

/// Test that budget pots are properly funded at era start.
#[test]
fn test_era_pots_funded() {
	ExtBuilder::default().build_and_execute(|| {
		Eras::<T>::reward_active_era(vec![(11, 1)]);

		let _events_before = staking_events_since_last_call();

		Session::roll_until_active_era(2);

		let events = staking_events_since_last_call();

		// Should have EraPotsFunded event
		let funded_event = events.iter().find_map(|e| match e {
			Event::EraPotsFunded { era_index, validator_budget, nominator_budget } =>
				Some((*era_index, *validator_budget, *nominator_budget)),
			_ => None,
		}).expect("EraPotsFunded event should be emitted");

		let (era_index, validator_budget, nominator_budget) = funded_event;

		// Era 1 should be funded
		assert_eq!(era_index, 1);

		// Both budgets should be non-zero and equal (50/50 split in tests)
		assert!(validator_budget > 0);
		assert!(nominator_budget > 0);
		assert_eq!(validator_budget, nominator_budget);
	});
}
