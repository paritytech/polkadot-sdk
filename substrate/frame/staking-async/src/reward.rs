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

//! Era reward management.
//!
//! This module provides utilities for managing the lifecycle of era reward pot accounts,
//! including creation, funding, and cleanup.

use crate::*;
use frame_support::{
	defensive,
	traits::{
		fungible::{Inspect, Mutate},
		tokens::Preservation,
		Defensive,
	},
};
use sp_runtime::traits::Zero;
use sp_staking::{EraIndex, UnclaimedRewardSink};

/// Manager for era reward allocation and distribution.
///
/// Handles the lifecycle of era rewards from creation to cleanup:
/// - Creates reward pot accounts with provider references to prevent premature reaping
/// - Manages funding through the reward provider
/// - Cleans up old pots by transferring unclaimed rewards and removing providers
pub struct EraRewardManager<T: Config>(core::marker::PhantomData<T>);

impl<T: Config> EraRewardManager<T> {
	/// Creates and initializes an era pot account by adding a provider reference.
	///
	/// This must be called when creating a new era pot to prevent the account from being
	/// reaped prematurely. The provider will be removed during cleanup via [`Self::destroy`].
	///
	/// # Returns
	/// The account ID of the created pot.
	pub(crate) fn create(era: EraIndex, pot_type: EraPotType) -> T::AccountId {
		let pot_account = T::EraPotAccountProvider::era_pot_account(era, pot_type);
		frame_system::Pallet::<T>::inc_providers(&pot_account);
		pot_account
	}

	/// Snapshots the general reward pots and transfers their balances into era-specific pots.
	///
	/// DAP drips inflation continuously into the general pots. At era boundary, this method:
	/// 1. Creates era-specific pot accounts
	/// 2. Reads balance from general pots (accumulated since last era)
	/// 3. Transfers from general → era-specific pots
	///
	/// # Note on general pot account lifecycle
	/// General pot accounts do not have explicit provider references. They are kept alive by
	/// their balance: the first DAP inflation drip (which exceeds ED) creates the account, and
	/// subsequent snapshots use `Preservation::Preserve` to keep ED in the account. Someone (a
	/// runtime maintainer) can also send ED to the general pot account to ensure they are created
	/// before mint.
	///
	/// # Returns
	/// The allocation breakdown showing amounts transferred into each era pot.
	pub(crate) fn snapshot_era_rewards(
		era: EraIndex,
	) -> sp_staking::EraRewardAllocation<BalanceOf<T>> {
		let staker_era_pot = Self::create(era, EraPotType::StakerRewards);
		let incentive_era_pot = Self::create(era, EraPotType::ValidatorSelfStake);

		// Read accumulated balances from general pots.
		let general_staker_pot = T::GeneralPots::general_pot_account(GeneralPotType::StakerRewards);
		let general_incentive_pot =
			T::GeneralPots::general_pot_account(GeneralPotType::ValidatorIncentive);
		// we want to leave ED in the general pot accounts to keep them alive.
		let staker_balance = T::Currency::reducible_balance(
			&general_staker_pot,
			Preservation::Preserve,
			frame_support::traits::tokens::Fortitude::Polite,
		);
		let incentive_balance = T::Currency::reducible_balance(
			&general_incentive_pot,
			Preservation::Preserve,
			frame_support::traits::tokens::Fortitude::Polite,
		);

		// Transfer from general pots to era-specific pots, keeping general pots alive.
		// Track actual transferred amounts — if a transfer fails, we must not report
		// the intended amount as available in the era pot.
		let actual_staker = if !staker_balance.is_zero() {
			match T::Currency::transfer(
				&general_staker_pot,
				&staker_era_pot,
				staker_balance,
				Preservation::Preserve,
			) {
				Ok(_) => staker_balance,
				Err(e) => {
					log!(error, "Era {:?}: staker reward transfer failed: {:?}", era, e);
					defensive!("Failed to transfer staker rewards to era pot");
					Zero::zero()
				},
			}
		} else {
			Zero::zero()
		};

		let actual_incentive = if !incentive_balance.is_zero() {
			match T::Currency::transfer(
				&general_incentive_pot,
				&incentive_era_pot,
				incentive_balance,
				Preservation::Preserve,
			) {
				Ok(_) => incentive_balance,
				Err(e) => {
					log!(error, "Era {:?}: validator incentive transfer failed: {:?}", era, e);
					defensive!("Failed to transfer validator incentive to era pot");
					Zero::zero()
				},
			}
		} else {
			Zero::zero()
		};

		log::info!(
			target: LOG_TARGET,
			"Era {era}: snapshotted staker_rewards={actual_staker:?}, \
			 validator_incentive={actual_incentive:?}"
		);

		sp_staking::EraRewardAllocation {
			staker_rewards: actual_staker,
			validator_incentive: actual_incentive,
		}
	}

