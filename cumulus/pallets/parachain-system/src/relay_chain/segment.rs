// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Cumulus.
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

//! Maintenance of the unincluded segment against the latest relay-chain head.

use alloc::vec::Vec;
use frame_support::{traits::Get, weights::Weight};
use sp_runtime::traits::Hash;

use crate::{
	consensus_hook, relay_chain::relay_state_snapshot::RelayChainStateProof,
	AggregatedUnincludedSegment, Ancestor, Config, UnincludedSegment,
};

/// Drop blocks from the unincluded segment with respect to the latest parachain head.
pub(crate) fn maybe_drop_included_ancestors<T: Config>(
	relay_state_proof: &RelayChainStateProof,
	capacity: consensus_hook::UnincludedSegmentCapacity,
) -> Weight {
	let mut weight_used = Weight::zero();
	// If the unincluded segment length is nonzero, then the parachain head must be present.
	let para_head =
		relay_state_proof.read_included_para_head().ok().map(|h| T::Hashing::hash(&h.0));

	let unincluded_segment_len = <UnincludedSegment<T>>::decode_len().unwrap_or(0);
	weight_used += T::DbWeight::get().reads(1);

	// Clean up unincluded segment if nonempty.
	let included_head = match (para_head, capacity.is_expecting_included_parent()) {
		(Some(h), true) => {
			assert_eq!(
				h,
				frame_system::Pallet::<T>::parent_hash(),
				"expected parent to be included"
			);

			h
		},
		(Some(h), false) => h,
		(None, true) => {
			// All this logic is essentially a workaround to support collators which
			// might still not provide the included block with the state proof.
			frame_system::Pallet::<T>::parent_hash()
		},
		(None, false) => panic!("included head not present in relay storage proof"),
	};

	let new_len = {
		let para_head_hash = included_head;
		let dropped: Vec<Ancestor<T::Hash>> = <UnincludedSegment<T>>::mutate(|chain| {
			// Drop everything up to (inclusive) the block with an included para head, if
			// present.
			let idx = chain
				.iter()
				.position(|block| {
					let head_hash = block
						.para_head_hash()
						.expect("para head hash is updated during block initialization; qed");
					head_hash == &para_head_hash
				})
				.map_or(0, |idx| idx + 1); // inclusive.

			chain.drain(..idx).collect()
		});
		weight_used += T::DbWeight::get().reads_writes(1, 1);

		let new_len = unincluded_segment_len - dropped.len();
		if !dropped.is_empty() {
			<AggregatedUnincludedSegment<T>>::mutate(|agg| {
				let agg = agg
					.as_mut()
					.expect("dropped part of the segment wasn't empty, hence value exists; qed");
				for block in dropped {
					agg.subtract(&block);
				}
			});
			weight_used += T::DbWeight::get().reads_writes(1, 1);
		}

		new_len as u32
	};

	// Current block validity check: ensure there is space in the unincluded segment.
	//
	// If this fails, the parachain needs to wait for ancestors to be included before
	// a new block is allowed.
	assert!(
		new_len < capacity.get(),
		"No space left for the block in the unincluded segment: new_len({new_len}) < capacity({})",
		capacity.get()
	);
	weight_used
}
