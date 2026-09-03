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

use std::collections::{HashMap, HashSet};

use polkadot_node_subsystem_util::inclusion_emulator::{ConstraintModifications, Fragment};
use polkadot_primitives::{CandidateHash, Hash, HeadData, Id as ParaId};

/// A node in the backed candidate chain.
/// Represents a single candidate with its execution context and cumulative constraint
/// modifications relative to the chain's base constraints.
#[cfg_attr(test, derive(Clone, Debug, PartialEq))]
pub(super) struct FragmentNode {
	pub(super) fragment: Fragment,
	pub(super) candidate_hash: CandidateHash,
	pub(super) cumulative_modifications: ConstraintModifications,
	pub(super) parent_head_data_hash: Hash,
	pub(super) output_head_data_hash: Hash,
	pub(super) scheduling_parent: Hash,
	pub(super) para_id: ParaId,
}

impl FragmentNode {
	/// Execution context: the relay parent determines PVD, constraints, and message state.
	pub(super) fn relay_parent(&self) -> Hash {
		self.fragment.relay_parent().hash
	}
}

/// A candidate chain of backed/backable candidates.
/// Includes the candidates pending availability and candidates which may be backed on-chain.
///
/// Maintains a linear, ordered sequence of `FragmentNode`s (parent -> child) along with
/// synchronized indexes for O(1) lookups by parent head, output head, and candidate hash.
///
/// Invariants:
/// - The `chain` vec is always ordered parent -> child.
/// - All indexes (`by_parent_head`, `by_output_head`, `candidates`) are always in sync with the
///   vec.
/// - New candidates are only appended at the tail; removal is only from the tail (suffix removal).
#[derive(Default)]
#[cfg_attr(test, derive(Clone))]
pub(super) struct BackedChain {
	// Holds the candidate chain.
	chain: Vec<FragmentNode>,
	// Index from head data hash to the candidate hash with that head data as a parent.
	// Only contains the candidates present in the `chain`.
	by_parent_head: HashMap<Hash, CandidateHash>,
	// Index from head data hash to the candidate hash outputting that head data.
	// Only contains the candidates present in the `chain`.
	by_output_head: HashMap<Hash, CandidateHash>,
	// A set of the candidate hashes in the `chain`.
	candidates: HashSet<CandidateHash>,
}

impl BackedChain {
	/// Append a candidate to the end of the chain, updating all indexes.
	/// Returns an error if the candidate doesn't continue the chain (i.e. its parent head
	/// doesn't match the previous candidate's output head).
	pub(super) fn push(&mut self, candidate: FragmentNode) -> Result<(), FragmentNode> {
		if let Some(last) = self.chain.last() {
			if candidate.parent_head_data_hash != last.output_head_data_hash {
				return Err(candidate);
			}
		}

		self.candidates.insert(candidate.candidate_hash);
		self.by_parent_head
			.insert(candidate.parent_head_data_hash, candidate.candidate_hash);
		self.by_output_head
			.insert(candidate.output_head_data_hash, candidate.candidate_hash);
		self.chain.push(candidate);
		Ok(())
	}

	/// Remove all candidates from the chain, returning them.
	pub(super) fn clear(&mut self) -> Vec<FragmentNode> {
		self.by_parent_head.clear();
		self.by_output_head.clear();
		self.candidates.clear();

		std::mem::take(&mut self.chain)
	}

	/// Remove all candidates after the one whose output head matches the given hash.
	/// Returns `None` if no candidate with that output head exists.
	pub(super) fn revert_to_output_head(
		&mut self,
		output_head_data_hash: &Hash,
	) -> Option<Vec<FragmentNode>> {
		// O(1) negative-case bailout via the output-head index, before the linear scan.
		if !self.by_output_head.contains_key(output_head_data_hash) {
			return None;
		}

		let found_index = self
			.chain
			.iter()
			.position(|node| &node.output_head_data_hash == output_head_data_hash)?;

		let removed: Vec<_> = self.chain.drain(found_index.saturating_add(1)..).collect();
		for node in &removed {
			self.by_parent_head.remove(&node.parent_head_data_hash);
			self.by_output_head.remove(&node.output_head_data_hash);
			self.candidates.remove(&node.candidate_hash);
		}

		Some(removed)
	}

	/// Whether the given candidate hash is part of this chain.
	pub(super) fn contains(&self, hash: &CandidateHash) -> bool {
		self.candidates.contains(hash)
	}

