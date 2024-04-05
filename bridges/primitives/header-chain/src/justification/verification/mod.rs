// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Parity Bridges Common.

// Parity Bridges Common is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Parity Bridges Common is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Parity Bridges Common.  If not, see <http://www.gnu.org/licenses/>.

//! Logic for checking GRANDPA Finality Proofs.

pub mod equivocation;
pub mod optimizer;
pub mod strict;

use crate::{justification::GrandpaJustification, AuthoritySet};

use bp_runtime::HeaderId;
use finality_grandpa::voter_set::VoterSet;
use sp_consensus_grandpa::{AuthorityId, AuthoritySignature, SetId};
use sp_runtime::{traits::Header as HeaderT, RuntimeDebug};
use sp_std::{
	collections::{
		btree_map::{
			BTreeMap,
			Entry::{Occupied, Vacant},
		},
		btree_set::BTreeSet,
	},
	prelude::*,
};

type SignedPrecommit<Header> = finality_grandpa::SignedPrecommit<
	<Header as HeaderT>::Hash,
	<Header as HeaderT>::Number,
	AuthoritySignature,
	AuthorityId,
>;

/// Votes ancestries with useful methods.
#[derive(RuntimeDebug)]
pub struct AncestryChain<Header: HeaderT> {
	/// We expect all forks in the ancestry chain to be descendants of base.
	base: HeaderId<Header::Hash, Header::Number>,
	/// Header hash => parent header hash mapping.
	parents: BTreeMap<Header::Hash, Header::Hash>,
	/// Hashes of headers that were not visited by `ancestry()`.
	unvisited: BTreeSet<Header::Hash>,
}

impl<Header: HeaderT> AncestryChain<Header> {
	/// Creates a new instance of `AncestryChain` starting from a `GrandpaJustification`.
	///
	/// Returns the `AncestryChain` and a `Vec` containing the `votes_ancestries` entries
	/// that were ignored when creating it, because they are duplicates.
	pub fn new(
		justification: &GrandpaJustification<Header>,
	) -> (AncestryChain<Header>, Vec<usize>) {
		let mut parents = BTreeMap::new();
		let mut unvisited = BTreeSet::new();
		let mut ignored_idxs = Vec::new();
		for (idx, ancestor) in justification.votes_ancestries.iter().enumerate() {
			let hash = ancestor.hash();
			match parents.entry(hash) {
				Occupied(_) => {
					ignored_idxs.push(idx);
				},
				Vacant(entry) => {
					entry.insert(*ancestor.parent_hash());
					unvisited.insert(hash);
				},
			}
		}
		(AncestryChain { base: justification.commit_target_id(), parents, unvisited }, ignored_idxs)
	}

	/// Returns the hash of a block's parent if the block is present in the ancestry.
	pub fn parent_hash_of(&self, hash: &Header::Hash) -> Option<&Header::Hash> {
		self.parents.get(hash)
	}

	/// Returns a route if the precommit target block is a descendant of the `base` block.
	pub fn ancestry(
		&self,
		precommit_target_hash: &Header::Hash,
		precommit_target_number: &Header::Number,
	) -> Option<Vec<Header::Hash>> {
		if precommit_target_number < &self.base.number() {
			return None
		}

		let mut route = vec![];
		let mut current_hash = *precommit_target_hash;
		loop {
			if current_hash == self.base.hash() {
				break
			}

			current_hash = match self.parent_hash_of(&current_hash) {
				Some(parent_hash) => {
					let is_visited_before = self.unvisited.get(&current_hash).is_none();
					if is_visited_before {
						// If the current header has been visited in a previous call, it is a
						// descendent of `base` (we assume that the previous call was successful).
						return Some(route)
					}
					route.push(current_hash);

					*parent_hash
				},
				None => return None,
			};
		}

		Some(route)
	}

	fn mark_route_as_visited(&mut self, route: Vec<Header::Hash>) {
		for hash in route {
			self.unvisited.remove(&hash);
		}
	}

	fn is_fully_visited(&self) -> bool {
		self.unvisited.is_empty()
	}
}

/// Justification verification error.
#[derive(Eq, RuntimeDebug, PartialEq)]
pub enum Error {
	/// Could not convert `AuthorityList` to `VoterSet`.
	InvalidAuthorityList,
	/// Justification is finalizing unexpected header.
	InvalidJustificationTarget,
	/// The justification contains duplicate headers in its `votes_ancestries` field.
	DuplicateVotesAncestries,
	/// Error validating a precommit
	Precommit(PrecommitError),
	/// The cumulative weight of all votes in the justification is not enough to justify commit
	/// header finalization.
	TooLowCumulativeWeight,
	/// The justification contains extra (unused) headers in its `votes_ancestries` field.
	RedundantVotesAncestries,
}

/// Justification verification error.
#[derive(Eq, RuntimeDebug, PartialEq)]
pub enum PrecommitError {
	/// Justification contains redundant votes.
	RedundantAuthorityVote,
	/// Justification contains unknown authority precommit.
	UnknownAuthorityVote,
	/// Justification contains duplicate authority precommit.
	DuplicateAuthorityVote,
	/// The authority has provided an invalid signature.
	InvalidAuthoritySignature,
	/// The justification contains precommit for header that is not a descendant of the commit
	/// header.
	UnrelatedAncestryVote,
}

