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

//! `ClaimQueueState` tracks the state of the claim queue over a set of relay blocks. Refer to
//! [`ClaimQueueState`] for more details.

use std::collections::VecDeque;

use crate::LOG_TARGET;
use polkadot_primitives::{Hash, Id as ParaId};

/// Represents a single claim from the claim queue, mapped to the relay chain block where it could
/// be backed on-chain.
#[derive(Debug, PartialEq)]
struct ClaimInfo {
	// Hash of the relay chain block. Can be `None` if it is still not known (a future block).
	hash: Option<Hash>,
	/// Represents the `ParaId` scheduled for the block. Can be `None` if nothing is scheduled.
	claim: Option<ParaId>,
	/// The length of the claim queue at the block. It is used to determine the 'block window'
	/// where a claim can be made.
	claim_queue_len: usize,
	/// A flag that indicates if the slot is claimed or not.
	claimed: bool,
}

/// Tracks the state of the claim queue over a set of relay blocks.
///
/// Generally the claim queue represents the `ParaId` that should be scheduled at the current block
/// (the first element of the claim queue) and N other `ParaId`s which are supposed to be scheduled
/// on the next relay blocks. In other words the claim queue is a rolling window giving a hint what
/// should be built/fetched/accepted (depending on the context) at each block.
///
/// Since the claim queue peeks into the future blocks there is a relation between the claim queue
/// state between the current block and the future blocks.
/// Let's see an example with 2 co-scheduled parachains:
/// - relay parent 1; Claim queue: [A, B, A]
/// - relay parent 2; Claim queue: [B, A, B]
/// - relay parent 3; Claim queue: [A, B, A]
/// - and so on
///
/// Note that at rp1 the second element in the claim queue is equal to the first one in rp2. Also
/// the third element of the claim queue at rp1 is equal to the second one in rp2 and the first one
/// in rp3.
///
/// So if we want to claim the third slot at rp 1 we are also claiming the second at rp2 and first
/// at rp3. To track this in a simple way we can project the claim queue onto the relay blocks like
/// this:
///               [A]   [B]   [A] -> this is the claim queue at rp3
///         [B]   [A]   [B]       -> this is the claim queue at rp2
///   [A]   [B]   [A]	          -> this is the claim queue at rp1
/// [RP 1][RP 2][RP 3][RP X][RP Y] -> relay blocks, RP x and RP Y are future blocks
///
/// Note that the claims at each column are the same so we can simplify this by just projecting a
/// single claim over a block:
///   [A]   [B]   [A]   [B]   [A]  -> claims effectively are the same
/// [RP 1][RP 2][RP 3][RP X][RP Y] -> relay blocks, RP x and RP Y are future blocks
///
/// Basically this is how `ClaimQueueState` works. It keeps track of claims at each block by mapping
/// claims to relay blocks.
///
/// How making a claim works?
/// At each relay block we keep track how long is the claim queue. This is a 'window' where we can
/// make a claim. So adding a claim just looks for a free spot at this window and claims it.
///
/// Note on adding a new leaf.
/// When a new leaf is added we check if the first element in its claim queue matches with the
/// projection on the first element in 'future blocks'. If yes - the new relay block inherits this
/// claim. If not - this means that the claim queue changed for some reason so the claim can't be
/// inherited. This should not happen under normal circumstances. But if it happens it means that we
/// have got one claim which won't be satisfied in the worst case scenario.
pub(crate) struct ClaimQueueState {
	block_state: VecDeque<ClaimInfo>,
	future_blocks: VecDeque<ClaimInfo>,
}

impl ClaimQueueState {
	pub(crate) fn new() -> Self {
		Self { block_state: VecDeque::new(), future_blocks: VecDeque::new() }
	}

