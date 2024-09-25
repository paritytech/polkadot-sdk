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

//! OPF pallet.
//!
//! The OPF Pallet handles the Optimistic Project Funding.
//! It allows users to nominate projects (whitelisted in OpenGov) with their DOT.
//!
//! ## Overview
//!
//! This mechanism will be funded with a constant stream of DOT taken directly from inflation
//! and distributed to projects based on the proportion of DOT that has nominated them.
//! The project rewards distribution is handled by the Distribution Pallet.
//!
//! ### Terminology
//!
//! - **MaxWhitelistedProjects:** Maximum number of Whitelisted projects that can be handled by the
//!   pallet.
//! - **VoteLockingPeriod:** Period during which voting is disabled.
//! - **VotingPeriod:**Period during which voting is enabled.
//! - **TemporaryRewards:**For test purposes only ⇒ used as a substitute for the inflation portion
//!   used for the rewards.
//!
//! ## Interface
//!
//! ### Permissionless Functions
//!
//! ### Privileged Functions
//!
//! * `vote`: Allows users to [vote for/nominate] a whitelisted project using their funds.
//! * `remove_vote`: Allows users to remove a casted vote.
//! * `unlock_funds`: Allows users to unlock funds related to a specific project.

#![cfg_attr(not(feature = "std"), no_std)]

