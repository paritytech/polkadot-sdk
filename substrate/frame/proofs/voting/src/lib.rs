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

//! # Voting by proofs pallet
//!
//! TODO: FAIL-CI - it is copied from conviction-voting palletso we need to adjust a remove.
//! TODO: FAIL-CI - we need to adjust and remove what is not needed here.
//! TODO: FAIL-CI - add desc.

#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;
extern crate core;

use frame_proofs_primitives::ProvideHash;
use frame_support::{
	dispatch::{DispatchResult, DispatchResultWithPostInfo},
	ensure,
	traits::{Get, Polling},
};
use sp_runtime::traits::BlockNumberProvider;
use sp_runtime::{traits::AtLeast32BitUnsigned, Perbill};

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
pub type TallyOf<T, I = ()> = Tally<VotingProofBalanceOf<T, I>, <T as Config<I>>::MaxTurnout>;
pub type PollIndexOf<T, I = ()> = <<T as Config<I>>::Polls as Polling<TallyOf<T, I>>>::Index;
pub type ClassOf<T, I = ()> = <<T as Config<I>>::Polls as Polling<TallyOf<T, I>>>::Class;

#[frame_support::pallet]
pub mod pallet {
	use super::*;
	use frame_support::{
		pallet_prelude::{IsType, Pays, StorageDoubleMap, ValueQuery},
		Twox64Concat,
	};
	use frame_system::pallet_prelude::{ensure_signed, OriginFor};
	use sp_runtime::ArithmeticError;

	#[pallet::pallet]
	pub struct Pallet<T, I = ()>(_);

	#[pallet::config]
	pub trait Config<I: 'static = ()>: frame_system::Config {
		/// The overarching event type.
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

		// TODO: FAIL-CI: add implementation `LockableCurrency` over proofed balance and local accounting
		// or we do just 1 account 1 vote and remove Balance as Vote
		// or we could fire some remote locking with HRMP to AssetHub, which could eventually be executed,
		// when AssetHub is unstalled.
		// type Currency: ReservableCurrency<Self::AccountId>
		// + LockableCurrency<Self::AccountId, Moment = BlockNumberFor<Self, I>>
		// + fungible::Inspect<Self::AccountId>;

		// TODO: FAIL-CI - we can don't have a hooks when referendum starts, we could add it, but also we could count it dynamically from proofs?
		type MaxTurnout: Get<VotingProofBalanceOf<Self, I>>;

		/// The maximum number of concurrent votes an account may have.
		///
		/// Also used to compute weight, an overly large value can lead to extrinsics with large
		/// weight estimation: see `delegate` for instance.
		#[pallet::constant]
		type MaxVotes: Get<u32>;
		/// Provider for the block number. Normally this is the `frame_system` pallet.
		type BlockNumberProvider: BlockNumberProvider;

		/// Proof verifier.
		type Prover: VerifyProof;
		/// Proof root provider.
		type ProofRootProvider: ProvideHash<
			Key = VotingProofBlockNumberOf<Self, I>,
			Hash = VotingProofHashOf<Self, I>,
		>;
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

	#[pallet::event]
	#[pallet::generate_deposit(pub fn deposit_event)]
	pub enum Event<T: Config<I>, I: 'static = ()> {
		/// An account has voted.
		Voted { who: VotingProofAccountIdOf<T, I>, vote: AccountVote<VotingProofBalanceOf<T, I>> },
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
		/// TBD: FAIL-CI
		InvalidProof,
		/// TBD: FAIL-CI
		InvalidProofRoot,
	}

	#[pallet::call(weight = T::WeightInfo)]
	impl<T: Config<I>, I: 'static> Pallet<T, I>
	where
		VotingProofAccountIdOf<T, I>: From<T::AccountId>,
	{
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
			let who_voting_power = Self::voting_power_of(who, proof)?;
			Self::try_vote(poll_index, vote, who_voting_power)
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

		// TODO: FAIL-CI - do we need other extrinsics here?
		// fn cleanup_poll(..) {..}
		// fn set_poll(stalled_state_root) {..} ?
	}

	impl<T: Config<I>, I: 'static> Pallet<T, I>
	where
		VotingProofAccountIdOf<T, I>: From<T::AccountId>,
	{
		/// Verifies the submitted proof and converts it into `VotingPower`.
		///
		/// The account `who` is a signer account from which we extract the `AccountId`
		/// contained in the proof.
		fn voting_power_of(
			who: T::AccountId,
			proof: (VotingProofBlockNumberOf<T, I>, VotingProofOf<T, I>),
		) -> Result<(VotingProofAccountIdOf<T, I>, VotingPowerOf<T, I>), Error<T, I>> {
			// convert local account to proof account.
			let proving_who: VotingProofAccountIdOf<T, I> = who.into();

			// get the proof root
			let (at_block, proof) = proof;
			let proof_root = T::ProofRootProvider::provide_hash_for(at_block)
				.ok_or(Error::<T, I>::InvalidProofRoot)?;

			// verify the proof
			let voting_power = T::Prover::query_voting_power_for(&proving_who, proof_root, proof)
				.ok_or(Error::<T, I>::InvalidProof)?;
			Ok((proving_who, voting_power))
		}

		/// Actually enact a vote, if legit.
		fn try_vote(
			poll_index: PollIndexOf<T, I>,
			vote: AccountVote<VotingProofBalanceOf<T, I>>,
			account_voting_power: (VotingProofAccountIdOf<T, I>, VotingPowerOf<T, I>),
		) -> DispatchResultWithPostInfo {
			let (who, voting_power) = account_voting_power;
			ensure!(vote.balance() <= voting_power.account_power, Error::<T, I>::InsufficientFunds);
			T::Polls::try_access_poll(poll_index, |poll_status| {
				let (tally, class) =
					poll_status.ensure_ongoing().ok_or(Error::<T, I>::NotOngoing)?;
				VotingFor::<T, I>::try_mutate(who.clone(), &class, |voting| {
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

					// TODO: FAIL-CI: Do we need some locking here?
					// TODO: FAIL-CI: We could `send_xcm(do_remote_lock())` to stalled chain?
					// Extend the lock to `balance` (rather than setting it) since we don't know what
					// other votes are in place.
					// Self::extend_lock(who, &class, vote.balance());

					Self::deposit_event(Event::Voted { who: who.clone(), vote });
					Ok(Pays::No.into())
				})
			})
			.map_err(Into::into)
		}
	}
}