	// Appends a new leaf
	pub(crate) fn add_leaf(&mut self, hash: &Hash, claim_queue: &Vec<ParaId>) {
		if self.block_state.iter().any(|s| s.hash == Some(*hash)) {
			return
		}

		// First check if our view for the future blocks is consistent with the one in the claim
		// queue of the new block. If not - the claim queue has changed for some reason and we need
		// to readjust our view.
		for (idx, expected_claim) in claim_queue.iter().enumerate() {
			match self.future_blocks.get_mut(idx) {
				Some(future_block) =>
					if future_block.claim.as_ref() != Some(expected_claim) {
						// There is an inconsistency. Update our view with the one from the claim
						// queue. `claimed` can't be true anymore since the `ParaId` has changed.
						future_block.claimed = false;
						future_block.claim = Some(*expected_claim);
					},
				None => {
					self.future_blocks.push_back(ClaimInfo {
						hash: None,
						claim: Some(*expected_claim),
						// For future blocks we don't know the size of the claim queue.
						// `claim_queue_len` could be an option but there is not much benefit from
						// the extra boilerplate code to handle it. We set it to one since we
						// usually know about one claim at each future block but this value is not
						// used anywhere in the code.
						claim_queue_len: 1,
						claimed: false,
					});
				},
			}
		}

		// Now pop the first future block and add it as a leaf
		let claim_info = if let Some(new_leaf) = self.future_blocks.pop_front() {
			ClaimInfo {
				hash: Some(*hash),
				claim: claim_queue.first().copied(),
				claim_queue_len: claim_queue.len(),
				claimed: new_leaf.claimed,
			}
		} else {
			// maybe the claim queue was empty but we still need to add a leaf
			ClaimInfo {
				hash: Some(*hash),
				claim: claim_queue.first().copied(),
				claim_queue_len: claim_queue.len(),
				claimed: false,
			}
		};

		// `future_blocks` can't be longer than the length of the claim queue at the last block - 1.
		// For example this can happen if at relay block N we have got a claim queue of a length 4
		// and it's shrunk to 2.
		self.future_blocks.truncate(claim_queue.len().saturating_sub(1));

		self.block_state.push_back(claim_info);
	}

	fn get_window<'a>(
		&'a mut self,
		relay_parent: &'a Hash,
	) -> impl Iterator<Item = &'a mut ClaimInfo> + 'a {
		let mut window = self
			.block_state
			.iter_mut()
			.skip_while(|b| b.hash != Some(*relay_parent))
			.peekable();
		let cq_len = window.peek().map_or(0, |b| b.claim_queue_len);
		window.chain(self.future_blocks.iter_mut()).take(cq_len)
	}

	pub(crate) fn claim_at(&mut self, relay_parent: &Hash, para_id: &ParaId) -> bool {
		gum::trace!(
			target: LOG_TARGET,
			?para_id,
			?relay_parent,
			"claim_at"
		);
		self.find_a_claim(relay_parent, para_id, true)
	}

	pub(crate) fn can_claim_at(&mut self, relay_parent: &Hash, para_id: &ParaId) -> bool {
		gum::trace!(
			target: LOG_TARGET,
			?para_id,
			?relay_parent,
			"can_claim_at"
		);

		self.find_a_claim(relay_parent, para_id, false)
	}

	// Returns `true` if there is a claim within `relay_parent`'s view of the claim queue for
	// `para_id`. If `claim_it` is set to `true` the slot is claimed. Otherwise the function just
	// reports the availability of the slot.
	fn find_a_claim(&mut self, relay_parent: &Hash, para_id: &ParaId, claim_it: bool) -> bool {
		let window = self.get_window(relay_parent);

		for w in window {
			gum::trace!(
				target: LOG_TARGET,
				?para_id,
				?relay_parent,
				claim_info=?w,
				?claim_it,
				"Checking claim"
			);

			if !w.claimed && w.claim == Some(*para_id) {
				w.claimed = claim_it;
				return true
			}
		}

		false
	}

	pub(crate) fn unclaimed_at(&mut self, relay_parent: &Hash) -> Vec<ParaId> {
		let window = self.get_window(relay_parent);

		window.filter(|b| !b.claimed).filter_map(|b| b.claim).collect()
	}
}

#[cfg(test)]
mod test {
	use super::*;

	#[test]
	fn sane_initial_state() {
		let mut state = ClaimQueueState::new();
		let relay_parent = Hash::from_low_u64_be(1);
		let para_id = ParaId::new(1);

		assert!(!state.can_claim_at(&relay_parent, &para_id));
		assert!(!state.claim_at(&relay_parent, &para_id));
		assert_eq!(state.unclaimed_at(&relay_parent), vec![]);
	}

