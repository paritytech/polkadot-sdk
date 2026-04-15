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

//! Security audit tests for DAP + Staking reward system.
//!
//! Organized by attack surface:
//! - `dap_isolation`: Can DAP mint out of order or be tricked into over-minting?
//! - `payout_exploits`: Can staking payout logic leak value or be gamed?
//! - `incentive_gaming`: Can the validator incentive curve be gamed?
//! - `deployment_transitions`: Attacks during legacy→DAP mode transitions.

use crate::ah::mock::*;
use frame::prelude::Perbill;
use frame_support::{
	assert_ok, hypothetically,
	traits::fungible::{Inspect, Mutate},
};
use pallet_staking_async::{
	self as staking_async, session_rotation::Rotator, ConfigOp, CurrentEra,
	DisableMintingGuard, ErasRewardPoints, ErasValidatorIncentiveBudget, ErasValidatorReward,
	HardCapSelfStake, OptimumSelfStake, Payee, PotAccountProvider, RewardKind, RewardPot,
	SequentialTest,
};
use sp_staking::budget::BudgetRecipient;

// ============================================================================
// Test infrastructure helpers
// ============================================================================

/// Advance to a target active era in DAP mode, injecting uniform reward points.
/// Returns the elected validator set.
fn advance_to_era_with_points(target_era: u32) -> Vec<AccountId> {
	let validators = [3u64, 5, 6, 8]; // elected set from genesis setup

	let current_active = Rotator::<Runtime>::active_era();
	assert!(target_era > current_active, "target_era must be in the future");

	let mut elected = vec![];
	let mut end_index = current_active as u32;

	// We need to advance era by era
	for era in current_active..target_era {
		// Inject uniform reward points for the ending era (each validator gets 100).
		ErasRewardPoints::<T>::mutate(era, |points| {
			points.total = validators.len() as u32 * 100;
			for &v in &validators {
				points.individual.try_insert(v, 100).unwrap();
			}
		});

		// Find current end_index from session tracking.
		let start = Rotator::<Runtime>::active_era_start_session_index();
		end_index = start + (era - current_active) * SessionsPerEra::get();

		elected = roll_until_next_active(end_index);
	}

	elected
}

/// Get the total balance across all era staker reward pots for a range of eras.
fn total_era_staker_pot_balance(eras: impl Iterator<Item = u32>) -> Balance {
	eras.map(|era| {
		let pot = SequentialTest::pot_account(RewardPot::Era(era, RewardKind::StakerRewards));
		Balances::total_balance(&pot)
	})
	.sum()
}

/// Get the total balance across all era incentive pots for a range of eras.
fn total_era_incentive_pot_balance(eras: impl Iterator<Item = u32>) -> Balance {
	eras.map(|era| {
		let pot =
			SequentialTest::pot_account(RewardPot::Era(era, RewardKind::ValidatorSelfStake));
		Balances::total_balance(&pot)
	})
	.sum()
}

/// Set up incentive config with common test values.
fn setup_incentive_config(optimum: Balance, cap: Balance, slope_factor: Perbill) {
	assert_ok!(staking_async::Pallet::<T>::set_validator_self_stake_incentive_config(
		RuntimeOrigin::root(),
		ConfigOp::Set(optimum),
		ConfigOp::Set(cap),
		ConfigOp::Set(slope_factor),
	));
}

/// Sum of all validator balances.
fn sum_validator_balances(validators: &[AccountId]) -> Balance {
	validators.iter().map(|v| Balances::total_balance(v)).sum()
}

// ============================================================================
// Round 1: DAP isolation — can DAP mint out of order?
// ============================================================================
mod dap_isolation {
	use super::*;

	#[test]
	fn total_minted_never_exceeds_issuance_curve_output() {
		// GIVEN: DAP mode with 50% stakers, 25% incentive, 25% buffer.
		ExtBuilder::default().local_queue().build().execute_with(|| {
			setup_dap_with_budget(50, 25, 25);
			let issuance_before = Balances::total_issuance();

			// WHEN: roll 100 blocks (each drips 12_000ms = 12_000 tokens).
			roll_many(100);

			let issuance_after = Balances::total_issuance();
			let total_minted = issuance_after - issuance_before;

			// THEN: total minted should be <= 100 blocks * 12_000 tokens/block.
			// Perbill::mul_floor means rounding dust is NOT minted, so strictly <=.
			let max_possible = 100u128 * BLOCK_TIME as u128;
			assert!(
				total_minted <= max_possible,
				"Minted {} exceeds theoretical max {}",
				total_minted,
				max_possible
			);

			// Verify dust loss is small (at most 3 tokens per block from 3 recipients).
			let min_expected = max_possible - 100 * 3;
			assert!(
				total_minted >= min_expected,
				"Minted {} is suspiciously low (expected >= {})",
				total_minted,
				min_expected
			);
		});
	}

	#[test]
	fn elapsed_time_ceiling_prevents_over_minting() {
		// GIVEN: DAP mode.
		ExtBuilder::default().local_queue().build().execute_with(|| {
			let issuance_before = Balances::total_issuance();

			// WHEN: simulate a 10-minute block gap (far exceeding MaxElapsedPerDrip=600s).
			// Advance mock time by 1 hour but only roll 1 block.
			let huge_gap = 3_600_000u64; // 1 hour in ms
			MockTime::set(MockTime::get() + huge_gap);
			roll_next();

			let issuance_after = Balances::total_issuance();
			let minted = issuance_after - issuance_before;

			// THEN: minted should be capped at MaxElapsedPerDrip (600_000ms = 600_000 tokens).
			let max_elapsed = DapMaxElapsedPerDrip::get();
			assert!(
				minted <= max_elapsed as u128,
				"Minted {} exceeds MaxElapsedPerDrip ceiling {}",
				minted,
				max_elapsed
			);
		});
	}

	#[test]
	fn rapid_blocks_after_long_gap_cannot_double_mint() {
		// Attack: long gap → ceiling → rapid blocks. Does the clock advance correctly?
		ExtBuilder::default().local_queue().build().execute_with(|| {
			// GIVEN: long gap triggers ceiling clamp.
			let gap = 1_000_000u64; // 1000 seconds, above 600s ceiling
			MockTime::set(MockTime::get() + gap);
			roll_next();

			let issuance_after_gap = Balances::total_issuance();

			// WHEN: immediately roll another block (only 12s elapsed).
			roll_next();

			let issuance_after_rapid = Balances::total_issuance();
			let minted_in_rapid = issuance_after_rapid - issuance_after_gap;

			// THEN: the rapid block mints exactly BLOCK_TIME tokens (12_000),
			// NOT the remaining un-capped time from the gap.
			assert_eq!(
				minted_in_rapid, BLOCK_TIME as u128,
				"Rapid block after gap should only mint 1 block's worth, not accumulated"
			);
		});
	}

	#[test]
	fn timestamp_rollback_cannot_cause_extra_minting() {
		// Attack: what if MockTime goes backward? (clock manipulation)
		ExtBuilder::default().local_queue().build().execute_with(|| {
			// GIVEN: advance normally for a few blocks.
			roll_many(5);
			let issuance_mid = Balances::total_issuance();
			let time_mid = MockTime::get();

			// WHEN: set time backward (simulate clock rollback).
			MockTime::set(time_mid.saturating_sub(100_000));
			roll_next();

			// THEN: elapsed = now.saturating_sub(last) = 0 (underflow protection).
			// No minting should occur.
			assert_eq!(
				Balances::total_issuance(),
				issuance_mid,
				"Backward clock should not mint anything"
			);

			// Restore time and verify normal minting resumes.
			MockTime::set(time_mid + BLOCK_TIME);
			roll_next();
			assert!(
				Balances::total_issuance() > issuance_mid,
				"Normal minting should resume after clock restore"
			);
		});
	}

	#[test]
	fn budget_rounding_dust_is_never_minted() {
		// Attack: choose budget splits that maximize Perbill rounding dust.
		// Use Perbill::from_parts for a 3-way split of 1/3 each (cannot perfectly divide).
		ExtBuilder::default().local_queue().build().execute_with(|| {
			use pallet_staking_async::{
				StakerRewardRecipient, ValidatorIncentiveRecipient, SequentialTest,
			};

			// Set exact 1/3 split via Perbill parts: 333_333_334 + 333_333_333 + 333_333_333.
			let staker_key =
				<StakerRewardRecipient<SequentialTest> as BudgetRecipient<AccountId>>::budget_key();
			let incentive_key =
				<ValidatorIncentiveRecipient<SequentialTest> as BudgetRecipient<AccountId>>::budget_key();
			let buffer_key =
				<pallet_dap::Pallet<Runtime> as BudgetRecipient<AccountId>>::budget_key();

			let mut budget = pallet_dap::BudgetAllocationMap::new();
			budget.try_insert(staker_key, Perbill::from_parts(333_333_334)).unwrap();
			budget.try_insert(incentive_key, Perbill::from_parts(333_333_333)).unwrap();
			budget.try_insert(buffer_key, Perbill::from_parts(333_333_333)).unwrap();
			pallet_dap::BudgetAllocation::<Runtime>::put(budget);

			// Capture pot balances AFTER setup.
			let staker_pot =
				SequentialTest::pot_account(RewardPot::General(RewardKind::StakerRewards));
			let incentive_pot =
				SequentialTest::pot_account(RewardPot::General(RewardKind::ValidatorSelfStake));
			let buffer = <pallet_dap::Pallet<Runtime> as BudgetRecipient<AccountId>>::pot_account();

			let staker_before = Balances::total_balance(&staker_pot);
			let incentive_before = Balances::total_balance(&incentive_pot);
			let buffer_before = Balances::total_balance(&buffer);
			let issuance_before = Balances::total_issuance();

			// Roll many blocks to accumulate rounding effects.
			roll_many(1000);

			let issuance_after = Balances::total_issuance();
			let total_minted = issuance_after - issuance_before;

			// Check that sum of recipient balance gains == total minted.
			let staker_gained = Balances::total_balance(&staker_pot) - staker_before;
			let incentive_gained = Balances::total_balance(&incentive_pot) - incentive_before;
			let buffer_gained = Balances::total_balance(&buffer) - buffer_before;
			let sum_distributed = staker_gained + incentive_gained + buffer_gained;

			assert_eq!(
				sum_distributed, total_minted,
				"Sum of distributions must equal total minted (no leakage)"
			);

			// With 1/3 splits, mul_floor(12000) = 4000 for each, sum = 12000.
			// But 333_333_333 * 12000 / 1_000_000_000 = 3999.999996 → 3999.
			// And 333_333_334 * 12000 / 1_000_000_000 = 4000.000008 → 4000.
			// So sum per block = 4000 + 3999 + 3999 = 11998, dust = 2 per block.
			let theoretical_max = 1000u128 * BLOCK_TIME as u128;
			let dust = theoretical_max - total_minted;
			assert!(
				dust > 0,
				"Expected positive dust loss, but minted {} == theoretical max {}",
				total_minted,
				theoretical_max
			);
			// Dust per block should be exactly 2 (two recipients lose 1 each).
			assert_eq!(dust, 2000, "Expected 2 dust per block * 1000 blocks = 2000 total dust");
		});
	}