	/// Number of candidates in the chain.
	pub(super) fn len(&self) -> usize {
		self.chain.len()
	}

	/// Whether the chain has no candidates.
	pub(super) fn is_empty(&self) -> bool {
		self.chain.is_empty()
	}

	/// The last (most recent) node in the chain, if any.
	pub(super) fn last(&self) -> Option<&FragmentNode> {
		self.chain.last()
	}

	/// Iterate over nodes in chain order (parent to child).
	pub(super) fn iter(
		&self,
	) -> impl DoubleEndedIterator<Item = &FragmentNode> + ExactSizeIterator {
		self.chain.iter()
	}

	/// A slice of the chain by positional range.
	pub(super) fn slice(&self, range: std::ops::Range<usize>) -> &[FragmentNode] {
		&self.chain[range]
	}

	/// Ordered candidate hashes in chain order.
	pub(super) fn candidate_hashes(&self) -> Vec<CandidateHash> {
		self.chain.iter().map(|c| c.candidate_hash).collect()
	}

	/// The candidate hash whose parent head data matches the given hash, if any.
	pub(super) fn candidate_by_parent_head(
		&self,
		parent_head_hash: &Hash,
	) -> Option<CandidateHash> {
		self.by_parent_head.get(parent_head_hash).copied()
	}

	/// The candidate hash whose output head data matches the given hash, if any.
	pub(super) fn candidate_by_output_head(
		&self,
		output_head_hash: &Hash,
	) -> Option<CandidateHash> {
		self.by_output_head.get(output_head_hash).copied()
	}

	/// Find the node with the given candidate hash.
	pub(super) fn node_by_candidate_hash(
		&self,
		candidate_hash: &CandidateHash,
	) -> Option<&FragmentNode> {
		self.chain.iter().find(|c| &c.candidate_hash == candidate_hash)
	}

	/// Find the node whose output head data matches the given hash.
	pub(super) fn node_by_output_head(&self, output_head_hash: &Hash) -> Option<&FragmentNode> {
		let candidate_hash = self.by_output_head.get(output_head_hash)?;
		self.node_by_candidate_hash(candidate_hash)
	}