	#[test]
	fn add_leaf_works() {
		let mut state = ClaimQueueState::new();
		let relay_parent_a = Hash::from_low_u64_be(1);
		let para_id = ParaId::new(1);
		let claim_queue = vec![para_id, para_id, para_id];

		state.add_leaf(&relay_parent_a, &claim_queue);
		assert_eq!(state.unclaimed_at(&relay_parent_a), vec![para_id, para_id, para_id]);

		assert_eq!(
			state.block_state,
			VecDeque::from(vec![ClaimInfo {
				hash: Some(relay_parent_a),
				claim: Some(para_id),
				claim_queue_len: 3,
				claimed: false,
			},])
		);
		assert_eq!(
			state.future_blocks,
			VecDeque::from(vec![
				ClaimInfo { hash: None, claim: Some(para_id), claim_queue_len: 1, claimed: false },
				ClaimInfo { hash: None, claim: Some(para_id), claim_queue_len: 1, claimed: false }
			])
		);

		// should be no op
		state.add_leaf(&relay_parent_a, &claim_queue);
		assert_eq!(state.block_state.len(), 1);
		assert_eq!(state.future_blocks.len(), 2);

		// add another leaf
		let relay_parent_b = Hash::from_low_u64_be(2);
		state.add_leaf(&relay_parent_b, &claim_queue);

		assert_eq!(
			state.block_state,
			VecDeque::from(vec![
				ClaimInfo {
					hash: Some(relay_parent_a),
					claim: Some(para_id),
					claim_queue_len: 3,
					claimed: false,
				},
				ClaimInfo {
					hash: Some(relay_parent_b),
					claim: Some(para_id),
					claim_queue_len: 3,
					claimed: false,
				}
			])
		);
		assert_eq!(
			state.future_blocks,
			VecDeque::from(vec![
				ClaimInfo { hash: None, claim: Some(para_id), claim_queue_len: 1, claimed: false },
				ClaimInfo { hash: None, claim: Some(para_id), claim_queue_len: 1, claimed: false }
			])
		);

		assert_eq!(state.unclaimed_at(&relay_parent_a), vec![para_id, para_id, para_id]);
		assert_eq!(state.unclaimed_at(&relay_parent_b), vec![para_id, para_id, para_id]);
	}

	#[test]
	fn claims_at_separate_relay_parents_work() {
		let mut state = ClaimQueueState::new();
		let relay_parent_a = Hash::from_low_u64_be(1);
		let relay_parent_b = Hash::from_low_u64_be(2);
		let para_id = ParaId::new(1);
		let claim_queue = vec![para_id, para_id, para_id];

		state.add_leaf(&relay_parent_a, &claim_queue);
		state.add_leaf(&relay_parent_b, &claim_queue);

		// add one claim for a
		assert!(state.can_claim_at(&relay_parent_a, &para_id));
		assert_eq!(state.unclaimed_at(&relay_parent_a), vec![para_id, para_id, para_id]);
		assert!(state.claim_at(&relay_parent_a, &para_id));

		// and one for b
		assert!(state.can_claim_at(&relay_parent_b, &para_id));
		assert_eq!(state.unclaimed_at(&relay_parent_b), vec![para_id, para_id, para_id]);
		assert!(state.claim_at(&relay_parent_b, &para_id));

		// a should have one claim since the one for b was claimed
		assert_eq!(state.unclaimed_at(&relay_parent_a), vec![para_id]);
		// and two more for b
		assert_eq!(state.unclaimed_at(&relay_parent_b), vec![para_id, para_id]);

		assert_eq!(
			state.block_state,
			VecDeque::from(vec![
				ClaimInfo {
					hash: Some(relay_parent_a),
					claim: Some(para_id),
					claim_queue_len: 3,
					claimed: true,
				},
				ClaimInfo {
					hash: Some(relay_parent_b),
					claim: Some(para_id),
					claim_queue_len: 3,
					claimed: true,
				}
			])
		);
		assert_eq!(
			state.future_blocks,
			VecDeque::from(vec![
				ClaimInfo { hash: None, claim: Some(para_id), claim_queue_len: 1, claimed: false },
				ClaimInfo { hash: None, claim: Some(para_id), claim_queue_len: 1, claimed: false }
			])
		);
	}

