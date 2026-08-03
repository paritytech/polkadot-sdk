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

//! Sender-side fixtures for tests: streams with deterministic payloads,
//! their frontiers/roots/proofs at any point, and lift assembly against the
//! commitment tree over the streams' current roots.
//!
//! Shared between the lift verification tests (this crate and the
//! `validate_block` wrapper) and the node-side lift assembly (issue 11):
//! [`SourceFixture::lift`] builds exactly what the assembler produces from
//! public data, so both sides can be tested against the same material.
//! `std`-only — never part of consensus code.

use alloc::{collections::BTreeMap, vec::Vec};
use mmr_lib::{
	leaf_index_to_mmr_size,
	util::{MemMMR, MemStore},
	MMRStoreReadOps,
};
use polkadot_core_primitives::Hash;
use polkadot_parachain_primitives::primitives::Id as ParaId;
use polkadot_primitives::StreamsRoot;

use crate::{
	hash_leaf,
	lift::RequiresLift,
	mmr::{MMRExtensionProof, MmrFrontier, MmrInclusionProof, MmrRoot, SpecHasher, SpecMerge},
	record::{ConsumptionRecord, Interval},
	stream_id::StreamId,
	tree::{compute_streams_root, prove_stream, TreeInclusionProof},
	LEAF_VERSION,
};

type Merge = SpecMerge<SpecHasher>;

/// One sender-side stream over `count` deterministic payloads, able to
/// reproduce its state (frontier, root, proofs) after any prefix.
pub struct StreamFixture {
	/// The stream's id.
	pub id: StreamId,
	leaves: Vec<Hash>,
}

impl StreamFixture {
	/// A stream carrying `count` deterministic messages.
	pub fn new(id: StreamId, count: u64) -> Self {
		let leaves = (0..count)
			.map(|i| hash_leaf::<SpecHasher>(LEAF_VERSION, &i.to_le_bytes()))
			.collect();
		Self { id, leaves }
	}

	/// A stream carrying exactly the given payloads — for tests that need
	/// the leaves to decode meaningfully (`SpecMsgKind` payloads, register
	/// leaves) or to mirror payloads fed elsewhere.
	pub fn from_payloads(id: StreamId, payloads: &[Vec<u8>]) -> Self {
		let leaves = payloads
			.iter()
			.map(|payload| hash_leaf::<SpecHasher>(LEAF_VERSION, payload))
			.collect();
		Self { id, leaves }
	}

	/// Total number of messages in the stream.
	pub fn leaf_count(&self) -> u64 {
		self.leaves.len() as u64
	}

	/// The frontier after the first `leaf_count` messages.
	pub fn frontier_at(&self, leaf_count: u64) -> MmrFrontier {
		let mut frontier = MmrFrontier::default();
		for leaf in &self.leaves[..leaf_count as usize] {
			frontier.append_leaf(*leaf);
		}
		frontier
	}

	/// The stream root after the first `leaf_count` messages.
	pub fn root_at(&self, leaf_count: u64) -> MmrRoot {
		self.frontier_at(leaf_count).root()
	}

	/// The stream's current root (all messages).
	pub fn current_root(&self) -> MmrRoot {
		self.root_at(self.leaf_count())
	}

	/// A channel consumption interval: entered the block at `from` consumed
	/// messages, left it at `to`.
	pub fn interval(&self, from: u64, to: u64) -> Interval {
		Interval { start: self.root_at(from), end: self.frontier_at(to) }
	}

	/// A register/event read context pinned `at` messages in: nothing
	/// advances (`start == end.root()`).
	pub fn read_context(&self, at: u64) -> Interval {
		Interval { start: self.root_at(at), end: self.frontier_at(at) }
	}

	/// The head read after the first `leaf_count` messages: the inclusion
	/// proof of the then-last leaf, as a register/event read carries it.
	/// This is the node-side generation path (`gen_proof` over the full
	/// MMR), matching what issue 11's inherent provider produces.
	pub fn head_proof(&self, leaf_count: u64) -> MmrInclusionProof {
		assert!(
			leaf_count > 0 && leaf_count <= self.leaf_count(),
			"a head read needs a non-empty prefix of this stream"
		);
		let store = MemStore::default();
		{
			let mut mmr = MemMMR::<Hash, Merge>::new(0, &store);
			for leaf in &self.leaves[..leaf_count as usize] {
				mmr.push(*leaf).expect("push into in-memory MMR never fails; qed");
			}
			mmr.commit().expect("commit into in-memory store never fails; qed");
		}
		let mmr_size = leaf_index_to_mmr_size(leaf_count - 1);
		let mmr = MemMMR::<Hash, Merge>::new(mmr_size, &store);
		let proof = mmr
			.gen_proof(alloc::vec![mmr_lib::leaf_index_to_pos(leaf_count - 1)])
			.expect("the head leaf exists in the committed MMR; qed");
		MmrInclusionProof { mmr_size, items: proof.proof_items().to_vec() }
	}

