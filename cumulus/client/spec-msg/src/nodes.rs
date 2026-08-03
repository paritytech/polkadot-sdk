// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Cumulus.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

// Cumulus is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Cumulus is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Cumulus. If not, see <https://www.gnu.org/licenses/>.

//! Historic MMR state over the archive's retained material.
//!
//! Serving a request under an old root needs the stream's MMR *as of that
//! root* — peaks at arbitrary leaf counts, extension proofs between block
//! boundaries, single-leaf inclusion proofs. The archive stores only leaf
//! hashes (from the horizon floor up) plus the frontier at the floor;
//! [`HistoricNodes`] recomputes every other node on demand: a node is
//! either a floor peak verbatim, a retained leaf hash, or the domain-tagged
//! merge of its two children — recursively. Nodes whose subtree dips below
//! the floor (other than the floor peaks themselves, which tile everything
//! below it) are unavailable, and the operation fails cleanly: exactly the
//! "below the resumable horizon" refusal the protocol specifies.

use std::{cell::RefCell, collections::HashMap};

use cumulus_primitives_spec_messaging::{
	MMRExtensionProof, MmrFrontier, MmrInclusionProof, MmrRoot, SpecHasher, SpecMerge,
};
use mmr_lib::{
	helper::{get_peaks, leaf_index_to_mmr_size, leaf_index_to_pos, pos_height_in_tree},
	MMRStoreReadOps,
};
use polkadot_core_primitives::Hash;

type Merge = SpecMerge<SpecHasher>;

/// Read access to one stream's retained leaf hashes.
pub(crate) trait LeafHashes {
	/// The leaf hash at leaf `index`; `None` where pruned or out of range.
	fn leaf_hash(&self, index: u64) -> Option<Hash>;
}

impl LeafHashes for Vec<Hash> {
	fn leaf_hash(&self, index: u64) -> Option<Hash> {
		usize::try_from(index).ok().and_then(|index| self.get(index).copied())
	}
}

/// On-demand MMR node computation for one stream, from the floor frontier
/// plus the retained leaf hashes above it. Per-request scoped: the memo
/// cache lives and dies with the value.
pub(crate) struct HistoricNodes<L> {
	leaves: L,
	/// Leaf count of the floor frontier: leaf hashes below are pruned.
	floor_leaf_count: u64,
	/// The floor frontier's peaks at their MMR positions — they tile
	/// everything below the floor.
	floor_peaks: HashMap<u64, Hash>,
	/// Memoized inner nodes, per instance.
	cache: RefCell<HashMap<u64, Hash>>,
}

impl<L: LeafHashes> HistoricNodes<L> {
	pub fn new(leaves: L, floor: &MmrFrontier) -> Self {
		let floor_peaks = if floor.leaf_count == 0 {
			HashMap::new()
		} else {
			get_peaks(leaf_index_to_mmr_size(floor.leaf_count - 1))
				.into_iter()
				.zip(floor.peaks.iter().copied())
				.collect()
		};
		Self {
			leaves,
			floor_leaf_count: floor.leaf_count,
			floor_peaks,
			cache: RefCell::new(HashMap::new()),
		}
	}

	/// The node at MMR position `pos`; `None` where the subtree dips below
	/// the floor.
	fn node(&self, pos: u64) -> Option<Hash> {
		if let Some(hash) = self.floor_peaks.get(&pos) {
			return Some(*hash);
		}
		if let Some(hash) = self.cache.borrow().get(&pos) {
			return Some(*hash);
		}
		let height = pos_height_in_tree(pos);
		let hash = if height == 0 {
			let index = pos_to_leaf_index(pos)?;
			if index < self.floor_leaf_count {
				return None;
			}
			self.leaves.leaf_hash(index)?
		} else {
			let left = self.node(pos - (1u64 << height))?;
			let right = self.node(pos - 1)?;
			<Merge as mmr_lib::Merge>::merge(&left, &right)
				.expect("SpecMerge::merge is infallible; qed")
		};
		self.cache.borrow_mut().insert(pos, hash);
		Some(hash)
	}