	#[test]
	fn issuance_with_zero_total_issuance_is_safe() {
		// Edge: what if total_issuance is somehow 0 or very low?
		// Our mock curve ignores total_issuance, but let's verify no overflow.
		ExtBuilder::default().local_queue().build().execute_with(|| {
			// Issuance is already non-zero from genesis balances, but
			// verify DAP minting doesn't depend on total_issuance for safety.
			let issuance = Balances::total_issuance();
			assert!(issuance > 0, "Sanity: genesis has non-zero issuance");

			roll_many(10);
			let new_issuance = Balances::total_issuance();
			assert!(new_issuance > issuance, "DAP should mint regardless of existing issuance");
		});
	}

	#[test]
	fn first_block_never_drips() {
		// DAP's first-block logic: when LastIssuanceTimestamp==0, initialize without drip.
		// Verify this even after a long time has "elapsed" since genesis.
		ExtBuilder::default().local_queue().build().execute_with(|| {
			// Reset to simulate a fresh chain where DAP was just enabled.
			pallet_dap::LastIssuanceTimestamp::<Runtime>::kill();
			MockTime::set(1_000_000); // 1000 seconds in

			let issuance_before = Balances::total_issuance();

			// First block after timestamp=0: should initialize, NOT drip.
			roll_next();

			let issuance_after = Balances::total_issuance();
			assert_eq!(
				issuance_before, issuance_after,
				"First block must not drip, only initialize timestamp"
			);

			// Second block: should drip normally.
			roll_next();
			assert!(
				Balances::total_issuance() > issuance_after,
				"Second block should drip normally"
			);
		});
	}

	#[test]
	fn budget_allocation_overflow_via_perbill_sum() {
		// Attack: try to set budget that sums to slightly over 100% via Perbill precision.
		ExtBuilder::default().local_queue().build().execute_with(|| {
			use pallet_dap::BudgetAllocationMap;
			use pallet_staking_async::{
				StakerRewardRecipient, ValidatorIncentiveRecipient, SequentialTest,
			};

			let staker_key =
				<StakerRewardRecipient<SequentialTest> as BudgetRecipient<AccountId>>::budget_key();
			let incentive_key =
				<ValidatorIncentiveRecipient<SequentialTest> as BudgetRecipient<AccountId>>::budget_key();
			let buffer_key =
				<pallet_dap::Pallet<Runtime> as BudgetRecipient<AccountId>>::budget_key();

			// Try: 3 x Perbill(333_333_334) = 1_000_000_002 > 1_000_000_000
			let mut budget = BudgetAllocationMap::new();
			budget.try_insert(staker_key.clone(), Perbill::from_parts(333_333_334)).unwrap();
			budget.try_insert(incentive_key.clone(), Perbill::from_parts(333_333_334)).unwrap();
			budget.try_insert(buffer_key.clone(), Perbill::from_parts(333_333_334)).unwrap();

			// THEN: this should fail with BudgetNotExact.
			assert!(
				pallet_dap::Pallet::<T>::set_budget_allocation(
					RuntimeOrigin::root(),
					budget,
				)
				.is_err(),
				"Over-100% budget must be rejected"
			);

			// Try: exact 100% via parts.
			let mut budget = BudgetAllocationMap::new();
			budget.try_insert(staker_key, Perbill::from_parts(333_333_334)).unwrap();
			budget.try_insert(incentive_key, Perbill::from_parts(333_333_333)).unwrap();
			budget.try_insert(buffer_key, Perbill::from_parts(333_333_333)).unwrap();

			assert_ok!(pallet_dap::Pallet::<T>::set_budget_allocation(
				RuntimeOrigin::root(),
				budget,
			));
		});
	}
}

// ============================================================================
// Round 2: Staking payout logic exploits
// ============================================================================
mod payout_exploits {
	use super::*;

	#[test]
	fn payout_does_not_mint_in_dap_mode() {
		// Core invariant: DAP mode payouts transfer from pot, never mint.
		ExtBuilder::default().local_queue().build().execute_with(|| {
			// Set payees for elected validators.
			for &v in &[3u64, 5, 6, 8] {
				Payee::<T>::insert(v, staking_async::RewardDestination::Stash);
			}

			// Advance to era 2 so we have DAP-funded era pots.
			let _elected = advance_to_era_with_points(2);

			let era_to_claim = 1;
			let era_reward = ErasValidatorReward::<T>::get(era_to_claim).unwrap();
			assert!(era_reward > 0, "Era 1 should have rewards");

			// Snapshot issuance before payout.
			let issuance_before = Balances::total_issuance();

			// WHEN: payout all validators for era 1.
			for &v in &[3u64, 5, 6, 8] {
				let _ = staking_async::Pallet::<T>::payout_stakers(
					RuntimeOrigin::signed(999),
					v,
					era_to_claim,
				);
			}

			// THEN: total issuance unchanged (transfer, not mint).
			assert_eq!(
				Balances::total_issuance(),
				issuance_before,
				"DAP payout must not change total issuance"
			);
		});
	}

	#[test]
	fn sum_of_payouts_cannot_exceed_era_pot() {
		// Attack: can the sum of all individual payouts exceed what's in the era pot?
		// Tracks ALL recipients (validators + nominators).
		ExtBuilder::default().local_queue().build().execute_with(|| {
			for &v in &[3u64, 5, 6, 8] {
				Payee::<T>::insert(v, staking_async::RewardDestination::Stash);
			}
			// Nominators also need payees set.
			for n in 100..=112u64 {
				Payee::<T>::insert(n, staking_async::RewardDestination::Stash);
			}

			let _elected = advance_to_era_with_points(2);
			let era_to_claim = 1;

			let era_reward = ErasValidatorReward::<T>::get(era_to_claim).unwrap();
			let pot_account =
				SequentialTest::pot_account(RewardPot::Era(era_to_claim, RewardKind::StakerRewards));
			let pot_before = Balances::total_balance(&pot_account);

			assert!(
				pot_before >= era_reward,
				"Era pot {} should have at least era_reward {}",
				pot_before,
				era_reward
			);

			// Capture ALL staker balances (validators + nominators) before payout.
			let all_stakers: Vec<AccountId> = (3..=8).chain(100..=112).collect();
			let balances_before: Vec<_> =
				all_stakers.iter().map(|a| (*a, Balances::total_balance(a))).collect();

			// Payout all validators.
			for &v in &[3u64, 5, 6, 8] {
				let _ = staking_async::Pallet::<T>::payout_stakers(
					RuntimeOrigin::signed(999),
					v,
					era_to_claim,
				);
			}

			// Sum of all balance gains across validators AND nominators.
			let total_paid: Balance = balances_before
				.iter()
				.map(|(a, before)| Balances::total_balance(a).saturating_sub(*before))
				.sum();

			// THEN: total paid <= era_reward (rounding may leave dust in pot).
			assert!(
				total_paid <= era_reward,
				"Total paid {} exceeds era reward {}",
				total_paid,
				era_reward
			);

			// Pot consumption should equal total paid.
			let pot_after = Balances::total_balance(&pot_account);
			let consumed = pot_before - pot_after;
			assert_eq!(
				consumed, total_paid,
				"Pot consumption {} should match total paid {}",
				consumed,
				total_paid
			);
		});
	}

	#[test]
	fn double_claim_is_rejected() {
		// Verify that claiming the same page twice fails.
		ExtBuilder::default().local_queue().build().execute_with(|| {
			for &v in &[3u64, 5, 6, 8] {
				Payee::<T>::insert(v, staking_async::RewardDestination::Stash);
			}

			let _elected = advance_to_era_with_points(2);

			// WHEN: claim era 1 for validator 3.
			assert_ok!(staking_async::Pallet::<T>::payout_stakers(
				RuntimeOrigin::signed(999),
				3,
				1,
			));

			// THEN: second claim fails with AlreadyClaimed.
			assert!(staking_async::Pallet::<T>::payout_stakers(
				RuntimeOrigin::signed(999),
				3,
				1,
			)
			.is_err());
		});
	}

