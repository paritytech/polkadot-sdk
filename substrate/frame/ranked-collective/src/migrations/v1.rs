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

//! storage migration for ranked-collective pallet from v0 to v1


//imports
use super::*;
use crate as pallet_ranked_collective;
use core::marker::PhantomData;
use codec::{Decode, Encode, MaxEncodedLen};
use scale_info::TypeInfo;
use frame_support::{
    pallet_prelude::*, 
    traits::UncheckedOnRuntimeUpgrade,
    migrations,
};
use log;

#[cfg(feature = "try-runtime")]
use sp_runtime::{TryRuntimeError, RuntimeDebug};

// Old enum for `VoteRecord`
#[derive(PartialEq, Eq, Clone, Copy, Encode, Decode, RuntimeDebug, TypeInfo, MaxEncodedLen)]
pub enum OldVoteRecord {
    /// Vote was an aye with given vote weight.
    Aye(Votes),
    /// Vote was a nay with given vote weight.
    Nay(Votes),
}

impl OldVoteRecord {
    fn migrate_to_v1(self) -> VoteRecord {
        match self {
            OldVoteRecord::Aye(vote_weight) => VoteRecord::Aye(Some(vote_weight)),
            OldVoteRecord::Nay(vote_weight) => VoteRecord::Nay(Some(vote_weight)),
        }
    }
}

pub struct VotingV0ToV1<T, I = ()>(PhantomData<(T, I)>);
impl<T: pallet_ranked_collective::Config<I>, I: 'static> UncheckedOnRuntimeUpgrade for VotingV0ToV1<T, I> {
    #[cfg(feature = "try-runtime")]
    fn pre_upgrade() -> Result<Vec<u8>, TryRuntimeError> {
        log::info!("Running pre-upgrade checks for V0 to V1 migration...");

        ensure!(
            Pallet::<T>::on_chain_storage_version() == 0,
            "This migration will only execute if onchain version is zero."
        );

        let votes_count = Voting::<T, I>::iter().count();

        // ensure old data exists for storage items that need a migration. this prevents unecessary runtime upgrade, eg for new runtimes 
        //that haven't populated these storage items yet. 
        ensure!(votes_count > 0, "No votes to migrate");

        log::info!(
            "Pre-upgrade checks complete: {} votes found.",
            votes_count
        );

        // metadata about the old state for post-upgrade validation.
        Ok((votes_count).encode())

    }

    fn on_runtime_upgrade() -> frame_support::weights::Weight {

        // Migrate `VotingV0` (OldVoteRecord to NewVoteRecord).
        let mut translated = 0u64;
        Voting::<T, I>::translate::<OldVoteRecord, _>(|_, _, old_vote| {
            translated.saturating_inc();
            Some(old_vote.migrate_to_v1())
        });

        T::DbWeight::get()
			.reads_writes(translated, translated.saturating_add(1))
    }

    #[cfg(feature = "try-runtime")] 
    fn post_upgrade(state: Vec<u8>) -> Result<(), sp_runtime::TryRuntimeError> {
        log::info!("Running post-upgrade checks for V0 to V1 migration...");

        let votes_count_before: usize =
        Decode::decode(&mut &*state).map_err(|_| TryRuntimeError::Custom("State decoding failed".into()))?;

        let votes_count_after = Voting::<T, I>::iter().count();
        
        ensure!(
            votes_count_before == votes_count_after,
            "Mismatch in votes count: expected {}, found {}",
            votes_count_before,
            votes_count_after
        );

        ensure!(
            Pallet::<T>::on_chain_storage_version() == 1,
            "Post-migration onchain storage version should be 1"
        );


        Ok(())

    }

}

//Wrapping in versioned migration
pub type MigrateV0ToV1<T, I = ()> = 
    migrations::VersionedMigration<
        0,
        1,
        VotingV0ToV1<T, I>,
        Pallet<T, I>,
        <T as frame_system::Config>::DbWeight
    >;
