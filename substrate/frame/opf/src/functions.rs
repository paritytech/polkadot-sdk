pub use super::*;
impl<T: Config> Pallet<T> {

    // Helper function for voting action. Existing votes are over-written, and Hold is adjusted
    pub fn try_vote(voter_id: AccountIdOf<T>, project: AccountIdOf<T>, amount: BalanceOf<T>, is_fund:bool ) -> DispatchResult {
        let projects = WhiteListedProjectAccounts::<T>::get();

        // Project is whiteListed
        ensure!(projects.contains(project.clone()), Error::<T>::NotWhitelistedProject);
        let mut old_amount = Zero::zero();
        let new_vote = VoteInfo{
            amount,
            is_fund,
        };
        if Votes::<T>::contains_key(project,voter_id) {
            Votes::<T>::mutate(project,voter_id,|value|{
                *value = Some(new_vote);
            });
            // Adjust locked amount
            T::NativeBalance::set_on_hold(
                &HoldReason::FundsReserved.into(),
				&voter_id,
				amount,
            )?;

        } else{
            Votes::<T>::insert(project, voter_id, new_vote).ok_or(Error::<T>::VoteFailed)?;
            // Lock the necessary amount
            T::NativeBalance::hold(
                &HoldReason::FundsReserved.into(),
				&voter_id,
				amount,
            )?;
        }

        Ok(())
    }

    // Helper function for complete vote data removal
    pub fn try_remove_vote(voter_id: AccountIdOf<T>, project: AccountIdOf<T>) -> DispatchResult {
        if Votes::<T>::contains_key(project,voter_id) {
            let infos = Votes::<T>::get(project, voter_id).ok_or(Error::<T>::NoVoteData)?;
            let amount = infos.amount;
            Votes::<T>::remove(project,voter_id);
            
            T::NativeBalance::release(
                &HoldReason::FundsReserved.into(),
                &voter_id,
                amount,
                Precision::Exact,
            )?;
        }
        Ok(())
    }
}