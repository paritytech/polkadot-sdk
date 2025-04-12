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

//! # OPF pallet.
//!
//! The OPF Pallet handles the Optimistic Project Funding.
//! It allows users to nominate projects (whitelisted in OpenGov) with their DOT.
//!
//! ## Overview
//!
//! This mechanism will be funded with a constant stream of DOT taken directly from inflation
//! and distributed to projects based on the proportion of DOT that has nominated them.
//!
//! ### Terminology
//!
//! - **MaxWhitelistedProjects:** Maximum number of Whitelisted projects that can be handled by the
//!   pallet.
//! - **VotingPeriod:**Period during which voting is enabled.
//! - **TemporaryRewards:**For test purposes only ⇒ used as a substitute for the inflation portion
//!   used for the rewards.
//! - **PotId:** Pot containing the funds used to pay the rewards.
//! - **ClaimingPeriod:**Period during which allocated funds can be claimed
//!
//! ## Interface
//!
//! ### Permissioned Calls
//! * `register_projects_batch`: Allows a AdminOrigin to register a list of whitelisted projects for
//!   funding allocation
//! * `unregister_project`: Allows an AdminOrigin to unregister a previously whitelisted project
//!
//! ### Permissionless Calls
//! * `vote`: Allows users to [vote for/nominate] a whitelisted project using their funds.
//! * `remove_vote`: Allows users to remove a casted vote.
//! * `release_voter_funds`: Allows users to unlock funds related to a specific project.
//! * `claim_reward_for`: Claim a reward for a nominated/whitelisted project.
//! * `execute_call_dispatch`: Used for delayed calls execution

#![cfg_attr(not(feature = "std"), no_std)]

pub use pallet::*;
mod functions;
mod traits;
mod types;
pub use traits::*;
pub use types::*;

#[cfg(feature = "runtime-benchmarks")]
mod benchmarking;
pub mod weights;

pub use weights::WeightInfo;

#[cfg(test)]
mod mock;

#[cfg(test)]
mod tests;

#[frame_support::pallet]
pub mod pallet {
	use super::*;

	#[pallet::pallet]
	pub struct Pallet<T>(_);

	#[pallet::config]
	pub trait Config: frame_system::Config {
		type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;
		type RuntimeCall: Parameter
			+ UnfilteredDispatchable<RuntimeOrigin = <Self as frame_system::Config>::RuntimeOrigin>
			+ From<Call<Self>>
			+ Into<<Self as frame_system::Config>::RuntimeCall>
			+ Into<
				<Self::Governance as traits::ReferendumTrait<
					<Self as frame_system::Config>::AccountId,
				>>::Call,
			> + GetDispatchInfo;
		/// The admin origin that can list and un-list whitelisted projects.
		type AdminOrigin: EnsureOrigin<Self::RuntimeOrigin>;

		/// Type to access the Balances Pallet.
		type NativeBalance: fungible::Inspect<Self::AccountId>
			+ fungible::Mutate<Self::AccountId>
			+ fungible::hold::Inspect<Self::AccountId>
			+ fungible::hold::Mutate<Self::AccountId, Reason = Self::RuntimeHoldReason>
			+ fungible::freeze::Inspect<Self::AccountId>
			+ fungible::freeze::Mutate<Self::AccountId>;
		type Governance: ReferendumTrait<Self::AccountId>;
		type Conviction: ConvictionVotingTrait<Self::AccountId>;
		type RuntimeHoldReason: From<HoldReason>;
		/// Provider for the block number.
		type BlockNumberProvider: BlockNumberProvider;

		/// Treasury account Id
		#[pallet::constant]
		type PotId: Get<PalletId>;

		/// Time period in which people can vote.
		/// After the period has ended, the votes are counted (STOP THE COUNT)
		/// and then the funds are distributed into Spends.
		#[pallet::constant]
		type VotingPeriod: Get<ProvidedBlockNumberFor<Self>>;

		#[pallet::constant]
		type EnactmentPeriod: Get<ProvidedBlockNumberFor<Self>>;

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

		/// Used for Pallet testing only. Represents the Total Reward distributed
		type TemporaryRewards: Get<BalanceOf<Self>>;

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

	/// Number of Voting Rounds executed so far
	#[pallet::storage]
	pub type NextVotingRoundNumber<T: Config> = StorageValue<_, u32, ValueQuery>;