	/// Extension proof from the state after `from` messages to the state
	/// after `to` messages — the canonical empty proof when equal. This is
	/// the node-side generation path (`gen_ancestry_proof` over the full
	/// MMR), matching what issue 11's assembler produces.
	pub fn extension_proof(&self, from: u64, to: u64) -> MMRExtensionProof {
		assert!(from <= to && to <= self.leaf_count(), "not a forward extension of this stream");
		if from == to {
			return MMRExtensionProof::empty();
		}

		let store = MemStore::default();
		{
			let mut mmr = MemMMR::<Hash, Merge>::new(0, &store);
			for leaf in &self.leaves[..to as usize] {
				mmr.push(*leaf).expect("push into in-memory MMR never fails; qed");
			}
			mmr.commit().expect("commit into in-memory store never fails; qed");
		}
		let new_mmr_size = leaf_index_to_mmr_size(to - 1);
		let mmr = MemMMR::<Hash, Merge>::new(new_mmr_size, &store);

		if from == 0 {
			// From empty, the connecting nodes are exactly the new peaks.
			let connecting_nodes = mmr_lib::helper::get_peaks(new_mmr_size)
				.into_iter()
				.map(|pos| {
					let hash = mmr
						.store()
						.get_elem(pos)
						.expect("store read never fails; qed")
						.expect("peaks of a committed MMR exist; qed");
					(pos, hash)
				})
				.collect();
			return MMRExtensionProof { leaf_count: to, connecting_nodes };
		}

		let ancestry = mmr
			.gen_ancestry_proof(leaf_index_to_mmr_size(from - 1))
			.expect("`from < to <= leaf_count` is a valid ancestor; qed");
		MMRExtensionProof::from_ancestry_proof(to, &ancestry)
	}
}

/// A source parachain fixture: its streams plus the commitment tree over
/// their *current* roots — everything a lift binds against.
pub struct SourceFixture {
	/// The source chain's id.
	pub para: ParaId,
	/// The source's streams, sorted by id (canonical record order).
	pub streams: Vec<StreamFixture>,
}

impl SourceFixture {
	/// New fixture; streams are sorted by id.
	pub fn new(para: ParaId, mut streams: Vec<StreamFixture>) -> Self {
		streams.sort_by(|a, b| a.id.cmp(&b.id));
		Self { para, streams }
	}

	/// The stream fixture with the given id.
	pub fn stream(&self, id: &StreamId) -> &StreamFixture {
		self.streams
			.iter()
			.find(|stream| stream.id == *id)
			.expect("fixture stream ids are chosen by the test; qed")
	}

	fn entries(&self) -> BTreeMap<StreamId, MmrRoot> {
		self.streams.iter().map(|stream| (stream.id, stream.current_root())).collect()
	}

	/// The source's committed [`StreamsRoot`] over all current stream
	/// roots — what a correct lift must land on.
	pub fn streams_root(&self) -> StreamsRoot {
		compute_streams_root(&self.entries()).expect("fixture has at least one stream; qed")
	}

	/// The tree inclusion proof of `id`'s current root under
	/// [`Self::streams_root`].
	pub fn tree_proof(&self, id: &StreamId) -> TreeInclusionProof {
		prove_stream(&self.entries(), id).expect("fixture stream ids exist in the tree; qed")
	}

	/// The lift for `id` whose recorded chain ended at `consumed` messages,
	/// with one advance proof per `(from, to)` gap in the chain: exactly
	/// what the node-side assembler (issue 11) regenerates from public
	/// data. Caught up and gap-free, this is a bare tree proof.
	pub fn lift(&self, id: &StreamId, consumed: u64, gaps: &[(u64, u64)]) -> RequiresLift {
		let stream = self.stream(id);
		RequiresLift {
			advances: gaps.iter().map(|(from, to)| stream.extension_proof(*from, *to)).collect(),
			extension: stream.extension_proof(consumed, stream.leaf_count()),
			tree_proof: self.tree_proof(id),
		}
	}
}

/// Builds a consumption record from `(source, stream, interval)` items,
/// grouped by source and per source sorted by [`StreamId`] — the exact
/// shape `consumption_record()` yields.
pub fn record(items: impl IntoIterator<Item = (ParaId, StreamId, Interval)>) -> ConsumptionRecord {
	let mut record = ConsumptionRecord::default();
	for (source, stream, interval) in items {
		record.entries.entry(source).or_default().push((stream, interval));
	}
	for streams in record.entries.values_mut() {
		streams.sort_by(|a, b| a.0.cmp(&b.0));
	}
	record
}
