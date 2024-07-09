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
//! The main type exposed by this module is the [`FragmentChain`].
//!
//! Each fragment chain is associated with a particular relay-parent (an active leaf) and has a
//! [`Scope`], which contains the allowed relay parents (up to `allowed_ancestry_len`), the pending
//! availability candidates and base constraints derived from the latest included candidate. Each
//! parachain has a single `FragmentChain` for each active leaf where it's scheduled.
//!
//! A fragment chain consists mainly of the current best backable chain (we'll call this the best
//! chain) and a storage of unconnected potential candidates (we'll call this the unconnected
//! storage).
//!
//! The best chain contains all the candidates pending availability and a subsequent chain
//! of candidates that have reached the backing quorum and are better than any other backable forks
//! according to the fork selection rule (more on this rule later). It has a length of size at most
//! `max_candidate_depth + 1`.
//!
//! The unconnected storage keeps a record of seconded/backable candidates that may be
//! added to the best chain in the future.
//!	Once a candidate is seconded, it becomes part of this unconnected storage.
//! Only after it is backed it may be added to the best chain (but not neccessarily). It's only
//! added if it builds on the latest candidate in the chain and if there isn't a better backable
//! candidate according to the fork selection rule.
//!
//! An important thing to note is that the candidates present in the unconnected storage may have
//! any/no relationship between them. In other words, they may form N trees and may even form
//! cycles. This is needed so that we may begin validating candidates for which we don't yet know
//! their parent (so we may parallelise the backing process across different groups for elastic
//! scaling) and so that we accept parachain forks.
//!
//! We accept parachain forks only until reaching the backing quorum. After that, we assume all
//! validators pick the same fork accroding to the fork selection rule. If we decided to not accept
//! parachain forks, candidates could end up getting only half of the backing votes or even less
//! (for forks of larger arity). This would affect the validator rewards. Still, we don't guarantee
//! that a fork-producing parachains will be able to fully use elastic scaling.
//!
//! Once a candidate is backed and becomes part of the best chain, we can trim from the
//! unconnected storage candidates which constitute forks on the best chain and no longer have
//! potential.
//!
//! This module also makes use of types provided by the Inclusion Emulator module, such as
//! [`Fragment`] and [`Constraints`]. These perform the actual job of checking for validity of
//! prospective fragments.
//!
//! # Fork choice rule
//!
//! The motivation for the fork choice rule is described in the previous chapter.
//!
//! The current rule is: choose the candidate with the lower candidate hash.
//! The candidate hash is quite random and finding a candidate with a lower hash in order to favour
//! it would essentially mean solving a proof of work problem.
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
//! practical reason.
//! These cycles may be accepted by fragment chains while candidates are part of the unconnected
//! storage, but they will definitely not make it to the best chain.
//!
//! On the other hand, enforcing that a parachain will NEVER be acyclic would be very complicated
//! (looping through the entire parachain's history on every new candidate or changing the candidate
//! receipt to reference the parent's candidate hash).
//!
//! Therefore, we don't provide a guarantee that a cycle-producing parachain will work (although in
//! practice they probably will if the cycle length is larger than the number of assigned cores
//! multiplied by two).
//!
//! # Spam protection
//!
//! As long as the supplied number of candidates is bounded, [`FragmentChain`] complexity is
//! bounded. This means that higher-level code needs to be selective about limiting the amount of
//! candidates that are considered.
//!
//! Practically speaking, the collator-protocol will not allow more than `max_candidate_depth + 1`
//! collations to be fetched at a relay parent and statement-distribution will not allow more than
//! `max_candidate_depth + 1` seconded candidates at a relay parent per each validator in the
//! backing group. Considering the `allowed_ancestry_len` configuration value, the number of
//! candidates in a `FragmentChain` (including its unconnected storage) should not exceed:
//!
//! `allowed_ancestry_len * (max_candidate_depth + 1) * backing_group_size`.
//!
//! The code in this module is not designed for speed or efficiency, but conceptual simplicity.
//! Our assumption is that the amount of candidates and parachains we consider will be reasonably
//! bounded and in practice will not exceed a few thousand at any time. This naive implementation
//! will still perform fairly well under these conditions, despite being somewhat wasteful of
//! memory.
//!
//! Still, the expensive candidate data (CandidateCommitments) are wrapped in an `Arc` and shared
//! across fragment chains of the same para on different active leaves.

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
use polkadot_node_subsystem::messages::Ancestors;
use polkadot_node_subsystem_util::inclusion_emulator::{
	self, ConstraintModifications, Constraints, Fragment, HypotheticalOrConcreteCandidate,
	ProspectiveCandidate, RelayChainBlockInfo,
};
use polkadot_primitives::{
	BlockNumber, CandidateCommitments, CandidateHash, CommittedCandidateReceipt, Hash, HeadData,
	PersistedValidationData, ValidationCodeHash,
};
use thiserror::Error;