	#[test]
	fn claims_are_transferred_to_next_slot() {
		let mut state = ClaimQueueState::new();
		let relay_parent_a = Hash::from_low_u64_be(1);
		let para_id = ParaId::new(1);
		let claim_queue = vec![para_id, para_id, para_id];

		state.add_leaf(&relay_parent_a, &claim_queue);

		// add two claims, 2nd should be transferred to a new leaf
		assert!(state.can_claim_at(&relay_parent_a, &para_id));
		assert_eq!(state.unclaimed_at(&relay_parent_a), vec![para_id, para_id, para_id]);
		assert!(state.claim_at(&relay_parent_a, &para_id));

		assert!(state.can_claim_at(&relay_parent_a, &para_id));
		assert_eq!(state.unclaimed_at(&relay_parent_a), vec![para_id, para_id]);
		assert!(state.claim_at(&relay_parent_a, &para_id));

		assert_eq!(
			state.block_state,
			VecDeque::from(vec![ClaimInfo {
				hash: Some(relay_parent_a),
				claim: Some(para_id),
				claim_queue_len: 3,
				claimed: true,
			},])
		);
		assert_eq!(
			state.future_blocks,
			VecDeque::from(vec![
				ClaimInfo { hash: None, claim: Some(para_id), claim_queue_len: 1, claimed: true },
				ClaimInfo { hash: None, claim: Some(para_id), claim_queue_len: 1, claimed: false }
			])
		);

		// one more
		assert!(state.can_claim_at(&relay_parent_a, &para_id));
		assert_eq!(state.unclaimed_at(&relay_parent_a), vec![para_id]);
		assert!(state.claim_at(&relay_parent_a, &para_id));

		assert_eq!(
			state.block_state,
			VecDeque::from(vec![ClaimInfo {
				hash: Some(relay_parent_a),
				claim: Some(para_id),
				claim_queue_len: 3,
				claimed: true,
			},])
		);
		assert_eq!(
			state.future_blocks,
			VecDeque::from(vec![
				ClaimInfo { hash: None, claim: Some(para_id), claim_queue_len: 1, claimed: true },
				ClaimInfo { hash: None, claim: Some(para_id), claim_queue_len: 1, claimed: true }
			])
		);

		// no more claims
		assert!(!state.can_claim_at(&relay_parent_a, &para_id));
		assert_eq!(state.unclaimed_at(&relay_parent_a), vec![]);
	}

	#[test]
	fn claims_are_transferred_to_new_leaves() {
		let mut state = ClaimQueueState::new();
		let relay_parent_a = Hash::from_low_u64_be(1);
		let para_id = ParaId::new(1);
		let claim_queue = vec![para_id, para_id, para_id];

		state.add_leaf(&relay_parent_a, &claim_queue);

		for _ in 0..3 {
			assert!(state.can_claim_at(&relay_parent_a, &para_id));
			assert!(state.claim_at(&relay_parent_a, &para_id));
		}

		assert_eq!(
			state.block_state,
			VecDeque::from(vec![ClaimInfo {
				hash: Some(relay_parent_a),
				claim: Some(para_id),
				claim_queue_len: 3,
				claimed: true,
			},])
		);
		assert_eq!(
			state.future_blocks,
			VecDeque::from(vec![
				ClaimInfo { hash: None, claim: Some(para_id), claim_queue_len: 1, claimed: true },
				ClaimInfo { hash: None, claim: Some(para_id), claim_queue_len: 1, claimed: true }
			])
		);

		// no more claims
		assert!(!state.can_claim_at(&relay_parent_a, &para_id));

		// new leaf
		let relay_parent_b = Hash::from_low_u64_be(2);
		state.add_leaf(&relay_parent_b, &claim_queue);

		assert_eq!(
			state.block_state,
			VecDeque::from(vec![
				ClaimInfo {
					hash: Some(relay_parent_a),
					claim: Some(para_id),
					claim_queue_len: 3,
					claimed: true,
				},
				ClaimInfo {
					hash: Some(relay_parent_b),
					claim: Some(para_id),
					claim_queue_len: 3,
					claimed: true,
				}
			])
		);
		assert_eq!(
			state.future_blocks,
			VecDeque::from(vec![
				ClaimInfo { hash: None, claim: Some(para_id), claim_queue_len: 1, claimed: true },
				ClaimInfo { hash: None, claim: Some(para_id), claim_queue_len: 1, claimed: false }
			])
		);

		// still no claims for a
		assert!(!state.can_claim_at(&relay_parent_a, &para_id));

		// but can accept for b
		assert!(state.can_claim_at(&relay_parent_b, &para_id));
		assert!(state.claim_at(&relay_parent_b, &para_id));

		assert_eq!(
			state.block_state,
			VecDeque::from(vec![
				ClaimInfo {
					hash: Some(relay_parent_a),
					claim: Some(para_id),
					claim_queue_len: 3,
					claimed: true,
				},
				ClaimInfo {
					hash: Some(relay_parent_b),
					claim: Some(para_id),
					claim_queue_len: 3,
					claimed: true,
				}
			])
		);
		assert_eq!(
			state.future_blocks,
			VecDeque::from(vec![
				ClaimInfo { hash: None, claim: Some(para_id), claim_queue_len: 1, claimed: true },
				ClaimInfo { hash: None, claim: Some(para_id), claim_queue_len: 1, claimed: true }
			])
		);
	}

