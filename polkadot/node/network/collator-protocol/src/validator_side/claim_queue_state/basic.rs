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

use super::*;
use crate::LOG_TARGET;

use std::collections::{HashMap, HashSet, VecDeque};

use polkadot_primitives::{CandidateHash, Hash, Id as ParaId};

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
	pub(super) block_state: VecDeque<ClaimInfo>,
	/// Represents blocks which are yet to be created. The claim queue at a leaf tells us what's
	/// scheduled on the next two blocks. Since they are not yet authored we keep them separately
	/// here.
	future_blocks: VecDeque<ClaimInfo>,
	/// Candidates with claimed slots per relay parent. We need this information in order to remove
	/// claims. The key is the relay parent of the candidate and the value - the set of candidates
	/// at it.
	///
	/// Note 1: We can't map the candidates to an exact slot since we need to keep track on their
	/// ordering which will be an overkill in the context of `ClaimQueueState`. That's why we keep
	/// the claims on first came first claimed basis.
	///
	/// Let's say there are three candidates built on top of each other A->B->C. We claim slots for
	/// them in the order we receive them. If we receive them out of order the claims will be out
	/// of order too, but this is fine since we only care about the count. If we fetch B and C
	/// successfully and can't fetch A the user of `ClaimQueueState` must make sure that B and C
	/// are removed.
	///
	/// Note 2: During pruning we remove all the candidates for the pruned relay parent because we
	/// no longer need to know about them. If the claim was not undone so far - it will be
	/// permanent.
	candidates_per_rp: HashMap<Hash, HashSet<CandidateHash>>,
}

impl ClaimQueueState {
	/// Create an empty `ClaimQueueState`. Use [`add_leaf`] to populate it.
	pub(crate) fn new() -> Self {
		Self {
			block_state: VecDeque::new(),
			future_blocks: VecDeque::new(),
			candidates_per_rp: HashMap::new(),
		}
	}

	pub(super) fn fork(&self, target_relay_parent: &Hash) -> Option<Self> {
		if self.block_state.back().and_then(|state| state.hash) == Some(*target_relay_parent) {
			// don't fork from the last block!
			return None
		}

		// Find the index of the target relay parent in the old state
		let target_index = self
			.block_state
			.iter()
			.position(|claim_info| claim_info.hash == Some(*target_relay_parent))?;

		let block_state =
			self.block_state.iter().cloned().take(target_index + 1).collect::<VecDeque<_>>();

		let rp_in_view = block_state.iter().filter_map(|c| c.hash).collect::<HashSet<_>>();

		let candidates = self
			.candidates_per_rp
			.iter()
			.filter(|(rp, _)| rp_in_view.contains(rp))
			.map(|(rp, c)| (*rp, c.clone()))
			.collect::<HashMap<_, _>>();

		let candidates_in_view = candidates.iter().fold(HashSet::new(), |mut acc, (_, c)| {
			acc.extend(c.iter().cloned());
			acc
		});

		// Transfer any claims from the target relay parent's ancestors onto the new path.
		// Example:
		// old_state: [A B C D]; target_relay_parent = B;
		// Any claims on C and D should also be transferred to the new fork. Because when we claim a
		// slot at relay parent X we claim it at all possible forks.
		// Also note that only claims for candidates which fall within new fork's view are
		// transferred.
		let future_blocks = self
			.get_window(target_relay_parent)
			.skip(1)
			.map(|c| ClaimInfo {
				hash: None,
				claim: c.claim,
				claim_queue_len: 1,
				claimed: c.claimed.clone_or_default(&candidates_in_view),
			})
			.collect::<VecDeque<_>>();

		Some(ClaimQueueState { block_state, future_blocks, candidates_per_rp: candidates })
	}

