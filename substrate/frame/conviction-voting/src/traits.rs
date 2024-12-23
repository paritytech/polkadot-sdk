use crate::AccountVote;
use frame_support::dispatch::DispatchResult;

pub trait VotingHooks<AccountId, Index, Balance> {
	// Called when vote is executed.
	fn on_vote(who: &AccountId, ref_index: Index, vote: AccountVote<Balance>) -> DispatchResult;

	// Called when removed vote is executed.
	// is_finished indicates the state of the referendum = None if referendum is cancelled, Some(true) if referendum is ongoing and Some(false) when finished.
	fn on_remove_vote(who: &AccountId, ref_index: Index, ongoing: Option<bool>);

	// Called when removed vote is executed and voter lost the direction to possibly lock some balance.
	// Can return an amount that should be locked for the conviction time.
	fn balance_locked_on_unsuccessful_vote(who: &AccountId, ref_index: Index) -> Option<Balance>;

	#[cfg(feature = "runtime-benchmarks")]
	fn on_vote_worst_case(who: &AccountId);

	#[cfg(feature = "runtime-benchmarks")]
	fn on_remove_vote_worst_case(who: &AccountId);
}

// Default implementation for VotingHooks
impl<A, I, B> VotingHooks<A, I, B> for () {
	fn on_vote(_who: &A, _ref_index: I, _vote: AccountVote<B>) -> DispatchResult {
		Ok(())
	}

	fn on_remove_vote(_who: &A, _ref_index: I, _ongoing: Option<bool>) {}

	fn balance_locked_on_unsuccessful_vote(_who: &A, _ref_index: I) -> Option<B> {
		None
	}

	#[cfg(feature = "runtime-benchmarks")]
	fn on_vote_worst_case(_who: &A) {}

	#[cfg(feature = "runtime-benchmarks")]
	fn on_remove_vote_worst_case(_who: &A) {}
}
