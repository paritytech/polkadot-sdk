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

/// A reward index.
pub type SpendingIndex = u32;

/// The state of the payment claim.
#[derive(Encode, Decode, Clone, PartialEq, Eq, MaxEncodedLen, RuntimeDebug, TypeInfo, Default)]
pub enum SpendingState {
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


//Processed Reward status
#[derive(Encode, Decode, Clone, PartialEq, MaxEncodedLen, RuntimeDebug, TypeInfo)]
#[scale_info(skip_type_params(T))]
pub struct SpendingInfo<T: Config> {	
	/// The asset amount of the spend.
	pub amount: BalanceOf<T>,
	/// The block number from which the spend can be claimed(24h after SpendStatus Creation).
	pub valid_from: BlockNumberFor<T>,
	/// The status of the payout/claim.
	pub status: SpendingState,
	/// Corresponding proposal_id
	pub whitelisted_project: Option<T::AccountId>,
	/// Has it been claimed?
	pub claimed: bool,
	/// Amount paid
	pub paid: BalanceOf<T>,
}

impl<T: Config> SpendingInfo<T> {
	pub fn new(
		whitelisted: ProjectInfo<T>,
	) -> Self {
		let amount = whitelisted.amount;
		let whitelisted_project = Some(whitelisted.project_account);
		let paid = Zero::zero();
		let claimed = false;
		let status = SpendingState::default();
		let valid_from = 
				<frame_system::Pallet<T>>::block_number().saturating_add(T::PaymentPeriod::get());
		
		let spending = SpendingInfo{
			amount,
			valid_from,
			status,
			whitelisted_project,
			claimed,
			paid,
		};
		// Get the spending index 
		let index = SpendingsCount::<T>::get();
		Spendings::<T>::insert(index, spending.clone());
		SpendingsCount::<T>::put(index+1);

		spending

	}
}



#[derive(Encode, Decode, Clone, PartialEq, Eq, MaxEncodedLen, RuntimeDebug, TypeInfo)]
#[scale_info(skip_type_params(T))]
pub struct ProjectInfo<T: Config>  {
	/// AcountId that will receive the payment.
	project_account: T::AccountId,

	/// Block at which the project was whitelisted
	whitelisted_block: BlockNumberFor<T>,

	/// Amount to be lock & pay for this project 
	amount: BalanceOf<T>,

	/// Has the payment been executed already?
	reward_paid: bool,
  }