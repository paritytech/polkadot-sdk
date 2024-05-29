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

use crate::{inclusion, initializer, scheduler};
use polkadot_primitives::{CommittedCandidateReceipt, CoreIndex, Id as ParaId};
use sp_runtime::traits::One;
use sp_std::{
	collections::{btree_map::BTreeMap, vec_deque::VecDeque},
	vec::Vec,
};

/// Returns the claimqueue from the scheduler
pub fn claim_queue<T: scheduler::Config>() -> BTreeMap<CoreIndex, VecDeque<ParaId>> {
	let now = <frame_system::Pallet<T>>::block_number() + One::one();

	// This explicit update is only strictly required for session boundaries:
	//
	// At the end of a session we clear the claim queues: Without this update call, nothing would be
	// scheduled to the client.
	<scheduler::Pallet<T>>::free_cores_and_fill_claimqueue(Vec::new(), now);

	scheduler::ClaimQueue::<T>::get()
		.into_iter()
		.map(|(core_index, entries)| {
			(core_index, entries.into_iter().map(|e| e.para_id()).collect())
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
