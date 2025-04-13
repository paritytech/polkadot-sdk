pub use super::*;
use frame_support::traits::{ReservableCurrency, VoteTally};
use pallet_referenda::{Deposit, TallyOf, TracksInfo};

pub trait ReferendumTrait<AccountId> {
	type Index: From<u32>
		+ Parameter
		+ Member
		+ Ord
		+ PartialOrd
		+ Copy
		+ HasCompact
		+ MaxEncodedLen;
	type Proposal: Parameter + Member + MaxEncodedLen;
	type ReferendumInfo: Eq + PartialEq + Debug + Encode + Decode + TypeInfo + Clone;
	type Preimages;
	type Call;
	type Moment;

	fn create_proposal(proposal_call: Self::Call) -> Self::Proposal;
	fn submit_proposal(caller: AccountId, proposal: Self::Proposal) -> Result<u32, DispatchError>;
	fn get_referendum_info(index: Self::Index) -> Option<Self::ReferendumInfo>;
	fn handle_referendum_info(infos: Self::ReferendumInfo) -> Option<ReferendumStates>;
	fn referendum_count() -> Self::Index;
	fn get_time_periods(index: Self::Index) -> Result<TimePeriods, DispatchError>;
	fn enter_decision_period(
		index: Self::Index,
		project_id: AccountId,
	) -> Result<u128, DispatchError>;
}

pub trait ConvictionVotingTrait<AccountId> {
	type Vote;
	type AccountVote;
	type Index: From<u32>
		+ Parameter
		+ Member
		+ Ord
		+ PartialOrd
		+ Copy
		+ HasCompact
		+ MaxEncodedLen;
	type Balance;
	type Moment;

	fn u128_to_balance(x: u128) -> Option<Self::Balance>;
	fn vote_data(aye: bool, conviction: Conviction, balance: Self::Balance) -> Self::AccountVote;
	fn try_vote(
		caller: &AccountId,
		ref_index: Self::Index,
		vote: Self::AccountVote,
	) -> DispatchResult;
	fn try_remove_vote(caller: &AccountId, ref_index: Self::Index) -> DispatchResult;
	fn unlock_voter_balance(caller: AccountId, ref_index: Self::Index, voter: AccountId) -> DispatchResult;
}

// Implement VotingHooks for pallet_conviction_voting
impl<T: Config> VotingHooks<AccountIdOf<T>, ReferendumIndex, BalanceOf<T>> for Pallet<T> {
	fn on_before_vote(
		who: &AccountIdOf<T>,
		ref_index: ReferendumIndex,
		vote: AccountVoteOf<T>,
	) -> DispatchResult {
		// lock user's funds
		let ref_info = T::Governance::get_referendum_info(ref_index.into())
			.ok_or_else(|| DispatchError::Other("No referendum info found"))?;
		let ref_status = T::Governance::handle_referendum_info(ref_info.clone())
			.ok_or_else(|| DispatchError::Other("No referendum status found"))?;
		match ref_status {
			ReferendumStates::Ongoing => {
				let amount = vote.balance();
				// Check that voter has enough funds to vote
				let voter_balance = T::NativeBalance::reducible_balance(
					&who,
					Preservation::Preserve,
					Fortitude::Polite,
				);
				ensure!(voter_balance >= amount, Error::<T>::NotEnoughFunds);
				// Check the available un-holded balance
				let voter_holds = T::NativeBalance::balance_on_hold(
					&<T as Config>::RuntimeHoldReason::from(HoldReason::FundsReserved),
					&who,
				);
				let available_funds = voter_balance.saturating_sub(voter_holds);
				ensure!(available_funds > amount, Error::<T>::NotEnoughFunds);
				// Lock the necessary amount
				T::NativeBalance::hold(&HoldReason::FundsReserved.into(), &who, amount)?;
			},
			_ => return Err(DispatchError::Other("Not an ongoing referendum")),
		};
		Ok(())
	}
	fn on_remove_vote(_who: &AccountIdOf<T>, _ref_index: ReferendumIndex, _status: Status) {
		let ref_info = match T::Governance::get_referendum_info(_ref_index.into()) {
			Some(info) => info,
			None => return,
		};

		let ref_status = match T::Governance::handle_referendum_info(ref_info.clone()){
			Some(status) => status,
			None => return,
		};
		match ref_status {
			ReferendumStates::Ongoing => {
				let vote_infos = match Votes::<T>::get(&_ref_index, _who) {
					Some(vote_infos) => vote_infos,
					None => return,
				};
				let vote_info = vote_infos;
				let amount = vote_info.amount;
				// Unlock user's funds
				T::NativeBalance::release(
					&HoldReason::FundsReserved.into(),
					&_who,
					amount,
					Precision::Exact,
				)
				.ok()
			},
			_ => {
				// No-op
				None
			},
		};
	}
	fn lock_balance_on_unsuccessful_vote(
		_who: &AccountIdOf<T>,
		_ref_index: ReferendumIndex,
	) -> Option<BalanceOf<T>> {
		// No-op
		None
	}
}

