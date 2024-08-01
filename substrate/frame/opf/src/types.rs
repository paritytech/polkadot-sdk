pub use super::*;

pub use frame_support::{
	pallet_prelude::*,
	traits::{fungible, fungibles, EnsureOrigin},
	PalletId, Serialize,
};
pub use frame_system::{pallet_prelude::*, RawOrigin};
pub use scale_info::prelude::vec::Vec;
pub use sp_runtime::traits::Saturating;
pub use sp_runtime::traits::{AccountIdConversion, Convert, StaticLookup, Zero};
pub use pallet_distribution::{AccountIdOf, BalanceOf, ProjectInfo };

#[derive(Encode, Decode, Clone, RuntimeDebug, PartialEq, TypeInfo)]
#[scale_info(skip_type_params(T))]
pub struct VoteInfo<T: Config> {
    /// Voter account_id
    voter_id: AccountIdOf<T>,

    /// The amount of stake placed on this vote.
    amount: BalanceOf<T>,

    //conviction: Conviction,

    /// Whether the vote is "fund" / "not fund"
    is_fund: bool,

    /// To be unreserved upon removal of the stake.
	pub deposit: BalanceOf<T>,
  }