	/// The frontier after the first `leaf_count` messages.
	pub fn frontier_at(&self, leaf_count: u64) -> Option<MmrFrontier> {
		if leaf_count == 0 {
			return Some(MmrFrontier::default());
		}
		let peaks: Vec<Hash> = get_peaks(leaf_index_to_mmr_size(leaf_count - 1))
			.into_iter()
			.map(|pos| self.node(pos))
			.collect::<Option<_>>()?;
		let peaks = peaks.try_into().expect("a u64 leaf count has at most 64 peaks; qed");
		Some(MmrFrontier { leaf_count, peaks })
	}

	/// The stream root after the first `leaf_count` messages.
	pub fn root_at(&self, leaf_count: u64) -> Option<MmrRoot> {
		self.frontier_at(leaf_count).map(|frontier| frontier.root())
	}

	/// Extension proof from the state after `from` messages to the state
	/// after `to` — the canonical empty proof when equal.
	pub fn extension(&self, from: u64, to: u64) -> Option<MMRExtensionProof> {
		if from == to {
			return Some(MMRExtensionProof::empty());
		}
		if from > to {
			return None;
		}
		let new_mmr_size = leaf_index_to_mmr_size(to - 1);
		if from == 0 {
			// From empty, the connecting nodes are exactly the new peaks.
			let connecting_nodes = get_peaks(new_mmr_size)
				.into_iter()
				.map(|pos| Some((pos, self.node(pos)?)))
				.collect::<Option<_>>()?;
			return Some(MMRExtensionProof { leaf_count: to, connecting_nodes });
		}
		let mmr = mmr_lib::MMR::<Hash, Merge, &Self>::new(new_mmr_size, self);
		let ancestry = mmr.gen_ancestry_proof(leaf_index_to_mmr_size(from - 1)).ok()?;
		Some(MMRExtensionProof::from_ancestry_proof(to, &ancestry))
	}

	/// Inclusion proof for the leaf at `position` in the MMR over the first
	/// `leaf_count` messages.
	pub fn inclusion(&self, leaf_count: u64, position: u64) -> Option<MmrInclusionProof> {
		if position >= leaf_count {
			return None;
		}
		let mmr_size = leaf_index_to_mmr_size(leaf_count - 1);
		let mmr = mmr_lib::MMR::<Hash, Merge, &Self>::new(mmr_size, self);
		let proof = mmr.gen_proof(vec![leaf_index_to_pos(position)]).ok()?;
		Some(MmrInclusionProof { mmr_size, items: proof.proof_items().to_vec() })
	}
}

impl<L: LeafHashes> MMRStoreReadOps<Hash> for &HistoricNodes<L> {
	fn get_elem(&self, pos: u64) -> mmr_lib::Result<Option<Hash>> {
		Ok(self.node(pos))
	}
}

/// Inverts [`leaf_index_to_pos`]; `None` if `pos` is not a leaf position.
///
/// `leaf_index_to_pos` is strictly increasing with `pos >= index`, so a
/// binary search over `0..=pos` inverts it exactly. Positions anywhere near
/// the arithmetic overflow of the size formulas cannot come from real
/// streams and are rejected up front.
fn pos_to_leaf_index(pos: u64) -> Option<u64> {
	if pos >= 1 << 62 {
		return None;
	}
	let (mut lo, mut hi) = (0u64, pos);
	while lo < hi {
		let mid = lo + (hi - lo) / 2;
		if leaf_index_to_pos(mid) < pos {
			lo = mid + 1;
		} else {
			hi = mid;
		}
	}
	(leaf_index_to_pos(lo) == pos).then_some(lo)
}

#[cfg(test)]
mod tests {
	use super::*;
	use cumulus_primitives_spec_messaging::{
		hash_leaf, test_utils::StreamFixture, StreamId, LEAF_VERSION,
	};

	fn stream() -> StreamId {
		StreamId::Broadcast { domain: 0, subdomain: 0, num: 0 }
	}

	/// The fixture's leaves (same generator as `StreamFixture::new`).
	fn leaves(count: u64) -> Vec<Hash> {
		(0..count)
			.map(|i| hash_leaf::<SpecHasher>(LEAF_VERSION, &i.to_le_bytes()))
			.collect()
	}