	#[test]
	fn two_paras() {
		let mut state = ClaimQueueState::new();
		let relay_parent_a = Hash::from_low_u64_be(1);
		let para_id_a = ParaId::new(1);
		let para_id_b = ParaId::new(2);
		let claim_queue = vec![para_id_a, para_id_b, para_id_a];

		state.add_leaf(&relay_parent_a, &claim_queue);
		assert!(state.can_claim_at(&relay_parent_a, &para_id_a));
		assert!(state.can_claim_at(&relay_parent_a, &para_id_b));
		assert_eq!(state.unclaimed_at(&relay_parent_a), vec![para_id_a, para_id_b, para_id_a]);

		assert_eq!(
			state.block_state,
			VecDeque::from(vec![ClaimInfo {
				hash: Some(relay_parent_a),
				claim: Some(para_id_a),
				claim_queue_len: 3,
				claimed: false,
			},])
		);
		assert_eq!(
			state.future_blocks,
			VecDeque::from(vec![
				ClaimInfo {
					hash: None,
					claim: Some(para_id_b),
					claim_queue_len: 1,
					claimed: false
				},
				ClaimInfo {
					hash: None,
					claim: Some(para_id_a),
					claim_queue_len: 1,
					claimed: false
				}
			])
		);

		assert!(state.claim_at(&relay_parent_a, &para_id_a));
		assert!(state.can_claim_at(&relay_parent_a, &para_id_a));
		assert!(state.can_claim_at(&relay_parent_a, &para_id_b));
		assert_eq!(state.unclaimed_at(&relay_parent_a), vec![para_id_b, para_id_a]);

		assert_eq!(
			state.block_state,
			VecDeque::from(vec![ClaimInfo {
				hash: Some(relay_parent_a),
				claim: Some(para_id_a),
				claim_queue_len: 3,
				claimed: true,
			},])
		);
		assert_eq!(
			state.future_blocks,
			VecDeque::from(vec![
				ClaimInfo {
					hash: None,
					claim: Some(para_id_b),
					claim_queue_len: 1,
					claimed: false
				},
				ClaimInfo {
					hash: None,
					claim: Some(para_id_a),
					claim_queue_len: 1,
					claimed: false
				}
			])
		);

		assert!(state.claim_at(&relay_parent_a, &para_id_a));
		assert!(!state.can_claim_at(&relay_parent_a, &para_id_a));
		assert!(state.can_claim_at(&relay_parent_a, &para_id_b));
		assert_eq!(state.unclaimed_at(&relay_parent_a), vec![para_id_b]);

		assert_eq!(
			state.block_state,
			VecDeque::from(vec![ClaimInfo {
				hash: Some(relay_parent_a),
				claim: Some(para_id_a),
				claim_queue_len: 3,
				claimed: true,
			},])
		);
		assert_eq!(
			state.future_blocks,
			VecDeque::from(vec![
				ClaimInfo {
					hash: None,
					claim: Some(para_id_b),
					claim_queue_len: 1,
					claimed: false
				},
				ClaimInfo { hash: None, claim: Some(para_id_a), claim_queue_len: 1, claimed: true }
			])
		);

		assert!(state.claim_at(&relay_parent_a, &para_id_b));
		assert!(!state.can_claim_at(&relay_parent_a, &para_id_a));
		assert!(!state.can_claim_at(&relay_parent_a, &para_id_b));
		assert_eq!(state.unclaimed_at(&relay_parent_a), vec![]);

		assert_eq!(
			state.block_state,
			VecDeque::from(vec![ClaimInfo {
				hash: Some(relay_parent_a),
				claim: Some(para_id_a),
				claim_queue_len: 3,
				claimed: true,
			},])
		);
		assert_eq!(
			state.future_blocks,
			VecDeque::from(vec![
				ClaimInfo { hash: None, claim: Some(para_id_b), claim_queue_len: 1, claimed: true },
				ClaimInfo { hash: None, claim: Some(para_id_a), claim_queue_len: 1, claimed: true }
			])
		);
	}

