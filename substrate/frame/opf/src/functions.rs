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
	// Helper function for voting action. Existing votes are over-written, and Hold is adjusted
	pub fn try_vote(
		voter_id: VoterId<T>,
		project: ProjectId<T>,
		amount: BalanceOf<T>,
		is_fund: bool,
		conviction: Conviction,
	) -> DispatchResult {
		if !ProjectFunds::<T>::contains_key(&project) {
			let bounded = BoundedVec::<BalanceOf<T>, ConstU32<2>>::try_from(vec![
				BalanceOf::<T>::zero(),
				BalanceOf::<T>::zero(),
			])
			.expect("It works");
			ProjectFunds::<T>::insert(&project, bounded);
		}

		let projects = WhiteListedProjectAccounts::<T>::get();
		let conviction_fund = amount.saturating_add(
			amount.saturating_mul(<u8 as From<Conviction>>::from(conviction).into()),
		);

		// Check that Project is whiteListed
		ensure!(projects.contains(&project), Error::<T>::NotWhitelistedProject);

		// Create vote infos and store/adjust them
		let round_number = VotingRoundNumber::<T>::get().saturating_sub(1);
		let mut round = VotingRounds::<T>::get(round_number).ok_or(Error::<T>::NoRoundFound)?;
		if is_fund {
			round.total_positive_votes_amount =
				round.total_positive_votes_amount.saturating_add(conviction_fund);
		} else {
			round.total_negative_votes_amount =
				round.total_negative_votes_amount.saturating_add(conviction_fund);
		}

		VotingRounds::<T>::mutate(round_number, |val| {
			*val = Some(round.clone());
		});

		let mut new_vote = VoteInfo {
			amount,
			round: round.clone(),
			is_fund,
			conviction,
			funds_unlock_block: round.round_ending_block,
		};

		// Update Funds unlock block according to the selected conviction
		new_vote.funds_unlock();
		if Votes::<T>::contains_key(&project, &voter_id) {
			let old_vote = Votes::<T>::get(&project, &voter_id).ok_or(Error::<T>::NoVoteData)?;
			let old_amount = old_vote.amount;
			let old_conviction = old_vote.conviction;
			let old_conviction_amount = old_amount.saturating_add(
				old_amount.saturating_mul(<u8 as From<Conviction>>::from(old_conviction).into()),
			);
			ProjectFunds::<T>::mutate(&project, |val| {
				let mut val0 = val.clone().into_inner();
				if is_fund {
					val0[0] = val0[0 as usize]
						.saturating_add(conviction_fund)
						.saturating_sub(old_conviction_amount);
				} else {
					val0[1] = val0[1 as usize]
						.saturating_add(conviction_fund)
						.saturating_sub(old_conviction_amount);
				}
				*val = BoundedVec::<BalanceOf<T>, ConstU32<2>>::try_from(val0).expect("It works");
			});

			Votes::<T>::mutate(&project, &voter_id, |value| {
				*value = Some(new_vote);
			});

			// Adjust locked amount
			let total_hold = T::NativeBalance::total_balance_on_hold(&voter_id);
			let new_hold = total_hold.saturating_sub(old_amount).saturating_add(amount);
			T::NativeBalance::set_on_hold(&HoldReason::FundsReserved.into(), &voter_id, new_hold)?;
		} else {
			Votes::<T>::insert(&project, &voter_id, new_vote);
			ProjectFunds::<T>::mutate(&project, |val| {
				let mut val0 = val.clone().into_inner();
				if is_fund {
					val0[0] = val0[0 as usize].saturating_add(conviction_fund);
				} else {
					val0[1] = val0[1 as usize].saturating_add(conviction_fund);
				}
				*val = BoundedVec::<BalanceOf<T>, ConstU32<2>>::try_from(val0).expect("It works");
			});
			// Lock the necessary amount
			T::NativeBalance::hold(&HoldReason::FundsReserved.into(), &voter_id, amount)?;
		}

		Ok(())
	}

	// Voting Period checks
	pub fn period_check() -> DispatchResult {
		// Get current voting round & check if we are in voting period or not
		let current_round_index = VotingRoundNumber::<T>::get().saturating_sub(1);
		let round = VotingRounds::<T>::get(current_round_index).ok_or(Error::<T>::NoRoundFound)?;
		let now = T::BlockNumberProvider::current_block_number();
		ensure!(now < round.voting_locked_block, Error::<T>::VotePeriodClosed);
		ensure!(now < round.round_ending_block, Error::<T>::VotingRoundOver);
		Ok(())
	}

	// Helper function for complete vote data removal from storage.
	pub fn try_remove_vote(voter_id: VoterId<T>, project: ProjectId<T>) -> DispatchResult {
		if Votes::<T>::contains_key(&project, &voter_id) {
			let infos = Votes::<T>::get(&project, &voter_id).ok_or(Error::<T>::NoVoteData)?;
			let amount = infos.amount;
			let conviction = infos.conviction;
			let is_fund = infos.is_fund;

			let conviction_fund = amount.saturating_add(
				amount.saturating_mul(<u8 as From<Conviction>>::from(conviction).into()),
			);

			// Update Round infos
			let round_number = VotingRoundNumber::<T>::get().saturating_sub(1);
			let mut round = VotingRounds::<T>::get(round_number).ok_or(Error::<T>::NoRoundFound)?;
			if is_fund {
				round.total_positive_votes_amount =
					round.total_positive_votes_amount.saturating_sub(conviction_fund);
			} else {
				round.total_negative_votes_amount =
					round.total_negative_votes_amount.saturating_sub(conviction_fund);
			}

			VotingRounds::<T>::mutate(round_number, |val| {
				*val = Some(round.clone());
			});

			// Update ProjectFund Storage
			ProjectFunds::<T>::mutate(&project, |val| {
				let mut val0 = val.clone().into_inner();
				if is_fund {
					val0[0] = val0[0 as usize].saturating_sub(conviction_fund);
				} else {
					val0[1] = val0[1 as usize].saturating_sub(conviction_fund);
				}
				*val = BoundedVec::<BalanceOf<T>, ConstU32<2>>::try_from(val0).expect("It works");
			});

			// Remove Vote Infos
			Votes::<T>::remove(&project, &voter_id);

			T::NativeBalance::release(
				&HoldReason::FundsReserved.into(),
				&voter_id,
				amount,
				Precision::Exact,
			)?;
		}
		Ok(())
	}

	// The total reward to be distributed is a portion or inflation, determined in another pallet
	// Reward calculation is executed within VotingLocked period --> "VotingLockBlock ==
	// EpochBeginningBlock"
	pub fn calculate_rewards(total_reward: BalanceOf<T>) -> DispatchResult {
		let projects = WhiteListedProjectAccounts::<T>::get();
		let round_number = VotingRoundNumber::<T>::get().saturating_sub(1);
		let round = VotingRounds::<T>::get(round_number).ok_or(Error::<T>::NoRoundFound)?;
		if projects.clone().len() > 0 as usize {
			let total_positive_votes_amount = round.total_positive_votes_amount;
			let total_negative_votes_amount = round.total_negative_votes_amount;

			let total_votes_amount =
				total_positive_votes_amount.saturating_sub(total_negative_votes_amount);

			// for each project, calculate the percentage of votes, the amount to be distributed,
			// and then populate the storage Projects in pallet_distribution
			for project in projects {
				if ProjectFunds::<T>::contains_key(&project) {
					let funds = ProjectFunds::<T>::get(&project);
					let project_positive_reward = funds[0];
					let project_negative_reward = funds[1];

					let project_reward =
						project_positive_reward.saturating_sub(project_negative_reward);

					if !project_reward.is_zero() {
						let project_percentage =
							Percent::from_rational(project_reward, total_votes_amount);
						let final_amount = project_percentage * total_reward;

						// Send calculated reward for distribution
						let now = T::BlockNumberProvider::current_block_number()
							.checked_add(&T::BufferPeriod::get())
							.ok_or(Error::<T>::InvalidResult)?;
						let project_info = ProjectInfo {
							project_id: project.clone(),
							submission_block: now,
							amount: final_amount,
						};

						let mut rewarded = Distribution::Projects::<T>::get();
						rewarded
							.try_push(project_info.clone())
							.map_err(|_| Error::<T>::MaximumProjectsNumber)?;

						Distribution::Projects::<T>::mutate(|value| {
							*value = rewarded;
						});

						let when = T::BlockNumberProvider::current_block_number();
						Self::deposit_event(Event::<T>::ProjectFundingAccepted {
							project_id: project,
							when,
							round_number,
							amount: project_info.amount,
						})
					} else {
						// remove unfunded project from whitelisted storage
						Self::remove_unfunded_project(project.clone())?;
						let when = T::BlockNumberProvider::current_block_number();
						Self::deposit_event(Event::<T>::ProjectFundingRejected {
							when,
							project_id: project,
						});
					}
				}
			}
		}

		Ok(())
	}

	pub fn remove_unfunded_project(project_id: ProjectId<T>) -> DispatchResult {
		WhiteListedProjectAccounts::<T>::mutate(|value| {
			let mut val = value.clone();
			val.retain(|x| *x != project_id);
			*value = val;
		});
		let when = T::BlockNumberProvider::current_block_number();

		Self::deposit_event(Event::<T>::ProjectUnlisted { when, project_id });

		Ok(())
	}

	// To be executed in a hook, on_initialize
	pub fn on_idle_function(now: BlockNumberFor<T>, limit: Weight) -> Weight {
		let mut meter = WeightMeter::with_limit(limit);
		let max_block_weight = T::BlockWeights::get().max_block/10;

		if meter.try_consume(max_block_weight).is_err() {
			return meter.consumed();
		}
		let mut round_index = VotingRoundNumber::<T>::get();

		// No active round?
		if round_index == 0 {
			// Start the first voting round
			let _round0 = VotingRoundInfo::<T>::new();
			round_index = VotingRoundNumber::<T>::get();
		}

		let current_round_index = round_index.saturating_sub(1);

		let round_infos = VotingRounds::<T>::get(current_round_index).expect("InvalidResult");
		let voting_locked_block = round_infos.voting_locked_block;
		let round_ending_block = round_infos.round_ending_block;

		// Conditions for distribution preparations are:
		// - We are within voting_round period
		// - We are past the voting_round_lock block

		if now == voting_locked_block {
			// Emmit event
			Self::deposit_event(Event::<T>::VoteActionLocked {
				when: now,
				round_number: round_infos.round_number,
			});
			// prepare reward distribution
			// for now we are using the temporary-constant reward.
			let _ = Self::calculate_rewards(T::TemporaryRewards::get())
				.map_err(|_| Error::<T>::FailedRewardCalculation);
		}

		// Create a new round when we reach the end of the current round.
		if now == round_ending_block {
			let _new_round = VotingRoundInfo::<T>::new();
			// Clear WhiteListedProjectAccounts storage
			WhiteListedProjectAccounts::<T>::kill();
			// Clear Votes storage
			Votes::<T>::drain();
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
