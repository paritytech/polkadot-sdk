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

use polkadot_primitives::{CandidateHash, CoreIndex, Hash, Id as ParaId};

/// Keeps a per leaf state of the claim queue for multiple forks.
#[derive(Default)]
pub struct PerLeafClaimQueueState {
	/// The state of the claim queue per leaf, per core index.
	leaves: HashMap<Hash, HashMap<CoreIndex, ClaimQueueState>>,
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
		core_index: CoreIndex,
		claim_queue: &VecDeque<ParaId>,
		maybe_parent: Option<&Hash>,
	) {
		if let Some(parent) = maybe_parent {
			debug_assert!(leaf != parent, "Leaf and parent can't be equal");

			let maybe_path = self.leaves.remove(parent);
			// The new leaf builds on top of previous leaf
			if let Some(mut path) = maybe_path {
				path.entry(core_index)
					.or_insert_with(ClaimQueueState::new)
					.add_leaf(leaf, claim_queue);
				self.leaves.insert(*leaf, path);
				gum::trace!(
					target: LOG_TARGET,
					?leaf,
					?parent,
					?claim_queue,
					"add_leaf: adding to existing path"
				);
				return;
			}

			// The new leaf could be a fork from a previous non-leaf block
			let maybe_new_fork = self.leaves.values().find_map(|core_states| {
				let forked: HashMap<CoreIndex, ClaimQueueState> = core_states
					.iter()
					.filter_map(|(ci, cqs)| cqs.fork(parent).map(|f| (*ci, f)))
					.collect();
				if forked.is_empty() { None } else { Some(forked) }
			});

			if let Some(mut state) = maybe_new_fork {
				state
					.entry(core_index)
					.or_insert_with(ClaimQueueState::new)
					.add_leaf(leaf, claim_queue);
				self.leaves.insert(*leaf, state);
				gum::trace!(
					target: LOG_TARGET,
					?leaf,
					?parent,
					?claim_queue,
					"add_leaf: adding fork from a previous non-leaf block"
				);
				return;
			}
		}

		// The new leaf is a completely separate fork
		let mut new_fork = ClaimQueueState::new();
		new_fork.add_leaf(leaf, claim_queue);
		self.leaves.insert(*leaf, HashMap::from([(core_index, new_fork)]));
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
		self.leaves.retain(|_, core_states| {
			for state in core_states.values_mut() {
				state.remove_pruned_ancestors(removed);
			}
			core_states.retain(|_, state| !state.is_empty());
			!core_states.is_empty()
		});
	}

	/// Releases a claim for a candidate.
	pub fn release_claims_for_candidate(&mut self, candidate_hash: &CandidateHash) -> bool {
		let mut result = false;
		for (_, core_states) in &mut self.leaves {
			for state in core_states.values_mut() {
				if state.release_claim(candidate_hash) {
					result = true;
				}
			}
		}
		result
	}

	/// Explicitly clears a claim at a specific relay parent for all leaves.
	pub fn release_claims_for_relay_parent(&mut self, relay_parent: &Hash) -> bool {
		let mut result = false;
		for (_, core_states) in &mut self.leaves {
			for state in core_states.values_mut() {
				if state.release_claim_for_relay_parent(relay_parent) {
					result = true;
				}
			}
		}
		result
	}

	/// Claims the first available slot for `para_id` at `relay_parent` as pending for all leaves.
	/// Returns `true` if the claim was successful.
	pub fn claim_pending_slot(
		&mut self,
		relay_parent: &Hash,
		para_id: &ParaId,
		candidate_hash: Option<CandidateHash>,
		core_index: CoreIndex,
	) -> bool {
		let mut result = false;
		for (leaf, core_states) in &mut self.leaves {
			let Some(state) = core_states.get_mut(&core_index) else { continue };

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
	/// If `core_index` is `None`, the update is applied to all cores (fallback path).
	pub fn mark_pending_slot_with_candidate(
		&mut self,
		relay_parent: &Hash,
		para_id: &ParaId,
		candidate_hash: &CandidateHash,
		core_index: Option<CoreIndex>,
	) -> bool {
		let mut result = false;
		for (leaf, core_states) in &mut self.leaves {
			let states: Vec<&mut ClaimQueueState> = match core_index {
				Some(idx) => core_states.get_mut(&idx).into_iter().collect(),
				None => core_states.values_mut().collect(),
			};
			for state in states {
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
					"mark_pending_slot_with_candidate"
				);
			}
		}
		result
	}

	/// Seconds a slot for a candidate at each leaf. Returns true if the claim was successful for at
	/// least one leaf. If a pending slot exists for the candidate it is upgraded to seconded.
	/// Otherwise a new claim is made.
	/// If `core_index` is `None`, the claim is applied to all cores (fallback path).
	pub fn claim_seconded_slot(
		&mut self,
		relay_parent: &Hash,
		para_id: &ParaId,
		candidate_hash: &CandidateHash,
		core_index: Option<CoreIndex>,
	) -> bool {
		let mut result = false;
		for (leaf, core_states) in &mut self.leaves {
			let states: Vec<&mut ClaimQueueState> = match core_index {
				Some(idx) => core_states.get_mut(&idx).into_iter().collect(),
				None => core_states.values_mut().collect(),
			};
			for state in states {
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
		}
		result
	}

	/// Returns the number of claims for a specific para id at a specific relay parent for all
	/// leaves.
	pub fn count_all_slots_for_para_at(&mut self, relay_parent: &Hash, para_id: &ParaId) -> usize {
		self.leaves
			.values()
			.map(|core_states| {
				core_states
					.values()
					.map(|state| state.count_all_for_para_at(relay_parent, para_id))
					.max()
					.unwrap_or_default()
			})
			.max()
			.unwrap_or_default()
	}

	/// Returns the claim queue entries for all known and future blocks for all leaves.
	pub fn all_assignments(&self) -> BTreeSet<ParaId> {
		self.leaves
			.values()
			.flat_map(|core_states| {
				core_states.values().flat_map(|state| state.all_assignments()).copied()
			})
			.collect()
	}

	/// Returns the hashes of all tracked leaves.
	pub fn leaves(&self) -> impl Iterator<Item = &Hash> {
		self.leaves.keys()
	}

	/// Returns the corresponding para ids for all unclaimed slots in the claim queue for the
	/// specified leaf.
	pub fn free_slots(&self, leaf: &Hash) -> Vec<ParaId> {
		self.leaves
			.get(leaf)
			.map(|core_states| {
				core_states.values().flat_map(|state| state.free_slots()).collect()
			})
			.unwrap_or_default()
	}

	/// Returns the corresponding para ids for all unclaimed slots in the claim queue for all
	/// leaves.
	pub fn all_free_slots(&self) -> BTreeSet<ParaId> {
		self.leaves
			.values()
			.flat_map(|core_states| core_states.values().flat_map(|state| state.free_slots()))
			.collect()
	}
}

#[cfg(test)]
mod test {
	use super::*;
	use crate::validator_side::claim_queue_state::test::*;

	impl PerLeafClaimQueueState {
		/// Returns `true` if there is a free claim within `relay_parent`'s view of the claim queue
		/// for `leaf` or if there already is a claimed slot for the candidate.
		fn has_free_slot_at_leaf_for(
			&mut self,
			leaf: &Hash,
			relay_parent: &Hash,
			para_id: &ParaId,
			candidate_hash: &CandidateHash,
		) -> bool {
			let Some(core_states) = self.leaves.get_mut(leaf) else {
				return false;
			};
			core_states
				.values_mut()
				.any(|state| state.has_or_can_claim_at(relay_parent, para_id, Some(*candidate_hash)))
		}
	}

	#[test]
	fn add_leaf_works() {
		let mut state = PerLeafClaimQueueState::new();
		let claim_queue = VecDeque::from(vec![PARA_1, PARA_1, PARA_1]);

		//       / -> d
		// 0 -> a -> b
		//  \-> c
		state.add_leaf(&RELAY_PARENT_A, CORE_0, &claim_queue, Some(&ROOT_RELAY_PARENT));
		assert_eq!(state.leaves.len(), 1);
		assert_eq!(state.leaves[&RELAY_PARENT_A][&CORE_0].block_state.len(), 1);

		state.add_leaf(&RELAY_PARENT_B, CORE_0, &claim_queue, Some(&RELAY_PARENT_A));
		assert_eq!(state.leaves.len(), 1);
		assert_eq!(state.leaves[&RELAY_PARENT_B][&CORE_0].block_state.len(), 2);

		state.add_leaf(&RELAY_PARENT_C, CORE_0, &claim_queue, Some(&ROOT_RELAY_PARENT));
		assert_eq!(state.leaves.len(), 2);
		assert_eq!(state.leaves[&RELAY_PARENT_B][&CORE_0].block_state.len(), 2);
		assert_eq!(state.leaves[&RELAY_PARENT_C][&CORE_0].block_state.len(), 1);

		state.add_leaf(&RELAY_PARENT_D, CORE_0, &claim_queue, Some(&RELAY_PARENT_A));
		assert_eq!(state.leaves.len(), 3);
		assert_eq!(state.leaves[&RELAY_PARENT_B][&CORE_0].block_state.len(), 2);
		assert_eq!(state.leaves[&RELAY_PARENT_C][&CORE_0].block_state.len(), 1);
		assert_eq!(state.leaves[&RELAY_PARENT_D][&CORE_0].block_state.len(), 2);
	}

	#[test]
	fn claim_pending_slot_works() {
		let mut state = PerLeafClaimQueueState::new();
		let claim_queue = VecDeque::from(vec![PARA_1, PARA_1]);

		// 0 -> a -> b
		//       \-> c
		state.add_leaf(&RELAY_PARENT_A, CORE_0, &claim_queue, Some(&ROOT_RELAY_PARENT));
		state.add_leaf(&RELAY_PARENT_B, CORE_0, &claim_queue, Some(&RELAY_PARENT_A));
		state.add_leaf(&RELAY_PARENT_C, CORE_0, &claim_queue, Some(&RELAY_PARENT_A));

		assert!(state.claim_pending_slot(&RELAY_PARENT_A, &PARA_1, Some(*CANDIDATE_A1), CORE_0));
		assert!(state.claim_pending_slot(&RELAY_PARENT_B, &PARA_1, Some(*CANDIDATE_B1), CORE_0));
		assert!(!state.has_free_slot_at_leaf_for(
			&RELAY_PARENT_B,
			&RELAY_PARENT_A,
			&PARA_1,
			&CANDIDATE_C1
		));
		assert!(state.has_free_slot_at_leaf_for(
			&RELAY_PARENT_C,
			&RELAY_PARENT_A,
			&PARA_1,
			&CANDIDATE_C1
		));
	}

	#[test]
	fn seconding_works() {
		let mut state = PerLeafClaimQueueState::new();
		let claim_queue = VecDeque::from(vec![PARA_1]);

		// 0 -> a -> b
		//       \-> c
		state.add_leaf(&RELAY_PARENT_A, CORE_0, &claim_queue, Some(&ROOT_RELAY_PARENT));
		state.add_leaf(&RELAY_PARENT_B, CORE_0, &claim_queue, Some(&RELAY_PARENT_A));
		state.add_leaf(&RELAY_PARENT_C, CORE_0, &claim_queue, Some(&RELAY_PARENT_A));

		assert!(state.claim_pending_slot(&RELAY_PARENT_A, &PARA_1, Some(*CANDIDATE_A1), CORE_0));

		// CQ is of size 1. We have claimed one slot at A, so there should be one free slot at
		// each leaf.
		assert_eq!(claim_queue.len(), 1);
		assert_eq!(state.free_slots(&RELAY_PARENT_B), vec![PARA_1]);
		assert_eq!(state.free_slots(&RELAY_PARENT_C), vec![PARA_1]);
		// and the same slots should remain available after seconding CANDIDATE_A1
		assert!(state.claim_seconded_slot(&RELAY_PARENT_A, &PARA_1, &CANDIDATE_A1, Some(CORE_0)));
		assert_eq!(state.free_slots(&RELAY_PARENT_B), vec![PARA_1]);
		assert_eq!(state.free_slots(&RELAY_PARENT_C), vec![PARA_1]);

		// Now claim a seconded slot directly at relay parent b
		assert!(state.claim_seconded_slot(&RELAY_PARENT_B, &PARA_1, &CANDIDATE_B1, Some(CORE_0)));
		// which means there are no more free slots at leaf b
		assert_eq!(state.free_slots(&RELAY_PARENT_B), vec![]);
		// but the free slot at leaf c stays
		assert_eq!(state.free_slots(&RELAY_PARENT_C), vec![PARA_1]);
	}

	#[test]
	fn remove_pruned_ancestors_works() {
		let mut state = PerLeafClaimQueueState::new();
		let claim_queue = VecDeque::from(vec![PARA_1, PARA_1, PARA_1]);

		// 0 -> a -> b
		//  \-> c
		state.add_leaf(&RELAY_PARENT_A, CORE_0, &claim_queue, Some(&ROOT_RELAY_PARENT));
		state.add_leaf(&RELAY_PARENT_B, CORE_0, &claim_queue, Some(&RELAY_PARENT_A));
		state.add_leaf(&RELAY_PARENT_C, CORE_0, &claim_queue, Some(&ROOT_RELAY_PARENT));

		let removed = vec![*RELAY_PARENT_A, *RELAY_PARENT_B];
		state.remove_pruned_ancestors(&HashSet::from_iter(removed.iter().cloned()));

		assert_eq!(state.leaves.len(), 1);
		assert_eq!(state.leaves[&RELAY_PARENT_C][&CORE_0].block_state.len(), 1);
	}

	#[test]
	fn different_claims_per_leaf() {
		let mut state = PerLeafClaimQueueState::new();
		let claim_queue = VecDeque::from(vec![PARA_1, PARA_1]);

		// 0 -> a -> b
		//       \-> c
		state.add_leaf(&RELAY_PARENT_A, CORE_0, &claim_queue, Some(&ROOT_RELAY_PARENT));
		state.add_leaf(&RELAY_PARENT_B, CORE_0, &claim_queue, Some(&RELAY_PARENT_A));
		state.add_leaf(&RELAY_PARENT_C, CORE_0, &claim_queue, Some(&RELAY_PARENT_A));

		// `RELAY_PARENT_A` is not a leaf (b and c are)
		assert!(!state.has_free_slot_at_leaf_for(
			&RELAY_PARENT_A,
			&RELAY_PARENT_A,
			&PARA_1,
			&CANDIDATE_A1
		));
		assert!(state.has_free_slot_at_leaf_for(
			&RELAY_PARENT_B,
			&RELAY_PARENT_A,
			&PARA_1,
			&CANDIDATE_A1
		));
		assert!(state.has_free_slot_at_leaf_for(
			&RELAY_PARENT_C,
			&RELAY_PARENT_A,
			&PARA_1,
			&CANDIDATE_B1
		));

		// Claim a slot at the common ancestor (rp a) and rp b
		assert!(state.claim_seconded_slot(&RELAY_PARENT_A, &PARA_1, &CANDIDATE_A1, Some(CORE_0)));
		assert!(state.claim_seconded_slot(&RELAY_PARENT_B, &PARA_1, &CANDIDATE_B1, Some(CORE_0)));

		// now try adding another candidate at the common ancestor at both leaves. It should
		// fail for b and succeed for c
		assert!(!state.has_free_slot_at_leaf_for(
			&RELAY_PARENT_B,
			&RELAY_PARENT_A,
			&PARA_1,
			&CANDIDATE_C1
		));
		assert!(state.has_free_slot_at_leaf_for(
			&RELAY_PARENT_C,
			&RELAY_PARENT_A,
			&PARA_1,
			&CANDIDATE_C1
		));
	}

	#[test]
	fn claims_at_common_ancestor_occupy_all_forks() {
		let mut state = PerLeafClaimQueueState::new();
		let claim_queue = VecDeque::from(vec![PARA_1, PARA_1]);

		// 0 -> a -> b
		//       \-> c
		state.add_leaf(&RELAY_PARENT_A, CORE_0, &claim_queue, Some(&ROOT_RELAY_PARENT));
		state.add_leaf(&RELAY_PARENT_B, CORE_0, &claim_queue, Some(&RELAY_PARENT_A));
		state.add_leaf(&RELAY_PARENT_C, CORE_0, &claim_queue, Some(&RELAY_PARENT_A));

		// Claim a slot at the common ancestor (rp a) for two candidates
		assert!(state.claim_seconded_slot(&RELAY_PARENT_A, &PARA_1, &CANDIDATE_A1, Some(CORE_0)));
		assert!(state.claim_seconded_slot(&RELAY_PARENT_A, &PARA_1, &CANDIDATE_B1, Some(CORE_0)));

		// now try adding another candidate at the common ancestor at both leaves. It should
		// fail for both
		assert!(!state.has_free_slot_at_leaf_for(
			&RELAY_PARENT_B,
			&RELAY_PARENT_A,
			&PARA_1,
			&CANDIDATE_C1
		));
		assert!(!state.has_free_slot_at_leaf_for(
			&RELAY_PARENT_C,
			&RELAY_PARENT_A,
			&PARA_1,
			&CANDIDATE_C1
		));

		// add one more leaf from a:
		// 0 -> a -> b
		//       \-> c
		//        \-> d
		// the claim should be transferred there too
		state.add_leaf(&RELAY_PARENT_D, CORE_0, &claim_queue, Some(&RELAY_PARENT_A));
		assert!(!state.has_free_slot_at_leaf_for(
			&RELAY_PARENT_D,
			&RELAY_PARENT_A,
			&PARA_1,
			&CANDIDATE_C1
		));
	}

	mod multi_core {
		use super::*;
		use crate::validator_side::claim_queue_state::test::*;

		/// Tests that at a group rotation boundary, claims for the old core (Para 1) are preserved
		/// alongside new claims for the new core (Para 2).
		#[test]
		fn rotation_preserves_old_core_claims() {
			let mut state = PerLeafClaimQueueState::new();

			// 0 -> A(core 0, para 1) -> B(rotation: core 1, para 2)
			let cq_para1 = VecDeque::from(vec![PARA_1, PARA_1]);
			let cq_para2 = VecDeque::from(vec![PARA_2, PARA_2]);

			state.add_leaf(&RELAY_PARENT_A, CORE_0, &cq_para1, Some(&ROOT_RELAY_PARENT));
			state.add_leaf(&RELAY_PARENT_B, CORE_1, &cq_para2, Some(&RELAY_PARENT_A));

			// B should have free slots for both paras
			let free = state.free_slots(&RELAY_PARENT_B);
			assert!(free.contains(&PARA_1), "para 1 slot from old core should still be free");
			assert!(free.contains(&PARA_2), "para 2 slot from new core should be free");

			// Claims succeed on each core independently
			assert!(state.claim_pending_slot(&RELAY_PARENT_A, &PARA_1, Some(*CANDIDATE_A1), CORE_0));
			assert!(state.claim_pending_slot(&RELAY_PARENT_B, &PARA_2, Some(*CANDIDATE_B1), CORE_1));
		}

		/// Tests that when forking from a non-leaf ancestor after a rotation, all cores' states
		/// are carried into the fork — not just the state of the new leaf's core.
		#[test]
		fn fork_from_non_leaf_after_rotation_carries_all_cores() {
			let mut state = PerLeafClaimQueueState::new();

			// 0 -> A(core0) -> B(core1, rotation) -> C(core1)
			//                                     \-> D(core0, fork from B)
			let cq0 = VecDeque::from(vec![PARA_1, PARA_1]);
			let cq1 = VecDeque::from(vec![PARA_2, PARA_2]);

			state.add_leaf(&RELAY_PARENT_A, CORE_0, &cq0, Some(&ROOT_RELAY_PARENT));
			state.add_leaf(&RELAY_PARENT_B, CORE_1, &cq1, Some(&RELAY_PARENT_A));
			state.add_leaf(&RELAY_PARENT_C, CORE_1, &cq1, Some(&RELAY_PARENT_B));
			state.add_leaf(&RELAY_PARENT_D, CORE_0, &cq0, Some(&RELAY_PARENT_B));

			assert_eq!(state.leaves.len(), 2);

			// D should carry core1's state (forked at B) alongside its own core0 state
			assert!(state.leaves[&RELAY_PARENT_D].contains_key(&CORE_0));
			assert!(state.leaves[&RELAY_PARENT_D].contains_key(&CORE_1));

			// Both paras should have free slots at D
			let free = state.free_slots(&RELAY_PARENT_D);
			assert!(free.contains(&PARA_1), "para 1 slot should be present in fork");
			assert!(free.contains(&PARA_2), "para 2 slot (from core1 forked at B) should be present");
		}

		/// Tests that claiming on one core does not consume slots on another core.
		#[test]
		fn claims_are_core_isolated() {
			let mut state = PerLeafClaimQueueState::new();

			// 0 -> A(core0) -> B(core1, rotation)
			let cq0 = VecDeque::from(vec![PARA_1, PARA_1]);
			let cq1 = VecDeque::from(vec![PARA_2, PARA_2]);

			state.add_leaf(&RELAY_PARENT_A, CORE_0, &cq0, Some(&ROOT_RELAY_PARENT));
			state.add_leaf(&RELAY_PARENT_B, CORE_1, &cq1, Some(&RELAY_PARENT_A));

			// Exhaust both slots on core0
			assert!(state.claim_pending_slot(&RELAY_PARENT_A, &PARA_1, Some(*CANDIDATE_A1), CORE_0));
			assert!(state.claim_pending_slot(&RELAY_PARENT_A, &PARA_1, Some(*CANDIDATE_A2), CORE_0));
			assert!(!state.claim_pending_slot(&RELAY_PARENT_A, &PARA_1, Some(*CANDIDATE_A3), CORE_0));

			// core1 slots for para2 must be unaffected
			assert!(state.claim_pending_slot(&RELAY_PARENT_B, &PARA_2, Some(*CANDIDATE_B1), CORE_1));
			assert!(state.claim_pending_slot(&RELAY_PARENT_B, &PARA_2, Some(*CANDIDATE_B2), CORE_1));

			// Claiming on the wrong core returns false
			assert!(!state.claim_pending_slot(&RELAY_PARENT_B, &PARA_2, Some(*CANDIDATE_C1), CORE_0));
		}

		/// Tests the full pending → seconded lifecycle across two cores at a rotation boundary,
		/// including a fork: both children of A must see claims made at A.
		#[test]
		fn rotation_full_seconding_lifecycle() {
			let mut state = PerLeafClaimQueueState::new();

			// 0 -> A(core0) -> B(core1, rotation)
			//              \-> C(core1, rotation, fork)
			let cq0 = VecDeque::from(vec![PARA_1, PARA_1]);
			let cq1 = VecDeque::from(vec![PARA_2, PARA_2]);

			state.add_leaf(&RELAY_PARENT_A, CORE_0, &cq0, Some(&ROOT_RELAY_PARENT));
			state.add_leaf(&RELAY_PARENT_B, CORE_1, &cq1, Some(&RELAY_PARENT_A));
			state.add_leaf(&RELAY_PARENT_C, CORE_1, &cq1, Some(&RELAY_PARENT_A));

			assert_eq!(state.leaves.len(), 2);

			// Claims at A on the old core are visible at both leaves
			let free_b = state.free_slots(&RELAY_PARENT_B);
			let free_c = state.free_slots(&RELAY_PARENT_C);
			assert!(free_b.contains(&PARA_1));
			assert!(free_c.contains(&PARA_1));

			// Old core: claim two pending slots at A, then second both
			assert!(state.claim_pending_slot(&RELAY_PARENT_A, &PARA_1, Some(*CANDIDATE_A1), CORE_0));
			assert!(state.claim_pending_slot(&RELAY_PARENT_A, &PARA_1, Some(*CANDIDATE_A2), CORE_0));
			assert!(state.claim_seconded_slot(&RELAY_PARENT_A, &PARA_1, &CANDIDATE_A1, Some(CORE_0)));
			assert!(state.claim_seconded_slot(&RELAY_PARENT_A, &PARA_1, &CANDIDATE_A2, Some(CORE_0)));

			// New core at B: claim two pending slots, then second both
			assert!(state.claim_pending_slot(&RELAY_PARENT_B, &PARA_2, Some(*CANDIDATE_B1), CORE_1));
			assert!(state.claim_pending_slot(&RELAY_PARENT_B, &PARA_2, Some(*CANDIDATE_B2), CORE_1));
			assert!(state.claim_seconded_slot(&RELAY_PARENT_B, &PARA_2, &CANDIDATE_B1, Some(CORE_1)));
			assert!(state.claim_seconded_slot(&RELAY_PARENT_B, &PARA_2, &CANDIDATE_B2, Some(CORE_1)));

			// New core at C: claim two pending slots, then second both
			assert!(state.claim_pending_slot(&RELAY_PARENT_C, &PARA_2, Some(*CANDIDATE_C1), CORE_1));
			assert!(state.claim_pending_slot(&RELAY_PARENT_C, &PARA_2, Some(*CANDIDATE_C2), CORE_1));
			assert!(state.claim_seconded_slot(&RELAY_PARENT_C, &PARA_2, &CANDIDATE_C1, Some(CORE_1)));
			assert!(state.claim_seconded_slot(&RELAY_PARENT_C, &PARA_2, &CANDIDATE_C2, Some(CORE_1)));

			// Both leaves fully exhausted
			assert_eq!(state.free_slots(&RELAY_PARENT_B), vec![]);
			assert_eq!(state.free_slots(&RELAY_PARENT_C), vec![]);
		}

		/// Tests that `None` core_index in `mark_pending_slot_with_candidate` and
		/// `claim_seconded_slot` searches across all cores to find the matching slot
		/// (v1 compatibility path where the core index is not known).
		#[test]
		fn none_core_index_considers_all_cores() {
			let mut state = PerLeafClaimQueueState::new();

			// 0 -> A(core0) -> B(core1, rotation)
			let cq0 = VecDeque::from(vec![PARA_1]);
			let cq1 = VecDeque::from(vec![PARA_2]);

			state.add_leaf(&RELAY_PARENT_A, CORE_0, &cq0, Some(&ROOT_RELAY_PARENT));
			state.add_leaf(&RELAY_PARENT_B, CORE_1, &cq1, Some(&RELAY_PARENT_A));

			// v1 claim: no candidate hash known yet
			assert!(state.claim_pending_slot(&RELAY_PARENT_A, &PARA_1, None, CORE_0));

			// v1 mark: core not known — None must find and mark core0's pending claim
			assert!(state.mark_pending_slot_with_candidate(
				&RELAY_PARENT_A,
				&PARA_1,
				&CANDIDATE_A1,
				None,
			));
			// The slot is now marked — a second mark attempt on the same slot must fail
			assert!(!state.mark_pending_slot_with_candidate(
				&RELAY_PARENT_A,
				&PARA_1,
				&CANDIDATE_A2,
				None,
			));

			// None also works for seconding — hits core1 at B
			assert!(state.claim_seconded_slot(&RELAY_PARENT_B, &PARA_2, &CANDIDATE_B1, None));
			let free = state.free_slots(&RELAY_PARENT_B);
			assert!(!free.contains(&PARA_2), "core1 slot should be consumed");

			// Releasing a claim that was made via None (v1) also works:
			// CANDIDATE_A1 was marked via None — release it and the slot should be free again
			assert!(state.release_claims_for_candidate(&CANDIDATE_A1));
			let free = state.free_slots(&RELAY_PARENT_B);
			assert!(free.contains(&PARA_1), "core0 slot should be free after release");

			// CANDIDATE_B1 was seconded via None — release it and the slot should be free again
			assert!(state.release_claims_for_candidate(&CANDIDATE_B1));
			let free = state.free_slots(&RELAY_PARENT_B);
			assert!(free.contains(&PARA_2), "core1 slot should be free after release");
		}

		/// Tests that `remove_pruned_ancestors` correctly handles leaves with multiple cores:
		/// per-core states are pruned individually and a leaf is only removed when all its
		/// cores become empty.
		#[test]
		fn remove_pruned_ancestors_multi_core() {
			let mut state = PerLeafClaimQueueState::new();

			// 0 -> A(core0) -> B(core1, rotation) -> C(core1)
			//  \-> D(core0)
			let cq0 = VecDeque::from(vec![PARA_1, PARA_1]);
			let cq1 = VecDeque::from(vec![PARA_2, PARA_2]);

			state.add_leaf(&RELAY_PARENT_A, CORE_0, &cq0, Some(&ROOT_RELAY_PARENT));
			state.add_leaf(&RELAY_PARENT_B, CORE_1, &cq1, Some(&RELAY_PARENT_A));
			state.add_leaf(&RELAY_PARENT_C, CORE_1, &cq1, Some(&RELAY_PARENT_B));
			state.add_leaf(&RELAY_PARENT_D, CORE_0, &cq0, Some(&ROOT_RELAY_PARENT));

			// Prune A and B from C's history
			let removed = HashSet::from([*RELAY_PARENT_A, *RELAY_PARENT_B]);
			state.remove_pruned_ancestors(&removed);

			// C and D survive
			assert_eq!(state.leaves.len(), 2);

			// CORE_0 only ever tracked A; removing A empties its block_state, so it is dropped.
			// CORE_1 tracked B and C; after removing B, C remains.
			assert!(!state.leaves[&RELAY_PARENT_C].contains_key(&CORE_0));
			assert!(state.leaves[&RELAY_PARENT_C].contains_key(&CORE_1));

			// D is on an independent fork (child of ROOT), unaffected
			assert!(state.leaves[&RELAY_PARENT_D].contains_key(&CORE_0));
		}

		/// After a rotation, `add_leaf` is no longer called for the old core. Its future slots
		/// (hash: None) are only reachable via the window-from-A — the last relay parent that was
		/// actually added to the old core's block_state. Once that relay parent is pruned the
		/// future slots become unreachable and the old core's state is correctly dropped.
		#[test]
		fn old_core_future_slots_only_reachable_via_pre_rotation_relay_parent() {
			let mut state = PerLeafClaimQueueState::new();

			// 0 -> A(core0, cq=[P1,P1,P1]) -> B(core1, rotation) -> C(core1)
			//
			// The three-entry CQ for CORE_0 represents slots at A, B, C. After the rotation at B
			// however, `add_leaf` is never called for CORE_0 again, so B and C are never resolved
			// to real hashes in CORE_0's future_blocks. All three slots are therefore only
			// accessible via the window starting at A.
			let cq0 = VecDeque::from(vec![PARA_1, PARA_1, PARA_1]);
			let cq1 = VecDeque::from(vec![PARA_2, PARA_2, PARA_2]);

			state.add_leaf(&RELAY_PARENT_A, CORE_0, &cq0, Some(&ROOT_RELAY_PARENT));
			state.add_leaf(&RELAY_PARENT_B, CORE_1, &cq1, Some(&RELAY_PARENT_A));
			state.add_leaf(&RELAY_PARENT_C, CORE_1, &cq1, Some(&RELAY_PARENT_B));

			// All three CORE_0 slots are claimable via scheduling_parent = A
			assert!(state.claim_pending_slot(&RELAY_PARENT_A, &PARA_1, Some(*CANDIDATE_A1), CORE_0));
			assert!(state.claim_pending_slot(&RELAY_PARENT_A, &PARA_1, Some(*CANDIDATE_A2), CORE_0));
			assert!(state.claim_pending_slot(&RELAY_PARENT_A, &PARA_1, Some(*CANDIDATE_A3), CORE_0));

			// Release them so we can test the scheduling_parent = B case with free slots
			assert!(state.release_claims_for_candidate(&CANDIDATE_A1));
			assert!(state.release_claims_for_candidate(&CANDIDATE_A2));
			assert!(state.release_claims_for_candidate(&CANDIDATE_A3));

			// B's hash was never resolved in CORE_0's future_blocks, so claiming via
			// scheduling_parent = B fails even though there are free slots
			assert!(!state.claim_pending_slot(&RELAY_PARENT_B, &PARA_1, Some(*CANDIDATE_A1), CORE_0));

			// Prune A: CORE_0's block_state becomes empty. The future slots (hash: None) are
			// unreachable without A's window, so CORE_0 is correctly considered empty and dropped.
			let removed = HashSet::from([*RELAY_PARENT_A]);
			state.remove_pruned_ancestors(&removed);
			assert!(!state.leaves[&RELAY_PARENT_C].contains_key(&CORE_0));
		}

		/// Tests that releasing a candidate's claim on one core does not affect another core.
		#[test]
		fn release_claims_are_core_isolated() {
			let mut state = PerLeafClaimQueueState::new();

			// 0 -> A(core0) -> B(core1, rotation)
			let cq0 = VecDeque::from(vec![PARA_1]);
			let cq1 = VecDeque::from(vec![PARA_2]);

			state.add_leaf(&RELAY_PARENT_A, CORE_0, &cq0, Some(&ROOT_RELAY_PARENT));
			state.add_leaf(&RELAY_PARENT_B, CORE_1, &cq1, Some(&RELAY_PARENT_A));

			// Claim on both cores
			assert!(state.claim_pending_slot(&RELAY_PARENT_A, &PARA_1, Some(*CANDIDATE_A1), CORE_0));
			assert!(state.claim_pending_slot(&RELAY_PARENT_B, &PARA_2, Some(*CANDIDATE_B1), CORE_1));

			// Release core0's candidate only
			assert!(state.release_claims_for_candidate(&CANDIDATE_A1));

			// core0 slot is free again, core1 slot is still claimed
			let free = state.free_slots(&RELAY_PARENT_B);
			assert!(free.contains(&PARA_1), "core0 slot should be free after release");
			assert!(!free.contains(&PARA_2), "core1 slot should still be claimed");
		}
	}
}
