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

#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

mod weights;
#[cfg(feature = "runtime-benchmarks")]
mod benchmarking;
#[cfg(test)]
mod mock;
#[cfg(test)]
mod tests;

pub use weights::WeightInfo;

pub use pallet::*;
#[frame_support::pallet]
pub mod pallet {
	use super::*;
	use alloc::collections::BTreeSet;
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

		type RoundDuration: Get<
			<<Self as Config>::BlockNumberProvider as BlockNumberProvider>::BlockNumber,
		>;

		type ConvictionVotingInstance;

		type Fungible: fungible::Mutate<Self::AccountId>;

		/// The number of rounds after which all the votes are reset.
		type ResetVotesRoundNumber: Get<u32>;

		/// Pot account Id
		#[pallet::constant]
		type PotId: Get<PalletId>;

		/// Weight information for extrinsics.
		type WeightInfo: weights::WeightInfo;

		/// The maximum number of projects that can be registered.
		type MaxProjects: Get<u32>;
	}

	pub type MomentFor<T> = <<T as pallet_conviction_voting::Config<
		<T as Config>::ConvictionVotingInstance,
	>>::Polls as Polling<TallyFor<T>>>::Moment;
	pub type TallyFor<T> =
		pallet_conviction_voting::TallyOf<T, <T as Config>::ConvictionVotingInstance>;
	pub type VotesFor<T> =
		pallet_conviction_voting::BalanceOf<T, <T as Config>::ConvictionVotingInstance>;

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

	pub type RoundIndex = u32;

	#[derive(
		Encode, Decode, MaxEncodedLen, Clone, Debug, TypeInfo, DecodeWithMemTracking, PartialEq, Eq,
	)]
	pub struct ProjectInfo<AccountId> {
		pub(crate) owner: AccountId,
		pub(crate) fund_dest: AccountId,
		pub(crate) name: BoundedVec<u8, ConstUint<256>>,
		pub(crate) description: BoundedVec<u8, ConstUint<256>>,
	}

	pub type ProjectIndex = u32;

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
		pub fn round_index(self) -> RoundIndex {
			(self.0 >> 32) as RoundIndex
		}
		pub fn project_index(self) -> ProjectIndex {
			(self.0 & 0xFFFFFFFF) as ProjectIndex
		}
		pub fn new(round_index: RoundIndex, project_index: ProjectIndex) -> Self {
			Self(((round_index as u64) << 32) | (project_index as u64))
		}
	}

	#[derive(Encode, Decode, MaxEncodedLen, Clone, Debug, TypeInfo, DecodeWithMemTracking)]
	pub struct RoundInfo<BlockNumber, Votes> {
		pub starting_block: BlockNumber,
		/// This follow a precise calculation. only overall yes amount for each project.
		pub total_vote_amount: Votes,
	}

	#[derive(Encode, Decode, MaxEncodedLen, Clone, Debug, TypeInfo, DecodeWithMemTracking)]
	pub enum PollInfo<Tally, Moment> {
		Ongoing(Tally, Class),
		Completed(Moment, bool),
	}

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

	#[pallet::storage]
	pub type NextRoundIndex<T> = StorageValue<_, RoundIndex, ValueQuery>;

	#[pallet::storage]
	pub type Round<T: Config> = StorageValue<
		_,
		RoundInfo<
			<<T as Config>::BlockNumberProvider as BlockNumberProvider>::BlockNumber,
			VotesFor<T>,
		>,
		OptionQuery,
	>;

	#[pallet::storage]
	pub type NextProjectIndex<T> = StorageValue<_, ProjectIndex, ValueQuery>;

	#[pallet::storage]
	pub type Projects<T: Config> =
		CountedStorageMap<_, Twox64Concat, ProjectIndex, ProjectInfo<T::AccountId>, OptionQuery>;

	#[pallet::storage]
	pub type VotesForwardingState<T: Config> =
		StorageValue<_, VotesForwardingStateInfo<T::AccountId>, ValueQuery>;

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

	#[derive(Encode, Decode, MaxEncodedLen, Clone, Debug, TypeInfo, DecodeWithMemTracking)]
	pub struct VoteInSession<Vote> {
		pub session: u32,
		pub vote: Vote,
	}

	#[derive(Encode, Decode, MaxEncodedLen, Clone, Debug, TypeInfo, DecodeWithMemTracking)]
	pub struct VotesForwardingStateInfo<AccountId> {
		pub session: u32,
		pub forwarding: VoteForwardingState<AccountId>,
		pub reset_session: Option<u32>,
	}

	impl<AccountId> Default for VotesForwardingStateInfo<AccountId> {
		fn default() -> Self {
			Self { session: 0, forwarding: VoteForwardingState::Start, reset_session: None }
		}
	}

	#[derive(Encode, Decode, MaxEncodedLen, Clone, Debug, TypeInfo, DecodeWithMemTracking)]
	pub enum VoteForwardingState<AccountId> {
		Start,
		LastProcessed(ProjectIndex, AccountId),
		Finished,
	}

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
		NoProjectAtIndex,
		AccountIsNotProjectOwner,
		AccountAlreadyHasProject,
		NoVoteForAccountAndProject,
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
					round_info.starting_block + T::RoundDuration::get()
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

			Pallet::<T>::on_poll_forward_votes(weight);
		}
	}

	#[pallet::call]
	impl<T: Config> Pallet<T> {
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

			Self::deposit_event(Event::ProjectRegistered { project_index: index, owner: project.owner });

			Ok(())
		}

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

		#[pallet::call_index(2)]
		#[pallet::weight(<T as Config>::WeightInfo::unregister_project())]
		pub fn unregister_project(
			origin: OriginFor<T>,
			index: ProjectIndex,
		) -> DispatchResult {
			T::AdminOrigin::ensure_origin_or_root(origin)?;

			Projects::<T>::take(&index).ok_or(Error::<T>::NoProjectAtIndex)?;

			Self::deposit_event(Event::ProjectUnregistered { project_index: index });

			Ok(())
		}

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
				defensive!();
				return
			};

			let round_index =
				NextRoundIndex::<T>::get().checked_sub(1).defensive().unwrap_or_default();
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
						let _ = T::Fungible::transfer(
							&pot_account,
							&project.fund_dest,
							reward,
							Preservation::Expendable,
						);
					}
				}
				Polls::<T>::insert(
					round_index,
					project_index,
					PollInfo::Completed(now, true),
				);
			}

			Round::<T>::kill();
		}

		pub(crate) fn on_poll_new_round() {
			let round_index = NextRoundIndex::<T>::mutate(|next_index| {
				let index = *next_index;
				*next_index = next_index.saturating_add(1);
				index
			});

			let round_starting_block =
				<T as Config>::BlockNumberProvider::current_block_number();
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

			let reset = round_index % T::ResetVotesRoundNumber::get() == 0;
			if reset {
				vote_record_state.reset_session = Some(vote_record_state.session);
			}
			vote_record_state.session += 1;
			vote_record_state.forwarding = VoteForwardingState::Start;
			VotesForwardingState::<T>::put(vote_record_state);
		}

		pub(crate) fn on_poll_forward_votes(_weight: &mut WeightMeter) {
			let mut vote_record_state = VotesForwardingState::<T>::get();
			let mut iterator = match &vote_record_state.forwarding {
				VoteForwardingState::Start => VotesToForward::<T>::iter(),
				VoteForwardingState::LastProcessed(k1, k2) => {
					let key = VotesToForward::<T>::hashed_key_for(k1.clone(), k2.clone());
					VotesToForward::<T>::iter_from(key)
				},
				VoteForwardingState::Finished => return,
			};

			for _ in 0..10_000 {
				if let Some((project_index, voter, vote)) = iterator.next() {
					let round_index =
						NextRoundIndex::<T>::get().checked_sub(1).defensive().unwrap_or_default();
					if Polls::<T>::contains_key(round_index, project_index) {
						if vote_record_state
							.reset_session
							.is_some_and(|reset| vote.session <= reset)
						{
							VotesToForward::<T>::remove(project_index, &voter);
						} else if vote.session < vote_record_state.session {
							let _ = with_storage_layer(|| {
								pallet_conviction_voting::Pallet::<T, T::ConvictionVotingInstance>::vote(
									OriginFor::<T>::signed(voter.clone()),
									PollIndex::new(round_index, project_index),
									vote.vote,
								)
							});
						}
					} else {
						VotesToForward::<T>::remove(project_index, &voter);
					}
					vote_record_state.forwarding =
						VoteForwardingState::LastProcessed(project_index, voter);
				} else {
					vote_record_state.forwarding = VoteForwardingState::Finished;
					break;
				}
			}

			VotesForwardingState::<T>::put(vote_record_state);
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
			match Polls::<T>::get(index.round_index(), index.project_index()) {
				Some(PollInfo::Ongoing(ref mut tally, class)) => {
					let positive_tally_before = tally.ayes.saturating_sub(tally.nays);
					let r = f(PollStatus::Ongoing(tally, class.clone()));
					let positive_tally_after = tally.ayes.saturating_sub(tally.nays);
					if positive_tally_after != positive_tally_before {
						if let Some(mut round_info) = Round::<T>::get().defensive() {
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
					r
				},
				Some(PollInfo::Completed(moment, result)) =>
					f(PollStatus::Completed(moment, result)),
				None => f(PollStatus::None),
			}
		}
		fn try_access_poll<R>(
			index: Self::Index,
			f: impl FnOnce(
				PollStatus<&mut TallyFor<T>, Self::Moment, Self::Class>,
			) -> Result<R, DispatchError>,
		) -> Result<R, DispatchError> {
			match Polls::<T>::get(index.round_index(), index.project_index()) {
				Some(PollInfo::Ongoing(ref mut tally, class)) => {
					let positive_tally_before = tally.ayes.saturating_sub(tally.nays);
					let r = f(PollStatus::Ongoing(tally, class.clone()))?;
					let positive_tally_after = tally.ayes.saturating_sub(tally.nays);
					if positive_tally_after != positive_tally_before {
						if let Some(mut round_info) = Round::<T>::get().defensive() {
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

		#[cfg(feature = "runtime-benchmarks")]
		fn end_ongoing(index: Self::Index, approved: bool) -> Result<(), ()> {
			let round_index = index.round_index();
			let project_index = index.project_index();
			let now = <T as pallet_conviction_voting::Config<T::ConvictionVotingInstance>>::BlockNumberProvider::current_block_number();
			match Polls::<T>::get(round_index, project_index) {
				Some(PollInfo::Ongoing(_, _)) => {
					Polls::<T>::insert(round_index, project_index, PollInfo::Completed(now, approved));
					Ok(())
				},
				_ => Err(())
			}
		}

		#[cfg(feature = "runtime-benchmarks")]
		fn max_ongoing() -> (Self::Class, u32) {
			(Class, T::MaxProjects::get())
		}

		#[cfg(feature = "runtime-benchmarks")]
		fn create_ongoing(_class: Self::Class) -> Result<Self::Index, ()> {
			let round_index = 0u32;
			let project_index = 0u32;
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
			let session = VotesForwardingState::<T>::get().session;
			let vote_in_session = VoteInSession { session, vote };
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

			votes.binary_search_by_key(&poll_index, |i| i.0)
				.ok()
				.and_then(|vote_index| votes.get(vote_index))
				.and_then(|vote|vote.1.locked_if(LockedIf::Always))
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
