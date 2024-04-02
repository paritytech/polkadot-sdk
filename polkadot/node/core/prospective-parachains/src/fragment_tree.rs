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

//! A tree utility for managing parachain fragments not referenced by the relay-chain.
//!
//! # Overview
//!
//! This module exposes two main types: [`FragmentTree`] and [`CandidateStorage`] which are meant to
//! be used in close conjunction. Each fragment tree is associated with a particular relay-parent
//! and each node in the tree represents a candidate. Each parachain has a single candidate storage,
//! but can have multiple trees for each relay chain block in the view.
//!
//! A tree has an associated [`Scope`] which defines limits on candidates within the tree.
//! Candidates themselves have their own [`Constraints`] which are either the constraints from the
//! scope, or, if there are previous nodes in the tree, a modified version of the previous
//! candidate's constraints.
//!
//! This module also makes use of types provided by the Inclusion Emulator module, such as
//! [`Fragment`] and [`Constraints`]. These perform the actual job of checking for validity of
//! prospective fragments.
//!
//! # Usage
//!
//! It's expected that higher-level code will have a tree for each relay-chain block which might
//! reasonably have blocks built upon it.
//!
//! Because a para only has a single candidate storage, trees only store indices into the storage.
//! The storage is meant to be pruned when trees are dropped by higher-level code.
//!
//! # Cycles
//!
//! Nodes do not uniquely refer to a parachain block for two reasons.
//!   1. There's no requirement that head-data is unique for a parachain. Furthermore, a parachain
//!      is under no obligation to be acyclic, and this is mostly just because it's totally
//!      inefficient to enforce it. Practical use-cases are acyclic, but there is still more than
//!      one way to reach the same head-data.
//!   2. and candidates only refer to their parent by its head-data. This whole issue could be
//!      resolved by having candidates reference their parent by candidate hash.
//!
//! The implication is that when we receive a candidate receipt, there are actually multiple
//! possibilities for any candidates between the para-head recorded in the relay parent's state
//! and the candidate in question.
//!
//! This means that our candidates need to handle multiple parents and that depth is an
//! attribute of a node in a tree, not a candidate. Put another way, the same candidate might
//! have different depths in different parts of the tree.
//!
//! As an extreme example, a candidate which produces head-data which is the same as its parent
//! can correspond to multiple nodes within the same [`FragmentTree`]. Such cycles are bounded
//! by the maximum depth allowed by the tree. An example with `max_depth: 4`:
//!
//! ```text
//!           committed head
//!                  |
//! depth 0:      head_a
//!                  |
//! depth 1:      head_b
//!                  |
//! depth 2:      head_a
//!                  |
//! depth 3:      head_b
//!                  |
//! depth 4:      head_a
//! ```
//!
//! As long as the [`CandidateStorage`] has bounded input on the number of candidates supplied,
//! [`FragmentTree`] complexity is bounded. This means that higher-level code needs to be selective
//! about limiting the amount of candidates that are considered.
//!
//! The code in this module is not designed for speed or efficiency, but conceptual simplicity.
//! Our assumption is that the amount of candidates and parachains we consider will be reasonably
//! bounded and in practice will not exceed a few thousand at any time. This naive implementation
//! will still perform fairly well under these conditions, despite being somewhat wasteful of
//! memory.

use std::{
	borrow::Cow,
	collections::{hash_map::HashMap, BTreeMap, HashSet},
};

use super::LOG_TARGET;
use polkadot_node_subsystem::messages::{Ancestors, MemberState};
use polkadot_node_subsystem_util::inclusion_emulator::{
	ConstraintModifications, Constraints, Fragment, ProspectiveCandidate, RelayChainBlockInfo,
};
use polkadot_primitives::{
	BlockNumber, CandidateHash, CommittedCandidateReceipt, Hash, HeadData, Id as ParaId,
	PersistedValidationData,
};

/// Kinds of failures to import a candidate into storage.
#[derive(Debug, Clone, PartialEq)]
pub enum CandidateStorageInsertionError {
	/// An error indicating that a supplied candidate didn't match the persisted
	/// validation data provided alongside it.
	PersistedValidationDataMismatch,
	/// The candidate was already known.
	CandidateAlreadyKnown(CandidateHash),
	/// There's another candidate with this parent head hash already present. We don't accept
	/// forks.
	CandidateWithDuplicateParentHeadHash(Hash),
	/// There's another candidate with this output head hash already present. We don't accept
	/// cycles.
	CandidateWithDuplicateOutputHeadHash(Hash),
}

/// Stores candidates and information about them such as their relay-parents and their backing
/// states.
pub(crate) struct CandidateStorage {
	// Index from head data hash to candidate hashes with that head data as a parent.
	by_parent_head: HashMap<Hash, CandidateHash>,

	// Index from head data hash to candidate hashes outputting that head data.
	by_output_head: HashMap<Hash, CandidateHash>,

	// Index from candidate hash to fragment node.
	by_candidate_hash: HashMap<CandidateHash, CandidateEntry>,
}

impl CandidateStorage {
	/// Create a new `CandidateStorage`.
	pub fn new() -> Self {
		CandidateStorage {
			by_parent_head: HashMap::new(),
			by_output_head: HashMap::new(),
			by_candidate_hash: HashMap::new(),
		}
	}

	/// Introduce a new candidate.
	pub fn add_candidate(
		&mut self,
		candidate: CommittedCandidateReceipt,
		persisted_validation_data: PersistedValidationData,
		state: CandidateState,
	) -> Result<CandidateHash, CandidateStorageInsertionError> {
		let candidate_hash = candidate.hash();

		if self.by_candidate_hash.contains_key(&candidate_hash) {
			return Err(CandidateStorageInsertionError::CandidateAlreadyKnown(candidate_hash))
		}

		if persisted_validation_data.hash() != candidate.descriptor.persisted_validation_data_hash {
			return Err(CandidateStorageInsertionError::PersistedValidationDataMismatch)
		}

		let parent_head_hash = persisted_validation_data.parent_head.hash();
		let output_head_hash = candidate.commitments.head_data.hash();

		if self.by_parent_head.contains_key(&parent_head_hash) {
			return Err(CandidateStorageInsertionError::CandidateWithDuplicateParentHeadHash(
				parent_head_hash,
			));
		}

		if self.by_output_head.contains_key(&output_head_hash) {
			return Err(CandidateStorageInsertionError::CandidateWithDuplicateOutputHeadHash(
				output_head_hash,
			));
		}

		let entry = CandidateEntry {
			candidate_hash,
			relay_parent: candidate.descriptor.relay_parent,
			state,
			candidate: ProspectiveCandidate {
				commitments: Cow::Owned(candidate.commitments),
				collator: candidate.descriptor.collator,
				collator_signature: candidate.descriptor.signature,
				persisted_validation_data,
				pov_hash: candidate.descriptor.pov_hash,
				validation_code_hash: candidate.descriptor.validation_code_hash,
			},
		};

		// These have all been sanity checked already.
		self.by_parent_head.insert(parent_head_hash, candidate_hash);
		self.by_output_head.insert(output_head_hash, candidate_hash);
		self.by_candidate_hash.insert(candidate_hash, entry);

		Ok(candidate_hash)
	}

	/// Remove a candidate from the store.
	pub fn remove_candidate(&mut self, candidate_hash: &CandidateHash) {
		if let Some(entry) = self.by_candidate_hash.remove(candidate_hash) {
			let parent_head_hash = entry.candidate.persisted_validation_data.parent_head.hash();
			let output_head_hash = entry.candidate.commitments.head_data.hash();
			self.by_parent_head.remove(&parent_head_hash);
			self.by_output_head.remove(&output_head_hash);
		}
	}

	/// Note that an existing candidate has been backed.
	pub fn mark_backed(&mut self, candidate_hash: &CandidateHash) {
		if let Some(entry) = self.by_candidate_hash.get_mut(candidate_hash) {
			gum::trace!(target: LOG_TARGET, ?candidate_hash, "Candidate marked as backed");
			entry.state = CandidateState::Backed;
		} else {
			gum::trace!(target: LOG_TARGET, ?candidate_hash, "Candidate not found while marking as backed");
		}
	}

	/// Whether a candidate is recorded as being backed.
	pub fn is_backed(&self, candidate_hash: &CandidateHash) -> bool {
		self.by_candidate_hash
			.get(candidate_hash)
			.map_or(false, |e| e.state == CandidateState::Backed)
	}

	/// Whether a candidate is contained within the storage already.
	pub fn contains(&self, candidate_hash: &CandidateHash) -> bool {
		self.by_candidate_hash.contains_key(candidate_hash)
	}

	/// The number of stored candidates.
	pub fn len(&self) -> usize {
		self.by_candidate_hash.len()
	}

	/// Return an iterator over the stored candidate hashes.
	pub fn candidate_hashes(&self) -> impl Iterator<Item = &CandidateHash> {
		self.by_candidate_hash.keys()
	}

	/// Retain only candidates which pass the predicate.
	pub(crate) fn retain(&mut self, pred: impl Fn(&CandidateHash) -> bool) {
		self.by_candidate_hash.retain(|h, _v| pred(h));
		self.by_parent_head.retain(|_parent, child| pred(child));
		self.by_output_head.retain(|_output, candidate| pred(candidate));
	}