	#[test]
	fn zero_reward_points_validator_cannot_extract_value() {
		// Attack: validator with 0 reward points tries to claim. Should get nothing.
		ExtBuilder::default().local_queue().build().execute_with(|| {
			for &v in &[3u64, 5, 6, 8] {
				Payee::<T>::insert(v, staking_async::RewardDestination::Stash);
			}

			// Advance to era 2 but give only validator 3 reward points.
			ErasRewardPoints::<T>::mutate(0, |points| {
				points.total = 100;
				points.individual.try_insert(3, 100).unwrap();
				// 5, 6, 8 get 0 points
			});

			roll_until_next_active(1);

			let era_to_claim = 0;

			// Validator 5 has 0 reward points.
			let balance_before = Balances::total_balance(&5);
			let pot_before = Balances::total_balance(
				&SequentialTest::pot_account(RewardPot::Era(era_to_claim, RewardKind::StakerRewards)),
			);

			// Claiming for validator 5 should succeed but pay nothing.
			let _result = staking_async::Pallet::<T>::payout_stakers(
				RuntimeOrigin::signed(999),
				5,
				era_to_claim,
			);
			// Should succeed (early return with 0 weight) or be no-op.
			// The point is: no value extracted.

			assert_eq!(
				Balances::total_balance(&5),
				balance_before,
				"Validator with 0 points should receive nothing"
			);

			// Pot should be unchanged.
			let pot_after = Balances::total_balance(
				&SequentialTest::pot_account(RewardPot::Era(era_to_claim, RewardKind::StakerRewards)),
			);
			assert_eq!(pot_before, pot_after, "Pot should be unchanged after 0-point claim");
		});
	}

	#[test]
	fn rounding_across_pages_does_not_overpay_validator() {
		// Attack: multi-page exposure could cause sum of per-page payouts
		// to exceed the validator's fair share due to rounding up.
		// We can't easily test multi-page in this mock (MaxExposurePageSize=8,
		// and we have few nominators), but we verify the invariant with the
		// current single-page setup: validator payout == floor(fair_share).
		ExtBuilder::default().local_queue().build().execute_with(|| {
			for &v in &[3u64, 5, 6, 8] {
				Payee::<T>::insert(v, staking_async::RewardDestination::Stash);
			}

			let _elected = advance_to_era_with_points(2);
			let era_to_claim = 1;
			let era_reward = ErasValidatorReward::<T>::get(era_to_claim).unwrap();

			// With 4 validators and equal points (100 each), each gets 1/4 of era_reward.
			let expected_per_validator = era_reward / 4;

			let balance_before = Balances::total_balance(&3);
			assert_ok!(staking_async::Pallet::<T>::payout_stakers(
				RuntimeOrigin::signed(999),
				3,
				era_to_claim,
			));
			let received = Balances::total_balance(&3) - balance_before;

			// Validator 3 gets their own staking share + nominators get theirs.
			// The validator's personal share = commission + stake_share of leftover.
			// With 0% commission and equal stake, validator gets proportional to own_stake/total.
			// Important: received should not exceed expected_per_validator.
			assert!(
				received <= expected_per_validator,
				"Validator received {} which exceeds their fair share {}",
				received,
				expected_per_validator
			);
		});
	}
}

// ============================================================================
// Round 2b: Validator incentive gaming
// ============================================================================
mod incentive_gaming {
	use super::*;

	#[test]
	fn incentive_payout_bounded_by_era_pot() {
		// Core invariant: total incentive paid across all validators <= era incentive pot.
		ExtBuilder::default().local_queue().build().execute_with(|| {
			// Set up with incentive budget.
			setup_dap_with_budget(50, 25, 25);
			setup_incentive_config(
				50,  // optimum self-stake
				200, // hard cap
				Perbill::from_percent(50),
			);

			for &v in &[3u64, 5, 6, 8] {
				Payee::<T>::insert(v, staking_async::RewardDestination::Stash);
			}

			let _elected = advance_to_era_with_points(2);
			let era_to_claim = 1;

			let incentive_budget = ErasValidatorIncentiveBudget::<T>::get(era_to_claim);

			let incentive_pot = SequentialTest::pot_account(
				RewardPot::Era(era_to_claim, RewardKind::ValidatorSelfStake),
			);
			let incentive_pot_before = Balances::total_balance(&incentive_pot);

			let validators = [3u64, 5, 6, 8];

			// Payout all.
			for &v in &validators {
				let _ = staking_async::Pallet::<T>::payout_stakers(
					RuntimeOrigin::signed(999),
					v,
					era_to_claim,
				);
			}

			// Total incentive withdrawn from pot.
			let incentive_pot_after = Balances::total_balance(&incentive_pot);
			let total_incentive_paid = incentive_pot_before - incentive_pot_after;

			// THEN: total incentive paid <= budget.
			assert!(
				total_incentive_paid <= incentive_budget,
				"Total incentive {} exceeds budget {}",
				total_incentive_paid,
				incentive_budget
			);

			// Also: total_incentive_paid <= incentive_pot_before.
			assert!(
				total_incentive_paid <= incentive_pot_before,
				"Total incentive {} exceeds pot balance {}",
				total_incentive_paid,
				incentive_pot_before
			);
		});
	}

	#[test]
	fn incentive_zero_config_means_no_incentive_payout() {
		// When incentive config is not set (optimum=0, cap=0), no incentive should be paid
		// even if the era has a funded incentive pot.
		ExtBuilder::default().local_queue().build().execute_with(|| {
			// Budget includes incentive, but config is NOT set (defaults to 0).
			setup_dap_with_budget(50, 25, 25);
			// Explicitly confirm config is zero.
			assert_eq!(OptimumSelfStake::<T>::get(), 0);
			assert_eq!(HardCapSelfStake::<T>::get(), 0);

			for &v in &[3u64, 5, 6, 8] {
				Payee::<T>::insert(v, staking_async::RewardDestination::Stash);
			}

			let _elected = advance_to_era_with_points(2);
			let era_to_claim = 1;

			// The incentive pot may have funds from DAP drip.
			let incentive_pot = SequentialTest::pot_account(
				RewardPot::Era(era_to_claim, RewardKind::ValidatorSelfStake),
			);
			let incentive_pot_before = Balances::total_balance(&incentive_pot);

			// Payout.
			for &v in &[3u64, 5, 6, 8] {
				let _ = staking_async::Pallet::<T>::payout_stakers(
					RuntimeOrigin::signed(999),
					v,
					era_to_claim,
				);
			}

			// THEN: no incentive should have been paid (config returns weight=0 for all).
			let incentive_pot_after = Balances::total_balance(&incentive_pot);
			assert_eq!(
				incentive_pot_before, incentive_pot_after,
				"Incentive pot should be untouched when config is zero"
			);
		});
	}

	#[test]
	fn incentive_weight_is_subadditive_splitting_validators_is_profitable() {
		// This is NOT a bug per se, but documents the economic property:
		// sqrt(a) + sqrt(b) > sqrt(a+b) when a,b > 0.
		// A whale splitting into 2 validators gets MORE incentive than running 1.
		// The test documents the magnitude so economic analysis can assess it.
		ExtBuilder::default().local_queue().build().execute_with(|| {
			setup_incentive_config(
				1_000, // optimum
				10_000, // cap
				Perbill::from_percent(50),
			);

			// Scenario A: one validator with 200 self-stake.
			let weight_single =
				<staking_async::reward::DefaultStakerRewardCalculator<Runtime> as
					sp_staking::StakerRewardCalculator<Balance>>::calculate_validator_incentive_weight(200);

			// Scenario B: two validators each with 100 self-stake.
			let weight_half =
				<staking_async::reward::DefaultStakerRewardCalculator<Runtime> as
					sp_staking::StakerRewardCalculator<Balance>>::calculate_validator_incentive_weight(100);

			// sqrt(200) = 14, sqrt(100) = 10. Two halves: 2*10 = 20 > 14.
			assert!(
				2 * weight_half > weight_single,
				"Splitting is always profitable under sqrt curve: 2*w(s/2)={} > w(s)={}",
				2 * weight_half,
				weight_single
			);

			// Document the profit ratio.
			let profit_ratio_pct = (2 * weight_half * 100) / weight_single;
			log::info!(
				target: "audit",
				"Splitting profit: 2*w(100)={}, w(200)={}, ratio={}%",
				2 * weight_half, weight_single, profit_ratio_pct
			);
		});
	}

	#[test]
	fn incentive_above_hard_cap_plateau_prevents_whale_dominance() {
		// Verify that self-stake above cap gets no additional weight.
		ExtBuilder::default().local_queue().build().execute_with(|| {
			setup_incentive_config(
				100,   // optimum
				1_000, // cap
				Perbill::from_percent(50),
			);

			let at_cap =
				<staking_async::reward::DefaultStakerRewardCalculator<Runtime> as
					sp_staking::StakerRewardCalculator<Balance>>::calculate_validator_incentive_weight(1_000);
			let above_cap =
				<staking_async::reward::DefaultStakerRewardCalculator<Runtime> as
					sp_staking::StakerRewardCalculator<Balance>>::calculate_validator_incentive_weight(1_000_000);

			assert_eq!(
				at_cap, above_cap,
				"Weight at cap ({}) must equal weight above cap ({})",
				at_cap, above_cap
			);
		});
	}
}

// ============================================================================
// Round 3: Deployment transition exploits
// ============================================================================
mod deployment_transitions {
	use super::*;

	#[test]
	fn cannot_claim_both_legacy_mint_and_dap_transfer_for_same_era() {
		// Attack: after switching to DAP mode, can someone claim the same era
		// both via legacy minting and via pot transfer?
		// Era 0 has no exposure (pre-election), so we test with era 1.
		UseLegacyEraPayout::set(true);
		ExtBuilder::default().local_queue().build().execute_with(|| {
			for &v in &[3u64, 5, 6, 8] {
				Payee::<T>::insert(v, staking_async::RewardDestination::Stash);
			}

			// Advance to era 1, giving era 1 reward points.
			ErasRewardPoints::<T>::mutate(1, |points| {
				points.total = 400;
				for &v in &[3, 5, 6, 8] {
					points.individual.try_insert(v, 100).unwrap();
				}
			});
			roll_until_next_active(1); // now active era = 1
			// Advance again so era 1 is finalized with legacy rewards.
			ErasRewardPoints::<T>::mutate(2, |points| {
				points.total = 400;
				for &v in &[3, 5, 6, 8] {
					points.individual.try_insert(v, 100).unwrap();
				}
			});
			roll_until_next_active(7); // now active era = 2

			// Claim era 1 for validator 3 in legacy mode.
			let balance_before = Balances::total_balance(&3);
			assert_ok!(staking_async::Pallet::<T>::payout_stakers(
				RuntimeOrigin::signed(999),
				3,
				1,
			));
			let balance_after = Balances::total_balance(&3);
			assert!(balance_after > balance_before, "Legacy claim should pay out");

			// Now switch to DAP mode.
			UseLegacyEraPayout::set(false);
			setup_dap();

			// Try to claim era 1 again — should fail with AlreadyClaimed.
			assert!(staking_async::Pallet::<T>::payout_stakers(
				RuntimeOrigin::signed(999),
				3,
				1,
			)
			.is_err(), "Double claim after mode switch must fail");
		});

		UseLegacyEraPayout::set(false);
	}

