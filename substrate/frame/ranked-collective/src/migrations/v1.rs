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
use core::marker::PhantomData;
use codec::{Decode, Encode, EncodeLike, MaxEncodedLen};
use scale_info::TypeInfo;
use frame_support::{
    pallet_prelude::*, 
    storage_alias, 
    traits::UncheckedOnRuntimeUpgrade,
    migrations,
    CloneNoBound, EqNoBound, PartialEqNoBound, RuntimeDebugNoBound,
};
use log;

#[cfg(feature = "try-runtime")]
use sp_runtime::{TryRuntimeError, RuntimeDebug};

// initial version of storage types
pub mod v0 {
    use super::*;

    // old struct for `Tally`
    #[derive(
        CloneNoBound,
        PartialEqNoBound,
        EqNoBound,
        RuntimeDebugNoBound,
        TypeInfo,
        Encode,
        Decode,
        MaxEncodedLen,
    )]
    #[scale_info(skip_type_params(T, I, M))]
    #[codec(mel_bound())]
    pub struct OldTally<T, I, M: GetMaxVoters> {
	    bare_ayes: MemberIndex,
	    ayes: Votes,
	    nays: Votes,
	    dummy: PhantomData<(T, I, M)>,
    }

    pub type OldTallyOf<T, I = ()> = OldTally<T, I, Pallet<T, I>>;

    
    pub type OldReferendumInfoOf<T, I> = T::Polls::ReferendumInfo<
        TrackIdOf<T, I>,
        PalletsOriginOf<T>,
        BlockNumberFor<T>,
        BoundedCallOf<T, I>,
        BalanceOf<T, I>,
        OldTallyOf<T, I>, // Alias for Tally
        <T as frame_system::Config>::AccountId,
        ScheduleAddressOf<T, I>,
    >;

    // Old enum for `VoteRecord`
    #[derive(PartialEq, Eq, Clone, Copy, Encode, Decode, RuntimeDebug, TypeInfo, MaxEncodedLen)]
    pub enum OldVoteRecord {
	    /// Vote was an aye with given vote weight.
	    Aye(Votes),
	    /// Vote was a nay with given vote weight.
	    Nay(Votes),
    }



    //Storage aliase for storage item that consumes `Tally` struct (v0)
    #[pallet::storage]
    pub type ReferendumInfoForV0<T: Config<I>, I: 'static = ()> =
    StorageMap<_, Blake2_128Concat, ReferendumIndex, OldReferendumInfoOf<T, I>>;


    //Storage alias for storage item that consumes `VoteRecord` enum (v0)
    #[pallet::storage]
    pub type VotingV0<T: Config<I>, I: 'static = ()> = StorageDoubleMap<
        _,
        Blake2_128Concat,
        PollIndexOf<T, I>,
        Twox64Concat,
        T::AccountId,
        OldVoteRecord,
    >;
}


// New struct for `Tally`
#[derive(
	CloneNoBound,
	PartialEqNoBound,
	EqNoBound,
	RuntimeDebugNoBound,
	TypeInfo,
	Encode,
	Decode,
	MaxEncodedLen,
)]
#[scale_info(skip_type_params(T, I, M))]
#[codec(mel_bound())]
pub struct NewTally<T, I, M: GetMaxVoters> {
	bare_ayes: MemberIndex,
	out_of_rank_ayes: MemberIndex,
	out_of_rank_nays: MemberIndex,
	ayes: Votes,
	nays: Votes,
	dummy: PhantomData<(T, I, M)>,

}

pub type NewTallyOf<T, I = ()> = NewTally<T, I, Pallet<T, I>>;

pub type NewReferendumInfoOf<T, I> = T::Polls::ReferendumInfo<
        TrackIdOf<T, I>,
        PalletsOriginOf<T>,
        BlockNumberFor<T>,
        BoundedCallOf<T, I>,
        BalanceOf<T, I>,
        NewTallyOf<T, I>, // Alias for Tally
        <T as frame_system::Config>::AccountId,
        ScheduleAddressOf<T, I>,
    >;


//New enum for `VoteRecord`
#[derive(PartialEq, Eq, Clone, Copy, Encode, Decode, RuntimeDebug, TypeInfo, MaxEncodedLen)]
pub enum NewVoteRecord {
	/// Vote was an aye with given vote weight.
	Aye(Option<Votes>),
	/// Vote was a nay with given vote weight.
	Nay(Option<Votes>),
}

//Storage aliase for storage item that consumes `Tally` struct (v1)
#[pallet::storage]
pub type ReferendumInfoForV1<T: Config<I>, I: 'static = ()> =
StorageMap<_, Blake2_128Concat, ReferendumIndex, NewReferendumInfoOf<T, I>>;

//Storage alias for storage item that consumes `VoteRecord` enum (v1)
#[pallet::storage]
pub type VotingV1<T: Config<I>, I: 'static = ()> = StorageDoubleMap<
    _,
    Blake2_128Concat,
    PollIndexOf<T, I>,
    Twox64Concat,
    T::AccountId,
    NewVoteRecord,
>;


