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

use super::basic::*;
use crate::LOG_TARGET;

use std::collections::{BTreeSet, HashMap, HashSet, VecDeque};

use polkadot_primitives::{CandidateHash, Hash, Id as ParaId};

/// Keeps a per leaf state of the claim queue for multiple forks.
#[derive(Default)]
pub struct PerLeafClaimQueueState {
	/// The state of the claim queue per leaf
	leaves: HashMap<Hash, ClaimQueueState>,
}

impl PerLeafClaimQueueState {
	/// Creates an empty `PerLeafClaimQueueState`
	pub fn new() -> Self {
		Self { leaves: HashMap::new() }
	}

	/// Adds new leaf to the state. If the parent of the leaf is already in the state `leaf` is
	/// added to the corresponding path. Otherwise a new path is created.
	pub fn add_leaf(
		&mut self,
		leaf: &Hash,
		claim_queue: &VecDeque<ParaId>,
		maybe_parent: Option<&Hash>,
	) {
		if let Some(parent) = maybe_parent {
			debug_assert!(leaf != parent, "Leaf and parent can't be equal");

			let maybe_path = self.leaves.remove(parent);

			// The new leaf builds on top of previous leaf
			if let Some(mut path) = maybe_path {
				path.add_leaf(leaf, claim_queue);
				self.leaves.insert(*leaf, path);
				gum::trace!(
					target: LOG_TARGET,
					?leaf,
					?parent,
					?claim_queue,
					"add_leaf: adding to existing path"
				);
				return
			}

			// The new leaf could be a fork from a previous non-leaf block
			for state in self.leaves.values() {
				if let Some(mut new_fork) = state.fork(parent) {
					new_fork.add_leaf(leaf, claim_queue);
					self.leaves.insert(*leaf, new_fork);
					gum::trace!(
						target: LOG_TARGET,
						?leaf,
						?parent,
						?claim_queue,
						"add_leaf: adding fork from a previous non-leaf block"
					);
					return
				}
			}
		}

		// The new leaf is a completely separate fork
		let mut new_fork = ClaimQueueState::new();
		new_fork.add_leaf(leaf, claim_queue);
		self.leaves.insert(*leaf, new_fork);
		gum::trace!(
			target: LOG_TARGET,
			?leaf,
			?maybe_parent,
			?claim_queue,
			"add_leaf: adding new fork"
		);
	}

	/// Removes a set of pruned blocks from all paths. If a path becomes empty it is removed from
	/// the state.
	pub fn remove_pruned_ancestors(&mut self, removed: &HashSet<Hash>) {
		// Remove the pruned blocks from the paths
		for (_, path) in &mut self.leaves {
			path.remove_pruned_ancestors(removed);
		}

		// Remove all empty paths
		self.leaves.retain(|_, p| !p.is_empty());
	}

	/// Releases a claim for a candidate.
	pub fn release_claims_for_candidate(&mut self, candidate_hash: &CandidateHash) -> bool {
		let mut result = false;
		for (_, state) in &mut self.leaves {
			if state.release_claim(candidate_hash) {
				result = true;
			}
		}
		result
	}

	/// Explicitly clears a claim at a specific relay parent for all leaves.
	pub fn release_claims_for_relay_parent(&mut self, relay_parent: &Hash) -> bool {
		let mut result = false;
		for (_, state) in &mut self.leaves {
			if state.release_claim_for_relay_parent(relay_parent) {
				result = true;
			}
		}
		result
	}

	/// Claims the first available slot for `para_id` at `relay_parent` as pending for all leaves.
	/// Returns `true` if the claim was successful.
	pub fn claim_pending_slot(
		&mut self,
		candidate_hash: Option<CandidateHash>,
		relay_parent: &Hash,
		para_id: &ParaId,
	) -> bool {
		let mut result = false;
		for (leaf, state) in &mut self.leaves {
			let claimed = if candidate_hash.is_none() {
				// special treatment -  we can't claim a future slot for v1 candidates
				state.claim_pending_at_v1(relay_parent, para_id)
			} else {
				state.claim_pending_at(relay_parent, para_id, candidate_hash)
			};

			if claimed {
				result = true;
			}

			gum::trace!(
				target: LOG_TARGET,
				?leaf,
				?para_id,
				?relay_parent,
				maybe_candidate_hash = ?candidate_hash,
				result,
				"claim_pending_slot"
			);
		}
		result
	}

