#![cfg_attr(not(feature = "std"), no_std)]

pub use pallet::*;
mod functions;
mod types;
pub use pallet_scheduler as Schedule;
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
		type RuntimeCall: Parameter
			+ Dispatchable<RuntimeOrigin = Self::RuntimeOrigin>
			+ From<Call<Self>>
			+ IsType<<Self as frame_system::Config>::RuntimeCall>
			+ From<frame_system::Call<Self>>;

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

		/// The preimage provider.
		type Preimages: QueryPreimage<H = Self::Hashing> + StorePreimage;

		/// The Scheduler.
		type Scheduler: ScheduleAnon<
				BlockNumberFor<Self>,
				CallOf<Self>,
				PalletsOriginOf<Self>,
				Hasher = Self::Hashing,
			> + ScheduleNamed<
				BlockNumberFor<Self>,
				CallOf<Self>,
				PalletsOriginOf<Self>,
				Hasher = Self::Hashing,
			>;

		/// Time period in which people can vote. 
	/// After the period has ended, the votes are counted (STOP THE COUNT) 
	/// and then the funds are distributed into Spends.
		#[pallet::constant]
		type VotingPeriod: Get<BlockNumberFor<Self>>;

		/// Maximum number projects that can be accepted by this pallet
		#[pallet::constant]
		type MaxProjects: Get<u32>;

		/// Time for claiming a Spend. 
	/// After the period has passed, a spend is thrown away 
	/// and the funds are available again for distribution in the pot.
		#[pallet::constant]
		type ClaimingPeriod: Get<BlockNumberFor<Self>>;

		/// Period after which all the votes are resetted.
		#[pallet::constant]
		type VoteValidityPeriod: Get<BlockNumberFor<Self>>;


		/// Weight information for extrinsics in this pallet.
		type WeightInfo: WeightInfo;
	}

	/// Spends that still have to be claimed.
	#[pallet::storage]
	pub(super) type Spends<T: Config> =
		CountedStorageMap<_, Twox64Concat, ProjectId<T>, SpendInfo<T>, OptionQuery>;

	/// List of whitelisted projects to be rewarded
	#[pallet::storage]
	pub type Projects<T: Config> =
		StorageValue<_, BoundedVec<ProjectInfo<T>, T::MaxProjects>, ValueQuery>;

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {
		/// Reward successfully claimed
		RewardClaimed { when: BlockNumberFor<T>, amount: BalanceOf<T>, project_id: ProjectId<T> },

		/// A Spend was created
		SpendCreated { when: BlockNumberFor<T>, amount: BalanceOf<T>, project_id: ProjectId<T> },

		/// Not yet in the claiming period
		NotClaimingPeriod { project_id: ProjectId<T>, claiming_period: BlockNumberFor<T> },

		/// Payment will be enacted for corresponding project
		WillBeEnacted { project_id: ProjectId<T> },
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

	/*#[pallet::hooks]
	impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
		fn on_initialize(n: BlockNumberFor<T>) -> Weight {
			Self::begin_block(n)
		}
	}*/
}