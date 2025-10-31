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

//! The v0 -> v1 multi-block migration.

extern crate alloc;

use super::CONVICTION_VOTING_ID;
use crate::{
	pallet::{Config, VotingFor as VotingForNew},
	AccountVote as NewAccountVote,
	VoteRecord,
	VotingOf as NewVotingOf,
};
use frame_support::{
	migrations::{MigrationId, SteppedMigration, SteppedMigrationError},
	pallet_prelude::PhantomData,
	weights::WeightMeter,
};
#[cfg(feature = "try-runtime")]
use frame_support::sp_runtime::TryRuntimeError;

mod benchmarking;
mod tests;
pub mod weights;

/// V0 types.
pub mod v0 {
	use super::Config;
	use crate::{pallet::Pallet, types::Tally, vote::PriorLock, Conviction, Delegations, Vote};
	use codec::{Decode, DecodeWithMemTracking, Encode, MaxEncodedLen};
	use frame_support::{
		pallet_prelude::{StorageDoubleMap, ValueQuery},
		storage_alias,
		traits::{Currency, Get, Polling},
		BoundedVec, Twox64Concat,
	};
	use scale_info::TypeInfo;
	use sp_runtime::{traits::BlockNumberProvider, RuntimeDebug};

	pub type BlockNumberFor<T, I> =
		<<T as Config<I>>::BlockNumberProvider as BlockNumberProvider>::BlockNumber;
	pub type BalanceOf<T, I = ()> =
		<<T as Config<I>>::Currency as Currency<<T as frame_system::Config>::AccountId>>::Balance;
	pub type TallyOf<T, I = ()> = Tally<BalanceOf<T, I>, <T as Config<I>>::MaxTurnout>;
	pub type PollIndexOf<T, I = ()> =
		<<T as Config<I>>::Polls as Polling<TallyOf<T, I>>>::Index;
	pub type ClassOf<T, I = ()> =
		<<T as Config<I>>::Polls as Polling<TallyOf<T, I>>>::Class;
	pub type VotingOf<T, I = ()> = Voting<
		BalanceOf<T, I>,
		<T as frame_system::Config>::AccountId,
		BlockNumberFor<T, I>,
		PollIndexOf<T, I>,
		<T as Config<I>>::MaxVotes,
	>;

	/// A vote for a referendum of a particular account.
	#[derive(
		Encode,
		Decode,
		DecodeWithMemTracking,
		Copy,
		Clone,
		Eq,
		PartialEq,
		RuntimeDebug,
		TypeInfo,
		MaxEncodedLen,
	)]
	pub enum AccountVote<Balance> {
		Standard { vote: Vote, balance: Balance },
		Split { aye: Balance, nay: Balance },
		SplitAbstain { aye: Balance, nay: Balance, abstain: Balance },
	}

	/// Information concerning the delegation of some voting power.
	#[derive(
		Encode,
		Decode,
		DecodeWithMemTracking,
		Default,
		Clone,
		Eq,
		PartialEq,
		RuntimeDebug,
		TypeInfo,
		MaxEncodedLen,
	)]
	pub struct Delegating<Balance, AccountId, BlockNumber> {
		pub balance: Balance,
		pub target: AccountId,
		pub conviction: Conviction,
		pub delegations: Delegations<Balance>,
		pub prior: PriorLock<BlockNumber, Balance>,
	}

	/// Information concerning the direct vote-casting of some voting power.
	#[derive(
		Encode,
		Decode,
		DecodeWithMemTracking,
		Clone,
		Eq,
		PartialEq,
		RuntimeDebug,
		TypeInfo,
		MaxEncodedLen,
	)]
	#[scale_info(skip_type_params(MaxVotes))]
	#[codec(mel_bound(
		Balance: MaxEncodedLen,
		BlockNumber: MaxEncodedLen,
		PollIndex: MaxEncodedLen
	))]
	pub struct Casting<Balance, BlockNumber, PollIndex, MaxVotes>
	where
		MaxVotes: Get<u32>,
	{
		pub votes: BoundedVec<(PollIndex, AccountVote<Balance>), MaxVotes>,
		pub delegations: Delegations<Balance>,
		pub prior: PriorLock<BlockNumber, Balance>,
	}

	/// An indicator for what an account is doing; it can either be delegating or voting.
	#[derive(
		Encode,
		Decode,
		DecodeWithMemTracking,
		Clone,
		Eq,
		PartialEq,
		RuntimeDebug,
		TypeInfo,
		MaxEncodedLen,
	)]
	#[scale_info(skip_type_params(MaxVotes))]
	#[codec(mel_bound(
		Balance: MaxEncodedLen,
		AccountId: MaxEncodedLen,
		BlockNumber: MaxEncodedLen,
		PollIndex: MaxEncodedLen
	))]
	pub enum Voting<Balance, AccountId, BlockNumber, PollIndex, MaxVotes>
	where
		MaxVotes: Get<u32>,
	{
		Casting(Casting<Balance, BlockNumber, PollIndex, MaxVotes>),
		Delegating(Delegating<Balance, AccountId, BlockNumber>),
	}

	impl<Balance, AccountId, BlockNumber, PollIndex, MaxVotes> Default
		for Voting<Balance, AccountId, BlockNumber, PollIndex, MaxVotes>
	where
		Balance: Default,
		BlockNumber: Default,
		MaxVotes: Get<u32>,
	{
		fn default() -> Self {
			Self::Casting(Casting {
				votes: Default::default(),
				delegations: Default::default(),
				prior: Default::default(),
			})
		}
	}

	#[storage_alias]
	pub type VotingFor<T: Config<I> + frame_system::Config, I: 'static> = StorageDoubleMap<
		Pallet<T, I>,
		Twox64Concat,
		<T as frame_system::Config>::AccountId,
		Twox64Concat,
		ClassOf<T, I>,
		VotingOf<T, I>,
		ValueQuery,
	>;
}

