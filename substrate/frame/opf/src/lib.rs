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

//! # Optimistic Project Funding (OPF) pallet
//!
//! OPF is a mechanism for voter to distribute funds to projects using conviction voting at regular
//! intervals. The OPF pot should be funded from a continuous stream of fund, the funds
//! not spend in a round are sent to another destination account, typically the treasury.
//! Projects to be voted on must registered by admin origin, and can be unregistered by the same
//! origin.
//!
//! This pallet is tied to an instance of pallet conviction voting.
//!
//! ## Calls:
//!
//! * [`register_project`](Pallet::register_project) – An Admin origin (e.g. OpenGov referendum) can
//!   register a project, providing owner, reward destination, name and description. Voter can vote
//!   on the project starting on next round.
//! * [`manage_project_info`](Pallet::manage_project_info) – The project owner can update the
//!   project information, such as fund destination, name and description.
//! * [`unregister_project`](Pallet::unregister_project) – An Admin origin can unregister a project,
//!   removing it from the set of registered projects. The project will not receive reward for the
//!   current round, and will not be part of the next round.
//! * [`remove_automatic_forwarding`](Pallet::remove_automatic_forwarding) – By default votes are
//!   automatically forwarded to the next round, this forwarding is stopped every
//!   [`Config::ResetVotesRoundNumber`]. This call allows to remove the forwarding of the vote. Any
//!   subsequent vote will automatically re-enable the forwarding.
//!
//! The calls to vote on polls are inside pallet conviction voting.
//!
//! ## Rounds:
//!
//! Every [`Config::RoundDuration`] a new round is created, polls for all registered
//! projects are created, user can vote on the polls.
//! At the end of the round, the votes are gathered and projects are rewarded. Then votes are
//! forwarded to the next round in the background over the course of multiple blocks.
//!
//! ### Reward calculation
//!
//! At the end of a round the pallet disburses the pot as follows:
//! ```text
//! Sᵢ = max(ayesᵢ − naysᵢ, 0)      // net support of project i
//! Σ  = Σ Sᵢ                       // total positive support across all projects
//! rewardᵢ = (Sᵢ / Σ) × pot_balance
//! ```
//! * `ayesᵢ`, `naysᵢ` come from the conviction-voting tally of project i.
//! * Projects with `Sᵢ = 0` or that were unregistered mid‑round receive no payout.
//! * Any unallocated funds are transferred to [`Config::TreasuryAccountId`], leaving the pot empty
//!   before the next round begins.

#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

#[cfg(feature = "runtime-benchmarks")]
pub mod benchmarking;
#[cfg(test)]
mod mock;
#[cfg(test)]
mod tests;
pub mod weights;

pub use weights::WeightInfo;

pub use pallet::*;
#[frame_support::pallet]
pub mod pallet {
	use super::*;
	use alloc::{collections::BTreeSet, vec, vec::Vec};
	use frame_support::{
		pallet_prelude::*,
		storage::transactional::with_storage_layer,
		traits::{
			fungible,
			fungible::{Inspect, Mutate},
			tokens::{Fortitude, Preservation},
			Defensive, PollStatus, Polling,
		},
		weights::WeightMeter,
		PalletId,
	};
	use frame_system::pallet_prelude::*;
	use pallet_conviction_voting::{AccountVote, LockedIf, Voting, VotingHooks};
	use sp_runtime::{
		traits::{AccountIdConversion, BlockNumberProvider},
		Perbill, Saturating,
	};

	const LOG_TARGET: &str = "runtime::opf";

	#[pallet::pallet]
	pub struct Pallet<T>(_);

