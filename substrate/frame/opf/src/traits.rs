pub use super::*;
use frame_support::traits::{ReservableCurrency, VoteTally};
use pallet_referenda::{TallyOf, Deposit, TracksInfo};


pub trait ReferendumTrait<AccountId> {
	type Index: Parameter + Member + Ord + PartialOrd + Copy + HasCompact + MaxEncodedLen;
	type Proposal: Parameter + Member + MaxEncodedLen;
	type ProposalOrigin: Parameter + Member + MaxEncodedLen;
	type ReferendumInfo: Eq + PartialEq + Debug + Encode + Decode + TypeInfo + Clone;
	type Moment;

	fn submit_proposal(
		caller: AccountId,
		proposal: Self::Proposal,
		proposal_origin: Box<Self::ProposalOrigin>,
		enactment_moment: DispatchTime<Self::Moment>,
	) -> DispatchResult;

	fn get_referendum_info(index: Self::Index) -> Option<Self::ReferendumInfo>;
}

pub trait ConvictionVotingTrait<AccountId> {
	type Vote;
	type AccountVote;
	type Index: Parameter + Member + Ord + PartialOrd + Copy + HasCompact + MaxEncodedLen;
	type Balance;
	type Moment;

	fn vote_data(aye:bool, conviction: Conviction, balance: Self::Balance) -> Self::AccountVote;
	fn try_vote(
		caller: &AccountId,
		ref_index: Self::Index,
		vote: Self::AccountVote,
	) -> DispatchResult;
	/*fn try_remove_vote(ref_index: Self::Index) -> Result<(), ()>;*/
}

impl<T: pallet_conviction_voting::Config<I>, I: 'static> ConvictionVotingTrait<AccountIdOf<T>>
	for pallet_conviction_voting::Pallet<T, I>
{
	type Vote = pallet_conviction_voting::VotingOf<T, I>;
	type AccountVote =
		pallet_conviction_voting::AccountVote<Self::Balance>;
	type Index = pallet_conviction_voting::PollIndexOf<T, I>;
	type Balance = pallet_conviction_voting::BalanceOf<T, I>;
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
}

impl<T: frame_system::Config + pallet_referenda::Config<I>, I: 'static> ReferendumTrait<AccountIdOf<T>>
	for pallet_referenda::Pallet<T, I>
where
	<T as pallet_referenda::Config<I>>::RuntimeCall: Sync + Send,
{
	type Index = pallet_referenda::ReferendumIndex;
	type Proposal = Bounded<
		<T as pallet_referenda::Config<I>>::RuntimeCall,
		<T as frame_system::Config>::Hashing,
	>;
	type ProposalOrigin =
		<<T as frame_system::Config>::RuntimeOrigin as OriginTrait>::PalletsOrigin;
	type ReferendumInfo = pallet_referenda::ReferendumInfoOf<T, I>;
	type Moment = <T::BlockNumberProvider as BlockNumberProvider>::BlockNumber;

	fn submit_proposal(
		who: AccountIdOf<T>,
		proposal: Self::Proposal,
		proposal_origin: Box<Self::ProposalOrigin>,
		enactment_moment: DispatchTime<Self::Moment>,
	) -> DispatchResult{
		/*let _ = pallet_referenda::Pallet::<T, I>::submit(
			origin,
			proposal_origin,
			proposal,
			enactment_moment,
		);*/
		if let (Some(preimage_len), Some(proposal_len)) =
				(proposal.lookup_hash().and_then(|h| T::Preimages::len(&h)), proposal.lookup_len())
			{
				if preimage_len != proposal_len {
					return Err(pallet_referenda::Error::<T, I>::PreimageStoredWithDifferentLength.into())
				}
			}
			let track =
			T::Tracks::track_for(&proposal_origin).map_err(|_| pallet_referenda::Error::<T, I>::NoTrack)?;
		T::Currency::reserve(&who, T::SubmissionDeposit::get())?;
		let amount = T::SubmissionDeposit::get();
		let submission_deposit = Deposit { who, amount};
		let index = pallet_referenda::ReferendumCount::<T, I>::mutate(|x| {
			let r = *x;
			*x += 1;
			r
		});
		let now = T::BlockNumberProvider::current_block_number();
		let nudge_call =
			T::Preimages::bound(<<T as pallet_referenda::Config<I>>::RuntimeCall>::from(pallet_referenda::Call::nudge_referendum { index }))?;
		

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
		debug_assert!(
			result.is_ok(),
			"Unable to schedule a new alarm at #{:?} (now: #{:?}), scheduler error: `{:?}`",
			when,
			T::BlockNumberProvider::current_block_number(),
			result.unwrap_err(),
		);
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
			pallet_referenda::ReferendumInfoFor::<T, I>::insert(index, Self::ReferendumInfo::Ongoing(status));
			Ok(())
	}

	fn get_referendum_info(index: Self::Index) -> Option<Self::ReferendumInfo> {
		pallet_referenda::ReferendumInfoFor::<T, I>::get(index)
	}
}