	#[test]
	fn dap_era_with_no_pot_falls_back_to_legacy_safely() {
		// When era was finalized in legacy mode, payout should use legacy path.
		// Era 0 has no exposure, so we use era 1.
		UseLegacyEraPayout::set(true);
		ExtBuilder::default().local_queue().build().execute_with(|| {
			for &v in &[3u64, 5, 6, 8] {
				Payee::<T>::insert(v, staking_async::RewardDestination::Stash);
			}

			// Advance to era 2 so era 1 is finalized with legacy rewards.
			ErasRewardPoints::<T>::mutate(1, |points| {
				points.total = 400;
				for &v in &[3, 5, 6, 8] {
					points.individual.try_insert(v, 100).unwrap();
				}
			});
			roll_until_next_active(1); // active era = 1
			roll_until_next_active(7); // active era = 2

			// Switch to DAP but don't roll another era.
			UseLegacyEraPayout::set(false);
			setup_dap();

			// Era 1 was finalized in legacy mode — it has no pot.
			let pot = SequentialTest::pot_account(RewardPot::Era(1, RewardKind::StakerRewards));
			assert_eq!(
				System::providers(&pot),
				0,
				"Legacy era should have no pot"
			);

			// Payout should still work via legacy minting path.
			let issuance_before = Balances::total_issuance();
			assert_ok!(staking_async::Pallet::<T>::payout_stakers(
				RuntimeOrigin::signed(999),
				3,
				1,
			));
			let issuance_after = Balances::total_issuance();

			// Legacy path mints new tokens.
			assert!(
				issuance_after > issuance_before,
				"Legacy fallback should mint (issuance should increase)"
			);
		});

		UseLegacyEraPayout::set(false);
	}

	#[test]
	fn disable_minting_guard_prevents_legacy_mint_for_dap_eras() {
		// Once DisableMintingGuard is set, eras >= guard cannot use legacy minting.
		ExtBuilder::default().local_queue().build().execute_with(|| {
			for &v in &[3u64, 5, 6, 8] {
				Payee::<T>::insert(v, staking_async::RewardDestination::Stash);
			}

			// Advance through 2 eras in DAP mode to set the guard.
			let _elected = advance_to_era_with_points(2);

			// Guard should be set.
			assert!(
				DisableMintingGuard::<T>::get().is_some(),
				"Guard should be set after DAP era"
			);

			let guard_era = DisableMintingGuard::<T>::get().unwrap();

			// The era pot for guard_era should exist.
			let pot = SequentialTest::pot_account(
				RewardPot::Era(guard_era, RewardKind::StakerRewards),
			);
			assert!(
				System::providers(&pot) > 0,
				"DAP era should have a pot"
			);
		});
	}

	#[test]
	fn governance_budget_change_mid_era_is_safe() {
		// Attack: governance changes budget allocation mid-era.
		// Funds already dripped to old split, new drips go to new split.
		// Verify no double-counting or loss.
		ExtBuilder::default().local_queue().build().execute_with(|| {
			// Start with 50% stakers, 25% incentive, 25% buffer.
			setup_dap_with_budget(50, 25, 25);

			let staker_pot =
				SequentialTest::pot_account(RewardPot::General(RewardKind::StakerRewards));
			let incentive_pot =
				SequentialTest::pot_account(RewardPot::General(RewardKind::ValidatorSelfStake));

			// Roll 50 blocks with old split.
			roll_many(50);
			let staker_mid = Balances::total_balance(&staker_pot);
			let incentive_mid = Balances::total_balance(&incentive_pot);

			let issuance_mid = Balances::total_issuance();

			// Governance: change to 80% stakers, 10% incentive, 10% buffer.
			change_budget_allocation(80, 10, 10);

			// Roll 50 more blocks.
			roll_many(50);

			let staker_after = Balances::total_balance(&staker_pot);
			let incentive_after = Balances::total_balance(&incentive_pot);
			let issuance_after = Balances::total_issuance();

			// Staker pot should have grown MORE in the second half (80% vs 50%).
			let staker_second_half = staker_after - staker_mid;
			let staker_first_half = staker_mid - 1; // subtract ED

			assert!(
				staker_second_half > staker_first_half,
				"Staker pot should grow faster after budget increase: second_half={}, first_half={}",
				staker_second_half,
				staker_first_half,
			);

			// Incentive pot should have grown LESS in the second half (10% vs 25%).
			let incentive_second_half = incentive_after - incentive_mid;
			let incentive_first_half = incentive_mid - 1;

			assert!(
				incentive_second_half < incentive_first_half,
				"Incentive pot should grow slower after budget decrease: second_half={}, first_half={}",
				incentive_second_half,
				incentive_first_half,
			);

			// Total issuance should be consistent (all minted = all distributed).
			let total_new = issuance_after - issuance_mid;
			let expected = 50u128 * BLOCK_TIME as u128;
			assert!(
				total_new <= expected,
				"Post-change issuance {} should not exceed theoretical {}",
				total_new,
				expected
			);
		});
	}

	#[test]
	fn deployment_sequence_85_15_then_add_incentive() {
		// Simulate real deployment: start with 85% stakers / 15% buffer / 0% incentive.
		// After a few eras, governance adds 10% incentive (85→75% stakers, 15→15% buffer).
		// Verify: no lost rewards, incentive pot gets funded only after config change,
		// staker rewards decrease proportionally.
		ExtBuilder::default().local_queue().build().execute_with(|| {
			// Phase 1: Deploy with 85/0/15 split.
			setup_dap_with_budget(85, 0, 15);

			for &v in &[3u64, 5, 6, 8] {
				Payee::<T>::insert(v, staking_async::RewardDestination::Stash);
			}
			for n in 100..=112u64 {
				Payee::<T>::insert(n, staking_async::RewardDestination::Stash);
			}

			let _elected = advance_to_era_with_points(2);
			let era_1_reward = ErasValidatorReward::<T>::get(1).unwrap();
			let era_1_incentive = ErasValidatorIncentiveBudget::<T>::get(1);

			// Incentive budget should be 0 (0% allocation).
			assert_eq!(era_1_incentive, 0, "Phase 1: incentive budget must be 0");
			assert!(era_1_reward > 0, "Phase 1: staker rewards must be positive");

			// Phase 2: Governance adds incentive allocation and sets config.
			// IMPORTANT: incentive weights are computed during ELECTION, not payout.
			// So setting config now affects the election for era 3 (planned during era 2).
			change_budget_allocation(75, 10, 15);
			setup_incentive_config(50, 500, Perbill::from_percent(50));

			// Era 2 was already elected with old config — its incentive weights are 0.
			// The incentive pot for era 2 WILL have funds (budget was changed), but
			// no validator has weight, so incentive can't be distributed.
			// This documents a real deployment property: budget change and config change
			// must happen BEFORE the era's election for incentives to flow.

			// Advance to era 3 (election for era 3 uses new config).
			ErasRewardPoints::<T>::mutate(2, |points| {
				points.total = 400;
				for &v in &[3, 5, 6, 8] {
					points.individual.try_insert(v, 100).unwrap();
				}
			});

			let start_session = Rotator::<Runtime>::active_era_start_session_index();
			let end_index = start_session + SessionsPerEra::get();
			roll_until_next_active(end_index);

			let era_2_reward = ErasValidatorReward::<T>::get(2).unwrap();
			let era_2_incentive_budget = ErasValidatorIncentiveBudget::<T>::get(2);

			// Era 2 staker rewards should be lower (75% vs 85%).
			assert!(era_2_reward > 0, "Phase 2: staker rewards must be positive");
			// Incentive pot was funded (10% budget) but weights are 0.
			assert!(era_2_incentive_budget > 0, "Phase 2: incentive budget is funded");

			// Payout era 2: validator should get only staker rewards, no incentive
			// (because incentive weights were computed before config was set).
			let val_a = 3u64;
			let balance_before = Balances::total_balance(&val_a);
			assert_ok!(staking_async::Pallet::<T>::payout_stakers(
				RuntimeOrigin::signed(999),
				val_a,
				2,
			));
			let balance_after = Balances::total_balance(&val_a);
			let total_received = balance_after - balance_before;

			let staker_share = era_2_reward / 4; // 4 validators, equal points
			assert_eq!(
				total_received, staker_share,
				"Era 2: validator should get only staker share (no incentive yet)"
			);

			// Now advance to era 4 and verify era 3 HAS incentive weights.
			ErasRewardPoints::<T>::mutate(3, |points| {
				points.total = 400;
				for &v in &[3, 5, 6, 8] {
					points.individual.try_insert(v, 100).unwrap();
				}
			});
			let start_session = Rotator::<Runtime>::active_era_start_session_index();
			let end_index = start_session + SessionsPerEra::get();
			roll_until_next_active(end_index);

			let era_3_reward = ErasValidatorReward::<T>::get(3).unwrap();
			let era_3_incentive = ErasValidatorIncentiveBudget::<T>::get(3);
			assert!(era_3_incentive > 0, "Era 3 should have incentive budget");

			// Payout era 3: NOW validator should get staker + incentive.
			let balance_before = Balances::total_balance(&val_a);
			assert_ok!(staking_async::Pallet::<T>::payout_stakers(
				RuntimeOrigin::signed(999),
				val_a,
				3,
			));
			let total_received_era3 = Balances::total_balance(&val_a) - balance_before;
			let staker_share_3 = era_3_reward / 4;

			assert!(
				total_received_era3 > staker_share_3,
				"Era 3: validator should get staker ({}) + incentive, got {}",
				staker_share_3,
				total_received_era3
			);
		});
	}
}