	#[test]
	fn claim_queue_changes_unexpectedly() {
		let mut state = ClaimQueueState::new();
		let relay_parent_a = Hash::from_low_u64_be(1);
		let para_id_a = ParaId::new(1);
		let para_id_b = ParaId::new(2);
		let claim_queue_a = vec![para_id_a, para_id_b, para_id_a];

		state.add_leaf(&relay_parent_a, &claim_queue_a);
		assert!(state.can_claim_at(&relay_parent_a, &para_id_a));
		assert!(state.can_claim_at(&relay_parent_a, &para_id_b));
		assert!(state.claim_at(&relay_parent_a, &para_id_a));
		assert!(state.claim_at(&relay_parent_a, &para_id_a));
		assert!(state.claim_at(&relay_parent_a, &para_id_b));
		assert_eq!(state.unclaimed_at(&relay_parent_a), vec![]);

		assert_eq!(
			state.block_state,
			VecDeque::from(vec![ClaimInfo {
				hash: Some(relay_parent_a),
				claim: Some(para_id_a),
				claim_queue_len: 3,
				claimed: true,
			},])
		);
		assert_eq!(
			state.future_blocks,
			VecDeque::from(vec![
				ClaimInfo { hash: None, claim: Some(para_id_b), claim_queue_len: 1, claimed: true },
				ClaimInfo { hash: None, claim: Some(para_id_a), claim_queue_len: 1, claimed: true }
			])
		);

		let relay_parent_b = Hash::from_low_u64_be(2);
		let claim_queue_b = vec![para_id_a, para_id_a, para_id_a]; // should be [b, a, ...]
		state.add_leaf(&relay_parent_b, &claim_queue_b);

		// because of the unexpected change in claim queue we lost the claim for paraB and have one
		// unclaimed for paraA
		assert_eq!(state.unclaimed_at(&relay_parent_a), vec![para_id_a]);

		assert_eq!(
			state.block_state,
			VecDeque::from(vec![
				ClaimInfo {
					hash: Some(relay_parent_a),
					claim: Some(para_id_a),
					claim_queue_len: 3,
					claimed: true,
				},
				ClaimInfo {
					hash: Some(relay_parent_b),
					claim: Some(para_id_a),
					claim_queue_len: 3,
					claimed: false,
				}
			])
		);
		assert_eq!(
			state.future_blocks,
			// since the 3rd slot of the claim queue at rp1 is equal to the second one in rp2, this
			// claim still exists
			VecDeque::from(vec![
				ClaimInfo { hash: None, claim: Some(para_id_a), claim_queue_len: 1, claimed: true },
				ClaimInfo {
					hash: None,
					claim: Some(para_id_a),
					claim_queue_len: 1,
					claimed: false
				}
			])
		);
	}

	#[test]
	fn claim_queue_changes_unexpectedly_with_two_blocks() {
		let mut state = ClaimQueueState::new();
		let relay_parent_a = Hash::from_low_u64_be(1);
		let para_id_a = ParaId::new(1);
		let para_id_b = ParaId::new(2);
		let claim_queue_a = vec![para_id_a, para_id_b, para_id_b];

		state.add_leaf(&relay_parent_a, &claim_queue_a);
		assert!(state.can_claim_at(&relay_parent_a, &para_id_a));
		assert!(state.can_claim_at(&relay_parent_a, &para_id_b));
		assert!(state.claim_at(&relay_parent_a, &para_id_a));
		assert!(state.claim_at(&relay_parent_a, &para_id_b));
		assert!(state.claim_at(&relay_parent_a, &para_id_b));
		assert_eq!(state.unclaimed_at(&relay_parent_a), vec![]);

		assert_eq!(
			state.block_state,
			VecDeque::from(vec![ClaimInfo {
				hash: Some(relay_parent_a),
				claim: Some(para_id_a),
				claim_queue_len: 3,
				claimed: true,
			},])
		);
		assert_eq!(
			state.future_blocks,
			VecDeque::from(vec![
				ClaimInfo { hash: None, claim: Some(para_id_b), claim_queue_len: 1, claimed: true },
				ClaimInfo { hash: None, claim: Some(para_id_b), claim_queue_len: 1, claimed: true }
			])
		);

		let relay_parent_b = Hash::from_low_u64_be(2);
		let claim_queue_b = vec![para_id_a, para_id_a, para_id_a]; // should be [b, b, ...]
		state.add_leaf(&relay_parent_b, &claim_queue_b);

		// because of the unexpected change in claim queue we lost both claims for paraB and have
		// two unclaimed for paraA
		assert_eq!(state.unclaimed_at(&relay_parent_a), vec![para_id_a, para_id_a]);

		assert_eq!(
			state.block_state,
			VecDeque::from(vec![
				ClaimInfo {
					hash: Some(relay_parent_a),
					claim: Some(para_id_a),
					claim_queue_len: 3,
					claimed: true,
				},
				ClaimInfo {
					hash: Some(relay_parent_b),
					claim: Some(para_id_a),
					claim_queue_len: 3,
					claimed: false,
				}
			])
		);
		assert_eq!(
			state.future_blocks,
			VecDeque::from(vec![
				ClaimInfo {
					hash: None,
					claim: Some(para_id_a),
					claim_queue_len: 1,
					claimed: false
				},
				ClaimInfo {
					hash: None,
					claim: Some(para_id_a),
					claim_queue_len: 1,
					claimed: false
				}
			])
		);
	}