pub struct VotingAndRefInfoForV0ToV1<T, I = ()>(PhantomData<(T, I)>);
impl<T: Config<I>, I: 'static> UncheckedOnRuntimeUpgrade for VotingAndRefInfoForV0ToV1<T, I> {
    #[cfg(feature = "try-runtime")]
    fn pre_upgrade() -> Result<Vec<u8>, TryRuntimeError> {
        log::info!("Running pre-upgrade checks for V0 to V1 migration...");

        let old_referenda_count = v0::ReferendumInfoForV0::<T, I>::iter().count();
        let old_votes_count = v0::VotingV0::<T, I>::iter().count();

        // ensure old data exists for storage items that need a migration. this prevents unecessary runtime upgrade, eg for new runtimes 
        //that haven't populated these storage items yet. 
        // checking old refrenda count should suffice, since there can't be old votes if there're no old referenda
        ensure!(old_referenda_count > 0, "No referenda to migrate");

        log::info!(
            "Pre-upgrade checks complete: {} referenda and {} votes found.",
            old_referenda_count,
            old_votes_count
        );

        // metadata about the old state for post-upgrade validation.
        Ok((old_referenda_count, old_votes_count).encode())

    }

    fn on_runtime_upgrade() -> frame_support::weights::Weight {
        log::info!("Starting migration from V0 to V1...");

        let mut weight = 0;

        // Migrate `ReferendumInfoForV0` (OldTally to NewTally).
        v0::ReferendumInfoForV0::<T, I>::translate::<NewReferendumInfoOf<T, I>, _>(
            |_, old_info| {
                 // Transform `OldTally` to `NewTally`.
                 let new_info = match old_info {
                    v0::OldReferendumInfoOf::Ongoing(old_status) => {
                        let new_tally = NewTally {
                            bare_ayes: old_status.tally.bare_ayes,
                            out_of_rank_ayes: Default::default(), 
                            out_of_rank_nays: Default::default(),
                            ayes: old_status.tally.ayes,
                            nays: old_status.tally.nays,
                            dummy: Default::default(),
                        },
                        let new_status = ReferendumStatus {
                            track: old_status.track,
                            origin: old_status.origin,
                            proposal: old_status.proposal,
                            enactment: old_status.enactment,
                            submitted: old_status.submitted,
                            submission_deposit: old_status.submission_deposit,
                            decision_deposit: old_status.decision_deposit,
                            deciding: old_status.deciding,
                            tally: new_tally,
                            in_queue: old_status.in_queue,
                            alarm: old_status.alarm,
                        };

                        NewReferendumInfoOf::<T, I>::Ongoing(new_status)

                    },

                    v0::OldReferendumInfoOf::Approved(moment, deposit1, deposit2) => {
                        NewReferendumInfoOf::Approved(moment, deposit1, deposit2)
                    },

                    v0::OldReferendumInfoOf::Rejected(moment, deposit1, deposit2) => {
                        NewReferendumInfoOf::Rejected(moment, deposit1, deposit2)
                    },

                    v0::OldReferendumInfoOf::Cancelled(moment, deposit1, deposit2) => {
                        NewReferendumInfoOf::Cancelled(moment, deposit1, deposit2)
                    },

                    v0::OldReferendumInfoOf::TimedOut(moment, deposit1, deposit2) => {
                        NewReferendumInfoOf::TimedOut(moment, deposit1, deposit2)
                    },

                    v0::OldReferendumInfoOf::Killed(moment) => NewReferendumInfoOf::Killed(moment),
                };
                
                // increment weight. 1 read of `old_value` from storage, and one write of `new_value` back to storage
                weight += T::DbWeight::get().reads_writes(1, 1);

                Some(new_info)
            },
        );

        log::info!("ReferendumInfoFor migration to V1 complete.");


        // Migrate `VotingV0` (OldVoteRecord to NewVoteRecord).
        v0::VotingV0::<T, I>::translate::<NewVoteRecord, _>(|_, _, old_vote| {
            let new_vote = match old_vote {
                v0::OldVoteRecord::Aye(vote_weight) => NewVoteRecord::Aye(Some(vote_weight)),
                v0::OldVoteRecord::Nay(vote_weight) => NewVoteRecord::Nay(Some(vote_weight)),
            };

            // increment weight. 1 read of `old_vote` from storage, and one write of `new_vote` back to storage
            weight += T::DbWeight::get().reads_writes(1, 1);

            Some(new_vote)
        });
        log::info!("Voting migration to V1 complete.");

        log::info!("Migration from V0 to V1 finished. Total weight consumed during migration: {}", weight);
        weight
    }

    #[cfg(feature = "try-runtime")] 
    fn post_upgrade(state: Vec<u8>) -> Result<(), sp_runtime::TryRuntimeError> {
        log::info!("Running post-upgrade checks for V0 to V1 migration...");

        let (old_referenda_count, old_votes_count): (usize, usize) =
        Decode::decode(&mut &*state).map_err(|_| TryRuntimeError::Custom("State decoding failed".into()))?;

        let new_referenda_count = ReferendumInfoForV1::<T, I>::iter().count();
        let new_votes_count = VotingV1::<T, I>::iter().count();

        ensure!(
            new_referenda_count == old_referenda_count,
            "Mismatch in referenda count: expected {}, found {}",
            old_referenda_count,
            new_referenda_count
        );

        ensure!(
            new_votes_count == old_votes_count,
            "Mismatch in votes count: expected {}, found {}",
            old_votes_count,
            new_votes_count
        );

        assert!(v0::ReferendumInfoForV0::<T, I>::iter().count() == 0, "Old ReferendumInfoForV0 not empty");
        assert!(v0::VotingV0::<T, I>::iter().count() == 0, "Old VotingV0 not empty");

        log::info!("Post-upgrade checks passed.");

        Ok(())

    }

}

//Wrapping in versioned migration
pub type MigrateV0ToV1<T, I = ()> = 
    migrations::VersionedMigration<
        0,
        1,
        VotingAndRefInfoForV0ToV1<T, I>,
        Pallet<T, I>,
        <T as frame_system::Config>::DbWeight
    >;
