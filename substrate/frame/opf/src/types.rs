pub use super::*;

pub use frame_support::traits::tokens::{Precision, Preservation};
pub use frame_support::{
	pallet_prelude::*,
	traits::{fungible, fungibles, EnsureOrigin},
	PalletId, Serialize,
};
pub use frame_system::{pallet_prelude::*, RawOrigin};
pub use pallet_distribution::MutateHold;
pub use pallet_distribution::{AccountIdOf, BalanceOf, HoldReason, ProjectInfo};
pub use scale_info::prelude::vec::Vec;
pub use sp_runtime::traits::Saturating;
pub use sp_runtime::traits::{AccountIdConversion, Convert, StaticLookup, Zero};
pub use sp_runtime::Percent;
#[derive(Encode, Decode, Clone, PartialEq, MaxEncodedLen, RuntimeDebug, TypeInfo)]
#[scale_info(skip_type_params(T))]
pub struct VoteInfo<T: Config> {
	/// The amount of stake/slash placed on this vote.
	pub amount: BalanceOf<T>,

	/// Whether the vote is "fund" / "not fund"
	pub is_fund: bool,
}