	/// Get head-data by hash.
	pub(crate) fn head_data_by_hash(&self, hash: &Hash) -> Option<&HeadData> {
		// First, search for candidates outputting this head data and extract the head data
		// from their commitments if they exist.
		//
		// Otherwise, search for candidates building upon this head data and extract the head data
		// from their persisted validation data if they exist.
		self.by_output_head
			.get(hash)
			.and_then(|a_candidate| self.by_candidate_hash.get(a_candidate))
			.map(|e| &e.candidate.commitments.head_data)
			.or_else(|| {
				self.by_parent_head
					.get(hash)
					.and_then(|a_candidate| self.by_candidate_hash.get(a_candidate))
					.map(|e| &e.candidate.persisted_validation_data.parent_head)
			})
	}

	/// Returns candidate's relay parent, if present.
	pub(crate) fn relay_parent_by_candidate_hash(
		&self,
		candidate_hash: &CandidateHash,
	) -> Option<Hash> {
		self.by_candidate_hash.get(candidate_hash).map(|entry| entry.relay_parent)
	}

	fn get_para_child<'a>(&'a self, parent_head_hash: &Hash) -> Option<&'a CandidateEntry> {
		let by_candidate_hash = &self.by_candidate_hash;
		self.by_parent_head
			.get(parent_head_hash)
			.map(move |h| by_candidate_hash.get(h))
			.flatten()
	}

	fn get(&'_ self, candidate_hash: &CandidateHash) -> Option<&'_ CandidateEntry> {
		self.by_candidate_hash.get(candidate_hash)
	}

	#[cfg(test)]
	pub fn len(&self) -> (usize, usize) {
		(self.by_parent_head.len(), self.by_candidate_hash.len())
	}
}

/// The state of a candidate.
///
/// Candidates aren't even considered until they've at least been seconded.
#[derive(Debug, PartialEq)]
pub(crate) enum CandidateState {
	/// The candidate has been seconded.
	Seconded,
	/// The candidate has been completely backed by the group.
	Backed,
}

#[derive(Debug)]
struct CandidateEntry {
	candidate_hash: CandidateHash,
	relay_parent: Hash,
	candidate: ProspectiveCandidate<'static>,
	state: CandidateState,
}

/// A candidate existing on-chain but pending availability, for special treatment
/// in the [`Scope`].
#[derive(Debug, Clone)]
pub(crate) struct PendingAvailability {
	/// The candidate hash.
	pub candidate_hash: CandidateHash,
	/// The block info of the relay parent.
	pub relay_parent: RelayChainBlockInfo,
}

/// The scope of a [`FragmentTree`].
#[derive(Debug)]
pub(crate) struct Scope {
	para: ParaId,
	relay_parent: RelayChainBlockInfo,
	ancestors: BTreeMap<BlockNumber, RelayChainBlockInfo>,
	ancestors_by_hash: HashMap<Hash, RelayChainBlockInfo>,
	pending_availability: Vec<PendingAvailability>,
	base_constraints: Constraints,
	max_depth: usize,
}

/// An error variant indicating that ancestors provided to a scope
/// had unexpected order.
#[derive(Debug)]
pub struct UnexpectedAncestor {
	/// The block number that this error occurred at.
	pub number: BlockNumber,
	/// The previous seen block number, which did not match `number`.
	pub prev: BlockNumber,
}

impl Scope {
	/// Define a new [`Scope`].
	///
	/// All arguments are straightforward except the ancestors.
	///
	/// Ancestors should be in reverse order, starting with the parent
	/// of the `relay_parent`, and proceeding backwards in block number
	/// increments of 1. Ancestors not following these conditions will be
	/// rejected.
	///
	/// This function will only consume ancestors up to the `min_relay_parent_number` of
	/// the `base_constraints`.
	///
	/// Only ancestors whose children have the same session as the relay-parent's
	/// children should be provided.
	///
	/// It is allowed to provide zero ancestors.
	pub fn with_ancestors(
		para: ParaId,
		relay_parent: RelayChainBlockInfo,
		base_constraints: Constraints,
		pending_availability: Vec<PendingAvailability>,
		max_depth: usize,
		ancestors: impl IntoIterator<Item = RelayChainBlockInfo>,
	) -> Result<Self, UnexpectedAncestor> {
		let mut ancestors_map = BTreeMap::new();
		let mut ancestors_by_hash = HashMap::new();
		{
			let mut prev = relay_parent.number;
			for ancestor in ancestors {
				if prev == 0 {
					return Err(UnexpectedAncestor { number: ancestor.number, prev })
				} else if ancestor.number != prev - 1 {
					return Err(UnexpectedAncestor { number: ancestor.number, prev })
				} else if prev == base_constraints.min_relay_parent_number {
					break
				} else {
					prev = ancestor.number;
					ancestors_by_hash.insert(ancestor.hash, ancestor.clone());
					ancestors_map.insert(ancestor.number, ancestor);
				}
			}
		}

		Ok(Scope {
			para,
			relay_parent,
			base_constraints,
			pending_availability,
			max_depth,
			ancestors: ancestors_map,
			ancestors_by_hash,
		})
	}

	/// Get the earliest relay-parent allowed in the scope of the fragment tree.
	pub fn earliest_relay_parent(&self) -> RelayChainBlockInfo {
		self.ancestors
			.iter()
			.next()
			.map(|(_, v)| v.clone())
			.unwrap_or_else(|| self.relay_parent.clone())
	}

	/// Get the ancestor of the fragment tree by hash.
	pub fn ancestor_by_hash(&self, hash: &Hash) -> Option<RelayChainBlockInfo> {
		if hash == &self.relay_parent.hash {
			return Some(self.relay_parent.clone())
		}

		self.ancestors_by_hash.get(hash).map(|info| info.clone())
	}

	/// Whether the candidate in question is one pending availability in this scope.
	pub fn get_pending_availability(
		&self,
		candidate_hash: &CandidateHash,
	) -> Option<&PendingAvailability> {
		self.pending_availability.iter().find(|c| &c.candidate_hash == candidate_hash)
	}

	/// Get the base constraints of the scope
	pub fn base_constraints(&self) -> &Constraints {
		&self.base_constraints
	}
}

/// A hypothetical candidate, which may or may not exist in
/// the fragment tree already.
pub(crate) enum HypotheticalCandidate<'a> {
	Complete {
		receipt: Cow<'a, CommittedCandidateReceipt>,
		persisted_validation_data: Cow<'a, PersistedValidationData>,
	},
	Incomplete {
		relay_parent: Hash,
		parent_head_data_hash: Hash,
	},
}

impl<'a> HypotheticalCandidate<'a> {
	fn parent_head_data_hash(&self) -> Hash {
		match *self {
			HypotheticalCandidate::Complete { ref persisted_validation_data, .. } =>
				persisted_validation_data.as_ref().parent_head.hash(),
			HypotheticalCandidate::Incomplete { ref parent_head_data_hash, .. } =>
				*parent_head_data_hash,
		}
	}

	fn relay_parent(&self) -> Hash {
		match *self {
			HypotheticalCandidate::Complete { ref receipt, .. } =>
				receipt.descriptor().relay_parent,
			HypotheticalCandidate::Incomplete { ref relay_parent, .. } => *relay_parent,
		}
	}
}

/// This is a tree of candidates based on some underlying storage of candidates and a scope.
///
/// All nodes in the tree must be either pending availability or within the scope. Within the scope
/// means it's built off of the relay-parent or an ancestor.
pub(crate) struct FragmentChain {
	scope: Scope,

	// Invariant: a contiguous prefix of the 'nodes' storage will contain
	// the top-level children.
	chain: Vec<FragmentNode>,

	candidates: HashSet<CandidateHash>,
}

impl FragmentChain {
	/// Create a new [`FragmentChain`] with given scope and populated from the storage.
	pub fn populate(scope: Scope, storage: &CandidateStorage) -> Self {
		gum::trace!(
			target: LOG_TARGET,
			relay_parent = ?scope.relay_parent.hash,
			relay_parent_num = scope.relay_parent.number,
			para_id = ?scope.para,
			ancestors = scope.ancestors.len(),
			"Instantiating Fragment Chain",
		);

		let mut fragment_chain = Self { scope, chain: Vec::new(), candidates: HashSet::new() };

		fragment_chain.populate_chain(storage);

		fragment_chain
	}

	/// Get the scope of the Fragment Tree.
	pub fn scope(&self) -> &Scope {
		&self.scope
	}