	/// Appends a new leaf with its corresponding claim queue to the state.
	pub(crate) fn add_leaf(&mut self, hash: &Hash, claim_queue: &VecDeque<ParaId>) {
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
						// queue.
						future_block.claim = Some(*expected_claim);
						// We mark the slot as unclaimed since the `ParaId` has changed.
						future_block.claimed = ClaimState::Free;

						// IMPORTANT: at this point there will be a slight inconsistency between
						// `block_state`/`future_blocks` and `candidates`. We just removed a future
						// claim but we can't be sure which candidate should be removed since we
						// don't know the exact ordering between them. So we just keep `candidates`
						// untouched which means we will sheepishly pretend there are claims for the
						// extra candidate. This is not ideal but the case is very rare and is not
						// worth the extra complexity for handling it.
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
						claimed: ClaimState::Free,
					});
				},
			}
		}

		// Now pop the first future block and add it as a leaf
		let claim_info = match self.future_blocks.pop_front() {
			Some(new_leaf) => ClaimInfo {
				hash: Some(*hash),
				claim: claim_queue.front().copied(),
				claim_queue_len: claim_queue.len(),
				claimed: new_leaf.claimed,
			},
			None => {
				// maybe the claim queue was empty, but we still need to add a leaf
				ClaimInfo {
					hash: Some(*hash),
					claim: claim_queue.front().copied(),
					claim_queue_len: claim_queue.len(),
					claimed: ClaimState::Free,
				}
			},
		};

		// `future_blocks` can't be longer than the length of the claim queue at the last block - 1.
		// For example this can happen if at relay block N we have got a claim queue of length 4
		// and it's shrunk to 2.
		self.future_blocks.truncate(claim_queue.len().saturating_sub(1));

		self.block_state.push_back(claim_info);
	}

	fn has_claim(&self, relay_parent: &Hash, candidate_hash: &CandidateHash) -> bool {
		self.candidates_per_rp
			.get(relay_parent)
			.map_or(false, |c| c.contains(candidate_hash))
	}

	fn trace_has_claim(
		&self,
		relay_parent: &Hash,
		para_id: &ParaId,
		maybe_candidate_hash: Option<&CandidateHash>,
	) -> bool {
		if let Some(candidate_hash) = maybe_candidate_hash {
			if self.has_claim(relay_parent, candidate_hash) {
				// there is already a claim for this candidate - return now
				gum::trace!(
					target: LOG_TARGET,
					?para_id,
					?relay_parent,
					?candidate_hash,
					"Claim already exists"
				);
				return true
			}
		}

		false
	}

	fn cache_claim(&mut self, relay_parent: Hash, candidate_hash: CandidateHash) {
		self.candidates_per_rp.entry(relay_parent).or_default().insert(candidate_hash);
	}

	fn do_get_window<T: ClaimInfoRef, I: Iterator<Item = T>>(
		block_state: I,
		future_blocks: I,
		relay_parent: &Hash,
	) -> impl Iterator<Item = T> + use<'_, T, I> {
		let mut window = block_state.skip_while(|info| !info.hash_equals(relay_parent)).peekable();
		let len = window.peek().map_or(0, |info| info.claim_queue_len());
		window.chain(future_blocks).take(len)
	}

	/// Returns an iterator over the claim queue of `relay_parent`
	fn get_window<'a>(&'a self, relay_parent: &'a Hash) -> impl Iterator<Item = &'a ClaimInfo> {
		Self::do_get_window(self.block_state.iter(), self.future_blocks.iter(), relay_parent)
	}

	/// Returns a mutating iterator over the claim queue of `relay_parent`
	fn get_window_mut<'a>(
		&'a mut self,
		relay_parent: &'a Hash,
	) -> impl Iterator<Item = &'a mut ClaimInfo> {
		Self::do_get_window(
			self.block_state.iter_mut(),
			self.future_blocks.iter_mut(),
			relay_parent,
		)
	}

	/// Searches for any of the types provided within `relay_parent`'s view of the claim queue for
	/// `para_id` and returns the first one that is found.
	fn find_claim<'a>(
		&'a mut self,
		relay_parent: &'a Hash,
		para_id: &ParaId,
		lookup: &[ClaimState],
		search_in_future_blocks: bool,
	) -> Option<&'a mut ClaimInfo> {
		let window = self.get_window_mut(relay_parent);
		let window: Box<dyn Iterator<Item = &mut ClaimInfo>> = match search_in_future_blocks {
			true => Box::new(window),
			false => Box::new(window.take(1)),
		};

		for info in window {
			gum::trace!(
				target: LOG_TARGET,
				?para_id,
				?relay_parent,
				claim_info=?info,
				"Checking claim"
			);

			if info.claim == Some(*para_id) && lookup.contains(&info.claimed) {
				return Some(info)
			}
		}

		None
	}

	/// Searches for any of the types provided within `relay_parent`'s view of the claim queue for
	/// `para_id` and replaces the state of the first one that is found with `new_state`.
	///
	/// Returns whether a claim was found, no matter if it had to be replaced or not.
	fn find_and_replace_claim(
		&mut self,
		relay_parent: &Hash,
		para_id: &ParaId,
		lookup: &[ClaimState],
		search_in_future_blocks: bool,
		new_state: ClaimState,
	) -> bool {
		let info = match self.find_claim(relay_parent, para_id, lookup, search_in_future_blocks) {
			Some(info) => {
				info.claimed = new_state;
				info
			},
			None => return false,
		};

		if let Some(candidate_hash) = info.claimed.candidate_hash().cloned() {
			self.cache_claim(*relay_parent, candidate_hash);
		}

		true
	}

	/// Claims the first available slot for `para_id` at `relay_parent` as pending. Returns `true`
	/// if the claim was successful.
	pub(crate) fn claim_pending_at(
		&mut self,
		relay_parent: &Hash,
		para_id: &ParaId,
		maybe_candidate_hash: Option<CandidateHash>,
	) -> bool {
		gum::trace!(
			target: LOG_TARGET,
			?para_id,
			?relay_parent,
			"claim_at"
		);

		if self.trace_has_claim(relay_parent, para_id, maybe_candidate_hash.as_ref()) {
			return true;
		}
		self.find_and_replace_claim(
			relay_parent,
			para_id,
			&[ClaimState::Free],
			true,
			ClaimState::Pending(maybe_candidate_hash),
		)
	}

	/// Claims the first available slot for `para_id` at `relay_parent` as pending. Returns `true`
	/// if the claim was successful. For a v1 advertisement.
	pub(super) fn claim_pending_at_v1(&mut self, relay_parent: &Hash, para_id: &ParaId) -> bool {
		gum::trace!(
			target: LOG_TARGET,
			?para_id,
			?relay_parent,
			"claim_at_v1"
		);

		self.find_and_replace_claim(
			relay_parent,
			para_id,
			&[ClaimState::Free],
			false,
			ClaimState::Pending(None),
		)
	}

	/// Sets the candidate hash for a pending claim. If no such claim is found - returns false.
	/// Note that the candidate is set at first available `Pending(None)` claim. Tracking the exact
	/// candidate order is not required here.
	pub(crate) fn mark_pending_slot_with_candidate(
		&mut self,
		relay_parent: &Hash,
		para_id: &ParaId,
		candidate_hash: CandidateHash,
	) -> bool {
		if self.trace_has_claim(relay_parent, para_id, Some(&candidate_hash)) {
			return true
		}

		self.find_and_replace_claim(
			relay_parent,
			para_id,
			&[ClaimState::Pending(None)],
			false,
			ClaimState::Pending(Some(candidate_hash)),
		)
	}

	/// If there is a pending claim for the candidate at `relay_parent` it is upgraded to seconded.
	/// Otherwise a new claim is made.
	pub(crate) fn claim_seconded_at(
		&mut self,
		relay_parent: &Hash,
		para_id: &ParaId,
		candidate_hash: CandidateHash,
	) -> bool {
		gum::trace!(
			target: LOG_TARGET,
			?para_id,
			?relay_parent,
			?candidate_hash,
			"second_at"
		);

		if self.has_claim(relay_parent, &candidate_hash) {
			let claimed = self.find_and_replace_claim(
				relay_parent,
				para_id,
				&[ClaimState::Pending(Some(candidate_hash)), ClaimState::Seconded(candidate_hash)],
				true,
				ClaimState::Seconded(candidate_hash),
			);
			if claimed {
				return true;
			}

			gum::warn!(
				target: LOG_TARGET,
				?para_id,
				?relay_parent,
				?candidate_hash,
				"Hash found in candidates but can't find a claim for it. This should never happen"
			);
			return false;
		}

		// this is a new claim
		self.find_and_replace_claim(
			relay_parent,
			para_id,
			&[ClaimState::Free],
			true,
			ClaimState::Seconded(candidate_hash),
		)
	}

	/// Returns `true` if there is a free spot in claim queue (free claim) for `para_id` at
	/// `relay_parent` or if there is an existing claim for the provided candidate at
	/// `relay_parent`.
	pub(crate) fn has_or_can_claim_at(
		&mut self,
		relay_parent: &Hash,
		para_id: &ParaId,
		maybe_candidate_hash: Option<CandidateHash>,
	) -> bool {
		gum::trace!(
			target: LOG_TARGET,
			?para_id,
			?relay_parent,
			"has_or_can_claim_at"
		);

		if self.trace_has_claim(relay_parent, para_id, maybe_candidate_hash.as_ref()) {
			return true
		}
		self.find_claim(relay_parent, para_id, &[ClaimState::Free], true).is_some()
	}

	/// Returns a `Vec` of `ParaId`s with all free claims at `relay_parent`.
	pub(crate) fn get_free_at(&self, relay_parent: &Hash) -> VecDeque<ParaId> {
		let window = self.get_window(relay_parent);
		window
			.filter(|b| matches!(b.claimed, ClaimState::Free))
			.filter_map(|b| b.claim)
			.collect()
	}

	/// Returns the number of claims for a specific para id at a specific relay parent.
	pub(super) fn count_all_for_para_at(&self, relay_parent: &Hash, para_id: &ParaId) -> usize {
		let window = self.get_window(relay_parent);
		window.filter(|info| info.claim == Some(*para_id)).count()
	}

	/// Removes pruned relay parent blocks from the beginning of all paths.
	///
	/// Only the hashes that are at the beginning of the paths will be removed.
	///
	/// Example: if a path is [A, B, C, D] and `targets` contains [A, B] then both A and B will be
	/// removed. But if `targets` contains [B, C] then nothing will be removed.
	pub(super) fn remove_pruned_ancestors(&mut self, targets: &HashSet<Hash>) {
		// All the blocks that should be pruned are in the front of `block_state`. Since
		// `block_state` is not ordered - keep popping until the first element is not found in
		// `targets`.
		let mut actual_targets = HashSet::new();
		loop {
			match self.block_state.front().and_then(|claim_info| claim_info.hash) {
				Some(hash) if targets.contains(&hash) => {
					self.block_state.pop_front();
					actual_targets.insert(hash);
				},
				_ => break,
			}
		}

		// First remove all entries from candidates for each removed relay parent. Any Seconded
		// entries for it can't be undone anymore, but the claimed ones may need to be freed.
		let mut removed_candidates = HashSet::with_capacity(actual_targets.len());
		for target in actual_targets {
			if let Some(candidates) = self.candidates_per_rp.remove(&target) {
				removed_candidates.extend(candidates.into_iter());
			}
		}

		for claim_info in self.block_state.iter_mut() {
			if let ClaimState::Pending(Some(candidate_hash)) = claim_info.claimed {
				if removed_candidates.contains(&candidate_hash) {
					claim_info.claimed = ClaimState::Free;
				}
			}
		}
	}

	/// Returns true if the path is empty
	pub(super) fn is_empty(&self) -> bool {
		self.block_state.is_empty()
	}

	/// Releases a pending or seconded claim (sets it to free) for a candidate.
	pub(super) fn release_claim(&mut self, candidate_hash: &CandidateHash) -> bool {
		// Get the relay parent from candidates.
		let mut maybe_relay_parent = None;
		for (relay_parent, candidates) in &mut self.candidates_per_rp {
			if candidates.remove(candidate_hash) {
				maybe_relay_parent = Some(*relay_parent);
				break
			}
		}
		let relay_parent = match maybe_relay_parent {
			Some(relay_parent) => relay_parent,
			None => return false,
		};

		let window = self.get_window_mut(&relay_parent);
		for w in window {
			if w.claimed == ClaimState::Pending(Some(*candidate_hash)) ||
				w.claimed == ClaimState::Seconded(*candidate_hash)
			{
				w.claimed = ClaimState::Free;
				return true
			}
		}

		false
	}

	/// Explicitly clears a claim at a specific relay parent.
	pub(super) fn release_claim_for_relay_parent(&mut self, relay_parent: &Hash) -> bool {
		for claim in self.block_state.iter_mut() {
			if claim.hash.as_ref() != Some(relay_parent) {
				continue;
			}

			if let Some(candidate_hash) = claim.claimed.candidate_hash() {
				if let Some(candidates) = self.candidates_per_rp.get_mut(relay_parent) {
					candidates.remove(&candidate_hash);
				}
			}
			claim.claimed = ClaimState::Free;
			return true
		}

		false
	}

	fn get_full_path(&self) -> impl Iterator<Item = &ClaimInfo> {
		self.block_state.iter().chain(self.future_blocks.iter())
	}

	/// Returns the claim queue entries for all known and future blocks.
	pub(super) fn all_assignments(&self) -> impl Iterator<Item = &ParaId> {
		self.get_full_path().filter_map(|claim_info| claim_info.claim.as_ref())
	}

	/// Returns the corresponding para ids for all unclaimed slots in the claim queue.
	pub(super) fn free_slots(&self) -> Vec<ParaId> {
		self.get_full_path()
			.filter_map(|claim_info| {
				if claim_info.claimed == ClaimState::Free {
					return claim_info.claim;
				}
				None
			})
			.collect()
	}

	/// Returns a `Vec` of `ParaId`s with all pending claims at `relay_parent`.
	#[cfg(test)]
	fn get_pending_at(&self, relay_parent: &Hash) -> VecDeque<ParaId> {
		let window = self.get_window(relay_parent);

		window
			.filter(|b| matches!(b.claimed, ClaimState::Pending(_)))
			.filter_map(|b| b.claim)
			.collect()
	}
}

