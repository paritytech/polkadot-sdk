#![cfg_attr(not(feature = "std"), no_std)]

// Re-export all pallet parts, this is needed to properly import the pallet into the runtime.
pub use pallet::*;
mod functions;
mod types;
pub use types::*;

#[cfg(feature = "runtime-benchmarks")]
mod benchmarking;

#[cfg(test)]
mod mock;

#[cfg(test)]
mod tests;
pub mod weights;

#[frame_support::pallet]
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

		/// Provider for the block number.
		type BlockNumberProvider: BlockNumberProvider<BlockNumber = BlockNumberFor<Self>>;

		/// Treasury account Id
		#[pallet::constant]
		type PotId: Get<PalletId>;

		type RuntimeHoldReason: From<HoldReason>;

		/// This the minimum required buffer time period between project nomination
		/// and payment/reward_claim from the treasury.
		#[pallet::constant]
		type BufferPeriod: Get<BlockNumberFor<Self>>;

		/// Maximum number projects that can be accepted by this pallet
		#[pallet::constant]
		type MaxProjects: Get<u32>;

		/// Epoch duration in blocks
		#[pallet::constant]
		type EpochDurationBlocks: Get<BlockNumberFor<Self>>;

		/// Weight information for extrinsics in this pallet.
		type WeightInfo: WeightInfo;
	}

	/// A reason for placing a hold on funds.
	#[pallet::composite_enum]
	pub enum HoldReason {
		/// Funds are held for a given buffer time before payment
		#[codec(index = 0)]
		FundsReserved,
	}

	/// Number of Spends that have been executed so far.
	#[pallet::storage]
	pub(super) type SpendsCount<T: Config> = StorageValue<_, SpendIndex, ValueQuery>;

	/// Spends that still have to be completed.
	#[pallet::storage]
	pub(super) type Spends<T: Config> =
		StorageMap<_, Twox64Concat, SpendIndex, SpendInfo<T>, OptionQuery>;

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

		/// A Spend was created
		SpendCreated {
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
		/// Spend or Spend index does not exists
		InexistentSpend,
		/// No valid Account_id found
		NoValidAccount,
		/// No project available for funding
		NoProjectAvailable,
		/// The Funds transfer failed
		FailedSpendOperation,
		/// Still not in claiming period
		NotClaimingPeriod,
		/// Funds locking failed
		FundsReserveFailed,
		/// An invalid result  was returned
		InvalidResult,
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
		/// OPF Reward Claim logic
		///
		/// ## Dispatch Origin
		///
		/// Must be signed
		///
		/// ## Details
		///
		/// From this extrinsic any user can claim a reward for a nominated/whitelisted project.
		///
		/// ### Parameters
		/// - `project_account`: The account that will receive the reward.
		///
		/// ### Errors
		/// - [`Error::<T>::InexistentSpend`]:Spend or Spend index does not exists
		/// - [`Error::<T>::NoValidAccount`]:  No valid Account_id found
		/// - [`Error::<T>::NotClaimingPeriod`]: Still not in claiming period
		///  
		/// ## Events
		/// Emits [`Event::<T>::RewardClaimed`] if successful for a positive approval.
		#[pallet::call_index(0)]
		#[pallet::weight(T::WeightInfo::claim_reward_for(T::MaxProjects::get()))]
		pub fn claim_reward_for(
			origin: OriginFor<T>,
			project_account: ProjectId<T>,
		) -> DispatchResult {
			let _caller = ensure_signed(origin)?;
			let spend_indexes = Self::get_spend(project_account);
			let pot = Self::pot_account();
			for i in spend_indexes {
				let info = Spends::<T>::get(i).ok_or(Error::<T>::InexistentSpend)?;
				let project_account =
					info.whitelisted_project.clone().ok_or(Error::<T>::NoValidAccount)?;
				let now = T::BlockNumberProvider::current_block_number();

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
				Self::spend(info.amount, project_account.clone(), i)?;

				// Update SpendInfo claimed field in the storage
				let mut infos = Spends::<T>::get(i).ok_or(Error::<T>::InexistentSpend)?;
				Spends::<T>::remove(i);
				infos.status = SpendState::Completed;

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
