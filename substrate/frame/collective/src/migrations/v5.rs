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

use crate::{Config, Pallet};
use codec::{Decode, Encode};
use frame_support::{
	pallet_prelude::{OptionQuery, TypeInfo, ValueQuery},
	sp_runtime::RuntimeDebug,
	traits::{Get, OnRuntimeUpgrade, QueryPreimage, StorePreimage},
	weights::Weight,
	BoundedVec, Identity,
};
use frame_system::pallet_prelude::BlockNumberFor;
use sp_std::prelude::*;

pub mod old {
	use super::*;

	#[frame_support::storage_alias]
	pub type Proposals<T: Config<I>, I: 'static> = StorageValue<
		Pallet<T, I>,
		BoundedVec<<T as frame_system::Config>::Hash, <T as Config<I>>::MaxProposals>,
		ValueQuery,
	>;

	#[frame_support::storage_alias]
	pub type ProposalOf<T: Config<I>, I: 'static> = StorageMap<
		Pallet<T, I>,
		Identity,
		<T as frame_system::Config>::Hash,
		<T as Config<I>>::Proposal,
		OptionQuery,
	>;

	#[frame_support::storage_alias]
	pub type Voting<T: Config<I>, I: 'static> = StorageMap<
		Pallet<T, I>,
		Identity,
		<T as frame_system::Config>::Hash,
		Votes<<T as frame_system::Config>::AccountId, BlockNumberFor<T>>,
		OptionQuery,
	>;

	#[frame_support::storage_alias]
	pub type Members<T: Config<I>, I: 'static> =
		StorageValue<Pallet<T, I>, Vec<<T as frame_system::Config>::AccountId>, ValueQuery>;

	/// Info for keeping track of a motion being voted on.
	#[derive(PartialEq, Eq, Clone, Encode, Decode, RuntimeDebug, TypeInfo)]
	pub struct Votes<AccountId, BlockNumber> {
		/// The proposal's unique index.
		pub index: crate::ProposalIndex,
		/// The number of approval votes that are needed to pass the motion.
		pub threshold: crate::MemberCount,
		/// The current set of voters that approved it.
		pub ayes: Vec<AccountId>,
		/// The current set of voters that rejected it.
		pub nays: Vec<AccountId>,
		/// The hard end time of this vote.
		pub end: BlockNumber,
	}
}
/// This migration moves all the state to v5 of Collective
pub struct VersionUncheckedMigrateToV5<T, I>(sp_std::marker::PhantomData<(T, I)>);
impl<T: Config<I>, I: 'static> OnRuntimeUpgrade for VersionUncheckedMigrateToV5<T, I> {
	#[cfg(feature = "try-runtime")]
	fn pre_upgrade() -> Result<Vec<u8>, sp_runtime::TryRuntimeError> {
		log::info!("pre-migration collective v5");
		let count = old::ProposalOf::<T, I>::iter().count();
		if old::Proposals::<T, I>::get().len() != count {
			log::info!("collective proposals count inconsistency");
		}
		if old::Members::<T, I>::get().len() > <T as Config<I>>::MaxMembers::get() as usize {
			log::info!("collective members exceeds MaxMembers");
		}

		Ok((old::Proposals::<T, I>::get()).encode())
	}

	fn on_runtime_upgrade() -> Weight {
		let mut weight = Weight::zero();

		// ProposalOf
		for (hash, proposal) in old::ProposalOf::<T, I>::drain() {
			weight = weight.saturating_add(T::DbWeight::get().reads_writes(1, 1));
			let Ok(new_hash) = <T as Config<I>>::Preimages::note(proposal.encode().into()) else {
				log::info!(
					target: "runtime::collective",
					"Failed to note preimage for proposal {:?}",
					hash,
				);
				continue
			};
			weight = weight.saturating_add(T::DbWeight::get().writes(1));
			if new_hash != hash {
				log::info!(
					target: "runtime::collective",
					"Preimage hash mismatch for proposal, expected {:?}, got {:?}",
					hash,
					new_hash,
				);
			}
			if !<T as Config<I>>::Preimages::is_requested(&new_hash) {
				log::info!(
					target: "runtime::collective",
					"Preimage for proposal {:?} was not requested",
					hash,
				);
			}
		}

		// Proposals
		old::Proposals::<T, I>::kill();
		weight = weight.saturating_add(T::DbWeight::get().writes(1));

		// Voting
		for (hash, vote) in old::Voting::<T, I>::drain() {
			weight = weight.saturating_add(T::DbWeight::get().reads_writes(1, 2));
			crate::Voting::<T, I>::insert(
				hash,
				crate::Votes::<T::AccountId, BlockNumberFor<T>, <T as Config<I>>::MaxMembers> {
					index: vote.index,
					threshold: vote.threshold,
					// the following operations are safe since the bound was previously enforced
					// by runtime code
					ayes: BoundedVec::truncate_from(vote.ayes),
					nays: BoundedVec::truncate_from(vote.nays),
					end: vote.end,
				},
			);
		}

		// Members
		crate::Members::<T, I>::put(BoundedVec::truncate_from(old::Members::<T, I>::get()));
		weight.saturating_add(T::DbWeight::get().reads_writes(1, 1))
	}

	#[cfg(feature = "try-runtime")]
	fn post_upgrade(state: Vec<u8>) -> Result<(), sp_runtime::TryRuntimeError> {
		use frame_support::ensure;
		log::info!("post-migration collective v5");

		let old_proposals = Vec::<T::Hash>::decode(&mut state.as_slice()).map_err(|_| {
			"the state parameter should be something that was generated by pre_upgrade"
		})?;

		ensure!(
			old_proposals.len() as u32 == crate::Voting::<T, I>::count(),
			"the number of proposals should be the same",
		);
		for old in old_proposals {
			ensure!(
				crate::Voting::<T, I>::contains_key(&old),
				"old proposal not found in new state"
			);
			ensure!(
				<T as Config<I>>::Preimages::is_requested(&old),
				"preimage for proposal not found in new state"
			);
		}
		ensure!(
			old::ProposalOf::<T, I>::iter().count() == 0,
			"collective v4 ProposalOf should be empty"
		);
		ensure!(
			old::Proposals::<T, I>::get().len() == 0,
			"collective v4 Proposals should be empty"
		);

		Pallet::<T, I>::do_try_state()
	}
}

/// [`VersionUncheckedMigrateToV5`] wrapped in a [`frame_support::migrations::VersionedMigration`],
/// ensuring the migration is only performed when on-chain version is 4.
pub type VersionCheckedMigrateToV5<T, I> = frame_support::migrations::VersionedMigration<
	4,
	5,
	VersionUncheckedMigrateToV5<T, I>,
	crate::pallet::Pallet<T, I>,
	<T as frame_system::Config>::DbWeight,
>;
