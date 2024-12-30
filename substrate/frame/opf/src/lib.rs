// This file is part of Substrate.

// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
// 	http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

#![cfg_attr(not(feature = "std"), no_std)]

pub use pallet::*;
mod types;
mod functions;
pub use pallet_scheduler as Schedule;
pub use types::*;

#[frame_support::pallet(dev_mode)]
pub mod pallet {
	use super::*;
	use frame_system::WeightInfo;

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
			+ fungible::Mutate<Self::AccountId>;

		/// Provider for the block number.
		type BlockNumberProvider: BlockNumberProvider;

		/// Treasury account Id
		#[pallet::constant]
		type PotId: Get<PalletId>;

		/// The preimage provider.
		type Preimages: QueryPreimage<H = Self::Hashing> + StorePreimage;

		/// The Scheduler.
		type Scheduler: ScheduleAnon<
		ProvidedBlockNumberFor<Self>,
				CallOf<Self>,
				PalletsOriginOf<Self>,
				Hasher = Self::Hashing,
			> + ScheduleNamed<
			ProvidedBlockNumberFor<Self>,
				CallOf<Self>,
				PalletsOriginOf<Self>,
				Hasher = Self::Hashing,
			>;

		/// Time period in which people can vote. 
	/// After the period has ended, the votes are counted (STOP THE COUNT) 
	/// and then the funds are distributed into Spends.
		#[pallet::constant]
		type VotingPeriod: Get<ProvidedBlockNumberFor<Self>>;

		/// Maximum number projects that can be accepted by this pallet
		#[pallet::constant]
		type MaxProjects: Get<u32>;

		/// Time for claiming a Spend. 
	/// After the period has passed, a spend is thrown away 
	/// and the funds are available again for distribution in the pot.
		#[pallet::constant]
		type ClaimingPeriod: Get<ProvidedBlockNumberFor<Self>>;

		/// Period after which all the votes are reset.
		#[pallet::constant]
		type VoteValidityPeriod: Get<ProvidedBlockNumberFor<Self>>;