	/// Returns an O(n) iterator over the hashes of candidates contained in the
	/// tree.
	pub(crate) fn candidates(&self) -> impl Iterator<Item = CandidateHash> + '_ {
		self.candidates.iter().cloned()
	}

	/// Whether the candidate exists.
	pub(crate) fn contains_candidate(&self, candidate: &CandidateHash) -> bool {
		self.candidates.contains(candidate)
	}

	/// Add a candidate and recursively populate from storage.
	///
	/// Candidates can be added either as children of the root or children of other candidates.
	pub(crate) fn add_and_populate(&mut self, hash: CandidateHash, storage: &CandidateStorage) {
		let candidate_entry = match storage.get(&hash) {
			None => return,
			Some(e) => e,
		};

		let required_parent_head = self
			.chain
			.last()
			.map(|c| &c.fragment.candidate().commitments.head_data)
			.unwrap_or_else(|| &self.scope.base_constraints.required_parent);

		let candidate_parent = &candidate_entry.candidate.persisted_validation_data.parent_head;

		// If this builds on the latest required head, add it and populate from storage,
		// as it may connect previously disconnected candidates also.
		if required_parent_head == candidate_parent {
			self.populate_chain(storage);
		}
		// If not, there's nothing to be done
	}

	/// Returns the hypothetical depths where a candidate with the given hash and parent head data
	/// would be added to the tree, without applying other candidates recursively on top of it.
	///
	/// If the candidate is already known, this returns the actual depths where this
	/// candidate is part of the tree.
	pub(crate) fn hypothetical_depths(
		&self,
		candidate_hash: CandidateHash,
		candidate: HypotheticalCandidate,
		candidate_storage: &CandidateStorage,
	) -> MemberState {
		// pub enum MemberState {
		// 	/// Present in the candidate storage, but not connected to the prospective chain.
		// 	Unconnected,
		// 	/// Present in the fragment chain
		// 	Present,
		// 	/// Can be added to the fragment chain
		// 	Potential,
		// 	/// Not present in the candidate storage and cannot be added to the fragment chain in the
		// 	/// future.
		// 	None,
		// }
		let mut can_be_chained = false;

		// If we've already used this candidate in the chain
		if self.candidates.contains(&candidate_hash) {
			return MemberState::Present
		}

		if !self.check_potential(candidate_storage, &candidate_hash) {
			return MemberState::None
		}

		let Some(candidate_relay_parent) = self.scope.ancestor_by_hash(&candidate.relay_parent())
		else {
			// check_potential already checked for this, but just to be safe.
			return MemberState::None
		};

		let identity_modifications = ConstraintModifications::identity();
		let cumulative_modifications = if let Some(last_candidate) = self.chain.last() {
			&last_candidate.cumulative_modifications
		} else {
			&identity_modifications
		};

		let child_constraints =
			match self.scope.base_constraints.apply_modifications(&cumulative_modifications) {
				Err(e) => {
					gum::debug!(
						target: LOG_TARGET,
						new_parent_head = ?cumulative_modifications.required_parent,
						err = ?e,
						"Failed to apply modifications",
					);

					return MemberState::None
				},
				Ok(c) => c,
			};

		let parent_head_hash = candidate.parent_head_data_hash();
		if parent_head_hash == child_constraints.required_parent.hash() {
			// We do additional checks for complete candidates.
			if let HypotheticalCandidate::Complete { ref receipt, ref persisted_validation_data } =
				candidate
			{
				let prospective_candidate = ProspectiveCandidate {
					commitments: Cow::Borrowed(&receipt.commitments),
					collator: receipt.descriptor().collator.clone(),
					collator_signature: receipt.descriptor().signature.clone(),
					persisted_validation_data: persisted_validation_data.as_ref().clone(),
					pov_hash: receipt.descriptor().pov_hash,
					validation_code_hash: receipt.descriptor().validation_code_hash,
				};

				if Fragment::new(
					candidate_relay_parent.clone(),
					child_constraints,
					prospective_candidate,
				)
				.is_err()
				{
					return MemberState::None
				}
			}

			can_be_chained = true;
		}

		if can_be_chained {
			MemberState::Potential
		} else {
			// Check if this is already an unconnected candidate
			if candidate_storage.contains(&candidate_hash) {
				MemberState::Unconnected
			} else {
				MemberState::Potential
			}
		}
	}

	/// Select `count` candidates after the given `ancestors` which pass
	/// the predicate and have not already been backed on chain.
	///
	/// Does an exhaustive search into the tree after traversing the ancestors path.
	/// If the ancestors draw out a path that can be traversed in multiple ways, no
	/// candidates will be returned.
	/// If the ancestors do not draw out a full path (the path contains holes), candidates will be
	/// suggested that may fill these holes.
	/// If the ancestors don't draw out a valid path, no candidates will be returned. If there are
	/// multiple possibilities of the same size, this will select the first one. If there is no
	/// chain of size `count` that matches the criteria, this will return the largest chain it could
	/// find with the criteria. If there are no candidates meeting those criteria, returns an empty
	/// `Vec`.
	/// Cycles are accepted, but this code expects that the runtime will deduplicate
	/// identical candidates when occupying the cores (when proposing to back A->B->A, only A will
	/// be backed on chain).
	///
	/// The intention of the `ancestors` is to allow queries on the basis of
	/// one or more candidates which were previously pending availability becoming
	/// available or candidates timing out.
	pub(crate) fn find_backable_chain(
		&self,
		ancestors: Ancestors,
		count: u32,
		pred: impl Fn(&CandidateHash) -> bool,
	) -> Vec<CandidateHash> {
		if count == 0 {
			return vec![]
		}
		let base_pos = self.find_ancestor_path(ancestors);

		let actual_end_index = std::cmp::min(base_pos + (count as usize), self.chain.len());
		let mut res = Vec::with_capacity(actual_end_index - base_pos);

		for elem in &self.chain[base_pos..actual_end_index] {
			if self.scope.get_pending_availability(&elem.candidate_hash).is_none() &&
				pred(&elem.candidate_hash)
			{
				res.push(elem.candidate_hash);
			} else {
				break
			}
		}

		res
	}

	// Orders the ancestors into a viable path from root to the last one.
	// Returns a pointer to the last node in the path.
	// We assume that the ancestors form a chain (that the
	// av-cores do not back parachain forks), None is returned otherwise.
	// If we cannot use all ancestors, stop at the first found hole in the chain. This usually
	// translates to a timed out candidate.
	fn find_ancestor_path(&self, mut ancestors: Ancestors) -> usize {
		for (index, candidate) in self.chain.iter().enumerate() {
			if !ancestors.remove(&candidate.candidate_hash) {
				return index
			}
		}

		0
	}

	pub fn check_potential(
		&self,
		storage: &CandidateStorage,
		candidate_hash: &CandidateHash,
	) -> bool {
		const MAX_UNCONNECTED: usize = 5;
		let Some(candidate) = storage.get(candidate_hash) else { return false };

		let Some(unconnected_candidates) = storage.len().checked_sub(self.candidates.len()) else {
			return false
		};

		// If we've got enough candidates for the configured depth.
		if self.chain.len() + 1 > self.scope.max_depth {
			return false
		}

		if unconnected_candidates >= MAX_UNCONNECTED {
			return false
		}

		let earliest_rp = if let Some(last_candidate) = self.chain.last() {
			self.scope
				.ancestor_by_hash(&last_candidate.relay_parent())
				.or_else(|| {
					// if the relay-parent is out of scope _and_ it is in the tree,
					// it must be a candidate pending availability.
					self.scope
						.get_pending_availability(&last_candidate.candidate_hash)
						.map(|c| c.relay_parent.clone())
				})
				.expect("All nodes in tree are either pending availability or within scope; qed")
		} else {
			self.scope.earliest_relay_parent()
		};

		let Some(relay_parent) = self.scope.ancestor_by_hash(&candidate.relay_parent) else {
			return false
		};

		let min_relay_parent_number =
			std::cmp::max(earliest_rp.number, self.scope.earliest_relay_parent().number);
		if relay_parent.number < min_relay_parent_number {
			return false // relay parent moved backwards.
		}

		true
	}

	fn populate_chain(&mut self, storage: &CandidateStorage) {
		let mut cumulative_modifications = if let Some(last_candidate) = self.chain.last() {
			last_candidate.cumulative_modifications.clone()
		} else {
			ConstraintModifications::identity()
		};
		let mut earliest_rp = if let Some(last_candidate) = self.chain.last() {
			self.scope
				.ancestor_by_hash(&last_candidate.relay_parent())
				.or_else(|| {
					// if the relay-parent is out of scope _and_ it is in the tree,
					// it must be a candidate pending availability.
					self.scope
						.get_pending_availability(&last_candidate.candidate_hash)
						.map(|c| c.relay_parent.clone())
				})
				.expect("All nodes in tree are either pending availability or within scope; qed")
		} else {
			self.scope.earliest_relay_parent()
		};

		loop {
			if self.chain.len() > self.scope.max_depth {
				break;
			}

			let child_constraints =
				match self.scope.base_constraints.apply_modifications(&cumulative_modifications) {
					Err(e) => {
						gum::debug!(
							target: LOG_TARGET,
							new_parent_head = ?cumulative_modifications.required_parent,
							err = ?e,
							"Failed to apply modifications",
						);

						break
					},
					Ok(c) => c,
				};

			let required_head_hash = child_constraints.required_parent.hash();
			let possible_child = storage.get_para_child(&required_head_hash);

			if let Some(candidate) = possible_child {
				// Add one node to chain if
				// 1. parent hash is correct
				// 2. relay-parent does not move backwards.
				// 3. all non-pending-availability candidates have relay-parent in scope.
				// 4. candidate outputs fulfill constraints

				let pending = self.scope.get_pending_availability(&candidate.candidate_hash);
				let Some(relay_parent) = pending
					.map(|p| p.relay_parent.clone())
					.or_else(|| self.scope.ancestor_by_hash(&candidate.relay_parent))
				else {
					break
				};

				// require: pending availability candidates don't move backwards
				// and only those can be out-of-scope.
				//
				// earliest_rp can be before the earliest relay parent in the scope
				// when the parent is a pending availability candidate as well, but
				// only other pending candidates can have a relay parent out of scope.
				let min_relay_parent_number = pending
					.map(|p| match self.chain.len() {
						0 => p.relay_parent.number,
						_ => earliest_rp.number,
					})
					.unwrap_or_else(|| {
						std::cmp::max(earliest_rp.number, self.scope.earliest_relay_parent().number)
					});

				if relay_parent.number < min_relay_parent_number {
					break // relay parent moved backwards.
				}

				// don't add candidates if they're already present in the chain.
				// this can never happen, as candidates can only be duplicated if there's a cycle.
				// and CandidateStorage does not allow cycles.
				if self.contains_candidate(&candidate.candidate_hash) {
					break
				}

				let fragment = {
					let mut constraints = child_constraints.clone();
					if let Some(ref p) = pending {
						// overwrite for candidates pending availability as a special-case.
						constraints.min_relay_parent_number = p.relay_parent.number;
					}

					let f = Fragment::new(
						relay_parent.clone(),
						constraints,
						candidate.candidate.partial_clone(),
					);

					match f {
						Ok(f) => f.into_owned(),
						Err(e) => {
							gum::debug!(
								target: LOG_TARGET,
								err = ?e,
								?relay_parent,
								candidate_hash = ?candidate.candidate_hash,
								"Failed to instantiate fragment",
							);

							break
						},
					}
				};

				// Update the cumulative constraint modifications.
				cumulative_modifications.stack(fragment.constraint_modifications());
				// Update the earliest rp
				earliest_rp = relay_parent;

				let node = FragmentNode {
					fragment,
					candidate_hash: candidate.candidate_hash,
					cumulative_modifications: cumulative_modifications.clone(),
				};

				self.chain.push(node);
				self.candidates.insert(candidate.candidate_hash);
			} else {
				break;
			}
		}
	}
}

