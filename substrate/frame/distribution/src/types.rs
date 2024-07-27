pub use super::*;

pub use frame_support::traits::fungible::MutateHold;
pub use frame_support::traits::fungibles::{metadata, Inspect, Mutate};
pub use frame_support::traits::tokens::{Precision, Preservation};
pub use frame_support::traits::UnfilteredDispatchable;
pub use frame_support::{
	pallet_prelude::*,
	traits::{fungible, fungibles, EnsureOrigin},
	PalletId, Serialize,
};
pub use frame_system::{pallet_prelude::*, RawOrigin};
pub use scale_info::prelude::vec::Vec;
pub use sp_runtime::traits::Saturating;
pub use sp_runtime::traits::{AccountIdConversion, Convert, StaticLookup, Zero};

pub type BalanceOf<T> = <<T as Config>::NativeBalance as fungible::Inspect<
	<T as frame_system::Config>::AccountId,
>>::Balance;

/// The state of the payment claim.
#[derive(Encode, Decode, Clone, PartialEq, Eq, MaxEncodedLen, RuntimeDebug, TypeInfo, Default)]
pub enum PaymentState {
	/// Unclaimed
	#[default]
	Unclaimed,
    /// Claimed & Pending.
	Pending,
	/// Claimed & Paid.
	Completed,
	/// Claimed but Failed.
	Failed,
}


//Processed Spending status
#[derive(Encode, Decode, Clone, PartialEq, MaxEncodedLen, RuntimeDebug, TypeInfo)]
#[scale_info(skip_type_params(T))]
pub struct SpendStatus<T: Config> {	
	/// The asset amount of the spend.
	pub amount: BalanceOf<T>,
	/// The project beneficiary of the spend.
	pub project_id: u32,
	/// The block number from which the spend can be claimed(24h after SpendStatus Creation).
	pub valid_from: BlockNumberFor<T>,
	/// The status of the payout/claim.
	pub status: PaymentState,
	/// Corresponding proposal_id
	pub proposal_id: u32,
	/// Has it been claimed?
	pub spending_claimed: bool,
	/// Amount already payed
	pub paid: BalanceOf<T>,
}

#[derive(Encode, Decode, Clone, PartialEq, Eq, MaxEncodedLen, RuntimeDebug, TypeInfo)]
#[scale_info(skip_type_params(T))]
struct ProjectInfo<T: Config>  {
	project_account: Option<T::AccountId>,
	whitelisted_block: BlockNumberFor<T>,
	requested_amount: BalanceOf<T>,
	distributed: bool,
  }