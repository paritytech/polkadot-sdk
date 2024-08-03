#![cfg_attr(not(feature = "std"), no_std)]

use codec::{Decode, Encode, MaxEncodedLen};
use frame_support::{pallet_prelude::*, traits::fungible, Parameter};
use frame_system::pallet_prelude::*;
use sp_runtime::{traits::Member, DispatchResult, RuntimeDebug};

pub use pallet::*;

pub type BalanceOf<T> = <<T as Config>::NativeBalance as fungible::Inspect<
	<T as frame_system::Config>::AccountId,
>>::Balance;

#[derive(Encode, Decode, Clone, PartialEq, MaxEncodedLen, RuntimeDebug, TypeInfo)]
#[scale_info(skip_type_params(T))]
pub struct ProjectVote<T: pallet::Config> {
	amount: BalanceOf<T>,
	conviction: T::Conviction,
	voted_at: BlockNumberFor<T>,
	is_fund: bool,
}

#[derive(Encode, Decode, Clone, PartialEq, MaxEncodedLen, RuntimeDebug, TypeInfo)]
#[scale_info(skip_type_params(T))]
pub struct ProjectInfo<T: pallet::Config> {
	whitelisted_block: BlockNumberFor<T>,
	last_claimed_block: BlockNumberFor<T>,
	unclaimed_reward: BalanceOf<T>,
	total_votes: u32,
}

#[frame_support::pallet]
pub mod pallet {
	use super::*;

	#[pallet::pallet]
	pub struct Pallet<T>(_);

	#[pallet::config]
	pub trait Config: frame_system::Config {
		type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;

		type NativeBalance: fungible::Inspect<Self::AccountId>
			+ fungible::Mutate<Self::AccountId>
			+ fungible::hold::Inspect<Self::AccountId>
			+ fungible::hold::Mutate<Self::AccountId, Reason = Self::RuntimeHoldReason>
			+ fungible::freeze::Inspect<Self::AccountId>
			+ fungible::freeze::Mutate<Self::AccountId>;

		type Conviction: Parameter + Member + MaxEncodedLen + Decode + Encode;

		type BlockNumber: Parameter + Member + MaxEncodedLen + Decode + Encode;

		type RuntimeHoldReason: From<HoldReason>;

		/// The origin that can add new projects to the whitelist.
		type WhitelistProjectOrigin: EnsureOrigin<Self::RuntimeOrigin>;

		/// The type used to identify projects.
		type ProjectId: Parameter + Member + MaxEncodedLen;

		/// The account which will be funding the whitelisted projects.
		#[pallet::constant]
		type TreasuryAccountId: Get<Self::AccountId>;

		/// The minimum duration for which votes are locked.
		#[pallet::constant]
		type VoteLockingPeriod: Get<BlockNumberFor<Self>>;

		/// The period after which votes must be renewed.
		#[pallet::constant]
		type VoteRenewalPeriod: Get<BlockNumberFor<Self>>;

		/// The number of blocks between funding periods.
		#[pallet::constant]
		type FundingPeriod: Get<BlockNumberFor<Self>>;

		/// The maximum number of projects that can be whitelisted.
		#[pallet::constant]
		type MaxProjects: Get<u32>;
	}

	/// A reason for placing a hold on funds.
	#[pallet::composite_enum]
	pub enum HoldReason {}

	/// List of whitelisted projects to be funded.
	#[pallet::storage]
	pub type WhitelistedProjects<T: Config> =
		StorageValue<_, BoundedVec<ProjectInfo<T>, T::MaxProjects>, ValueQuery>;

	/// Votes for projects.
	#[pallet::storage]
	pub type Votes<T: Config> = StorageDoubleMap<
		_,
		Blake2_128Concat,
		T::AccountId,
		Blake2_128Concat,
		T::ProjectId,
		ProjectVote<T>,
	>;

	/// The starting block number of the current voting period.
	#[pallet::storage]
	pub type CurrentVotingPeriodStartingBlock<T: Config> =
		StorageValue<_, BlockNumberFor<T>, ValueQuery>;

	#[pallet::event]
	pub enum Event<T: Config> {}

	#[pallet::error]
	pub enum Error<T> {}

	#[pallet::hooks]
	impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
		fn on_initialize(n: BlockNumberFor<T>) -> Weight {
			// check if current block is the start of a new voting period

			// distribute rewards for the previous voting period by updating project info

			0.into()
		}
	}

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		#[pallet::call_index(0)]
		#[pallet::weight(0)]
		pub fn vote_project(
			origin: OriginFor<T>,
			project_id: T::ProjectId,
			amount: BalanceOf<T>,
			conviction: T::Conviction,
			is_fund: bool,
		) -> DispatchResult {
			// check if project_id is valid

			// set the vote data

			// try to hold user funds based on conviction
			// if user has voted before, update hold amount instead

			Ok(())
		}

		#[pallet::call_index(1)]
		#[pallet::weight(0)]
		pub fn remove_vote(
			origin: OriginFor<T>,
			project_id: T::ProjectId,
			amount: BalanceOf<T>,
		) -> DispatchResult {
			// check if project_id is valid

			// set the vote data

			// try to release `amount` from the user's hold

			Ok(())
		}

		#[pallet::call_index(2)]
		#[pallet::weight(0)]
		pub fn whitelist_project(origin: OriginFor<T>, project_id: T::ProjectId) -> DispatchResult {
			// check origin is whitelist origin

			// ensure project has not been whitelisted before

			// add to project list

			Ok(())
		}

		#[pallet::call_index(3)]
		#[pallet::weight(0)]
		pub fn remove_whitelist_project(
			origin: OriginFor<T>,
			project_id: T::ProjectId,
		) -> DispatchResult {
			// check origin is whitelist origin

			// ensure project has been whitelisted before

			// remove from project list

			Ok(())
		}

		#[pallet::call_index(4)]
		#[pallet::weight(0)]
		pub fn claim_reward(
			origin: OriginFor<T>,
			project_id: T::ProjectId,
			amount: BalanceOf<T>,
		) -> DispatchResult {
			// check origin is project id

			// ensure project is whitelisted
			// current block > project.whitelisted_block

			// ensure project has not claimed in the previous voting period
			// current block > project.last_claimed_block
			// current block < project.last_claimed_block + voting_period

			// update project info

			// release funds for project

			Ok(())
		}
	}
}