struct FragmentNode {
	fragment: Fragment<'static>,
	candidate_hash: CandidateHash,
	cumulative_modifications: ConstraintModifications,
}

impl FragmentNode {
	fn relay_parent(&self) -> Hash {
		self.fragment.relay_parent().hash
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use assert_matches::assert_matches;
	use polkadot_node_subsystem_util::inclusion_emulator::InboundHrmpLimitations;
	use polkadot_primitives::{BlockNumber, CandidateCommitments, CandidateDescriptor, HeadData};
	use polkadot_primitives_test_helpers as test_helpers;
	use rstest::rstest;
	use std::iter;

	impl NodePointer {
		fn unwrap_idx(self) -> usize {
			match self {
				NodePointer::Root => panic!("Unexpected root"),
				NodePointer::Storage(index) => index,
			}
		}
	}

	fn make_constraints(
		min_relay_parent_number: BlockNumber,
		valid_watermarks: Vec<BlockNumber>,
		required_parent: HeadData,
	) -> Constraints {
		Constraints {
			min_relay_parent_number,
			max_pov_size: 1_000_000,
			max_code_size: 1_000_000,
			ump_remaining: 10,
			ump_remaining_bytes: 1_000,
			max_ump_num_per_candidate: 10,
			dmp_remaining_messages: [0; 10].into(),
			hrmp_inbound: InboundHrmpLimitations { valid_watermarks },
			hrmp_channels_out: HashMap::new(),
			max_hrmp_num_per_candidate: 0,
			required_parent,
			validation_code_hash: Hash::repeat_byte(42).into(),
			upgrade_restriction: None,
			future_validation_code: None,
		}
	}

	fn make_committed_candidate(
		para_id: ParaId,
		relay_parent: Hash,
		relay_parent_number: BlockNumber,
		parent_head: HeadData,
		para_head: HeadData,
		hrmp_watermark: BlockNumber,
	) -> (PersistedValidationData, CommittedCandidateReceipt) {
		let persisted_validation_data = PersistedValidationData {
			parent_head,
			relay_parent_number,
			relay_parent_storage_root: Hash::repeat_byte(69),
			max_pov_size: 1_000_000,
		};

		let candidate = CommittedCandidateReceipt {
			descriptor: CandidateDescriptor {
				para_id,
				relay_parent,
				collator: test_helpers::dummy_collator(),
				persisted_validation_data_hash: persisted_validation_data.hash(),
				pov_hash: Hash::repeat_byte(1),
				erasure_root: Hash::repeat_byte(1),
				signature: test_helpers::dummy_collator_signature(),
				para_head: para_head.hash(),
				validation_code_hash: Hash::repeat_byte(42).into(),
			},
			commitments: CandidateCommitments {
				upward_messages: Default::default(),
				horizontal_messages: Default::default(),
				new_validation_code: None,
				head_data: para_head,
				processed_downward_messages: 1,
				hrmp_watermark,
			},
		};

		(persisted_validation_data, candidate)
	}

	#[test]
	fn scope_rejects_ancestors_that_skip_blocks() {
		let para_id = ParaId::from(5u32);
		let relay_parent = RelayChainBlockInfo {
			number: 10,
			hash: Hash::repeat_byte(10),
			storage_root: Hash::repeat_byte(69),
		};

		let ancestors = vec![RelayChainBlockInfo {
			number: 8,
			hash: Hash::repeat_byte(8),
			storage_root: Hash::repeat_byte(69),
		}];

		let max_depth = 2;
		let base_constraints = make_constraints(8, vec![8, 9], vec![1, 2, 3].into());
		let pending_availability = Vec::new();

		assert_matches!(
			Scope::with_ancestors(
				para_id,
				relay_parent,
				base_constraints,
				pending_availability,
				max_depth,
				ancestors
			),
			Err(UnexpectedAncestor { number: 8, prev: 10 })
		);
	}

	#[test]
	fn scope_rejects_ancestor_for_0_block() {
		let para_id = ParaId::from(5u32);
		let relay_parent = RelayChainBlockInfo {
			number: 0,
			hash: Hash::repeat_byte(0),
			storage_root: Hash::repeat_byte(69),
		};

		let ancestors = vec![RelayChainBlockInfo {
			number: 99999,
			hash: Hash::repeat_byte(99),
			storage_root: Hash::repeat_byte(69),
		}];

		let max_depth = 2;
		let base_constraints = make_constraints(0, vec![], vec![1, 2, 3].into());
		let pending_availability = Vec::new();

		assert_matches!(
			Scope::with_ancestors(
				para_id,
				relay_parent,
				base_constraints,
				pending_availability,
				max_depth,
				ancestors,
			),
			Err(UnexpectedAncestor { number: 99999, prev: 0 })
		);
	}

	#[test]
	fn scope_only_takes_ancestors_up_to_min() {
		let para_id = ParaId::from(5u32);
		let relay_parent = RelayChainBlockInfo {
			number: 5,
			hash: Hash::repeat_byte(0),
			storage_root: Hash::repeat_byte(69),
		};

		let ancestors = vec![
			RelayChainBlockInfo {
				number: 4,
				hash: Hash::repeat_byte(4),
				storage_root: Hash::repeat_byte(69),
			},
			RelayChainBlockInfo {
				number: 3,
				hash: Hash::repeat_byte(3),
				storage_root: Hash::repeat_byte(69),
			},
			RelayChainBlockInfo {
				number: 2,
				hash: Hash::repeat_byte(2),
				storage_root: Hash::repeat_byte(69),
			},
		];

		let max_depth = 2;
		let base_constraints = make_constraints(3, vec![2], vec![1, 2, 3].into());
		let pending_availability = Vec::new();

		let scope = Scope::with_ancestors(
			para_id,
			relay_parent,
			base_constraints,
			pending_availability,
			max_depth,
			ancestors,
		)
		.unwrap();

		assert_eq!(scope.ancestors.len(), 2);
		assert_eq!(scope.ancestors_by_hash.len(), 2);
	}

	#[test]
	fn storage_add_candidate() {
		let mut storage = CandidateStorage::new();
		let relay_parent = Hash::repeat_byte(69);

		let (pvd, candidate) = make_committed_candidate(
			ParaId::from(5u32),
			relay_parent,
			8,
			vec![4, 5, 6].into(),
			vec![1, 2, 3].into(),
			7,
		);

		let candidate_hash = candidate.hash();
		let parent_head_hash = pvd.parent_head.hash();

		storage.add_candidate(candidate, pvd).unwrap();
		assert!(storage.contains(&candidate_hash));
		assert_eq!(storage.iter_para_children(&parent_head_hash).count(), 1);

		assert_eq!(storage.relay_parent_by_candidate_hash(&candidate_hash), Some(relay_parent));
	}

	#[test]
	fn storage_retain() {
		let mut storage = CandidateStorage::new();

		let (pvd, candidate) = make_committed_candidate(
			ParaId::from(5u32),
			Hash::repeat_byte(69),
			8,
			vec![4, 5, 6].into(),
			vec![1, 2, 3].into(),
			7,
		);

		let candidate_hash = candidate.hash();
		let output_head_hash = candidate.commitments.head_data.hash();
		let parent_head_hash = pvd.parent_head.hash();

		storage.add_candidate(candidate, pvd).unwrap();
		storage.retain(|_| true);
		assert!(storage.contains(&candidate_hash));
		assert_eq!(storage.iter_para_children(&parent_head_hash).count(), 1);
		assert!(storage.head_data_by_hash(&output_head_hash).is_some());

		storage.retain(|_| false);
		assert!(!storage.contains(&candidate_hash));
		assert_eq!(storage.iter_para_children(&parent_head_hash).count(), 0);
		assert!(storage.head_data_by_hash(&output_head_hash).is_none());
	}

	// [`FragmentTree::populate`] should pick up candidates that build on other candidates.
	#[test]
	fn populate_works_recursively() {
		let mut storage = CandidateStorage::new();

		let para_id = ParaId::from(5u32);
		let relay_parent_a = Hash::repeat_byte(1);
		let relay_parent_b = Hash::repeat_byte(2);

		let (pvd_a, candidate_a) = make_committed_candidate(
			para_id,
			relay_parent_a,
			0,
			vec![0x0a].into(),
			vec![0x0b].into(),
			0,
		);
		let candidate_a_hash = candidate_a.hash();

		let (pvd_b, candidate_b) = make_committed_candidate(
			para_id,
			relay_parent_b,
			1,
			vec![0x0b].into(),
			vec![0x0c].into(),
			1,
		);
		let candidate_b_hash = candidate_b.hash();

		let base_constraints = make_constraints(0, vec![0], vec![0x0a].into());
		let pending_availability = Vec::new();

		let ancestors = vec![RelayChainBlockInfo {
			number: pvd_a.relay_parent_number,
			hash: relay_parent_a,
			storage_root: pvd_a.relay_parent_storage_root,
		}];

		let relay_parent_b_info = RelayChainBlockInfo {
			number: pvd_b.relay_parent_number,
			hash: relay_parent_b,
			storage_root: pvd_b.relay_parent_storage_root,
		};

		storage.add_candidate(candidate_a, pvd_a).unwrap();
		storage.add_candidate(candidate_b, pvd_b).unwrap();
		let scope = Scope::with_ancestors(
			para_id,
			relay_parent_b_info,
			base_constraints,
			pending_availability,
			4,
			ancestors,
		)
		.unwrap();
		let tree = FragmentTree::populate(scope, &storage);

		let candidates: Vec<_> = tree.candidates().collect();
		assert_eq!(candidates.len(), 2);
		assert!(candidates.contains(&candidate_a_hash));
		assert!(candidates.contains(&candidate_b_hash));

		assert_eq!(tree.nodes.len(), 2);
		assert_eq!(tree.nodes[0].parent, NodePointer::Root);
		assert_eq!(tree.nodes[0].candidate_hash, candidate_a_hash);
		assert_eq!(tree.nodes[0].depth, 0);

		assert_eq!(tree.nodes[1].parent, NodePointer::Storage(0));
		assert_eq!(tree.nodes[1].candidate_hash, candidate_b_hash);
		assert_eq!(tree.nodes[1].depth, 1);
	}

	#[test]
	fn children_of_root_are_contiguous() {
		let mut storage = CandidateStorage::new();

		let para_id = ParaId::from(5u32);
		let relay_parent_a = Hash::repeat_byte(1);
		let relay_parent_b = Hash::repeat_byte(2);

		let (pvd_a, candidate_a) = make_committed_candidate(
			para_id,
			relay_parent_a,
			0,
			vec![0x0a].into(),
			vec![0x0b].into(),
			0,
		);

		let (pvd_b, candidate_b) = make_committed_candidate(
			para_id,
			relay_parent_b,
			1,
			vec![0x0b].into(),
			vec![0x0c].into(),
			1,
		);

		let (pvd_a2, candidate_a2) = make_committed_candidate(
			para_id,
			relay_parent_a,
			0,
			vec![0x0a].into(),
			vec![0x0b, 1].into(),
			0,
		);
		let candidate_a2_hash = candidate_a2.hash();

		let base_constraints = make_constraints(0, vec![0], vec![0x0a].into());
		let pending_availability = Vec::new();

		let ancestors = vec![RelayChainBlockInfo {
			number: pvd_a.relay_parent_number,
			hash: relay_parent_a,
			storage_root: pvd_a.relay_parent_storage_root,
		}];

		let relay_parent_b_info = RelayChainBlockInfo {
			number: pvd_b.relay_parent_number,
			hash: relay_parent_b,
			storage_root: pvd_b.relay_parent_storage_root,
		};

		storage.add_candidate(candidate_a, pvd_a).unwrap();
		storage.add_candidate(candidate_b, pvd_b).unwrap();
		let scope = Scope::with_ancestors(
			para_id,
			relay_parent_b_info,
			base_constraints,
			pending_availability,
			4,
			ancestors,
		)
		.unwrap();
		let mut tree = FragmentTree::populate(scope, &storage);

		storage.add_candidate(candidate_a2, pvd_a2).unwrap();
		tree.add_and_populate(candidate_a2_hash, &storage);
		let candidates: Vec<_> = tree.candidates().collect();
		assert_eq!(candidates.len(), 3);

		assert_eq!(tree.nodes[0].parent, NodePointer::Root);
		assert_eq!(tree.nodes[1].parent, NodePointer::Root);
		assert_eq!(tree.nodes[2].parent, NodePointer::Storage(0));
	}

	#[test]
	fn add_candidate_child_of_root() {
		let mut storage = CandidateStorage::new();

		let para_id = ParaId::from(5u32);
		let relay_parent_a = Hash::repeat_byte(1);

		let (pvd_a, candidate_a) = make_committed_candidate(
			para_id,
			relay_parent_a,
			0,
			vec![0x0a].into(),
			vec![0x0b].into(),
			0,
		);

		let (pvd_b, candidate_b) = make_committed_candidate(
			para_id,
			relay_parent_a,
			0,
			vec![0x0a].into(),
			vec![0x0c].into(),
			0,
		);
		let candidate_b_hash = candidate_b.hash();

		let base_constraints = make_constraints(0, vec![0], vec![0x0a].into());
		let pending_availability = Vec::new();

		let relay_parent_a_info = RelayChainBlockInfo {
			number: pvd_a.relay_parent_number,
			hash: relay_parent_a,
			storage_root: pvd_a.relay_parent_storage_root,
		};

		storage.add_candidate(candidate_a, pvd_a).unwrap();
		let scope = Scope::with_ancestors(
			para_id,
			relay_parent_a_info,
			base_constraints,
			pending_availability,
			4,
			vec![],
		)
		.unwrap();
		let mut tree = FragmentTree::populate(scope, &storage);

		storage.add_candidate(candidate_b, pvd_b).unwrap();
		tree.add_and_populate(candidate_b_hash, &storage);
		let candidates: Vec<_> = tree.candidates().collect();
		assert_eq!(candidates.len(), 2);

		assert_eq!(tree.nodes[0].parent, NodePointer::Root);
		assert_eq!(tree.nodes[1].parent, NodePointer::Root);
	}

	#[test]
	fn add_candidate_child_of_non_root() {
		let mut storage = CandidateStorage::new();

		let para_id = ParaId::from(5u32);
		let relay_parent_a = Hash::repeat_byte(1);

		let (pvd_a, candidate_a) = make_committed_candidate(
			para_id,
			relay_parent_a,
			0,
			vec![0x0a].into(),
			vec![0x0b].into(),
			0,
		);

		let (pvd_b, candidate_b) = make_committed_candidate(
			para_id,
			relay_parent_a,
			0,
			vec![0x0b].into(),
			vec![0x0c].into(),
			0,
		);
		let candidate_b_hash = candidate_b.hash();

		let base_constraints = make_constraints(0, vec![0], vec![0x0a].into());
		let pending_availability = Vec::new();

		let relay_parent_a_info = RelayChainBlockInfo {
			number: pvd_a.relay_parent_number,
			hash: relay_parent_a,
			storage_root: pvd_a.relay_parent_storage_root,
		};

		storage.add_candidate(candidate_a, pvd_a).unwrap();
		let scope = Scope::with_ancestors(
			para_id,
			relay_parent_a_info,
			base_constraints,
			pending_availability,
			4,
			vec![],
		)
		.unwrap();
		let mut tree = FragmentTree::populate(scope, &storage);

		storage.add_candidate(candidate_b, pvd_b).unwrap();
		tree.add_and_populate(candidate_b_hash, &storage);
		let candidates: Vec<_> = tree.candidates().collect();
		assert_eq!(candidates.len(), 2);

		assert_eq!(tree.nodes[0].parent, NodePointer::Root);
		assert_eq!(tree.nodes[1].parent, NodePointer::Storage(0));
	}

	#[test]
	fn test_find_ancestor_path_and_find_backable_chain_empty_tree() {
		let para_id = ParaId::from(5u32);
		let relay_parent = Hash::repeat_byte(1);
		let required_parent: HeadData = vec![0xff].into();
		let max_depth = 10;

		// Empty tree
		let storage = CandidateStorage::new();
		let base_constraints = make_constraints(0, vec![0], required_parent.clone());

		let relay_parent_info =
			RelayChainBlockInfo { number: 0, hash: relay_parent, storage_root: Hash::zero() };

		let scope = Scope::with_ancestors(
			para_id,
			relay_parent_info,
			base_constraints,
			vec![],
			max_depth,
			vec![],
		)
		.unwrap();
		let tree = FragmentTree::populate(scope, &storage);
		assert_eq!(tree.candidates().collect::<Vec<_>>().len(), 0);
		assert_eq!(tree.nodes.len(), 0);

		assert_eq!(tree.find_ancestor_path(Ancestors::new()).unwrap(), NodePointer::Root);
		assert_eq!(tree.find_backable_chain(Ancestors::new(), 2, |_| true), vec![]);
		// Invalid candidate.
		let ancestors: Ancestors = [CandidateHash::default()].into_iter().collect();
		assert_eq!(tree.find_ancestor_path(ancestors.clone()), Some(NodePointer::Root));
		assert_eq!(tree.find_backable_chain(ancestors, 2, |_| true), vec![]);
	}

	#[rstest]
	#[case(true, 13)]
	#[case(false, 8)]
	// The tree with no cycles looks like:
	// Make a tree that looks like this (note that there's no cycle):
	//         +-(root)-+
	//         |        |
	//    +----0---+    7
	//    |        |
	//    1----+   5
	//    |    |
	//    |    |
	//    2    6
	//    |
	//    3
	//    |
	//    4
	//
	// The tree with cycles is the same as the first but has a cycle from 4 back to the state
	// produced by 0 (It's bounded by the max_depth + 1).
	//         +-(root)-+
	//         |        |
	//    +----0---+    7
	//    |        |
	//    1----+   5
	//    |    |
	//    |    |
	//    2    6
	//    |
	//    3
	//    |
	//    4---+
	//    |   |
	//    1   5
	//    |
	//    2
	//    |
	//    3
	fn test_find_ancestor_path_and_find_backable_chain(
		#[case] has_cycle: bool,
		#[case] expected_node_count: usize,
	) {
		let para_id = ParaId::from(5u32);
		let relay_parent = Hash::repeat_byte(1);
		let required_parent: HeadData = vec![0xff].into();
		let max_depth = 7;
		let relay_parent_number = 0;
		let relay_parent_storage_root = Hash::repeat_byte(69);

		let mut candidates = vec![];

		// Candidate 0
		candidates.push(make_committed_candidate(
			para_id,
			relay_parent,
			0,
			required_parent.clone(),
			vec![0].into(),
			0,
		));
		// Candidate 1
		candidates.push(make_committed_candidate(
			para_id,
			relay_parent,
			0,
			vec![0].into(),
			vec![1].into(),
			0,
		));
		// Candidate 2
		candidates.push(make_committed_candidate(
			para_id,
			relay_parent,
			0,
			vec![1].into(),
			vec![2].into(),
			0,
		));
		// Candidate 3
		candidates.push(make_committed_candidate(
			para_id,
			relay_parent,
			0,
			vec![2].into(),
			vec![3].into(),
			0,
		));
		// Candidate 4
		candidates.push(make_committed_candidate(
			para_id,
			relay_parent,
			0,
			vec![3].into(),
			vec![4].into(),
			0,
		));
		// Candidate 5
		candidates.push(make_committed_candidate(
			para_id,
			relay_parent,
			0,
			vec![0].into(),
			vec![5].into(),
			0,
		));
		// Candidate 6
		candidates.push(make_committed_candidate(
			para_id,
			relay_parent,
			0,
			vec![1].into(),
			vec![6].into(),
			0,
		));
		// Candidate 7
		candidates.push(make_committed_candidate(
			para_id,
			relay_parent,
			0,
			required_parent.clone(),
			vec![7].into(),
			0,
		));

		if has_cycle {
			candidates[4] = make_committed_candidate(
				para_id,
				relay_parent,
				0,
				vec![3].into(),
				vec![0].into(), // put the cycle here back to the output state of 0.
				0,
			);
		}

		let base_constraints = make_constraints(0, vec![0], required_parent.clone());
		let mut storage = CandidateStorage::new();

		let relay_parent_info = RelayChainBlockInfo {
			number: relay_parent_number,
			hash: relay_parent,
			storage_root: relay_parent_storage_root,
		};

		for (pvd, candidate) in candidates.iter() {
			storage.add_candidate(candidate.clone(), pvd.clone()).unwrap();
		}
		let candidates =
			candidates.into_iter().map(|(_pvd, candidate)| candidate).collect::<Vec<_>>();
		let scope = Scope::with_ancestors(
			para_id,
			relay_parent_info,
			base_constraints,
			vec![],
			max_depth,
			vec![],
		)
		.unwrap();
		let tree = FragmentTree::populate(scope, &storage);

		assert_eq!(tree.candidates().collect::<Vec<_>>().len(), candidates.len());
		assert_eq!(tree.nodes.len(), expected_node_count);

		// Do some common tests on both trees.
		{
			// No ancestors supplied.
			assert_eq!(tree.find_ancestor_path(Ancestors::new()).unwrap(), NodePointer::Root);
			assert_eq!(
				tree.find_backable_chain(Ancestors::new(), 4, |_| true),
				[0, 1, 2, 3].into_iter().map(|i| candidates[i].hash()).collect::<Vec<_>>()
			);
			// Ancestor which is not part of the tree. Will be ignored.
			let ancestors: Ancestors = [CandidateHash::default()].into_iter().collect();
			assert_eq!(tree.find_ancestor_path(ancestors.clone()).unwrap(), NodePointer::Root);
			assert_eq!(
				tree.find_backable_chain(ancestors, 4, |_| true),
				[0, 1, 2, 3].into_iter().map(|i| candidates[i].hash()).collect::<Vec<_>>()
			);
			// A chain fork.
			let ancestors: Ancestors =
				[(candidates[0].hash()), (candidates[7].hash())].into_iter().collect();
			assert_eq!(tree.find_ancestor_path(ancestors.clone()), None);
			assert_eq!(tree.find_backable_chain(ancestors, 1, |_| true), vec![]);

			// Ancestors which are part of the tree but don't form a path. Will be ignored.
			let ancestors: Ancestors =
				[candidates[1].hash(), candidates[2].hash()].into_iter().collect();
			assert_eq!(tree.find_ancestor_path(ancestors.clone()).unwrap(), NodePointer::Root);
			assert_eq!(
				tree.find_backable_chain(ancestors, 4, |_| true),
				[0, 1, 2, 3].into_iter().map(|i| candidates[i].hash()).collect::<Vec<_>>()
			);

			// Valid ancestors.
			let ancestors: Ancestors = [candidates[7].hash()].into_iter().collect();
			let res = tree.find_ancestor_path(ancestors.clone()).unwrap();
			let candidate = &tree.nodes[res.unwrap_idx()];
			assert_eq!(candidate.candidate_hash, candidates[7].hash());
			assert_eq!(tree.find_backable_chain(ancestors, 1, |_| true), vec![]);

			let ancestors: Ancestors =
				[candidates[2].hash(), candidates[0].hash(), candidates[1].hash()]
					.into_iter()
					.collect();
			let res = tree.find_ancestor_path(ancestors.clone()).unwrap();
			let candidate = &tree.nodes[res.unwrap_idx()];
			assert_eq!(candidate.candidate_hash, candidates[2].hash());
			assert_eq!(
				tree.find_backable_chain(ancestors.clone(), 2, |_| true),
				[3, 4].into_iter().map(|i| candidates[i].hash()).collect::<Vec<_>>()
			);

			// Valid ancestors with candidates which have been omitted due to timeouts
			let ancestors: Ancestors =
				[candidates[0].hash(), candidates[2].hash()].into_iter().collect();
			let res = tree.find_ancestor_path(ancestors.clone()).unwrap();
			let candidate = &tree.nodes[res.unwrap_idx()];
			assert_eq!(candidate.candidate_hash, candidates[0].hash());
			assert_eq!(
				tree.find_backable_chain(ancestors, 3, |_| true),
				[1, 2, 3].into_iter().map(|i| candidates[i].hash()).collect::<Vec<_>>()
			);

			let ancestors: Ancestors =
				[candidates[0].hash(), candidates[1].hash(), candidates[3].hash()]
					.into_iter()
					.collect();
			let res = tree.find_ancestor_path(ancestors.clone()).unwrap();
			let candidate = &tree.nodes[res.unwrap_idx()];
			assert_eq!(candidate.candidate_hash, candidates[1].hash());
			if has_cycle {
				assert_eq!(
					tree.find_backable_chain(ancestors, 2, |_| true),
					[2, 3].into_iter().map(|i| candidates[i].hash()).collect::<Vec<_>>()
				);
			} else {
				assert_eq!(
					tree.find_backable_chain(ancestors, 4, |_| true),
					[2, 3, 4].into_iter().map(|i| candidates[i].hash()).collect::<Vec<_>>()
				);
			}

			let ancestors: Ancestors =
				[candidates[1].hash(), candidates[2].hash()].into_iter().collect();
			let res = tree.find_ancestor_path(ancestors.clone()).unwrap();
			assert_eq!(res, NodePointer::Root);
			assert_eq!(
				tree.find_backable_chain(ancestors, 4, |_| true),
				[0, 1, 2, 3].into_iter().map(|i| candidates[i].hash()).collect::<Vec<_>>()
			);

			// Requested count is 0.
			assert_eq!(tree.find_backable_chain(Ancestors::new(), 0, |_| true), vec![]);

			let ancestors: Ancestors =
				[candidates[2].hash(), candidates[0].hash(), candidates[1].hash()]
					.into_iter()
					.collect();
			assert_eq!(tree.find_backable_chain(ancestors, 0, |_| true), vec![]);

			let ancestors: Ancestors =
				[candidates[2].hash(), candidates[0].hash()].into_iter().collect();
			assert_eq!(tree.find_backable_chain(ancestors, 0, |_| true), vec![]);
		}

		// Now do some tests only on the tree with cycles
		if has_cycle {
			// Exceeds the maximum tree depth. 0-1-2-3-4-1-2-3-4, when the tree stops at
			// 0-1-2-3-4-1-2-3.
			let ancestors: Ancestors = [
				candidates[0].hash(),
				candidates[1].hash(),
				candidates[2].hash(),
				candidates[3].hash(),
				candidates[4].hash(),
			]
			.into_iter()
			.collect();
			let res = tree.find_ancestor_path(ancestors.clone()).unwrap();
			let candidate = &tree.nodes[res.unwrap_idx()];
			assert_eq!(candidate.candidate_hash, candidates[4].hash());
			assert_eq!(
				tree.find_backable_chain(ancestors, 4, |_| true),
				[1, 2, 3].into_iter().map(|i| candidates[i].hash()).collect::<Vec<_>>()
			);

			// 0-1-2.
			let ancestors: Ancestors =
				[candidates[0].hash(), candidates[1].hash(), candidates[2].hash()]
					.into_iter()
					.collect();
			let res = tree.find_ancestor_path(ancestors.clone()).unwrap();
			let candidate = &tree.nodes[res.unwrap_idx()];
			assert_eq!(candidate.candidate_hash, candidates[2].hash());
			assert_eq!(
				tree.find_backable_chain(ancestors.clone(), 1, |_| true),
				[3].into_iter().map(|i| candidates[i].hash()).collect::<Vec<_>>()
			);
			assert_eq!(
				tree.find_backable_chain(ancestors, 5, |_| true),
				[3, 4, 1, 2, 3].into_iter().map(|i| candidates[i].hash()).collect::<Vec<_>>()
			);

			// 0-1
			let ancestors: Ancestors =
				[candidates[0].hash(), candidates[1].hash()].into_iter().collect();
			let res = tree.find_ancestor_path(ancestors.clone()).unwrap();
			let candidate = &tree.nodes[res.unwrap_idx()];
			assert_eq!(candidate.candidate_hash, candidates[1].hash());
			assert_eq!(
				tree.find_backable_chain(ancestors, 6, |_| true),
				[2, 3, 4, 1, 2, 3].into_iter().map(|i| candidates[i].hash()).collect::<Vec<_>>(),
			);

			// For 0-1-2-3-4-5, there's more than 1 way of finding this path in
			// the tree. `None` should be returned. The runtime should not have accepted this.
			let ancestors: Ancestors = [
				candidates[0].hash(),
				candidates[1].hash(),
				candidates[2].hash(),
				candidates[3].hash(),
				candidates[4].hash(),
				candidates[5].hash(),
			]
			.into_iter()
			.collect();
			let res = tree.find_ancestor_path(ancestors.clone());
			assert_eq!(res, None);
			assert_eq!(tree.find_backable_chain(ancestors, 1, |_| true), vec![]);
		}
	}

	#[test]
	fn graceful_cycle_of_0() {
		let mut storage = CandidateStorage::new();

		let para_id = ParaId::from(5u32);
		let relay_parent_a = Hash::repeat_byte(1);

		let (pvd_a, candidate_a) = make_committed_candidate(
			para_id,
			relay_parent_a,
			0,
			vec![0x0a].into(),
			vec![0x0a].into(), // input same as output
			0,
		);
		let candidate_a_hash = candidate_a.hash();
		let base_constraints = make_constraints(0, vec![0], vec![0x0a].into());
		let pending_availability = Vec::new();

		let relay_parent_a_info = RelayChainBlockInfo {
			number: pvd_a.relay_parent_number,
			hash: relay_parent_a,
			storage_root: pvd_a.relay_parent_storage_root,
		};

		let max_depth = 4;
		storage.add_candidate(candidate_a, pvd_a).unwrap();
		let scope = Scope::with_ancestors(
			para_id,
			relay_parent_a_info,
			base_constraints,
			pending_availability,
			max_depth,
			vec![],
		)
		.unwrap();
		let tree = FragmentTree::populate(scope, &storage);

		let candidates: Vec<_> = tree.candidates().collect();
		assert_eq!(candidates.len(), 1);
		assert_eq!(tree.nodes.len(), max_depth + 1);

		assert_eq!(tree.nodes[0].parent, NodePointer::Root);
		assert_eq!(tree.nodes[1].parent, NodePointer::Storage(0));
		assert_eq!(tree.nodes[2].parent, NodePointer::Storage(1));
		assert_eq!(tree.nodes[3].parent, NodePointer::Storage(2));
		assert_eq!(tree.nodes[4].parent, NodePointer::Storage(3));

		assert_eq!(tree.nodes[0].candidate_hash, candidate_a_hash);
		assert_eq!(tree.nodes[1].candidate_hash, candidate_a_hash);
		assert_eq!(tree.nodes[2].candidate_hash, candidate_a_hash);
		assert_eq!(tree.nodes[3].candidate_hash, candidate_a_hash);
		assert_eq!(tree.nodes[4].candidate_hash, candidate_a_hash);

		for count in 1..10 {
			assert_eq!(
				tree.find_backable_chain(Ancestors::new(), count, |_| true),
				iter::repeat(candidate_a_hash)
					.take(std::cmp::min(count as usize, max_depth + 1))
					.collect::<Vec<_>>()
			);
			assert_eq!(
				tree.find_backable_chain(
					[candidate_a_hash].into_iter().collect(),
					count - 1,
					|_| true
				),
				iter::repeat(candidate_a_hash)
					.take(std::cmp::min(count as usize - 1, max_depth))
					.collect::<Vec<_>>()
			);
		}
	}

	#[test]
	fn graceful_cycle_of_1() {
		let mut storage = CandidateStorage::new();

		let para_id = ParaId::from(5u32);
		let relay_parent_a = Hash::repeat_byte(1);

		let (pvd_a, candidate_a) = make_committed_candidate(
			para_id,
			relay_parent_a,
			0,
			vec![0x0a].into(),
			vec![0x0b].into(), // input same as output
			0,
		);
		let candidate_a_hash = candidate_a.hash();

		let (pvd_b, candidate_b) = make_committed_candidate(
			para_id,
			relay_parent_a,
			0,
			vec![0x0b].into(),
			vec![0x0a].into(), // input same as output
			0,
		);
		let candidate_b_hash = candidate_b.hash();

		let base_constraints = make_constraints(0, vec![0], vec![0x0a].into());
		let pending_availability = Vec::new();

		let relay_parent_a_info = RelayChainBlockInfo {
			number: pvd_a.relay_parent_number,
			hash: relay_parent_a,
			storage_root: pvd_a.relay_parent_storage_root,
		};

		let max_depth = 4;
		storage.add_candidate(candidate_a, pvd_a).unwrap();
		storage.add_candidate(candidate_b, pvd_b).unwrap();
		let scope = Scope::with_ancestors(
			para_id,
			relay_parent_a_info,
			base_constraints,
			pending_availability,
			max_depth,
			vec![],
		)
		.unwrap();
		let tree = FragmentTree::populate(scope, &storage);

		let candidates: Vec<_> = tree.candidates().collect();
		assert_eq!(candidates.len(), 2);
		assert_eq!(tree.nodes.len(), max_depth + 1);

		assert_eq!(tree.nodes[0].parent, NodePointer::Root);
		assert_eq!(tree.nodes[1].parent, NodePointer::Storage(0));
		assert_eq!(tree.nodes[2].parent, NodePointer::Storage(1));
		assert_eq!(tree.nodes[3].parent, NodePointer::Storage(2));
		assert_eq!(tree.nodes[4].parent, NodePointer::Storage(3));

		assert_eq!(tree.nodes[0].candidate_hash, candidate_a_hash);
		assert_eq!(tree.nodes[1].candidate_hash, candidate_b_hash);
		assert_eq!(tree.nodes[2].candidate_hash, candidate_a_hash);
		assert_eq!(tree.nodes[3].candidate_hash, candidate_b_hash);
		assert_eq!(tree.nodes[4].candidate_hash, candidate_a_hash);

		assert_eq!(tree.find_backable_chain(Ancestors::new(), 1, |_| true), vec![candidate_a_hash],);
		assert_eq!(
			tree.find_backable_chain(Ancestors::new(), 2, |_| true),
			vec![candidate_a_hash, candidate_b_hash],
		);
		assert_eq!(
			tree.find_backable_chain(Ancestors::new(), 3, |_| true),
			vec![candidate_a_hash, candidate_b_hash, candidate_a_hash],
		);
		assert_eq!(
			tree.find_backable_chain([candidate_a_hash].into_iter().collect(), 2, |_| true),
			vec![candidate_b_hash, candidate_a_hash],
		);

		assert_eq!(
			tree.find_backable_chain(Ancestors::new(), 6, |_| true),
			vec![
				candidate_a_hash,
				candidate_b_hash,
				candidate_a_hash,
				candidate_b_hash,
				candidate_a_hash
			],
		);

		for count in 3..7 {
			assert_eq!(
				tree.find_backable_chain(
					[candidate_a_hash, candidate_b_hash].into_iter().collect(),
					count,
					|_| true
				),
				vec![candidate_a_hash, candidate_b_hash, candidate_a_hash],
			);
		}
	}

	#[test]
	fn hypothetical_depths_known_and_unknown() {
		let mut storage = CandidateStorage::new();

		let para_id = ParaId::from(5u32);
		let relay_parent_a = Hash::repeat_byte(1);

		let (pvd_a, candidate_a) = make_committed_candidate(
			para_id,
			relay_parent_a,
			0,
			vec![0x0a].into(),
			vec![0x0b].into(), // input same as output
			0,
		);
		let candidate_a_hash = candidate_a.hash();

		let (pvd_b, candidate_b) = make_committed_candidate(
			para_id,
			relay_parent_a,
			0,
			vec![0x0b].into(),
			vec![0x0a].into(), // input same as output
			0,
		);
		let candidate_b_hash = candidate_b.hash();

		let base_constraints = make_constraints(0, vec![0], vec![0x0a].into());
		let pending_availability = Vec::new();

		let relay_parent_a_info = RelayChainBlockInfo {
			number: pvd_a.relay_parent_number,
			hash: relay_parent_a,
			storage_root: pvd_a.relay_parent_storage_root,
		};

		let max_depth = 4;
		storage.add_candidate(candidate_a, pvd_a).unwrap();
		storage.add_candidate(candidate_b, pvd_b).unwrap();
		let scope = Scope::with_ancestors(
			para_id,
			relay_parent_a_info,
			base_constraints,
			pending_availability,
			max_depth,
			vec![],
		)
		.unwrap();
		let tree = FragmentTree::populate(scope, &storage);

		let candidates: Vec<_> = tree.candidates().collect();
		assert_eq!(candidates.len(), 2);
		assert_eq!(tree.nodes.len(), max_depth + 1);

		assert_eq!(
			tree.hypothetical_depths(
				candidate_a_hash,
				HypotheticalCandidate::Incomplete {
					parent_head_data_hash: HeadData::from(vec![0x0a]).hash(),
					relay_parent: relay_parent_a,
				},
				&storage,
				false,
			),
			vec![0, 2, 4],
		);

		assert_eq!(
			tree.hypothetical_depths(
				candidate_b_hash,
				HypotheticalCandidate::Incomplete {
					parent_head_data_hash: HeadData::from(vec![0x0b]).hash(),
					relay_parent: relay_parent_a,
				},
				&storage,
				false,
			),
			vec![1, 3],
		);

		assert_eq!(
			tree.hypothetical_depths(
				CandidateHash(Hash::repeat_byte(21)),
				HypotheticalCandidate::Incomplete {
					parent_head_data_hash: HeadData::from(vec![0x0a]).hash(),
					relay_parent: relay_parent_a,
				},
				&storage,
				false,
			),
			vec![0, 2, 4],
		);

		assert_eq!(
			tree.hypothetical_depths(
				CandidateHash(Hash::repeat_byte(22)),
				HypotheticalCandidate::Incomplete {
					parent_head_data_hash: HeadData::from(vec![0x0b]).hash(),
					relay_parent: relay_parent_a,
				},
				&storage,
				false,
			),
			vec![1, 3]
		);
	}

	#[test]
	fn hypothetical_depths_stricter_on_complete() {
		let storage = CandidateStorage::new();

		let para_id = ParaId::from(5u32);
		let relay_parent_a = Hash::repeat_byte(1);

		let (pvd_a, candidate_a) = make_committed_candidate(
			para_id,
			relay_parent_a,
			0,
			vec![0x0a].into(),
			vec![0x0b].into(),
			1000, // watermark is illegal
		);

		let candidate_a_hash = candidate_a.hash();

		let base_constraints = make_constraints(0, vec![0], vec![0x0a].into());
		let pending_availability = Vec::new();

		let relay_parent_a_info = RelayChainBlockInfo {
			number: pvd_a.relay_parent_number,
			hash: relay_parent_a,
			storage_root: pvd_a.relay_parent_storage_root,
		};

		let max_depth = 4;
		let scope = Scope::with_ancestors(
			para_id,
			relay_parent_a_info,
			base_constraints,
			pending_availability,
			max_depth,
			vec![],
		)
		.unwrap();
		let tree = FragmentTree::populate(scope, &storage);

		assert_eq!(
			tree.hypothetical_depths(
				candidate_a_hash,
				HypotheticalCandidate::Incomplete {
					parent_head_data_hash: HeadData::from(vec![0x0a]).hash(),
					relay_parent: relay_parent_a,
				},
				&storage,
				false,
			),
			vec![0],
		);

		assert!(tree
			.hypothetical_depths(
				candidate_a_hash,
				HypotheticalCandidate::Complete {
					receipt: Cow::Owned(candidate_a),
					persisted_validation_data: Cow::Owned(pvd_a),
				},
				&storage,
				false,
			)
			.is_empty());
	}

	#[test]
	fn hypothetical_depths_backed_in_path() {
		let mut storage = CandidateStorage::new();

		let para_id = ParaId::from(5u32);
		let relay_parent_a = Hash::repeat_byte(1);

		let (pvd_a, candidate_a) = make_committed_candidate(
			para_id,
			relay_parent_a,
			0,
			vec![0x0a].into(),
			vec![0x0b].into(),
			0,
		);
		let candidate_a_hash = candidate_a.hash();

		let (pvd_b, candidate_b) = make_committed_candidate(
			para_id,
			relay_parent_a,
			0,
			vec![0x0b].into(),
			vec![0x0c].into(),
			0,
		);
		let candidate_b_hash = candidate_b.hash();

		let (pvd_c, candidate_c) = make_committed_candidate(
			para_id,
			relay_parent_a,
			0,
			vec![0x0b].into(),
			vec![0x0d].into(),
			0,
		);

		let base_constraints = make_constraints(0, vec![0], vec![0x0a].into());
		let pending_availability = Vec::new();

		let relay_parent_a_info = RelayChainBlockInfo {
			number: pvd_a.relay_parent_number,
			hash: relay_parent_a,
			storage_root: pvd_a.relay_parent_storage_root,
		};

		let max_depth = 4;
		storage.add_candidate(candidate_a, pvd_a).unwrap();
		storage.add_candidate(candidate_b, pvd_b).unwrap();
		storage.add_candidate(candidate_c, pvd_c).unwrap();

		// `A` and `B` are backed, `C` is not.
		storage.mark_backed(&candidate_a_hash);
		storage.mark_backed(&candidate_b_hash);

		let scope = Scope::with_ancestors(
			para_id,
			relay_parent_a_info,
			base_constraints,
			pending_availability,
			max_depth,
			vec![],
		)
		.unwrap();
		let tree = FragmentTree::populate(scope, &storage);

		let candidates: Vec<_> = tree.candidates().collect();
		assert_eq!(candidates.len(), 3);
		assert_eq!(tree.nodes.len(), 3);

		let candidate_d_hash = CandidateHash(Hash::repeat_byte(0xAA));

		assert_eq!(
			tree.hypothetical_depths(
				candidate_d_hash,
				HypotheticalCandidate::Incomplete {
					parent_head_data_hash: HeadData::from(vec![0x0a]).hash(),
					relay_parent: relay_parent_a,
				},
				&storage,
				true,
			),
			vec![0],
		);

		assert_eq!(
			tree.hypothetical_depths(
				candidate_d_hash,
				HypotheticalCandidate::Incomplete {
					parent_head_data_hash: HeadData::from(vec![0x0c]).hash(),
					relay_parent: relay_parent_a,
				},
				&storage,
				true,
			),
			vec![2],
		);

		assert_eq!(
			tree.hypothetical_depths(
				candidate_d_hash,
				HypotheticalCandidate::Incomplete {
					parent_head_data_hash: HeadData::from(vec![0x0d]).hash(),
					relay_parent: relay_parent_a,
				},
				&storage,
				true,
			),
			Vec::<usize>::new(),
		);

		assert_eq!(
			tree.hypothetical_depths(
				candidate_d_hash,
				HypotheticalCandidate::Incomplete {
					parent_head_data_hash: HeadData::from(vec![0x0d]).hash(),
					relay_parent: relay_parent_a,
				},
				&storage,
				false,
			),
			vec![2], // non-empty if `false`.
		);
	}

	#[test]
	fn pending_availability_in_scope() {
		let mut storage = CandidateStorage::new();

		let para_id = ParaId::from(5u32);
		let relay_parent_a = Hash::repeat_byte(1);
		let relay_parent_b = Hash::repeat_byte(2);
		let relay_parent_c = Hash::repeat_byte(3);

		let (pvd_a, candidate_a) = make_committed_candidate(
			para_id,
			relay_parent_a,
			0,
			vec![0x0a].into(),
			vec![0x0b].into(),
			0,
		);
		let candidate_a_hash = candidate_a.hash();

		let (pvd_b, candidate_b) = make_committed_candidate(
			para_id,
			relay_parent_b,
			1,
			vec![0x0b].into(),
			vec![0x0c].into(),
			1,
		);

		// Note that relay parent `a` is not allowed.
		let base_constraints = make_constraints(1, vec![], vec![0x0a].into());

		let relay_parent_a_info = RelayChainBlockInfo {
			number: pvd_a.relay_parent_number,
			hash: relay_parent_a,
			storage_root: pvd_a.relay_parent_storage_root,
		};
		let pending_availability = vec![PendingAvailability {
			candidate_hash: candidate_a_hash,
			relay_parent: relay_parent_a_info,
		}];

		let relay_parent_b_info = RelayChainBlockInfo {
			number: pvd_b.relay_parent_number,
			hash: relay_parent_b,
			storage_root: pvd_b.relay_parent_storage_root,
		};
		let relay_parent_c_info = RelayChainBlockInfo {
			number: pvd_b.relay_parent_number + 1,
			hash: relay_parent_c,
			storage_root: Hash::zero(),
		};

		let max_depth = 4;
		storage.add_candidate(candidate_a, pvd_a).unwrap();
		storage.add_candidate(candidate_b, pvd_b).unwrap();
		storage.mark_backed(&candidate_a_hash);

		let scope = Scope::with_ancestors(
			para_id,
			relay_parent_c_info,
			base_constraints,
			pending_availability,
			max_depth,
			vec![relay_parent_b_info],
		)
		.unwrap();
		let tree = FragmentTree::populate(scope, &storage);

		let candidates: Vec<_> = tree.candidates().collect();
		assert_eq!(candidates.len(), 2);
		assert_eq!(tree.nodes.len(), 2);

		let candidate_d_hash = CandidateHash(Hash::repeat_byte(0xAA));

		assert_eq!(
			tree.hypothetical_depths(
				candidate_d_hash,
				HypotheticalCandidate::Incomplete {
					parent_head_data_hash: HeadData::from(vec![0x0b]).hash(),
					relay_parent: relay_parent_c,
				},
				&storage,
				false,
			),
			vec![1],
		);

		assert_eq!(
			tree.hypothetical_depths(
				candidate_d_hash,
				HypotheticalCandidate::Incomplete {
					parent_head_data_hash: HeadData::from(vec![0x0c]).hash(),
					relay_parent: relay_parent_b,
				},
				&storage,
				false,
			),
			vec![2],
		);
	}
}
