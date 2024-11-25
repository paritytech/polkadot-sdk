use std::collections::VecDeque;

use polkadot_primitives::{Hash, Id as ParaId};

#[derive(Debug, PartialEq)]
struct ClaimInfo {
	hash: Option<Hash>,
	claim: Option<ParaId>,
	claim_queue_len: usize,
	claimed: bool,
}

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

		self.block_state.push_back(claim_info);
	}

	fn get_window<'a>(
		&'a mut self,
		relay_parent: &'a Hash,
	) -> impl Iterator<Item = &mut ClaimInfo> + 'a {
		let mut window = self
			.block_state
			.iter_mut()
			.skip_while(|b| b.hash != Some(*relay_parent))
			.peekable();
		let cq_len = window.peek().map_or(0, |b| b.claim_queue_len);
		window.chain(self.future_blocks.iter_mut()).take(cq_len)
	}

	pub(crate) fn claim_at(&mut self, relay_parent: &Hash, para_id: &ParaId) -> bool {
		let window = self.get_window(relay_parent);

		for w in window {
			if w.claimed {
				continue
			}

			if w.claim == Some(*para_id) {
				w.claimed = true;
				return true;
			}
		}

		false
	}

	pub(crate) fn can_claim_at(&mut self, relay_parent: &Hash, para_id: &ParaId) -> bool {
		let window = self.get_window(relay_parent);

		for w in window {
			if !w.claimed && w.claim == Some(*para_id) {
				return true
			}
		}

		false
	}

	pub(crate) fn unclaimed_at(&mut self, relay_parent: &Hash) -> Vec<ParaId> {
		let window = self.get_window(relay_parent);

		window.filter(|b| !b.claimed).filter_map(|b| b.claim.clone()).collect()
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
}