	/// Leaves with everything below `floor` pruned (still index-addressed).
	fn pruned_leaves(count: u64, floor: u64) -> PrunedLeaves {
		PrunedLeaves { leaves: leaves(count), floor }
	}

	struct PrunedLeaves {
		leaves: Vec<Hash>,
		floor: u64,
	}

	impl LeafHashes for PrunedLeaves {
		fn leaf_hash(&self, index: u64) -> Option<Hash> {
			(index >= self.floor).then(|| self.leaves.leaf_hash(index)).flatten()
		}
	}

	#[test]
	fn pos_leaf_index_round_trips() {
		for index in (0..300).chain([1_000, 65_535, 1 << 40]) {
			assert_eq!(pos_to_leaf_index(leaf_index_to_pos(index)), Some(index));
		}
		// Inner-node positions are not leaf positions.
		assert_eq!(pos_to_leaf_index(2), None);
		assert_eq!(pos_to_leaf_index(6), None);
		assert_eq!(pos_to_leaf_index(1 << 63), None);
	}

	#[test]
	fn matches_fixture_at_floor_zero() {
		let fixture = StreamFixture::new(stream(), 130);
		let nodes = HistoricNodes::new(leaves(130), &MmrFrontier::default());

		for count in [0u64, 1, 2, 3, 64, 129, 130] {
			assert_eq!(nodes.frontier_at(count), Some(fixture.frontier_at(count)), "at {count}");
		}
		for (from, to) in [(0u64, 130u64), (0, 1), (10, 25), (25, 25), (129, 130)] {
			assert_eq!(
				nodes.extension(from, to),
				Some(fixture.extension_proof(from, to)),
				"extension {from} -> {to}"
			);
		}
		assert_eq!(nodes.extension(5, 3), None);
	}

	#[test]
	fn matches_fixture_above_a_floor() {
		let fixture = StreamFixture::new(stream(), 100);
		// Floor at 13 (three peaks): everything below pruned.
		let floor = fixture.frontier_at(13);
		let nodes = HistoricNodes::new(pruned_leaves(100, 13), &floor);

		for count in [13u64, 14, 20, 64, 100] {
			assert_eq!(nodes.frontier_at(count), Some(fixture.frontier_at(count)), "at {count}");
		}
		for (from, to) in [(13u64, 100u64), (13, 14), (20, 64), (64, 100)] {
			assert_eq!(
				nodes.extension(from, to),
				Some(fixture.extension_proof(from, to)),
				"extension {from} -> {to}"
			);
		}

		// Counts below the floor whose peaks happen to be floor peaks (12 =
		// 8 + 4; both are peaks of 13 = 8 + 4 + 1) remain serveable...
		assert_eq!(nodes.frontier_at(12), Some(fixture.frontier_at(12)));
		// ...anything actually reaching below the floor's tiling (11 needs
		// the subtree over leaves 8..10, strictly inside the floor peak
		// over 8..12) is unavailable — not wrong.
		assert_eq!(nodes.frontier_at(11), None);
		assert_eq!(nodes.extension(11, 100), None);
		assert_eq!(nodes.inclusion(100, 5), None);
	}

	#[test]
	fn inclusion_proofs_verify_to_the_stream_root() {
		let fixture = StreamFixture::new(stream(), 45);
		let floor = fixture.frontier_at(8);
		let nodes = HistoricNodes::new(pruned_leaves(45, 8), &floor);
		let all = leaves(45);

		for count in [45u64, 33, 9] {
			for position in [8u64, count / 2, count - 1] {
				if position < 8 || position >= count {
					continue;
				}
				let proof = nodes.inclusion(count, position).expect("above the floor");
				assert_eq!(
					proof
						.verify_leaf(
							cumulus_primitives_spec_messaging::MessagePosition(position),
							all[position as usize],
						)
						.expect("valid proof"),
					fixture.root_at(count),
					"position {position} of {count}"
				);
			}
		}

		// The head read matches the appended frontier bit for bit.
		let proof = nodes.inclusion(45, 44).expect("head is above the floor");
		let (position, frontier) = proof.verify_head(all[44]).expect("valid head proof");
		assert_eq!(position, cumulus_primitives_spec_messaging::MessagePosition(44));
		assert_eq!(frontier, fixture.frontier_at(45));
	}
}