#[cfg(test)]
mod test {
	use super::*;
	use crate::validator_side::claim_queue_state::test::*;

	#[test]
	fn sane_initial_state() {
		let mut state = ClaimQueueState::new();

		assert!(!state.has_or_can_claim_at(&RELAY_PARENT_A, &PARA_1, Some(*CANDIDATE_A1)));
		assert!(!state.claim_pending_at(&RELAY_PARENT_A, &PARA_1, Some(*CANDIDATE_A1)));
		assert!(state.get_pending_at(&RELAY_PARENT_A).is_empty());
		assert!(state.is_empty());
	}

	#[test]
	fn fork_works() {
		let mut state = ClaimQueueState::new();

		state.add_leaf(&RELAY_PARENT_A, &[PARA_1, PARA_2].into());
		state.add_leaf(&RELAY_PARENT_B, &[PARA_2, PARA_3].into());
		state.add_leaf(&RELAY_PARENT_C, &[PARA_3, PARA_1, PARA_2].into());
		state.add_leaf(&RELAY_PARENT_D, &[PARA_1, PARA_2, PARA_3, PARA_1].into());

		assert!(state.claim_pending_at(&RELAY_PARENT_A, &PARA_1, Some(*CANDIDATE_A1)));
		assert!(state.claim_pending_at(&RELAY_PARENT_B, &PARA_2, Some(*CANDIDATE_B1)));
		assert!(state.claim_seconded_at(&RELAY_PARENT_B, &PARA_3, *CANDIDATE_B2));
		assert!(state.claim_pending_at(&RELAY_PARENT_C, &PARA_1, Some(*CANDIDATE_C1)));
		assert!(state.claim_pending_at(&RELAY_PARENT_C, &PARA_2, Some(*CANDIDATE_C2)));
		assert!(state.claim_seconded_at(&RELAY_PARENT_D, &PARA_3, *CANDIDATE_D1));

		let expected_block_state = [
			ClaimInfo::new_pending(2, Some(*CANDIDATE_A1)).with(*RELAY_PARENT_A, PARA_1),
			ClaimInfo::new_pending(2, Some(*CANDIDATE_B1)).with(*RELAY_PARENT_B, PARA_2),
			ClaimInfo::new_seconded(3, *CANDIDATE_B2).with(*RELAY_PARENT_C, PARA_3),
			ClaimInfo::new_pending(4, Some(*CANDIDATE_C1)).with(*RELAY_PARENT_D, PARA_1),
		];
		assert_eq!(state.block_state.make_contiguous(), expected_block_state,);

		let expected_future_blocks = [
			ClaimInfo::new_pending(1, Some(*CANDIDATE_C2)).with_claim(PARA_2),
			ClaimInfo::new_seconded(1, *CANDIDATE_D1).with_claim(PARA_3),
			ClaimInfo::new_free(1).with_claim(PARA_1),
		];
		assert_eq!(state.future_blocks.make_contiguous(), expected_future_blocks);

		let expected_candidates_per_rp = [
			(*RELAY_PARENT_A, HashSet::from([*CANDIDATE_A1])),
			(*RELAY_PARENT_B, HashSet::from([*CANDIDATE_B1, *CANDIDATE_B2])),
			(*RELAY_PARENT_C, HashSet::from([*CANDIDATE_C1, *CANDIDATE_C2])),
			(*RELAY_PARENT_D, HashSet::from([*CANDIDATE_D1])),
		];
		assert_eq!(state.candidates_per_rp, HashMap::from(expected_candidates_per_rp.clone()));

		// Fork at `RELAY_PARENT_A`
		let mut fork = state.fork(&RELAY_PARENT_A).unwrap();
		assert_eq!(fork.block_state.make_contiguous(), &expected_block_state[..1],);
		assert_eq!(
			fork.future_blocks,
			VecDeque::from([ClaimInfo::new_free(1).with_claim(PARA_2),])
		);
		assert_eq!(
			fork.candidates_per_rp,
			HashMap::from_iter(expected_candidates_per_rp[0..1].iter().cloned())
		);

		// Fork at `RELAY_PARENT_B`
		let mut fork = state.fork(&RELAY_PARENT_B).unwrap();
		assert_eq!(fork.block_state.make_contiguous(), &expected_block_state[..2],);
		assert_eq!(
			fork.future_blocks,
			VecDeque::from([ClaimInfo::new_seconded(1, *CANDIDATE_B2).with_claim(PARA_3),])
		);
		assert_eq!(
			fork.candidates_per_rp,
			HashMap::from_iter(expected_candidates_per_rp[0..2].iter().cloned())
		);

		// Fork at `RELAY_PARENT_C`
		let mut fork = state.fork(&RELAY_PARENT_C).unwrap();
		assert_eq!(fork.block_state.make_contiguous(), &expected_block_state[..3],);
		assert_eq!(
			fork.future_blocks,
			VecDeque::from([
				ClaimInfo::new_pending(1, Some(*CANDIDATE_C1)).with_claim(PARA_1),
				expected_future_blocks[0].clone()
			])
		);
		assert_eq!(
			fork.candidates_per_rp,
			HashMap::from_iter(expected_candidates_per_rp[0..3].iter().cloned())
		);

		// Fork at `RELAY_PARENT_D`
		// Should not be able to fork from the last block
		assert!(state.fork(&RELAY_PARENT_D).is_none());
	}

