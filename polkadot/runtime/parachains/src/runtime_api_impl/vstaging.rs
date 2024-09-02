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
use polkadot_primitives::{
	vstaging::CommittedCandidateReceiptV2 as CommittedCandidateReceipt, CoreIndex, Id as ParaId,
};
use sp_runtime::traits::One;

/// Returns the claimqueue from the scheduler
pub fn claim_queue<T: scheduler::Config>() -> BTreeMap<CoreIndex, VecDeque<ParaId>> {
	let now = <frame_system::Pallet<T>>::block_number() + One::one();

	// This is needed so that the claim queue always has the right size (equal to
	// scheduling_lookahead). Otherwise, if a candidate is backed in the same block where the
	// previous candidate is included, the claim queue will have already pop()-ed the next item
	// from the queue and the length would be `scheduling_lookahead - 1`.
	<scheduler::Pallet<T>>::free_cores_and_fill_claim_queue(Vec::new(), now);
	let config = configuration::ActiveConfig::<T>::get();
	// Extra sanity, config should already never be smaller than 1:
	let n_lookahead = config.scheduler_params.lookahead.max(1);

	scheduler::ClaimQueue::<T>::get()
		.into_iter()
		.map(|(core_index, entries)| {
			// on cores timing out internal claim queue size may be temporarily longer than it
			// should be as the timed out assignment might got pushed back to an already full claim
			// queue:
			(
				core_index,
				entries.into_iter().map(|e| e.para_id()).take(n_lookahead as usize).collect(),
			)
		})
		.collect()
}

/// Returns all the candidates that are pending availability for a given `ParaId`.
/// Deprecates `candidate_pending_availability` in favor of supporting elastic scaling.
pub fn candidates_pending_availability<T: initializer::Config>(
	para_id: ParaId,
) -> Vec<CommittedCandidateReceipt<T::Hash>> {
	<inclusion::Pallet<T>>::candidates_pending_availability(para_id)
}
