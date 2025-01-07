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
//! Helper functions for OPF pallet.

pub use super::*;
impl<T: Config> Pallet<T> {
	pub fn pot_account() -> AccountIdOf<T> {
		// Get Pot account
		T::PotId::get().into_account_truncating()
	}

	/// Funds transfer from the Pot to a project account
	pub fn spend(amount: BalanceOf<T>, beneficiary: AccountIdOf<T>) -> DispatchResult {
		// Get Pot account
		let pot_account: AccountIdOf<T> = Self::pot_account();

		//Operate the transfer
		T::NativeBalance::transfer(&pot_account, &beneficiary, amount, Preservation::Preserve)?;

		Ok(())
	}

	/// Series of checks on the Pot, to ensure that we have enough funds
	/// before executing a Spend --> used in tests.
	pub fn pot_check(spend: BalanceOf<T>) -> DispatchResult {
		// Get Pot account
		let pot_account = Self::pot_account();

		// Check that the Pot as enough funds for the transfer
		let balance = T::NativeBalance::balance(&pot_account);
		let minimum_balance = T::NativeBalance::minimum_balance();
		let remaining_balance = balance.saturating_sub(spend);

		ensure!(remaining_balance > minimum_balance, Error::<T>::InsufficientPotReserves);
		ensure!(balance > spend, Error::<T>::InsufficientPotReserves);
		Ok(())
	}

	// Voting Period checks
	pub fn period_check() -> DispatchResult {
		// Get current voting round & check if we are in voting period or not
		let current_round_index = NextVotingRoundNumber::<T>::get().saturating_sub(1);
		let round = VotingRounds::<T>::get(current_round_index).ok_or(Error::<T>::NoRoundFound)?;
		let now = T::BlockNumberProvider::current_block_number();
		ensure!(now < round.round_ending_block, Error::<T>::VotingRoundOver);
		Ok(())
	}

	pub fn unlist_project(project_id: ProjectId<T>) -> DispatchResult {
		WhiteListedProjectAccounts::<T>::remove(&project_id);

		Ok(())
	}

	// The total reward to be distributed is a portion or inflation, determined in another pallet
	// Reward calculation is executed within the Voting period
	pub fn calculate_rewards(total_reward: BalanceOf<T>) -> DispatchResult {
		let projects: Vec<ProjectId<T>> = WhiteListedProjectAccounts::<T>::iter_keys().collect();
		if projects.is_empty() { return Ok(()) }
		let round_number = NextVotingRoundNumber::<T>::get().saturating_sub(1);
		let round = VotingRounds::<T>::get(round_number).ok_or(Error::<T>::NoRoundFound)?;
		if projects.clone().len() > 0 as usize {
			let total_positive_votes_amount = round.total_positive_votes_amount;
			let total_negative_votes_amount = round.total_negative_votes_amount;
			let when = T::BlockNumberProvider::current_block_number();
			let total_votes_amount =
				total_positive_votes_amount.saturating_sub(total_negative_votes_amount);

			// for each project, calculate the percentage of votes, the amount to be distributed,
			// and then populate the storage Projects
			for project_id in projects {
				if ProjectFunds::<T>::contains_key(&project_id) {
					let funds = ProjectFunds::<T>::get(&project_id);
					let project_positive_reward = funds[0];
					let project_negative_reward = funds[1];

					if project_positive_reward > project_negative_reward {
						let project_reward =
							project_positive_reward.saturating_sub(project_negative_reward);

						let project_percentage =
							Percent::from_rational(project_reward, total_votes_amount);
						let final_amount = project_percentage * total_reward;

						// Send calculated reward for reward distribution
						let project_info = ProjectInfo {
							project_id: project_id.clone(),
							submission_block: when,
							amount: final_amount,
						};

						// create a spend for project to be rewarded
						let _ = SpendInfo::<T>::new(&project_info);

						Self::deposit_event(Event::<T>::ProjectFundingAccepted {
							project_id: project_id.clone(),
							when,
							round_number,
							amount: project_info.amount,
						})
					} else {
						Self::deposit_event(Event::<T>::ProjectFundingRejected {
							when,
							project_id: project_id.clone(),
						})
					}
				}
			}
		}

		Ok(())
	}

	// To be executed in a hook, on_initialize
	pub fn on_idle_function(limit: Weight) -> Weight {
		let now = T::BlockNumberProvider::current_block_number();
		let mut meter = WeightMeter::with_limit(limit);
		let max_block_weight = T::DbWeight::get().reads_writes(14, 8);

		if meter.try_consume(max_block_weight).is_err() {
			return meter.consumed();
		}
		let mut round_index = NextVotingRoundNumber::<T>::get();

		// No active round?
		if round_index == 0 {
			// Start the first voting round
			let _round0 = VotingRoundInfo::<T>::new();
			round_index = NextVotingRoundNumber::<T>::get();
		}

		let current_round_index = round_index.saturating_sub(1);

		let round_infos = VotingRounds::<T>::get(current_round_index).expect("InvalidResult");
		let round_ending_block = round_infos.round_ending_block;

		// Conditions for reward distribution preparations are:
		// - We are at the end of voting_round period
		if now >= round_ending_block {
			// prepare reward distribution
			// for now we are using the temporary-constant reward.
			let _ = Self::calculate_rewards(T::TemporaryRewards::get())
				.map_err(|_| Error::<T>::FailedRewardCalculation);

			// Create a new round.
			let _new_round = VotingRoundInfo::<T>::new();
			// Clear ProjectFunds storage
			ProjectFunds::<T>::drain();
			// Emmit events
			Self::deposit_event(Event::<T>::VotingRoundEnded {
				when: now,
				round_number: round_infos.round_number,
			});
		}

		meter.consumed()
	}
}