	#[test]
	fn add_leaf_works() {
		let mut state = ClaimQueueState::new();
		let claim_queue = VecDeque::from([PARA_1, PARA_1, PARA_1]);

		state.add_leaf(&RELAY_PARENT_A, &claim_queue);
		assert!(state.has_or_can_claim_at(&RELAY_PARENT_A, &PARA_1, None));
		assert!(state.get_pending_at(&RELAY_PARENT_A).is_empty());

		assert_eq!(
			state.block_state,
			VecDeque::from([ClaimInfo::new_free(3).with(*RELAY_PARENT_A, PARA_1),])
		);
		assert_eq!(
			state.future_blocks,
			VecDeque::from([
				ClaimInfo::new_free(1).with_claim(PARA_1),
				ClaimInfo::new_free(1).with_claim(PARA_1),
			])
		);
		assert!(state.candidates_per_rp.is_empty());

		// should be no op
		state.add_leaf(&RELAY_PARENT_A, &claim_queue);
		assert_eq!(state.block_state.len(), 1);
		assert_eq!(state.future_blocks.len(), 2);

		// add another leaf
		state.add_leaf(&RELAY_PARENT_B, &claim_queue);

		assert_eq!(
			state.block_state,
			VecDeque::from([
				ClaimInfo::new_free(3).with(*RELAY_PARENT_A, PARA_1),
				ClaimInfo::new_free(3).with(*RELAY_PARENT_B, PARA_1),
			])
		);
		assert_eq!(
			state.future_blocks,
			VecDeque::from([
				ClaimInfo::new_free(1).with_claim(PARA_1),
				ClaimInfo::new_free(1).with_claim(PARA_1)
			])
		);
		assert!(state.candidates_per_rp.is_empty());
		assert!(state.has_or_can_claim_at(&RELAY_PARENT_B, &PARA_1, None));
		assert!(state.get_pending_at(&RELAY_PARENT_B).is_empty());
	}

	#[test]
	fn basic_claims_work() {
		let mut state = ClaimQueueState::new();
		let claim_queue = VecDeque::from([PARA_1, PARA_1, PARA_1]);

		state.add_leaf(&RELAY_PARENT_A, &claim_queue);

		assert!(state.has_or_can_claim_at(&RELAY_PARENT_A, &PARA_1, Some(*CANDIDATE_A1)));
		assert!(state.claim_pending_at(&RELAY_PARENT_A, &PARA_1, Some(*CANDIDATE_A1)));
		// Claiming the same slot again should return true
		assert!(state.has_or_can_claim_at(&RELAY_PARENT_A, &PARA_1, Some(*CANDIDATE_A1)));
		assert!(state.claim_pending_at(&RELAY_PARENT_A, &PARA_1, Some(*CANDIDATE_A1)));

		assert!(state.has_or_can_claim_at(&RELAY_PARENT_A, &PARA_1, Some(*CANDIDATE_A2)));
		assert!(state.claim_seconded_at(&RELAY_PARENT_A, &PARA_1, *CANDIDATE_A2));
		// Claiming the same slot again should return true
		assert!(state.claim_seconded_at(&RELAY_PARENT_A, &PARA_1, *CANDIDATE_A2));
		assert!(state.has_or_can_claim_at(&RELAY_PARENT_A, &PARA_1, Some(*CANDIDATE_A2)));
	}

	#[test]
	fn claims_at_separate_relay_parents_work() {
		let mut state = ClaimQueueState::new();
		let claim_queue = VecDeque::from([PARA_1, PARA_1, PARA_1]);

		state.add_leaf(&RELAY_PARENT_A, &claim_queue);
		state.add_leaf(&RELAY_PARENT_B, &claim_queue);

		// add one claim for a
		assert!(state.has_or_can_claim_at(&RELAY_PARENT_A, &PARA_1, Some(*CANDIDATE_A1)));
		assert!(state.claim_pending_at(&RELAY_PARENT_A, &PARA_1, Some(*CANDIDATE_A1)));

		// and one for b
		assert!(state.has_or_can_claim_at(&RELAY_PARENT_B, &PARA_1, Some(*CANDIDATE_B1)));
		assert!(state.claim_pending_at(&RELAY_PARENT_B, &PARA_1, Some(*CANDIDATE_B1)));

		assert_eq!(
			state.block_state,
			VecDeque::from([
				ClaimInfo::new_pending(3, Some(*CANDIDATE_A1)).with(*RELAY_PARENT_A, PARA_1),
				ClaimInfo::new_pending(3, Some(*CANDIDATE_B1)).with(*RELAY_PARENT_B, PARA_1),
			])
		);
		assert_eq!(
			state.future_blocks,
			VecDeque::from([
				ClaimInfo::new_free(1).with_claim(PARA_1),
				ClaimInfo::new_free(1).with_claim(PARA_1)
			])
		);
		assert_eq!(
			state.candidates_per_rp,
			HashMap::from([
				(*RELAY_PARENT_A, HashSet::from([*CANDIDATE_A1])),
				(*RELAY_PARENT_B, HashSet::from([*CANDIDATE_B1]))
			])
		);
	}

