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
//! Manages the lifecycle of era reward pot accounts: creation, funding
//! via snapshot from the general DAP pot, and cleanup of expired eras.

use crate::*;
use frame_support::{
	defensive,
	traits::{
		fungible::{Balanced, Inspect, Mutate},
		tokens::{Fortitude, Precision, Preservation},
		Defensive, OnUnbalanced,
	},
};
use sp_runtime::traits::Zero;
use sp_staking::EraIndex;

/// Manager for era reward pot lifecycle.
pub struct EraRewardManager<T: Config>(core::marker::PhantomData<T>);

impl<T: Config> EraRewardManager<T> {
	/// Creates an era pot account by adding a provider reference.
	///
	/// Should only be called in non-minting mode (`DisableMinting = true`).
	pub(crate) fn create(era: EraIndex, kind: RewardKind) -> T::AccountId {
		debug_assert!(
			T::DisableMinting::get(),
			"Era pots should only be created when DisableMinting is true"
		);
		let pot_account = T::RewardPots::pot_account(RewardPot::Era(era, kind));
		frame_system::Pallet::<T>::inc_providers(&pot_account);
		pot_account
	}

	/// Snapshots the general staker reward pot into an era-specific pot.
	///
	/// DAP drips inflation continuously into the general pot. At era boundary,
	/// this transfers the accumulated balance (minus ED) into an era pot.
	pub(crate) fn snapshot_era_rewards(era: EraIndex) -> BalanceOf<T> {
		let staker_era_pot = Self::create(era, RewardKind::StakerRewards);

		let general_staker_pot =
			T::RewardPots::pot_account(RewardPot::General(RewardKind::StakerRewards));

		// Leave ED in the general pot to keep it alive.
		let staker_balance = T::Currency::reducible_balance(
			&general_staker_pot,
			Preservation::Preserve,
			Fortitude::Polite,
		);

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

		log!(info, "Era {:?}: snapshotted staker_rewards={:?}", era, actual_staker);

		actual_staker
	}

	/// Destroys an era pot by withdrawing unclaimed rewards and removing the provider.
	///
	/// No-op if the pot was never created (e.g. in legacy minting mode).
	pub(crate) fn destroy(era: EraIndex, kind: RewardKind) {
		let pot_account = T::RewardPots::pot_account(RewardPot::Era(era, kind));

		// Skip if pot was never created (legacy mode doesn't create pots).
		if frame_system::Pallet::<T>::providers(&pot_account) == 0 {
			return;
		}

		let remaining = T::Currency::balance(&pot_account);

		if !remaining.is_zero() {
			match T::Currency::withdraw(
				&pot_account,
				remaining,
				Precision::BestEffort,
				Preservation::Expendable,
				Fortitude::Force,
			) {
				Ok(credit) => {
					T::UnclaimedRewardHandler::on_unbalanced(credit);
					log!(
						debug,
						"Withdrew {:?} unclaimed rewards from era {:?} {:?} pot",
						remaining,
						era,
						kind
					);
				},
				Err(e) => {
					defensive!("Failed to withdraw unclaimed rewards from era pot");
					log!(
						error,
						"Era {:?} {:?}: unclaimed reward withdrawal failed: {:?}",
						era,
						kind,
						e
					);
				},
			}
		}

		let _ = frame_system::Pallet::<T>::dec_providers(&pot_account)
			.defensive_proof("Provider was added in Self::create; qed");
	}

	/// Checks if an era has a staker rewards pot.
	pub(crate) fn has_staker_rewards_pot(era: EraIndex) -> bool {
		let pot = T::RewardPots::pot_account(RewardPot::Era(era, RewardKind::StakerRewards));
		frame_system::Pallet::<T>::providers(&pot) > 0
	}

	/// Cleans up pot accounts for a given era.
	pub(crate) fn cleanup_era(era: EraIndex) {
		Self::destroy(era, RewardKind::StakerRewards);
	}
}

/// Default implementation of the staker reward calculator.
///
/// Commission-based split: validator gets commission + proportional stake share,
/// nominators get the rest. Incentive weight returns zero (no incentive curve).
pub struct DefaultStakerRewardCalculator<T>(core::marker::PhantomData<T>);

impl<T: Config> sp_staking::StakerRewardCalculator<BalanceOf<T>>
	for DefaultStakerRewardCalculator<T>
where
	BalanceOf<T>: Into<u128> + From<u128>,
{
	fn calculate_validator_incentive_weight(_self_stake: BalanceOf<T>) -> BalanceOf<T> {
		Zero::zero()
	}

	fn calculate_staker_reward(
		validator_total_reward: BalanceOf<T>,
		validator_commission: Perbill,
		validator_own_stake: BalanceOf<T>,
		total_exposure: BalanceOf<T>,
	) -> sp_staking::StakerRewardResult<BalanceOf<T>> {
		let validator_commission_payout = validator_commission.mul_floor(validator_total_reward);
		let leftover = validator_total_reward.saturating_sub(validator_commission_payout);
		let validator_exposure_part = Perbill::from_rational(validator_own_stake, total_exposure);
		let validator_staking_payout = validator_exposure_part.mul_floor(leftover);
		let validator_payout = validator_staking_payout.saturating_add(validator_commission_payout);
		let nominator_payout = leftover.saturating_sub(validator_staking_payout);

		// Validator and nominator payout is exactly same as total reward.
		debug_assert_eq!(validator_payout + nominator_payout, validator_total_reward);

		sp_staking::StakerRewardResult { validator_payout, nominator_payout }
	}
}