	/// Returns Infos about a Voting Round agains the Voting Round index
	#[pallet::storage]
	pub type VotingRounds<T: Config> =
		StorageMap<_, Twox64Concat, RoundIndex, VotingRoundInfo<T>, OptionQuery>;

	/// Spends that still have to be claimed.
	#[pallet::storage]
	pub(super) type Spends<T: Config> =
		CountedStorageMap<_, Twox64Concat, ProjectId<T>, SpendInfo<T>, OptionQuery>;

	/// List of Whitelisted Project registered
	#[pallet::storage]
	pub type WhiteListedProjectAccounts<T: Config> =
		CountedStorageMap<_, Twox64Concat, ProjectId<T>, ProjectInfo<T>, OptionQuery>;

	/// Returns (positive_funds,negative_funds) of Whitelisted Project accounts
	#[pallet::storage]
	pub type ProjectFunds<T: Config> =
		CountedStorageMap<_, Twox64Concat, ProjectId<T>, Funds<T>, ValueQuery>;

	/// Returns Votes Infos against (project_id, voter_id) key
	#[pallet::storage]
	pub type Votes<T: Config> = StorageDoubleMap<
		_,
		Blake2_128Concat,
		ReferendumIndex,
		Twox64Concat,
		VoterId<T>,
		VoteInfo<T>,
		OptionQuery,
	>;

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {
		/// Reward successfully claimed
		RewardClaimed { amount: BalanceOf<T>, project_id: ProjectId<T> },

		/// A Spend was created
		SpendCreated { amount: BalanceOf<T>, project_id: ProjectId<T> },

		/// Payment will be enacted for corresponding project
		WillBeEnacted { project_id: ProjectId<T> },

		/// User's vote successfully submitted
		VoteCasted { who: VoterId<T>, project_id: ProjectId<T> },

		/// User's vote successfully removed
		VoteRemoved { who: VoterId<T>, project_id: ProjectId<T> },

		/// Project added to whitelisted projects list
		Projectlisted { project_id: ProjectId<T> },

		/// Several projects added to whitelisted projects list
		Projectslisted { projects_id: Vec<ProjectId<T>> },

		/// Project removed from whitelisted projects list
		ProjectUnlisted { project_id: ProjectId<T> },

		/// Project Funding Accepted by voters
		ProjectFundingAccepted { project_id: ProjectId<T>, amount: BalanceOf<T> },

		/// Reward claim has expired
		ExpiredClaim { expired_when: ProvidedBlockNumberFor<T>, project_id: ProjectId<T> },

		/// Project Funding rejected by voters
		ProjectFundingRejected { project_id: ProjectId<T> },

		/// A new voting round started
		VotingRoundStarted { round_number: u32 },

		/// The users voting period ended. Reward calculation will start.
		VoteActionLocked { round_number: u32 },

		/// The voting round ended
		VotingRoundEnded { round_number: u32 },
	}

	#[pallet::error]
	pub enum Error<T> {
		/// Not enough Funds in the Pot
		InsufficientPotReserves,
		/// The funds transfer operation failed
		TransferFailed,
		/// Spend or Spend index does not exists
		InexistentSpend,
		/// No project found under this project_id
		NoProjectAvailable,
		/// The Funds transfer failed
		FailedSpendOperation,
		/// Still not in claiming period
		NotClaimingPeriod,
		/// Still not in funds unlock period
		NotUnlockPeriod,
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
		/// Another project has already been submitted under the same project_id
		SubmittedProjectId,
		/// Project batch already submitted
		BatchAlreadySubmitted,
		/// Requested vote data do not exist
		NoVoteData,
		/// Not enough funds to process the transaction
		NotEnoughFunds,
		/// This referendum does not exists
		ReferendumNotFound,

		FailedToDispatchCall,
	}

