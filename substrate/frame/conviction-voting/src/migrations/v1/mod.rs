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
use crate::pallet::Config;
use frame_support::{
	migrations::{MigrationId, SteppedMigration, SteppedMigrationError},
	pallet_prelude::PhantomData,
	weights::WeightMeter,
};

// #[cfg(feature = "try-runtime")]
// use alloc::collections::btree_map::BTreeMap;

// #[cfg(feature = "try-runtime")]
// use alloc::vec::Vec;

mod benchmarking;
mod tests;
pub mod weights;

/// V0 types.
pub mod v0 {
	use super::Config;
	use crate::pallet::Pallet;
	use frame_support::{storage_alias, Twox64Concat};

    pub type BlockNumberFor<T, I> =
	<<T as Config<I>>::BlockNumberProvider as BlockNumberProvider>::BlockNumber;
    // type AccountIdLookupOf<T> = <<T as frame_system::Config>::Lookup as StaticLookup>::Source;
    pub type BalanceOf<T, I = ()> =
        <<T as Config<I>>::Currency as Currency<<T as frame_system::Config>::AccountId>>::Balance;
    pub type VotingOf<T, I = ()> = Voting<
        BalanceOf<T, I>,
        <T as frame_system::Config>::AccountId,
        BlockNumberFor<T, I>,
        PollIndexOf<T, I>,
        <T as Config<I>>::MaxVotes,
    >;
    // #[allow(dead_code)]
    // type DelegatingOf<T, I = ()> =
    //     Delegating<BalanceOf<T, I>, <T as frame_system::Config>::AccountId, BlockNumberFor<T, I>>;
    pub type TallyOf<T, I = ()> = Tally<BalanceOf<T, I>, <T as Config<I>>::MaxTurnout>;
    // pub type VotesOf<T, I = ()> = BalanceOf<T, I>;
    pub type PollIndexOf<T, I = ()> = <<T as Config<I>>::Polls as Polling<TallyOf<T, I>>>::Index;
    // #[cfg(feature = "runtime-benchmarks")]
    // pub type IndexOf<T, I = ()> = <<T as Config<I>>::Polls as Polling<TallyOf<T, I>>>::Index;
    pub type ClassOf<T, I = ()> = <<T as Config<I>>::Polls as Polling<TallyOf<T, I>>>::Class;

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
    #[codec(mel_bound(Balance: MaxEncodedLen, BlockNumber: MaxEncodedLen, PollIndex: MaxEncodedLen))]
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
        Balance: MaxEncodedLen, AccountId: MaxEncodedLen, BlockNumber: MaxEncodedLen,
        PollIndex: MaxEncodedLen,
    ))]
    pub enum Voting<Balance, AccountId, BlockNumber, PollIndex, MaxVotes>
    where
        MaxVotes: Get<u32>,
    {
        Casting(Casting<Balance, BlockNumber, PollIndex, MaxVotes>),
        Delegating(Delegating<Balance, AccountId, BlockNumber>),
    }

	#[storage_alias]
    pub type VotingFor<T: Config<I>, I: 'static = ()> = StorageDoubleMap<
		Pallet<T, I>,
		Twox64Concat,
		T::AccountId,
		Twox64Concat,
		ClassOf<T, I>,
		VotingOf<T, I>,
		ValueQuery,
	>;
}

/// Migrates storage items from v0 to v1.
pub struct SteppedMigrationV1<T: Config<I>, W: weights::WeightInfo, I = ()>(PhantomData<(T, W, I)>);
impl<T: Config<I>, W: weights::WeightInfo, I: 'static> SteppedMigration for SteppedMigrationV1<T, W, I> {
	type Cursor = (T::AccountId, v0::ClassOf<T, I>);
	type Identifier = MigrationId<24>;

	/// The identifier of this migration. Which should be globally unique.
	fn id() -> Self::Identifier {
		MigrationId { pallet_id: *CONVICTION_VOTING_ID, version_from: 0, version_to: 1 }
	}

    #[cfg(feature = "try-runtime")]
	fn pre_upgrade() -> Result<Vec<u8>, frame_support::sp_runtime::TryRuntimeError> {
		use codec::Encode;

        // Send over all voting data.
		Ok(v0::VotingFor::<T, I>::iter().collect::<BTreeMap<_, _>>().encode())
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

			let mut iter = if let Some((last_key_one, last_key_two) = cursor {
				// Iterate over value.
                let hashed_key = v0::VotingFor::<T, I>::hashed_key_for(last_key_one, last_key_two);
				v0::VotingFor::<T, I>::iter_from(hashed_key)
			} else {
				// If no cursor is provided, start iterating from the beginning.
				v0::VotingFor::<T, I>::iter()
			};

			// If there's a next item in the iterator, perform the migration.
			if let Some((last_key_one, last_key_two, value)) = iter.next() {
                
				// Migrate old vote data structure to the new one.
				
                // instantiate new voting structure
                let mut new_value = VotingOf::<T, I>::default();

                // pub struct Delegating<Balance, AccountId, BlockNumber> {
                //     pub balance: Balance,
                //     pub target: AccountId,
                //     pub conviction: Conviction,
                //     pub delegations: Delegations<Balance>,
                //     pub prior: PriorLock<BlockNumber, Balance>,
                // }

                // pub struct Casting<Balance, BlockNumber, PollIndex, MaxVotes>
                // {
                //     pub votes: BoundedVec<(PollIndex, AccountVote<Balance>), MaxVotes>,
                //     pub delegations: Delegations<Balance>,
                //     pub prior: PriorLock<BlockNumber, Balance>,
                // }

                match value {
                    Casting(casting) => {
                        new_value.votes = casting.votes;
                        for vote in casting.votes {
                            let new_vote = VoteRecord {
                                poll_index: vote.0,
                                maybe_vote: Some(vote.1),
                                retracted_votes: Default::default(),
                            }
                            new_value.votes.try_push(new_vote).map_err(|_| {
                                SteppedMigrationError::InsufficientWeight { required }
                            })
                        }
                        new_value.delegations = casting.delegations;
                        new_value.prior = casting.prior;
                    },
                    Delegating(delegating) => {
                        new_value.delegated_balance = delegating.balance;
                        new_value.maybe_delegate = Some(delegating.target);
                        new_value.maybe_conviction = Some(delegating.conviction);
                        new_value.delegations = delegating.delegations;
                        new_value.prior = delegating.prior;
                    },
                }

                VotingFor::<T, I>::insert(last_key_one, last_key_two, new_value);
                cursor = Some((last_key_one, last_key_two))
			} else {
                // Migration is complete.
				cursor = None;
				break
			}
		}
		Ok(cursor)
	}

	#[cfg(feature = "try-runtime")]
	fn post_upgrade(prev: Vec<u8>) -> Result<(), frame_support::sp_runtime::TryRuntimeError> {
		use codec::Decode;

		// Check the state of the storage after the migration.
		// let prev_map = BTreeMap::<u32, u32>::decode(&mut &prev[..])
		// 	.expect("Failed to decode the previous storage state");

		// Check the len of prev and post are the same.
		// assert_eq!(
		// 	MyMap::<T>::iter().count(),
		// 	prev_map.len(),
		// 	"Migration failed: the number of items in the storage after the migration is not the same as before"
		// );

		// for (key, value) in prev_map {
		// 	let new_value =
		// 		MyMap::<T>::get(key).expect("Failed to get the value after the migration");
		// 	assert_eq!(
		// 		value as u64, new_value,
		// 		"Migration failed: the value after the migration is not the same as before"
		// 	);
		// }

		Ok(())
	}
}
