pub use super::*;

pub trait ReferendumTrait {
    type Index: Parameter + Member + Ord + PartialOrd + Copy + HasCompact + MaxEncodedLen;
    type Proposal: Parameter + Member + Ord + PartialOrd + Copy + HasCompact + MaxEncodedLen;
    type Moment;

    fn submit_proposal(proposal: Self::Proposal) -> Self::Index;
    fn create_ongoing(ref_index: Self::Index) -> Result<(), ()>;

}

pub trait ConvictionVotingTrait {
    type AccountVote: Parameter + Member + Ord + PartialOrd + Copy + HasCompact + MaxEncodedLen;
    type Index: Parameter + Member + Ord + PartialOrd + Copy + HasCompact + MaxEncodedLen;
    type Moment;

    fn try_vote(ref_index: Self::Index, vote:Self::AccountVote) -> Result<(), ()>;
    fn try_remove_vote(ref_index: Self::Index) -> Result<(), ()>;

}