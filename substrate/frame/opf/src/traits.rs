pub use super::*;

pub trait ReferendumTrait {
	type Index: Parameter + Member + Ord + PartialOrd + Copy + HasCompact + MaxEncodedLen;
	type Proposal: Parameter + Member + MaxEncodedLen;
	type ProposalOrigin: Parameter + Member + MaxEncodedLen;
	type OriginFor;
    type ReferendumInfo: Eq + PartialEq + Debug + Encode + Decode + TypeInfo + Clone;
	type Moment;

	fn submit_proposal(
		origin: Self::OriginFor,
		proposal: Self::Proposal,
		proposal_origin: Box<Self::ProposalOrigin>,
		enactment_moment: DispatchTime<Self::Moment>,
	) -> Self::Index;

    fn get_referendum_info(index: Self::Index) -> Option<Self::ReferendumInfo>;
}

pub trait ConvictionVotingTrait {
	type Vote;
	type AccountVote;
	type Conviction: Parameter + Member + Ord + PartialOrd + Copy + MaxEncodedLen;
	type Index: Parameter + Member + Ord + PartialOrd + Copy + HasCompact + MaxEncodedLen;
	type Moment;

	/*fn try_vote(ref_index: Self::Index, vote: Self::AccountVote) -> Result<(), ()>;
	fn try_remove_vote(ref_index: Self::Index) -> Result<(), ()>;*/
}

impl<T:pallet_conviction_voting::Config<I>, I: 'static> ConvictionVotingTrait
	for pallet_conviction_voting::Pallet<T, I>{
		type Vote = pallet_conviction_voting::VotingOf<T, I>;
		type AccountVote = pallet_conviction_voting::AccountVote<pallet_conviction_voting::BalanceOf<T,I>> ;
		type Conviction = pallet_conviction_voting::Conviction;
		type Index = pallet_conviction_voting::PollIndexOf<T, I>;
		type Moment = <T::BlockNumberProvider as BlockNumberProvider>::BlockNumber;
	}

impl<T: frame_system::Config + pallet_referenda::Config<I>, I: 'static> ReferendumTrait
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
	type OriginFor = OriginFor<T>;
    type ReferendumInfo = pallet_referenda::ReferendumInfoOf<T, I>;
	type Moment = <T::BlockNumberProvider as BlockNumberProvider>::BlockNumber;

	fn submit_proposal(
		origin: Self::OriginFor,
		proposal: Self::Proposal,
		proposal_origin: Box<Self::ProposalOrigin>,
		enactment_moment: DispatchTime<Self::Moment>,
	) -> Self::Index {
		let _ = pallet_referenda::Pallet::<T, I>::submit(
			origin,
			proposal_origin,
			proposal,
			enactment_moment,
		);
		pallet_referenda::ReferendumCount::<T, I>::get()
	}

    fn get_referendum_info(index: Self::Index) -> Option<Self::ReferendumInfo> {    
        pallet_referenda::ReferendumInfoFor::<T, I>::get(index)
    } 
}