/// Migrates storage items from v0 to v1.
pub struct SteppedMigrationV1<T: Config<I>, W: weights::WeightInfo, I: 'static = ()>(
	PhantomData<(T, W, I)>,
);
impl<T: Config<I>, W: weights::WeightInfo, I: 'static> SteppedMigration
	for SteppedMigrationV1<T, W, I>
{
	type Cursor = (T::AccountId, v0::ClassOf<T, I>);
	type Identifier = MigrationId<24>;

	/// The identifier of this migration. Which should be globally unique.
	fn id() -> Self::Identifier {
		MigrationId { pallet_id: *CONVICTION_VOTING_ID, version_from: 0, version_to: 1 }
	}

	#[cfg(feature = "try-runtime")]
	fn pre_upgrade() -> Result<Vec<u8>, TryRuntimeError> {
		Ok(Default::default())
	}

	/// The logic for each step in the migratoin.
	fn step(
		mut cursor: Option<Self::Cursor>,
		meter: &mut WeightMeter,
	) -> Result<Option<Self::Cursor>, SteppedMigrationError> {
		let required = W::step();
		
        // No weight for even a single step.
		if meter.remaining().any_lt(required) {
			return Err(SteppedMigrationError::InsufficientWeight { required });
		}

		// We loop here to do as much progress as possible per step.
		loop {
			if meter.try_consume(required).is_err() {
				break;
			}

			let mut iter = if let Some((ref last_account, ref last_class)) = cursor {
				let raw_key =
					v0::VotingFor::<T, I>::hashed_key_for(last_account, last_class);
				v0::VotingFor::<T, I>::iter_from(raw_key)
			} else {
				// If no cursor is provided, start iterating from the beginning.
				v0::VotingFor::<T, I>::iter()
			};

			// If there's a next item in the iterator, perform the migration.
			if let Some((account, class, value)) = iter.next() {
				let mut new_value = NewVotingOf::<T, I>::default();

				match value {
					v0::Voting::Casting(casting) => {
						new_value.delegations = casting.delegations;
						new_value.prior = casting.prior;

						for (poll_index, vote) in casting.votes.into_iter() {
							let record = VoteRecord {
								poll_index,
								maybe_vote: Some(convert_account_vote(vote)),
								retracted_votes: Default::default(),
							};
							new_value
								.votes
								.try_push(record)
								.map_err(|_| SteppedMigrationError::Failed)?;
						}
					},
					v0::Voting::Delegating(delegating) => {
						new_value.delegated_balance = delegating.balance;
						new_value.maybe_delegate = Some(delegating.target);
						new_value.maybe_conviction = Some(delegating.conviction);
						new_value.delegations = delegating.delegations;
						new_value.prior = delegating.prior;
					},
				}

				VotingForNew::<T, I>::insert(&account, &class, new_value);
				cursor = Some((account, class));

			} else {
                // Migration is complete.
				cursor = None;
				break
			}

		}
		Ok(cursor)
	}

	#[cfg(feature = "try-runtime")]
	fn post_upgrade(_: Vec<u8>) -> Result<(), TryRuntimeError> {
		Ok(())
	}
}

fn convert_account_vote<Balance>(vote: v0::AccountVote<Balance>) -> NewAccountVote<Balance> {
	match vote {
		v0::AccountVote::Standard { vote, balance } =>
			NewAccountVote::Standard { vote, balance },
		v0::AccountVote::Split { aye, nay } => NewAccountVote::Split { aye, nay },
		v0::AccountVote::SplitAbstain { aye, nay, abstain } =>
			NewAccountVote::SplitAbstain { aye, nay, abstain },
	}
}
