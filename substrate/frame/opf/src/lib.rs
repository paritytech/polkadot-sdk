#![cfg_attr(not(feature = "std"), no_std)]

// Re-export all pallet parts, this is needed to properly import the pallet into the runtime.
pub use pallet::*;
pub mod functions;
mod types;
pub use pallet_distribution as Distribution;
pub use types::*;

#[frame_support::pallet(dev_mode)]
pub mod pallet {
	use super::*;

	#[pallet::pallet]
	pub struct Pallet<T>(_);

	#[pallet::config]
	pub trait Config: frame_system::Config + Distribution::Config {
		type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;

		/// The minimum duration for which votes are locked
		#[pallet::constant]
		type VoteLockingPeriod: Get<BlockNumberFor<Self>>;

		/// The period after which nominations must be renewed
		#[pallet::constant]
		type NominationRenewalPeriod: Get<BlockNumberFor<Self>>;

		/// The maximum number of whitelisted projects per nomination round
		#[pallet::constant]
		type MaxWhitelistedProjects: Get<u32>;

		/// Time during which it is possible to cast a vote or change an existing vote.
		/// less than nomination period.
		#[pallet::constant]
		type VotingPeriod: Get<BlockNumberFor<Self>>;
	}

	#[pallet::storage]
	pub type WhiteListedProjectAccounts<T: Config> =
		StorageValue<_, BoundedVec<AccountIdOf<T>, T::MaxWhitelistedProjects>, ValueQuery>;

	/// Returns Votes Infos against (project_id, voter_id) key
	#[pallet::storage]
	pub type Votes<T: Config> = StorageDoubleMap<
		_,
		Blake2_128Concat,
		AccountIdOf<T>,
		Twox64Concat,
		AccountIdOf<T>,
		VoteInfo<T>,
		OptionQuery,
	>;

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {
		/// Reward successfully claimed
		RewardsAssigned { when: BlockNumberFor<T> },
	}

	#[pallet::error]
	pub enum Error<T> {
		/// This account is not connected to any WhiteListed Project.
		NotWhitelistedProject,

		/// The voting action failed.
		VoteFailed,

		/// No such voting data
		NoVoteData,

		/// Invalid Result
		InvalidResult,

		/// Maximum number of projects submission for distribution as been reached
		MaximumProjectsNumber,
	}

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		#[pallet::call_index(0)]
		pub fn dummy(origin: OriginFor<T>, project_account: AccountIdOf<T>) -> DispatchResult {
			Ok(())
		}
	}
}
