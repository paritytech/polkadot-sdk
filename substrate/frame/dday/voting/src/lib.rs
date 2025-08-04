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

//! # DDay Voting by Proofs Pallet
//!
//! This pallet is a modified version of conviction voting that allows users to vote
//! using a generic proof from a different chain.
//!
//! ## Key Features
//! - Uses [`ProofDescription`] to describe the external chain and its proof.
//! - Proof validation is handled by `type Prover: ProofInterface`, which verifies the proof against
//!   the proof root stored by call `fn submit_proof_root_for_voting()`.
//! - The voting is not allowed, while no proof root is stored for `poll_index`,
//! - A valid proof is converted into `VotingPower(account_power: Balance, total: Balance)`.

#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;
extern crate core;

use codec::{Decode, DecodeWithMemTracking, Encode, MaxEncodedLen};
use frame_support::{
	dispatch::{DispatchResult, DispatchResultWithPostInfo},
	ensure,
	traits::{EnsureOriginWithArg, Get, Polling},
	CloneNoBound, EqNoBound, Parameter, PartialEqNoBound, RuntimeDebugNoBound,
};
use scale_info::TypeInfo;
use sp_runtime::traits::BlockNumberProvider;

mod conviction;
mod proofs;
mod types;
mod vote;
pub mod weights;
pub use self::{
	conviction::Conviction,
	pallet::*,
	proofs::*,
	types::{Delegations, Tally, UnvoteScope},
	vote::{AccountVote, Casting, Delegating, Vote, Voting, VotingPower},
	weights::WeightInfo,
};

#[cfg(feature = "runtime-benchmarks")]
pub mod benchmarking {
	//! TODO: FAIL-CI
}
#[cfg(test)]
mod mock {
	//! TODO: FAIL-CI
}
#[cfg(test)]
mod tests {
	//! TODO: FAIL-CI
}

pub type BlockNumberFor<T, I> =
	<<T as Config<I>>::BlockNumberProvider as BlockNumberProvider>::BlockNumber;

pub type VotingProofBalanceOf<T, I = ()> = ProofBalanceOf<<T as Config<I>>::Prover>;
pub type VotingProofAccountIdOf<T, I = ()> = ProofAccountIdOf<<T as Config<I>>::Prover>;
pub type VotingProofHashOf<T, I = ()> = ProofHashOf<<T as Config<I>>::Prover>;
pub type VotingProofBlockNumberOf<T, I = ()> = ProofBlockNumberOf<<T as Config<I>>::Prover>;
pub type VotingProofOf<T, I = ()> = ProofOf<<T as Config<I>>::Prover>;
pub type VotingPowerOf<T, I = ()> = VotingPower<VotesOf<T, I>>;
pub type VotesOf<T, I = ()> = VotingProofBalanceOf<T, I>;

pub type VotingOf<T, I = ()> = Voting<
	VotingProofBalanceOf<T, I>,
	VotingProofAccountIdOf<T, I>,
	BlockNumberFor<T, I>,
	PollIndexOf<T, I>,
	<T as Config<I>>::MaxVotes,
>;
pub type TallyOf<T, I = ()> = Tally<VotingProofBalanceOf<T, I>>;
pub type PollIndexOf<T, I = ()> = <<T as Config<I>>::Polls as Polling<TallyOf<T, I>>>::Index;
pub type ClassOf<T, I = ()> = <<T as Config<I>>::Polls as Polling<TallyOf<T, I>>>::Class;

/// Holds a `at_block` block number and corespondent `proof_root` hash used for voting by proof.
#[derive(
	Encode,
	Decode,
	DecodeWithMemTracking,
	CloneNoBound,
	EqNoBound,
	PartialEqNoBound,
	RuntimeDebugNoBound,
	TypeInfo,
	MaxEncodedLen,
)]
#[scale_info(skip_type_params(T, I))]
pub struct ProofRoot<T: Config<I>, I: 'static> {
	pub at_block: VotingProofBlockNumberOf<T, I>,
	pub proof_root: VotingProofHashOf<T, I>,
}

#[frame_support::pallet]
pub mod pallet {
	use super::*;
	use frame_support::{
		pallet_prelude::{
			IsType, OptionQuery, Pays, StorageDoubleMap, StorageMap, ValueQuery, Zero,
		},
		Twox64Concat,
	};
	use frame_system::pallet_prelude::{ensure_signed, OriginFor};
	use sp_runtime::ArithmeticError;

	#[pallet::pallet]
	pub struct Pallet<T, I = ()>(_);

