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

/// Manager for era reward pot accounts.
///
/// Handles the lifecycle of era pot accounts from creation to cleanup:
/// - Creates pot accounts with provider references to prevent premature reaping
/// - Manages funding through the reward provider
/// - Cleans up old pots by transferring unclaimed rewards and removing providers
// CLAUDE: can we name this better? Like it doesn't have to be reward pot, but reward
// manager or something, and pot details should be abstracted as much as possible.
pub struct EraRewardPots<T: Config>(core::marker::PhantomData<T>);

impl<T: Config> EraRewardPots<T> {
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

	/// Cleans up all pot accounts for a given era.
	///
	/// Calls [`Self::destroy`] for both staker rewards and validator incentive pots.
	pub fn cleanup_era(era: EraIndex) {
		Self::destroy(era, EraPotType::StakerRewards);
		Self::destroy(era, EraPotType::ValidatorSelfStake);
	}
}