	#[test]
	fn claims_are_transferred_to_next_slot() {
		let mut state = ClaimQueueState::new();
		let claim_queue = VecDeque::from([PARA_1, PARA_1, PARA_1]);

		state.add_leaf(&RELAY_PARENT_A, &claim_queue);

		// add two claims, 2nd should be transferred to a new leaf
		assert!(state.claim_pending_at(&RELAY_PARENT_A, &PARA_1, Some(*CANDIDATE_A1)));

		assert!(state.claim_pending_at(&RELAY_PARENT_A, &PARA_1, Some(*CANDIDATE_A2)));

		assert_eq!(
			state.block_state,
			VecDeque::from([
				ClaimInfo::new_pending(3, Some(*CANDIDATE_A1)).with(*RELAY_PARENT_A, PARA_1),
			])
		);
		assert_eq!(
			state.future_blocks,
			VecDeque::from([
				ClaimInfo::new_pending(1, Some(*CANDIDATE_A2)).with_claim(PARA_1),
				ClaimInfo::new_free(1).with_claim(PARA_1),
			])
		);
		assert_eq!(
			state.candidates_per_rp,
			HashMap::from([(*RELAY_PARENT_A, HashSet::from([*CANDIDATE_A1, *CANDIDATE_A2]))])
		);

		// one more
		assert!(state.claim_pending_at(&RELAY_PARENT_A, &PARA_1, Some(*CANDIDATE_A3)));

		assert_eq!(
			state.block_state,
			VecDeque::from([
				ClaimInfo::new_pending(3, Some(*CANDIDATE_A1)).with(*RELAY_PARENT_A, PARA_1),
			])
		);
		assert_eq!(
			state.future_blocks,
			VecDeque::from([
				ClaimInfo::new_pending(1, Some(*CANDIDATE_A2)).with_claim(PARA_1),
				ClaimInfo::new_pending(1, Some(*CANDIDATE_A3)).with_claim(PARA_1),
			])
		);
		assert_eq!(
			state.candidates_per_rp,
			HashMap::from([(
				*RELAY_PARENT_A,
				HashSet::from([*CANDIDATE_A1, *CANDIDATE_A2, *CANDIDATE_A3])
			)])
		);

		// no more claims
		assert!(!state.has_or_can_claim_at(&RELAY_PARENT_A, &PARA_1, Some(*CANDIDATE_A4)));
	}

	#[test]
	fn claims_are_transferred_to_new_leaves() {
		let mut state = ClaimQueueState::new();
		let claim_queue = VecDeque::from([PARA_1, PARA_1, PARA_1]);

		state.add_leaf(&RELAY_PARENT_A, &claim_queue);

		assert!(state.claim_pending_at(&RELAY_PARENT_A, &PARA_1, Some(*CANDIDATE_A1)));
		assert!(state.claim_pending_at(&RELAY_PARENT_A, &PARA_1, Some(*CANDIDATE_A2)));
		assert!(state.claim_pending_at(&RELAY_PARENT_A, &PARA_1, Some(*CANDIDATE_A3)));

		assert_eq!(
			state.block_state,
			VecDeque::from([
				ClaimInfo::new_pending(3, Some(*CANDIDATE_A1)).with(*RELAY_PARENT_A, PARA_1),
			])
		);
		assert_eq!(
			state.future_blocks,
			VecDeque::from([
				ClaimInfo::new_pending(1, Some(*CANDIDATE_A2)).with_claim(PARA_1),
				ClaimInfo::new_pending(1, Some(*CANDIDATE_A3)).with_claim(PARA_1),
			])
		);
		assert_eq!(
			state.candidates_per_rp,
			HashMap::from([(
				*RELAY_PARENT_A,
				HashSet::from([*CANDIDATE_A1, *CANDIDATE_A2, *CANDIDATE_A3])
			)])
		);

		// no more claims
		assert!(!state.has_or_can_claim_at(&RELAY_PARENT_A, &PARA_1, Some(*CANDIDATE_A4)));

		// new leaf
		state.add_leaf(&RELAY_PARENT_B, &claim_queue);

		assert_eq!(
			state.block_state,
			VecDeque::from([
				ClaimInfo::new_pending(3, Some(*CANDIDATE_A1)).with(*RELAY_PARENT_A, PARA_1),
				ClaimInfo::new_pending(3, Some(*CANDIDATE_A2)).with(*RELAY_PARENT_B, PARA_1),
			])
		);
		assert_eq!(
			state.future_blocks,
			VecDeque::from([
				ClaimInfo::new_pending(1, Some(*CANDIDATE_A3)).with_claim(PARA_1),
				ClaimInfo::new_free(1).with_claim(PARA_1),
			])
		);

		// still no claims for a
		assert!(!state.has_or_can_claim_at(&RELAY_PARENT_A, &PARA_1, Some(*CANDIDATE_A4)));

		// but can accept for b
		assert!(state.claim_pending_at(&RELAY_PARENT_B, &PARA_1, Some(*CANDIDATE_A4)));

		assert_eq!(
			state.block_state,
			VecDeque::from([
				ClaimInfo::new_pending(3, Some(*CANDIDATE_A1)).with(*RELAY_PARENT_A, PARA_1),
				ClaimInfo::new_pending(3, Some(*CANDIDATE_A2)).with(*RELAY_PARENT_B, PARA_1),
			])
		);
		assert_eq!(
			state.future_blocks,
			VecDeque::from([
				ClaimInfo::new_pending(1, Some(*CANDIDATE_A3)).with_claim(PARA_1),
				ClaimInfo::new_pending(1, Some(*CANDIDATE_A4)).with_claim(PARA_1),
			])
		);
		assert_eq!(
			state.candidates_per_rp,
			HashMap::from([
				(*RELAY_PARENT_A, HashSet::from([*CANDIDATE_A1, *CANDIDATE_A2, *CANDIDATE_A3])),
				(*RELAY_PARENT_B, HashSet::from([*CANDIDATE_A4]))
			])
		);
	}