	#[pallet::hooks]
	impl<T: Config> Hooks<SystemBlockNumberFor<T>> for Pallet<T> {
		fn on_idle(_n: SystemBlockNumberFor<T>, remaining_weight: Weight) -> Weight {
			Self::on_idle_function(remaining_weight)
		}
	}

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		/// OPF Projects registration
		///
		/// ## Dispatch Origin
		///
		/// Must be AdminOrigin
		///
		/// ## Details
		///
		/// From this extrinsic only AdminOrigin can register whitelisted projects.
		///
		/// ### Parameters
		/// - `projects_id`: The accounts that might be funded.
		///
		/// ### Errors
		/// - [`Error::<T>::MaximumProjectsNumber`]: Maximum number of project subscriptions reached
		///  
		/// ## Events
		/// Emits [`Event::<T>::Projectslisted`].
		#[pallet::call_index(1)]
		#[pallet::weight(<T as Config>::WeightInfo::register_projects_batch(T::MaxProjects::get()))]
		pub fn register_projects_batch(
			origin: OriginFor<T>,
			projects_id: BoundedVec<ProjectId<T>, T::MaxProjects>,
		) -> DispatchResult {
			T::AdminOrigin::ensure_origin_or_root(origin.clone())?;
			//let who = ensure_signed(origin)?;
			Self::round_check()?;
			let round_index = NextVotingRoundNumber::<T>::get();
			let current_round_index = round_index.saturating_sub(1);
			let mut round_infos =
				VotingRounds::<T>::get(current_round_index).ok_or(Error::<T>::InvalidResult)?;

			// Check no Project batch has been submitted yet
			ensure!(!round_infos.batch_submitted, Error::<T>::BatchAlreadySubmitted);
			round_infos.batch_submitted = true;

			let mut round_ending_block = round_infos.round_ending_block;
			let mut projects_submitted: Vec<ProjectId<T>> = vec![];

			for project_id in &projects_id {
				ProjectInfo::<T>::new(project_id.clone());
				// Check if the project is already submitted
				ensure!(
					!round_infos.projects_submitted.contains(project_id),
					Error::<T>::SubmittedProjectId
				);
				projects_submitted.push(project_id.clone());
				// Prepare the proposal call
				let call = Call::<T>::on_registration { project_id: project_id.clone() };

				let referendum_index = Self::start_referendum(project_id.clone(), call.into())?;
				let mut new_infos = WhiteListedProjectAccounts::<T>::get(&project_id)
					.ok_or(Error::<T>::NoProjectAvailable)?;
				new_infos.index = referendum_index;

				WhiteListedProjectAccounts::<T>::mutate(project_id, |value| {
					*value = Some(new_infos);
				});
				let time_periods = T::Governance::get_time_periods(referendum_index.into())?;
				let enactment_period_128 = time_periods.min_enactment_period;
				let round_period_128 =
					time_periods.total_period.saturating_sub(enactment_period_128);
				// convert decision_period to block number, as it is a u128
				let round_period: ProvidedBlockNumberFor<T> =
					round_period_128.try_into().map_err(|_| Error::<T>::InvalidResult)?;
				if round_infos.round_ending_block == round_infos.round_starting_block {
					round_ending_block = round_ending_block.saturating_add(round_period.into());
					round_infos.round_ending_block = round_ending_block;
					round_infos.time_periods = Some(time_periods);
				}
			}
			round_infos.projects_submitted =
				projects_submitted.clone().try_into().map_err(|_| Error::<T>::InvalidResult)?;
			VotingRounds::<T>::mutate(current_round_index, |round| *round = Some(round_infos));

			Self::deposit_event(Event::Projectslisted { projects_id: projects_submitted });
			Ok(())
		}

		/// OPF Projects de-listing
		///
		/// ## Dispatch Origin
		///
		/// Must be signed
		///
		/// ## Details
		///
		/// From this extrinsic only Root can de-list a project.
		///
		/// ### Parameters
		/// - `project_id`: The account that will receive the reward.
		///
		/// ### Errors
		/// - [`Error::<T>::NoProjectAvailable`]: No project found under this project_id
		///  
		/// ## Events
		/// Emits [`Event::<T>::ProjectUnlisted`].
		#[pallet::call_index(2)]
		#[pallet::weight(<T as Config>::WeightInfo::unregister_project(T::MaxProjects::get()))]
		pub fn unregister_project(
			origin: OriginFor<T>,
			project_id: ProjectId<T>,
		) -> DispatchResult {
			T::AdminOrigin::ensure_origin_or_root(origin)?;
			WhiteListedProjectAccounts::<T>::remove(&project_id);
			Self::deposit_event(Event::<T>::ProjectUnlisted { project_id });

			Ok(())
		}