	#[pallet::config]
	pub trait Config<I: 'static = ()>: frame_system::Config
	where
		VotingProofAccountIdOf<Self, I>: From<Self::AccountId>,
		VotingProofHashOf<Self, I>: Parameter + MaxEncodedLen + TypeInfo,
		VotingProofBlockNumberOf<Self, I>: Parameter + MaxEncodedLen + TypeInfo,
	{
		/// The overarching event type.
		#[allow(deprecated)]
		type RuntimeEvent: From<Event<Self, I>>
			+ IsType<<Self as frame_system::Config>::RuntimeEvent>;

		/// The Weight information for this pallet.
		type WeightInfo: WeightInfo;

		/// The implementation of the logic which conducts polls.
		type Polls: Polling<
			TallyOf<Self, I>,
			Votes = VotingProofBalanceOf<Self, I>,
			Moment = BlockNumberFor<Self, I>,
		>;

		/// The manager origin who can change the voting proof root (which enables voting).
		type ManagerOrigin: EnsureOriginWithArg<Self::RuntimeOrigin, PollIndexOf<Self, I>>;

		/// The maximum number of concurrent votes an account may have.
		///
		/// Also used to compute weight, an overly large value can lead to extrinsics with large
		/// weight estimation: see `delegate` for instance.
		#[pallet::constant]
		type MaxVotes: Get<u32>;
		/// Provider for the block number. Normally this is the `frame_system` pallet.
		type BlockNumberProvider: BlockNumberProvider;

		/// Proof verifier.
		type Prover: ProofInterface<RemoteProofRootOutput = ProofRoot<Self, I>>;
	}

	/// All voting for a particular voter in a particular voting class. We store the balance for the
	/// number of votes that we have recorded.
	#[pallet::storage]
	pub type VotingFor<T: Config<I>, I: 'static = ()> = StorageDoubleMap<
		_,
		Twox64Concat,
		VotingProofAccountIdOf<T, I>,
		Twox64Concat,
		ClassOf<T, I>,
		VotingOf<T, I>,
		ValueQuery,
	>;