	/// Sets the candidate hash for a pending claim at all leaves. If no such claim is found -
	/// returns false. Note that the candidate is set at first available `Pending(None)` claim at
	/// each leaf. Tracking the exact candidate order is not required here.
	pub fn mark_pending_slot_with_candidate(
		&mut self,
		candidate_hash: &CandidateHash,
		relay_parent: &Hash,
		para_id: &ParaId,
	) -> bool {
		let mut result = false;
		for (leaf, state) in &mut self.leaves {
			if state.mark_pending_slot_with_candidate(relay_parent, para_id, *candidate_hash) {
				result = true;
			}
			gum::trace!(
				target: LOG_TARGET,
				?leaf,
				?para_id,
				?relay_parent,
				?candidate_hash,
				result,
				"claim_pending_slot"
			);
		}
		result
	}

	/// Seconds a slot for a candidate at each leaf. Returns true if the claim was successful for at
	/// least one leaf. If a pending slot exists for the candidate it is upgraded to seconded.
	/// Otherwise a new claim is made.
	pub fn claim_seconded_slot(
		&mut self,
		candidate_hash: &CandidateHash,
		relay_parent: &Hash,
		para_id: &ParaId,
	) -> bool {
		let mut result = false;
		for (leaf, state) in &mut self.leaves {
			if state.claim_seconded_at(relay_parent, para_id, *candidate_hash) {
				result = true;
			}
			gum::trace!(
				target: LOG_TARGET,
				?leaf,
				?para_id,
				?relay_parent,
				?candidate_hash,
				result,
				"claim_seconded_slot"
			);
		}
		result
	}

	/// Returns the number of claims for a specific para id at a specific relay parent for all
	/// leaves.
	pub fn get_all_slots_for_para_at(&mut self, relay_parent: &Hash, para_id: &ParaId) -> usize {
		self.leaves
			.values()
			.map(|s| s.count_all_for_para_at(relay_parent, para_id))
			.max()
			.unwrap_or_default()
	}

	/// Returns the claim queue entries for all known and future blocks for all leaves.
	pub fn all_assignments(&self) -> BTreeSet<ParaId> {
		self.leaves
			.values()
			.flat_map(|claim_queue_state| claim_queue_state.all_assignments())
			.copied()
			.collect()
	}

	/// Returns the hashes of all tracked leaves.
	pub fn leaves(&self) -> impl Iterator<Item = &Hash> {
		self.leaves.keys()
	}

	/// Returns the corresponding para ids for all unclaimed slots in the claim queue for the
	/// specified leaf.
	pub fn free_slots(&self, leaf: &Hash) -> Vec<ParaId> {
		self.leaves.get(leaf).map(|state| state.free_slots()).unwrap_or_default()
	}

	/// Returns the corresponding para ids for all unclaimed slots in the claim queue for all
	/// leaves.
	pub fn all_free_slots(&self) -> BTreeSet<ParaId> {
		self.leaves
			.values()
			.flat_map(|claim_queue_state| claim_queue_state.free_slots())
			.collect()
	}

	/// Returns `true` if there is a free claim within `relay_parent`'s view of the claim queue for
	/// `leaf` or if there already is a claimed slot for the candidate.
	#[cfg(test)]
	fn has_free_slot_at_leaf_for(
		&mut self,
		leaf: &Hash,
		relay_parent: &Hash,
		para_id: &ParaId,
		candidate_hash: &CandidateHash,
	) -> bool {
		self.leaves.get_mut(leaf).map_or(false, |p: &mut ClaimQueueState| {
			p.has_or_can_claim_at(relay_parent, para_id, Some(*candidate_hash))
		})
	}
}

#[cfg(test)]
mod test {
	use super::*;

	use std::vec;

	#[test]
	fn add_leaf_works() {
		let mut state = PerLeafClaimQueueState::new();
		let para_id = ParaId::new(1);
		let claim_queue = VecDeque::from(vec![para_id, para_id, para_id]);
		let relay_parent_a = Hash::from_low_u64_be(1);
		let relay_parent_b = Hash::from_low_u64_be(2);
		let relay_parent_c = Hash::from_low_u64_be(3);
		let relay_parent_d = Hash::from_low_u64_be(4);

		//       / -> d
		// 0 -> a -> b
		//  \-> c
		state.add_leaf(&relay_parent_a, &claim_queue, Some(&Hash::from_low_u64_be(0)));
		assert_eq!(state.leaves.len(), 1);
		assert_eq!(state.leaves[&relay_parent_a].block_state.len(), 1);

		state.add_leaf(&relay_parent_b, &claim_queue, Some(&relay_parent_a));
		assert_eq!(state.leaves.len(), 1);
		assert_eq!(state.leaves[&relay_parent_b].block_state.len(), 2);

		state.add_leaf(&relay_parent_c, &claim_queue, Some(&Hash::from_low_u64_be(0)));
		assert_eq!(state.leaves.len(), 2);
		assert_eq!(state.leaves[&relay_parent_b].block_state.len(), 2);
		assert_eq!(state.leaves[&relay_parent_c].block_state.len(), 1);

		state.add_leaf(&relay_parent_d, &claim_queue, Some(&relay_parent_a));
		assert_eq!(state.leaves.len(), 3);
		assert_eq!(state.leaves[&relay_parent_b].block_state.len(), 2);
		assert_eq!(state.leaves[&relay_parent_c].block_state.len(), 1);
		assert_eq!(state.leaves[&relay_parent_d].block_state.len(), 2);
	}