	#[test]
	fn two_paras() {
		let mut state = ClaimQueueState::new();
		let claim_queue = VecDeque::from([PARA_1, PARA_2, PARA_1]);

		state.add_leaf(&RELAY_PARENT_A, &claim_queue);
		assert!(state.has_or_can_claim_at(&RELAY_PARENT_A, &PARA_1, Some(*CANDIDATE_A1)));
		assert!(state.has_or_can_claim_at(&RELAY_PARENT_A, &PARA_2, Some(*CANDIDATE_B1)));

		assert_eq!(
			state.block_state,
			VecDeque::from([ClaimInfo::new_free(3).with(*RELAY_PARENT_A, PARA_1),])
		);
		assert_eq!(
			state.future_blocks,
			VecDeque::from([
				ClaimInfo::new_free(1).with_claim(PARA_2),
				ClaimInfo::new_free(1).with_claim(PARA_1),
			])
		);
		assert!(state.candidates_per_rp.is_empty());

		// claim a candidate
		assert!(state.claim_pending_at(&RELAY_PARENT_A, &PARA_1, Some(*CANDIDATE_A1)));

		// we should still be able to claim candidates for both paras
		assert!(state.has_or_can_claim_at(&RELAY_PARENT_A, &PARA_1, Some(*CANDIDATE_A2)));
		assert!(state.has_or_can_claim_at(&RELAY_PARENT_A, &PARA_2, Some(*CANDIDATE_B1)));

		assert_eq!(
			state.block_state,
			VecDeque::from([
				ClaimInfo::new_pending(3, Some(*CANDIDATE_A1)).with(*RELAY_PARENT_A, PARA_1),
			])
		);
		assert_eq!(
			state.future_blocks,
			VecDeque::from([
				ClaimInfo::new_free(1).with_claim(PARA_2),
				ClaimInfo::new_free(1).with_claim(PARA_1),
			])
		);
		assert_eq!(
			state.candidates_per_rp,
			HashMap::from([(*RELAY_PARENT_A, HashSet::from([*CANDIDATE_A1])),])
		);

		// another claim for a
		assert!(state.claim_pending_at(&RELAY_PARENT_A, &PARA_1, Some(*CANDIDATE_A2)));

		// no more claims for a, but should be able to claim for b
		assert!(!state.has_or_can_claim_at(&RELAY_PARENT_A, &PARA_1, Some(*CANDIDATE_A3)));
		assert!(state.has_or_can_claim_at(&RELAY_PARENT_A, &PARA_2, Some(*CANDIDATE_B1)));

		assert_eq!(
			state.block_state,
			VecDeque::from([
				ClaimInfo::new_pending(3, Some(*CANDIDATE_A1)).with(*RELAY_PARENT_A, PARA_1),
			])
		);
		assert_eq!(
			state.future_blocks,
			VecDeque::from([
				ClaimInfo::new_free(1).with_claim(PARA_2),
				ClaimInfo::new_pending(1, Some(*CANDIDATE_A2)).with_claim(PARA_1),
			])
		);
		assert_eq!(
			state.candidates_per_rp,
			HashMap::from([(*RELAY_PARENT_A, HashSet::from([*CANDIDATE_A1, *CANDIDATE_A2]))])
		);

		// another claim for b
		assert!(state.claim_pending_at(&RELAY_PARENT_A, &PARA_2, Some(*CANDIDATE_B1)));

		// no more claims neither for a nor for b
		assert!(!state.has_or_can_claim_at(&RELAY_PARENT_A, &PARA_1, Some(*CANDIDATE_A3)));
		assert!(!state.has_or_can_claim_at(&RELAY_PARENT_A, &PARA_2, Some(*CANDIDATE_B2)));

		assert_eq!(
			state.block_state,
			VecDeque::from([
				ClaimInfo::new_pending(3, Some(*CANDIDATE_A1)).with(*RELAY_PARENT_A, PARA_1),
			])
		);
		assert_eq!(
			state.future_blocks,
			VecDeque::from([
				ClaimInfo::new_pending(1, Some(*CANDIDATE_B1)).with_claim(PARA_2),
				ClaimInfo::new_pending(1, Some(*CANDIDATE_A2)).with_claim(PARA_1),
			])
		);
		assert_eq!(
			state.candidates_per_rp,
			HashMap::from([(
				*RELAY_PARENT_A,
				HashSet::from([*CANDIDATE_A1, *CANDIDATE_A2, *CANDIDATE_B1])
			)])
		);
	}

	#[test]
	fn claim_queue_changes_unexpectedly() {
		let mut state = ClaimQueueState::new();
		let claim_queue_a = VecDeque::from([PARA_1, PARA_2, PARA_1]);

		state.add_leaf(&RELAY_PARENT_A, &claim_queue_a);

		assert!(state.claim_pending_at(&RELAY_PARENT_A, &PARA_1, Some(*CANDIDATE_A1)));
		assert!(state.claim_pending_at(&RELAY_PARENT_A, &PARA_1, Some(*CANDIDATE_A2)));
		assert!(state.claim_pending_at(&RELAY_PARENT_A, &PARA_2, Some(*CANDIDATE_B1)));

		assert_eq!(
			state.block_state,
			VecDeque::from([
				ClaimInfo::new_pending(3, Some(*CANDIDATE_A1)).with(*RELAY_PARENT_A, PARA_1),
			])
		);
		assert_eq!(
			state.future_blocks,
			VecDeque::from([
				ClaimInfo::new_pending(1, Some(*CANDIDATE_B1)).with_claim(PARA_2),
				ClaimInfo::new_pending(1, Some(*CANDIDATE_A2)).with_claim(PARA_1),
			])
		);
		assert_eq!(
			state.candidates_per_rp,
			HashMap::from([(
				*RELAY_PARENT_A,
				HashSet::from([*CANDIDATE_A1, *CANDIDATE_A2, *CANDIDATE_B1,])
			)])
		);

		let claim_queue_b = VecDeque::from([PARA_1, PARA_1, PARA_1]); // should be [b, a,...]
		state.add_leaf(&RELAY_PARENT_B, &claim_queue_b);

		// because of the unexpected change in claim queue we lost the claim for paraB and have
		// one unclaimed for paraA
		assert_eq!(
			state.block_state,
			VecDeque::from([
				ClaimInfo::new_pending(3, Some(*CANDIDATE_A1)).with(*RELAY_PARENT_A, PARA_1),
				ClaimInfo::new_free(3).with(*RELAY_PARENT_B, PARA_1),
			])
		);
		assert_eq!(
			state.future_blocks,
			// since the 3rd slot of the claim queue at rp1 is equal to the second one in rp2,
			// this claim still exists
			VecDeque::from([
				ClaimInfo::new_pending(1, Some(*CANDIDATE_A2)).with_claim(PARA_1),
				ClaimInfo::new_free(1).with_claim(PARA_1),
			])
		);
		// IMPORTANT: we don't change `candidates`
		assert_eq!(
			state.candidates_per_rp,
			HashMap::from([(
				*RELAY_PARENT_A,
				HashSet::from([*CANDIDATE_A1, *CANDIDATE_A2, *CANDIDATE_B1])
			)])
		);
	}

	#[test]
	fn claim_queue_changes_unexpectedly_with_two_blocks() {
		let mut state = ClaimQueueState::new();
		let claim_queue_a = VecDeque::from([PARA_1, PARA_2, PARA_2]);

		state.add_leaf(&RELAY_PARENT_A, &claim_queue_a);
		assert!(state.claim_pending_at(&RELAY_PARENT_A, &PARA_1, Some(*CANDIDATE_A1)));
		assert!(state.claim_pending_at(&RELAY_PARENT_A, &PARA_2, Some(*CANDIDATE_B1)));
		assert!(state.claim_pending_at(&RELAY_PARENT_A, &PARA_2, Some(*CANDIDATE_B2)));

		assert_eq!(
			state.block_state,
			VecDeque::from([
				ClaimInfo::new_pending(3, Some(*CANDIDATE_A1)).with(*RELAY_PARENT_A, PARA_1),
			])
		);
		assert_eq!(
			state.future_blocks,
			VecDeque::from([
				ClaimInfo::new_pending(1, Some(*CANDIDATE_B1)).with_claim(PARA_2),
				ClaimInfo::new_pending(1, Some(*CANDIDATE_B2)).with_claim(PARA_2),
			])
		);
		assert_eq!(
			state.candidates_per_rp,
			HashMap::from([(
				*RELAY_PARENT_A,
				HashSet::from([*CANDIDATE_A1, *CANDIDATE_B1, *CANDIDATE_B2])
			)])
		);

		let claim_queue_b = VecDeque::from([PARA_1, PARA_1, PARA_1]); // should be [b, b, ...]
		state.add_leaf(&RELAY_PARENT_B, &claim_queue_b);

		// because of the unexpected change in claim queue we lost both claims for paraB and
		// have two unclaimed for paraA
		assert_eq!(
			state.block_state,
			VecDeque::from([
				ClaimInfo::new_pending(3, Some(*CANDIDATE_A1)).with(*RELAY_PARENT_A, PARA_1),
				ClaimInfo::new_free(3).with(*RELAY_PARENT_B, PARA_1)
			])
		);
		assert_eq!(
			state.future_blocks,
			VecDeque::from([
				ClaimInfo::new_free(1).with_claim(PARA_1),
				ClaimInfo::new_free(1).with_claim(PARA_1)
			])
		);
		// IMPORTANT: we don't change `candidates`
		assert_eq!(
			state.candidates_per_rp,
			HashMap::from([(
				*RELAY_PARENT_A,
				HashSet::from([*CANDIDATE_A1, *CANDIDATE_B1, *CANDIDATE_B2])
			)])
		);
	}

