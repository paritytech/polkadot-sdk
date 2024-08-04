#![cfg_attr(not(feature = "std"), no_std)]

// Re-export all pallet parts, this is needed to properly import the pallet into the runtime.
pub use pallet::*;
mod functions;
mod types;
pub use types::*;

#[cfg(test)]
mod mock;

#[cfg(test)]
mod tests;

#[frame_support::pallet(dev_mode)]
pub mod pallet {
	use super::*;

	#[pallet::pallet]
	pub struct Pallet<T>(_);

	#[pallet::config]
	pub trait Config: frame_system::Config {
		/// Because this pallet emits events, it depends on the runtime's definition of an event.
		/// https://paritytech.github.io/polkadot-sdk/master/polkadot_sdk_docs/reference_docs/frame_runtime_types/index.html
		type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;

		/// Type to access the Balances Pallet.
		type NativeBalance: fungible::Inspect<Self::AccountId>
			+ fungible::Mutate<Self::AccountId>
			+ fungible::hold::Inspect<Self::AccountId>
			+ fungible::hold::Mutate<Self::AccountId, Reason = Self::RuntimeHoldReason>
			+ fungible::freeze::Inspect<Self::AccountId>
			+ fungible::freeze::Mutate<Self::AccountId>;

		/// Treasury account Id
		#[pallet::constant]
		type PotId: Get<PalletId>;

		type RuntimeHoldReason: From<HoldReason>;

		/// This the minimum required time period between project whitelisting
		/// and payment/reward_claim from the treasury.
		#[pallet::constant]
		type PaymentPeriod: Get<BlockNumberFor<Self>>;

		/// Maximum number projects that can be accepted by this pallet
		#[pallet::constant]
		type MaxProjects: Get<u32>;

		/// Epoch duration in blocks
		#[pallet::constant]
		type EpochDurationBlocks: Get<BlockNumberFor<Self>>;
	}

	/// A reason for placing a hold on funds.
	#[pallet::composite_enum]
	pub enum HoldReason {
		/// Funds are held for a given buffer time before payment
		#[codec(index = 0)]
		FundsReserved,
	}

	/// Number of spendings that have been executed so far.
	#[pallet::storage]
	pub type SpendingsCount<T: Config> = StorageValue<_, SpendingIndex, ValueQuery>;

	/// Executed spendings information.
	#[pallet::storage]
	pub type CompletedSpendings<T: Config> =
		StorageMap<_, Twox64Concat, SpendingIndex, SpendingInfo<T>, OptionQuery>;

	/// Spendings that still have to be completed.
	#[pallet::storage]
	pub type Spendings<T: Config> =
		StorageMap<_, Twox64Concat, SpendingIndex, SpendingInfo<T>, OptionQuery>;

	/// List of whitelisted projects to be rewarded
	#[pallet::storage]
	pub type Projects<T: Config> =
		StorageValue<_, BoundedVec<ProjectInfo<T>, T::MaxProjects>, ValueQuery>;

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {
		/// Reward successfully claimed
		RewardClaimed {
			when: BlockNumberFor<T>,
			amount: BalanceOf<T>,
			project_account: ProjectId<T>,
		},

		/// A Spending was created
		SpendingCreated {
			when: BlockNumberFor<T>,
			amount: BalanceOf<T>,
			project_account: ProjectId<T>,
		},
	}

	#[pallet::error]
	pub enum Error<T> {
		/// Not enough Funds in the Pot
		InsufficientPotReserves,
		/// The funds transfer operation failed
		TransferFailed,
		/// Spending or spending index does not exists
		InexistentSpending,
		/// No valid Account_id found
		NoValidAccount,
		/// No project available for funding
		NoProjectAvailable,
		/// The Funds transfer failed
		FailedSpendingOperation,
		/// Still not in claiming period
		NotClaimingPeriod,
		/// Funds locking failed
		FundsReserveFailed,
	}

	#[pallet::hooks]
	impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
		/// Weight: see `begin_block`
		fn on_initialize(n: BlockNumberFor<T>) -> Weight {
			Self::begin_block(n)
		}
	}

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		/// Reward Claim logic
		///
		/// ## Dispatch Origin
		///
		/// Must be signed
		///
		/// ## Details
		///
		/// From this extrinsic any user can claim a reward for a nominated/whitelisted project
		///
		/// ### Parameters
		/// - `project_account`: The account that will receive the reward
		///
		/// ### Errors
		/// - [`Error::<T>::InexistentSpending`]: Fungible asset creation failed
		/// - [`Error::<T>::NoValidAccount`]: Fungible Asset minting into the treasury account failed.
		/// - [`Error::<T>::NotClaimingPeriod`]: Rewards can be claimed only within the claiming period
		///  
		/// ## Events
		/// Emits [`Event::<T>::RewardClaimed`] if successful for a positive approval.
		///
		#[pallet::call_index(0)]
		pub fn claim_reward_for(
			origin: OriginFor<T>,
			project_account: ProjectId<T>,
		) -> DispatchResult {
			let _caller = ensure_signed(origin)?;
			let spending_indexes = Self::get_spending(project_account);
			let pot = Self::pot_account();
			for i in spending_indexes {
				let mut info = Spendings::<T>::get(i).ok_or(Error::<T>::InexistentSpending)?;
				let project_account =
					info.whitelisted_project.clone().ok_or(Error::<T>::NoValidAccount)?;
				let now = <frame_system::Pallet<T>>::block_number();

				// Check that we're within the claiming period
				ensure!(now > info.valid_from, Error::<T>::NotClaimingPeriod);
				// Unlock the funds
				T::NativeBalance::release(
					&HoldReason::FundsReserved.into(),
					&pot,
					info.amount,
					Precision::Exact,
				)?;
				// transfer the funds
				Self::spending(info.amount, project_account.clone(), i)?;

				// Update SpendingInfo claimed field in the storage
				Spendings::<T>::mutate(i, |val| {
					info.claimed = true;
					info.status = SpendingState::Completed;

					*val = Some(info.clone());
				});

				// Move completed spending to corresponding storage
				CompletedSpendings::<T>::insert(i, info.clone());
				Spendings::<T>::remove(i);

				Self::deposit_event(Event::RewardClaimed {
					when: now,
					amount: info.amount,
					project_account,
				});
			}
			Ok(())
		}
	}
}