		/// Weight information for extrinsics in this pallet.
		type WeightInfo: WeightInfo;
	}

	/// Number of Voting Rounds executed so far
	#[pallet::storage]
	pub type VotingRoundNumber<T: Config> = StorageValue<_, u32, ValueQuery>;

	/// Returns Infos about a Voting Round agains the Voting Round index
	#[pallet::storage]
	pub type VotingRounds<T: Config> =
		StorageMap<_, Twox64Concat, RoundIndex, VotingRoundInfo<T>, OptionQuery>;

	/// Spends that still have to be claimed.
	#[pallet::storage]
	pub(super) type Spends<T: Config> =
		CountedStorageMap<_, Twox64Concat, ProjectId<T>, SpendInfo<T>, OptionQuery>;

	/// List of whitelisted projects to be rewarded
	#[pallet::storage]
	pub type Projects<T: Config> =
		StorageValue<_, BoundedVec<ProjectInfo<T>, T::MaxProjects>, ValueQuery>;
	
	/// List of Whitelisted Project registered
	#[pallet::storage]
	pub type WhiteListedProjectAccounts<T: Config> =
		StorageValue<_, BoundedVec<ProjectId<T>, T::MaxProjects>, ValueQuery>;
	
	/// Returns (positive_funds,negative_funds) of Whitelisted Project accounts
	#[pallet::storage]
	pub type ProjectFunds<T: Config> = StorageMap<
		_,
		Twox64Concat,
		ProjectId<T>,
		BoundedVec<BalanceOf<T>, ConstU32<2>>,
		ValueQuery,
	>;

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {
		/// Reward successfully claimed
		RewardClaimed { when: ProvidedBlockNumberFor<T>, amount: BalanceOf<T>, project_id: ProjectId<T> },

		/// A Spend was created
		SpendCreated { when: ProvidedBlockNumberFor<T>, amount: BalanceOf<T>, project_id: ProjectId<T> },

		/// Not yet in the claiming period
		NotClaimingPeriod { project_id: ProjectId<T>, claiming_period: ProvidedBlockNumberFor<T> },

		/// Payment will be enacted for corresponding project
		WillBeEnacted { project_id: ProjectId<T> },

		/// Reward successfully assigned
		RewardsAssigned { when: ProvidedBlockNumberFor<T> },

		/// User's vote successfully submitted
		VoteCasted { who: VoterId<T>, when: ProvidedBlockNumberFor<T>, project_id: ProjectId<T> },

		/// User's vote successfully removed
		VoteRemoved { who: VoterId<T>, when: ProvidedBlockNumberFor<T>, project_id: ProjectId<T> },

		/// Project added to whitelisted projects list
		Projectlisted { when:ProvidedBlockNumberFor<T>, project_id: ProjectId<T> },

		/// Project removed from whitelisted projects list
		ProjectUnlisted { when:ProvidedBlockNumberFor<T>, project_id: ProjectId<T> },

		/// Project Funding Accepted by voters
		ProjectFundingAccepted {
			project_id: ProjectId<T>,
			when:ProvidedBlockNumberFor<T>,
			round_number: u32,
			amount: BalanceOf<T>,
		},

		/// Project Funding rejected by voters
		ProjectFundingRejected { when:ProvidedBlockNumberFor<T>, project_id: ProjectId<T> },

		/// A new voting round started
		VotingRoundStarted { when:ProvidedBlockNumberFor<T>, round_number: u32 },

		/// The users voting period ended. Reward calculation will start.
		VoteActionLocked { when:ProvidedBlockNumberFor<T>, round_number: u32 },

		/// The voting round ended
		VotingRoundEnded { when:ProvidedBlockNumberFor<T>, round_number: u32 },
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
		/// The reward calculation failed due to an internal error
		FailedRewardCalculation,
		/// Voting round is over
		VotingRoundOver,
		/// This voting round does not exists
		NoRoundFound,
		/// Maximum number of projects submission for reward distribution as been reached
		MaximumProjectsNumber,
	}

	/*#[pallet::hooks]
	impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
		fn on_initialize(n:ProvidedBlockNumberFor<T>) -> Weight {
			Self::begin_block(n)
		}
	}*/

	#[pallet::call]
	impl<T: Config> Pallet<T> {

		#[pallet::call_index(0)]
		#[transactional]
		pub fn register_project(origin: OriginFor<T>, project_id: ProjectId<T>) -> DispatchResult{
			let _caller = ensure_signed(origin)?;
			let when = T::BlockNumberProvider::current_block_number();
			Self::register_new(project_id.clone())?;
			Self::deposit_event(Event::Projectlisted {
				when,
				project_id,
			});
			Ok(())
		}

		#[pallet::call_index(1)]
		#[transactional]
		pub fn unregister_project(origin: OriginFor<T>, project_id:  ProjectId<T> ) -> DispatchResult {
			ensure_root(origin)?;
			let when = T::BlockNumberProvider::current_block_number();
			Self::unlist_project(project_id.clone())?;
			Self::deposit_event(Event::<T>::ProjectUnlisted { when, project_id });

			Ok(())
		}

		#[pallet::call_index(2)]
		#[transactional]
		pub fn vote(origin: OriginFor<T>) -> DispatchResult {
			Ok(())
		}

		#[pallet::call_index(3)]
		#[transactional]
		pub fn remove_vote(origin: OriginFor<T>) -> DispatchResult {
			Ok(())
		}

		#[pallet::call_index(4)]
		#[transactional]
		pub fn claim_reward_for(origin: OriginFor<T>, project_id: ProjectId<T>) -> DispatchResult {
			Ok(())
		}

		#[pallet::call_index(5)]
		#[transactional]
		pub fn execute_claim(origin: OriginFor<T>, project_id: ProjectId<T>) -> DispatchResult {
			Ok(())
		}

	}
}