	#[test]
	fn basic_remove_works() {
		let mut state = ClaimQueueState::new();
		let claim_queue = VecDeque::from([PARA_1, PARA_1, PARA_1]);

		state.add_leaf(&RELAY_PARENT_A, &claim_queue);
		state.add_leaf(&RELAY_PARENT_B, &claim_queue);

		// add one claim per leaf
		assert!(state.claim_pending_at(&RELAY_PARENT_A, &PARA_1, Some(*CANDIDATE_A1)));
		assert!(state.claim_pending_at(&RELAY_PARENT_B, &PARA_1, Some(*CANDIDATE_A2)));

		state.remove_pruned_ancestors(&HashSet::from([*RELAY_PARENT_A]));

		assert_eq!(state.block_state.len(), 1);
		assert_eq!(state.block_state[0].hash, Some(*RELAY_PARENT_B));
		assert_eq!(state.future_blocks.len(), 2);
		assert_eq!(
			state.candidates_per_rp,
			HashMap::from([(*RELAY_PARENT_B, HashSet::from([*CANDIDATE_A2]))])
		);
	}

	#[test]
	fn remove_non_first_does_nothing() {
		let mut state = ClaimQueueState::new();
		let claim_queue = VecDeque::from([PARA_1, PARA_1, PARA_1]);

		state.add_leaf(&RELAY_PARENT_A, &claim_queue);
		state.add_leaf(&RELAY_PARENT_B, &claim_queue);

		// add one claim per leaf
		assert!(state.claim_pending_at(&RELAY_PARENT_A, &PARA_1, Some(*CANDIDATE_A1)));
		assert!(state.claim_pending_at(&RELAY_PARENT_B, &PARA_1, Some(*CANDIDATE_A2)));

		state.remove_pruned_ancestors(&HashSet::from([*RELAY_PARENT_B]));

		assert_eq!(state.block_state.len(), 2);
		assert_eq!(state.future_blocks.len(), 2);
		assert_eq!(
			state.candidates_per_rp,
			HashMap::from([
				(*RELAY_PARENT_A, HashSet::from([*CANDIDATE_A1])),
				(*RELAY_PARENT_B, HashSet::from([*CANDIDATE_A2]))
			])
		);
	}

	#[test]
	fn remove_multiple_works() {
		let mut state = ClaimQueueState::new();
		let claim_queue = VecDeque::from([PARA_1, PARA_1, PARA_1]);

		state.add_leaf(&RELAY_PARENT_A, &claim_queue);
		assert!(state.claim_pending_at(&RELAY_PARENT_A, &PARA_1, Some(*CANDIDATE_A1)));
		state.add_leaf(&RELAY_PARENT_B, &claim_queue);
		assert!(state.claim_pending_at(&RELAY_PARENT_B, &PARA_1, Some(*CANDIDATE_A2)));
		state.add_leaf(&RELAY_PARENT_C, &claim_queue);
		assert!(state.claim_pending_at(&RELAY_PARENT_C, &PARA_1, Some(*CANDIDATE_A3)));

		state.remove_pruned_ancestors(&HashSet::from([*RELAY_PARENT_A, *RELAY_PARENT_B]));

		assert_eq!(state.block_state.len(), 1);
		assert_eq!(state.block_state[0].hash, Some(*RELAY_PARENT_C));
		assert_eq!(state.future_blocks.len(), 2);
		assert_eq!(
			state.candidates_per_rp,
			HashMap::from([(*RELAY_PARENT_C, HashSet::from([*CANDIDATE_A3]))])
		);
	}

	#[test]
	fn empty_claim_queue() {
		let mut state = ClaimQueueState::new();
		let claim_queue_a = VecDeque::new();

		state.add_leaf(&RELAY_PARENT_A, &claim_queue_a);
		assert_eq!(state.get_pending_at(&RELAY_PARENT_A), []);

		assert_eq!(
			state.block_state,
			VecDeque::from([ClaimInfo::new_free(0).with_hash(*RELAY_PARENT_A),])
		);
		// no claim queue so we know nothing about future blocks
		assert!(state.future_blocks.is_empty());

		assert!(!state.has_or_can_claim_at(&RELAY_PARENT_A, &PARA_1, Some(*CANDIDATE_A1)));
		assert!(!state.claim_pending_at(&RELAY_PARENT_A, &PARA_1, Some(*CANDIDATE_A1)));

		let claim_queue_b = VecDeque::from([PARA_1]);
		state.add_leaf(&RELAY_PARENT_B, &claim_queue_b);

		assert_eq!(
			state.block_state,
			VecDeque::from([
				ClaimInfo::new_free(0).with_hash(*RELAY_PARENT_A),
				ClaimInfo::new_free(1).with(*RELAY_PARENT_B, PARA_1),
			])
		);
		// claim queue with length 1 doesn't say anything about future blocks
		assert!(state.future_blocks.is_empty());

		assert!(!state.has_or_can_claim_at(&RELAY_PARENT_A, &PARA_1, Some(*CANDIDATE_A2)));
		assert!(!state.claim_pending_at(&RELAY_PARENT_A, &PARA_1, Some(*CANDIDATE_A2)));

		assert!(state.has_or_can_claim_at(&RELAY_PARENT_B, &PARA_1, Some(*CANDIDATE_A2)));
		assert!(state.claim_pending_at(&RELAY_PARENT_B, &PARA_1, Some(*CANDIDATE_A2)));

		let claim_queue_c = VecDeque::from([PARA_1, PARA_1]);
		state.add_leaf(&RELAY_PARENT_C, &claim_queue_c);

		assert_eq!(
			state.block_state,
			VecDeque::from([
				ClaimInfo::new_free(0).with_hash(*RELAY_PARENT_A),
				ClaimInfo::new_pending(1, Some(*CANDIDATE_A2)).with(*RELAY_PARENT_B, PARA_1),
				ClaimInfo::new_free(2).with(*RELAY_PARENT_C, PARA_1),
			])
		);
		// claim queue with length 2 fills only one future block
		assert_eq!(
			state.future_blocks,
			VecDeque::from([ClaimInfo::new_free(1).with_claim(PARA_1),])
		);

		assert!(!state.has_or_can_claim_at(&RELAY_PARENT_A, &PARA_1, Some(*CANDIDATE_A3)));
		assert!(!state.claim_pending_at(&RELAY_PARENT_A, &PARA_1, Some(*CANDIDATE_A3)));

		// already claimed
		assert!(!state.has_or_can_claim_at(&RELAY_PARENT_B, &PARA_1, Some(*CANDIDATE_A3)));
		assert!(!state.claim_pending_at(&RELAY_PARENT_B, &PARA_1, Some(*CANDIDATE_A3)));

		assert!(state.has_or_can_claim_at(&RELAY_PARENT_C, &PARA_1, Some(*CANDIDATE_A3)));
	}