	#[pallet::config]
	pub trait Config:
		frame_system::Config
		+ pallet_conviction_voting::Config<
			Self::ConvictionVotingInstance,
			Polls = Pallet<Self>,
			VotingHooks = Pallet<Self>,
		>
	{
		/// The admin origin that can list and un-list whitelisted projects.
		type AdminOrigin: EnsureOrigin<Self::RuntimeOrigin>;

		/// Provider for the block number.
		type BlockNumberProvider: BlockNumberProvider;

		/// The duration of a round in blocks, as counted by the configured block provider.
		type RoundDuration: Get<
			<<Self as Config>::BlockNumberProvider as BlockNumberProvider>::BlockNumber,
		>;

		/// The instance of the conviction voting pallet to use for this pallet.
		type ConvictionVotingInstance;

		/// The fungible implementation for assets in pot, and transfered to projects.
		type Fungible: fungible::Mutate<Self::AccountId>;

		/// The number of rounds after which all the votes are reset.
		type ResetVotesRoundNumber: Get<u32>;

		/// Pot account Id
		#[pallet::constant]
		type PotId: Get<PalletId>;

		/// The account ID for the treasury pot to send funds not spent in the current round.
		type TreasuryAccountId: Get<Self::AccountId>;

		/// The maximum number of projects that can be registered.
		type MaxProjects: Get<u32>;

		/// Weight information for extrinsics.
		type WeightInfo: weights::WeightInfo;
	}

	/// Type alias to access Moment as used in pallet conviction voting.
	pub type MomentFor<T> = <<T as pallet_conviction_voting::Config<
		<T as Config>::ConvictionVotingInstance,
	>>::Polls as Polling<TallyFor<T>>>::Moment;
	/// Type alias to access the Tally as used in pallet conviction voting.
	pub type TallyFor<T> =
		pallet_conviction_voting::TallyOf<T, <T as Config>::ConvictionVotingInstance>;
	/// Type alias to access the Votes as used in pallet conviction voting.
	pub type VotesFor<T> =
		pallet_conviction_voting::BalanceOf<T, <T as Config>::ConvictionVotingInstance>;