/// Fragment chain related errors.
#[derive(Debug, Clone, PartialEq, Error)]
pub(crate) enum Error {
	#[error("Candidate already known")]
	CandidateAlreadyKnown,
	#[error("Candidate is already pending availability")]
	CandidateAlreadyPendingAvailability,
	#[error("Candidate's parent head is equal to its output head. Would introduce a cycle.")]
	ZeroLengthCycle,
	#[error("Candidate would introduce a cycle")]
	Cycle,
	#[error("Candidate would introduce two paths to the same output state")]
	MultiplePaths,
	#[error("Attempting to directly introduce a Backed candidate. It should first be introduced as Seconded")]
	IntroduceBackedCandidate,
	#[error("Current backed candidate chain reached the `max_candidate_depth + 1` limit")]
	ChainTooLong,
	#[error("Relay parent {0:?} of the candidate precedes the relay parent {0:?} of a pending availability candidate")]
	RelayParentPrecedesCandidatePendingAvailability(Hash, Hash),
	#[error("Candidate would introduce a fork with a pending availability candidate: {0:?}")]
	ForkWithCandidatePendingAvailability(CandidateHash),
	#[error("Fork selection rule favours another candidate: {0:?}")]
	ForkChoiceRule(CandidateHash),
	#[error("Could not find parent of the candidate")]
	ParentCandidateNotFound,
	#[error("Could not compute candidate constraints: {0:?}")]
	ComputeConstraints(inclusion_emulator::ModificationError),
	#[error("Candidate violates constraints: {0:?}")]
	CheckAgainstConstraints(inclusion_emulator::FragmentValidityError),
	#[error("Relay parent would move backwards from the latest candidate in the chain")]
	RelayParentMovedBackwards,
	#[error(transparent)]
	CandidateEntry(#[from] CandidateEntryError),
	#[error("Relay parent {0:?} not in scope. Earliest relay parent allowed {0:?}")]
	RelayParentNotInScope(Hash, Hash),
}

/// The rule for selecting between two backed candidate forks, when adding to the chain.
/// All validators should adhere to this rule.
fn fork_selection_rule(hash1: &CandidateHash, hash2: &CandidateHash) -> Ordering {
	hash1.cmp(hash2)
}

/// Utility for storing candidates and information about them such as their relay-parents and their
/// backing states. This does not assume any restriction on whether or not the candidates form a
/// chain. Useful for storing all kinds of candidates.
#[derive(Clone, Default)]
pub(crate) struct CandidateStorage {
	// Index from head data hash to candidate hashes with that head data as a parent. Useful for
	// efficiency when responding to `ProspectiveValidationDataRequest`s or when trying to find a
	// new candidate to push to a chain.
	by_parent_head: HashMap<Hash, HashSet<CandidateHash>>,

	// Index from head data hash to candidate hashes outputting that head data. For
	// efficiency when responding to `ProspectiveValidationDataRequest`s.
	by_output_head: HashMap<Hash, HashSet<CandidateHash>>,

	// Index from candidate hash to fragment node.
	by_candidate_hash: HashMap<CandidateHash, CandidateEntry>,
}

impl CandidateStorage {
	/// Introduce a new pending availability candidate.
	pub fn add_pending_availability_candidate(
		&mut self,
		candidate_hash: CandidateHash,
		candidate: CommittedCandidateReceipt,
		persisted_validation_data: PersistedValidationData,
	) -> Result<(), Error> {
		let entry = CandidateEntry::new(
			candidate_hash,
			candidate,
			persisted_validation_data,
			CandidateState::Backed,
		)?;

		self.add_candidate_entry(entry)
	}