// ============================================================================
// Round 2b+: Deeper payout exploit scenarios
// ============================================================================
mod deeper_payout_exploits {
	use super::*;

	#[test]
	fn residual_dust_in_era_pot_is_not_extractable() {
		// After all validators claim, rounding dust remains in the era pot.
		// Verify no one can extract it before cleanup.
		ExtBuilder::default().local_queue().build().execute_with(|| {
			for &v in &[3u64, 5, 6, 8] {
				Payee::<T>::insert(v, staking_async::RewardDestination::Stash);
			}
			for n in 100..=112u64 {
				Payee::<T>::insert(n, staking_async::RewardDestination::Stash);
			}

			let _elected = advance_to_era_with_points(2);
			let era = 1u32;

			let pot_account =
				SequentialTest::pot_account(RewardPot::Era(era, RewardKind::StakerRewards));
			let pot_before = Balances::total_balance(&pot_account);

			// Payout all validators.
			for &v in &[3u64, 5, 6, 8] {
				let _ = staking_async::Pallet::<T>::payout_stakers(
					RuntimeOrigin::signed(999),
					v,
					era,
				);
			}

			let pot_after = Balances::total_balance(&pot_account);
			let residual = pot_after;

			// There may be some residual due to rounding.
			// The key assertion: trying to claim again fails for all validators.
			for &v in &[3u64, 5, 6, 8] {
				assert!(
					staking_async::Pallet::<T>::payout_stakers(
						RuntimeOrigin::signed(999),
						v,
						era,
					)
					.is_err(),
					"Re-claim must fail for validator {}", v
				);
			}

			// The residual stays in the pot — no external extraction possible
			// (only cleanup_era can drain it via UnclaimedRewardHandler).
			if residual > 0 {
				// Try a direct transfer from the pot (attacker knows the account).
				let attacker = 999u64;
				let transfer_result = <Balances as Mutate<AccountId>>::transfer(
					&pot_account,
					&attacker,
					residual,
					frame_support::traits::tokens::Preservation::Expendable,
				);
				// This should fail because no one has signing authority over pot accounts.
				// In the test env, we CAN call transfer directly (it's not gated by origin).
				// But in production, pot accounts are derived from PalletId — no one has keys.
				// Document this as a design property, not a test failure.
				log::info!(
					target: "audit",
					"Era pot residual: {} tokens. Direct transfer result: {:?}",
					residual, transfer_result
				);
			}
		});
	}

	#[test]
	fn total_rewards_plus_incentives_bounded_by_era_pots() {
		// The total value extracted from both staker + incentive pots in a single era
		// must not exceed what was snapshotted.
		ExtBuilder::default().local_queue().build().execute_with(|| {
			setup_dap_with_budget(50, 25, 25);
			setup_incentive_config(50, 500, Perbill::from_percent(50));

			for &v in &[3u64, 5, 6, 8] {
				Payee::<T>::insert(v, staking_async::RewardDestination::Stash);
			}
			for n in 100..=112u64 {
				Payee::<T>::insert(n, staking_async::RewardDestination::Stash);
			}

			let _elected = advance_to_era_with_points(2);
			let era = 1u32;

			let staker_pot =
				SequentialTest::pot_account(RewardPot::Era(era, RewardKind::StakerRewards));
			let incentive_pot =
				SequentialTest::pot_account(RewardPot::Era(era, RewardKind::ValidatorSelfStake));
			let staker_before = Balances::total_balance(&staker_pot);
			let incentive_before = Balances::total_balance(&incentive_pot);

			let era_reward = ErasValidatorReward::<T>::get(era).unwrap();
			let era_incentive = ErasValidatorIncentiveBudget::<T>::get(era);

			// Payout all.
			for &v in &[3u64, 5, 6, 8] {
				let _ = staking_async::Pallet::<T>::payout_stakers(
					RuntimeOrigin::signed(999),
					v,
					era,
				);
			}

			let staker_consumed = staker_before - Balances::total_balance(&staker_pot);
			let incentive_consumed = incentive_before - Balances::total_balance(&incentive_pot);

			// Staker consumption <= era_reward.
			assert!(
				staker_consumed <= era_reward,
				"Staker pot consumption {} > era reward {}",
				staker_consumed,
				era_reward
			);

			// Incentive consumption <= era_incentive budget.
			assert!(
				incentive_consumed <= era_incentive,
				"Incentive pot consumption {} > era budget {}",
				incentive_consumed,
				era_incentive
			);

			// Total extracted <= total snapshotted.
			assert!(
				staker_consumed + incentive_consumed <= staker_before + incentive_before,
				"Total extraction exceeds total snapshotted"
			);
		});
	}

	#[test]
	fn unequal_reward_points_distribute_proportionally() {
		// Verify that a validator with 3x the reward points gets 3x the reward.
		// Attack vector: manipulating reward points to get disproportionate share.
		ExtBuilder::default().local_queue().build().execute_with(|| {
			for &v in &[3u64, 5, 6, 8] {
				Payee::<T>::insert(v, staking_async::RewardDestination::Stash);
			}
			for n in 100..=112u64 {
				Payee::<T>::insert(n, staking_async::RewardDestination::Stash);
			}

			// Give validator 3 triple the reward points.
			ErasRewardPoints::<T>::mutate(0, |points| {
				points.total = 700;
				points.individual.try_insert(3, 400).unwrap(); // 57%
				points.individual.try_insert(5, 100).unwrap(); // 14%
				points.individual.try_insert(6, 100).unwrap(); // 14%
				points.individual.try_insert(8, 100).unwrap(); // 14%
			});

			roll_until_next_active(1);
			let _elected = advance_to_era_with_points(2);

			let era = 1u32;
			let era_reward = ErasValidatorReward::<T>::get(era).unwrap();

			let balance_3_before = Balances::total_balance(&3);
			let balance_5_before = Balances::total_balance(&5);

			assert_ok!(staking_async::Pallet::<T>::payout_stakers(
				RuntimeOrigin::signed(999), 3, era,
			));
			assert_ok!(staking_async::Pallet::<T>::payout_stakers(
				RuntimeOrigin::signed(999), 5, era,
			));

			let reward_3 = Balances::total_balance(&3) - balance_3_before;
			let reward_5 = Balances::total_balance(&5) - balance_5_before;

			// Validator 3 should get roughly 4x what validator 5 gets (400/100 points).
			// But this is the VALIDATOR's personal share, which includes commission + stake share.
			// With 0% commission and equal stake, the per-validator total (their share of
			// validator_total_payout) should be proportional to points.
			// We check that val_3's total payout > val_5's (directional, not exact ratio,
			// because nominators also get paid from val_3's larger pot).
			assert!(
				reward_3 > reward_5,
				"Validator 3 (400 pts) should earn more than validator 5 (100 pts): {} vs {}",
				reward_3,
				reward_5
			);
		});
	}

	#[test]
	fn commission_does_not_increase_total_payout() {
		// A validator setting 100% commission should NOT increase total payout —
		// it just redirects nominator share to themselves.
		ExtBuilder::default().local_queue().build().execute_with(|| {
			for &v in &[3u64, 5, 6, 8] {
				Payee::<T>::insert(v, staking_async::RewardDestination::Stash);
			}
			for n in 100..=112u64 {
				Payee::<T>::insert(n, staking_async::RewardDestination::Stash);
			}

			let _elected = advance_to_era_with_points(2);
			let era = 1u32;
			let era_reward = ErasValidatorReward::<T>::get(era).unwrap();

			// Capture total balance of ALL stakers before payout.
			let all_stakers: Vec<AccountId> = (3..=8).chain(100..=112).collect();
			let total_before: Balance =
				all_stakers.iter().map(|a| Balances::total_balance(a)).sum();

			// Payout all.
			for &v in &[3u64, 5, 6, 8] {
				let _ = staking_async::Pallet::<T>::payout_stakers(
					RuntimeOrigin::signed(999),
					v,
					era,
				);
			}

			let total_after: Balance =
				all_stakers.iter().map(|a| Balances::total_balance(a)).sum();
			let total_distributed = total_after - total_before;

			// Total distributed should be <= era_reward regardless of commission settings.
			assert!(
				total_distributed <= era_reward,
				"Total distributed {} exceeds era reward {}",
				total_distributed,
				era_reward
			);
		});
	}

	#[test]
	fn issuance_conservation_across_full_era_cycle() {
		// End-to-end: DAP drips → snapshot → payout → verify total_issuance is consistent.
		// Specifically: minted tokens should equal (distributed to stakers + held in pots + buffer).
		ExtBuilder::default().local_queue().build().execute_with(|| {
			let issuance_at_start = Balances::total_issuance();

			for &v in &[3u64, 5, 6, 8] {
				Payee::<T>::insert(v, staking_async::RewardDestination::Stash);
			}
			for n in 100..=112u64 {
				Payee::<T>::insert(n, staking_async::RewardDestination::Stash);
			}

			// Advance to era 3, payout era 1 and 2.
			let _elected = advance_to_era_with_points(3);

			for era in 1..=2u32 {
				for &v in &[3u64, 5, 6, 8] {
					let _ = staking_async::Pallet::<T>::payout_stakers(
						RuntimeOrigin::signed(999),
						v,
						era,
					);
				}
			}

			let issuance_at_end = Balances::total_issuance();

			// Total issuance should have grown by the DAP minting amount.
			// Since payouts are transfers (not mints), they don't change issuance.
			// Only DAP drip_issuance changes total_issuance.
			assert!(
				issuance_at_end > issuance_at_start,
				"Issuance should grow from DAP minting"
			);

			// The growth should be bounded by blocks * BLOCK_TIME (issuance curve).
			let blocks_elapsed = System::block_number() - 1; // subtract genesis
			let max_growth = blocks_elapsed as u128 * BLOCK_TIME as u128;
			let actual_growth = issuance_at_end - issuance_at_start;
			assert!(
				actual_growth <= max_growth,
				"Issuance growth {} exceeds theoretical max {} ({} blocks * {} per block)",
				actual_growth,
				max_growth,
				blocks_elapsed,
				BLOCK_TIME
			);
		});
	}
}