	/// The class of the poll. We use a single class.
	#[derive(
		Encode,
		Decode,
		PartialEq,
		Eq,
		PartialOrd,
		Ord,
		Clone,
		MaxEncodedLen,
		Debug,
		TypeInfo,
		DecodeWithMemTracking,
	)]
	pub struct Class;

	/// The index of a round.
	pub type RoundIndex = u32;

	/// The information about a project.
	#[derive(
		Encode, Decode, MaxEncodedLen, Clone, Debug, TypeInfo, DecodeWithMemTracking, PartialEq, Eq,
	)]
	pub struct ProjectInfo<AccountId> {
		/// The owner of the project.
		///
		/// They can manage the project information.
		pub(crate) owner: AccountId,
		/// The destination account where the funds will be sent.
		pub(crate) fund_dest: AccountId,
		/// The name of the project.
		pub(crate) name: BoundedVec<u8, ConstUint<256>>,
		/// The description of the project.
		pub(crate) description: BoundedVec<u8, ConstUint<256>>,
	}

	/// The index of a project.
	pub type ProjectIndex = u32;

	/// The index of a poll.
	///
	/// It is a combination of the round index and the project index.
	/// the 32 most significant bits are the round index, and the 32 least significant bits are the
	/// project index.
	#[derive(
		Encode,
		Decode,
		MaxEncodedLen,
		Clone,
		Debug,
		TypeInfo,
		DecodeWithMemTracking,
		Copy,
		Eq,
		PartialEq,
		Ord,
		PartialOrd,
		codec::CompactAs,
	)]
	pub struct PollIndex(u64);

	impl PollIndex {
		/// Returns the round index of the poll.
		pub fn round_index(self) -> RoundIndex {
			(self.0 >> 32) as RoundIndex
		}
		/// Returns the project index of the poll.
		pub fn project_index(self) -> ProjectIndex {
			(self.0 & 0xFFFFFFFF) as ProjectIndex
		}
		/// Creates a new poll index from the round index and project index.
		pub fn new(round_index: RoundIndex, project_index: ProjectIndex) -> Self {
			Self(((round_index as u64) << 32) | (project_index as u64))
		}
	}

	/// The information about the current round.
	#[derive(Encode, Decode, MaxEncodedLen, Clone, Debug, TypeInfo, DecodeWithMemTracking)]
	pub struct RoundInfo<BlockNumber, Votes> {
		/// The block number when the round started.
		pub starting_block: BlockNumber,
		/// The total amount of votes in the round. This is not the sum of all votes but the sum of
		/// all positive votes. Projects with negative ratios are not counted.
		pub total_vote_amount: Votes,
	}

	/// The status of a poll.
	#[derive(Encode, Decode, MaxEncodedLen, Clone, Debug, TypeInfo, DecodeWithMemTracking)]
	pub enum PollInfo<Tally, Moment> {
		/// The poll is ongoing, with the current tally and class.
		Ongoing(Tally, Class),
		/// The poll is completed, with the moment it was completed and whether it was approved.
		Completed(Moment, bool),
	}

	/// A vote recored in a round.
	#[derive(Encode, Decode, MaxEncodedLen, Clone, Debug, TypeInfo, DecodeWithMemTracking)]
	pub struct VoteInSession<Vote> {
		/// The round in which the vote was cast.
		pub round: u32,
		/// The vote itself.
		pub vote: Vote,
	}

	/// The information about the votes forwarding state.
	#[derive(Encode, Decode, MaxEncodedLen, Clone, Debug, TypeInfo, DecodeWithMemTracking)]
	pub struct VotesForwardingInfo<AccountId> {
		/// The state of the votes forwarding of previous rounds.
		pub forwarding: ForwardingProcess<AccountId>,
		/// The last round in which the votes were reset.
		///
		/// Votes from rounds and before won't be forwarded.
		pub reset_round: Option<u32>,
	}

	impl<AccountId> Default for VotesForwardingInfo<AccountId> {
		fn default() -> Self {
			Self { forwarding: ForwardingProcess::Start, reset_round: None }
		}
	}

	/// The state of the votes forwarding system.
	///
	/// It iterates on all votes and forwards the votes that needs to be forwarded.
	#[derive(Encode, Decode, MaxEncodedLen, Clone, Debug, TypeInfo, DecodeWithMemTracking)]
	pub enum ForwardingProcess<AccountId> {
		/// The process is starting. No votes has bee processed.
		Start,
		/// The process is in the middle of processing votes.
		LastProcessed(ProjectIndex, AccountId),
		/// The process is finished. All votes has been processed.
		Finished,
	}

	/// The status of polls.
	#[pallet::storage]
	pub type Polls<T: Config> = StorageDoubleMap<
		_,
		Twox64Concat,
		RoundIndex,
		Twox64Concat,
		ProjectIndex,
		PollInfo<TallyFor<T>, MomentFor<T>>,
		OptionQuery,
	>;

	/// The next round index.
	///
	/// The current round index is `NextRoundIndex - 1` (or None if next round index is 0).
	#[pallet::storage]
	pub type NextRoundIndex<T> = StorageValue<_, RoundIndex, ValueQuery>;

	/// The current round information if there is a current round.
	#[pallet::storage]
	pub type Round<T: Config> = StorageValue<
		_,
		RoundInfo<
			<<T as Config>::BlockNumberProvider as BlockNumberProvider>::BlockNumber,
			VotesFor<T>,
		>,
		OptionQuery,
	>;

	/// The next project index.
	///
	/// This is increemented every time a new project is registered.
	#[pallet::storage]
	pub type NextProjectIndex<T> = StorageValue<_, ProjectIndex, ValueQuery>;

	/// The projects registered in the OPF.
	#[pallet::storage]
	pub type Projects<T: Config> =
		CountedStorageMap<_, Twox64Concat, ProjectIndex, ProjectInfo<T::AccountId>, OptionQuery>;

	/// The state of votes forwarding system.
	#[pallet::storage]
	pub type VotesForwardingState<T: Config> =
		StorageValue<_, VotesForwardingInfo<T::AccountId>, ValueQuery>;

	/// The votes to forward for each project and voter.
	#[pallet::storage]
	pub type VotesToForward<T: Config> = StorageDoubleMap<
		_,
		Twox64Concat,
		ProjectIndex,
		Twox64Concat,
		T::AccountId,
		VoteInSession<
			AccountVote<pallet_conviction_voting::BalanceOf<T, T::ConvictionVotingInstance>>,
		>,
		OptionQuery,
	>;

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {
		/// A project was registered.
		ProjectRegistered { project_index: ProjectIndex, owner: T::AccountId },
		/// Project info was updated.
		ProjectInfoUpdated { project_index: ProjectIndex, owner: T::AccountId },
		/// A project was unregistered.
		ProjectUnregistered { project_index: ProjectIndex },
		/// Automatic forwarding was removed for a user.
		AutomaticForwardingRemoved { project_index: ProjectIndex, who: T::AccountId },
	}

	#[pallet::error]
	pub enum Error<T> {
		/// The project at the given index does not exist.
		NoProjectAtIndex,
		/// The account is not the owner of the project.
		AccountIsNotProjectOwner,
		/// The maximum number of projects has been reached.
		MaxProjectsReached,
	}

	#[pallet::hooks]
	impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
		fn on_poll(_n: BlockNumberFor<T>, weight: &mut WeightMeter) {
			if weight.try_consume(<T as Config>::WeightInfo::on_poll_base()).is_err() {
				return;
			}

			if let Some(round_info) = Round::<T>::get() {
				if <T as Config>::BlockNumberProvider::current_block_number() >=
					round_info.starting_block.saturating_add(T::RoundDuration::get())
				{
					if weight.try_consume(<T as Config>::WeightInfo::on_poll_end_round()).is_err() {
						return;
					}
					Self::on_poll_end_round();
				}
			}

			if Round::<T>::get().is_none() {
				if weight.try_consume(<T as Config>::WeightInfo::on_poll_new_round()).is_err() {
					return;
				}
				Self::on_poll_new_round();
			}

			// We use only 10% of the remaining weight to forward votes.
			let mut limited_weight = WeightMeter::with_limit(weight.remaining() / 10);
			Pallet::<T>::on_poll_on_idle_forward_votes(&mut limited_weight);
			weight.consume(limited_weight.consumed());
		}
		fn on_idle(_n: BlockNumberFor<T>, remaining_weight: Weight) -> Weight {
			// We avoid using 100% in case weight are underestimated.
			let weight_limit = remaining_weight / 2;
			let mut weight = WeightMeter::with_limit(weight_limit);
			Pallet::<T>::on_poll_on_idle_forward_votes(&mut weight);
			weight.remaining()
		}
	}

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		/// Register a new project.
		///
		/// Origin must be the admin origin.
		///
		/// The project is registered and be voted on in the next round and forward.
		///
		/// There is a maximum number of projects as configured by `MaxProjects`.
		#[pallet::call_index(0)]
		#[pallet::weight(<T as Config>::WeightInfo::register_project())]
		pub fn register_project(
			origin: OriginFor<T>,
			project: ProjectInfo<T::AccountId>,
		) -> DispatchResult {
			T::AdminOrigin::ensure_origin_or_root(origin.clone())?;

			// Enforce MaxProjects
			ensure!(Projects::<T>::count() < T::MaxProjects::get(), Error::<T>::MaxProjectsReached);

			let index = NextProjectIndex::<T>::mutate(|next_index| {
				let index = *next_index;
				*next_index = next_index.saturating_add(1);
				index
			});

			Projects::<T>::insert(index, project.clone());

			Self::deposit_event(Event::ProjectRegistered {
				project_index: index,
				owner: project.owner,
			});

			Ok(())
		}

		/// Manage the project information.
		///
		/// Origin must be the owner of the project.
		#[pallet::call_index(1)]
		#[pallet::weight(<T as Config>::WeightInfo::manage_project_info())]
		pub fn manage_project_info(
			origin: OriginFor<T>,
			index: ProjectIndex,
			project: ProjectInfo<T::AccountId>,
		) -> DispatchResult {
			let who = ensure_signed(origin)?;

			let old_project = Projects::<T>::take(&index).ok_or(Error::<T>::NoProjectAtIndex)?;
			ensure!(old_project.owner == who, Error::<T>::AccountIsNotProjectOwner);
			Projects::<T>::insert(&index, project.clone());

			Self::deposit_event(Event::ProjectInfoUpdated { project_index: index, owner: who });

			Ok(())
		}

		/// Unregister a project.
		///
		/// Origin must be the admin origin.
		///
		/// The project is removed, its current poll is not ended but the reward will not be sent
		/// and stay in the pot for the next round.
		#[pallet::call_index(2)]
		#[pallet::weight(<T as Config>::WeightInfo::unregister_project())]
		pub fn unregister_project(origin: OriginFor<T>, index: ProjectIndex) -> DispatchResult {
			T::AdminOrigin::ensure_origin_or_root(origin)?;

			Projects::<T>::take(&index).ok_or(Error::<T>::NoProjectAtIndex)?;

			Self::deposit_event(Event::ProjectUnregistered { project_index: index });

			Ok(())
		}

		/// Remove the automatic forwarding of a vote for a project.
		///
		/// Origin must be the voter account.
		///
		/// This is only effective if the voter doesn't vote again.
		/// Any new vote will override this removal.
		#[pallet::call_index(3)]
		#[pallet::weight(<T as Config>::WeightInfo::remove_automatic_forwarding())]
		pub fn remove_automatic_forwarding(
			origin: OriginFor<T>,
			project_index: ProjectIndex,
		) -> DispatchResult {
			let who = ensure_signed(origin)?;

			VotesToForward::<T>::remove(project_index, &who);

			Self::deposit_event(Event::AutomaticForwardingRemoved { project_index, who });

			Ok(())
		}
	}

	impl<T: Config> Pallet<T> {
		pub(crate) fn on_poll_end_round() {
			let Some(round_info) = Round::<T>::get() else {
				defensive!("This function must be called when ending an existing round");
				return
			};

			let round_index = NextRoundIndex::<T>::get()
				.checked_sub(1)
				.defensive_proof("There is a round so next round index is at least 1.")
				.unwrap_or_default();
			let pot_account = T::PotId::get().into_account_truncating();
			let pot_balance = T::Fungible::reducible_balance(
				&pot_account,
				Preservation::Expendable,
				Fortitude::Polite,
			);
			let now = <T as pallet_conviction_voting::Config<
				T::ConvictionVotingInstance,
			>>::BlockNumberProvider::current_block_number();

			for (project_index, status) in Polls::<T>::iter_prefix(round_index) {
				if let Some(project) = Projects::<T>::get(project_index) {
					let PollInfo::Ongoing(tally, _) = status else {
						defensive!("Impossible, polls are ended at the end of the round");
						continue;
					};
					// Note: from_rational always rounds down.
					let reward_percent = Perbill::from_rational(
						tally.ayes.saturating_sub(tally.nays),
						round_info.total_vote_amount,
					);
					let reward = reward_percent.mul_floor(pot_balance);
					if !reward.is_zero() {
						let _res = T::Fungible::transfer(
							&pot_account,
							&project.fund_dest,
							reward,
							Preservation::Expendable,
						)
						.defensive_proof("Transfer part of the pot balance")
						.inspect_err(|e| {
							log::error!(
								target: LOG_TARGET,
								"Failed to transfer reward for project {project_index}: {e:?}",
							)
						});
					}
				}
				Polls::<T>::insert(round_index, project_index, PollInfo::Completed(now, true));
			}

			// Send remaining funds to the treasury pot.
			let remaining_balance = T::Fungible::reducible_balance(
				&pot_account,
				Preservation::Expendable,
				Fortitude::Polite,
			);

			let _res = T::Fungible::transfer(
				&pot_account,
				&T::TreasuryAccountId::get(),
				remaining_balance,
				Preservation::Expendable,
			)
			.defensive_proof("Transfer remaining funds to treasury pot")
			.inspect_err(|e| {
				log::error!(
					target: LOG_TARGET,
					"Failed to transfer remaining funds from pot to treasury: {e:?}",
				)
			});

			Round::<T>::kill();
		}

		pub(crate) fn on_poll_new_round() {
			let round_index = NextRoundIndex::<T>::mutate(|next_index| {
				let index = *next_index;
				*next_index = next_index.saturating_add(1);
				index
			});

			let round_starting_block = <T as Config>::BlockNumberProvider::current_block_number();
			let round_info = RoundInfo {
				starting_block: round_starting_block,
				total_vote_amount: Default::default(),
			};
			Round::<T>::put(round_info);

			let mut project_ids = BTreeSet::new();

			for (project_index, _project_info) in Projects::<T>::iter() {
				let tally = TallyFor::<T>::from_parts(0u32.into(), 0u32.into(), 0u32.into());
				let poll_status = PollInfo::Ongoing(tally, Class);
				project_ids.insert(project_index);
				Polls::<T>::insert(round_index, project_index, poll_status);
			}

			let mut vote_record_state = VotesForwardingState::<T>::get();

			let reset = round_index != 0 && round_index % T::ResetVotesRoundNumber::get() == 0;
			if reset {
				vote_record_state.reset_round = Some(round_index);
			}
			// Note: if the forwarding of previous rounds is not finished, we just restart from
			// the beginning.
			vote_record_state.forwarding = ForwardingProcess::Start;
			VotesForwardingState::<T>::put(vote_record_state);
		}

		pub(crate) fn on_poll_on_idle_forward_votes(weight: &mut WeightMeter) {
			let base_weight = <T as Config>::WeightInfo::on_poll_on_idle_forward_votes(0);
			let loop_weight = <T as Config>::WeightInfo::on_poll_on_idle_forward_votes(1000)
				.saturating_sub(<T as Config>::WeightInfo::on_poll_on_idle_forward_votes(500)) /
				500;

			if weight.try_consume(base_weight).is_err() {
				return;
			}

			let mut vote_record_state = VotesForwardingState::<T>::get();
			let mut iterator = match &vote_record_state.forwarding {
				ForwardingProcess::Start => VotesToForward::<T>::iter(),
				ForwardingProcess::LastProcessed(k1, k2) => {
					let key = VotesToForward::<T>::hashed_key_for(*k1, k2.clone());
					VotesToForward::<T>::iter_from(key)
				},
				ForwardingProcess::Finished => return,
			};

			// This is an arbitrary limit, the loop is constraint by the weight meter.
			for _ in 0..10_000 {
				if weight.try_consume(loop_weight).is_err() {
					return;
				}

				if let Some((project_index, voter, vote)) = iterator.next() {
					let round_index = NextRoundIndex::<T>::get()
						.checked_sub(1)
						.defensive_proof("There is vote to forward, round is not 0")
						.unwrap_or_default();
					if Polls::<T>::contains_key(round_index, project_index) {
						if vote_record_state.reset_round.is_some_and(|reset| vote.round <= reset) {
							VotesToForward::<T>::remove(project_index, &voter);
						} else if vote.round < round_index {
							let _ = with_storage_layer(|| {
								pallet_conviction_voting::Pallet::<T, T::ConvictionVotingInstance>::vote(
									OriginFor::<T>::signed(voter.clone()),
									PollIndex::new(round_index, project_index),
									vote.vote,
								)
							})
							.inspect_err(|e| {
								log::error!(
									target: LOG_TARGET,
									"Failed to forward vote from voter {voter:?} for project \
									{project_index}: {e:?}",
								)
							});
						}
					} else {
						VotesToForward::<T>::remove(project_index, &voter);
					}
					vote_record_state.forwarding =
						ForwardingProcess::LastProcessed(project_index, voter);
				} else {
					vote_record_state.forwarding = ForwardingProcess::Finished;
					break;
				}
			}

			VotesForwardingState::<T>::put(vote_record_state);
		}

		// Helper to implement both `access_poll` and `try_access_poll` in one function.
		fn try_access_poll_inner<R, E>(
			index: <Self as Polling<TallyFor<T>>>::Index,
			f: impl FnOnce(
				PollStatus<
					&mut TallyFor<T>,
					<Self as Polling<TallyFor<T>>>::Moment,
					<Self as Polling<TallyFor<T>>>::Class,
				>,
			) -> Result<R, E>,
		) -> Result<R, E> {
			match Polls::<T>::get(index.round_index(), index.project_index()) {
				Some(PollInfo::Ongoing(ref mut tally, class)) => {
					let positive_tally_before = tally.ayes.saturating_sub(tally.nays);
					let r = f(PollStatus::Ongoing(tally, class.clone()))?;
					let positive_tally_after = tally.ayes.saturating_sub(tally.nays);
					if positive_tally_after != positive_tally_before {
						if let Some(mut round_info) = Round::<T>::get()
							.defensive_proof("Poll is ongoing, thus there is a round")
						{
							round_info.total_vote_amount = round_info
								.total_vote_amount
								.saturating_sub(positive_tally_before)
								.saturating_add(positive_tally_after);
							Round::<T>::put(round_info);
						}
					}
					Polls::<T>::insert(
						index.round_index(),
						index.project_index(),
						PollInfo::Ongoing(tally.clone(), class),
					);
					Ok(r)
				},
				Some(PollInfo::Completed(moment, result)) =>
					f(PollStatus::Completed(moment, result)),
				None => f(PollStatus::None),
			}
		}
	}

	impl<T: Config> frame_support::traits::Polling<TallyFor<T>> for Pallet<T> {
		type Index = PollIndex;
		type Class = Class;
		type Votes = pallet_conviction_voting::BalanceOf<T, T::ConvictionVotingInstance>;
		type Moment = pallet_conviction_voting::BlockNumberFor<T, T::ConvictionVotingInstance>;

		fn classes() -> Vec<Self::Class> {
			vec![Class]
		}

		fn as_ongoing(index: Self::Index) -> Option<(TallyFor<T>, Self::Class)> {
			Polls::<T>::get(index.round_index(), index.project_index()).and_then(
				|poll| match poll {
					PollInfo::Ongoing(tally, class) => Some((tally, class)),
					_ => None,
				},
			)
		}
		fn access_poll<R>(
			index: Self::Index,
			f: impl FnOnce(PollStatus<&mut TallyFor<T>, Self::Moment, Self::Class>) -> R,
		) -> R {
			pub enum Never {}
			match Self::try_access_poll_inner::<R, Never>(index, |s| Ok(f(s))) {
				Ok(r) => r,
			}
		}
		fn try_access_poll<R>(
			index: Self::Index,
			f: impl FnOnce(
				PollStatus<&mut TallyFor<T>, Self::Moment, Self::Class>,
			) -> Result<R, DispatchError>,
		) -> Result<R, DispatchError> {
			Self::try_access_poll_inner::<R, DispatchError>(index, f)
		}

		#[cfg(feature = "runtime-benchmarks")]
		fn end_ongoing(index: Self::Index, approved: bool) -> Result<(), ()> {
			let round_index = index.round_index();
			let project_index = index.project_index();
			let now = <T as pallet_conviction_voting::Config<T::ConvictionVotingInstance>>::BlockNumberProvider::current_block_number();
			match Polls::<T>::get(round_index, project_index) {
				Some(PollInfo::Ongoing(_, _)) => {
					Polls::<T>::insert(
						round_index,
						project_index,
						PollInfo::Completed(now, approved),
					);
					Ok(())
				},
				_ => Err(()),
			}
		}

		#[cfg(feature = "runtime-benchmarks")]
		fn max_ongoing() -> (Self::Class, u32) {
			(Class, T::MaxProjects::get())
		}

		#[cfg(feature = "runtime-benchmarks")]
		fn create_ongoing(_class: Self::Class) -> Result<Self::Index, ()> {
			if !Round::<T>::exists() {
				Round::<T>::put(RoundInfo {
					starting_block: <T as Config>::BlockNumberProvider::current_block_number(),
					total_vote_amount: Default::default(),
				});
			}
			let round_index = 0u32;
			NextRoundIndex::<T>::put(1);
			let project_index = NextProjectIndex::<T>::get();
			NextProjectIndex::<T>::put(project_index + 1);
			let tally = TallyFor::<T>::from_parts(0u32.into(), 0u32.into(), 0u32.into());
			let class = Class;
			Polls::<T>::insert(round_index, project_index, PollInfo::Ongoing(tally, class));
			Ok(PollIndex::new(round_index, project_index))
		}
	}

	impl<T: Config> VotingHooks<T::AccountId, PollIndex, VotesFor<T>> for Pallet<T> {
		fn on_before_vote(
			who: &T::AccountId,
			ref_index: PollIndex,
			vote: AccountVote<pallet_conviction_voting::BalanceOf<T, T::ConvictionVotingInstance>>,
		) -> DispatchResult {
			let round = NextRoundIndex::<T>::get()
				.checked_sub(1)
				.defensive_proof("Poll only exist in rounds, thus votes too.")
				.unwrap_or_default();
			let vote_in_session = VoteInSession { round, vote };
			VotesToForward::<T>::insert(ref_index.project_index(), who, vote_in_session);
			Ok(())
		}

		fn on_remove_vote(
			who: &T::AccountId,
			ref_index: PollIndex,
			status: pallet_conviction_voting::Status,
		) {
			use pallet_conviction_voting::Status;
			match status {
				Status::Ongoing => VotesToForward::<T>::remove(ref_index.project_index(), who),
				Status::Completed => (),
				Status::None => (),
			}
		}

		fn lock_balance_on_unsuccessful_vote(
			who: &T::AccountId,
			poll_index: PollIndex,
		) -> Option<pallet_conviction_voting::BalanceOf<T, T::ConvictionVotingInstance>> {
			let vote = pallet_conviction_voting::VotingFor::<T, T::ConvictionVotingInstance>::get(
				who, Class,
			);

			let votes = match vote {
				Voting::Casting(casting) => casting.votes,
				Voting::Delegating { .. } => return None,
			};

			votes
				.binary_search_by_key(&poll_index, |i| i.0)
				.ok()
				.and_then(|vote_index| votes.get(vote_index))
				.and_then(|vote| vote.1.locked_if(LockedIf::Always))
				.map(|(_period, balance)| balance)
		}

		#[cfg(feature = "runtime-benchmarks")]
		fn on_vote_worst_case(_who: &T::AccountId) {
			// Setup a simple poll for benchmarking: round 0, project 0
			let round_index = 0u32;
			let project_index = 0u32;
			let tally = TallyFor::<T>::from_parts(0u32.into(), 0u32.into(), 0u32.into());
			let class = Class;
			Polls::<T>::insert(round_index, project_index, PollInfo::Ongoing(tally, class));
		}

		#[cfg(feature = "runtime-benchmarks")]
		fn on_remove_vote_worst_case(_who: &T::AccountId) {
			// Setup a simple poll for benchmarking: round 0, project 0
			let round_index = 0u32;
			let project_index = 0u32;
			let tally = TallyFor::<T>::from_parts(0u32.into(), 0u32.into(), 0u32.into());
			let class = Class;
			Polls::<T>::insert(round_index, project_index, PollInfo::Ongoing(tally, class));
		}
	}
}