	/// Introduce a new candidate entry.
	pub fn add_candidate_entry(&mut self, candidate: CandidateEntry) -> Result<(), Error> {
		let candidate_hash = candidate.candidate_hash;
		if self.by_candidate_hash.contains_key(&candidate_hash) {
			return Err(Error::CandidateAlreadyKnown)
		}

		self.by_parent_head
			.entry(candidate.parent_head_data_hash)
			.or_default()
			.insert(candidate_hash);
		self.by_output_head
			.entry(candidate.output_head_data_hash)
			.or_default()
			.insert(candidate_hash);
		self.by_candidate_hash.insert(candidate_hash, candidate);

		Ok(())
	}

	/// Remove a candidate from the store.
	fn remove_candidate(&mut self, candidate_hash: &CandidateHash) {
		if let Some(entry) = self.by_candidate_hash.remove(candidate_hash) {
			if let Entry::Occupied(mut e) = self.by_parent_head.entry(entry.parent_head_data_hash) {
				e.get_mut().remove(&candidate_hash);
				if e.get().is_empty() {
					e.remove();
				}
			}

			if let Entry::Occupied(mut e) = self.by_output_head.entry(entry.output_head_data_hash) {
				e.get_mut().remove(&candidate_hash);
				if e.get().is_empty() {
					e.remove();
				}
			}
		}
	}

	/// Note that an existing candidate has been backed. Return false if the candidate was not
	/// found.
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
	fn contains(&self, candidate_hash: &CandidateHash) -> bool {
		self.by_candidate_hash.contains_key(candidate_hash)
	}

	/// Return an iterator over references to the stored candidates, in arbitrary order.
	fn candidates(&self) -> impl Iterator<Item = &CandidateEntry> {
		self.by_candidate_hash.values()
	}

	/// Consume self into an iterator over the stored candidates, in arbitrary order.
	pub fn into_candidates(self) -> impl Iterator<Item = CandidateEntry> {
		self.by_candidate_hash.into_values()
	}

	/// Try getting head-data by hash.
	fn head_data_by_hash(&self, hash: &Hash) -> Option<&HeadData> {
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

	/// Returns the backed candidates which have the given head data hash as parent.
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

	fn len(&self) -> usize {
		self.by_candidate_hash.len()
	}
}

/// The state of a candidate.
///
/// Candidates aren't even considered until they've at least been seconded.
#[derive(Debug, PartialEq, Clone)]
enum CandidateState {
	/// The candidate has been seconded.
	Seconded,
	/// The candidate has been completely backed by the group.
	Backed,
}

#[derive(Debug, Clone, PartialEq, Error)]
/// Possible errors when construcing a candidate entry.
pub enum CandidateEntryError {
	#[error("Candidate does not match the persisted validation data provided alongside it")]
	PersistedValidationDataMismatch,
	#[error("Candidate's parent head is equal to its output head. Would introduce a cycle")]
	ZeroLengthCycle,
}

#[derive(Debug, Clone)]
/// Representation of a candidate into the [`CandidateStorage`].
pub(crate) struct CandidateEntry {
	candidate_hash: CandidateHash,
	parent_head_data_hash: Hash,
	output_head_data_hash: Hash,
	relay_parent: Hash,
	candidate: Arc<ProspectiveCandidate>,
	state: CandidateState,
}

impl CandidateEntry {
	/// Create a new seconded candidate entry.
	pub fn new_seconded(
		candidate_hash: CandidateHash,
		candidate: CommittedCandidateReceipt,
		persisted_validation_data: PersistedValidationData,
	) -> Result<Self, CandidateEntryError> {
		Self::new(candidate_hash, candidate, persisted_validation_data, CandidateState::Seconded)
	}

