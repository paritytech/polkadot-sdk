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

		/// The maximum number of whitelisted projects per nomination round
		#[pallet::constant]
		type MaxWhitelistedProjects: Get<u32>;

		/// Time during which it is possible to cast a vote or change an existing vote.
		/// less than nomination period.
		#[pallet::constant]
		type VotingPeriod: Get<BlockNumberFor<Self>>;
	}

	/// Number of Voting Rounds executed so far
	#[pallet::storage]
	pub type VotingRoundsNumber<T:Config> = StorageValue<_,u32, ValueQuery>;

	/// Returns Infos about a Voting Round agains the Voting Round index
	#[pallet::storage]
	pub type VotingRounds<T:Config> = StorageMap<_,Twox64Concat, RoundIndex, VotingRoundInfo<T>, OptionQuery>;

	/// Returns a list of Whitelisted Project accounts
	#[pallet::storage]
	pub type WhiteListedProjectAccounts<T: Config> =
		StorageValue<_, BoundedVec<ProjectId<T>, T::MaxWhitelistedProjects>, ValueQuery>;

	/// Returns Votes Infos against (project_id, voter_id) key
	#[pallet::storage]
	pub type Votes<T: Config> = StorageDoubleMap<
		_,
		Blake2_128Concat,
		ProjectId<T>,
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

		/// An invalid result  was returned
		InvalidResult,

		/// Maximum number of projects submission for distribution as been reached
		MaximumProjectsNumber,

		/// This voting round does not exists
		NoRoundFound,

		/// Voting period closed for this round
		VotePeriodClosed,

		/// Not enough funds to vote, you need to decrease your stake
		NotEnoughFunds
	}

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		#[pallet::call_index(0)]
		pub fn vote(origin: OriginFor<T>, project_account: ProjectId<T>, amount: BalanceOf<T>, is_fund: bool) -> DispatchResult {
			// Get current voting round & check if we are in voting period or not
			// Check that voter has enough funds to vote
			// Vote action executed
			Ok(())
		}
		#[pallet::call_index(1)]
		pub fn remove_vote(origin: OriginFor<T>, project_account: ProjectId<T>, amount: BalanceOf<T>, is_fund: bool) -> DispatchResult {
			// Get current voting round & check if we are in voting period or not
			// Removal action executed
			Ok(())
		}
	}
}