	/// Stores a `poll_index` to `proof_root` mapping.
	///
	/// (Alternatively, we could store this proof root directly into the Tally,
	/// so we wouldn't need this storage item.)
	/// TODO: we could join this with `voting_power.total` with one call, so we won't need `record_totals` from the proofs.
	#[pallet::storage]
	pub type ProofRootForVoting<T: Config<I>, I: 'static = ()> =
		StorageMap<_, Twox64Concat, PollIndexOf<T, I>, ProofRoot<T, I>, OptionQuery>;

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config<I>, I: 'static = ()> {
		/// An account has voted.
		Voted { who: VotingProofAccountIdOf<T, I>, vote: AccountVote<VotingProofBalanceOf<T, I>> },
		/// A proof root was updated.
		ProofRootUpdated { previous: Option<ProofRoot<T, I>>, new: Option<ProofRoot<T, I>> },
	}

	#[pallet::error]
	pub enum Error<T, I = ()> {
		/// Poll is not ongoing.
		NotOngoing,
		/// Maximum number of votes reached.
		MaxVotesReached,
		/// The account is already delegating.
		AlreadyDelegating,
		/// Too high a balance was provided that the account cannot afford.
		InsufficientFunds,
		/// The proof is not valid.
		InvalidProof,
		/// The proof root is not valid.
		InvalidProofRoot,
		/// The proof root cannot be updated.
		CannotUpdateProofRoot,
	}

	#[pallet::call(weight = T::WeightInfo)]
	impl<T: Config<I>, I: 'static> Pallet<T, I> {
		/// Vote in a poll. If `vote.is_aye()`, the vote is to enact the proposal;
		/// otherwise it is a vote to keep the status quo.
		///
		/// The dispatch origin of this call must be _Signed_.
		///
		/// - `poll_index`: The index of the poll to vote for.
		/// - `vote`: The vote configuration.
		///
		/// Weight: `O(R)` where R is the number of polls the voter has voted on.
		#[pallet::call_index(0)]
		#[pallet::weight(T::WeightInfo::vote_new().max(T::WeightInfo::vote_existing()))]
		pub fn vote(
			origin: OriginFor<T>,
			#[pallet::compact] poll_index: PollIndexOf<T, I>,
			vote: AccountVote<VotingProofBalanceOf<T, I>>,
			proof: (VotingProofBlockNumberOf<T, I>, VotingProofOf<T, I>),
		) -> DispatchResultWithPostInfo {
			let who = ensure_signed(origin)?;
			ensure!(T::Polls::as_ongoing(poll_index).is_some(), Error::<T, I>::NotOngoing);
			let voting_power = Self::voting_power_of(who, proof, &poll_index)?;
			Self::try_vote(poll_index, vote, voting_power)
		}

		/// Remove a vote for a poll.
		#[pallet::call_index(1)]
		#[pallet::weight(T::WeightInfo::remove_vote())]
		pub fn remove_vote(
			origin: OriginFor<T>,
			_class: Option<ClassOf<T, I>>,
			_index: PollIndexOf<T, I>,
			_account_proof: VotingProofOf<T, I>,
		) -> DispatchResult {
			let _who = ensure_signed(origin)?;
			todo!("TODO: FAIL-CI - implement remove_vote")
		}

		#[pallet::call_index(2)]
		#[pallet::weight(T::WeightInfo::submit_proof_root_for_voting())]
		pub fn submit_proof_root_for_voting(
			origin: OriginFor<T>,
			#[pallet::compact] poll_index: PollIndexOf<T, I>,
			proof_root: Option<<<T as Config<I>>::Prover as ProofInterface>::RemoteProofRootInput>,
		) -> DispatchResult {
			let _ = T::ManagerOrigin::ensure_origin(origin, &poll_index)?;
			let (tally, _) = T::Polls::as_ongoing(poll_index).ok_or(Error::<T, I>::NotOngoing)?;
			ensure!(
				tally.ayes.is_zero() && tally.nays.is_zero(),
				Error::<T, I>::CannotUpdateProofRoot
			);

			// Verify proof root based on the input and update.
			ProofRootForVoting::<T, I>::try_mutate_exists(poll_index, |maybe_root| {
				let previous = maybe_root.clone();

				let new = match proof_root {
					Some(raw_root) => {
						let verified = T::Prover::verify_proof_root(raw_root)
							.ok_or(Error::<T, I>::InvalidProofRoot)?;
						*maybe_root = Some(verified.clone());
						Some(verified)
					},
					None => {
						*maybe_root = None;
						None
					},
				};
				Self::deposit_event(Event::ProofRootUpdated { previous, new });
				Ok(())
			})
		}
	}

	impl<T: Config<I>, I: 'static> Pallet<T, I> {
		/// Verifies the submitted proof and converts it into `VotingPower`.
		///
		/// The account `who` is a signer account from which we extract the `AccountId`
		/// contained in the proof.
		pub fn voting_power_of(
			who: T::AccountId,
			proof: (VotingProofBlockNumberOf<T, I>, VotingProofOf<T, I>),
			poll_index: &PollIndexOf<T, I>,
		) -> Result<
			(VotingProofAccountIdOf<T, I>, VotingProofBlockNumberOf<T, I>, VotingPowerOf<T, I>),
			Error<T, I>,
		> {
			// convert a local account to a proof account.
			let proving_who: VotingProofAccountIdOf<T, I> = who.into();

			// get the proof root
			let (proof_at_block, proof) = proof;
			let proof_root =
				ProofRootForVoting::<T, I>::get(poll_index)
					.and_then(|ProofRoot { at_block: voting_at_block, proof_root }| {
						if voting_at_block == proof_at_block {
							Some(proof_root)
						} else {
							None
						}
					})
					.ok_or(Error::<T, I>::NotOngoing)?;

			// verify the proof
			let voting_power = T::Prover::query_voting_power_for(&proving_who, proof_root, proof)
				.ok_or(Error::<T, I>::InvalidProof)?;
			Ok((proving_who, proof_at_block, voting_power))
		}

		/// Actually enact a vote, if legit.
		fn try_vote(
			poll_index: PollIndexOf<T, I>,
			vote: AccountVote<VotingProofBalanceOf<T, I>>,
			voting_power: (
				VotingProofAccountIdOf<T, I>,
				VotingProofBlockNumberOf<T, I>,
				VotingPowerOf<T, I>,
			),
		) -> DispatchResultWithPostInfo {
			let (remote_who, _, voting_power) = voting_power;
			ensure!(vote.balance() <= voting_power.account_power, Error::<T, I>::InsufficientFunds);
			T::Polls::try_access_poll(poll_index, |poll_status| {
				let (tally, class) =
					poll_status.ensure_ongoing().ok_or(Error::<T, I>::NotOngoing)?;
				VotingFor::<T, I>::try_mutate(remote_who.clone(), &class, |voting| {
					if let Voting::Casting(Casting { ref mut votes, delegations, .. }) = voting {
						match votes.binary_search_by_key(&poll_index, |i| i.0) {
							Ok(i) => {
								// Shouldn't be possible to fail, but we handle it gracefully.
								tally.remove(votes[i].1).ok_or(ArithmeticError::Underflow)?;
								if let Some(approve) = votes[i].1.as_standard() {
									tally.reduce(approve, *delegations);
								}
								votes[i].1 = vote;
							},
							Err(i) => {
								votes
									.try_insert(i, (poll_index, vote))
									.map_err(|_| Error::<T, I>::MaxVotesReached)?;
							},
						}
						// Shouldn't be possible to fail, but we handle it gracefully.
						tally.add(vote).ok_or(ArithmeticError::Overflow)?;
						if let Some(approve) = vote.as_standard() {
							tally.increase(approve, *delegations);
						}
					} else {
						return Err(Error::<T, I>::AlreadyDelegating.into());
					}

					// Record used total voting power from the proof.
					tally.record_totals(voting_power.total);

					Self::deposit_event(Event::Voted { who: remote_who.clone(), vote });
					Ok(Pays::No.into())
				})
			})
			.map_err(Into::into)
		}
	}
}