		/// OPF voting logic
		///
		/// ## Dispatch Origin
		///
		/// Must be signed
		///
		/// ## Details
		///
		/// This extrinsic allows users to [vote for/nominate] a whitelisted project using their
		/// funds. The amount defined by the user is locked and released only when the project
		/// reward is ready for distribution, or when the project is not dimmed fundable.
		/// Users can edit/over-write an existing vote within the vote-casting period.
		/// At the end of the voting period, rewards are calculated based on the total user amount
		/// attributed to each project by the user’s votes.
		///
		/// ### Parameters
		/// - `project_id`: The account that will receive the reward.
		/// - `amount`: Amount that will be locked in user’s balance to nominate a project.
		/// - `fund`: Parameter that defines if user’s vote is in favor (*true*), or against
		///   (*false*)
		/// the project funding.
		/// - `conviction`: Used to calculate the value allocated to the project, & determine
		/// when the voter's funds will be unlocked. Amount actually locked is the amount without
		/// conviction  
		///
		/// ### Errors
		/// - [`Error::<T>::NotEnoughFunds`]: The user does not have enough balance to cast a vote
		///  
		/// ## Events
		/// - [`Event::<T>::VoteCasted { who, project_id }`]: User's vote successfully submitted
		#[pallet::call_index(3)]
		#[pallet::weight(<T as Config>::WeightInfo::vote(T::MaxProjects::get()))]
		pub fn vote(
			origin: OriginFor<T>,
			project_id: ProjectId<T>,
			#[pallet::compact] amount: BalanceOf<T>,
			fund: bool,
			conviction: Conviction,
		) -> DispatchResult {
			let voter = ensure_signed(origin.clone())?;
			// Get current voting round & check if we are in voting period or not
			Self::period_check()?;

			let infos = WhiteListedProjectAccounts::<T>::get(&project_id)
				.ok_or(Error::<T>::NoProjectAvailable)?;
			let ref_index = infos.index;

			// Funds lock is handled by the opf pallet
			let conv = Conviction::None;
			let u128_amount = amount.saturated_into::<u128>();
			let converted_amount =
				T::Conviction::u128_to_balance(u128_amount).ok_or("Failed Conversion!!!")?;
			let account_vote = T::Conviction::vote_data(fund, conv, converted_amount);
			if Votes::<T>::contains_key(&ref_index, &voter) {
				T::Conviction::try_remove_vote(&voter, ref_index.into())?;
			}
			T::Conviction::try_vote(&voter, ref_index.into(), account_vote)?;

			Self::try_vote(voter.clone(), project_id.clone(), amount, fund, conviction)?;

			Self::deposit_event(Event::<T>::VoteCasted { who: voter, project_id });

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
		/// - [`Error::<T>::NoProjectAvailable`]: No project found under this project_id
		///  
		/// ## Events
		/// - [`Event::<T>::VoteRemoved { who, project_id }`]: User's vote successfully removed
		#[pallet::call_index(4)]
		#[pallet::weight(<T as Config>::WeightInfo::remove_vote(T::MaxProjects::get()))]
		pub fn remove_vote(origin: OriginFor<T>, project_id: ProjectId<T>) -> DispatchResult {
			let voter = ensure_signed(origin.clone())?;
			// Get current voting round & check if we are in voting period or not
			Self::period_check()?;

			// Remove previous vote from Referendum
			let infos = WhiteListedProjectAccounts::<T>::get(project_id.clone())
				.ok_or(Error::<T>::NoProjectAvailable)?;
			T::Conviction::try_remove_vote(&voter, infos.index.into())?;
			// Removal action executed
			Self::try_remove_vote(voter.clone(), project_id.clone())?;

			Self::deposit_event(Event::<T>::VoteRemoved { who: voter, project_id });
			Ok(())
		}

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
		/// - `project_id`: The account that will receive the reward.
		///
		/// ### Errors
		/// - [`Error::<T>::InexistentSpend`]:Spend or Spend index does not exists
		/// - [`Not Claiming Period`]: Still not in claiming period
		///  
		/// ## Events
		/// Emits [`Event::<T>::RewardClaimed`] if successful for a positive approval.
		#[pallet::call_index(5)]
		#[pallet::weight(<T as Config>::WeightInfo::claim_reward_for(T::MaxProjects::get()))]
		pub fn claim_reward_for(origin: OriginFor<T>, project_id: ProjectId<T>) -> DispatchResult {
			let _caller = ensure_signed(origin)?;
			let now = T::BlockNumberProvider::current_block_number();
			let mut info = Spends::<T>::get(&project_id).ok_or(Error::<T>::InexistentSpend)?;
			Self::pot_check(info.amount)?;

			if now >= info.expire {
				Spends::<T>::remove(&project_id);
				Self::deposit_event(Event::ExpiredClaim {
					expired_when: info.expire,
					project_id: project_id.clone(),
				});
				return Ok(());
			}
			if now < info.expire {
				// transfer the funds
				Spends::<T>::mutate(project_id.clone(), |val| {
					info.claimed = true;
					*val = Some(info.clone())
				});
				Self::spend(info.amount, project_id.clone())?;
				Self::deposit_event(Event::RewardClaimed {
					amount: info.amount,
					project_id: project_id.clone(),
				});
				WhiteListedProjectAccounts::<T>::remove(&project_id);
				return Ok(());
			}
			Ok(())
		}

		#[pallet::call_index(6)]
		#[pallet::weight(<T as Config>::WeightInfo::on_registration(T::MaxProjects::get()))]
		pub fn on_registration(origin: OriginFor<T>, project_id: ProjectId<T>) -> DispatchResult {
			let _who = T::AdminOrigin::ensure_origin(origin.clone())?;
			let mut infos = WhiteListedProjectAccounts::<T>::get(project_id.clone())
				.ok_or(Error::<T>::NoProjectAvailable)?;

			let ref_index = infos.index;
			let amount = infos.amount;
			if let Some(ref_infos) = T::Governance::get_referendum_info(ref_index.into()) {
				let state = T::Governance::handle_referendum_info(ref_infos);
				match state {
					Some(ReferendumStates::Approved) => {
						let pot = Self::pot_account();
						let balance = T::NativeBalance::balance(&pot);
						let minimum_balance = T::NativeBalance::minimum_balance();
						// check if the pot has enough fund for the Spend
						// Check that the Pot as enough funds for the transfer
						let remaining_balance = balance.saturating_sub(infos.amount);
						ensure!(remaining_balance > minimum_balance, Error::<T>::NotEnoughFunds);
						infos.spend_created = true;
						WhiteListedProjectAccounts::<T>::mutate(project_id.clone(), |val| {
							*val = Some(infos.clone())
						});
						// create a spend for project to be rewarded
						let new_spend = SpendInfo::<T>::new(&infos);
						Self::deposit_event(Event::ProjectFundingAccepted { project_id, amount });
						Self::deposit_event(Event::SpendCreated {
							amount: new_spend.amount,
							project_id: infos.project_id.clone(),
						});
					},
					Some(ReferendumStates::Rejected) =>
						Self::deposit_event(Event::ProjectFundingRejected { project_id }),
					Some(ReferendumStates::Ongoing) => (),
					None => (),
				}
				Ok(())
			} else {
				Err(Error::<T>::ReferendumNotFound.into())
			}
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
		/// - [`Error::<T>::NoVoteData`]: No vote data found for the specified project
		///  
		/// ## Events
		#[pallet::call_index(7)]
		#[pallet::weight(<T as Config>::WeightInfo::release_voter_funds(T::MaxProjects::get()))]
		pub fn release_voter_funds(
			origin: OriginFor<T>,
			referendum_index: ReferendumIndex,
		) -> DispatchResult {
			let voter_id = ensure_signed(origin)?;
			ensure!(Votes::<T>::contains_key(&referendum_index, &voter_id), Error::<T>::NoVoteData);
			let infos = Votes::<T>::get(&referendum_index, &voter_id).ok_or(Error::<T>::NoVoteData)?;
			let release_block = infos.funds_unlock_block;
			let amount = infos.amount;

			let now = T::BlockNumberProvider::current_block_number();
			ensure!(now >= release_block, Error::<T>::NotUnlockPeriod);
			T::NativeBalance::release(
				&HoldReason::FundsReserved.into(),
				&voter_id,
				amount,
				Precision::Exact,
			)?;

			Votes::<T>::remove(&referendum_index, &voter_id);
			Ok(())
		}

		#[pallet::call_index(8)]
		#[pallet::weight(<T as Config>::WeightInfo::remove_vote(T::MaxProjects::get()))]
		pub fn execute_call_dispatch(
			origin: OriginFor<T>,
			caller: T::AccountId,
			proposal: Box<<T as Config>::RuntimeCall>,
		) -> DispatchResult {
			ensure_root(origin)?;
			proposal
				.dispatch_bypass_filter(frame_system::RawOrigin::Signed(caller.clone()).into())
				.ok();
			Ok(().into())
		}
	}
}