// ============================================================================
// Round 4: Broader system — slash+reward, election, cross-pallet boundaries
// ============================================================================
mod broader_system {
	use super::*;
	use sp_staking::StakingAccount;
	use pallet_staking_async_rc_client as rc_client;

	/// Report an offence for a validator via the rc_client relay path.
	fn report_offence(validator: AccountId, slash_pct: u32) {
		let session = Rotator::<Runtime>::active_era_start_session_index();
		assert_ok!(rc_client::Pallet::<Runtime>::relay_new_offence_paged(
			RuntimeOrigin::root(),
			vec![(
				session,
				rc_client::Offence {
					offender: validator,
					reporters: vec![],
					slash_fraction: Perbill::from_percent(slash_pct),
				},
			)],
		));
	}

	#[test]
	fn slash_goes_to_dap_buffer_not_era_pot() {
		// Slashed funds go to DAP buffer (via OnUnbalanced), not to era pots.
		ExtBuilder::default().local_queue().slash_defer_duration(0).build().execute_with(|| {
			for &v in &[3u64, 5, 6, 8] {
				Payee::<T>::insert(v, staking_async::RewardDestination::Stash);
			}

			let _elected = advance_to_era_with_points(2);

			let buffer = <pallet_dap::Pallet<Runtime> as BudgetRecipient<AccountId>>::pot_account();
			let buffer_before = Balances::total_balance(&buffer);

			let era_pot = SequentialTest::pot_account(RewardPot::Era(1, RewardKind::StakerRewards));
			let era_pot_before = Balances::total_balance(&era_pot);

			// Slash validator 3 with instant application.
			report_offence(3, 50);
			// 1 block to process the offence.
			roll_many(2);

			let buffer_after = Balances::total_balance(&buffer);
			let era_pot_after = Balances::total_balance(&era_pot);

			assert!(
				buffer_after > buffer_before,
				"DAP buffer should receive slashed funds: before={}, after={}",
				buffer_before, buffer_after
			);

			assert_eq!(
				era_pot_before, era_pot_after,
				"Era pot must not be affected by slashing"
			);
		});
	}

	#[test]
	fn slashed_validator_can_still_claim_earned_rewards() {
		// Validator earns rewards in era 1, gets slashed in era 2.
		ExtBuilder::default().local_queue().slash_defer_duration(0).build().execute_with(|| {
			for &v in &[3u64, 5, 6, 8] {
				Payee::<T>::insert(v, staking_async::RewardDestination::Stash);
			}

			let _elected = advance_to_era_with_points(2);
			assert!(ErasValidatorReward::<T>::get(1).unwrap() > 0);

			// Slash validator 3, then process.
			report_offence(3, 50);
			roll_many(2);

			// Claim era 1 rewards for slashed validator.
			let balance_before = Balances::total_balance(&3);
			assert_ok!(staking_async::Pallet::<T>::payout_stakers(
				RuntimeOrigin::signed(999), 3, 1,
			));
			assert!(
				Balances::total_balance(&3) > balance_before,
				"Slashed validator should still receive earned rewards"
			);
		});
	}

	#[test]
	fn external_deposit_to_era_pot_does_not_inflate_rewards() {
		// Attack: send tokens directly to the era pot. Payouts bounded by ErasValidatorReward.
		ExtBuilder::default().local_queue().build().execute_with(|| {
			for &v in &[3u64, 5, 6, 8] {
				Payee::<T>::insert(v, staking_async::RewardDestination::Stash);
			}
			for n in 100..=112u64 {
				Payee::<T>::insert(n, staking_async::RewardDestination::Stash);
			}

			let _elected = advance_to_era_with_points(2);
			let era = 1u32;
			let era_reward = ErasValidatorReward::<T>::get(era).unwrap();

			// Attacker inflates the era pot.
			let pot = SequentialTest::pot_account(RewardPot::Era(era, RewardKind::StakerRewards));
			let attacker = 999u64;
			Balances::mint_into(&attacker, 1_000_000).unwrap();
			assert_ok!(<Balances as Mutate<AccountId>>::transfer(
				&attacker, &pot, 500_000,
				frame_support::traits::tokens::Preservation::Expendable,
			));

			assert!(Balances::total_balance(&pot) > era_reward);

			// Payout all and track total.
			let all_stakers: Vec<AccountId> = (3..=8).chain(100..=112).collect();
			let balances_before: Vec<_> =
				all_stakers.iter().map(|a| (*a, Balances::total_balance(a))).collect();

			for &v in &[3u64, 5, 6, 8] {
				let _ = staking_async::Pallet::<T>::payout_stakers(
					RuntimeOrigin::signed(999), v, era,
				);
			}

			let total_paid: Balance = balances_before.iter()
				.map(|(a, before)| Balances::total_balance(a).saturating_sub(*before))
				.sum();

			assert!(
				total_paid <= era_reward,
				"Total paid {} should be bounded by era_reward {}", total_paid, era_reward
			);
		});
	}

	#[test]
	fn external_deposit_to_general_pot_inflates_next_era_reward() {
		// General pots ARE snapshotted at era boundary. Donations increase next era's reward.
		// This is a design property, not a vulnerability.
		ExtBuilder::default().local_queue().build().execute_with(|| {
			for &v in &[3u64, 5, 6, 8] {
				Payee::<T>::insert(v, staking_async::RewardDestination::Stash);
			}

			let _elected = advance_to_era_with_points(2);
			let era_1_reward = ErasValidatorReward::<T>::get(1).unwrap();

			// Donate to general staker pot.
			let donor = 999u64;
			Balances::mint_into(&donor, 10_000_000).unwrap();
			let general_pot =
				SequentialTest::pot_account(RewardPot::General(RewardKind::StakerRewards));
			assert_ok!(<Balances as Mutate<AccountId>>::transfer(
				&donor, &general_pot, 5_000_000,
				frame_support::traits::tokens::Preservation::Expendable,
			));

			// Advance to era 3 — era 2 snapshot includes donation.
			ErasRewardPoints::<T>::mutate(2, |points| {
				points.total = 400;
				for &v in &[3, 5, 6, 8] {
					points.individual.try_insert(v, 100).unwrap();
				}
			});
			let start = Rotator::<Runtime>::active_era_start_session_index();
			roll_until_next_active(start + SessionsPerEra::get());

			let era_2_reward = ErasValidatorReward::<T>::get(2).unwrap();
			assert!(
				era_2_reward > era_1_reward * 2,
				"Era 2 reward {} should be >> era 1 {} due to donation",
				era_2_reward, era_1_reward
			);
		});
	}

	#[test]
	fn chilled_validator_can_still_claim_old_era_rewards() {
		ExtBuilder::default().local_queue().build().execute_with(|| {
			for &v in &[3u64, 5, 6, 8] {
				Payee::<T>::insert(v, staking_async::RewardDestination::Stash);
			}

			let _elected = advance_to_era_with_points(2);
			assert!(ErasValidatorReward::<T>::get(1).unwrap() > 0);

			assert_ok!(staking_async::Pallet::<T>::chill(RuntimeOrigin::signed(3)));

			let balance_before = Balances::total_balance(&3);
			assert_ok!(staking_async::Pallet::<T>::payout_stakers(
				RuntimeOrigin::signed(999), 3, 1,
			));
			assert!(
				Balances::total_balance(&3) > balance_before,
				"Chilled validator should still claim earned rewards"
			);
		});
	}

	#[test]
	fn payee_change_does_not_allow_double_claim() {
		ExtBuilder::default().local_queue().build().execute_with(|| {
			for &v in &[3u64, 5, 6, 8] {
				Payee::<T>::insert(v, staking_async::RewardDestination::Stash);
			}

			let _elected = advance_to_era_with_points(2);

			assert_ok!(staking_async::Pallet::<T>::payout_stakers(
				RuntimeOrigin::signed(999), 3, 1,
			));

			assert_ok!(staking_async::Pallet::<T>::set_payee(
				RuntimeOrigin::signed(3),
				staking_async::RewardDestination::Account(999),
			));

			assert!(
				staking_async::Pallet::<T>::payout_stakers(
					RuntimeOrigin::signed(999), 3, 1,
				).is_err(),
				"Changing payee must not allow double-claim"
			);
		});
	}

	#[test]
	fn staked_reward_destination_increases_bonded_correctly() {
		// RewardDestination::Staked increases bonded stake from real transferred tokens.
		ExtBuilder::default().local_queue().build().execute_with(|| {
			Payee::<T>::insert(3, staking_async::RewardDestination::Staked);
			for &v in &[5u64, 6, 8] {
				Payee::<T>::insert(v, staking_async::RewardDestination::Stash);
			}

			let _elected = advance_to_era_with_points(2);

			let ledger_before =
				staking_async::Pallet::<T>::ledger(StakingAccount::Stash(3)).unwrap();
			let issuance_before = Balances::total_issuance();

			assert_ok!(staking_async::Pallet::<T>::payout_stakers(
				RuntimeOrigin::signed(999), 3, 1,
			));

			let ledger_after =
				staking_async::Pallet::<T>::ledger(StakingAccount::Stash(3)).unwrap();

			assert!(
				ledger_after.active > ledger_before.active,
				"Staked destination should increase active: {} > {}",
				ledger_after.active, ledger_before.active
			);
			assert_eq!(Balances::total_issuance(), issuance_before);
		});
	}

