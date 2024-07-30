
#![cfg_attr(not(feature = "std"), no_std)]



// Re-export all pallet parts, this is needed to properly import the pallet into the runtime.
pub use pallet::*;
mod types;
mod functions;
pub use types::*;

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
		type PotId: Get<PalletId>;

		type RuntimeHoldReason: From<HoldReason>;

		/// This the minimum required time period between project whitelisting
		/// and payment/reward_claim from the treasury.
		type PaymentPeriod: Get<BlockNumberFor<Self>>;
		
		/// Maximum number projects that can be accepted by this pallet 
		type MaxProjects: Get<u32>;

		/// Epoch duration in blocks
		type EpochDurationBlocks: Get<BlockNumberFor<Self>>;


	}
	
	/// A reason for placing a hold on funds.
	#[pallet::composite_enum]
	pub enum HoldReason {
		/// Funds are held for a given buffer time before payment
		#[codec(index = 0)]
		FundsLock,
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
		/// We usually use passive tense for events.
		SomethingStored { something: u32, who: T::AccountId },

		/// Reward successfully claimed
		RewardClaimed {
			when: BlockNumberFor<T>,
			amount: BalanceOf<T>,
			project_account: T::AccountId,
		}
		
	}

	#[pallet::error]
	pub enum Error<T> {
		/// There was an attempt to increment the value in storage over `u32::MAX`.
		StorageOverflow,
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
		LockFailed,
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


		// ToDo: Add `claim_reward_for` 
		#[pallet::call_index(0)]
		pub fn  claim_reward_for(origin: OriginFor<T>, project_account:T::AccountId) -> DispatchResult {
			let _caller = ensure_signed(origin)?;
			let spending_indexes = Self::get_spending(project_account);
			let pot = Self::pot_account();
			for i in spending_indexes {
				let mut info = Spendings::<T>::get(i).ok_or(Error::<T>::InexistentSpending)?;
				let project_account = info.whitelisted_project.clone().ok_or(Error::<T>::NoValidAccount)?;
				let now = <frame_system::Pallet<T>>::block_number();

				// Check that we're within the claiming period
				ensure!(now > info.valid_from, Error::<T>::NotClaimingPeriod);
					// Unlock the funds
					T::NativeBalance::release(
						&HoldReason::FundsLock.into(),
						&pot,
						info.amount,
						Precision::Exact,
					)?;
					// transfer the funds
					Self::spending(info.amount, project_account.clone(), i)?;

					// Update SpendingInfo claimed field in the storage
					Spendings::<T>::mutate(i, |val|{
						info.claimed = true;
							info.status = SpendingState::Completed;					
			
						*val = Some(info.clone());
					});

					// Move completed spending to corresponding storage

					CompletedSpendings::<T>::insert(i,info.clone()); 
					Spendings::<T>::remove(i);

					Self::deposit_event(
						Event::RewardClaimed {
							when: now,
							amount: info.amount,
							project_account,
						}
					);
			}
			Ok(())			
			
		}

	}




	
}