	/// Destroys an era pot account by transferring out unclaimed rewards and removing the provider.
	///
	/// Transfers any remaining balance to the unclaimed reward sink, then decrements the provider
	/// to allow the account to be reaped.
	///
	/// This unconditionally:
	/// 1. Transfers out all balance (unclaimed rewards)
	/// 2. Decrements exactly one provider reference
	///
	/// The symmetric operation to [`Self::create`].
	pub(crate) fn destroy(era: EraIndex, pot_type: EraPotType) {
		let pot_account = T::EraPotAccountProvider::era_pot_account(era, pot_type);

		// Get remaining balance in pot
		let remaining = T::Currency::balance(&pot_account);

		// Transfer any remaining funds to unclaimed reward sink
		if !remaining.is_zero() {
			match T::UnclaimedRewardSink::deposit(&pot_account, remaining) {
				Ok(_) => {
					log::debug!(
						target: crate::LOG_TARGET,
						"Transferred {:?} unclaimed rewards from era {:?} {:?} pot to sink",
						remaining,
						era,
						pot_type
					);
				},
				Err(e) => {
					defensive!("Failed to transfer unclaimed rewards to sink");
					log::error!(
						target: crate::LOG_TARGET,
						"Era {:?} {:?}: unclaimed reward transfer failed: {:?}",
						era,
						pot_type,
						e
					);
				},
			}
		}

		// Decrement provider to allow account to be reaped.
		let _ = frame_system::Pallet::<T>::dec_providers(&pot_account)
			.defensive_proof("Provider was added in Self::create; qed");

		log::debug!(
			target: crate::LOG_TARGET,
			"✅ Cleaned up era {:?} {:?} pot account (removed provider)",
			era,
			pot_type
		);
	}

	/// Checks if an era has a staker rewards pot by checking if the account has providers.
	///
	/// Returns true if the pot exists (has providers), false otherwise.
	pub(crate) fn has_staker_rewards_pot(era: EraIndex) -> bool {
		let staker_rewards_pot =
			T::EraPotAccountProvider::era_pot_account(era, EraPotType::StakerRewards);
		frame_system::Pallet::<T>::providers(&staker_rewards_pot) > 0
	}

	/// Cleans up all pot accounts for a given era.
	///
	/// Calls [`Self::destroy`] for both staker rewards and validator incentive pots.
	pub(crate) fn cleanup_era(era: EraIndex) {
		Self::destroy(era, EraPotType::StakerRewards);
		Self::destroy(era, EraPotType::ValidatorSelfStake);
	}
}

/// Default implementation of the staker reward calculator.
///
/// Implements:
/// - Sqrt-based piecewise reward curve for validator self-stake incentives
/// - Standard staking reward distribution (commission + proportional stake split)
pub struct DefaultStakerRewardCalculator<T>(core::marker::PhantomData<T>);

impl<T: Config> sp_staking::StakerRewardCalculator<T::AccountId, BalanceOf<T>>
	for DefaultStakerRewardCalculator<T>
where
	BalanceOf<T>: Into<u128> + From<u128>,
{
	fn calculate_validator_incentive_weight(self_stake: BalanceOf<T>) -> BalanceOf<T> {
		let optimum = OptimumSelfStake::<T>::get();
		let cap = HardCapSelfStake::<T>::get();
		let slope_factor = SelfStakeSlopeFactor::<T>::get();

		incentive_weight::<BalanceOf<T>>(self_stake, optimum, cap, slope_factor)
	}

	fn calculate_staker_reward(
		validator_total_reward: BalanceOf<T>,
		validator_commission: Perbill,
		validator_own_stake: BalanceOf<T>,
		total_stake: BalanceOf<T>,
	) -> sp_staking::StakerRewardResult<BalanceOf<T>> {
		let validator_commission_payout = validator_commission.mul_floor(validator_total_reward);
		let leftover = validator_total_reward.saturating_sub(validator_commission_payout);
		let validator_exposure_part = Perbill::from_rational(validator_own_stake, total_stake);
		let validator_staking_payout = validator_exposure_part.mul_floor(leftover);
		let validator_payout = validator_staking_payout.saturating_add(validator_commission_payout);
		// Nominator payout as remainder to avoid double rounding.
		let nominator_payout = leftover.saturating_sub(validator_staking_payout);

		sp_staking::StakerRewardResult { validator_payout, nominator_payout }
	}
}