// Re-export all pallet parts, this is needed to properly import the pallet into the runtime.
pub use pallet::*;
pub mod functions;
mod types;
pub use pallet_distribution as Distribution;
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
	pub trait Config: frame_system::Config + Distribution::Config {
		type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;

		/// The minimum duration for which votes are locked
		#[pallet::constant]
		type VoteLockingPeriod: Get<BlockNumberFor<Self>>;

		/// The maximum number of whitelisted projects per nomination round
		#[pallet::constant]
		type MaxWhitelistedProjects: Get<u32>;

		/// Time during which it is possible to cast a vote or change an existing vote.
		#[pallet::constant]
		type VotingPeriod: Get<BlockNumberFor<Self>>;

		/// Used for Pallet testing only. Represents the Total Reward distributed
		type TemporaryRewards: Get<BalanceOf<Self>>;

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

	/// Returns a list of Whitelisted Project accounts
	#[pallet::storage]
	pub type WhiteListedProjectAccounts<T: Config> =
		StorageValue<_, BoundedVec<ProjectId<T>, T::MaxWhitelistedProjects>, ValueQuery>;

	/// Returns (positive_funds,negative_funds) of Whitelisted Project accounts
	#[pallet::storage]
	pub type ProjectFunds<T: Config> = StorageMap<
		_,
		Twox64Concat,
		ProjectId<T>,
		BoundedVec<BalanceOf<T>, ConstU32<2>>,
		ValueQuery,
	>;

	/// Returns Votes Infos against (project_id, voter_id) key
	#[pallet::storage]
	pub type Votes<T: Config> = StorageDoubleMap<
		_,
		Blake2_128Concat,
		ProjectId<T>,
		Twox64Concat,
		VoterId<T>,
		VoteInfo<T>,
		OptionQuery,
	>;

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {
		/// Reward successfully claimed
		RewardsAssigned { when: BlockNumberFor<T> },

		/// User's vote successfully submitted
		VoteCasted { who: VoterId<T>, when: BlockNumberFor<T>, project_id: ProjectId<T> },

		/// User's vote successfully removed
		VoteRemoved { who: VoterId<T>, when: BlockNumberFor<T>, project_id: ProjectId<T> },

		/// Project removed from whitelisted projects list
		ProjectUnlisted { when: BlockNumberFor<T>, project_id: ProjectId<T> },

		/// Project Funding Accepted by voters
		ProjectFundingAccepted {
			project_id: ProjectId<T>,
			when: BlockNumberFor<T>,
			round_number: u32,
			amount: BalanceOf<T>,
		},

		/// Project Funding rejected by voters
		ProjectFundingRejected { when: BlockNumberFor<T>, project_id: ProjectId<T> },

		/// A new voting round started
		VotingRoundStarted { when: BlockNumberFor<T>, round_number: u32 },

		/// The users voting period ended. Reward calculation will start.
		VoteActionLocked { when: BlockNumberFor<T>, round_number: u32 },

		/// The voting round ended
		VotingRoundEnded { when: BlockNumberFor<T>, round_number: u32 },

		/// User's funds unlocked
		FundsUnlocked { when: BlockNumberFor<T>, amount: BalanceOf<T>, project_id: ProjectId<T> },
	}

	#[pallet::error]
	pub enum Error<T> {
		/// This account is not connected to any WhiteListed Project.
		NotWhitelistedProject,

		/// There are no whitelisted project
		NoWhitelistedProject,

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
		NotEnoughFunds,

		/// The reward calculation failed due to an internal error
		FailedRewardCalculation,

		/// Voting round is over
		VotingRoundOver,

		/// User's funds still cannot be unlocked
		FundsUnlockNotPermitted,
	}

	#[pallet::hooks]
	impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
		/// Weight: see `begin_block`
		fn on_idle(n: BlockNumberFor<T>, remaining_weight: Weight) -> Weight {
			Self::on_idle_function(n, remaining_weight)
		}
	}

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		/// OPF voting logic
		///
		/// ## Dispatch Origin
		///
		/// Must be signed
		///
		/// ## Details
		///
		/// This extrinsic allows users to [vote for/nominate] a whitelisted project using their
		/// funds. As a first implementation, the `conviction` parameter was not included for
		/// simplicity, but /// should be in the next iteration of the pallet. The amount defined
		/// by the user is locked and released only when the project reward is /// sent for
		/// distribution, or when the project is not dimmed fundable. Users can edit an existing
		/// vote within the vote-casting period. Then, during the vote-locked period, rewards are
		/// calculated based on the total user amount attributed to each project by the user’s
		/// votes.
		///
		/// ### Parameters
		/// - `project_id`: The account that will receive the reward.
		/// - `amount`: Amount that will be locked in user’s balance to nominate a project.
		/// - `is_fund`: Parameter that defines if user’s vote is in favor (*true*), or against
		///   (*false*)
		/// the project funding.

		/// ### Errors
		/// - [`Error::<T>::NotEnoughFunds`]: The user does not have enough balance to cast a vote
		///  
		/// ## Events
		#[pallet::call_index(0)]
		#[pallet::weight(<T as Config>::WeightInfo::vote(T::MaxWhitelistedProjects::get()))]
		pub fn vote(
			origin: OriginFor<T>,
			project_id: ProjectId<T>,
			#[pallet::compact] amount: BalanceOf<T>,
			is_fund: bool,
			conviction: Conviction,
		) -> DispatchResult {
			let voter = ensure_signed(origin)?;
			// Get current voting round & check if we are in voting period or not
			Self::period_check()?;
			// Check that voter has enough funds to vote
			let voter_balance = T::NativeBalance::total_balance(&voter);
			ensure!(voter_balance > amount, Error::<T>::NotEnoughFunds);

			// Check the total amount locked in other projects
			let voter_holds = BalanceOf::<T>::zero();
			let projects = WhiteListedProjectAccounts::<T>::get();
			for project in projects {
				if let Some(infos) = Votes::<T>::get(&project, &voter) {
    				voter_holds.saturating_add(infos.amount);				
				}
			}

			let available_funds = voter_balance.saturating_sub(voter_holds);
			ensure!(available_funds > amount, Error::<T>::NotEnoughFunds);

			// Vote action executed

			Self::try_vote(voter.clone(), project_id.clone(), amount, is_fund, conviction)?;

			let when = T::BlockNumberProvider::current_block_number();

			Self::deposit_event(Event::<T>::VoteCasted { who: voter, when, project_id });

			Ok(())
		}

		/// OPF vote removal logic
		///
		/// ## Dispatch Origin
		///
		/// Must be signed
		///
		/// ## Details
		///
		/// This extrinsic allows users to remove a casted vote, as long as it is within the
		/// vote-casting period.
		///
		/// ### Parameters
		/// - `project_id`: The account that will receive the reward.
		///
		/// ### Errors
		/// - [`Error::<T>::NotEnoughFunds`]: The user does not have enough balance to cast a vote
		///  
		/// ## Events
		#[pallet::call_index(1)]
		#[pallet::weight(<T as Config>::WeightInfo::remove_vote(T::MaxWhitelistedProjects::get()))]
		pub fn remove_vote(origin: OriginFor<T>, project_id: ProjectId<T>) -> DispatchResult {
			let voter = ensure_signed(origin)?;
			// Get current voting round & check if we are in voting period or not
			Self::period_check()?;
			// Removal action executed
			Self::try_remove_vote(voter.clone(), project_id.clone())?;

			let when = T::BlockNumberProvider::current_block_number();
			Self::deposit_event(Event::<T>::VoteRemoved { who: voter, when, project_id });

			Ok(())
		}

		/// User's funds unlock
		///
		/// ## Dispatch Origin
		///
		/// Must be signed
		///
		/// ## Details
		///
		/// This extrinsic allows users to unlock funds related to a specific project,
		/// provided the locking period (which is dependant of the conviction) has ended.
		///
		/// ### Parameters
		/// - `project_id`: The account that will receive the reward.
		///
		/// ### Errors
		/// - [`Error::<T>::NotEnoughFunds`]: The user does not have enough balance to cast a vote
		///  
		/// ## Events
		#[pallet::call_index(2)]
		#[pallet::weight(<T as Config>::WeightInfo::unlock_funds(T::MaxWhitelistedProjects::get()))]
		pub fn unlock_funds(origin: OriginFor<T>, project: ProjectId<T>) -> DispatchResult {
			let voter = ensure_signed(origin)?;
			let infos = Votes::<T>::get(&project, &voter).ok_or(Error::<T>::NoVoteData)?;
			let amount = infos.amount;
			let now = T::BlockNumberProvider::current_block_number();
			ensure!(now >= infos.funds_unlock_block, Error::<T>::FundsUnlockNotPermitted);
			// release voter's funds
			T::NativeBalance::release(
				&HoldReason::FundsReserved.into(),
				&voter,
				amount,
				Precision::Exact,
			)?;

			Self::deposit_event(Event::<T>::FundsUnlocked {
				when: now,
				amount,
				project_id: project,
			});
			Ok(())
		}
	}
}