	/// Search for head data by hash. Checks both parent heads and output heads.
	/// Returns the full `HeadData` if found.
	pub(super) fn find_head_data(&self, head_data_hash: &Hash) -> Option<HeadData> {
		// Check if any node has this as its parent head or output head.
		if self
			.by_parent_head
			.get(head_data_hash)
			.or_else(|| self.by_output_head.get(head_data_hash))
			.is_none()
		{
			return None;
		}

		self.chain.iter().find_map(|candidate| {
			if &candidate.parent_head_data_hash == head_data_hash {
				Some(candidate.fragment.candidate().persisted_validation_data.parent_head.clone())
			} else if &candidate.output_head_data_hash == head_data_hash {
				Some(candidate.fragment.candidate().commitments.head_data.clone())
			} else {
				None
			}
		})
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use polkadot_node_subsystem_util::inclusion_emulator::{
		Constraints, Fragment, InboundHrmpLimitations, RelayChainBlockInfo as RelayParentInfo,
	};
	use polkadot_primitives::{
		CandidateCommitments, CandidateDescriptorV2, HeadData, PersistedValidationData,
	};
	use polkadot_primitives_test_helpers as test_helpers;
	use polkadot_primitives_test_helpers::CandidateDescriptor;
	use std::sync::Arc;

	fn make_constraints(min_relay_parent_number: u32, required_parent: HeadData) -> Constraints {
		Constraints {
			min_relay_parent_number,
			max_pov_size: 1_000_000,
			max_code_size: 1_000_000,
			max_head_data_size: 20480,
			ump_remaining: 10,
			ump_remaining_bytes: 1_000,
			max_ump_num_per_candidate: 10,
			dmp_remaining_messages: [0; 10].into(),
			hrmp_inbound: InboundHrmpLimitations { valid_watermarks: vec![0] },
			hrmp_channels_out: HashMap::new(),
			max_hrmp_num_per_candidate: 0,
			required_parent,
			validation_code_hash: Hash::repeat_byte(42).into(),
			upgrade_restriction: None,
			future_validation_code: None,
		}
	}

	fn make_relay_parent(number: u32, byte: u8) -> RelayParentInfo {
		RelayParentInfo { number, hash: Hash::repeat_byte(byte), storage_root: Hash::zero() }
	}

	/// Build a `FragmentNode` with the given parent/output head data and relay parent.
	/// The candidate is constructed to satisfy the given constraints.
	fn make_node(
		parent_head: HeadData,
		output_head: HeadData,
		relay_parent: &RelayParentInfo,
		constraints: &Constraints,
	) -> FragmentNode {
		let para_id = ParaId::from(5u32);
		let persisted_validation_data = PersistedValidationData {
			parent_head: parent_head.clone(),
			relay_parent_number: relay_parent.number,
			relay_parent_storage_root: Hash::zero(),
			max_pov_size: 1_000_000,
		};

		let descriptor: CandidateDescriptorV2<Hash> = CandidateDescriptor {
			para_id,
			relay_parent: relay_parent.hash,
			collator: test_helpers::dummy_collator(),
			persisted_validation_data_hash: persisted_validation_data.hash(),
			pov_hash: Hash::repeat_byte(1),
			erasure_root: Hash::repeat_byte(1),
			signature: test_helpers::zero_collator_signature(),
			para_head: output_head.hash(),
			validation_code_hash: Hash::repeat_byte(42).into(),
		}
		.into();

		let candidate = polkadot_primitives::CommittedCandidateReceiptV2 {
			descriptor,
			commitments: CandidateCommitments {
				upward_messages: Default::default(),
				horizontal_messages: Default::default(),
				new_validation_code: None,
				head_data: output_head.clone(),
				processed_downward_messages: 1,
				hrmp_watermark: relay_parent.number,
			},
		};

		let candidate_hash = candidate.hash();

		let prospective =
			Arc::new(polkadot_node_subsystem_util::inclusion_emulator::ProspectiveCandidate {
				commitments: candidate.commitments.clone(),
				persisted_validation_data: persisted_validation_data.clone(),
				pov_hash: Hash::repeat_byte(1),
				validation_code_hash: Hash::repeat_byte(42).into(),
			});

		let fragment = Fragment::new(relay_parent.clone(), constraints.clone(), prospective)
			.expect("fragment should be valid");

		let parent_head_data_hash = parent_head.hash();
		let output_head_data_hash = output_head.hash();

		FragmentNode {
			fragment,
			candidate_hash,
			cumulative_modifications: ConstraintModifications::identity(),
			parent_head_data_hash,
			output_head_data_hash,
			scheduling_parent: relay_parent.hash,
			para_id,
		}
	}

	/// Build a chain of 3 nodes: A -> B -> C
	fn make_chain_abc() -> (BackedChain, FragmentNode, FragmentNode, FragmentNode) {
		let relay_parent = make_relay_parent(0, 1);
		let constraints = make_constraints(0, vec![0x0a].into());

		let node_a = make_node(vec![0x0a].into(), vec![0x0b].into(), &relay_parent, &constraints);
		let node_b = make_node(
			vec![0x0b].into(),
			vec![0x0c].into(),
			&relay_parent,
			&make_constraints(0, vec![0x0b].into()),
		);
		let node_c = make_node(
			vec![0x0c].into(),
			vec![0x0d].into(),
			&relay_parent,
			&make_constraints(0, vec![0x0c].into()),
		);

		let mut chain = BackedChain::default();
		chain.push(node_a.clone()).unwrap();
		chain.push(node_b.clone()).unwrap();
		chain.push(node_c.clone()).unwrap();

		(chain, node_a, node_b, node_c)
	}

	#[test]
	fn push_enforces_chain_invariant() {
		let relay_parent = make_relay_parent(0, 1);
		let constraints = make_constraints(0, vec![0x0a].into());

		let mut chain = BackedChain::default();

		// First push to empty chain always succeeds.
		let node_a = make_node(vec![0x0a].into(), vec![0x0b].into(), &relay_parent, &constraints);
		let hash_a = node_a.candidate_hash;
		assert!(chain.push(node_a).is_ok());
		assert_eq!(chain.len(), 1);

		// Push with matching parent head succeeds.
		let node_b = make_node(
			vec![0x0b].into(),
			vec![0x0c].into(),
			&relay_parent,
			&make_constraints(0, vec![0x0b].into()),
		);
		let hash_b = node_b.candidate_hash;
		assert!(chain.push(node_b).is_ok());
		assert_eq!(chain.len(), 2);

		// Push with non-matching parent head fails.
		let bad_node = make_node(
			vec![0xff].into(),
			vec![0xee].into(),
			&relay_parent,
			&make_constraints(0, vec![0xff].into()),
		);
		let bad_hash = bad_node.candidate_hash;
		let result = chain.push(bad_node);
		assert!(result.is_err());
		let rejected = result.unwrap_err();
		assert_eq!(rejected.candidate_hash, bad_hash);

		// Chain is unchanged after failed push.
		assert_eq!(chain.len(), 2);
		assert!(chain.contains(&hash_a));
		assert!(chain.contains(&hash_b));
		assert!(!chain.contains(&bad_hash));
	}

	#[test]
	fn push_updates_all_indexes() {
		let relay_parent = make_relay_parent(0, 1);
		let constraints = make_constraints(0, vec![0x0a].into());

		let mut chain = BackedChain::default();
		let node = make_node(vec![0x0a].into(), vec![0x0b].into(), &relay_parent, &constraints);
		let hash = node.candidate_hash;
		let parent_head_hash = node.parent_head_data_hash;
		let output_head_hash = node.output_head_data_hash;

		chain.push(node).unwrap();

		assert!(chain.contains(&hash));
		assert_eq!(chain.candidate_by_parent_head(&parent_head_hash), Some(hash));
		assert_eq!(chain.candidate_by_output_head(&output_head_hash), Some(hash));
		assert_eq!(chain.node_by_candidate_hash(&hash).unwrap().candidate_hash, hash);
		assert_eq!(chain.node_by_output_head(&output_head_hash).unwrap().candidate_hash, hash);
	}

	#[test]
	fn clear_resets_everything() {
		let (mut chain, node_a, node_b, node_c) = make_chain_abc();
		assert_eq!(chain.len(), 3);

		let removed = chain.clear();
		assert_eq!(removed.len(), 3);
		assert_eq!(removed[0].candidate_hash, node_a.candidate_hash);
		assert_eq!(removed[1].candidate_hash, node_b.candidate_hash);
		assert_eq!(removed[2].candidate_hash, node_c.candidate_hash);

		// Chain is now empty.
		assert!(chain.is_empty());
		assert_eq!(chain.len(), 0);
		assert!(chain.last().is_none());
		assert!(!chain.contains(&node_a.candidate_hash));
		assert!(chain.candidate_by_parent_head(&node_a.parent_head_data_hash).is_none());
		assert!(chain.candidate_by_output_head(&node_a.output_head_data_hash).is_none());
		assert!(chain.candidate_hashes().is_empty());
	}

	#[test]
	fn revert_to_last_node_removes_nothing() {
		let (mut chain, _node_a, _node_b, node_c) = make_chain_abc();

		let removed = chain.revert_to_output_head(&node_c.output_head_data_hash);
		assert_eq!(removed, Some(vec![]));
		assert_eq!(chain.len(), 3);
	}

	#[test]
	fn revert_to_middle_node() {
		let (mut chain, node_a, node_b, node_c) = make_chain_abc();

		let removed = chain.revert_to_output_head(&node_a.output_head_data_hash).unwrap();
		assert_eq!(removed.len(), 2);
		assert_eq!(removed[0].candidate_hash, node_b.candidate_hash);
		assert_eq!(removed[1].candidate_hash, node_c.candidate_hash);

		// Chain has only A.
		assert_eq!(chain.len(), 1);
		assert!(chain.contains(&node_a.candidate_hash));
		assert!(!chain.contains(&node_b.candidate_hash));
		assert!(!chain.contains(&node_c.candidate_hash));

		// Indexes for removed nodes are gone.
		assert!(chain.candidate_by_parent_head(&node_b.parent_head_data_hash).is_none());
		assert!(chain.candidate_by_output_head(&node_b.output_head_data_hash).is_none());
		assert!(chain.candidate_by_parent_head(&node_c.parent_head_data_hash).is_none());
		assert!(chain.candidate_by_output_head(&node_c.output_head_data_hash).is_none());

		// Indexes for remaining node are still there.
		assert!(chain.candidate_by_parent_head(&node_a.parent_head_data_hash).is_some());
		assert!(chain.candidate_by_output_head(&node_a.output_head_data_hash).is_some());
	}

	#[test]
	fn revert_to_unknown_hash_returns_none() {
		let (mut chain, _node_a, _node_b, _node_c) = make_chain_abc();

		let unknown = Hash::repeat_byte(0xff);
		assert!(chain.revert_to_output_head(&unknown).is_none());
		// Chain is unchanged.
		assert_eq!(chain.len(), 3);
	}

	#[test]
	fn revert_to_first_nodes_parent_head_returns_none() {
		// The first node's parent head hash is not any node's output head hash,
		// so revert_to_output_head should return None and leave the chain unchanged.
		let (mut chain, node_a, _node_b, _node_c) = make_chain_abc();

		let first_parent_head = node_a.parent_head_data_hash;
		// Confirm this hash is not an output head of any node.
		assert!(chain.candidate_by_output_head(&first_parent_head).is_none());

		assert!(chain.revert_to_output_head(&first_parent_head).is_none());
		assert_eq!(chain.len(), 3);
	}

	#[test]
	fn iteration_and_ordering() {
		let (chain, node_a, node_b, node_c) = make_chain_abc();

		// iter() returns in order.
		let hashes: Vec<_> = chain.iter().map(|n| n.candidate_hash).collect();
		assert_eq!(
			hashes,
			vec![node_a.candidate_hash, node_b.candidate_hash, node_c.candidate_hash]
		);

		// candidate_hashes() matches.
		assert_eq!(chain.candidate_hashes(), hashes);

		// slice works.
		let middle = chain.slice(1..2);
		assert_eq!(middle.len(), 1);
		assert_eq!(middle[0].candidate_hash, node_b.candidate_hash);

		// last() is C.
		assert_eq!(chain.last().unwrap().candidate_hash, node_c.candidate_hash);

		// Reverse iteration.
		let rev_hashes: Vec<_> = chain.iter().rev().map(|n| n.candidate_hash).collect();
		assert_eq!(
			rev_hashes,
			vec![node_c.candidate_hash, node_b.candidate_hash, node_a.candidate_hash]
		);
	}

	#[test]
	fn find_head_data_by_parent_and_output() {
		let (chain, _node_a, _node_b, _node_c) = make_chain_abc();

		let head_a: HeadData = vec![0x0a].into();
		let head_b: HeadData = vec![0x0b].into();
		let head_d: HeadData = vec![0x0d].into();

		// Find by parent head hash — returns the parent head data.
		let found = chain.find_head_data(&head_a.hash()).unwrap();
		assert_eq!(found, head_a);

		// Find by output head hash — returns the output head data.
		// 0x0d is only an output head (of node C), not a parent head of any node.
		let found = chain.find_head_data(&head_d.hash()).unwrap();
		assert_eq!(found, head_d);

		// 0x0b is both an output (of A) and a parent (of B). The method checks parent heads
		// first, so we should get head_b back either way.
		let found = chain.find_head_data(&head_b.hash()).unwrap();
		assert_eq!(found, head_b);

		// Unknown hash returns None.
		let unknown: HeadData = vec![0xff].into();
		assert!(chain.find_head_data(&unknown.hash()).is_none());
	}

	#[test]
	fn node_lookups() {
		let (chain, node_a, _node_b, node_c) = make_chain_abc();

		// By candidate hash — found.
		let found = chain.node_by_candidate_hash(&node_a.candidate_hash).unwrap();
		assert_eq!(found.candidate_hash, node_a.candidate_hash);

		// By candidate hash — not found.
		assert!(chain.node_by_candidate_hash(&CandidateHash(Hash::repeat_byte(0xff))).is_none());

		// By output head — found.
		let found = chain.node_by_output_head(&node_c.output_head_data_hash).unwrap();
		assert_eq!(found.candidate_hash, node_c.candidate_hash);

		// By output head — not found.
		assert!(chain.node_by_output_head(&Hash::repeat_byte(0xff)).is_none());
	}

	#[test]
	fn empty_chain_behavior() {
		let chain = BackedChain::default();

		assert!(chain.is_empty());
		assert_eq!(chain.len(), 0);
		assert!(chain.last().is_none());
		assert!(chain.candidate_hashes().is_empty());
		assert!(chain.iter().next().is_none());
		assert!(!chain.contains(&CandidateHash(Hash::repeat_byte(1))));
		assert!(chain.candidate_by_parent_head(&Hash::repeat_byte(1)).is_none());
		assert!(chain.candidate_by_output_head(&Hash::repeat_byte(1)).is_none());
		assert!(chain.node_by_candidate_hash(&CandidateHash(Hash::repeat_byte(1))).is_none());
		assert!(chain.node_by_output_head(&Hash::repeat_byte(1)).is_none());
		assert!(chain.find_head_data(&Hash::repeat_byte(1)).is_none());
	}
}
