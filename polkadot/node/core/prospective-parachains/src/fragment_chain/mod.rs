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
//! backed soon enough. This is commonly called a potential candidate. Note that not all potential
//! candidates will be maintained in the CandidateStorage. The total number of connected + potential
//! candidates will be at most max_candidate_depth + 1.
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

#[cfg(test)]
mod tests;

use std::{
	collections::{
		hash_map::{Entry, HashMap},
		BTreeMap, HashSet,
	},
	sync::Arc,
};

use super::LOG_TARGET;
use polkadot_node_subsystem::messages::{Ancestors, HypotheticalCandidate};
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
#[derive(Clone, Default)]
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
			candidate: Arc::new(ProspectiveCandidate {
				commitments: candidate.commitments,
				collator: candidate.descriptor.collator,
				collator_signature: candidate.descriptor.signature,
				persisted_validation_data,
				pov_hash: candidate.descriptor.pov_hash,
				validation_code_hash: candidate.descriptor.validation_code_hash,
			}),
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
	pub(crate) fn relay_parent_of_candidate(&self, candidate_hash: &CandidateHash) -> Option<Hash> {
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
#[derive(Debug, PartialEq, Clone)]
pub(crate) enum CandidateState {
	/// The candidate has been seconded.
	Seconded,
	/// The candidate has been completely backed by the group.
	Backed,
}

#[derive(Debug, Clone)]
pub(crate) struct CandidateEntry {
	candidate_hash: CandidateHash,
	parent_head_data_hash: Hash,
	output_head_data_hash: Hash,
	relay_parent: Hash,
	candidate: Arc<ProspectiveCandidate>,
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
#[derive(Debug, Clone)]
pub(crate) struct Scope {
	/// The assigned para id of this `FragmentChain`.
	para: ParaId,
	/// The relay parent we're currently building on top of.
	relay_parent: RelayChainBlockInfo,
	/// The other relay parents candidates are allowed to build upon, mapped by the block number.
	ancestors: BTreeMap<BlockNumber, RelayChainBlockInfo>,
	/// The other relay parents candidates are allowed to build upon, mapped by the block hash.
	ancestors_by_hash: HashMap<Hash, RelayChainBlockInfo>,
	/// The candidates pending availability at this block.
	pending_availability: Vec<PendingAvailability>,
	/// The base constraints derived from the latest included candidate.
	base_constraints: Constraints,
	/// Equal to `max_candidate_depth`.
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
	pub fn ancestor(&self, hash: &Hash) -> Option<RelayChainBlockInfo> {
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

pub struct FragmentNode {
	fragment: Fragment,
	candidate_hash: CandidateHash,
	cumulative_modifications: ConstraintModifications,
}

impl FragmentNode {
	fn relay_parent(&self) -> Hash {
		self.fragment.relay_parent().hash
	}
}

/// Response given by `can_add_candidate_as_potential`
#[derive(PartialEq, Debug)]
pub enum PotentialAddition {
	/// Can be added as either connected or unconnected candidate.
	Anyhow,
	/// Can only be added as a connected candidate to the chain.
	IfConnected,
	/// Cannot be added.
	None,
}

/// This is a chain of candidates based on some underlying storage of candidates and a scope.
///
/// All nodes in the chain must be either pending availability or within the scope. Within the scope
/// means it's built off of the relay-parent or an ancestor.
pub(crate) struct FragmentChain {
	scope: Scope,

	chain: Vec<FragmentNode>,

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

	/// Returns the number of candidates in the chain
	pub(crate) fn len(&self) -> usize {
		self.candidates.len()
	}

	/// Whether the candidate exists.
	pub(crate) fn contains_candidate(&self, candidate: &CandidateHash) -> bool {
		self.candidates.contains(candidate)
	}

	/// Return a vector of the chain's candidate hashes, in-order.
	pub(crate) fn to_vec(&self) -> Vec<CandidateHash> {
		self.chain.iter().map(|candidate| candidate.candidate_hash).collect()
	}

	/// Try accumulating more candidates onto the chain.
	///
	/// Candidates can only be added if they build on the already existing chain.
	pub(crate) fn extend_from_storage(&mut self, storage: &CandidateStorage) {
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
		candidate: HypotheticalCandidate,
		candidate_storage: &CandidateStorage,
	) -> bool {
		let candidate_hash = candidate.candidate_hash();

		// If we've already used this candidate in the chain
		if self.candidates.contains(&candidate_hash) {
			return true
		}

		let can_add_as_potential = self.can_add_candidate_as_potential(
			candidate_storage,
			&candidate.candidate_hash(),
			&candidate.relay_parent(),
			candidate.parent_head_data_hash(),
			candidate.output_head_data_hash(),
		);

		if can_add_as_potential == PotentialAddition::None {
			return false
		}

		let Some(candidate_relay_parent) = self.scope.ancestor(&candidate.relay_parent()) else {
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
			if let HypotheticalCandidate::Complete {
				ref receipt,
				ref persisted_validation_data,
				..
			} = candidate
			{
				if Fragment::check_against_constraints(
					&candidate_relay_parent,
					&child_constraints,
					&receipt.commitments,
					&receipt.descriptor().validation_code_hash,
					persisted_validation_data,
				)
				.is_err()
				{
					gum::debug!(
						target: LOG_TARGET,
						"Fragment::check_against_constraints() returned error",
					);
					return false
				}
			}

			// If we got this far, it can be added to the chain right now.
			true
		} else if can_add_as_potential == PotentialAddition::Anyhow {
			// Otherwise it is or can be an unconnected candidate, but only if PotentialAddition
			// does not force us to only add a connected candidate.
			true
		} else {
			false
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
	// `None` is returned if the candidate's relay parent info cannot be found.
	fn earliest_relay_parent(&self) -> Option<RelayChainBlockInfo> {
		if let Some(last_candidate) = self.chain.last() {
			self.scope.ancestor(&last_candidate.relay_parent()).or_else(|| {
				// if the relay-parent is out of scope _and_ it is in the chain,
				// it must be a candidate pending availability.
				self.scope
					.get_pending_availability(&last_candidate.candidate_hash)
					.map(|c| c.relay_parent.clone())
			})
		} else {
			Some(self.scope.earliest_relay_parent())
		}
	}

	// Checks if this candidate could be added in the future to this chain.
	// This assumes that the chain does not already contain this candidate. It may or may not be
	// present in the `CandidateStorage`.
	// Even if the candidate is a potential candidate, this function will indicate that it can be
	// kept only if there's enough room for it.
	pub(crate) fn can_add_candidate_as_potential(
		&self,
		storage: &CandidateStorage,
		candidate_hash: &CandidateHash,
		relay_parent: &Hash,
		parent_head_hash: Hash,
		output_head_hash: Option<Hash>,
	) -> PotentialAddition {
		// If we've got enough candidates for the configured depth, no point in adding more.
		if self.chain.len() > self.scope.max_depth {
			return PotentialAddition::None
		}

		if !self.check_potential(relay_parent, parent_head_hash, output_head_hash) {
			return PotentialAddition::None
		}

		let present_in_storage = storage.contains(candidate_hash);

		let unconnected = self
			.find_unconnected_potential_candidates(
				storage,
				present_in_storage.then_some(candidate_hash),
			)
			.len();

		if (self.chain.len() + unconnected) < self.scope.max_depth {
			PotentialAddition::Anyhow
		} else if (self.chain.len() + unconnected) == self.scope.max_depth {
			// If we've only one slot left to fill, it must be filled with a connected candidate.
			PotentialAddition::IfConnected
		} else {
			PotentialAddition::None
		}
	}

	// The candidates which are present in `CandidateStorage`, are not part of this chain but could
	// become part of this chain in the future. Capped at the max depth minus the existing chain
	// length.
	// If `ignore_candidate` is supplied and found in storage, it won't be counted.
	pub(crate) fn find_unconnected_potential_candidates(
		&self,
		storage: &CandidateStorage,
		ignore_candidate: Option<&CandidateHash>,
	) -> Vec<CandidateHash> {
		let mut candidates = vec![];
		for candidate in storage.candidates() {
			if let Some(ignore_candidate) = ignore_candidate {
				if ignore_candidate == &candidate.candidate_hash {
					continue
				}
			}
			// We stop at max_depth + 1 with the search. There's no point in looping further.
			if (self.chain.len() + candidates.len()) > self.scope.max_depth {
				break
			}
			if !self.candidates.contains(&candidate.candidate_hash) &&
				self.check_potential(
					&candidate.relay_parent,
					candidate.candidate.persisted_validation_data.parent_head.hash(),
					Some(candidate.candidate.commitments.head_data.hash()),
				) {
				candidates.push(candidate.candidate_hash);
			}
		}

		candidates
	}

	// Check if adding a candidate which transitions `parent_head_hash` to `output_head_hash` would
	// introduce a fork or a cycle in the parachain.
	// `output_head_hash` is optional because we sometimes make this check before retrieving the
	// collation.
	fn is_fork_or_cycle(&self, parent_head_hash: Hash, output_head_hash: Option<Hash>) -> bool {
		if self.by_parent_head.contains_key(&parent_head_hash) {
			// fork. our parent has another child already
			return true
		}

		if let Some(output_head_hash) = output_head_hash {
			if self.by_output_head.contains_key(&output_head_hash) {
				// this is not a chain, there are multiple paths to the same state.
				return true
			}

			// trivial 0-length cycle.
			if parent_head_hash == output_head_hash {
				return true
			}

			// this should catch any other cycles. our output state cannot already be the parent
			// state of another candidate, unless this is a cycle, since the already added
			// candidates form a chain.
			if self.by_parent_head.contains_key(&output_head_hash) {
				return true
			}
		}

		false
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
		if self.is_fork_or_cycle(parent_head_hash, output_head_hash) {
			return false
		}

		let Some(earliest_rp) = self.earliest_relay_parent() else { return false };

		let Some(relay_parent) = self.scope.ancestor(relay_parent) else { return false };

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
		let Some(mut earliest_rp) = self.earliest_relay_parent() else { return };

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

				if self.is_fork_or_cycle(
					candidate.parent_head_data_hash(),
					Some(candidate.output_head_data_hash()),
				) {
					continue
				}

				let pending = self.scope.get_pending_availability(&candidate.candidate_hash);
				let Some(relay_parent) = pending
					.map(|p| p.relay_parent.clone())
					.or_else(|| self.scope.ancestor(&candidate.relay_parent))
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
						// It's cheap to clone because it's wrapped in an Arc
						candidate.candidate.clone(),
					);

					match f {
						Ok(f) => f,
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