impl<T: pallet_conviction_voting::Config<I>, I: 'static> ConvictionVotingTrait<AccountIdOf<T>>
	for pallet_conviction_voting::Pallet<T, I> where <<T as pallet_conviction_voting::Config<I>>::Polls as frame_support::traits::Polling<pallet_conviction_voting::Tally<<<T as pallet_conviction_voting::Config<I>>::Currency as frame_support::traits::Currency<<T as frame_system::Config>::AccountId>>::Balance, <T as pallet_conviction_voting::Config<I>>::MaxTurnout>>>::Index: From<u32>
{
	type Vote = pallet_conviction_voting::VotingOf<T, I>;
	type AccountVote =
		pallet_conviction_voting::AccountVote<Self::Balance>;
	type Index = pallet_conviction_voting::PollIndexOf<T, I>;
	type Balance = <<T as pallet_conviction_voting::Config<I>>::Currency as frame_support::traits::Currency<<T as frame_system::Config>::AccountId>>::Balance;
	type Moment = <T::BlockNumberProvider as BlockNumberProvider>::BlockNumber;

	fn vote_data(aye:bool, conviction: Conviction, balance: Self::Balance) -> Self::AccountVote {
		pallet_conviction_voting::AccountVote::Standard {
			vote: pallet_conviction_voting::Vote { aye, conviction },
			balance,
		}
	}
	fn try_vote(
		caller: &AccountIdOf<T>,
		ref_index: Self::Index,
		vote: Self::AccountVote,
	) -> DispatchResult {
		let origin = RawOrigin::Signed(caller.clone());
		pallet_conviction_voting::Pallet::<T, I>::vote(origin.into(), ref_index, vote)?;
		Ok(())
	}
	fn u128_to_balance(x: u128) -> Option<Self::Balance> {
		x.try_into().ok()
	}
	fn try_remove_vote(caller: &AccountIdOf<T>,ref_index: Self::Index) -> DispatchResult {
		let origin = RawOrigin::Signed(caller.clone());
		pallet_conviction_voting::Pallet::<T, I>::remove_vote(origin.into(),None,ref_index)?;
		Ok(())
	}
	fn unlock_voter_balance(
		caller: AccountIdOf<T>,
		ref_index: Self::Index,
		voter: AccountIdOf<T>,
	) -> DispatchResult{
		let origin = RawOrigin::Signed(caller.clone());
		let infos = T::Polls::as_ongoing(ref_index)
			.ok_or_else(|| DispatchError::Other("No ongoing referendum found"))?;
		let class = infos.1;
		// get type AccountIdLookupOf<T> from voter
		let target = T::Lookup::unlookup(voter.clone());
		pallet_conviction_voting::Pallet::<T, I>::unlock(origin.into(), class,target.into())?;

		Ok(())
	}
}

impl<T: frame_system::Config + pallet_referenda::Config<I>, I: 'static>
	ReferendumTrait<AccountIdOf<T>> for pallet_referenda::Pallet<T, I>
