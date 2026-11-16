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

//! Helpers shared by the unit tests.

use alloc::vec::Vec;
use mmr_lib::util::{MemMMR, MemStore};
use polkadot_core_primitives::Hash;
use sp_core::H256;
use sp_runtime::BoundedVec;

use crate::{
	lift::MMRExtensionProof,
	mmr::{self, MmrFrontier, SpecMerge},
	stream::StreamId,
};

/// `n` distinct leaves.
pub(crate) fn leaves(n: u64) -> Vec<Hash> {
	(0..n).map(|i| H256::from_low_u64_be(i + 1)).collect()
}

/// Peaks-only frontier after the first `k` leaves.
pub(crate) fn frontier_at(all: &[Hash], k: usize) -> MmrFrontier {
	let mut frontier = MmrFrontier::new();
	for leaf in &all[..k] {
		frontier.append(*leaf);
	}
	frontier
}

/// `mmr_lib` node count for `k` leaves.
pub(crate) fn mmr_size(k: usize) -> u64 {
	mmr::mmr_size(k as u64)
}

/// The honest O(log n) extension proof from `k` leaves to `n` leaves (`0 < k < n`), via
/// `gen_ancestry_proof`.
pub(crate) fn extension(all: &[Hash], k: usize, n: usize) -> MMRExtensionProof {
	let store = MemStore::<Hash>::default();
	let mut mmr = MemMMR::<Hash, SpecMerge>::new(0, &store);
	for leaf in &all[..n] {
		mmr.push(*leaf).unwrap();
	}
	let proof = mmr.gen_ancestry_proof(mmr_size(k)).unwrap();
	MMRExtensionProof {
		leaf_count: n as u64,
		connecting_nodes: BoundedVec::truncate_from(
			proof.prev_peaks_proof.proof_items().iter().map(|(_, h)| *h).collect::<Vec<_>>(),
		),
	}
}

/// A channel stream to `recipient`.
pub(crate) fn ch(recipient: u32) -> StreamId {
	StreamId::Channel { recipient: recipient.into(), domain: 0, num: 0 }
}
