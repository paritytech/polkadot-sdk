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
	cmp::Ordering,
	collections::{
		hash_map::{Entry, HashMap},
		BTreeMap, HashSet, VecDeque,
	},
	sync::Arc,
};

use super::LOG_TARGET;
use polkadot_node_subsystem::messages::{Ancestors, HypotheticalCandidate};
use polkadot_node_subsystem_util::inclusion_emulator::{
	ConstraintModifications, Constraints, Fragment, HypotheticalOrConcreteCandidate,
	ProspectiveCandidate, RelayChainBlockInfo,
};
use polkadot_primitives::{
	BlockNumber, CandidateHash, CommittedCandidateReceipt, Hash, HeadData, Id as ParaId,
	PersistedValidationData,
};

const EXTRA_UNCONNECTED_COUNT: usize = 10;

/// Kinds of failures to import a candidate into storage.
#[derive(Debug, Clone, PartialEq)]
pub enum CandidateStorageInsertionError {
	/// An error indicating that a supplied candidate didn't match the persisted
	/// validation data provided alongside it.
	PersistedValidationDataMismatch,
	/// The candidate was already known.
	CandidateAlreadyKnown(CandidateHash),
	/// TODO
	Cycle,
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
	// TODO: I don't think we can have multiple candidates with the same output head really.

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
	) -> Result<(), CandidateStorageInsertionError> {
		// Here, do not allow a cycle.
		let candidate_hash = candidate.hash();
		if self.by_candidate_hash.contains_key(&candidate_hash) {
			return Err(CandidateStorageInsertionError::CandidateAlreadyKnown(candidate_hash))
		}

		let entry =
			CandidateEntry::new(candidate_hash, candidate, persisted_validation_data, state)?;

		// if self.is_cycle(entry.parent_head_data_hash(), entry.output_head_data_hash()) {
		// 	return Err(CandidateStorageInsertionError::Cycle)
		// }

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

		Ok(())
	}

	fn add_candidate_entry(
		&mut self,
		candidate: CandidateEntry,
	) -> Result<(), CandidateStorageInsertionError> {
		let candidate_hash = candidate.candidate_hash;
		if self.by_candidate_hash.contains_key(&candidate_hash) {
			return Err(CandidateStorageInsertionError::CandidateAlreadyKnown(candidate_hash))
		}

		// if self.is_cycle(candidate.parent_head_data_hash(), candidate.output_head_data_hash()) {
		// 	return Err(CandidateStorageInsertionError::Cycle)
		// }

		self.by_parent_head
			.entry(candidate.parent_head_data_hash())
			.or_default()
			.insert(candidate_hash);
		self.by_output_head
			.entry(candidate.output_head_data_hash())
			.or_default()
			.insert(candidate_hash);
		// sanity-checked already.
		self.by_candidate_hash.insert(candidate_hash, candidate);

		Ok(())
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
	fn mark_backed(&mut self, candidate_hash: &CandidateHash) -> bool {
		if let Some(entry) = self.by_candidate_hash.get_mut(candidate_hash) {
			gum::trace!(target: LOG_TARGET, ?candidate_hash, "Candidate marked as backed");
			entry.state = CandidateState::Backed;
			true
		} else {
			gum::trace!(target: LOG_TARGET, ?candidate_hash, "Candidate not found while marking as backed");
			false
		}
	}

	/// Whether a candidate is contained within the storage already.
	pub fn contains(&self, candidate_hash: &CandidateHash) -> bool {
		self.by_candidate_hash.contains_key(candidate_hash)
	}

	/// Return an iterator over the stored candidates.
	pub fn candidates(&self) -> impl Iterator<Item = &CandidateEntry> {
		self.by_candidate_hash.values()
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

	/// Returns the candidates which have the given head data hash as parent.
	/// We don't allow forks in a parachain, but we may have multiple candidates with same parent
	/// across different relay chain forks. That's why it returns an iterator (but only one will be
	/// valid and used in the end).
	fn possible_backed_para_children<'a>(
		&'a self,
		parent_head_hash: &'a Hash,
	) -> impl Iterator<Item = &'a CandidateEntry> + 'a {
		let by_candidate_hash = &self.by_candidate_hash;
		self.by_parent_head
			.get(parent_head_hash)
			.into_iter()
			.flat_map(|hashes| hashes.iter())
			.filter_map(move |h| {
				by_candidate_hash.get(h).and_then(|candidate| {
					(candidate.state == CandidateState::Backed).then_some(candidate)
				})
			})
	}

	pub fn len(&self) -> usize {
		self.by_candidate_hash.len()
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
	pub fn new(
		candidate_hash: CandidateHash,
		candidate: CommittedCandidateReceipt,
		persisted_validation_data: PersistedValidationData,
		state: CandidateState,
	) -> Result<Self, CandidateStorageInsertionError> {
		if persisted_validation_data.hash() != candidate.descriptor.persisted_validation_data_hash {
			return Err(CandidateStorageInsertionError::PersistedValidationDataMismatch)
		}

		Ok(Self {
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
		})
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
	parent_head_data_hash: Hash,
	output_head_data_hash: Hash,
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

	chain: Vec<FragmentNode>,

	candidates: HashSet<CandidateHash>,

	// Index from head data hash to the candidate hash with that head data as a parent.
	by_parent_head: HashMap<Hash, CandidateHash>,
	// Index from head data hash to candidate hashes outputting that head data.
	by_output_head: HashMap<Hash, CandidateHash>,

	unconnected: CandidateStorage,
}

impl FragmentChain {
	/// Create a new [`FragmentChain`] with given scope and populated from the storage.
	pub fn populate(scope: Scope, parent_candidates: &mut CandidateStorage) -> Self {
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
			unconnected: CandidateStorage::default(),
		};

		fragment_chain.populate_chain(parent_candidates);

		// Trim the forks that we know can no longer make it on-chain.
		fragment_chain.trim_uneligible_forks(parent_candidates);

		fragment_chain.populate_unconnected_potential_candidates(parent_candidates);

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

	pub fn as_candidate_storage(&self) -> CandidateStorage {
		let mut storage = self.unconnected.clone();

		for candidate in self.chain.iter() {
			let _ = storage.add_candidate_entry(CandidateEntry {
				candidate_hash: candidate.candidate_hash,
				parent_head_data_hash: candidate.parent_head_data_hash,
				output_head_data_hash: candidate.output_head_data_hash,
				relay_parent: candidate.relay_parent(),
				candidate: candidate.fragment.candidate_clone(), // This clone is very cheap.
				state: CandidateState::Backed,
			}); // TODO: is it really ok to hide the error here?
		}

		storage
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
	pub(crate) fn hypothetical_membership(&self, candidate: HypotheticalCandidate) -> bool {
		if let Some(output_head_hash) = candidate.output_head_data_hash() {
			if self.is_cycle(candidate.parent_head_data_hash(), output_head_hash) {
				return false;
			}
		}

		let candidate_hash = candidate.candidate_hash();

		// If we've already used this candidate in the chain or in the unconnected storage.
		if self.candidates.contains(&candidate_hash) || self.unconnected.contains(&candidate_hash) {
			return true
		}

		self.can_add_candidate_as_potential(&candidate_hash, &candidate)
	}

	// TODO: this needs modifying.
	fn is_cycle(&self, parent_head_hash: Hash, output_head_hash: Hash) -> bool {
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

		false
	}

	pub(crate) fn get_head_data_by_hash(&self, head_data_hash: &Hash) -> Option<HeadData> {
		let required_parent = &self.scope.base_constraints().required_parent;
		if &required_parent.hash() == head_data_hash {
			return Some(required_parent.clone())
		}

		let has_head_data_in_chain = self
			.by_parent_head
			.get(head_data_hash)
			.or_else(|| self.by_output_head.get(head_data_hash))
			.is_some();

		if has_head_data_in_chain {
			return self.chain.iter().find_map(|candidate| {
				if &candidate.parent_head_data_hash == head_data_hash {
					Some(
						candidate
							.fragment
							.candidate()
							.persisted_validation_data
							.parent_head
							.clone(),
					)
				} else if &candidate.output_head_data_hash == head_data_hash {
					Some(candidate.fragment.candidate().commitments.head_data.clone())
				} else {
					None
				}
			});
		}

		self.unconnected.head_data_by_hash(head_data_hash).cloned()
	}

	/// Select `count` candidates after the given `ancestors` which can be backed on chain next.
	///
	/// The intention of the `ancestors` is to allow queries on the basis of
	/// one or more candidates which were previously pending availability becoming
	/// available or candidates timing out.
	pub(crate) fn find_backable_chain(
		&self,
		ancestors: Ancestors,
		count: u32,
	) -> Vec<(CandidateHash, Hash)> {
		if count == 0 {
			return vec![]
		}
		let base_pos = self.find_ancestor_path(ancestors);

		let actual_end_index = std::cmp::min(base_pos + (count as usize), self.chain.len());
		let mut res = Vec::with_capacity(actual_end_index - base_pos);

		for elem in &self.chain[base_pos..actual_end_index] {
			if self.scope.get_pending_availability(&elem.candidate_hash).is_none() {
				res.push((elem.candidate_hash, elem.relay_parent()));
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

	fn earliest_relay_parent_pending_availability(&self) -> RelayChainBlockInfo {
		self.chain
			.iter()
			.rev()
			.find_map(|candidate| {
				self.scope
					.get_pending_availability(&candidate.candidate_hash)
					.map(|c| c.relay_parent.clone())
			})
			.unwrap_or_else(|| self.scope.earliest_relay_parent())
	}

	// Checks if this candidate could be added in the future to this chain.
	// This assumes that the chain does not already contain this candidate. It may or may not be
	// present in the `CandidateStorage`.
	// Even if the candidate is a potential candidate, this function will indicate that it can be
	// kept only if there's enough room for it.
	fn can_add_candidate_as_potential(
		&self,
		candidate_hash: &CandidateHash,
		candidate: &impl HypotheticalOrConcreteCandidate,
	) -> bool {
		if !self.check_potential(candidate_hash, candidate) {
			return false
		}

		// TODO: chain len < max_depth OR unconnected < EXTRA_UNCONNECTED_COUNT. we shouldn't mix
		// the two?? NO. here we should have the max number of candidates in the tree!
		(self.chain.len() + self.unconnected.by_candidate_hash.len()) <=
			(self.scope.max_depth + EXTRA_UNCONNECTED_COUNT)
	}

	pub(crate) fn try_adding_seconded_candidate(
		&mut self,
		candidate: &CandidateEntry,
	) -> Result<bool, CandidateStorageInsertionError> {
		if self.candidates.contains(&candidate.candidate_hash) ||
			self.unconnected.contains(&candidate.candidate_hash)
		{
			return Err(CandidateStorageInsertionError::CandidateAlreadyKnown(
				candidate.candidate_hash,
			))
		}

		if self.can_add_candidate_as_potential(&candidate.candidate_hash, &*candidate.candidate) {
			// This clone is cheap, as it uses an Arc for the expensive stuff.
			self.unconnected.add_candidate_entry(candidate.clone())?;
			Ok(true)
		} else {
			Ok(false)
		}
	}

	// The candidates which are present in `CandidateStorage`, are not part of this chain but could
	// become part of this chain in the future. Capped at the max depth minus the existing chain
	// length.
	// If `ignore_candidate` is supplied and found in storage, it won't be counted.
	fn populate_unconnected_potential_candidates(&mut self, old_storage: &CandidateStorage) {
		for candidate in old_storage.candidates() {
			// Sanity check, all pending availability candidates should be already present in the
			// chain.
			if self.scope.get_pending_availability(&candidate.candidate_hash).is_some() {
				continue
			}
			// We stop at max_depth + 1 with the search. There's no point in looping further.
			if (self.chain.len() + self.unconnected.len()) >
				(self.scope.max_depth + EXTRA_UNCONNECTED_COUNT)
			{
				break
			}
			if !self.candidates.contains(&candidate.candidate_hash) &&
				self.check_potential(&candidate.candidate_hash, &*candidate.candidate)
			{
				// This is cheap because the expensive stuff is wrapped in an Arc
				let _ = self.unconnected.add_candidate_entry(candidate.clone()); // TODO: is it ok to hide
				                                                     // result here?
			}
		}
	}

	// Checks the potential of a candidate to be added to the chain in the future.
	// Verifies that the relay parent is in scope and not moving backwards and that we're not
	// introducing forks or cycles with other candidates in the chain.
	// `output_head_hash` is optional because we sometimes make this check before retrieving the
	// collation.
	fn check_potential(
		&self,
		candidate_hash: &CandidateHash,
		candidate: &impl HypotheticalOrConcreteCandidate,
	) -> bool {
		let relay_parent = candidate.relay_parent();
		let parent_head_hash = candidate.parent_head_data_hash();

		let Some(relay_parent) = self.scope.ancestor(&relay_parent) else { return false };
		let earliest_rp = self.earliest_relay_parent_pending_availability();
		if relay_parent.number < earliest_rp.number {
			return false // relay parent moved backwards.
		}

		let constraints = if let Some(parent_candidate) = self.by_output_head.get(&parent_head_hash)
		{
			// Check if it's a fork.
			if let Some(other_candidate) = self.by_parent_head.get(&parent_head_hash) {
				if self.scope().get_pending_availability(other_candidate).is_some() {
					// Cannot accept a fork with a candidate pending availability.
					return false
				}

				// If the candidate is backed and in the current chain, accept only a candidate with
				// a lower hash.
				if other_candidate < candidate_hash {
					return false
				}
			}

			let Some(parent_candidate) =
				self.chain.iter().find(|c| &c.candidate_hash == parent_candidate)
			else {
				return false
			};
			let Ok(constraints) = self
				.scope
				.base_constraints
				.apply_modifications(&parent_candidate.cumulative_modifications)
			else {
				return false
			};
			constraints
		// Check if it builds on the latest included candidate
		} else if self.scope.base_constraints.required_parent.hash() == parent_head_hash {
			self.scope.base_constraints.clone()
		} else {
			return true
		};

		// We do additional checks for complete candidates.
		if let (Some(commitments), Some(pvd), Some(validation_code_hash)) = (
			candidate.commitments(),
			candidate.persisted_validation_data(),
			candidate.validation_code_hash(),
		) {
			if Fragment::check_against_constraints(
				&relay_parent,
				&constraints,
				commitments,
				validation_code_hash,
				pvd,
			)
			.is_err()
			{
				gum::debug!(
					target: LOG_TARGET,
					"Fragment::check_against_constraints() returned error",
				);
				return false
			}
		} else {
			// Otherwise, at least check the relay parent progresses.
			return relay_parent.number >= constraints.min_relay_parent_number
		}

		true
	}

	fn trim_uneligible_forks(&self, storage: &mut CandidateStorage) {
		let mut queue: VecDeque<_> = self.chain.iter().map(|c| c.parent_head_data_hash).collect();

		// TODO: if there's a cycle or multiple paths to the same node, this will loop forever.
		while let Some(parent) = queue.pop_front() {
			let Some(children) = storage.by_parent_head.get(&parent) else { continue };
			let mut to_remove = vec![];

			for child_hash in children.iter() {
				let Some(child) = storage.by_candidate_hash.get(child_hash) else { continue };
				if !self.check_potential(child_hash, &*child.candidate) {
					to_remove.push(*child_hash);
					queue.push_back(child.output_head_data_hash);
				}
			}

			for hash in to_remove {
				storage.remove_candidate(&hash);
			}
		}
	}

	// Populate the fragment chain with candidates from CandidateStorage.
	// Can be called by the constructor or when introducing a new candidate.
	// If we're introducing a new candidate onto an existing chain, we may introduce more than one,
	// since we may connect already existing candidates to the chain.
	fn populate_chain(&mut self, storage: &mut CandidateStorage) {
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

			let possible_children = storage
				.possible_backed_para_children(&required_head_hash)
				.filter_map(|candidate| {
					// Add one node to chain if
					// 1. it does not introduce a fork or a cycle.
					// 2. parent hash is correct.
					// 3. relay-parent does not move backwards.
					// 4. all non-pending-availability candidates have relay-parent in scope.
					// 5. candidate outputs fulfill constraints

					let pending = self.scope.get_pending_availability(&candidate.candidate_hash);
					let Some(relay_parent) = pending
						.map(|p| p.relay_parent.clone())
						.or_else(|| self.scope.ancestor(&candidate.relay_parent))
					else {
						return None
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
						return None // relay parent moved backwards.
					}

					// don't add candidates if they're already present in the chain.
					// this can never happen, as candidates can only be duplicated if there's a
					// cycle and we shouldn't have allowed for a cycle to be chained.
					if self.contains_candidate(&candidate.candidate_hash) {
						return None
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

								return None
							},
						}
					};

					Some((
						fragment,
						candidate.candidate_hash,
						candidate.output_head_data_hash,
						candidate.parent_head_data_hash,
					))
				});

			let best_candidate = possible_children.min_by(|child1, child2| {
				// Always pick a candidate pending availability as best.
				if self.scope.get_pending_availability(&child1.1).is_some() {
					Ordering::Less
				} else if self.scope.get_pending_availability(&child2.1).is_some() {
					Ordering::Greater
				} else {
					child1.1.cmp(&child2.1)
				}
			});

			if let Some((fragment, candidate_hash, output_head_data_hash, parent_head_data_hash)) =
				best_candidate
			{
				storage.remove_candidate(&candidate_hash);

				// Update the cumulative constraint modifications.
				cumulative_modifications.stack(fragment.constraint_modifications());
				// Update the earliest rp
				earliest_rp = fragment.relay_parent().clone();

				let node = FragmentNode {
					fragment,
					candidate_hash,
					parent_head_data_hash,
					output_head_data_hash,
					cumulative_modifications: cumulative_modifications.clone(),
				};

				self.chain.push(node);
				self.candidates.insert(candidate_hash);
				// We've already checked for forks and cycles.
				self.by_parent_head.insert(parent_head_data_hash, candidate_hash);
				self.by_output_head.insert(output_head_data_hash, candidate_hash);
			} else {
				break
			}
		}
	}

	pub fn candidate_backed(mut self, newly_backed_candidate: &CandidateHash) -> Self {
		if !self.unconnected.mark_backed(newly_backed_candidate) {
			return self
		}

		// TODO: if is already in chain, it's an error.

		let mut old_storage = self.as_candidate_storage();
		Self::populate(self.scope, &mut old_storage)
	}
}
