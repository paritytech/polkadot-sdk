pub use super::*;

pub use frame_support::traits::tokens::{Precision, Preservation};
pub use frame_support::{
	pallet_prelude::*,
	traits::{fungible, fungibles, EnsureOrigin},
	PalletId, Serialize,
};
pub use frame_system::{pallet_prelude::*, RawOrigin};
pub use pallet_distribution::MutateHold;
pub use pallet_distribution::{AccountIdOf, BalanceOf, HoldReason, ProjectInfo, ProjectId};
pub use scale_info::prelude::vec::Vec;
pub use sp_runtime::traits::Saturating;
pub use sp_runtime::traits::{AccountIdConversion, Convert, StaticLookup, Zero,CheckedAdd};
pub use sp_runtime::Percent;

pub type RoundIndex = u32; 

#[derive(Encode, Decode, Clone, PartialEq, MaxEncodedLen, RuntimeDebug, TypeInfo)]
#[scale_info(skip_type_params(T))]
pub struct VoteInfo<T: Config> {
	/// The amount of stake/slash placed on this vote.
	pub amount: BalanceOf<T>,

	/// Round at which the vote was casted
	pub round: VotingRoundInfo<T>,

	/// Whether the vote is "fund" / "not fund"
	pub is_fund: bool,
}


/// Voting rounds are periodically created inside a hook on_initialize (use poll in the future)
#[derive(Encode, Decode, Clone, PartialEq, MaxEncodedLen, RuntimeDebug, TypeInfo)]
#[scale_info(skip_type_params(T))]
pub struct VotingRoundInfo<T: Config>{
	pub round_number: u32,
	pub round_starting_block: BlockNumberFor<T>,
	pub voting_locked_block: BlockNumberFor<T>,
	pub round_ending_block: BlockNumberFor<T>,
}

impl<T: Config> VotingRoundInfo<T>{
	pub fn new() -> Self{
		let round_starting_block = <frame_system::Pallet<T>>::block_number();
		let voting_locked_block = round_starting_block.clone().checked_add(&T::VotingPeriod::get()).expect("Failed Operation");
		let round_ending_block = voting_locked_block.clone().checked_add(&T::VoteLockingPeriod::get()).expect("Failed Operation");
		let round_number = VotingRoundsNumber::<T>::get();
		let new_number = round_number.checked_add(1).expect("Failed Operation");
		VotingRoundsNumber::<T>::put(new_number);

		VotingRoundInfo{round_number, round_starting_block, voting_locked_block, round_ending_block}
	}
}