	#[test]
	fn empty_claim_queue() {
		let mut state = ClaimQueueState::new();
		let relay_parent_a = Hash::from_low_u64_be(1);
		let para_id_a = ParaId::new(1);
		let claim_queue_a = vec![];

		state.add_leaf(&relay_parent_a, &claim_queue_a);
		assert_eq!(state.unclaimed_at(&relay_parent_a), vec![]);

		assert_eq!(
			state.block_state,
			VecDeque::from(vec![ClaimInfo {
				hash: Some(relay_parent_a),
				claim: None,
				claim_queue_len: 0,
				claimed: false,
			},])
		);
		// no claim queue so we know nothing about future blocks
		assert!(state.future_blocks.is_empty());

		assert!(!state.can_claim_at(&relay_parent_a, &para_id_a));
		assert!(!state.claim_at(&relay_parent_a, &para_id_a));
		assert_eq!(state.unclaimed_at(&relay_parent_a), vec![]);

		let relay_parent_b = Hash::from_low_u64_be(2);
		let claim_queue_b = vec![para_id_a];
		state.add_leaf(&relay_parent_b, &claim_queue_b);

		assert_eq!(
			state.block_state,
			VecDeque::from(vec![
				ClaimInfo {
					hash: Some(relay_parent_a),
					claim: None,
					claim_queue_len: 0,
					claimed: false,
				},
				ClaimInfo {
					hash: Some(relay_parent_b),
					claim: Some(para_id_a),
					claim_queue_len: 1,
					claimed: false,
				},
			])
		);
		// claim queue with length 1 doesn't say anything about future blocks
		assert!(state.future_blocks.is_empty());

		assert!(!state.can_claim_at(&relay_parent_a, &para_id_a));
		assert!(!state.claim_at(&relay_parent_a, &para_id_a));
		assert_eq!(state.unclaimed_at(&relay_parent_a), vec![]);

		assert!(state.can_claim_at(&relay_parent_b, &para_id_a));
		assert_eq!(state.unclaimed_at(&relay_parent_b), vec![para_id_a]);
		assert!(state.claim_at(&relay_parent_b, &para_id_a));

		let relay_parent_c = Hash::from_low_u64_be(3);
		let claim_queue_c = vec![para_id_a, para_id_a];
		state.add_leaf(&relay_parent_c, &claim_queue_c);

		assert_eq!(
			state.block_state,
			VecDeque::from(vec![
				ClaimInfo {
					hash: Some(relay_parent_a),
					claim: None,
					claim_queue_len: 0,
					claimed: false,
				},
				ClaimInfo {
					hash: Some(relay_parent_b),
					claim: Some(para_id_a),
					claim_queue_len: 1,
					claimed: true,
				},
				ClaimInfo {
					hash: Some(relay_parent_c),
					claim: Some(para_id_a),
					claim_queue_len: 2,
					claimed: false,
				},
			])
		);
		// claim queue with length 2 fills only one future block
		assert_eq!(
			state.future_blocks,
			VecDeque::from(vec![ClaimInfo {
				hash: None,
				claim: Some(para_id_a),
				claim_queue_len: 1,
				claimed: false,
			},])
		);

		assert!(!state.can_claim_at(&relay_parent_a, &para_id_a));
		assert!(!state.claim_at(&relay_parent_a, &para_id_a));
		assert_eq!(state.unclaimed_at(&relay_parent_a), vec![]);

		// already claimed
		assert!(!state.can_claim_at(&relay_parent_b, &para_id_a));
		assert_eq!(state.unclaimed_at(&relay_parent_b), vec![]);
		assert!(!state.claim_at(&relay_parent_b, &para_id_a));

		assert!(state.can_claim_at(&relay_parent_c, &para_id_a));
		assert_eq!(state.unclaimed_at(&relay_parent_c), vec![para_id_a, para_id_a]);
	}

