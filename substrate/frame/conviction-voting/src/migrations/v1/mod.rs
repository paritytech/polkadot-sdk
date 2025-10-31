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
use crate::{Pallet, pallet::{Config, VotingFor, MigrationOngoing},
	VoteRecord,
	VotingOf};
use frame_support::{
	migrations::{MigrationId, SteppedMigration, SteppedMigrationError},
	pallet_prelude::{PhantomData, StorageVersion, GetStorageVersion},
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
	use crate::{pallet::Pallet, types::Tally, vote::{PriorLock, AccountVote}, Conviction, Delegations};
    use codec::{Decode, DecodeWithMemTracking, Encode, MaxEncodedLen};
	use frame_support::{
		pallet_prelude::{StorageDoubleMap, ValueQuery},
		storage_alias,
		traits::{Currency, Get, Polling},
		BoundedVec, Twox64Concat,
	};
    use scale_info::TypeInfo;
	use sp_runtime::{traits::{BlockNumberProvider, Zero}, RuntimeDebug};

    pub type BlockNumberFor<T, I> =
	<<T as Config<I>>::BlockNumberProvider as BlockNumberProvider>::BlockNumber;
    pub type BalanceOf<T, I = ()> =
        <<T as Config<I>>::Currency as Currency<<T as frame_system::Config>::AccountId>>::Balance;
    pub type VotingOf<T, I = ()> = Voting<
        BalanceOf<T, I>,
        <T as frame_system::Config>::AccountId,
        BlockNumberFor<T, I>,
        PollIndexOf<T, I>,
        <T as Config<I>>::MaxVotes,
    >;
    pub type TallyOf<T, I = ()> = Tally<BalanceOf<T, I>, <T as Config<I>>::MaxTurnout>;
    pub type PollIndexOf<T, I = ()> = <<T as Config<I>>::Polls as Polling<TallyOf<T, I>>>::Index;
    pub type ClassOf<T, I = ()> = <<T as Config<I>>::Polls as Polling<TallyOf<T, I>>>::Class;

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

    impl<Balance: Default, AccountId, BlockNumber: Zero + Default, PollIndex, MaxVotes> Default
	for Voting<Balance, AccountId, BlockNumber, PollIndex, MaxVotes>
    where
        MaxVotes: Get<u32>,
    {
        fn default() -> Self {
            Voting::Casting(Casting {
                votes: Default::default(),
                delegations: Default::default(),
                prior: Default::default(),
            })
        }
    }

	#[storage_alias]
    pub type VotingFor<T: Config<I>, I: 'static> = StorageDoubleMap<
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
pub struct SteppedMigrationV1<T: Config<I>, W: weights::WeightInfo, I: 'static = ()>(PhantomData<(T, W, I)>);
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
        if Pallet::<T,I>::on_chain_storage_version() != Self::id().version_from as u16 {
            return Ok(None);
        }
		
        // No weight for even a single step.
		let required = W::step();
		if meter.remaining().any_lt(required) {
			return Err(SteppedMigrationError::InsufficientWeight { required });
		}

		// We loop here to do as much progress as possible per step.
		loop {
			if meter.try_consume(required).is_err() {
				break;
			}

			let mut iter = if let Some((ref last_account, ref last_class)) = cursor {
				// Iterate over value.
                let hashed_key = v0::VotingFor::<T, I>::hashed_key_for(last_account, last_class);
				v0::VotingFor::<T, I>::iter_from(hashed_key)
			} else {
				// If no cursor is provided, start iterating from the beginning.
                MigrationOngoing::<T,I>::set(true);
				v0::VotingFor::<T, I>::iter()
			};

			// If there's a next item in the iterator, perform the migration.
			if let Some((account, class, value)) = iter.next() {
                
				// Migrate old vote data structure to the new one.
                let mut new_voting = VotingOf::<T, I>::default();

                match value {
                    v0::Voting::Casting(v0::Casting { votes, delegations, prior }) => {
                        for (poll_index, vote) in votes {
                            let new_record = VoteRecord {
                                poll_index,
                                maybe_vote: Some(vote),
                                retracted_votes: Default::default(),
                            };
                            new_voting.votes.try_push(new_record).map_err(|_| SteppedMigrationError::Failed)?;
                        }
                        new_voting.delegations = delegations;
                        new_voting.prior = prior;
                    },
                    v0::Voting::Delegating(v0::Delegating { balance, target, conviction, delegations, prior }) => {
                        new_voting.delegated_balance = balance;
                        new_voting.maybe_delegate = Some(target);
                        new_voting.maybe_conviction = Some(conviction);
                        new_voting.delegations = delegations;
                        new_voting.prior = prior;
                    },
                }

                // Insert and move cursor.
                VotingFor::<T, I>::insert(&account, &class, new_voting);
                cursor = Some((account, class))
			} else {
                // Migration is complete.
				cursor = None;
                MigrationOngoing::<T,I>::set(false);
                StorageVersion::new(Self::id().version_to as u16).put::<Pallet<T,I>>();
				break
			}
		}
		Ok(cursor)
	}

	#[cfg(feature = "try-runtime")]
	fn post_upgrade(prev: Vec<u8>) -> Result<(), frame_support::sp_runtime::TryRuntimeError> {
		use codec::Decode;

        // Check state
        // Check migration flag set to false
        // Storage version is 1

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