/// Piecewise sqrt-based incentive weight function.
///
/// - Below optimum: `w(s) = √s`
/// - Between optimum and cap: `w(s) = √(T + k² × (s - T))`
/// - Above cap: plateau at `w(cap)`
fn incentive_weight<Balance>(
	self_stake: Balance,
	optimum: Balance,
	cap: Balance,
	slope_factor: Perbill,
) -> Balance
where
	Balance: AtLeast32BitUnsigned + Copy + Into<u128> + From<u128>,
{
	if self_stake.is_zero() {
		return Balance::zero();
	}

	if optimum.is_zero() && cap.is_zero() {
		return Balance::zero();
	}

	let self_stake_u128: u128 = self_stake.into();
	let optimum_u128: u128 = optimum.into();
	let cap_u128: u128 = cap.into();

	let weight_u128 = if self_stake <= optimum {
		sp_arithmetic::helpers_128bit::sqrt(self_stake_u128)
	} else if self_stake <= cap {
		let k_squared = slope_factor.square();
		let excess = self_stake_u128.saturating_sub(optimum_u128);
		let arg = optimum_u128.saturating_add(k_squared.mul_floor(excess));
		sp_arithmetic::helpers_128bit::sqrt(arg)
	} else {
		let k_squared = slope_factor.square();
		let excess = cap_u128.saturating_sub(optimum_u128);
		let arg = optimum_u128.saturating_add(k_squared.mul_floor(excess));
		sp_arithmetic::helpers_128bit::sqrt(arg)
	};

	Balance::from(weight_u128)
}

#[cfg(test)]
mod tests {
	use super::*;
	use sp_runtime::Perbill;

	type Balance = u128;

	fn calculate_weight(
		self_stake: Balance,
		optimum: Balance,
		cap: Balance,
		slope_factor: Perbill,
	) -> Balance {
		incentive_weight(self_stake, optimum, cap, slope_factor)
	}

	#[test]
	fn weight_calculation_zero_self_stake() {
		// GIVEN: Zero self-stake
		let self_stake: Balance = 0;
		let optimum: Balance = 100_000;
		let cap: Balance = 500_000;
		let slope_factor = Perbill::from_rational(1u32, 2u32);

		// WHEN/THEN
		assert_eq!(calculate_weight(self_stake, optimum, cap, slope_factor), 0);
	}

	#[test]
	fn weight_calculation_config_not_set() {
		// GIVEN: Config not set (both optimum and cap are zero)
		let self_stake: Balance = 100_000;
		let optimum: Balance = 0;
		let cap: Balance = 0;
		let slope_factor = Perbill::from_rational(1u32, 2u32);

		// WHEN/THEN
		assert_eq!(calculate_weight(self_stake, optimum, cap, slope_factor), 0);
	}

	#[test]
	fn weight_calculation_below_optimum() {
		// GIVEN: Self-stake below optimum; w(s) = √s
		let self_stake: Balance = 10_000;
		let optimum: Balance = 100_000;
		let cap: Balance = 500_000;
		let slope_factor = Perbill::from_rational(1u32, 2u32);

		// WHEN/THEN: √10_000 = 100
		assert_eq!(calculate_weight(self_stake, optimum, cap, slope_factor), 100);
	}

	#[test]
	fn weight_calculation_at_optimum() {
		// GIVEN: Self-stake exactly at optimum
		let self_stake: Balance = 100_000;
		let optimum: Balance = 100_000;
		let cap: Balance = 500_000;
		let slope_factor = Perbill::from_rational(1u32, 2u32);

		// WHEN/THEN: √100_000 ≈ 316
		assert_eq!(calculate_weight(self_stake, optimum, cap, slope_factor), 316);
	}

	#[test]
	fn weight_calculation_between_optimum_and_cap() {
		// w(s) = √(T + k² × (s - T)) = √(100k + 0.25 × 200k) = √150k ≈ 387
		let self_stake: Balance = 300_000;
		let optimum: Balance = 100_000;
		let cap: Balance = 500_000;
		let slope_factor = Perbill::from_rational(1u32, 2u32);

		assert_eq!(calculate_weight(self_stake, optimum, cap, slope_factor), 387);
	}