	#[test]
	fn claim_queue_becomes_shorter() {
		let mut state = ClaimQueueState::new();
		let relay_parent_a = Hash::from_low_u64_be(1);
		let para_id_a = ParaId::new(1);
		let para_id_b = ParaId::new(2);
		let claim_queue_a = vec![para_id_a, para_id_b, para_id_a];

		state.add_leaf(&relay_parent_a, &claim_queue_a);
		assert_eq!(state.unclaimed_at(&relay_parent_a), vec![para_id_a, para_id_b, para_id_a]);

		assert_eq!(
			state.block_state,
			VecDeque::from(vec![ClaimInfo {
				hash: Some(relay_parent_a),
				claim: Some(para_id_a),
				claim_queue_len: 3,
				claimed: false,
			},])
		);
		assert_eq!(
			state.future_blocks,
			VecDeque::from(vec![
				ClaimInfo {
					hash: None,
					claim: Some(para_id_b),
					claim_queue_len: 1,
					claimed: false
				},
				ClaimInfo {
					hash: None,
					claim: Some(para_id_a),
					claim_queue_len: 1,
					claimed: false
				}
			])
		);

		let relay_parent_b = Hash::from_low_u64_be(2);
		let claim_queue_b = vec![para_id_a, para_id_b]; // should be [b, a]
		state.add_leaf(&relay_parent_b, &claim_queue_b);

		assert_eq!(state.unclaimed_at(&relay_parent_b), vec![para_id_a, para_id_b]);
		// claims for `relay_parent_a` has changed.
		assert_eq!(state.unclaimed_at(&relay_parent_a), vec![para_id_a, para_id_a, para_id_b]);

		assert_eq!(
			state.block_state,
			VecDeque::from(vec![
				ClaimInfo {
					hash: Some(relay_parent_a),
					claim: Some(para_id_a),
					claim_queue_len: 3,
					claimed: false,
				},
				ClaimInfo {
					hash: Some(relay_parent_b),
					claim: Some(para_id_a),
					claim_queue_len: 2,
					claimed: false,
				}
			])
		);
		assert_eq!(
			state.future_blocks,
			VecDeque::from(vec![ClaimInfo {
				hash: None,
				claim: Some(para_id_b),
				claim_queue_len: 1,
				claimed: false
			},])
		);
	}

	#[test]
	fn claim_queue_becomes_shorter_and_drops_future_claims() {
		let mut state = ClaimQueueState::new();
		let relay_parent_a = Hash::from_low_u64_be(1);
		let para_id_a = ParaId::new(1);
		let para_id_b = ParaId::new(2);
		let claim_queue_a = vec![para_id_a, para_id_b, para_id_a, para_id_b];

		state.add_leaf(&relay_parent_a, &claim_queue_a);

		assert_eq!(
			state.unclaimed_at(&relay_parent_a),
			vec![para_id_a, para_id_b, para_id_a, para_id_b]
		);

		// We start with claim queue len 4.
		assert_eq!(
			state.block_state,
			VecDeque::from(vec![ClaimInfo {
				hash: Some(relay_parent_a),
				claim: Some(para_id_a),
				claim_queue_len: 4,
				claimed: false,
			},])
		);
		// we have got three future blocks
		assert_eq!(
			state.future_blocks,
			VecDeque::from(vec![
				ClaimInfo {
					hash: None,
					claim: Some(para_id_b),
					claim_queue_len: 1,
					claimed: false
				},
				ClaimInfo {
					hash: None,
					claim: Some(para_id_a),
					claim_queue_len: 1,
					claimed: false
				},
				ClaimInfo {
					hash: None,
					claim: Some(para_id_b),
					claim_queue_len: 1,
					claimed: false
				}
			])
		);

		// The next claim len is 2, so we loose one future block
		let relay_parent_b = Hash::from_low_u64_be(2);
		let para_id_a = ParaId::new(1);
		let para_id_b = ParaId::new(2);
		let claim_queue_b = vec![para_id_b, para_id_a];
		state.add_leaf(&relay_parent_b, &claim_queue_b);

		assert_eq!(state.unclaimed_at(&relay_parent_a), vec![para_id_a, para_id_b, para_id_a]);
		assert_eq!(state.unclaimed_at(&relay_parent_b), vec![para_id_b, para_id_a]);

		assert_eq!(
			state.block_state,
			VecDeque::from(vec![
				ClaimInfo {
					hash: Some(relay_parent_a),
					claim: Some(para_id_a),
					claim_queue_len: 4,
					claimed: false,
				},
				ClaimInfo {
					hash: Some(relay_parent_b),
					claim: Some(para_id_b),
					claim_queue_len: 2,
					claimed: false,
				}
			])
		);

		assert_eq!(
			state.future_blocks,
			VecDeque::from(vec![ClaimInfo {
				hash: None,
				claim: Some(para_id_a),
				claim_queue_len: 1,
				claimed: false
			},])
		);
	}
}
