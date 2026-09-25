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

//! Migration to storage version 5.
//!
//! Up to version 4 a proposal was keyed by the hash of the proposed call alone. From version 5
//! the key is [`Pallet::proposal_hash`], the hash of `(call, threshold)`. This migration re-keys
//! every active proposal so that on-chain state satisfies the new derivation.

use super::super::{Config, CostOf, Pallet, ProposalOf, Proposals, Voting, LOG_TARGET};
use alloc::vec::Vec;
use core::marker::PhantomData;
use frame_support::{
	migrations::VersionedMigration,
	pallet_prelude::*,
	traits::{DefensiveTruncateFrom, UncheckedOnRuntimeUpgrade},
	weights::Weight,
};
#[cfg(feature = "try-runtime")]
use sp_runtime::TryRuntimeError;

/// Re-key every active proposal from `hash(call)` to `hash((call, threshold))`.
///
/// The entries in [`ProposalOf`], [`Voting`] and [`CostOf`] are moved under the new key and
/// [`Proposals`] is rewritten in the same order. An entry that lacks its call or its votes is
/// left untouched under its old key and logged, since it cannot be re-keyed and removing it
/// could strand a proposer's deposit.
///
/// Use [`MigrateToV5`], which wraps this in a [`VersionedMigration`].
pub struct UncheckedMigrateToV5<T, I = ()>(PhantomData<(T, I)>);

impl<T: Config<I>, I: 'static> UncheckedOnRuntimeUpgrade for UncheckedMigrateToV5<T, I> {
	#[cfg(feature = "try-runtime")]
	fn pre_upgrade() -> Result<Vec<u8>, TryRuntimeError> {
		let proposals = Proposals::<T, I>::get();
		let entries: Vec<(T::Hash, <T as Config<I>>::Proposal, u32, u32, bool)> = proposals
			.iter()
			.filter_map(|old_hash| {
				let call = ProposalOf::<T, I>::get(old_hash)?;
				let votes = Voting::<T, I>::get(old_hash)?;
				Some((
					*old_hash,
					call,
					votes.threshold,
					votes.index,
					CostOf::<T, I>::contains_key(old_hash),
				))
			})
			.collect();
		Ok((proposals.len() as u32, entries).encode())
	}

	fn on_runtime_upgrade() -> Weight {
		let old_hashes = Proposals::<T, I>::get();
		let mut reads: u64 = 1;
		let mut writes: u64 = 1;
		let mut new_hashes = Vec::with_capacity(old_hashes.len());

		for old_hash in old_hashes.iter() {
			reads = reads.saturating_add(2);
			let (Some(call), Some(votes)) =
				(ProposalOf::<T, I>::get(old_hash), Voting::<T, I>::get(old_hash))
			else {
				log::warn!(
					target: LOG_TARGET,
					"Proposal {:?} has no call or no votes; leaving it under its old key.",
					old_hash,
				);
				new_hashes.push(*old_hash);
				continue;
			};

			let new_hash = Pallet::<T, I>::proposal_hash(&call, votes.threshold);
			new_hashes.push(new_hash);
			if new_hash == *old_hash {
				continue;
			}

			ProposalOf::<T, I>::remove(old_hash);
			ProposalOf::<T, I>::insert(new_hash, call);
			Voting::<T, I>::remove(old_hash);
			Voting::<T, I>::insert(new_hash, votes);
			writes = writes.saturating_add(4);

			reads = reads.saturating_add(1);
			if let Some(cost) = CostOf::<T, I>::take(old_hash) {
				CostOf::<T, I>::insert(new_hash, cost);
				writes = writes.saturating_add(2);
			}
		}

		let migrated = new_hashes.len();
		Proposals::<T, I>::put(BoundedVec::<_, T::MaxProposals>::defensive_truncate_from(
			new_hashes,
		));
		log::info!(target: LOG_TARGET, "Re-keyed {} proposals to storage version 5.", migrated);

		T::DbWeight::get().reads_writes(reads, writes)
	}

	#[cfg(feature = "try-runtime")]
	fn post_upgrade(state: Vec<u8>) -> Result<(), TryRuntimeError> {
		let (pre_len, entries): (u32, Vec<(T::Hash, <T as Config<I>>::Proposal, u32, u32, bool)>) =
			Decode::decode(&mut &state[..]).map_err(|_| "pre_upgrade provides a valid state")?;

		let proposals = Proposals::<T, I>::get();
		ensure!(proposals.len() as u32 == pre_len, "Proposal count changed during migration.");

		for (old_hash, call, threshold, index, had_cost) in entries {
			let new_hash = Pallet::<T, I>::proposal_hash(&call, threshold);
			ensure!(proposals.contains(&new_hash), "Re-keyed proposal missing from `Proposals`.");
			ensure!(
				ProposalOf::<T, I>::get(new_hash).as_ref() == Some(&call),
				"Re-keyed proposal missing from `ProposalOf`."
			);
			let votes = Voting::<T, I>::get(new_hash).ok_or("Re-keyed proposal has no votes.")?;
			ensure!(votes.threshold == threshold, "Threshold changed during migration.");
			ensure!(votes.index == index, "Proposal index changed during migration.");
			ensure!(
				CostOf::<T, I>::contains_key(new_hash) == had_cost,
				"Proposal cost was not moved with the proposal."
			);
			if new_hash != old_hash {
				ensure!(
					!ProposalOf::<T, I>::contains_key(old_hash) &&
						!Voting::<T, I>::contains_key(old_hash) &&
						!CostOf::<T, I>::contains_key(old_hash),
					"Old key still holds state after migration."
				);
			}
		}
		Ok(())
	}
}

/// [`UncheckedMigrateToV5`] wrapped in a [`VersionedMigration`], so that it runs exactly once
/// when the on-chain storage version is 4 and bumps it to 5.
pub type MigrateToV5<T, I> = VersionedMigration<
	4,
	5,
	UncheckedMigrateToV5<T, I>,
	Pallet<T, I>,
	<T as frame_system::Config>::DbWeight,
>;
