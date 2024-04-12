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

//! Utility for managing parachain fragments not referenced by the relay-chain.
//!
//! # Overview
//!
//! This module exposes two main types: [`FragmentChain`] and [`CandidateStorage`] which are meant
//! to be used in close conjunction. Each fragment chain is associated with a particular
//! relay-parent and each node in the chain represents a candidate. Each parachain has a single
//! candidate storage, but can have one chain for each relay chain block in the view.
//! Therefore, the same candidate can be present in multiple fragment chains of a parachain. One of
//! the purposes of the candidate storage is to deduplicate the large candidate data that is being
//! referenced from multiple fragment chains.
//!
//! A chain has an associated [`Scope`] which defines limits on candidates within the chain.
//! Candidates themselves have their own [`Constraints`] which are either the constraints from the
//! scope, or, if there are previous nodes in the chain, a modified version of the previous
//! candidate's constraints.
//!
//! Another use of the `CandidateStorage` is to keep a record of candidates which may not be yet
//! included in any chain, but which may become part of a chain in the future. This is needed for
//! elastic scaling, so that we may parallelise the backing process across different groups. As long
//! as some basic constraints are not violated by an unconnected candidate (like the relay parent
//! being in scope), we proceed with the backing process, hoping that its predecessors will be
//! backed soon enough.
//!
//! This module also makes use of types provided by the Inclusion Emulator module, such as
//! [`Fragment`] and [`Constraints`]. These perform the actual job of checking for validity of
//! prospective fragments.
//!
//! # Parachain forks
//!
//! Parachains are expected to not create forks, hence the use of fragment chains as opposed to
//! fragment trees. If parachains do create forks, their performance in regards to async backing and
//! elastic scaling will suffer, because different validators will have different views of the
//! future.
//!
//! This is a compromise we can make - collators which want to use async backing and elastic scaling
//! need to cooperate for the highest throughput.
//!
//! # Parachain cycles
//!
//! Parachains can create cycles, because:
//!   1. There's no requirement that head-data is unique for a parachain. Furthermore, a parachain
//!      is under no obligation to be acyclic, and this is mostly just because it's totally
//!      inefficient to enforce it. Practical use-cases are acyclic, but there is still more than
//!      one way to reach the same head-data.
//!   2. and candidates only refer to their parent by its head-data. This whole issue could be
//!      resolved by having candidates reference their parent by candidate hash.
//!
//! However, dealing with cycles increases complexity during the backing/inclusion process for no
//! practical reason. Therefore, fragment chains will not accept such candidates.
//!
//! On the other hand, enforcing that a parachain will NEVER be acyclic would be very complicated
//! (looping through the entire parachain's history on every new candidate or changing the candidate
//! receipt to reference the parent's candidate hash).
//!
//! # Spam protection
//!
//! As long as the [`CandidateStorage`] has bounded input on the number of candidates supplied,
//! [`FragmentChain`] complexity is bounded. This means that higher-level code needs to be selective
//! about limiting the amount of candidates that are considered.
//!
//! The code in this module is not designed for speed or efficiency, but conceptual simplicity.
//! Our assumption is that the amount of candidates and parachains we consider will be reasonably
//! bounded and in practice will not exceed a few thousand at any time. This naive implementation
//! will still perform fairly well under these conditions, despite being somewhat wasteful of
//! memory.

use std::{
	borrow::Cow,
	collections::{
		hash_map::{Entry, HashMap},
		BTreeMap, HashSet,
	},
};

use super::LOG_TARGET;
use polkadot_node_subsystem::messages::{self, Ancestors};
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
}

/// Stores candidates and information about them such as their relay-parents and their backing
/// states.
pub(crate) struct CandidateStorage {
	// Index from head data hash to candidate hashes with that head data as a parent. Purely for
	// efficiency when responding to `ProspectiveValidationDataRequest`s or when trying to find a
	// new candidate to push to a chain.
	// Even though having multiple candidates with same parent would be invalid for a parachain, it
	// could happen across different relay chain forks, hence the HashSet.
	by_parent_head: HashMap<Hash, HashSet<CandidateHash>>,

	// Index from head data hash to candidate hashes outputting that head data. Purely for
	// efficiency when responding to `ProspectiveValidationDataRequest`s.
	// Even though having multiple candidates with same output would be invalid for a parachain,
	// it could happen across different relay chain forks.
	by_output_head: HashMap<Hash, HashSet<CandidateHash>>,

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

		let entry = CandidateEntry {
			candidate_hash,
			parent_head_data_hash: persisted_validation_data.parent_head.hash(),
			output_head_data_hash: candidate.commitments.head_data.hash(),
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

		self.by_parent_head
			.entry(entry.parent_head_data_hash())
			.or_default()
			.insert(candidate_hash);
		self.by_output_head
			.entry(entry.output_head_data_hash())
			.or_default()
			.insert(candidate_hash);
		// sanity-checked already.
		self.by_candidate_hash.insert(candidate_hash, entry);

		Ok(candidate_hash)
	}