	#[test]
	fn weight_calculation_at_cap() {
		// w(C) = √(T + k² × (C - T)) = √(100k + 0.25 × 400k) = √200k ≈ 447
		let self_stake: Balance = 500_000;
		let optimum: Balance = 100_000;
		let cap: Balance = 500_000;
		let slope_factor = Perbill::from_rational(1u32, 2u32);

		assert_eq!(calculate_weight(self_stake, optimum, cap, slope_factor), 447);
	}

	#[test]
	fn weight_calculation_equal_optimum_and_cap() {
		// GIVEN: Optimum equals cap (edge case)
		let self_stake: Balance = 100_000;
		let optimum: Balance = 100_000;
		let cap: Balance = 100_000;
		let slope_factor = Perbill::from_rational(1u32, 2u32);

		// WHEN/THEN: √100_000 ≈ 316
		assert_eq!(calculate_weight(self_stake, optimum, cap, slope_factor), 316);
	}

	#[test]
	fn weight_calculation_monotonically_increasing_below_cap() {
		// GIVEN: Multiple self-stake values below cap
		let optimum: Balance = 100_000;
		let cap: Balance = 500_000;
		let slope_factor = Perbill::from_rational(1u32, 2u32);

		// WHEN: Calculate weights for increasing self-stakes
		let weight_50k = calculate_weight(50_000, optimum, cap, slope_factor);
		let weight_100k = calculate_weight(100_000, optimum, cap, slope_factor);
		let weight_200k = calculate_weight(200_000, optimum, cap, slope_factor);
		let weight_400k = calculate_weight(400_000, optimum, cap, slope_factor);

		// THEN: Weights should be monotonically increasing
		assert!(weight_50k < weight_100k, "{} < {}", weight_50k, weight_100k);
		assert!(weight_100k < weight_200k, "{} < {}", weight_100k, weight_200k);
		assert!(weight_200k < weight_400k, "{} < {}", weight_200k, weight_400k);
	}

	#[test]
	fn weight_calculation_plateau_above_cap() {
		// GIVEN: Self-stakes above cap
		let optimum: Balance = 100_000;
		let cap: Balance = 500_000;
		let slope_factor = Perbill::from_rational(1u32, 2u32);

		// WHEN: Calculate weights for self-stakes above cap
		let weight_at_cap = calculate_weight(500_000, optimum, cap, slope_factor);
		let weight_above_cap_1 = calculate_weight(1_000_000, optimum, cap, slope_factor);
		let weight_above_cap_2 = calculate_weight(10_000_000, optimum, cap, slope_factor);

		// THEN: All weights above cap should equal weight at cap (plateau)
		assert_eq!(weight_at_cap, weight_above_cap_1);
		assert_eq!(weight_at_cap, weight_above_cap_2);
	}

	#[test]
	fn weight_calculation_very_small_self_stake() {
		// GIVEN: Very small self-stake (1 unit)
		let self_stake: Balance = 1;
		let optimum: Balance = 100_000;
		let cap: Balance = 500_000;
		let slope_factor = Perbill::from_rational(1u32, 2u32);

		// WHEN/THEN: √1 = 1
		assert_eq!(calculate_weight(self_stake, optimum, cap, slope_factor), 1);
	}

	#[test]
	fn weight_calculation_different_slope_factors() {
		// GIVEN: Same self-stake with different slope factors
		let self_stake: Balance = 300_000; // Between optimum and cap
		let optimum: Balance = 100_000;
		let cap: Balance = 500_000;

		// WHEN: Calculate weights with different slope factors
		let weight_k_025 = calculate_weight(
			self_stake,
			optimum,
			cap,
			Perbill::from_rational(1u32, 4u32), // k = 0.25
		);
		let weight_k_050 = calculate_weight(
			self_stake,
			optimum,
			cap,
			Perbill::from_rational(1u32, 2u32), // k = 0.5
		);
		let weight_k_075 = calculate_weight(
			self_stake,
			optimum,
			cap,
			Perbill::from_rational(3u32, 4u32), // k = 0.75
		);

		// THEN: Larger slope factor should result in larger weight
		assert!(weight_k_025 < weight_k_050, "{} < {}", weight_k_025, weight_k_050);
		assert!(weight_k_050 < weight_k_075, "{} < {}", weight_k_050, weight_k_075);
	}
}