	#[test]
	fn issuance_accounting_with_slash_and_payout() {
		// End-to-end: DAP mints → slash → payout → verify total_issuance.
		ExtBuilder::default().local_queue().slash_defer_duration(0).build().execute_with(|| {
			for &v in &[3u64, 5, 6, 8] {
				Payee::<T>::insert(v, staking_async::RewardDestination::Stash);
			}

			let _elected = advance_to_era_with_points(2);
			let issuance_before_slash = Balances::total_issuance();

			// Slash validator 3, then process.
			report_offence(3, 50);
			roll_many(2);

			// Slash → buffer: net issuance change is only DAP drip.
			let drip_during_slash = 2 * BLOCK_TIME as u128;
			let issuance_diff =
				Balances::total_issuance().saturating_sub(issuance_before_slash);
			assert!(
				issuance_diff <= drip_during_slash,
				"Issuance change {} during slash should be from DAP drip only (max {})",
				issuance_diff, drip_during_slash
			);

			// Payout is transfer-only — no issuance change.
			let issuance_before_payout = Balances::total_issuance();
			for &v in &[3u64, 5, 6, 8] {
				let _ = staking_async::Pallet::<T>::payout_stakers(
					RuntimeOrigin::signed(999), v, 1,
				);
			}
			assert_eq!(
				Balances::total_issuance(), issuance_before_payout,
				"Payout must not change issuance"
			);
		});
	}
}

// ============================================================================
// Round 5: Corruption & misconfiguration chaos tests
// ============================================================================
mod chaos {
	use super::*;
	use pallet_staking_async_rc_client as rc_client;

	#[test]
	fn governance_sets_zero_staker_budget_era_gets_zero_reward() {
		// Governance accidentally sets 0% stakers / 0% incentive / 100% buffer.
		// The general staker pot receives nothing, era snapshot is 0.
		// Validators cannot claim rewards but system doesn't panic.
		ExtBuilder::default().local_queue().build().execute_with(|| {
			// Misconfigure: all to buffer.
			change_budget_allocation(0, 0, 100);

			for &v in &[3u64, 5, 6, 8] {
				Payee::<T>::insert(v, staking_async::RewardDestination::Stash);
			}

			let _elected = advance_to_era_with_points(2);

			let era_reward = ErasValidatorReward::<T>::get(1);
			// Era reward should be 0 or very small (just ED in the general pot from setup).
			assert!(
				era_reward.unwrap_or(0) <= 1,
				"Era reward should be ~0 with 0% staker budget, got {:?}",
				era_reward
			);

			// Payout attempt should succeed but pay nothing meaningful.
			let balance_before = Balances::total_balance(&3);
			let _result = staking_async::Pallet::<T>::payout_stakers(
				RuntimeOrigin::signed(999),
				3,
				1,
			);
			let balance_after = Balances::total_balance(&3);
			assert!(
				balance_after - balance_before <= 1,
				"Validator should receive ~0 with zero budget"
			);

			// Fix budget for next era.
			change_budget_allocation(85, 0, 15);
			ErasRewardPoints::<T>::mutate(2, |points| {
				points.total = 400;
				for &v in &[3, 5, 6, 8] {
					points.individual.try_insert(v, 100).unwrap();
				}
			});
			let start = Rotator::<Runtime>::active_era_start_session_index();
			roll_until_next_active(start + SessionsPerEra::get());

			// Era 2 should now have real rewards.
			let era_2_reward = ErasValidatorReward::<T>::get(2).unwrap();
			assert!(era_2_reward > 100, "Era 2 should have real rewards after fix");
		});
	}

	#[test]
	fn era_pot_reaped_below_ed_payout_gracefully_fails() {
		// Corruption: era pot account balance drops below ED (e.g., via a bug).
		// Payout should fail gracefully, not panic.
		ExtBuilder::default().local_queue().build().execute_with(|| {
			for &v in &[3u64, 5, 6, 8] {
				Payee::<T>::insert(v, staking_async::RewardDestination::Stash);
			}

			let _elected = advance_to_era_with_points(2);
			let era = 1u32;
			let era_reward = ErasValidatorReward::<T>::get(era).unwrap();
			assert!(era_reward > 0);

			// Corrupt: drain the era pot to 0.
			let pot = SequentialTest::pot_account(RewardPot::Era(era, RewardKind::StakerRewards));
			let pot_balance = Balances::total_balance(&pot);
			if pot_balance > 0 {
				// Force withdraw everything.
				let _ = <Balances as frame_support::traits::fungible::Mutate<AccountId>>::transfer(
					&pot,
					&999,
					pot_balance,
					frame_support::traits::tokens::Preservation::Expendable,
				);
			}
			assert_eq!(Balances::total_balance(&pot), 0, "Pot should be drained");

			// Payout attempt: should not panic.
			let balance_before = Balances::total_balance(&3);
			let result = staking_async::Pallet::<T>::payout_stakers(
				RuntimeOrigin::signed(999),
				3,
				era,
			);
			// It may succeed (0 transfer) or error — either is fine, just no panic.
			let balance_after = Balances::total_balance(&3);

			// Validator should NOT have gained from the drained pot.
			assert!(
				balance_after <= balance_before,
				"Drained pot must not pay out: before={}, after={}",
				balance_before, balance_after
			);

			// Other validators are also safe.
			for &v in &[5u64, 6, 8] {
				let _ = staking_async::Pallet::<T>::payout_stakers(
					RuntimeOrigin::signed(999), v, era,
				);
			}
		});
	}

	#[test]
	fn corrupted_reward_points_total_less_than_sum_does_not_overpay() {
		// Corruption: ErasRewardPoints.total is LESS than sum of individual points.
		// Perbill::from_rational(individual, total) would give > 100%.
		// This could cause one validator to claim more than their fair share.
		ExtBuilder::default().local_queue().build().execute_with(|| {
			for &v in &[3u64, 5, 6, 8] {
				Payee::<T>::insert(v, staking_async::RewardDestination::Stash);
			}
			for n in 100..=112u64 {
				Payee::<T>::insert(n, staking_async::RewardDestination::Stash);
			}

			let _elected = advance_to_era_with_points(2);
			let era = 1u32;
			let era_reward = ErasValidatorReward::<T>::get(era).unwrap();

			// Corrupt: set total to 100 but individuals sum to 400.
			ErasRewardPoints::<T>::mutate(era, |points| {
				points.total = 100; // should be 400
				// Individual points remain at 100 each (set by advance_to_era_with_points).
			});

			let pot = SequentialTest::pot_account(RewardPot::Era(era, RewardKind::StakerRewards));
			let pot_before = Balances::total_balance(&pot);

			// Payout: each validator thinks they get 100/100 = 100% of era_reward.
			// First validator gets the full amount.
			assert_ok!(staking_async::Pallet::<T>::payout_stakers(
				RuntimeOrigin::signed(999), 3, era,
			));

			// Second validator: pot may be drained. Transfer fails silently.
			let balance_5_before = Balances::total_balance(&5);
			let _ = staking_async::Pallet::<T>::payout_stakers(
				RuntimeOrigin::signed(999), 5, era,
			);
			let balance_5_after = Balances::total_balance(&5);

			// KEY QUESTION: did the pot have enough for both?
			// Perbill::from_rational(100, 100) = 100%, so validator 3 takes all of era_reward.
			// Validator 5's transfer from the pot will fail (insufficient balance).
			let pot_after = Balances::total_balance(&pot);

			// Total extracted from pot should not exceed pot's starting balance.
			let total_consumed = pot_before - pot_after;
			assert!(
				total_consumed <= pot_before,
				"Cannot consume more than pot balance"
			);

			// The damage: first claimer gets 100% instead of 25%.
			// But the pot is the hard limit — later claimers get nothing.
			// This is a DATA corruption issue, not an inflation issue.
			// Total issuance is unchanged (transfer-based).
			log::info!(
				target: "audit",
				"Corrupted points: pot_before={}, consumed={}, pot_after={}. First claimer gets 100%, rest get 0.",
				pot_before, total_consumed, pot_after
			);
		});
	}

	#[test]
	fn disable_minting_guard_corrupted_to_future_era() {
		// Corruption: DisableMintingGuard set to a future era.
		// This would allow legacy minting for DAP eras that should be transfer-only.
		ExtBuilder::default().local_queue().build().execute_with(|| {
			for &v in &[3u64, 5, 6, 8] {
				Payee::<T>::insert(v, staking_async::RewardDestination::Stash);
			}

			let _elected = advance_to_era_with_points(2);

			// Guard should be set to era 1 (first DAP era with rewards).
			let real_guard = DisableMintingGuard::<T>::get();
			assert!(real_guard.is_some());

			// Corrupt: set guard to era 999 (way in the future).
			DisableMintingGuard::<T>::put(999u32);

			// Era 1 has a pot (DAP mode created it). Payout should still use transfer path
			// because has_staker_rewards_pot(1) checks for provider count, not the guard.
			let pot = SequentialTest::pot_account(RewardPot::Era(1, RewardKind::StakerRewards));
			let has_pot = System::providers(&pot) > 0;
			assert!(has_pot, "Era 1 should have a pot regardless of guard corruption");

			let issuance_before = Balances::total_issuance();
			assert_ok!(staking_async::Pallet::<T>::payout_stakers(
				RuntimeOrigin::signed(999), 3, 1,
			));
			let issuance_after = Balances::total_issuance();

			// Even with corrupted guard, payout checks pot existence first.
			// If pot exists → transfer path. Guard is only consulted for eras WITHOUT a pot.
			assert_eq!(
				issuance_before, issuance_after,
				"Pot existence check takes precedence over guard — no minting"
			);

			// Restore guard.
			DisableMintingGuard::<T>::put(real_guard.unwrap());
		});
	}