/// The context needed for validating GRANDPA finality proofs.
#[derive(RuntimeDebug)]
pub struct JustificationVerificationContext {
	/// The authority set used to verify the justification.
	pub voter_set: VoterSet<AuthorityId>,
	/// The ID of the authority set used to verify the justification.
	pub authority_set_id: SetId,
}

impl TryFrom<AuthoritySet> for JustificationVerificationContext {
	type Error = Error;

	fn try_from(authority_set: AuthoritySet) -> Result<Self, Self::Error> {
		let voter_set =
			VoterSet::new(authority_set.authorities).ok_or(Error::InvalidAuthorityList)?;
		Ok(JustificationVerificationContext { voter_set, authority_set_id: authority_set.set_id })
	}
}

enum IterationFlow {
	Run,
	Skip,
}

/// Verification callbacks.
trait JustificationVerifier<Header: HeaderT> {
	/// Called when there are duplicate headers in the votes ancestries.
	fn process_duplicate_votes_ancestries(
		&mut self,
		duplicate_votes_ancestries: Vec<usize>,
	) -> Result<(), Error>;

	fn process_redundant_vote(
		&mut self,
		precommit_idx: usize,
	) -> Result<IterationFlow, PrecommitError>;

	fn process_known_authority_vote(
		&mut self,
		precommit_idx: usize,
		signed: &SignedPrecommit<Header>,
	) -> Result<IterationFlow, PrecommitError>;

	fn process_unknown_authority_vote(
		&mut self,
		precommit_idx: usize,
	) -> Result<(), PrecommitError>;

	fn process_unrelated_ancestry_vote(
		&mut self,
		precommit_idx: usize,
	) -> Result<IterationFlow, PrecommitError>;

	fn process_invalid_signature_vote(
		&mut self,
		precommit_idx: usize,
	) -> Result<(), PrecommitError>;

	fn process_valid_vote(&mut self, signed: &SignedPrecommit<Header>);

	/// Called when there are redundant headers in the votes ancestries.
	fn process_redundant_votes_ancestries(
		&mut self,
		redundant_votes_ancestries: BTreeSet<Header::Hash>,
	) -> Result<(), Error>;

	fn verify_justification(
		&mut self,
		finalized_target: (Header::Hash, Header::Number),
		context: &JustificationVerificationContext,
		justification: &GrandpaJustification<Header>,
	) -> Result<(), Error> {
		// ensure that it is justification for the expected header
		if (justification.commit.target_hash, justification.commit.target_number) !=
			finalized_target
		{
			return Err(Error::InvalidJustificationTarget)
		}

		let threshold = context.voter_set.threshold().get();
		let (mut chain, ignored_idxs) = AncestryChain::new(justification);
		let mut signature_buffer = Vec::new();
		let mut cumulative_weight = 0u64;

		if !ignored_idxs.is_empty() {
			self.process_duplicate_votes_ancestries(ignored_idxs)?;
		}

		for (precommit_idx, signed) in justification.commit.precommits.iter().enumerate() {
			if cumulative_weight >= threshold {
				let action =
					self.process_redundant_vote(precommit_idx).map_err(Error::Precommit)?;
				if matches!(action, IterationFlow::Skip) {
					continue
				}
			}

			// authority must be in the set
			let authority_info = match context.voter_set.get(&signed.id) {
				Some(authority_info) => {
					// The implementer may want to do extra checks here.
					// For example to see if the authority has already voted in the same round.
					let action = self
						.process_known_authority_vote(precommit_idx, signed)
						.map_err(Error::Precommit)?;
					if matches!(action, IterationFlow::Skip) {
						continue
					}

					authority_info
				},
				None => {
					self.process_unknown_authority_vote(precommit_idx).map_err(Error::Precommit)?;
					continue
				},
			};

			// all precommits must be descendants of the target block
			let maybe_route =
				chain.ancestry(&signed.precommit.target_hash, &signed.precommit.target_number);
			if maybe_route.is_none() {
				let action = self
					.process_unrelated_ancestry_vote(precommit_idx)
					.map_err(Error::Precommit)?;
				if matches!(action, IterationFlow::Skip) {
					continue
				}
			}

			// verify authority signature
			if !sp_consensus_grandpa::check_message_signature_with_buffer(
				&finality_grandpa::Message::Precommit(signed.precommit.clone()),
				&signed.id,
				&signed.signature,
				justification.round,
				context.authority_set_id,
				&mut signature_buffer,
			) {
				self.process_invalid_signature_vote(precommit_idx).map_err(Error::Precommit)?;
				continue
			}

			// now we can count the vote since we know that it is valid
			self.process_valid_vote(signed);
			if let Some(route) = maybe_route {
				chain.mark_route_as_visited(route);
				cumulative_weight = cumulative_weight.saturating_add(authority_info.weight().get());
			}
		}

		// check that the cumulative weight of validators that voted for the justification target
		// (or one of its descendants) is larger than the required threshold.
		if cumulative_weight < threshold {
			return Err(Error::TooLowCumulativeWeight)
		}

		// check that there are no extra headers in the justification
		if !chain.is_fully_visited() {
			self.process_redundant_votes_ancestries(chain.unvisited)?;
		}

		Ok(())
	}
}
