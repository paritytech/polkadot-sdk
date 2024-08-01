pub use super::*;
impl<T: Config> Pallet<T> {

    pub fn try_vote(voter_id: AccountIdOf<T>, project: AccountIdOf<T>, amount: BalanceOf<T>, is_fund:bool ) -> DispatchResult{
        let projects = WhiteListedProjectAccounts::<T>::get();

        // Project is whiteListed
        ensure!(projects.contains(project.clone()), Error::<T>::NotWhitelistedProject);
        let new_vote = VoteInfo{
            amount,
            is_fund,
        };
        if Votes::<T>::contains_key(project,voter_id) {
            Votes::<T>::mutate(project,voter_id,|value|{
                *value = Some(new_vote);
            });
        } else{
            Votes::<T>::insert(project, voter_id, new_vote).ok_or(Error::<T>::VoteFailed)?;
        }

        
        Ok(())
    }
}