	#[test]
	fn claim_pending_slot_works() {
		let mut state = PerLeafClaimQueueState::new();
		let para_id = ParaId::new(1);
		let claim_queue = VecDeque::from(vec![para_id, para_id]);
		let relay_parent_a = Hash::from_low_u64_be(1);
		let relay_parent_b = Hash::from_low_u64_be(2);
		let relay_parent_c = Hash::from_low_u64_be(3);

		// 0 -> a -> b
		//       \-> c
		state.add_leaf(&relay_parent_a, &claim_queue, Some(&Hash::from_low_u64_be(0)));
		state.add_leaf(&relay_parent_b, &claim_queue, Some(&relay_parent_a));
		state.add_leaf(&relay_parent_c, &claim_queue, Some(&relay_parent_a));

		let candidate_a = CandidateHash(Hash::from_low_u64_be(101));
		let candidate_b = CandidateHash(Hash::from_low_u64_be(102));
		let candidate_c = CandidateHash(Hash::from_low_u64_be(103));

		assert!(state.claim_pending_slot(Some(candidate_a), &relay_parent_a, &para_id));
		assert!(state.claim_pending_slot(Some(candidate_b), &relay_parent_b, &para_id));
		assert!(!state.has_free_slot_at_leaf_for(
			&relay_parent_b,
			&relay_parent_a,
			&para_id,
			&candidate_c
		));
		assert!(state.has_free_slot_at_leaf_for(
			&relay_parent_c,
			&relay_parent_a,
			&para_id,
			&candidate_c
		));
	}

	#[test]
	fn seconding_works() {
		let mut state = PerLeafClaimQueueState::new();
		let para_id = ParaId::new(1);
		let claim_queue = VecDeque::from(vec![para_id]);
		let relay_parent_a = Hash::from_low_u64_be(1);
		let relay_parent_b = Hash::from_low_u64_be(2);
		let relay_parent_c = Hash::from_low_u64_be(3);

		// 0 -> a -> b
		//       \-> c
		state.add_leaf(&relay_parent_a, &claim_queue, Some(&Hash::from_low_u64_be(0)));
		state.add_leaf(&relay_parent_b, &claim_queue, Some(&relay_parent_a));
		state.add_leaf(&relay_parent_c, &claim_queue, Some(&relay_parent_a));

		let relay_parent_a = Hash::from_low_u64_be(1);
		let candidate_a = CandidateHash(Hash::from_low_u64_be(101));

		assert!(state.claim_pending_slot(Some(candidate_a), &relay_parent_a, &para_id));

		// CQ is of size 1. We have claimed one slot at A, so there should be one free slot at
		// each leaf.
		assert_eq!(claim_queue.len(), 1);
		assert_eq!(state.free_slots(&relay_parent_b), vec![para_id]);
		assert_eq!(state.free_slots(&relay_parent_c), vec![para_id]);
		// and the same slots should remain available after seconding candidate_a
		assert!(state.claim_seconded_slot(&candidate_a, &relay_parent_a, &para_id));
		assert_eq!(state.free_slots(&relay_parent_b), vec![para_id]);
		assert_eq!(state.free_slots(&relay_parent_c), vec![para_id]);

		// Now claim a seconded slot directly at relay parent b
		let candidate_b = CandidateHash(Hash::from_low_u64_be(102));
		assert!(state.claim_seconded_slot(&candidate_b, &relay_parent_b, &para_id));
		// which means there are no more free slots at leaf b
		assert_eq!(state.free_slots(&relay_parent_b), vec![]);
		// but the free slot at leaf c stays
		assert_eq!(state.free_slots(&relay_parent_c), vec![para_id]);
	}

	#[test]
	fn remove_pruned_ancestors_works() {
		let mut state = PerLeafClaimQueueState::new();
		let para_id = ParaId::new(1);
		let claim_queue = VecDeque::from(vec![para_id, para_id, para_id]);
		let root_relay_parent = Hash::from_low_u64_be(0);
		let relay_parent_a = Hash::from_low_u64_be(1);
		let relay_parent_b = Hash::from_low_u64_be(2);
		let relay_parent_c = Hash::from_low_u64_be(3);

		// 0 -> a -> b
		//  \-> c
		state.add_leaf(&relay_parent_a, &claim_queue, Some(&root_relay_parent));
		state.add_leaf(&relay_parent_b, &claim_queue, Some(&relay_parent_a));
		state.add_leaf(&relay_parent_c, &claim_queue, Some(&root_relay_parent));

		let removed = vec![relay_parent_a, relay_parent_b];
		state.remove_pruned_ancestors(&HashSet::from_iter(removed.iter().cloned()));

		assert_eq!(state.leaves.len(), 1);
		assert_eq!(state.leaves[&relay_parent_c].block_state.len(), 1);
	}

