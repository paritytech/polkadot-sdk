// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Polkadot.

// Polkadot is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Polkadot is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Polkadot.  If not, see <http://www.gnu.org/licenses/>.

//! Put implementations of functions from staging APIs here.

use crate::{configuration, inclusion, initializer, scheduler};
use alloc::{
	collections::{btree_map::BTreeMap, vec_deque::VecDeque},
	vec::Vec,
};
use frame_support::traits::{GetStorageVersion, StorageVersion};
use polkadot_primitives::{
	vstaging::CommittedCandidateReceiptV2 as CommittedCandidateReceipt, CoreIndex, Id as ParaId,
};

/// Returns the claimqueue from the scheduler
pub fn claim_queue<T: scheduler::Config>() -> BTreeMap<CoreIndex, VecDeque<ParaId>> {
	let config = configuration::ActiveConfig::<T>::get();
	// Extra sanity, config should already never be smaller than 1:
	let n_lookahead = config.scheduler_params.lookahead.max(1);
	// Workaround for issue #64.
	if scheduler::Pallet::<T>::on_chain_storage_version() == StorageVersion::new(2) {
		scheduler::migration::v2::ClaimQueue::<T>::get()
			.into_iter()
			.map(|(core_index, entries)| {
				(
					core_index,
					entries
						.into_iter()
						.map(|e| e.assignment.para_id())
						.take(n_lookahead as usize)
						.collect(),
				)
			})
			.collect()
	} else {
		scheduler::ClaimQueue::<T>::get()
			.into_iter()
			.map(|(core_index, entries)| {
				(
					core_index,
					entries.into_iter().map(|e| e.para_id()).take(n_lookahead as usize).collect(),
				)
			})
			.collect()
	}
}

/// Returns all the candidates that are pending availability for a given `ParaId`.
/// Deprecates `candidate_pending_availability` in favor of supporting elastic scaling.
pub fn candidates_pending_availability<T: initializer::Config>(
	para_id: ParaId,
) -> Vec<CommittedCandidateReceipt<T::Hash>> {
	<inclusion::Pallet<T>>::candidates_pending_availability(para_id)
}
