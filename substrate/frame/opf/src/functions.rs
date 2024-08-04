pub use super::*;
impl<T: Config> Pallet<T> {
	// Helper function for voting action. Existing votes are over-written, and Hold is adjusted
	pub fn try_vote(
		voter_id: AccountIdOf<T>,
		project: ProjectId<T>,
		amount: BalanceOf<T>,
		is_fund: bool,
	) -> DispatchResult {
		let projects = WhiteListedProjectAccounts::<T>::get();

		// Check that Project is whiteListed
		ensure!(projects.contains(&project), Error::<T>::NotWhitelistedProject);

		// Create vote infos and store/adjust them 
		let round_number = VotingRoundsNumber::<T>::get().saturating_sub(1);
		let round = VotingRounds::<T>::get(round_number).ok_or(Error::<T>::NoRoundFound)?;
		let new_vote = VoteInfo { amount, round, is_fund };
		if Votes::<T>::contains_key(project.clone(), voter_id.clone()) {
			Votes::<T>::mutate(project.clone(), voter_id.clone(), |value| {
				*value = Some(new_vote);
			});
			// Adjust locked amount
			T::NativeBalance::set_on_hold(&HoldReason::FundsReserved.into(), &voter_id, amount)?;
		} else {
			Votes::<T>::insert(project.clone(), voter_id.clone(), new_vote);
			// Lock the necessary amount
			T::NativeBalance::hold(&HoldReason::FundsReserved.into(), &voter_id, amount)?;
		}

		Ok(())
	}

	// Helper function for complete vote data removal from storage.
	pub fn try_remove_vote(voter_id: AccountIdOf<T>, project: AccountIdOf<T>) -> DispatchResult {
		if Votes::<T>::contains_key(project.clone(), voter_id.clone()) {
			let infos =
				Votes::<T>::get(project.clone(), voter_id.clone()).ok_or(Error::<T>::NoVoteData)?;
			let amount = infos.amount;
			Votes::<T>::remove(project.clone(), voter_id.clone());

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
	// Reward calculation is executed within VotingLocked period --> "VotingLockBlock == EpochBeginningBlock" ???
	pub fn calculate_rewards(total_reward: BalanceOf<T>) -> DispatchResult {
		let projects = WhiteListedProjectAccounts::<T>::get();
		let votes = Votes::<T>::iter();
		let mut total_votes_amount = BalanceOf::<T>::zero();

		// Total amount from all votes
		for vote in votes {
			let info = vote.2.clone();
			total_votes_amount = total_votes_amount.saturating_add(info.amount);
		}

		// for each project, calculate the percentage of votes, the amount to be distributed,
		// and then populate the storage Projects in pallet_distribution
		for project in projects {
			let this_project_votes: Vec<_> =
				Votes::<T>::iter().filter(|x| x.0 == project.clone()).collect();

			let mut project_reward = BalanceOf::<T>::zero();
			for vote in this_project_votes.clone() {
				if vote.2.is_fund == true{
				project_reward = project_reward.saturating_add(vote.2.amount);
			}
			}
			for vote in this_project_votes {
				if vote.2.is_fund == false{
				project_reward = project_reward.saturating_sub(vote.2.amount);
			}
			}

			let project_percentage = Percent::from_rational(project_reward, total_votes_amount);
			let final_amount = project_percentage.mul_floor(total_reward);

			// Send calculated reward for distribution
			let now =
				<frame_system::Pallet<T>>::block_number().saturating_add(T::PaymentPeriod::get());
			let project_info = ProjectInfo {
				project_account: project,
				submission_block: now,
				amount: final_amount,
			};

			let mut rewarded = Distribution::Projects::<T>::get();
			rewarded.try_push(project_info).map_err(|_| Error::<T>::MaximumProjectsNumber)?;

			Distribution::Projects::<T>::mutate(|value| {
				*value = rewarded;
			});
		}
		Ok(())
	}
}