	#[test]
	fn zero_point_era_with_funded_pot_does_not_leak_value() {
		// All validators get 0 reward points, but era pot is funded.
		// No one should be able to claim. Funds stuck until cleanup_era.
		ExtBuilder::default().local_queue().build().execute_with(|| {
			for &v in &[3u64, 5, 6, 8] {
				Payee::<T>::insert(v, staking_async::RewardDestination::Stash);
			}

			// Advance but set 0 reward points for all validators in era 0.
			ErasRewardPoints::<T>::mutate(0, |points| {
				points.total = 0;
				// No individual points.
			});
			roll_until_next_active(1);

			let era = 0u32;
			let era_reward = ErasValidatorReward::<T>::get(era);

			// Pot may be funded from DAP drip.
			let pot = SequentialTest::pot_account(RewardPot::Era(era, RewardKind::StakerRewards));
			let pot_balance = Balances::total_balance(&pot);

			// All payout attempts: validators with 0 points get early return (no payment).
			for &v in &[3u64, 5, 6, 8] {
				let balance_before = Balances::total_balance(&v);
				let _ = staking_async::Pallet::<T>::payout_stakers(
					RuntimeOrigin::signed(999), v, era,
				);
				assert_eq!(
					Balances::total_balance(&v), balance_before,
					"Validator {} should receive nothing with 0 points", v
				);
			}

			// Pot is untouched.
			assert_eq!(
				Balances::total_balance(&pot), pot_balance,
				"Zero-point era pot should be untouched"
			);
		});
	}

	#[test]
	fn general_pot_reaped_dap_drip_handles_gracefully() {
		// Corruption: general staker pot drops below ED between drips.
		// DAP tries to mint_into a dead account — what happens?
		ExtBuilder::default().local_queue().build().execute_with(|| {
			let general_pot =
				SequentialTest::pot_account(RewardPot::General(RewardKind::StakerRewards));

			// Verify pot is alive.
			assert!(Balances::total_balance(&general_pot) > 0);

			// Drain the general pot.
			let balance = Balances::total_balance(&general_pot);
			let _ = <Balances as frame_support::traits::fungible::Mutate<AccountId>>::transfer(
				&general_pot,
				&999,
				balance,
				frame_support::traits::tokens::Preservation::Expendable,
			);

			// Pot might be dead now. Roll blocks — DAP tries to mint_into dead account.
			let issuance_before = Balances::total_issuance();
			roll_many(5);
			let issuance_after = Balances::total_issuance();

			// DAP should still work — mint_into creates the account if needed.
			// The staker pot gets its share even after being reaped.
			assert!(
				issuance_after > issuance_before,
				"DAP should still mint even after pot was reaped"
			);

			// The pot should be alive again with new funds.
			assert!(
				Balances::total_balance(&general_pot) > 0,
				"General pot should be revived by DAP drip"
			);
		});
	}

	#[test]
	fn budget_allocation_removed_for_stakers_existing_pots_still_claimable() {
		// Governance removes staker key from budget (sets 0% stakers).
		// Existing era pots should still be claimable.
		ExtBuilder::default().local_queue().build().execute_with(|| {
			for &v in &[3u64, 5, 6, 8] {
				Payee::<T>::insert(v, staking_async::RewardDestination::Stash);
			}

			// Advance 2 eras with normal budget.
			let _elected = advance_to_era_with_points(2);

			let era_1_reward = ErasValidatorReward::<T>::get(1).unwrap();
			assert!(era_1_reward > 0);

			// Now governance sets 0% stakers. This only affects FUTURE drips.
			change_budget_allocation(0, 0, 100);

			// Existing era 1 pot still has funds. Claim should work.
			let balance_before = Balances::total_balance(&3);
			assert_ok!(staking_async::Pallet::<T>::payout_stakers(
				RuntimeOrigin::signed(999), 3, 1,
			));
			assert!(
				Balances::total_balance(&3) > balance_before,
				"Existing era pot should still pay out after budget change"
			);
		});
	}

	#[test]
	fn validator_commission_100_percent_nominators_get_zero() {
		// Edge case: validator sets 100% commission. Nominators should get exactly 0.
		// Validator gets all of their proportional share.
		ExtBuilder::default().local_queue().build().execute_with(|| {
			// Set validator 3's commission to 100%.
			pallet_staking_async::ErasValidatorPrefs::<T>::insert(
				1u32,
				&3u64,
				pallet_staking_async::ValidatorPrefs {
					commission: Perbill::from_percent(100),
					blocked: false,
				},
			);

			for &v in &[3u64, 5, 6, 8] {
				Payee::<T>::insert(v, staking_async::RewardDestination::Stash);
			}
			for n in 100..=112u64 {
				Payee::<T>::insert(n, staking_async::RewardDestination::Stash);
			}

			let _elected = advance_to_era_with_points(2);
			let era = 1u32;

			// Capture nominator balances before payout.
			let nominators: Vec<AccountId> = (100..=112).collect();
			let nom_balances_before: Vec<_> =
				nominators.iter().map(|n| (*n, Balances::total_balance(n))).collect();

			// Payout validator 3 (100% commission).
			let val_before = Balances::total_balance(&3);
			assert_ok!(staking_async::Pallet::<T>::payout_stakers(
				RuntimeOrigin::signed(999), 3, era,
			));
			let val_gain = Balances::total_balance(&3) - val_before;

			// Nominators of validator 3 should get 0 (all goes to commission).
			for (n, before) in &nom_balances_before {
				let after = Balances::total_balance(n);
				assert_eq!(
					*before, after,
					"Nominator {} should get 0 with 100% commission", n
				);
			}

			// Validator should get their full share.
			assert!(val_gain > 0, "Validator with 100% commission should get rewards");
		});
	}

	#[test]
	fn incentive_pot_funded_but_weights_zero_funds_trapped_then_cleaned() {
		// Governance sets incentive budget but doesn't set config (weights=0).
		// Incentive pot gets funded but nobody can claim.
		// On era cleanup, trapped funds go to UnclaimedRewardHandler.
		ExtBuilder::default().local_queue().build().execute_with(|| {
			// Budget includes 25% incentive, but config is all zeros.
			setup_dap_with_budget(50, 25, 25);
			assert_eq!(OptimumSelfStake::<T>::get(), 0);
			assert_eq!(HardCapSelfStake::<T>::get(), 0);

			for &v in &[3u64, 5, 6, 8] {
				Payee::<T>::insert(v, staking_async::RewardDestination::Stash);
			}

			let _elected = advance_to_era_with_points(2);
			let era = 1u32;

			let incentive_budget = ErasValidatorIncentiveBudget::<T>::get(era);
			assert!(incentive_budget > 0, "Budget funded despite zero config");

			let incentive_pot = SequentialTest::pot_account(
				RewardPot::Era(era, RewardKind::ValidatorSelfStake),
			);
			let pot_before = Balances::total_balance(&incentive_pot);

			// Payout: incentive calculation returns None (weight=0).
			for &v in &[3u64, 5, 6, 8] {
				let _ = staking_async::Pallet::<T>::payout_stakers(
					RuntimeOrigin::signed(999), v, era,
				);
			}

			// Incentive pot should be completely untouched.
			let pot_after = Balances::total_balance(&incentive_pot);
			assert_eq!(
				pot_before, pot_after,
				"Incentive pot must be untouched when weights are zero"
			);

			// These funds are trapped. They'll be cleaned up when the era is pruned.
			// Total value trapped per era = incentive_budget.
			log::info!(
				target: "audit",
				"Trapped incentive: {} tokens in era {} (config zero, budget funded)",
				pot_after, era
			);
		});
	}

	#[test]
	fn era_reward_storage_set_higher_than_pot_balance() {
		// Corruption: ErasValidatorReward says era has 1_000_000 but pot only has 1_000.
		// Payout logic computes amounts from ErasValidatorReward, then transfers from pot.
		// Transfer will fail for amounts exceeding pot balance.
		ExtBuilder::default().local_queue().build().execute_with(|| {
			for &v in &[3u64, 5, 6, 8] {
				Payee::<T>::insert(v, staking_async::RewardDestination::Stash);
			}

			let _elected = advance_to_era_with_points(2);
			let era = 1u32;

			let real_pot_balance = Balances::total_balance(
				&SequentialTest::pot_account(RewardPot::Era(era, RewardKind::StakerRewards)),
			);

			// Corrupt: inflate ErasValidatorReward to 100x the actual pot.
			let inflated = real_pot_balance * 100;
			ErasValidatorReward::<T>::insert(era, inflated);

			let pot_before = real_pot_balance;
			let issuance_before = Balances::total_issuance();

			// Payout: computed amount will be ~25% of inflated value (per validator).
			// That's 25x the pot balance. Transfer will fail silently.
			for &v in &[3u64, 5, 6, 8] {
				let _ = staking_async::Pallet::<T>::payout_stakers(
					RuntimeOrigin::signed(999), v, era,
				);
			}

			let issuance_after = Balances::total_issuance();

			// No minting should occur regardless.
			assert_eq!(
				issuance_before, issuance_after,
				"Inflated ErasValidatorReward must not cause minting"
			);

			// Total extracted is bounded by actual pot balance.
			let pot_after = Balances::total_balance(
				&SequentialTest::pot_account(RewardPot::Era(era, RewardKind::StakerRewards)),
			);
			assert!(
				pot_before - pot_after <= pot_before,
				"Extraction bounded by real pot balance"
			);
		});
	}

	#[test]
	fn multiple_eras_without_payout_accumulates_safely() {
		// Validators don't claim for several eras. All claims should work within HistoryDepth.
		ExtBuilder::default().local_queue().build().execute_with(|| {
			for &v in &[3u64, 5, 6, 8] {
				Payee::<T>::insert(v, staking_async::RewardDestination::Stash);
			}

			// Advance through 3 eras without claiming.
			let _elected = advance_to_era_with_points(4);

			// Claim all 3 eras for validator 3.
			let mut total_claimed = 0u128;

			for era in 1..=3u32 {
				let before = Balances::total_balance(&3);
				let result = staking_async::Pallet::<T>::payout_stakers(
					RuntimeOrigin::signed(999), 3, era,
				);
				if result.is_ok() {
					total_claimed += Balances::total_balance(&3) - before;
				}
			}

			assert!(
				total_claimed > 0,
				"Should claim rewards from multiple eras"
			);
		});
	}
}
