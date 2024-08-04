pub use super::*;

pub use frame_support::{
	pallet_prelude::*,
	traits::{
		fungible,
		fungible::{Inspect, Mutate, MutateHold},
		fungibles,
		tokens::{Precision, Preservation},
		EnsureOrigin,
	},
	PalletId, Serialize,
};
pub use frame_system::{pallet_prelude::*, RawOrigin};
pub use scale_info::prelude::vec::Vec;
pub use sp_runtime::traits::{AccountIdConversion, Convert, Saturating, StaticLookup, Zero};

pub type BalanceOf<T> = <<T as Config>::NativeBalance as fungible::Inspect<
	<T as frame_system::Config>::AccountId,
>>::Balance;
pub type AccountIdOf<T> = <T as frame_system::Config>::AccountId;
/// A reward index.
pub type SpendingIndex = u32;

pub type ProjectId<T> = AccountIdOf<T>;

/// The state of the payment claim.
#[derive(Encode, Decode, Clone, PartialEq, Eq, MaxEncodedLen, RuntimeDebug, TypeInfo, Default)]
pub enum SpendingState {
	/// Unclaimed
	#[default]
	Unclaimed,
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
	/// Corresponding project id
	pub whitelisted_project: Option<AccountIdOf<T>>,
	/// Has it been claimed?
	pub claimed: bool,
}

impl<T: Config> SpendingInfo<T> {
	pub fn new(whitelisted: ProjectInfo<T>) -> Self {
		let amount = whitelisted.amount;
		let whitelisted_project = Some(whitelisted.project_account);
		let claimed = false;
		let status = SpendingState::default();
		let valid_from =
			<frame_system::Pallet<T>>::block_number().saturating_add(T::PaymentPeriod::get());

		let spending = SpendingInfo { amount, valid_from, status, whitelisted_project, claimed };

		// Lock the necessary amount

		// Get the spending index
		let index = SpendingsCount::<T>::get();
		//Add it to the Spendings storage
		Spendings::<T>::insert(index, spending.clone());
		SpendingsCount::<T>::put(index + 1);

		spending
	}
}

#[derive(Encode, Decode, Clone, PartialEq, Eq, MaxEncodedLen, RuntimeDebug, TypeInfo)]
#[scale_info(skip_type_params(T))]
pub struct ProjectInfo<T: Config> {
	/// AcountId that will receive the payment.
	pub project_account: ProjectId<T>,

	/// Block at which the project was submitted for reward distribution
	pub submission_block: BlockNumberFor<T>,

	/// Amount to be lock & pay for this project
	pub amount: BalanceOf<T>,
}