	/// Remove a candidate from the store.
	pub fn remove_candidate(&mut self, candidate_hash: &CandidateHash) {
		if let Some(entry) = self.by_candidate_hash.remove(candidate_hash) {
			if let Entry::Occupied(mut e) = self.by_parent_head.entry(entry.parent_head_data_hash())
			{
				e.get_mut().remove(&candidate_hash);
				if e.get().is_empty() {
					e.remove();
				}
			}

			if let Entry::Occupied(mut e) = self.by_output_head.entry(entry.output_head_data_hash())
			{
				e.get_mut().remove(&candidate_hash);
				if e.get().is_empty() {
					e.remove();
				}
			}
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

	/// Return an iterator over the stored candidates.
	pub fn candidates(&self) -> impl Iterator<Item = &CandidateEntry> {
		self.by_candidate_hash.values()
	}

	/// Retain only candidates which pass the predicate.
	pub(crate) fn retain(&mut self, pred: impl Fn(&CandidateHash) -> bool) {
		self.by_candidate_hash.retain(|h, _v| pred(h));
		self.by_parent_head.retain(|_parent, children| {
			children.retain(|h| pred(h));
			!children.is_empty()
		});
		self.by_output_head.retain(|_output, candidates| {
			candidates.retain(|h| pred(h));
			!candidates.is_empty()
		});
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
			.and_then(|m| m.iter().next())
			.and_then(|a_candidate| self.by_candidate_hash.get(a_candidate))
			.map(|e| &e.candidate.commitments.head_data)
			.or_else(|| {
				self.by_parent_head
					.get(hash)
					.and_then(|m| m.iter().next())
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

	/// Returns the candidates which have the given head data hash as parent.
	/// We don't allow forks in a parachain, but we may have multiple candidates with same parent
	/// across different relay chain forks. That's why it returns an iterator (but only one will be
	/// valid and used in the end).
	fn possible_para_children<'a>(
		&'a self,
		parent_head_hash: &'a Hash,
	) -> impl Iterator<Item = &'a CandidateEntry> + 'a {
		let by_candidate_hash = &self.by_candidate_hash;
		self.by_parent_head
			.get(parent_head_hash)
			.into_iter()
			.flat_map(|hashes| hashes.iter())
			.filter_map(move |h| by_candidate_hash.get(h))
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
pub(crate) struct CandidateEntry {
	candidate_hash: CandidateHash,
	parent_head_data_hash: Hash,
	output_head_data_hash: Hash,
	relay_parent: Hash,
	candidate: ProspectiveCandidate<'static>,
	state: CandidateState,
}

impl CandidateEntry {
	pub fn hash(&self) -> CandidateHash {
		self.candidate_hash
	}

	pub fn parent_head_data_hash(&self) -> Hash {
		self.parent_head_data_hash
	}

	pub fn output_head_data_hash(&self) -> Hash {
		self.output_head_data_hash
	}
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

/// The scope of a [`FragmentChain`].
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

	/// Get the earliest relay-parent allowed in the scope of the fragment chain.
	pub fn earliest_relay_parent(&self) -> RelayChainBlockInfo {
		self.ancestors
			.iter()
			.next()
			.map(|(_, v)| v.clone())
			.unwrap_or_else(|| self.relay_parent.clone())
	}

	/// Get the relay ancestor of the fragment chain by hash.
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
/// the fragment chain already.
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

	fn output_head_data_hash(&self) -> Option<Hash> {
		match *self {
			HypotheticalCandidate::Complete { ref receipt, .. } =>
				Some(receipt.descriptor.para_head),
			HypotheticalCandidate::Incomplete { .. } => None,
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

impl<'a> From<&'a messages::HypotheticalCandidate> for HypotheticalCandidate<'a> {
	fn from(value: &'a messages::HypotheticalCandidate) -> Self {
		match value {
			messages::HypotheticalCandidate::Complete {
				receipt,
				persisted_validation_data,
				..
			} => Self::Complete {
				receipt: Cow::Borrowed(receipt),
				persisted_validation_data: Cow::Borrowed(persisted_validation_data),
			},
			messages::HypotheticalCandidate::Incomplete {
				parent_head_data_hash,
				candidate_relay_parent,
				..
			} => Self::Incomplete {
				relay_parent: *candidate_relay_parent,
				parent_head_data_hash: *parent_head_data_hash,
			},
		}
	}
}

pub struct FragmentNode {
	fragment: Fragment<'static>,
	pub candidate_hash: CandidateHash,
	cumulative_modifications: ConstraintModifications,
}

impl FragmentNode {
	fn relay_parent(&self) -> Hash {
		self.fragment.relay_parent().hash
	}
}

/// This is a chain of candidates based on some underlying storage of candidates and a scope.
///
/// All nodes in the chain must be either pending availability or within the scope. Within the scope
/// means it's built off of the relay-parent or an ancestor.
pub(crate) struct FragmentChain {
	scope: Scope,

	pub chain: Vec<FragmentNode>,

	candidates: HashSet<CandidateHash>,

	// Index from head data hash to candidate hashes with that head data as a parent.
	by_parent_head: HashMap<Hash, CandidateHash>,
	// Index from head data hash to candidate hashes outputting that head data.
	by_output_head: HashMap<Hash, CandidateHash>,
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

		let mut fragment_chain = Self {
			scope,
			chain: Vec::new(),
			candidates: HashSet::new(),
			by_parent_head: HashMap::new(),
			by_output_head: HashMap::new(),
		};

		fragment_chain.populate_chain(storage);

		fragment_chain
	}

	/// Get the scope of the Fragment Chain.
	pub fn scope(&self) -> &Scope {
		&self.scope
	}

	/// Returns an O(n) iterator over the hashes of candidates contained in the
	/// chain.
	pub(crate) fn candidates(&self) -> impl Iterator<Item = CandidateHash> + '_ {
		self.candidates.iter().cloned()
	}

	/// Returns the number of candidates in the chain
	pub(crate) fn len(&self) -> usize {
		self.candidates.len()
	}

	/// Whether the candidate exists.
	pub(crate) fn contains_candidate(&self, candidate: &CandidateHash) -> bool {
		self.candidates.contains(candidate)
	}

	/// Try accumulating more candidates onto the chain.
	///
	/// Candidates can only be added if they build on the already existing chain.
	pub(crate) fn repopulate(&mut self, storage: &CandidateStorage) {
		self.populate_chain(storage);
	}

	/// Returns the hypothetical state of a candidate with the given hash and parent head data
	/// in regards to the existing chain.
	///
	/// Returns true if either:
	/// - the candidate is already present
	/// - the candidate can be added to the chain
	/// - the candidate could potentially be added to the chain in the future (its ancestors are
	///   still unknown but it doesn't violate other rules).
	///
	/// If this returns false, the candidate could never be added to the current chain (not now, not
	/// ever)
	pub(crate) fn hypothetical_membership(
		&self,
		candidate_hash: CandidateHash,
		candidate: HypotheticalCandidate,
		candidate_storage: &CandidateStorage,
	) -> bool {
		// If we've already used this candidate in the chain
		if self.candidates.contains(&candidate_hash) {
			return true
		}

		if !self.can_add_candidate_as_potential(
			candidate_storage,
			&candidate.relay_parent(),
			candidate.parent_head_data_hash(),
			candidate.output_head_data_hash(),
		) {
			return false
		}

		let Some(candidate_relay_parent) = self.scope.ancestor_by_hash(&candidate.relay_parent())
		else {
			// can_add_candidate_as_potential already checked for this, but just to be safe.
			return false
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
						?candidate_hash,
						err = ?e,
						"Failed to apply modifications",
					);

					return false
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
					gum::debug!(
						target: LOG_TARGET,
						"Fragment::new() returned error",
					);
					return false
				}
			}

			// If we got this far, it can be added to the chain right now.
			true
		} else {
			// Otherwise it is or can be an unconnected candidate.
			true
		}
	}

	/// Select `count` candidates after the given `ancestors` which pass
	/// the predicate and have not already been backed on chain.
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

	// Tries to orders the ancestors into a viable path from root to the last one.
	// Stops when the ancestors are all used or when a node in the chain is not present in the
	// ancestor set. Returns the index in the chain were the search stopped.
	fn find_ancestor_path(&self, mut ancestors: Ancestors) -> usize {
		if self.chain.is_empty() {
			return 0;
		}

		for (index, candidate) in self.chain.iter().enumerate() {
			if !ancestors.remove(&candidate.candidate_hash) {
				return index
			}
		}

		// This means that we found the entire chain in the ancestor set. There won't be anything
		// left to back.
		self.chain.len()
	}

	// Return the earliest relay parent a new candidate can have in order to be added to the chain.
	// This is the relay parent of the last candidate in the chain.
	// The value returned may not be valid if we want to add a candidate pending availability, which
	// may have a relay parent which is out of scope. Special handling is needed in that case.
	fn earliest_relay_parent(&self) -> RelayChainBlockInfo {
		if let Some(last_candidate) = self.chain.last() {
			self.scope
				.ancestor_by_hash(&last_candidate.relay_parent())
				.or_else(|| {
					// if the relay-parent is out of scope _and_ it is in the chain,
					// it must be a candidate pending availability.
					self.scope
						.get_pending_availability(&last_candidate.candidate_hash)
						.map(|c| c.relay_parent.clone())
				})
				.expect("All nodes in chain are either pending availability or within scope; qed")
		} else {
			self.scope.earliest_relay_parent()
		}
	}

	// Checks if this candidate could be added in the future to this chain.
	// This assumes that the chain does not already contain this candidate.
	pub(crate) fn can_add_candidate_as_potential(
		&self,
		storage: &CandidateStorage,
		relay_parent: &Hash,
		parent_head_hash: Hash,
		output_head_hash: Option<Hash>,
	) -> bool {
		// If we've got enough candidates for the configured depth, no point in adding more.
		if self.chain.len() >= self.scope.max_depth {
			return false
		}

		if !self.check_potential(relay_parent, parent_head_hash, output_head_hash) {
			return false
		}

		let unconnected = self.find_unconnected_potential_candidates(storage).len();

		if (self.chain.len() + unconnected) < self.scope.max_depth {
			true
		} else {
			gum::debug!(
				target: LOG_TARGET,
				"Too many unconnected candidates",
			);
			false
		}
	}

	// The candidates which are present in `CandidateStorage`, are not part of this chain but could
	// become part of this chain in the future. Capped at the max depth minus the existing chain
	// length.
	pub(crate) fn find_unconnected_potential_candidates(
		&self,
		storage: &CandidateStorage,
	) -> Vec<CandidateHash> {
		let mut candidates = vec![];
		for candidate in storage.candidates() {
			// We stop at max_depth with the search. There's no point in looping further.
			if (self.chain.len() + candidates.len()) >= self.scope.max_depth {
				break
			}
			if !self.candidates.contains(&candidate.candidate_hash) {
				if self.check_potential(
					&candidate.relay_parent,
					candidate.candidate.persisted_validation_data.parent_head.hash(),
					Some(candidate.candidate.commitments.head_data.hash()),
				) {
					candidates.push(candidate.candidate_hash);
				}
			}
		}

		candidates
	}

	// Check if adding a candidate which transitions `parent_head_hash` to `output_head_hash` would
	// introduce a fork or a cycle in the parachain.
	// `output_head_hash` is optional because we sometimes make this check before retrieving the
	// collation.
	fn check_forks_and_cycles(
		&self,
		parent_head_hash: Hash,
		output_head_hash: Option<Hash>,
	) -> bool {
		if self.by_parent_head.contains_key(&parent_head_hash) {
			// fork. our parent has another child already
			return false;
		}

		if let Some(output_head_hash) = output_head_hash {
			if self.by_output_head.contains_key(&output_head_hash) {
				// this is not a chain, there are multiple paths to the same state.
				return false;
			}

			// trivial 0-length cycle.
			if parent_head_hash == output_head_hash {
				return false;
			}

			// this should catch any other cycles. our output state cannot already be the parent
			// state of another candidate, unless this is a cycle, since the already added
			// candidates form a chain.
			if self.by_parent_head.contains_key(&output_head_hash) {
				return false;
			}
		}

		true
	}

	// Checks the potential of a candidate to be added to the chain in the future.
	// Verifies that the relay parent is in scope and not moving backwards and that we're not
	// introducing forks or cycles with other candidates in the chain.
	// `output_head_hash` is optional because we sometimes make this check before retrieving the
	// collation.
	fn check_potential(
		&self,
		relay_parent: &Hash,
		parent_head_hash: Hash,
		output_head_hash: Option<Hash>,
	) -> bool {
		if !self.check_forks_and_cycles(parent_head_hash, output_head_hash) {
			return false;
		}

		let earliest_rp = self.earliest_relay_parent();

		let Some(relay_parent) = self.scope.ancestor_by_hash(relay_parent) else { return false };

		if relay_parent.number < earliest_rp.number {
			return false // relay parent moved backwards.
		}

		true
	}

	// Populate the fragment chain with candidates from CandidateStorage.
	// Can be called by the constructor or when introducing a new candidate.
	// If we're introducing a new candidate onto an existing chain, we may introduce more than one,
	// since we may connect already existing candidates to the chain.
	fn populate_chain(&mut self, storage: &CandidateStorage) {
		let mut cumulative_modifications = if let Some(last_candidate) = self.chain.last() {
			last_candidate.cumulative_modifications.clone()
		} else {
			ConstraintModifications::identity()
		};
		let mut earliest_rp = self.earliest_relay_parent();

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
			// Even though we don't allow parachain forks under the same active leaf, they may still
			// appear under different relay chain forks, hence the iterator below.
			let possible_children = storage.possible_para_children(&required_head_hash);
			let mut added_child = false;
			for candidate in possible_children {
				// Add one node to chain if
				// 1. it does not introduce a fork or a cycle.
				// 2. parent hash is correct.
				// 3. relay-parent does not move backwards.
				// 4. all non-pending-availability candidates have relay-parent in scope.
				// 5. candidate outputs fulfill constraints

				if !self.check_forks_and_cycles(
					candidate.parent_head_data_hash(),
					Some(candidate.output_head_data_hash()),
				) {
					gum::debug!(
						target: LOG_TARGET,
						candidate_hash = ?candidate.candidate_hash,
						"Refusing to add candidate to the fragment chain, it would introduce a fork or a cycle",
					);
					continue
				}

				let pending = self.scope.get_pending_availability(&candidate.candidate_hash);
				let Some(relay_parent) = pending
					.map(|p| p.relay_parent.clone())
					.or_else(|| self.scope.ancestor_by_hash(&candidate.relay_parent))
				else {
					continue
				};

				// require: candidates don't move backwards
				// and only pending availability candidates can be out-of-scope.
				//
				// earliest_rp can be before the earliest relay parent in the scope
				// when the parent is a pending availability candidate as well, but
				// only other pending candidates can have a relay parent out of scope.
				let min_relay_parent_number = pending
					.map(|p| match self.chain.len() {
						0 => p.relay_parent.number,
						_ => earliest_rp.number,
					})
					.unwrap_or_else(|| earliest_rp.number);

				if relay_parent.number < min_relay_parent_number {
					continue // relay parent moved backwards.
				}

				// don't add candidates if they're already present in the chain.
				// this can never happen, as candidates can only be duplicated if there's a cycle
				// and we shouldn't have allowed for a cycle to be chained.
				if self.contains_candidate(&candidate.candidate_hash) {
					continue
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
				// We've already checked for forks and cycles.
				self.by_parent_head
					.insert(candidate.parent_head_data_hash(), candidate.candidate_hash);
				self.by_output_head
					.insert(candidate.output_head_data_hash(), candidate.candidate_hash);
				added_child = true;
				// We can only add one child for a candidate. (it's a chain, not a tree)
				break;
			}

			if !added_child {
				break
			}
		}
	}
}

// TODO: need to rework these tests
// #[cfg(test)]
// mod tests {
// 	use super::*;
// 	use assert_matches::assert_matches;
// 	use polkadot_node_subsystem_util::inclusion_emulator::InboundHrmpLimitations;
// 	use polkadot_primitives::{BlockNumber, CandidateCommitments, CandidateDescriptor, HeadData};
// 	use polkadot_primitives_test_helpers as test_helpers;
// 	use rstest::rstest;
// 	use std::iter;

// 	impl NodePointer {
// 		fn unwrap_idx(self) -> usize {
// 			match self {
// 				NodePointer::Root => panic!("Unexpected root"),
// 				NodePointer::Storage(index) => index,
// 			}
// 		}
// 	}

// 	fn make_constraints(
// 		min_relay_parent_number: BlockNumber,
// 		valid_watermarks: Vec<BlockNumber>,
// 		required_parent: HeadData,
// 	) -> Constraints {
// 		Constraints {
// 			min_relay_parent_number,
// 			max_pov_size: 1_000_000,
// 			max_code_size: 1_000_000,
// 			ump_remaining: 10,
// 			ump_remaining_bytes: 1_000,
// 			max_ump_num_per_candidate: 10,
// 			dmp_remaining_messages: [0; 10].into(),
// 			hrmp_inbound: InboundHrmpLimitations { valid_watermarks },
// 			hrmp_channels_out: HashMap::new(),
// 			max_hrmp_num_per_candidate: 0,
// 			required_parent,
// 			validation_code_hash: Hash::repeat_byte(42).into(),
// 			upgrade_restriction: None,
// 			future_validation_code: None,
// 		}
// 	}

// 	fn make_committed_candidate(
// 		para_id: ParaId,
// 		relay_parent: Hash,
// 		relay_parent_number: BlockNumber,
// 		parent_head: HeadData,
// 		para_head: HeadData,
// 		hrmp_watermark: BlockNumber,
// 	) -> (PersistedValidationData, CommittedCandidateReceipt) {
// 		let persisted_validation_data = PersistedValidationData {
// 			parent_head,
// 			relay_parent_number,
// 			relay_parent_storage_root: Hash::repeat_byte(69),
// 			max_pov_size: 1_000_000,
// 		};

// 		let candidate = CommittedCandidateReceipt {
// 			descriptor: CandidateDescriptor {
// 				para_id,
// 				relay_parent,
// 				collator: test_helpers::dummy_collator(),
// 				persisted_validation_data_hash: persisted_validation_data.hash(),
// 				pov_hash: Hash::repeat_byte(1),
// 				erasure_root: Hash::repeat_byte(1),
// 				signature: test_helpers::dummy_collator_signature(),
// 				para_head: para_head.hash(),
// 				validation_code_hash: Hash::repeat_byte(42).into(),
// 			},
// 			commitments: CandidateCommitments {
// 				upward_messages: Default::default(),
// 				horizontal_messages: Default::default(),
// 				new_validation_code: None,
// 				head_data: para_head,
// 				processed_downward_messages: 1,
// 				hrmp_watermark,
// 			},
// 		};

// 		(persisted_validation_data, candidate)
// 	}

// 	#[test]
// 	fn scope_rejects_ancestors_that_skip_blocks() {
// 		let para_id = ParaId::from(5u32);
// 		let relay_parent = RelayChainBlockInfo {
// 			number: 10,
// 			hash: Hash::repeat_byte(10),
// 			storage_root: Hash::repeat_byte(69),
// 		};

// 		let ancestors = vec![RelayChainBlockInfo {
// 			number: 8,
// 			hash: Hash::repeat_byte(8),
// 			storage_root: Hash::repeat_byte(69),
// 		}];

// 		let max_depth = 2;
// 		let base_constraints = make_constraints(8, vec![8, 9], vec![1, 2, 3].into());
// 		let pending_availability = Vec::new();

// 		assert_matches!(
// 			Scope::with_ancestors(
// 				para_id,
// 				relay_parent,
// 				base_constraints,
// 				pending_availability,
// 				max_depth,
// 				ancestors
// 			),
// 			Err(UnexpectedAncestor { number: 8, prev: 10 })
// 		);
// 	}

// 	#[test]
// 	fn scope_rejects_ancestor_for_0_block() {
// 		let para_id = ParaId::from(5u32);
// 		let relay_parent = RelayChainBlockInfo {
// 			number: 0,
// 			hash: Hash::repeat_byte(0),
// 			storage_root: Hash::repeat_byte(69),
// 		};

// 		let ancestors = vec![RelayChainBlockInfo {
// 			number: 99999,
// 			hash: Hash::repeat_byte(99),
// 			storage_root: Hash::repeat_byte(69),
// 		}];

// 		let max_depth = 2;
// 		let base_constraints = make_constraints(0, vec![], vec![1, 2, 3].into());
// 		let pending_availability = Vec::new();

// 		assert_matches!(
// 			Scope::with_ancestors(
// 				para_id,
// 				relay_parent,
// 				base_constraints,
// 				pending_availability,
// 				max_depth,
// 				ancestors,
// 			),
// 			Err(UnexpectedAncestor { number: 99999, prev: 0 })
// 		);
// 	}

// 	#[test]
// 	fn scope_only_takes_ancestors_up_to_min() {
// 		let para_id = ParaId::from(5u32);
// 		let relay_parent = RelayChainBlockInfo {
// 			number: 5,
// 			hash: Hash::repeat_byte(0),
// 			storage_root: Hash::repeat_byte(69),
// 		};

// 		let ancestors = vec![
// 			RelayChainBlockInfo {
// 				number: 4,
// 				hash: Hash::repeat_byte(4),
// 				storage_root: Hash::repeat_byte(69),
// 			},
// 			RelayChainBlockInfo {
// 				number: 3,
// 				hash: Hash::repeat_byte(3),
// 				storage_root: Hash::repeat_byte(69),
// 			},
// 			RelayChainBlockInfo {
// 				number: 2,
// 				hash: Hash::repeat_byte(2),
// 				storage_root: Hash::repeat_byte(69),
// 			},
// 		];

// 		let max_depth = 2;
// 		let base_constraints = make_constraints(3, vec![2], vec![1, 2, 3].into());
// 		let pending_availability = Vec::new();

// 		let scope = Scope::with_ancestors(
// 			para_id,
// 			relay_parent,
// 			base_constraints,
// 			pending_availability,
// 			max_depth,
// 			ancestors,
// 		)
// 		.unwrap();

// 		assert_eq!(scope.ancestors.len(), 2);
// 		assert_eq!(scope.ancestors_by_hash.len(), 2);
// 	}

// 	#[test]
// 	fn storage_add_candidate() {
// 		let mut storage = CandidateStorage::new();
// 		let relay_parent = Hash::repeat_byte(69);

// 		let (pvd, candidate) = make_committed_candidate(
// 			ParaId::from(5u32),
// 			relay_parent,
// 			8,
// 			vec![4, 5, 6].into(),
// 			vec![1, 2, 3].into(),
// 			7,
// 		);

// 		let candidate_hash = candidate.hash();
// 		let parent_head_hash = pvd.parent_head.hash();

// 		storage.add_candidate(candidate, pvd).unwrap();
// 		assert!(storage.contains(&candidate_hash));
// 		assert_eq!(storage.iter_para_children(&parent_head_hash).count(), 1);

// 		assert_eq!(storage.relay_parent_by_candidate_hash(&candidate_hash), Some(relay_parent));
// 	}

// 	#[test]
// 	fn storage_retain() {
// 		let mut storage = CandidateStorage::new();

// 		let (pvd, candidate) = make_committed_candidate(
// 			ParaId::from(5u32),
// 			Hash::repeat_byte(69),
// 			8,
// 			vec![4, 5, 6].into(),
// 			vec![1, 2, 3].into(),
// 			7,
// 		);

// 		let candidate_hash = candidate.hash();
// 		let output_head_hash = candidate.commitments.head_data.hash();
// 		let parent_head_hash = pvd.parent_head.hash();

// 		storage.add_candidate(candidate, pvd).unwrap();
// 		storage.retain(|_| true);
// 		assert!(storage.contains(&candidate_hash));
// 		assert_eq!(storage.iter_para_children(&parent_head_hash).count(), 1);
// 		assert!(storage.head_data_by_hash(&output_head_hash).is_some());

// 		storage.retain(|_| false);
// 		assert!(!storage.contains(&candidate_hash));
// 		assert_eq!(storage.iter_para_children(&parent_head_hash).count(), 0);
// 		assert!(storage.head_data_by_hash(&output_head_hash).is_none());
// 	}

// 	// [`FragmentTree::populate`] should pick up candidates that build on other candidates.
// 	#[test]
// 	fn populate_works_recursively() {
// 		let mut storage = CandidateStorage::new();

// 		let para_id = ParaId::from(5u32);
// 		let relay_parent_a = Hash::repeat_byte(1);
// 		let relay_parent_b = Hash::repeat_byte(2);

// 		let (pvd_a, candidate_a) = make_committed_candidate(
// 			para_id,
// 			relay_parent_a,
// 			0,
// 			vec![0x0a].into(),
// 			vec![0x0b].into(),
// 			0,
// 		);
// 		let candidate_a_hash = candidate_a.hash();

// 		let (pvd_b, candidate_b) = make_committed_candidate(
// 			para_id,
// 			relay_parent_b,
// 			1,
// 			vec![0x0b].into(),
// 			vec![0x0c].into(),
// 			1,
// 		);
// 		let candidate_b_hash = candidate_b.hash();

// 		let base_constraints = make_constraints(0, vec![0], vec![0x0a].into());
// 		let pending_availability = Vec::new();

// 		let ancestors = vec![RelayChainBlockInfo {
// 			number: pvd_a.relay_parent_number,
// 			hash: relay_parent_a,
// 			storage_root: pvd_a.relay_parent_storage_root,
// 		}];

// 		let relay_parent_b_info = RelayChainBlockInfo {
// 			number: pvd_b.relay_parent_number,
// 			hash: relay_parent_b,
// 			storage_root: pvd_b.relay_parent_storage_root,
// 		};

// 		storage.add_candidate(candidate_a, pvd_a).unwrap();
// 		storage.add_candidate(candidate_b, pvd_b).unwrap();
// 		let scope = Scope::with_ancestors(
// 			para_id,
// 			relay_parent_b_info,
// 			base_constraints,
// 			pending_availability,
// 			4,
// 			ancestors,
// 		)
// 		.unwrap();
// 		let tree = FragmentTree::populate(scope, &storage);

// 		let candidates: Vec<_> = tree.candidates().collect();
// 		assert_eq!(candidates.len(), 2);
// 		assert!(candidates.contains(&candidate_a_hash));
// 		assert!(candidates.contains(&candidate_b_hash));

// 		assert_eq!(tree.nodes.len(), 2);
// 		assert_eq!(tree.nodes[0].parent, NodePointer::Root);
// 		assert_eq!(tree.nodes[0].candidate_hash, candidate_a_hash);
// 		assert_eq!(tree.nodes[0].depth, 0);

// 		assert_eq!(tree.nodes[1].parent, NodePointer::Storage(0));
// 		assert_eq!(tree.nodes[1].candidate_hash, candidate_b_hash);
// 		assert_eq!(tree.nodes[1].depth, 1);
// 	}

// 	#[test]
// 	fn children_of_root_are_contiguous() {
// 		let mut storage = CandidateStorage::new();

// 		let para_id = ParaId::from(5u32);
// 		let relay_parent_a = Hash::repeat_byte(1);
// 		let relay_parent_b = Hash::repeat_byte(2);

// 		let (pvd_a, candidate_a) = make_committed_candidate(
// 			para_id,
// 			relay_parent_a,
// 			0,
// 			vec![0x0a].into(),
// 			vec![0x0b].into(),
// 			0,
// 		);

// 		let (pvd_b, candidate_b) = make_committed_candidate(
// 			para_id,
// 			relay_parent_b,
// 			1,
// 			vec![0x0b].into(),
// 			vec![0x0c].into(),
// 			1,
// 		);

// 		let (pvd_a2, candidate_a2) = make_committed_candidate(
// 			para_id,
// 			relay_parent_a,
// 			0,
// 			vec![0x0a].into(),
// 			vec![0x0b, 1].into(),
// 			0,
// 		);
// 		let candidate_a2_hash = candidate_a2.hash();

// 		let base_constraints = make_constraints(0, vec![0], vec![0x0a].into());
// 		let pending_availability = Vec::new();

// 		let ancestors = vec![RelayChainBlockInfo {
// 			number: pvd_a.relay_parent_number,
// 			hash: relay_parent_a,
// 			storage_root: pvd_a.relay_parent_storage_root,
// 		}];

// 		let relay_parent_b_info = RelayChainBlockInfo {
// 			number: pvd_b.relay_parent_number,
// 			hash: relay_parent_b,
// 			storage_root: pvd_b.relay_parent_storage_root,
// 		};

// 		storage.add_candidate(candidate_a, pvd_a).unwrap();
// 		storage.add_candidate(candidate_b, pvd_b).unwrap();
// 		let scope = Scope::with_ancestors(
// 			para_id,
// 			relay_parent_b_info,
// 			base_constraints,
// 			pending_availability,
// 			4,
// 			ancestors,
// 		)
// 		.unwrap();
// 		let mut tree = FragmentTree::populate(scope, &storage);

// 		storage.add_candidate(candidate_a2, pvd_a2).unwrap();
// 		tree.add_and_populate(candidate_a2_hash, &storage);
// 		let candidates: Vec<_> = tree.candidates().collect();
// 		assert_eq!(candidates.len(), 3);

// 		assert_eq!(tree.nodes[0].parent, NodePointer::Root);
// 		assert_eq!(tree.nodes[1].parent, NodePointer::Root);
// 		assert_eq!(tree.nodes[2].parent, NodePointer::Storage(0));
// 	}

// 	#[test]
// 	fn add_candidate_child_of_root() {
// 		let mut storage = CandidateStorage::new();

// 		let para_id = ParaId::from(5u32);
// 		let relay_parent_a = Hash::repeat_byte(1);

// 		let (pvd_a, candidate_a) = make_committed_candidate(
// 			para_id,
// 			relay_parent_a,
// 			0,
// 			vec![0x0a].into(),
// 			vec![0x0b].into(),
// 			0,
// 		);

// 		let (pvd_b, candidate_b) = make_committed_candidate(
// 			para_id,
// 			relay_parent_a,
// 			0,
// 			vec![0x0a].into(),
// 			vec![0x0c].into(),
// 			0,
// 		);
// 		let candidate_b_hash = candidate_b.hash();

// 		let base_constraints = make_constraints(0, vec![0], vec![0x0a].into());
// 		let pending_availability = Vec::new();

// 		let relay_parent_a_info = RelayChainBlockInfo {
// 			number: pvd_a.relay_parent_number,
// 			hash: relay_parent_a,
// 			storage_root: pvd_a.relay_parent_storage_root,
// 		};

// 		storage.add_candidate(candidate_a, pvd_a).unwrap();
// 		let scope = Scope::with_ancestors(
// 			para_id,
// 			relay_parent_a_info,
// 			base_constraints,
// 			pending_availability,
// 			4,
// 			vec![],
// 		)
// 		.unwrap();
// 		let mut tree = FragmentTree::populate(scope, &storage);

// 		storage.add_candidate(candidate_b, pvd_b).unwrap();
// 		tree.add_and_populate(candidate_b_hash, &storage);
// 		let candidates: Vec<_> = tree.candidates().collect();
// 		assert_eq!(candidates.len(), 2);

// 		assert_eq!(tree.nodes[0].parent, NodePointer::Root);
// 		assert_eq!(tree.nodes[1].parent, NodePointer::Root);
// 	}

// 	#[test]
// 	fn add_candidate_child_of_non_root() {
// 		let mut storage = CandidateStorage::new();

// 		let para_id = ParaId::from(5u32);
// 		let relay_parent_a = Hash::repeat_byte(1);

// 		let (pvd_a, candidate_a) = make_committed_candidate(
// 			para_id,
// 			relay_parent_a,
// 			0,
// 			vec![0x0a].into(),
// 			vec![0x0b].into(),
// 			0,
// 		);

// 		let (pvd_b, candidate_b) = make_committed_candidate(
// 			para_id,
// 			relay_parent_a,
// 			0,
// 			vec![0x0b].into(),
// 			vec![0x0c].into(),
// 			0,
// 		);
// 		let candidate_b_hash = candidate_b.hash();

// 		let base_constraints = make_constraints(0, vec![0], vec![0x0a].into());
// 		let pending_availability = Vec::new();

// 		let relay_parent_a_info = RelayChainBlockInfo {
// 			number: pvd_a.relay_parent_number,
// 			hash: relay_parent_a,
// 			storage_root: pvd_a.relay_parent_storage_root,
// 		};

// 		storage.add_candidate(candidate_a, pvd_a).unwrap();
// 		let scope = Scope::with_ancestors(
// 			para_id,
// 			relay_parent_a_info,
// 			base_constraints,
// 			pending_availability,
// 			4,
// 			vec![],
// 		)
// 		.unwrap();
// 		let mut tree = FragmentTree::populate(scope, &storage);

// 		storage.add_candidate(candidate_b, pvd_b).unwrap();
// 		tree.add_and_populate(candidate_b_hash, &storage);
// 		let candidates: Vec<_> = tree.candidates().collect();
// 		assert_eq!(candidates.len(), 2);

// 		assert_eq!(tree.nodes[0].parent, NodePointer::Root);
// 		assert_eq!(tree.nodes[1].parent, NodePointer::Storage(0));
// 	}

// 	#[test]
// 	fn test_find_ancestor_path_and_find_backable_chain_empty_tree() {
// 		let para_id = ParaId::from(5u32);
// 		let relay_parent = Hash::repeat_byte(1);
// 		let required_parent: HeadData = vec![0xff].into();
// 		let max_depth = 10;

// 		// Empty tree
// 		let storage = CandidateStorage::new();
// 		let base_constraints = make_constraints(0, vec![0], required_parent.clone());

// 		let relay_parent_info =
// 			RelayChainBlockInfo { number: 0, hash: relay_parent, storage_root: Hash::zero() };

// 		let scope = Scope::with_ancestors(
// 			para_id,
// 			relay_parent_info,
// 			base_constraints,
// 			vec![],
// 			max_depth,
// 			vec![],
// 		)
// 		.unwrap();
// 		let tree = FragmentTree::populate(scope, &storage);
// 		assert_eq!(tree.candidates().collect::<Vec<_>>().len(), 0);
// 		assert_eq!(tree.nodes.len(), 0);

// 		assert_eq!(tree.find_ancestor_path(Ancestors::new()).unwrap(), NodePointer::Root);
// 		assert_eq!(tree.find_backable_chain(Ancestors::new(), 2, |_| true), vec![]);
// 		// Invalid candidate.
// 		let ancestors: Ancestors = [CandidateHash::default()].into_iter().collect();
// 		assert_eq!(tree.find_ancestor_path(ancestors.clone()), Some(NodePointer::Root));
// 		assert_eq!(tree.find_backable_chain(ancestors, 2, |_| true), vec![]);
// 	}

// 	#[rstest]
// 	#[case(true, 13)]
// 	#[case(false, 8)]
// 	// The tree with no cycles looks like:
// 	// Make a tree that looks like this (note that there's no cycle):
// 	//         +-(root)-+
// 	//         |        |
// 	//    +----0---+    7
// 	//    |        |
// 	//    1----+   5
// 	//    |    |
// 	//    |    |
// 	//    2    6
// 	//    |
// 	//    3
// 	//    |
// 	//    4
// 	//
// 	// The tree with cycles is the same as the first but has a cycle from 4 back to the state
// 	// produced by 0 (It's bounded by the max_depth + 1).
// 	//         +-(root)-+
// 	//         |        |
// 	//    +----0---+    7
// 	//    |        |
// 	//    1----+   5
// 	//    |    |
// 	//    |    |
// 	//    2    6
// 	//    |
// 	//    3
// 	//    |
// 	//    4---+
// 	//    |   |
// 	//    1   5
// 	//    |
// 	//    2
// 	//    |
// 	//    3
// 	fn test_find_ancestor_path_and_find_backable_chain(
// 		#[case] has_cycle: bool,
// 		#[case] expected_node_count: usize,
// 	) {
// 		let para_id = ParaId::from(5u32);
// 		let relay_parent = Hash::repeat_byte(1);
// 		let required_parent: HeadData = vec![0xff].into();
// 		let max_depth = 7;
// 		let relay_parent_number = 0;
// 		let relay_parent_storage_root = Hash::repeat_byte(69);

// 		let mut candidates = vec![];

// 		// Candidate 0
// 		candidates.push(make_committed_candidate(
// 			para_id,
// 			relay_parent,
// 			0,
// 			required_parent.clone(),
// 			vec![0].into(),
// 			0,
// 		));
// 		// Candidate 1
// 		candidates.push(make_committed_candidate(
// 			para_id,
// 			relay_parent,
// 			0,
// 			vec![0].into(),
// 			vec![1].into(),
// 			0,
// 		));
// 		// Candidate 2
// 		candidates.push(make_committed_candidate(
// 			para_id,
// 			relay_parent,
// 			0,
// 			vec![1].into(),
// 			vec![2].into(),
// 			0,
// 		));
// 		// Candidate 3
// 		candidates.push(make_committed_candidate(
// 			para_id,
// 			relay_parent,
// 			0,
// 			vec![2].into(),
// 			vec![3].into(),
// 			0,
// 		));
// 		// Candidate 4
// 		candidates.push(make_committed_candidate(
// 			para_id,
// 			relay_parent,
// 			0,
// 			vec![3].into(),
// 			vec![4].into(),
// 			0,
// 		));
// 		// Candidate 5
// 		candidates.push(make_committed_candidate(
// 			para_id,
// 			relay_parent,
// 			0,
// 			vec![0].into(),
// 			vec![5].into(),
// 			0,
// 		));
// 		// Candidate 6
// 		candidates.push(make_committed_candidate(
// 			para_id,
// 			relay_parent,
// 			0,
// 			vec![1].into(),
// 			vec![6].into(),
// 			0,
// 		));
// 		// Candidate 7
// 		candidates.push(make_committed_candidate(
// 			para_id,
// 			relay_parent,
// 			0,
// 			required_parent.clone(),
// 			vec![7].into(),
// 			0,
// 		));

// 		if has_cycle {
// 			candidates[4] = make_committed_candidate(
// 				para_id,
// 				relay_parent,
// 				0,
// 				vec![3].into(),
// 				vec![0].into(), // put the cycle here back to the output state of 0.
// 				0,
// 			);
// 		}

// 		let base_constraints = make_constraints(0, vec![0], required_parent.clone());
// 		let mut storage = CandidateStorage::new();

// 		let relay_parent_info = RelayChainBlockInfo {
// 			number: relay_parent_number,
// 			hash: relay_parent,
// 			storage_root: relay_parent_storage_root,
// 		};

// 		for (pvd, candidate) in candidates.iter() {
// 			storage.add_candidate(candidate.clone(), pvd.clone()).unwrap();
// 		}
// 		let candidates =
// 			candidates.into_iter().map(|(_pvd, candidate)| candidate).collect::<Vec<_>>();
// 		let scope = Scope::with_ancestors(
// 			para_id,
// 			relay_parent_info,
// 			base_constraints,
// 			vec![],
// 			max_depth,
// 			vec![],
// 		)
// 		.unwrap();
// 		let tree = FragmentTree::populate(scope, &storage);

// 		assert_eq!(tree.candidates().collect::<Vec<_>>().len(), candidates.len());
// 		assert_eq!(tree.nodes.len(), expected_node_count);

// 		// Do some common tests on both trees.
// 		{
// 			// No ancestors supplied.
// 			assert_eq!(tree.find_ancestor_path(Ancestors::new()).unwrap(), NodePointer::Root);
// 			assert_eq!(
// 				tree.find_backable_chain(Ancestors::new(), 4, |_| true),
// 				[0, 1, 2, 3].into_iter().map(|i| candidates[i].hash()).collect::<Vec<_>>()
// 			);
// 			// Ancestor which is not part of the tree. Will be ignored.
// 			let ancestors: Ancestors = [CandidateHash::default()].into_iter().collect();
// 			assert_eq!(tree.find_ancestor_path(ancestors.clone()).unwrap(), NodePointer::Root);
// 			assert_eq!(
// 				tree.find_backable_chain(ancestors, 4, |_| true),
// 				[0, 1, 2, 3].into_iter().map(|i| candidates[i].hash()).collect::<Vec<_>>()
// 			);
// 			// A chain fork.
// 			let ancestors: Ancestors =
// 				[(candidates[0].hash()), (candidates[7].hash())].into_iter().collect();
// 			assert_eq!(tree.find_ancestor_path(ancestors.clone()), None);
// 			assert_eq!(tree.find_backable_chain(ancestors, 1, |_| true), vec![]);

// 			// Ancestors which are part of the tree but don't form a path. Will be ignored.
// 			let ancestors: Ancestors =
// 				[candidates[1].hash(), candidates[2].hash()].into_iter().collect();
// 			assert_eq!(tree.find_ancestor_path(ancestors.clone()).unwrap(), NodePointer::Root);
// 			assert_eq!(
// 				tree.find_backable_chain(ancestors, 4, |_| true),
// 				[0, 1, 2, 3].into_iter().map(|i| candidates[i].hash()).collect::<Vec<_>>()
// 			);

// 			// Valid ancestors.
// 			let ancestors: Ancestors = [candidates[7].hash()].into_iter().collect();
// 			let res = tree.find_ancestor_path(ancestors.clone()).unwrap();
// 			let candidate = &tree.nodes[res.unwrap_idx()];
// 			assert_eq!(candidate.candidate_hash, candidates[7].hash());
// 			assert_eq!(tree.find_backable_chain(ancestors, 1, |_| true), vec![]);

// 			let ancestors: Ancestors =
// 				[candidates[2].hash(), candidates[0].hash(), candidates[1].hash()]
// 					.into_iter()
// 					.collect();
// 			let res = tree.find_ancestor_path(ancestors.clone()).unwrap();
// 			let candidate = &tree.nodes[res.unwrap_idx()];
// 			assert_eq!(candidate.candidate_hash, candidates[2].hash());
// 			assert_eq!(
// 				tree.find_backable_chain(ancestors.clone(), 2, |_| true),
// 				[3, 4].into_iter().map(|i| candidates[i].hash()).collect::<Vec<_>>()
// 			);

// 			// Valid ancestors with candidates which have been omitted due to timeouts
// 			let ancestors: Ancestors =
// 				[candidates[0].hash(), candidates[2].hash()].into_iter().collect();
// 			let res = tree.find_ancestor_path(ancestors.clone()).unwrap();
// 			let candidate = &tree.nodes[res.unwrap_idx()];
// 			assert_eq!(candidate.candidate_hash, candidates[0].hash());
// 			assert_eq!(
// 				tree.find_backable_chain(ancestors, 3, |_| true),
// 				[1, 2, 3].into_iter().map(|i| candidates[i].hash()).collect::<Vec<_>>()
// 			);

// 			let ancestors: Ancestors =
// 				[candidates[0].hash(), candidates[1].hash(), candidates[3].hash()]
// 					.into_iter()
// 					.collect();
// 			let res = tree.find_ancestor_path(ancestors.clone()).unwrap();
// 			let candidate = &tree.nodes[res.unwrap_idx()];
// 			assert_eq!(candidate.candidate_hash, candidates[1].hash());
// 			if has_cycle {
// 				assert_eq!(
// 					tree.find_backable_chain(ancestors, 2, |_| true),
// 					[2, 3].into_iter().map(|i| candidates[i].hash()).collect::<Vec<_>>()
// 				);
// 			} else {
// 				assert_eq!(
// 					tree.find_backable_chain(ancestors, 4, |_| true),
// 					[2, 3, 4].into_iter().map(|i| candidates[i].hash()).collect::<Vec<_>>()
// 				);
// 			}

// 			let ancestors: Ancestors =
// 				[candidates[1].hash(), candidates[2].hash()].into_iter().collect();
// 			let res = tree.find_ancestor_path(ancestors.clone()).unwrap();
// 			assert_eq!(res, NodePointer::Root);
// 			assert_eq!(
// 				tree.find_backable_chain(ancestors, 4, |_| true),
// 				[0, 1, 2, 3].into_iter().map(|i| candidates[i].hash()).collect::<Vec<_>>()
// 			);

// 			// Requested count is 0.
// 			assert_eq!(tree.find_backable_chain(Ancestors::new(), 0, |_| true), vec![]);

// 			let ancestors: Ancestors =
// 				[candidates[2].hash(), candidates[0].hash(), candidates[1].hash()]
// 					.into_iter()
// 					.collect();
// 			assert_eq!(tree.find_backable_chain(ancestors, 0, |_| true), vec![]);

// 			let ancestors: Ancestors =
// 				[candidates[2].hash(), candidates[0].hash()].into_iter().collect();
// 			assert_eq!(tree.find_backable_chain(ancestors, 0, |_| true), vec![]);
// 		}

// 		// Now do some tests only on the tree with cycles
// 		if has_cycle {
// 			// Exceeds the maximum tree depth. 0-1-2-3-4-1-2-3-4, when the tree stops at
// 			// 0-1-2-3-4-1-2-3.
// 			let ancestors: Ancestors = [
// 				candidates[0].hash(),
// 				candidates[1].hash(),
// 				candidates[2].hash(),
// 				candidates[3].hash(),
// 				candidates[4].hash(),
// 			]
// 			.into_iter()
// 			.collect();
// 			let res = tree.find_ancestor_path(ancestors.clone()).unwrap();
// 			let candidate = &tree.nodes[res.unwrap_idx()];
// 			assert_eq!(candidate.candidate_hash, candidates[4].hash());
// 			assert_eq!(
// 				tree.find_backable_chain(ancestors, 4, |_| true),
// 				[1, 2, 3].into_iter().map(|i| candidates[i].hash()).collect::<Vec<_>>()
// 			);

// 			// 0-1-2.
// 			let ancestors: Ancestors =
// 				[candidates[0].hash(), candidates[1].hash(), candidates[2].hash()]
// 					.into_iter()
// 					.collect();
// 			let res = tree.find_ancestor_path(ancestors.clone()).unwrap();
// 			let candidate = &tree.nodes[res.unwrap_idx()];
// 			assert_eq!(candidate.candidate_hash, candidates[2].hash());
// 			assert_eq!(
// 				tree.find_backable_chain(ancestors.clone(), 1, |_| true),
// 				[3].into_iter().map(|i| candidates[i].hash()).collect::<Vec<_>>()
// 			);
// 			assert_eq!(
// 				tree.find_backable_chain(ancestors, 5, |_| true),
// 				[3, 4, 1, 2, 3].into_iter().map(|i| candidates[i].hash()).collect::<Vec<_>>()
// 			);

// 			// 0-1
// 			let ancestors: Ancestors =
// 				[candidates[0].hash(), candidates[1].hash()].into_iter().collect();
// 			let res = tree.find_ancestor_path(ancestors.clone()).unwrap();
// 			let candidate = &tree.nodes[res.unwrap_idx()];
// 			assert_eq!(candidate.candidate_hash, candidates[1].hash());
// 			assert_eq!(
// 				tree.find_backable_chain(ancestors, 6, |_| true),
// 				[2, 3, 4, 1, 2, 3].into_iter().map(|i| candidates[i].hash()).collect::<Vec<_>>(),
// 			);

// 			// For 0-1-2-3-4-5, there's more than 1 way of finding this path in
// 			// the tree. `None` should be returned. The runtime should not have accepted this.
// 			let ancestors: Ancestors = [
// 				candidates[0].hash(),
// 				candidates[1].hash(),
// 				candidates[2].hash(),
// 				candidates[3].hash(),
// 				candidates[4].hash(),
// 				candidates[5].hash(),
// 			]
// 			.into_iter()
// 			.collect();
// 			let res = tree.find_ancestor_path(ancestors.clone());
// 			assert_eq!(res, None);
// 			assert_eq!(tree.find_backable_chain(ancestors, 1, |_| true), vec![]);
// 		}
// 	}

// 	#[test]
// 	fn graceful_cycle_of_0() {
// 		let mut storage = CandidateStorage::new();

// 		let para_id = ParaId::from(5u32);
// 		let relay_parent_a = Hash::repeat_byte(1);

// 		let (pvd_a, candidate_a) = make_committed_candidate(
// 			para_id,
// 			relay_parent_a,
// 			0,
// 			vec![0x0a].into(),
// 			vec![0x0a].into(), // input same as output
// 			0,
// 		);
// 		let candidate_a_hash = candidate_a.hash();
// 		let base_constraints = make_constraints(0, vec![0], vec![0x0a].into());
// 		let pending_availability = Vec::new();

// 		let relay_parent_a_info = RelayChainBlockInfo {
// 			number: pvd_a.relay_parent_number,
// 			hash: relay_parent_a,
// 			storage_root: pvd_a.relay_parent_storage_root,
// 		};

// 		let max_depth = 4;
// 		storage.add_candidate(candidate_a, pvd_a).unwrap();
// 		let scope = Scope::with_ancestors(
// 			para_id,
// 			relay_parent_a_info,
// 			base_constraints,
// 			pending_availability,
// 			max_depth,
// 			vec![],
// 		)
// 		.unwrap();
// 		let tree = FragmentTree::populate(scope, &storage);

// 		let candidates: Vec<_> = tree.candidates().collect();
// 		assert_eq!(candidates.len(), 1);
// 		assert_eq!(tree.nodes.len(), max_depth + 1);

// 		assert_eq!(tree.nodes[0].parent, NodePointer::Root);
// 		assert_eq!(tree.nodes[1].parent, NodePointer::Storage(0));
// 		assert_eq!(tree.nodes[2].parent, NodePointer::Storage(1));
// 		assert_eq!(tree.nodes[3].parent, NodePointer::Storage(2));
// 		assert_eq!(tree.nodes[4].parent, NodePointer::Storage(3));

// 		assert_eq!(tree.nodes[0].candidate_hash, candidate_a_hash);
// 		assert_eq!(tree.nodes[1].candidate_hash, candidate_a_hash);
// 		assert_eq!(tree.nodes[2].candidate_hash, candidate_a_hash);
// 		assert_eq!(tree.nodes[3].candidate_hash, candidate_a_hash);
// 		assert_eq!(tree.nodes[4].candidate_hash, candidate_a_hash);

// 		for count in 1..10 {
// 			assert_eq!(
// 				tree.find_backable_chain(Ancestors::new(), count, |_| true),
// 				iter::repeat(candidate_a_hash)
// 					.take(std::cmp::min(count as usize, max_depth + 1))
// 					.collect::<Vec<_>>()
// 			);
// 			assert_eq!(
// 				tree.find_backable_chain(
// 					[candidate_a_hash].into_iter().collect(),
// 					count - 1,
// 					|_| true
// 				),
// 				iter::repeat(candidate_a_hash)
// 					.take(std::cmp::min(count as usize - 1, max_depth))
// 					.collect::<Vec<_>>()
// 			);
// 		}
// 	}

// 	#[test]
// 	fn graceful_cycle_of_1() {
// 		let mut storage = CandidateStorage::new();

// 		let para_id = ParaId::from(5u32);
// 		let relay_parent_a = Hash::repeat_byte(1);

// 		let (pvd_a, candidate_a) = make_committed_candidate(
// 			para_id,
// 			relay_parent_a,
// 			0,
// 			vec![0x0a].into(),
// 			vec![0x0b].into(), // input same as output
// 			0,
// 		);
// 		let candidate_a_hash = candidate_a.hash();

// 		let (pvd_b, candidate_b) = make_committed_candidate(
// 			para_id,
// 			relay_parent_a,
// 			0,
// 			vec![0x0b].into(),
// 			vec![0x0a].into(), // input same as output
// 			0,
// 		);
// 		let candidate_b_hash = candidate_b.hash();

// 		let base_constraints = make_constraints(0, vec![0], vec![0x0a].into());
// 		let pending_availability = Vec::new();

// 		let relay_parent_a_info = RelayChainBlockInfo {
// 			number: pvd_a.relay_parent_number,
// 			hash: relay_parent_a,
// 			storage_root: pvd_a.relay_parent_storage_root,
// 		};

// 		let max_depth = 4;
// 		storage.add_candidate(candidate_a, pvd_a).unwrap();
// 		storage.add_candidate(candidate_b, pvd_b).unwrap();
// 		let scope = Scope::with_ancestors(
// 			para_id,
// 			relay_parent_a_info,
// 			base_constraints,
// 			pending_availability,
// 			max_depth,
// 			vec![],
// 		)
// 		.unwrap();
// 		let tree = FragmentTree::populate(scope, &storage);

// 		let candidates: Vec<_> = tree.candidates().collect();
// 		assert_eq!(candidates.len(), 2);
// 		assert_eq!(tree.nodes.len(), max_depth + 1);

// 		assert_eq!(tree.nodes[0].parent, NodePointer::Root);
// 		assert_eq!(tree.nodes[1].parent, NodePointer::Storage(0));
// 		assert_eq!(tree.nodes[2].parent, NodePointer::Storage(1));
// 		assert_eq!(tree.nodes[3].parent, NodePointer::Storage(2));
// 		assert_eq!(tree.nodes[4].parent, NodePointer::Storage(3));

// 		assert_eq!(tree.nodes[0].candidate_hash, candidate_a_hash);
// 		assert_eq!(tree.nodes[1].candidate_hash, candidate_b_hash);
// 		assert_eq!(tree.nodes[2].candidate_hash, candidate_a_hash);
// 		assert_eq!(tree.nodes[3].candidate_hash, candidate_b_hash);
// 		assert_eq!(tree.nodes[4].candidate_hash, candidate_a_hash);

// 		assert_eq!(tree.find_backable_chain(Ancestors::new(), 1, |_| true), vec![candidate_a_hash],);
// 		assert_eq!(
// 			tree.find_backable_chain(Ancestors::new(), 2, |_| true),
// 			vec![candidate_a_hash, candidate_b_hash],
// 		);
// 		assert_eq!(
// 			tree.find_backable_chain(Ancestors::new(), 3, |_| true),
// 			vec![candidate_a_hash, candidate_b_hash, candidate_a_hash],
// 		);
// 		assert_eq!(
// 			tree.find_backable_chain([candidate_a_hash].into_iter().collect(), 2, |_| true),
// 			vec![candidate_b_hash, candidate_a_hash],
// 		);

// 		assert_eq!(
// 			tree.find_backable_chain(Ancestors::new(), 6, |_| true),
// 			vec![
// 				candidate_a_hash,
// 				candidate_b_hash,
// 				candidate_a_hash,
// 				candidate_b_hash,
// 				candidate_a_hash
// 			],
// 		);

// 		for count in 3..7 {
// 			assert_eq!(
// 				tree.find_backable_chain(
// 					[candidate_a_hash, candidate_b_hash].into_iter().collect(),
// 					count,
// 					|_| true
// 				),
// 				vec![candidate_a_hash, candidate_b_hash, candidate_a_hash],
// 			);
// 		}
// 	}

// 	#[test]
// 	fn hypothetical_depths_known_and_unknown() {
// 		let mut storage = CandidateStorage::new();

// 		let para_id = ParaId::from(5u32);
// 		let relay_parent_a = Hash::repeat_byte(1);

// 		let (pvd_a, candidate_a) = make_committed_candidate(
// 			para_id,
// 			relay_parent_a,
// 			0,
// 			vec![0x0a].into(),
// 			vec![0x0b].into(), // input same as output
// 			0,
// 		);
// 		let candidate_a_hash = candidate_a.hash();

// 		let (pvd_b, candidate_b) = make_committed_candidate(
// 			para_id,
// 			relay_parent_a,
// 			0,
// 			vec![0x0b].into(),
// 			vec![0x0a].into(), // input same as output
// 			0,
// 		);
// 		let candidate_b_hash = candidate_b.hash();

// 		let base_constraints = make_constraints(0, vec![0], vec![0x0a].into());
// 		let pending_availability = Vec::new();

// 		let relay_parent_a_info = RelayChainBlockInfo {
// 			number: pvd_a.relay_parent_number,
// 			hash: relay_parent_a,
// 			storage_root: pvd_a.relay_parent_storage_root,
// 		};

// 		let max_depth = 4;
// 		storage.add_candidate(candidate_a, pvd_a).unwrap();
// 		storage.add_candidate(candidate_b, pvd_b).unwrap();
// 		let scope = Scope::with_ancestors(
// 			para_id,
// 			relay_parent_a_info,
// 			base_constraints,
// 			pending_availability,
// 			max_depth,
// 			vec![],
// 		)
// 		.unwrap();
// 		let tree = FragmentTree::populate(scope, &storage);

// 		let candidates: Vec<_> = tree.candidates().collect();
// 		assert_eq!(candidates.len(), 2);
// 		assert_eq!(tree.nodes.len(), max_depth + 1);

// 		assert_eq!(
// 			tree.hypothetical_depths(
// 				candidate_a_hash,
// 				HypotheticalCandidate::Incomplete {
// 					parent_head_data_hash: HeadData::from(vec![0x0a]).hash(),
// 					relay_parent: relay_parent_a,
// 				},
// 				&storage,
// 				false,
// 			),
// 			vec![0, 2, 4],
// 		);

// 		assert_eq!(
// 			tree.hypothetical_depths(
// 				candidate_b_hash,
// 				HypotheticalCandidate::Incomplete {
// 					parent_head_data_hash: HeadData::from(vec![0x0b]).hash(),
// 					relay_parent: relay_parent_a,
// 				},
// 				&storage,
// 				false,
// 			),
// 			vec![1, 3],
// 		);

// 		assert_eq!(
// 			tree.hypothetical_depths(
// 				CandidateHash(Hash::repeat_byte(21)),
// 				HypotheticalCandidate::Incomplete {
// 					parent_head_data_hash: HeadData::from(vec![0x0a]).hash(),
// 					relay_parent: relay_parent_a,
// 				},
// 				&storage,
// 				false,
// 			),
// 			vec![0, 2, 4],
// 		);

// 		assert_eq!(
// 			tree.hypothetical_depths(
// 				CandidateHash(Hash::repeat_byte(22)),
// 				HypotheticalCandidate::Incomplete {
// 					parent_head_data_hash: HeadData::from(vec![0x0b]).hash(),
// 					relay_parent: relay_parent_a,
// 				},
// 				&storage,
// 				false,
// 			),
// 			vec![1, 3]
// 		);
// 	}

// 	#[test]
// 	fn hypothetical_depths_stricter_on_complete() {
// 		let storage = CandidateStorage::new();

// 		let para_id = ParaId::from(5u32);
// 		let relay_parent_a = Hash::repeat_byte(1);

// 		let (pvd_a, candidate_a) = make_committed_candidate(
// 			para_id,
// 			relay_parent_a,
// 			0,
// 			vec![0x0a].into(),
// 			vec![0x0b].into(),
// 			1000, // watermark is illegal
// 		);

// 		let candidate_a_hash = candidate_a.hash();

// 		let base_constraints = make_constraints(0, vec![0], vec![0x0a].into());
// 		let pending_availability = Vec::new();

// 		let relay_parent_a_info = RelayChainBlockInfo {
// 			number: pvd_a.relay_parent_number,
// 			hash: relay_parent_a,
// 			storage_root: pvd_a.relay_parent_storage_root,
// 		};

// 		let max_depth = 4;
// 		let scope = Scope::with_ancestors(
// 			para_id,
// 			relay_parent_a_info,
// 			base_constraints,
// 			pending_availability,
// 			max_depth,
// 			vec![],
// 		)
// 		.unwrap();
// 		let tree = FragmentTree::populate(scope, &storage);

// 		assert_eq!(
// 			tree.hypothetical_depths(
// 				candidate_a_hash,
// 				HypotheticalCandidate::Incomplete {
// 					parent_head_data_hash: HeadData::from(vec![0x0a]).hash(),
// 					relay_parent: relay_parent_a,
// 				},
// 				&storage,
// 				false,
// 			),
// 			vec![0],
// 		);

// 		assert!(tree
// 			.hypothetical_depths(
// 				candidate_a_hash,
// 				HypotheticalCandidate::Complete {
// 					receipt: Cow::Owned(candidate_a),
// 					persisted_validation_data: Cow::Owned(pvd_a),
// 				},
// 				&storage,
// 				false,
// 			)
// 			.is_empty());
// 	}

// 	#[test]
// 	fn hypothetical_depths_backed_in_path() {
// 		let mut storage = CandidateStorage::new();

// 		let para_id = ParaId::from(5u32);
// 		let relay_parent_a = Hash::repeat_byte(1);

// 		let (pvd_a, candidate_a) = make_committed_candidate(
// 			para_id,
// 			relay_parent_a,
// 			0,
// 			vec![0x0a].into(),
// 			vec![0x0b].into(),
// 			0,
// 		);
// 		let candidate_a_hash = candidate_a.hash();

// 		let (pvd_b, candidate_b) = make_committed_candidate(
// 			para_id,
// 			relay_parent_a,
// 			0,
// 			vec![0x0b].into(),
// 			vec![0x0c].into(),
// 			0,
// 		);
// 		let candidate_b_hash = candidate_b.hash();

// 		let (pvd_c, candidate_c) = make_committed_candidate(
// 			para_id,
// 			relay_parent_a,
// 			0,
// 			vec![0x0b].into(),
// 			vec![0x0d].into(),
// 			0,
// 		);

// 		let base_constraints = make_constraints(0, vec![0], vec![0x0a].into());
// 		let pending_availability = Vec::new();

// 		let relay_parent_a_info = RelayChainBlockInfo {
// 			number: pvd_a.relay_parent_number,
// 			hash: relay_parent_a,
// 			storage_root: pvd_a.relay_parent_storage_root,
// 		};

// 		let max_depth = 4;
// 		storage.add_candidate(candidate_a, pvd_a).unwrap();
// 		storage.add_candidate(candidate_b, pvd_b).unwrap();
// 		storage.add_candidate(candidate_c, pvd_c).unwrap();

// 		// `A` and `B` are backed, `C` is not.
// 		storage.mark_backed(&candidate_a_hash);
// 		storage.mark_backed(&candidate_b_hash);

// 		let scope = Scope::with_ancestors(
// 			para_id,
// 			relay_parent_a_info,
// 			base_constraints,
// 			pending_availability,
// 			max_depth,
// 			vec![],
// 		)
// 		.unwrap();
// 		let tree = FragmentTree::populate(scope, &storage);

// 		let candidates: Vec<_> = tree.candidates().collect();
// 		assert_eq!(candidates.len(), 3);
// 		assert_eq!(tree.nodes.len(), 3);

// 		let candidate_d_hash = CandidateHash(Hash::repeat_byte(0xAA));

// 		assert_eq!(
// 			tree.hypothetical_depths(
// 				candidate_d_hash,
// 				HypotheticalCandidate::Incomplete {
// 					parent_head_data_hash: HeadData::from(vec![0x0a]).hash(),
// 					relay_parent: relay_parent_a,
// 				},
// 				&storage,
// 				true,
// 			),
// 			vec![0],
// 		);

// 		assert_eq!(
// 			tree.hypothetical_depths(
// 				candidate_d_hash,
// 				HypotheticalCandidate::Incomplete {
// 					parent_head_data_hash: HeadData::from(vec![0x0c]).hash(),
// 					relay_parent: relay_parent_a,
// 				},
// 				&storage,
// 				true,
// 			),
// 			vec![2],
// 		);

// 		assert_eq!(
// 			tree.hypothetical_depths(
// 				candidate_d_hash,
// 				HypotheticalCandidate::Incomplete {
// 					parent_head_data_hash: HeadData::from(vec![0x0d]).hash(),
// 					relay_parent: relay_parent_a,
// 				},
// 				&storage,
// 				true,
// 			),
// 			Vec::<usize>::new(),
// 		);

// 		assert_eq!(
// 			tree.hypothetical_depths(
// 				candidate_d_hash,
// 				HypotheticalCandidate::Incomplete {
// 					parent_head_data_hash: HeadData::from(vec![0x0d]).hash(),
// 					relay_parent: relay_parent_a,
// 				},
// 				&storage,
// 				false,
// 			),
// 			vec![2], // non-empty if `false`.
// 		);
// 	}

// 	#[test]
// 	fn pending_availability_in_scope() {
// 		let mut storage = CandidateStorage::new();

// 		let para_id = ParaId::from(5u32);
// 		let relay_parent_a = Hash::repeat_byte(1);
// 		let relay_parent_b = Hash::repeat_byte(2);
// 		let relay_parent_c = Hash::repeat_byte(3);

// 		let (pvd_a, candidate_a) = make_committed_candidate(
// 			para_id,
// 			relay_parent_a,
// 			0,
// 			vec![0x0a].into(),
// 			vec![0x0b].into(),
// 			0,
// 		);
// 		let candidate_a_hash = candidate_a.hash();

// 		let (pvd_b, candidate_b) = make_committed_candidate(
// 			para_id,
// 			relay_parent_b,
// 			1,
// 			vec![0x0b].into(),
// 			vec![0x0c].into(),
// 			1,
// 		);

// 		// Note that relay parent `a` is not allowed.
// 		let base_constraints = make_constraints(1, vec![], vec![0x0a].into());

// 		let relay_parent_a_info = RelayChainBlockInfo {
// 			number: pvd_a.relay_parent_number,
// 			hash: relay_parent_a,
// 			storage_root: pvd_a.relay_parent_storage_root,
// 		};
// 		let pending_availability = vec![PendingAvailability {
// 			candidate_hash: candidate_a_hash,
// 			relay_parent: relay_parent_a_info,
// 		}];

// 		let relay_parent_b_info = RelayChainBlockInfo {
// 			number: pvd_b.relay_parent_number,
// 			hash: relay_parent_b,
// 			storage_root: pvd_b.relay_parent_storage_root,
// 		};
// 		let relay_parent_c_info = RelayChainBlockInfo {
// 			number: pvd_b.relay_parent_number + 1,
// 			hash: relay_parent_c,
// 			storage_root: Hash::zero(),
// 		};

// 		let max_depth = 4;
// 		storage.add_candidate(candidate_a, pvd_a).unwrap();
// 		storage.add_candidate(candidate_b, pvd_b).unwrap();
// 		storage.mark_backed(&candidate_a_hash);

// 		let scope = Scope::with_ancestors(
// 			para_id,
// 			relay_parent_c_info,
// 			base_constraints,
// 			pending_availability,
// 			max_depth,
// 			vec![relay_parent_b_info],
// 		)
// 		.unwrap();
// 		let tree = FragmentTree::populate(scope, &storage);

// 		let candidates: Vec<_> = tree.candidates().collect();
// 		assert_eq!(candidates.len(), 2);
// 		assert_eq!(tree.nodes.len(), 2);

// 		let candidate_d_hash = CandidateHash(Hash::repeat_byte(0xAA));

// 		assert_eq!(
// 			tree.hypothetical_depths(
// 				candidate_d_hash,
// 				HypotheticalCandidate::Incomplete {
// 					parent_head_data_hash: HeadData::from(vec![0x0b]).hash(),
// 					relay_parent: relay_parent_c,
// 				},
// 				&storage,
// 				false,
// 			),
// 			vec![1],
// 		);

// 		assert_eq!(
// 			tree.hypothetical_depths(
// 				candidate_d_hash,
// 				HypotheticalCandidate::Incomplete {
// 					parent_head_data_hash: HeadData::from(vec![0x0c]).hash(),
// 					relay_parent: relay_parent_b,
// 				},
// 				&storage,
// 				false,
// 			),
// 			vec![2],
// 		);
// 	}
// }
