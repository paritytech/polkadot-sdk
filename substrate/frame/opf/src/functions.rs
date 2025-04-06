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

//! # Helper functions for OPF pallet.

pub use super::*;
impl<T: Config> Pallet<T> {
	pub fn get_formatted_call(call: Call<T>) -> <T as Config>::RuntimeCall {
		call.into()
	}
	pub fn conviction_amount(amount: BalanceOf<T>, conviction: Conviction) -> Option<BalanceOf<T>> {
		let conviction_amount: BalanceOf<T> = match conviction {
			Conviction::None => amount.checked_div(&10u8.into()).unwrap_or_else(Zero::zero),
			_ => amount.saturating_mul(<u8 as From<Conviction>>::from(conviction).into()),
		};
		Some(conviction_amount)
	}

	pub fn start_referendum(
		caller: ProjectId<T>,
		proposal_call: pallet::Call<T>,
	) -> Result<u32, DispatchError> {
		let proposal0 = Box::new(Self::get_formatted_call(proposal_call.into()));
		let call = Call::<T>::execute_call_dispatch { caller: caller.clone(), proposal: proposal0 };
		let call_formatted = Self::get_formatted_call(call.into());
		let proposal = T::Governance::create_proposal(call_formatted.into());

		let index = T::Governance::submit_proposal(caller, proposal)?;
		Ok(index)
	}

	// Helper function for voting action. Existing votes are over-written, and Hold is adjusted
	pub fn try_vote(
		voter_id: VoterId<T>,
		project: ProjectId<T>,
		amount: BalanceOf<T>,
		fund: bool,
		conviction: Conviction,
	) -> DispatchResult {
		let _origin = T::RuntimeOrigin::from(RawOrigin::Signed(voter_id.clone()));
		if !ProjectFunds::<T>::contains_key(&project) {
			let fund = Funds {
				positive_funds: BalanceOf::<T>::zero(),
				negative_funds: BalanceOf::<T>::zero(),
			};
			ProjectFunds::<T>::insert(&project, fund);
		}

		let infos = WhiteListedProjectAccounts::<T>::get(project.clone())
			.ok_or(Error::<T>::NoProjectAvailable)?;
		let _ref_index = infos.index;

		let conviction_fund =
			Self::conviction_amount(amount, conviction).ok_or("Invalid conviction")?;

		// Create vote infos and store/adjust them
		let round_number = NextVotingRoundNumber::<T>::get().saturating_sub(1);
		let mut round = VotingRounds::<T>::get(round_number).ok_or(Error::<T>::NoRoundFound)?;
		if fund {
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
			fund,
			conviction,
			funds_unlock_block: round.round_ending_block,
		};

		// Update Funds unlock block according to the selected conviction
		//new_vote.funds_unlock();
		if Votes::<T>::contains_key(&project, &voter_id) {
			let old_vote = Votes::<T>::get(&project, &voter_id).ok_or(Error::<T>::NoVoteData)?;
			let old_amount = old_vote.amount;
			let old_conviction = old_vote.conviction;
			let old_conviction_amount =
				Self::conviction_amount(old_amount, old_conviction).ok_or("Invalid conviction")?;
			ProjectFunds::<T>::mutate(&project, |val| {
				let mut val0 = val.clone();
				if fund {
					val0.positive_funds = val0
						.positive_funds
						.saturating_add(conviction_fund)
						.saturating_sub(old_conviction_amount);
				} else {
					val0.negative_funds = val0
						.negative_funds
						.saturating_add(conviction_fund)
						.saturating_sub(old_conviction_amount);
				}
				*val = val0;
			});

			Votes::<T>::mutate(&project, &voter_id, |value| {
				*value = Some(new_vote);
			});
		} else {
			Votes::<T>::insert(&project, &voter_id, new_vote);
			ProjectFunds::<T>::mutate(&project, |val| {
				let mut val0 = val.clone();
				if fund {
					val0.positive_funds = val0.positive_funds.saturating_add(conviction_fund);
				} else {
					val0.negative_funds = val0.negative_funds.saturating_add(conviction_fund);
				}
				*val = val0;
			});
		}

		Ok(())
	}

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