	#[test]
	fn claim_queue_becomes_shorter() {
		let mut state = ClaimQueueState::new();
		let claim_queue_a = VecDeque::from([PARA_1, PARA_2, PARA_1]);

		state.add_leaf(&RELAY_PARENT_A, &claim_queue_a);

		assert_eq!(
			state.block_state,
			VecDeque::from([ClaimInfo::new_free(3).with(*RELAY_PARENT_A, PARA_1),])
		);
		assert_eq!(
			state.future_blocks,
			VecDeque::from([
				ClaimInfo::new_free(1).with_claim(PARA_2),
				ClaimInfo::new_free(1).with_claim(PARA_1)
			])
		);

		let claim_queue_b = VecDeque::from([PARA_1, PARA_2]); // should be [b, a, ...]
		state.add_leaf(&RELAY_PARENT_B, &claim_queue_b);

		// claims for `RELAY_PARENT_A` has changed.
		assert_eq!(
			state.block_state,
			VecDeque::from([
				ClaimInfo::new_free(3).with(*RELAY_PARENT_A, PARA_1),
				ClaimInfo::new_free(2).with(*RELAY_PARENT_B, PARA_1),
			])
		);
		assert_eq!(
			state.future_blocks,
			VecDeque::from([ClaimInfo::new_free(1).with_claim(PARA_2),])
		);
	}

	#[test]
	fn claim_queue_becomes_shorter_and_drops_future_claims() {
		let mut state = ClaimQueueState::new();
		let claim_queue_a = VecDeque::from([PARA_1, PARA_2, PARA_1, PARA_2]);

		state.add_leaf(&RELAY_PARENT_A, &claim_queue_a);

		// We start with claim queue len 4.
		assert_eq!(
			state.block_state,
			VecDeque::from([ClaimInfo::new_free(4).with(*RELAY_PARENT_A, PARA_1),])
		);
		// we have got three future blocks
		assert_eq!(
			state.future_blocks,
			VecDeque::from([
				ClaimInfo::new_free(1).with_claim(PARA_2),
				ClaimInfo::new_free(1).with_claim(PARA_1),
				ClaimInfo::new_free(1).with_claim(PARA_2)
			])
		);

		// The next claim len is 2, so we loose one future block
		let claim_queue_b = VecDeque::from([PARA_2, PARA_1]);
		state.add_leaf(&RELAY_PARENT_B, &claim_queue_b);

		assert_eq!(
			state.block_state,
			VecDeque::from([
				ClaimInfo::new_free(4).with(*RELAY_PARENT_A, PARA_1),
				ClaimInfo::new_free(2).with(*RELAY_PARENT_B, PARA_2),
			])
		);

		assert_eq!(
			state.future_blocks,
			VecDeque::from([ClaimInfo::new_free(1).with_claim(PARA_1),])
		);
	}

	#[test]
	fn release_claim_works() {
		let mut state = ClaimQueueState::new();
		let claim_queue = VecDeque::from([PARA_1, PARA_1, PARA_1]);

		state.add_leaf(&RELAY_PARENT_A, &claim_queue);

		assert!(state.claim_pending_at(&RELAY_PARENT_A, &PARA_1, Some(*CANDIDATE_A1)));
		assert!(state.claim_pending_at(&RELAY_PARENT_A, &PARA_1, Some(*CANDIDATE_A2)));

		assert_eq!(
			state.block_state,
			VecDeque::from([
				ClaimInfo::new_pending(3, Some(*CANDIDATE_A1)).with(*RELAY_PARENT_A, PARA_1),
			])
		);
		assert_eq!(
			state.future_blocks,
			VecDeque::from([
				ClaimInfo::new_pending(1, Some(*CANDIDATE_A2)).with_claim(PARA_1),
				ClaimInfo::new_free(1).with_claim(PARA_1)
			])
		);
		assert_eq!(
			state.candidates_per_rp,
			HashMap::from([(*RELAY_PARENT_A, HashSet::from([*CANDIDATE_A1, *CANDIDATE_A2]))])
		);

		state.release_claim(&CANDIDATE_A1);

		assert_eq!(
			state.block_state,
			VecDeque::from([ClaimInfo::new_free(3).with(*RELAY_PARENT_A, PARA_1),])
		);
		assert_eq!(
			state.future_blocks,
			VecDeque::from([
				ClaimInfo::new_pending(1, Some(*CANDIDATE_A2)).with_claim(PARA_1),
				ClaimInfo::new_free(1).with_claim(PARA_1)
			])
		);
		assert_eq!(
			state.candidates_per_rp,
			HashMap::from([(*RELAY_PARENT_A, HashSet::from([*CANDIDATE_A2]))])
		);
	}

	#[test]
	fn claim_seconded_at_works() {
		let mut state = ClaimQueueState::new();
		let claim_queue = VecDeque::from([PARA_1]);

		state.add_leaf(&RELAY_PARENT_A, &claim_queue);

		assert!(state.claim_seconded_at(&RELAY_PARENT_A, &PARA_1, *CANDIDATE_A1));
		assert!(state.claim_pending_at(&RELAY_PARENT_A, &PARA_1, Some(*CANDIDATE_A1)));
		assert!(!state.claim_pending_at(&RELAY_PARENT_A, &PARA_1, Some(*CANDIDATE_A2)));

		assert_eq!(
			state.block_state,
			VecDeque::from([
				ClaimInfo::new_seconded(1, *CANDIDATE_A1).with(*RELAY_PARENT_A, PARA_1),
			])
		);
		assert!(state.future_blocks.is_empty());
	}

	#[test]
	fn get_pending_at_works() {
		let mut state = ClaimQueueState::new();
		let claim_queue = VecDeque::from([PARA_1, PARA_2, PARA_1]);

		state.add_leaf(&RELAY_PARENT_A, &claim_queue);

		assert!(state.get_pending_at(&RELAY_PARENT_A).is_empty());

		assert!(state.claim_pending_at(&RELAY_PARENT_A, &PARA_1, Some(*CANDIDATE_A1)));
		assert_eq!(state.get_pending_at(&RELAY_PARENT_A), [PARA_1]);

		assert!(state.claim_pending_at(&RELAY_PARENT_A, &PARA_1, Some(*CANDIDATE_A2)));
		assert_eq!(state.get_pending_at(&RELAY_PARENT_A), [PARA_1, PARA_1]);

		assert!(state.claim_pending_at(&RELAY_PARENT_A, &PARA_2, Some(*CANDIDATE_B1)));
		assert_eq!(state.get_pending_at(&RELAY_PARENT_A), [PARA_1, PARA_2, PARA_1]);

		let claim_queue = VecDeque::from([PARA_2, PARA_1, PARA_2]);
		state.add_leaf(&RELAY_PARENT_B, &claim_queue);
		assert_eq!(state.get_pending_at(&RELAY_PARENT_B), [PARA_2, PARA_1]);
	}
}
