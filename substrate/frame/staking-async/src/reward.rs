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
use frame_support::traits::{
	fungible::{Inspect, Mutate},
	tokens::Preservation,
	Defensive,
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
	pub fn create(era: EraIndex, pot_type: EraPotType) -> T::AccountId {
		let pot_account = T::EraPotAccountProvider::era_pot_account(era, pot_type);
		frame_system::Pallet::<T>::inc_providers(&pot_account);
		pot_account
	}

	/// Allocates era rewards by creating pots and asking the reward provider to mint into them.
	///
	/// Creates both staker and validator incentive pots, then calls the configured
	/// reward provider to compute inflation and mint rewards into the respective pots.
	///
	/// # Returns
	/// The allocation breakdown showing amounts minted into each pot.
	pub fn allocate_rewards(
		era: EraIndex,
		total_staked: BalanceOf<T>,
		era_duration_millis: u64,
	) -> sp_staking::EraRewardAllocation<BalanceOf<T>> {
		// Create both pot accounts
		let staker_pot = Self::create(era, EraPotType::StakerRewards);
		let validator_incentive_pot = Self::create(era, EraPotType::ValidatorSelfStake);

		// Ask reward provider to mint and allocate
		T::RewardProvider::allocate_era_rewards(
			era,
			total_staked,
			era_duration_millis,
			&staker_pot,
			&validator_incentive_pot,
		)
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
	pub fn destroy(era: EraIndex, pot_type: EraPotType) {
		let pot_account = T::EraPotAccountProvider::era_pot_account(era, pot_type);

		// Get remaining balance in pot
		let remaining = T::Currency::balance(&pot_account);

		// Transfer any remaining funds to unclaimed reward sink
		if !remaining.is_zero() {
			let sink = T::UnclaimedRewardSink::unclaimed_reward_sink();
			let _ = T::Currency::transfer(&pot_account, &sink, remaining, Preservation::Expendable)
				.defensive();
			log::debug!(
				target: crate::LOG_TARGET,
				"Transferred {:?} unclaimed rewards from era {:?} {:?} pot to sink",
				remaining,
				era,
				pot_type
			);
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
	pub fn has_staker_rewards_pot(era: EraIndex) -> bool {
		let staker_rewards_pot =
			T::EraPotAccountProvider::era_pot_account(era, EraPotType::StakerRewards);
		frame_system::Pallet::<T>::providers(&staker_rewards_pot) > 0
	}

	/// Cleans up all pot accounts for a given era.
	///
	/// Calls [`Self::destroy`] for both staker rewards and validator incentive pots.
	pub fn cleanup_era(era: EraIndex) {
		Self::destroy(era, EraPotType::StakerRewards);
		Self::destroy(era, EraPotType::ValidatorSelfStake);
	}
}

/// Default implementation of the staker reward calculator.
///
/// Implements:
/// - Sqrt-based piecewise reward curve for validator self-stake incentives
/// - Standard staking reward distribution (commission + proportional stake split)
pub struct DefaultStakerRewardCalculator;

impl<AccountId, Balance> sp_staking::StakerRewardCalculator<AccountId, Balance>
	for DefaultStakerRewardCalculator
where
	AccountId: Clone,
	Balance: sp_runtime::traits::AtLeast32BitUnsigned + Copy + Into<u128> + From<u128>,
{
	fn calculate_validator_incentive_weight(
		self_stake: Balance,
		optimum: Balance,
		cap: Balance,
		slope_factor: Perbill,
	) -> Balance {
		// If self-stake is zero, return zero weight
		if self_stake.is_zero() {
			return Balance::zero();
		}

		// If config is not set (both optimum and cap are zero), return zero weight
		if optimum.is_zero() && cap.is_zero() {
			return Balance::zero();
		}

		// Convert to u128 for sqrt calculation
		let self_stake_u128: u128 = self_stake.into();
		let optimum_u128: u128 = optimum.into();
		let cap_u128: u128 = cap.into();

		// Calculate weight based on piecewise function
		let weight_u128 = if self_stake <= optimum {
			// Below optimum: w(s) = √s
			sp_arithmetic::helpers_128bit::sqrt(self_stake_u128)
		} else if self_stake <= cap {
			// Between optimum and cap: w(s) = √(T + k² × (s - T))
			let k_squared = slope_factor.square();
			let excess = self_stake_u128.saturating_sub(optimum_u128);
			let k_squared_times_excess = k_squared.mul_floor(excess);
			let arg = optimum_u128.saturating_add(k_squared_times_excess);
			sp_arithmetic::helpers_128bit::sqrt(arg)
		} else {
			// Above cap: w(s) = √(T + k² × (C - T)) (plateau)
			let k_squared = slope_factor.square();
			let excess = cap_u128.saturating_sub(optimum_u128);
			let k_squared_times_excess = k_squared.mul_floor(excess);
			let arg = optimum_u128.saturating_add(k_squared_times_excess);
			sp_arithmetic::helpers_128bit::sqrt(arg)
		};

		// Convert back to Balance
		Balance::from(weight_u128)
	}

	fn calculate_staker_reward(
		validator_total_reward: Balance,
		validator_commission: Perbill,
		validator_own_stake: Balance,
		total_stake: Balance,
	) -> sp_staking::StakerRewardResult<Balance> {
		// Calculate total commission the validator takes
		let validator_commission_payout = validator_commission.mul_floor(validator_total_reward);

		// Calculate leftover after commission
		let leftover = validator_total_reward.saturating_sub(validator_commission_payout);

		// Calculate validator's staking payout (their share of the leftover based on own stake)
		let validator_exposure_part = Perbill::from_rational(validator_own_stake, total_stake);
		let validator_staking_payout = validator_exposure_part.mul_floor(leftover);

		// Calculate total validator payout (staking + commission)
		let validator_payout = validator_staking_payout.saturating_add(validator_commission_payout);

		// Calculate total nominator payout as remainder to avoid double rounding
		// This ensures validator_payout + nominator_payout = commission + leftover exactly
		let nominator_payout = leftover.saturating_sub(validator_staking_payout);

		sp_staking::StakerRewardResult { validator_payout, nominator_payout }
	}
}