	// Helper function for complete vote data removal from storage.
	pub fn try_remove_vote(voter_id: VoterId<T>, project: ProjectId<T>) -> DispatchResult {
		if Votes::<T>::contains_key(&project, &voter_id) {
			let infos = Votes::<T>::get(&project, &voter_id).ok_or(Error::<T>::NoVoteData)?;
			let amount = infos.amount;
			let conviction = infos.conviction;
			let fund = infos.fund;

			let conviction_fund =
				Self::conviction_amount(amount, conviction).ok_or("Invalid conviction")?;

			// Update Round infos
			let round_number = NextVotingRoundNumber::<T>::get().saturating_sub(1);
			let mut round = VotingRounds::<T>::get(round_number).ok_or(Error::<T>::NoRoundFound)?;
			if fund {
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
				let mut val0 = val.clone();
				if fund {
					val0.positive_funds = val0.positive_funds.saturating_sub(conviction_fund);
				} else {
					val0.negative_funds = val0.negative_funds.saturating_sub(conviction_fund);
				}
				*val = val0;
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
	// Reward calculation is executed within the Voting period
	pub fn calculate_rewards(total_reward: BalanceOf<T>) -> DispatchResult {
		let projects: Vec<ProjectId<T>> = WhiteListedProjectAccounts::<T>::iter_keys().collect();
		if projects.is_empty() {
			return Ok(());
		}
		let round_number = NextVotingRoundNumber::<T>::get().saturating_sub(1);
		let round = VotingRounds::<T>::get(round_number).ok_or(Error::<T>::NoRoundFound)?;
		if projects.clone().len() > 0 as usize {
			let total_positive_votes_amount = round.total_positive_votes_amount;
			let total_votes_amount = total_positive_votes_amount;

			// for each project, calculate the percentage of votes, the amount to be distributed,
			// and then populate the storage Projects
			for project_id in projects {
				if ProjectFunds::<T>::contains_key(&project_id) {
					let funds = ProjectFunds::<T>::get(&project_id);
					let project_positive_reward = funds.positive_funds;
					let project_negative_reward = funds.negative_funds;

					if project_positive_reward > project_negative_reward {
						let project_reward =
							project_positive_reward.saturating_sub(project_negative_reward);

						let project_percentage =
							Percent::from_rational(project_reward, total_votes_amount);
						let final_amount = project_percentage * total_reward;
						let infos = WhiteListedProjectAccounts::<T>::get(&project_id)
							.ok_or(Error::<T>::NoProjectAvailable)?;
						let ref_index = infos.index;
						let submission_block = infos.submission_block;

						// Send calculated reward for reward distribution
						let project_info = ProjectInfo {
							project_id: project_id.clone(),
							submission_block,
							amount: final_amount,
							index: ref_index,
						};
						WhiteListedProjectAccounts::<T>::mutate(project_id.clone(), |val| {
							*val = Some(project_info.clone());
						});
					}
				}
			}
		}

		Ok(())
	}

	pub fn convert_moment_to_block_number(
        moment: <<T as pallet::Config>::Governance as traits::ReferendumTrait<<T as frame_system::Config>::AccountId>>::Moment,
	) -> <<T as pallet::Config>::BlockNumberProvider as sp_runtime::traits::BlockNumberProvider>::BlockNumber where <<T as pallet::Config>::BlockNumberProvider as sp_runtime::traits::BlockNumberProvider>::BlockNumber: From<<<T as pallet::Config>::Governance as traits::ReferendumTrait<<T as frame_system::Config>::AccountId>>::Moment>{
		moment.saturated_into()
	}

	// To be executed in a hook, on_initialize
	pub fn on_idle_function(limit: Weight) -> Weight {
		let now = T::BlockNumberProvider::current_block_number();
		let mut meter = WeightMeter::with_limit(limit);
		let max_block_weight = T::BlockWeights::get().max_block;

		if meter.try_consume(max_block_weight).is_err() {
			return meter.consumed();
		}
		let round_index = NextVotingRoundNumber::<T>::get();
		if round_index == 0 {
			return meter.consumed();
		}

		let current_round_index = round_index.saturating_sub(1);

		if let Some(round_infos) = VotingRounds::<T>::get(current_round_index) {
			if round_infos.round_ending_block != round_infos.round_starting_block
			{let round_ending_block = round_infos.round_ending_block;

			if now >= round_ending_block {
				// Emmit event
				Self::deposit_event(Event::<T>::VoteActionLocked {
					round_number: round_infos.round_number,
				});
				// Emmit events
				Self::deposit_event(Event::<T>::VotingRoundEnded {
					round_number: round_infos.round_number,
				});
				// prepare reward distribution
				// for now we are using the temporary-constant reward.
				let _ = Self::calculate_rewards(T::TemporaryRewards::get())
					.map_err(|_| Error::<T>::FailedRewardCalculation);

				// Clear ProjectFunds storage
				ProjectFunds::<T>::drain();
			}}
		}
		meter.consumed()
	}
}