	fn new(
		candidate_hash: CandidateHash,
		candidate: CommittedCandidateReceipt,
		persisted_validation_data: PersistedValidationData,
		state: CandidateState,
	) -> Result<Self, CandidateEntryError> {
		if persisted_validation_data.hash() != candidate.descriptor.persisted_validation_data_hash {
			return Err(CandidateEntryError::PersistedValidationDataMismatch)
		}

		let parent_head_data_hash = persisted_validation_data.parent_head.hash();
		let output_head_data_hash = candidate.commitments.head_data.hash();

		if parent_head_data_hash == output_head_data_hash {
			return Err(CandidateEntryError::ZeroLengthCycle)
		}

		Ok(Self {
			candidate_hash,
			parent_head_data_hash,
			output_head_data_hash,
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

	pub fn hash(&self) -> CandidateHash {
		self.candidate_hash
	}
}

impl HypotheticalOrConcreteCandidate for &CandidateEntry {
	fn commitments(&self) -> Option<&CandidateCommitments> {
		Some(&self.candidate.commitments)
	}

	fn persisted_validation_data(&self) -> Option<&PersistedValidationData> {
		Some(&self.candidate.persisted_validation_data)
	}

	fn validation_code_hash(&self) -> Option<&ValidationCodeHash> {
		Some(&self.candidate.validation_code_hash)
	}

	fn parent_head_data_hash(&self) -> Hash {
		self.parent_head_data_hash
	}

	fn output_head_data_hash(&self) -> Option<Hash> {
		Some(self.output_head_data_hash)
	}

	fn relay_parent(&self) -> Hash {
		self.relay_parent
	}

	fn candidate_hash(&self) -> CandidateHash {
		self.candidate_hash
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
pub(crate) struct UnexpectedAncestor {
	/// The block number that this error occurred at.
	/// Allow as dead code, but it's being read in logs.
	#[allow(dead_code)]
	pub number: BlockNumber,
	/// The previous seen block number, which did not match `number`.
	/// Allow as dead code, but it's being read in logs.
	#[allow(dead_code)]
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
	fn get_pending_availability(
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

/// A candidate chain of backed/backable candidates.
/// Includes the candidates pending availability and candidates which may be backed on-chain.
#[derive(Default)]
struct BackedChain {
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
	fn push(&mut self, candidate: FragmentNode) {
		self.candidates.insert(candidate.candidate_hash);
		self.by_parent_head
			.insert(candidate.parent_head_data_hash, candidate.candidate_hash);
		self.by_output_head
			.insert(candidate.output_head_data_hash, candidate.candidate_hash);
		self.chain.push(candidate);
	}

	fn contains(&self, hash: &CandidateHash) -> bool {
		self.candidates.contains(hash)
	}
}

/// This is the fragment chain specific to an active leaf.
///
/// It holds the current best backable candidate chain, as well as potential candidates
/// which could become connected to the chain in the future or which could even overwrite the
/// existing chain.
pub(crate) struct FragmentChain {
	// The current scope, which dictates the on-chain operating constraints that all future
	// candidates must adhere to.
	scope: Scope,

	// The current best chain of backable candidates. It only contains candidates which build on
	// top of each other and which have reached the backing quorum. In the presence of potential
	// forks, this chain will pick a fork according to the `fork_selection_rule`.
	best_chain: BackedChain,

	// The potential candidate storage. Contains candidates which are not yet part of the `chain`
	// but may become in the future. These can form any tree shape as well as contain any
	// unconnected candidates for which we don't know the parent.
	unconnected: CandidateStorage,
}

impl FragmentChain {
	/// Create a new [`FragmentChain`] with given scope and populated from the given storage.
	/// The `prev_storage` should contain the candidates of the `FragmentChain` at the previous
	/// relay parent, as well as the candidates pending availability at this relay parent.
	pub fn populate(scope: Scope, mut prev_storage: CandidateStorage) -> Self {
		// Initialize as empty
		let mut fragment_chain = Self {
			scope,
			best_chain: BackedChain::default(),
			unconnected: CandidateStorage::default(),
		};

		// First populate the best backable chain.
		fragment_chain.populate_chain(&mut prev_storage);

		// Now that we picked the best backable chain, trim the forks generated by candidates which
		// are not present in the best chain.
		fragment_chain.trim_uneligible_forks(&mut prev_storage);

		// Finally, keep any candidates which haven't been trimmed but still have potential.
		fragment_chain.populate_unconnected_potential_candidates(prev_storage);

		fragment_chain
	}

	/// Get the scope of the [`FragmentChain`].
	pub fn scope(&self) -> &Scope {
		&self.scope
	}

	/// Returns the number of candidates in the best backable chain.
	pub fn best_chain_len(&self) -> usize {
		self.best_chain.chain.len()
	}

	/// Returns the number of candidates in unconnected potential storage.
	pub fn unconnected_len(&self) -> usize {
		self.unconnected.len()
	}

	/// Whether the candidate exists as part of the unconnected potential candidates.
	pub fn contains_unconnected_candidate(&self, candidate: &CandidateHash) -> bool {
		self.unconnected.contains(candidate)
	}

	/// Return a vector of the chain's candidate hashes, in-order.
	pub fn best_chain_vec(&self) -> Vec<CandidateHash> {
		self.best_chain.chain.iter().map(|candidate| candidate.candidate_hash).collect()
	}

	/// Return a vector of the unconnected potential candidate hashes, in arbitrary order.
	pub fn unconnected(&self) -> impl Iterator<Item = &CandidateEntry> {
		self.unconnected.candidates()
	}

	/// Return whether this candidate is backed in this chain or the unconnected storage.
	pub fn is_candidate_backed(&self, hash: &CandidateHash) -> bool {
		self.best_chain.candidates.contains(hash) ||
			matches!(
				self.unconnected.by_candidate_hash.get(hash),
				Some(candidate) if candidate.state == CandidateState::Backed
			)
	}

	/// Return a new [`CandidateStorage`] containing all the candidates from this `FragmentChain`,
	/// as well as the unconnected ones.
	pub fn as_candidate_storage(&self) -> CandidateStorage {
		let mut storage = self.unconnected.clone();

		for candidate in self.best_chain.chain.iter() {
			let _ = storage.add_candidate_entry(CandidateEntry {
				candidate_hash: candidate.candidate_hash,
				parent_head_data_hash: candidate.parent_head_data_hash,
				output_head_data_hash: candidate.output_head_data_hash,
				relay_parent: candidate.relay_parent(),
				candidate: candidate.fragment.candidate_clone(), // This clone is very cheap.
				state: CandidateState::Backed,
			});
		}

		storage
	}

	/// Try getting the full head data associated with this hash.
	pub fn get_head_data_by_hash(&self, head_data_hash: &Hash) -> Option<HeadData> {
		// First, see if this is the head data of the latest included candidate.
		let required_parent = &self.scope.base_constraints().required_parent;
		if &required_parent.hash() == head_data_hash {
			return Some(required_parent.clone())
		}

		// Cheaply check if the head data is in the best backable chain.
		let has_head_data_in_chain = self
			.best_chain
			.by_parent_head
			.get(head_data_hash)
			.or_else(|| self.best_chain.by_output_head.get(head_data_hash))
			.is_some();

		if has_head_data_in_chain {
			return self.best_chain.chain.iter().find_map(|candidate| {
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

		// Lastly, try getting the head data from the unconnected candidates.
		self.unconnected.head_data_by_hash(head_data_hash).cloned()
	}

	/// Select `count` candidates after the given `ancestors` which can be backed on chain next.
	///
	/// The intention of the `ancestors` is to allow queries on the basis of
	/// one or more candidates which were previously pending availability becoming
	/// available or candidates timing out.
	pub fn find_backable_chain(
		&self,
		ancestors: Ancestors,
		count: u32,
	) -> Vec<(CandidateHash, Hash)> {
		if count == 0 {
			return vec![]
		}
		let base_pos = self.find_ancestor_path(ancestors);

		let actual_end_index =
			std::cmp::min(base_pos + (count as usize), self.best_chain.chain.len());
		let mut res = Vec::with_capacity(actual_end_index - base_pos);

		for elem in &self.best_chain.chain[base_pos..actual_end_index] {
			// Only supply candidates which are not yet pending availability. `ancestors` should
			// have already contained them, but check just in case.
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
		if self.best_chain.chain.is_empty() {
			return 0;
		}

		for (index, candidate) in self.best_chain.chain.iter().enumerate() {
			if !ancestors.remove(&candidate.candidate_hash) {
				return index
			}
		}

		// This means that we found the entire chain in the ancestor set. There won't be anything
		// left to back.
		self.best_chain.chain.len()
	}

	// Return the earliest relay parent a new candidate can have in order to be added to the chain
	// right now. This is the relay parent of the last candidate in the chain.
	// The value returned may not be valid if we want to add a candidate pending availability, which
	// may have a relay parent which is out of scope. Special handling is needed in that case.
	// `None` is returned if the candidate's relay parent info cannot be found.
	fn earliest_relay_parent(&self) -> Option<RelayChainBlockInfo> {
		if let Some(last_candidate) = self.best_chain.chain.last() {
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

	// Return the earliest relay parent a potential candidate may have for it to ever be added to
	// the chain. This is the relay parent of the last candidate pending availability or the
	// earliest relay parent in scope.
	fn earliest_relay_parent_pending_availability(&self) -> RelayChainBlockInfo {
		self.best_chain
			.chain
			.iter()
			.rev()
			.find_map(|candidate| {
				self.scope
					.get_pending_availability(&candidate.candidate_hash)
					.map(|c| c.relay_parent.clone())
			})
			.unwrap_or_else(|| self.scope.earliest_relay_parent())
	}

	/// Checks if this candidate could be added in the future to this chain.
	/// This will return `Error::CandidateAlreadyKnown` if the candidate is alrady in the chain or
	/// the unconnected candidate storage. It will return
	/// `Error::CandidateAlreadyPendingAvailability` if the candidate is already pending
	/// availability.
	pub fn can_add_candidate_as_potential(
		&self,
		candidate: &impl HypotheticalOrConcreteCandidate,
	) -> Result<(), Error> {
		let candidate_hash = candidate.candidate_hash();

		if self.scope.get_pending_availability(&candidate_hash).is_some() {
			return Err(Error::CandidateAlreadyPendingAvailability)
		}

		if self.best_chain.contains(&candidate_hash) || self.unconnected.contains(&candidate_hash) {
			return Err(Error::CandidateAlreadyKnown)
		}

		self.check_potential(candidate)
	}

	/// Try adding a seconded candidate, if the candidate has potential. It will never be added to
	/// the chain directly in the seconded state, it will only be part of the unconnected storage.
	pub fn try_adding_seconded_candidate(
		&mut self,
		candidate: &CandidateEntry,
	) -> Result<(), Error> {
		if candidate.state == CandidateState::Backed {
			return Err(Error::IntroduceBackedCandidate);
		}

		let res = self.can_add_candidate_as_potential(&candidate);

		if res.is_ok() {
			// This clone is cheap, as it uses an Arc for the expensive stuff.
			// We can't consume the candidate because other fragment chains may use it also.
			self.unconnected.add_candidate_entry(candidate.clone())?;
		}

		res
	}

	// Populate the unconnected potential candidate storage starting from a previous storage.
	fn populate_unconnected_potential_candidates(&mut self, old_storage: CandidateStorage) {
		for candidate in old_storage.into_candidates() {
			// Sanity check, all pending availability candidates should be already present in the
			// chain.
			if self.scope.get_pending_availability(&candidate.candidate_hash).is_some() {
				continue
			}

			match self.can_add_candidate_as_potential(&&candidate) {
				Ok(()) => {
					let _ = self.unconnected.add_candidate_entry(candidate);
				},
				// Swallow these errors as they can legitimately happen when pruning stale
				// candidates.
				Err(_) => {},
			};
		}
	}

	// Check whether a candidate outputting this head data would introduce a cycle or multiple paths
	// to the same state. Trivial 0-length cycles are checked in `CandidateEntry::new`.
	fn check_cycles_or_invalid_tree(&self, output_head_hash: &Hash) -> Result<(), Error> {
		// this should catch a cycle where this candidate would point back to the parent of some
		// candidate in the chain.
		if self.best_chain.by_parent_head.contains_key(output_head_hash) {
			return Err(Error::Cycle)
		}

		// multiple paths to the same state, which can't happen for a chain.
		if self.best_chain.by_output_head.contains_key(output_head_hash) {
			return Err(Error::MultiplePaths)
		}

		Ok(())
	}

	// Checks the potential of a candidate to be added to the chain now or in the future.
	// It works both with concrete candidates for which we have the full PVD and committed receipt,
	// but also does some more basic checks for incomplete candidates (before even fetching them).
	fn check_potential(
		&self,
		candidate: &impl HypotheticalOrConcreteCandidate,
	) -> Result<(), Error> {
		let relay_parent = candidate.relay_parent();
		let parent_head_hash = candidate.parent_head_data_hash();

		// trivial 0-length cycle.
		if let Some(output_head_hash) = candidate.output_head_data_hash() {
			if parent_head_hash == output_head_hash {
				return Err(Error::ZeroLengthCycle)
			}
		}

		// Check if the relay parent is in scope.
		let Some(relay_parent) = self.scope.ancestor(&relay_parent) else {
			return Err(Error::RelayParentNotInScope(
				relay_parent,
				self.scope.earliest_relay_parent().hash,
			))
		};

		// Check if the relay parent moved backwards from the latest candidate pending availability.
		let earliest_rp_of_pending_availability = self.earliest_relay_parent_pending_availability();
		if relay_parent.number < earliest_rp_of_pending_availability.number {
			return Err(Error::RelayParentPrecedesCandidatePendingAvailability(
				relay_parent.hash,
				earliest_rp_of_pending_availability.hash,
			))
		}

		// If it's a fork with a backed candidate in the current chain.
		if let Some(other_candidate) = self.best_chain.by_parent_head.get(&parent_head_hash) {
			if self.scope().get_pending_availability(other_candidate).is_some() {
				// Cannot accept a fork with a candidate pending availability.
				return Err(Error::ForkWithCandidatePendingAvailability(*other_candidate))
			}

			// If the candidate is backed and in the current chain, accept only a candidate
			// according to the fork selection rul.
			if fork_selection_rule(other_candidate, &candidate.candidate_hash()) == Ordering::Less {
				return Err(Error::ForkChoiceRule(*other_candidate))
			}
		}

		// Try seeing if the parent candidate is in the current chain or if it is the latest
		// included candidate. If so, get the constraints the candidate must satisfy.
		let constraints = if let Some(parent_candidate) =
			self.best_chain.by_output_head.get(&parent_head_hash)
		{
			let Some(parent_candidate_index) =
				self.best_chain.chain.iter().position(|c| &c.candidate_hash == parent_candidate)
			else {
				// Should never really happen.
				return Err(Error::ParentCandidateNotFound)
			};

			// We already have enough candidates in this chain.
			if parent_candidate_index >= self.scope.max_depth {
				return Err(Error::ChainTooLong)
			}

			let parent_candidate = &self.best_chain.chain[parent_candidate_index];
			self.scope
				.base_constraints
				.apply_modifications(&parent_candidate.cumulative_modifications)
				.map_err(Error::ComputeConstraints)?
		} else if self.scope.base_constraints.required_parent.hash() == parent_head_hash {
			// It builds on the latest included candidate.
			self.scope.base_constraints.clone()
		} else {
			// If the parent is not yet part of the chain, there's nothing else we can check for
			// now.
			return Ok(())
		};

		// Check for cycles or invalid tree transitions.
		if let Some(ref output_head_hash) = candidate.output_head_data_hash() {
			self.check_cycles_or_invalid_tree(output_head_hash)?;
		}

		// Check against constraints if we have a full concrete candidate.
		if let (Some(commitments), Some(pvd), Some(validation_code_hash)) = (
			candidate.commitments(),
			candidate.persisted_validation_data(),
			candidate.validation_code_hash(),
		) {
			Fragment::check_against_constraints(
				&relay_parent,
				&constraints,
				commitments,
				validation_code_hash,
				pvd,
			)
			.map_err(Error::CheckAgainstConstraints)?;
		}

		if relay_parent.number < constraints.min_relay_parent_number {
			return Err(Error::RelayParentMovedBackwards)
		}

		if let Some(earliest_rp) = self.earliest_relay_parent() {
			if relay_parent.number < earliest_rp.number {
				return Err(Error::RelayParentMovedBackwards)
			}
		}

		Ok(())
	}

	// Once the backable chain was populated, trim the forks generated by candidates which
	// are not present in the best chain. Fan out this into a full breadth-first search.
	fn trim_uneligible_forks(&self, storage: &mut CandidateStorage) {
		// Start out with the candidates in the chain. They are all valid candidates.
		let mut queue: VecDeque<_> =
			self.best_chain.chain.iter().map(|c| (c.parent_head_data_hash, true)).collect();
		// To make sure that cycles don't make us loop forever, keep track of the visited parent
		// heads.
		let mut visited = HashSet::new();

		while let Some((parent, parent_has_potential)) = queue.pop_front() {
			visited.insert(parent);

			let Some(children) = storage.by_parent_head.get(&parent) else { continue };
			// Cannot remove while iterating so store them here temporarily.
			let mut to_remove = vec![];

			for child_hash in children.iter() {
				let Some(child) = storage.by_candidate_hash.get(child_hash) else { continue };

				// Already visited this parent. Either is a cycle or multiple paths that lead to the
				// same candidate. Either way, stop this branch to avoid looping forever.
				if visited.contains(&child.output_head_data_hash) {
					continue
				}

				// Only keep a candidate if its full ancestry was already kept as potential and this
				// candidate itself has potential.
				if parent_has_potential && self.check_potential(&child).is_ok() {
					queue.push_back((child.output_head_data_hash, true));
				} else {
					// Otherwise, remove this candidate and continue looping for its children, but
					// mark the parent's potential as `false`. We only want to remove its
					// children.
					to_remove.push(*child_hash);
					queue.push_back((child.output_head_data_hash, false));
				}
			}

			for hash in to_remove {
				storage.remove_candidate(&hash);
			}
		}
	}

	// Populate the fragment chain with candidates from the supplied `CandidateStorage`.
	// Can be called by the constructor or when backing a new candidate.
	// When this is called, it may cause a the previous chain to be completely erased or it may add
	// more than one candidate.
	fn populate_chain(&mut self, storage: &mut CandidateStorage) {
		let mut cumulative_modifications =
			if let Some(last_candidate) = self.best_chain.chain.last() {
				last_candidate.cumulative_modifications.clone()
			} else {
				ConstraintModifications::identity()
			};
		let Some(mut earliest_rp) = self.earliest_relay_parent() else { return };

		loop {
			if self.best_chain.chain.len() > self.scope.max_depth {
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
			// Select the few possible backed/backable children which can be added to the chain
			// right now.

			let possible_children = storage
				.possible_backed_para_children(&required_head_hash)
				.filter_map(|candidate| {
					// Only select a candidates if:
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

					if self.check_cycles_or_invalid_tree(&candidate.output_head_data_hash).is_err()
					{
						return None
					}

					// require: candidates don't move backwards
					// and only pending availability candidates can be out-of-scope.
					//
					// earliest_rp can be before the earliest relay parent in the scope
					// when the parent is a pending availability candidate as well, but
					// only other pending candidates can have a relay parent out of scope.
					let min_relay_parent_number = pending
						.map(|p| match self.best_chain.chain.len() {
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
					if self.best_chain.contains(&candidate.candidate_hash) {
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

			// Choose the best candidate.
			let best_candidate =
				possible_children.min_by(|(_, ref child1, _, _), (_, ref child2, _, _)| {
					// Always pick a candidate pending availability as best.
					if self.scope.get_pending_availability(child1).is_some() {
						Ordering::Less
					} else if self.scope.get_pending_availability(child2).is_some() {
						Ordering::Greater
					} else {
						// Otherwise, use the fork selection rule.
						fork_selection_rule(child1, child2)
					}
				});

			if let Some((fragment, candidate_hash, output_head_data_hash, parent_head_data_hash)) =
				best_candidate
			{
				// Remove the candidate from storage.
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

				// Add the candidate to the chain now.
				self.best_chain.push(node);
			} else {
				break
			}
		}
	}

	/// Mark a candidate as backed. Return `None` if the candidate is not part of the unconnected
	/// storage.
	/// This will trigger a recreation of the best backable chain.
	pub fn candidate_backed(&self, newly_backed_candidate: &CandidateHash) -> Option<Self> {
		// Get the storage containing both the backable chain and the unconnected candidates.
		let mut old_storage = self.as_candidate_storage();

		if !old_storage.mark_backed(newly_backed_candidate) {
			return None
		}

		// Repopulate.
		Some(Self::populate(self.scope.clone(), old_storage))
	}
}