	#[test]
	fn different_claims_per_leaf() {
		let mut state = PerLeafClaimQueueState::new();
		let para_id = ParaId::new(1);
		let claim_queue = VecDeque::from(vec![para_id, para_id]);
		let relay_parent_a = Hash::from_low_u64_be(1);
		let relay_parent_b = Hash::from_low_u64_be(2);
		let relay_parent_c = Hash::from_low_u64_be(3);

		// 0 -> a -> b
		//       \-> c
		state.add_leaf(&relay_parent_a, &claim_queue, Some(&Hash::from_low_u64_be(0)));
		state.add_leaf(&relay_parent_b, &claim_queue, Some(&relay_parent_a));
		state.add_leaf(&relay_parent_c, &claim_queue, Some(&relay_parent_a));

		let relay_parent_a = Hash::from_low_u64_be(1);
		let candidate_a = CandidateHash(Hash::from_low_u64_be(101));
		let candidate_b = CandidateHash(Hash::from_low_u64_be(102));
		let candidate_c = CandidateHash(Hash::from_low_u64_be(103));

		// `relay_parent_a` is not a leaf (b and c are)
		assert!(!state.has_free_slot_at_leaf_for(
			&relay_parent_a,
			&relay_parent_a,
			&para_id,
			&candidate_a
		));
		assert!(state.has_free_slot_at_leaf_for(
			&relay_parent_b,
			&relay_parent_a,
			&para_id,
			&candidate_a
		));
		assert!(state.has_free_slot_at_leaf_for(
			&relay_parent_c,
			&relay_parent_a,
			&para_id,
			&candidate_b
		));

		// Claim a slot at the common ancestor (rpa) and rp b
		assert!(state.claim_seconded_slot(&candidate_a, &relay_parent_a, &para_id));
		assert!(state.claim_seconded_slot(&candidate_b, &relay_parent_b, &para_id));

		// now try adding another candidate at the common ancestor at both leaves. It should
		// fail for b and succeed for c
		assert!(!state.has_free_slot_at_leaf_for(
			&relay_parent_b,
			&relay_parent_a,
			&para_id,
			&candidate_c
		));
		assert!(state.has_free_slot_at_leaf_for(
			&relay_parent_c,
			&relay_parent_a,
			&para_id,
			&candidate_c
		));
	}

	#[test]
	fn claims_at_common_ancestor_occupy_all_forks() {
		let mut state = PerLeafClaimQueueState::new();
		let para_id = ParaId::new(1);
		let claim_queue = VecDeque::from(vec![para_id, para_id]);
		let relay_parent_a = Hash::from_low_u64_be(1);
		let relay_parent_b = Hash::from_low_u64_be(2);
		let relay_parent_c = Hash::from_low_u64_be(3);

		// 0 -> a -> b
		//       \-> c
		state.add_leaf(&relay_parent_a, &claim_queue, Some(&Hash::from_low_u64_be(0)));
		state.add_leaf(&relay_parent_b, &claim_queue, Some(&relay_parent_a));
		state.add_leaf(&relay_parent_c, &claim_queue, Some(&relay_parent_a));

		let candidate_a = CandidateHash(Hash::from_low_u64_be(101));
		let candidate_b = CandidateHash(Hash::from_low_u64_be(102));
		let candidate_c = CandidateHash(Hash::from_low_u64_be(103));

		// Claim a slot at the common ancestor (rpa) for two candidates
		assert!(state.claim_seconded_slot(&candidate_a, &relay_parent_a, &para_id));
		assert!(state.claim_seconded_slot(&candidate_b, &relay_parent_a, &para_id));

		// now try adding another candidate at the common ancestor at both leaves. It should
		// fail for both
		assert!(!state.has_free_slot_at_leaf_for(
			&relay_parent_b,
			&relay_parent_a,
			&para_id,
			&candidate_c
		));
		assert!(!state.has_free_slot_at_leaf_for(
			&relay_parent_c,
			&relay_parent_a,
			&para_id,
			&candidate_c
		));

		// add one more leaf from a:
		// 0 -> a -> b
		//       \-> c
		//        \-> d
		// the claim should be transferred there too
		let relay_parent_d = Hash::from_low_u64_be(4);
		state.add_leaf(&relay_parent_d, &claim_queue, Some(&relay_parent_a));
		assert!(!state.has_free_slot_at_leaf_for(
			&relay_parent_d,
			&relay_parent_a,
			&para_id,
			&candidate_c
		));
	}
}