where
	<T as pallet_referenda::Config<I>>::RuntimeCall: Sync + Send,
{
	type Index = pallet_referenda::ReferendumIndex;
	type Proposal = Bounded<
		<T as pallet_referenda::Config<I>>::RuntimeCall,
		<T as frame_system::Config>::Hashing,
	>;
	type ReferendumInfo = pallet_referenda::ReferendumInfoOf<T, I>;
	type Preimages = <T as pallet_referenda::Config<I>>::Preimages;
	type Call = <T as frame_system::Config>::RuntimeCall;
	type Moment = <T::BlockNumberProvider as BlockNumberProvider>::BlockNumber;

	fn create_proposal(proposal_call: Self::Call) -> Self::Proposal {
		let call_formatted = <T as pallet_referenda::Config<I>>::RuntimeCall::from(proposal_call);
		let bounded_proposal = match Self::Preimages::bound(call_formatted){
			Ok(bounded_proposal) => bounded_proposal,
			Err(_) => {
				panic!("Failed to bound proposal");
			},
		};
		bounded_proposal
	}

	fn submit_proposal(
		who: AccountIdOf<T>,
		proposal: Self::Proposal,
	) -> Result<u32, DispatchError> {
		let enactment_moment = DispatchTime::After(0u32.into());
		let proposal_origin0 = RawOrigin::Root.into();
		let proposal_origin = Box::new(proposal_origin0);
		if let (Some(preimage_len), Some(proposal_len)) =
			(proposal.lookup_hash().and_then(|h| Self::Preimages::len(&h)), proposal.lookup_len())
		{
			if preimage_len != proposal_len {
				return Err(
					pallet_referenda::Error::<T, I>::PreimageStoredWithDifferentLength.into()
				);
			}
		}
		let track = T::Tracks::track_for(&proposal_origin)
			.map_err(|_| pallet_referenda::Error::<T, I>::NoTrack)?;
		T::Currency::reserve(&who, T::SubmissionDeposit::get())?;
		let amount = T::SubmissionDeposit::get();
		let submission_deposit = Deposit { who, amount };
		let index = pallet_referenda::ReferendumCount::<T, I>::mutate(|x| {
			let r = *x;
			*x += 1;
			r
		});
		let now = T::BlockNumberProvider::current_block_number();
		let nudge_call =
			T::Preimages::bound(<<T as pallet_referenda::Config<I>>::RuntimeCall>::from(
				pallet_referenda::Call::nudge_referendum { index },
			))?;

		let alarm_interval = T::AlarmInterval::get().max(One::one());
		// Alarm must go off no earlier than `when`.
		// This rounds `when` upwards to the next multiple of `alarm_interval`.
		let when0 = now.saturating_add(T::UndecidingTimeout::get());
		let when = (when0.saturating_add(alarm_interval.saturating_sub(One::one())) /
			alarm_interval)
			.saturating_mul(alarm_interval);
		let result = T::Scheduler::schedule(
			DispatchTime::At(when),
			None,
			128u8,
			frame_system::RawOrigin::Root.into(),
			nudge_call,
		);
		if let Err(_e) = result {
			return Err(DispatchError::Other("SchedulerError"));
		}
		let alarm = result.ok().map(|x| (when, x));

		let status = pallet_referenda::ReferendumStatus {
			track,
			origin: *proposal_origin,
			proposal: proposal.clone(),
			enactment: enactment_moment,
			submitted: now,
			submission_deposit,
			decision_deposit: None,
			deciding: None,
			tally: TallyOf::<T, I>::new(track),
			in_queue: false,
			alarm,
		};
		pallet_referenda::ReferendumInfoFor::<T, I>::insert(
			index,
			Self::ReferendumInfo::Ongoing(status),
		);
		Ok(index)
	}

	fn get_referendum_info(index: Self::Index) -> Option<Self::ReferendumInfo> {
		pallet_referenda::ReferendumInfoFor::<T, I>::get(index)
	}
	fn handle_referendum_info(infos: Self::ReferendumInfo) -> Option<ReferendumStates> {
		match infos {
			Self::ReferendumInfo::Approved(..) => Some(ReferendumStates::Approved),
			Self::ReferendumInfo::Rejected(..) => Some(ReferendumStates::Rejected),
			Self::ReferendumInfo::Ongoing(..) => Some(ReferendumStates::Ongoing),
			_ => None,
		}
	}

	fn referendum_count() -> Self::Index {
		pallet_referenda::ReferendumCount::<T, I>::get()
	}

	fn get_time_periods(index: Self::Index) -> Result<TimePeriods, DispatchError> {
		let info = Self::get_referendum_info(index)
			.ok_or_else(|| DispatchError::Other("No referendum info found"))?;
		match info {
			Self::ReferendumInfo::Ongoing(ref info) => {
				let track_id = info.track;
				let track = T::Tracks::info(track_id)
					.ok_or_else(|| DispatchError::Other("No track info found"))?;

				let decision_period: u128 = track.decision_period.try_into().map_err(|_| {
					DispatchError::Other("Failed to convert decision period to u128")
				})?;
				let prepare_period: u128 = track.prepare_period.try_into().map_err(|_| {
					DispatchError::Other("Failed to convert decision period to u128")
				})?;
				let confirm_period: u128 = track.confirm_period.try_into().map_err(|_| {
					DispatchError::Other("Failed to convert decision period to u128")
				})?;
				let min_enactment_period: u128 =
					track.min_enactment_period.try_into().map_err(|_| {
						DispatchError::Other("Failed to convert decision period to u128")
					})?;
				// Calculate the total period
				let total_period =
					decision_period + prepare_period + confirm_period + min_enactment_period;
				let total_period: u128 = total_period.try_into().map_err(|_| {
					DispatchError::Other("Failed to convert decision period to u128")
				})?;
				let time_periods = TimePeriods {
					prepare_period,
					decision_period,
					confirm_period,
					min_enactment_period,
					total_period,
				};
				Ok(time_periods)
			},
			_ => Err(DispatchError::Other("Not an ongoing referendum")),
		}
	}

	fn enter_decision_period(
		index: Self::Index,
		project_id: AccountIdOf<T>,
	) -> Result<u128, DispatchError> {
		let origin = RawOrigin::Signed(project_id.clone()).into();
		let info = Self::get_referendum_info(index)
			.ok_or_else(|| DispatchError::Other("No referendum info found"))?;
		match info {
			Self::ReferendumInfo::Ongoing(ref info) => {
				let track_id = info.track;
				let track = T::Tracks::info(track_id)
					.ok_or_else(|| DispatchError::Other("No track info found"))?;

				pallet_referenda::Pallet::<T, I>::place_decision_deposit(origin, index)
					.map_err(|_| DispatchError::Other("Failed to place decision deposit"))?;

				let decision_period: u128 = track.decision_period.try_into().map_err(|_| {
					DispatchError::Other("Failed to convert decision period to u128")
				})?;
				Ok(decision_period)
			},
			_ => Err(DispatchError::Other("Not an ongoing referendum")),
		}
	}

}
