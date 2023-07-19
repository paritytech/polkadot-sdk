// Copyright 2019-2021 Parity Technologies (UK) Ltd.
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

//! Pallet for checking GRANDPA Finality Proofs.
//!
//! Adapted copy of substrate/client/finality-grandpa/src/justification.rs. If origin
//! will ever be moved to the sp_consensus_grandpa, we should reuse that implementation.

use crate::ChainWithGrandpa;

use bp_runtime::{BlockNumberOf, Chain, HashOf, HeaderId};
use codec::{Decode, Encode, MaxEncodedLen};
use finality_grandpa::voter_set::VoterSet;
use frame_support::{RuntimeDebug, RuntimeDebugNoBound};
use scale_info::TypeInfo;
use sp_consensus_grandpa::{AuthorityId, AuthoritySignature, SetId};
use sp_runtime::{traits::Header as HeaderT, SaturatedConversion};
use sp_std::{
	collections::{btree_map::BTreeMap, btree_set::BTreeSet},
	prelude::*,
};

/// A GRANDPA Justification is a proof that a given header was finalized
/// at a certain height and with a certain set of authorities.
///
/// This particular proof is used to prove that headers on a bridged chain
/// (so not our chain) have been finalized correctly.
#[derive(Encode, Decode, Clone, PartialEq, Eq, TypeInfo, RuntimeDebugNoBound)]
pub struct GrandpaJustification<Header: HeaderT> {
	/// The round (voting period) this justification is valid for.
	pub round: u64,
	/// The set of votes for the chain which is to be finalized.
	pub commit:
		finality_grandpa::Commit<Header::Hash, Header::Number, AuthoritySignature, AuthorityId>,
	/// A proof that the chain of blocks in the commit are related to each other.
	pub votes_ancestries: Vec<Header>,
}

impl<H: HeaderT> GrandpaJustification<H> {
	/// Returns reasonable size of justification using constants from the provided chain.
	///
	/// An imprecise analogue of `MaxEncodedLen` implementation. We don't use it for
	/// any precise calculations - that's just an estimation.
	pub fn max_reasonable_size<C>(required_precommits: u32) -> u32
	where
		C: Chain + ChainWithGrandpa,
	{
		// we don't need precise results here - just estimations, so some details
		// are removed from computations (e.g. bytes required to encode vector length)

		// structures in `finality_grandpa` crate are not implementing `MaxEncodedLength`, so
		// here's our estimation for the `finality_grandpa::Commit` struct size
		//
		// precommit is: hash + number
		// signed precommit is: precommit + signature (64b) + authority id
		// commit is: hash + number + vec of signed precommits
		let signed_precommit_size: u32 = BlockNumberOf::<C>::max_encoded_len()
			.saturating_add(HashOf::<C>::max_encoded_len().saturated_into())
			.saturating_add(64)
			.saturating_add(AuthorityId::max_encoded_len().saturated_into())
			.saturated_into();
		let max_expected_signed_commit_size = signed_precommit_size
			.saturating_mul(required_precommits)
			.saturating_add(BlockNumberOf::<C>::max_encoded_len().saturated_into())
			.saturating_add(HashOf::<C>::max_encoded_len().saturated_into());

		// justification is a signed GRANDPA commit, `votes_ancestries` vector and round number
		let max_expected_votes_ancestries_size = C::REASONABLE_HEADERS_IN_JUSTIFICATON_ANCESTRY
			.saturating_mul(C::AVERAGE_HEADER_SIZE_IN_JUSTIFICATION);

		8u32.saturating_add(max_expected_signed_commit_size)
			.saturating_add(max_expected_votes_ancestries_size)
	}

	pub fn commit_target_id(&self) -> HeaderId<H::Hash, H::Number> {
		HeaderId(self.commit.target_number, self.commit.target_hash)
	}
}

impl<H: HeaderT> crate::FinalityProof<H::Number> for GrandpaJustification<H> {
	fn target_header_number(&self) -> H::Number {
		self.commit.target_number
	}
}

/// Justification verification error.
#[derive(Eq, RuntimeDebug, PartialEq)]
pub enum Error {
	/// Failed to decode justification.
	JustificationDecode,
	/// Justification is finalizing unexpected header.
	InvalidJustificationTarget,
	/// Justification contains redundant votes.
	RedundantVotesInJustification,
	/// Justification contains unknown authority precommit.
	UnknownAuthorityVote,
	/// Justification contains duplicate authority precommit.
	DuplicateAuthorityVote,
	/// The authority has provided an invalid signature.
	InvalidAuthoritySignature,
	/// The justification contains precommit for header that is not a descendant of the commit
	/// header.
	UnrelatedAncestryVote,
	/// The cumulative weight of all votes in the justification is not enough to justify commit
	/// header finalization.
	TooLowCumulativeWeight,
	/// The justification contains extra (unused) headers in its `votes_ancestries` field.
	RedundantVotesAncestries,
}

/// Given GRANDPA authorities set size, return number of valid authorities votes that the
/// justification must have to be valid.
///
/// This function assumes that all authorities have the same vote weight.
pub fn required_justification_precommits(authorities_set_length: u32) -> u32 {
	authorities_set_length - authorities_set_length.saturating_sub(1) / 3
}

/// Decode justification target.
pub fn decode_justification_target<Header: HeaderT>(
	raw_justification: &[u8],
) -> Result<(Header::Hash, Header::Number), Error> {
	GrandpaJustification::<Header>::decode(&mut &*raw_justification)
		.map(|justification| (justification.commit.target_hash, justification.commit.target_number))
		.map_err(|_| Error::JustificationDecode)
}

/// Verify and optimize given justification by removing unknown and duplicate votes.
pub fn verify_and_optimize_justification<Header: HeaderT>(
	finalized_target: (Header::Hash, Header::Number),
	authorities_set_id: SetId,
	authorities_set: &VoterSet<AuthorityId>,
	justification: &mut GrandpaJustification<Header>,
) -> Result<(), Error> {
	let mut optimizer = OptimizationCallbacks {
		extra_precommits: vec![],
		redundant_votes_ancestries: Default::default(),
	};
	verify_justification_with_callbacks(
		finalized_target,
		authorities_set_id,
		authorities_set,
		justification,
		&mut optimizer,
	)?;
	optimizer.optimize(justification);

	Ok(())
}

/// Verify that justification, that is generated by given authority set, finalizes given header.
pub fn verify_justification<Header: HeaderT>(
	finalized_target: (Header::Hash, Header::Number),
	authorities_set_id: SetId,
	authorities_set: &VoterSet<AuthorityId>,
	justification: &GrandpaJustification<Header>,
) -> Result<(), Error> {
	verify_justification_with_callbacks(
		finalized_target,
		authorities_set_id,
		authorities_set,
		justification,
		&mut StrictVerificationCallbacks,
	)
}

/// Verification callbacks.
trait VerificationCallbacks<Header: HeaderT> {
	/// Called when we see a precommit from unknown authority.
	fn on_unkown_authority(&mut self, precommit_idx: usize) -> Result<(), Error>;
	/// Called when we see a precommit with duplicate vote from known authority.
	fn on_duplicate_authority_vote(&mut self, precommit_idx: usize) -> Result<(), Error>;
	/// Called when we see a precommit with an invalid signature.
	fn on_invalid_authority_signature(&mut self, precommit_idx: usize) -> Result<(), Error>;
	/// Called when we see a precommit after we've collected enough votes from authorities.
	fn on_redundant_authority_vote(&mut self, precommit_idx: usize) -> Result<(), Error>;
	/// Called when we see a precommit that is not a descendant of the commit target.
	fn on_unrelated_ancestry_vote(&mut self, precommit_idx: usize) -> Result<(), Error>;
	/// Called when there are redundant headers in the votes ancestries.
	fn on_redundant_votes_ancestries(
		&mut self,
		redundant_votes_ancestries: BTreeSet<Header::Hash>,
	) -> Result<(), Error>;
}

/// Verification callbacks that reject all unknown, duplicate or redundant votes.
struct StrictVerificationCallbacks;

impl<Header: HeaderT> VerificationCallbacks<Header> for StrictVerificationCallbacks {
	fn on_unkown_authority(&mut self, _precommit_idx: usize) -> Result<(), Error> {
		Err(Error::UnknownAuthorityVote)
	}

	fn on_duplicate_authority_vote(&mut self, _precommit_idx: usize) -> Result<(), Error> {
		Err(Error::DuplicateAuthorityVote)
	}

	fn on_invalid_authority_signature(&mut self, _precommit_idx: usize) -> Result<(), Error> {
		Err(Error::InvalidAuthoritySignature)
	}

	fn on_redundant_authority_vote(&mut self, _precommit_idx: usize) -> Result<(), Error> {
		Err(Error::RedundantVotesInJustification)
	}

	fn on_unrelated_ancestry_vote(&mut self, _precommit_idx: usize) -> Result<(), Error> {
		Err(Error::UnrelatedAncestryVote)
	}

	fn on_redundant_votes_ancestries(
		&mut self,
		_redundant_votes_ancestries: BTreeSet<Header::Hash>,
	) -> Result<(), Error> {
		Err(Error::RedundantVotesAncestries)
	}
}

/// Verification callbacks for justification optimization.
struct OptimizationCallbacks<Header: HeaderT> {
	extra_precommits: Vec<usize>,
	redundant_votes_ancestries: BTreeSet<Header::Hash>,
}

impl<Header: HeaderT> OptimizationCallbacks<Header> {
	fn optimize(self, justification: &mut GrandpaJustification<Header>) {
		for invalid_precommit_idx in self.extra_precommits.into_iter().rev() {
			justification.commit.precommits.remove(invalid_precommit_idx);
		}
		if !self.redundant_votes_ancestries.is_empty() {
			justification
				.votes_ancestries
				.retain(|header| !self.redundant_votes_ancestries.contains(&header.hash()))
		}
	}
}

impl<Header: HeaderT> VerificationCallbacks<Header> for OptimizationCallbacks<Header> {
	fn on_unkown_authority(&mut self, precommit_idx: usize) -> Result<(), Error> {
		self.extra_precommits.push(precommit_idx);
		Ok(())
	}

	fn on_duplicate_authority_vote(&mut self, precommit_idx: usize) -> Result<(), Error> {
		self.extra_precommits.push(precommit_idx);
		Ok(())
	}

	fn on_invalid_authority_signature(&mut self, precommit_idx: usize) -> Result<(), Error> {
		self.extra_precommits.push(precommit_idx);
		Ok(())
	}

	fn on_redundant_authority_vote(&mut self, precommit_idx: usize) -> Result<(), Error> {
		self.extra_precommits.push(precommit_idx);
		Ok(())
	}

	fn on_unrelated_ancestry_vote(&mut self, precommit_idx: usize) -> Result<(), Error> {
		self.extra_precommits.push(precommit_idx);
		Ok(())
	}

	fn on_redundant_votes_ancestries(
		&mut self,
		redundant_votes_ancestries: BTreeSet<Header::Hash>,
	) -> Result<(), Error> {
		self.redundant_votes_ancestries = redundant_votes_ancestries;
		Ok(())
	}
}

/// Verify that justification, that is generated by given authority set, finalizes given header.
fn verify_justification_with_callbacks<Header: HeaderT, C: VerificationCallbacks<Header>>(
	finalized_target: (Header::Hash, Header::Number),
	authorities_set_id: SetId,
	authorities_set: &VoterSet<AuthorityId>,
	justification: &GrandpaJustification<Header>,
	callbacks: &mut C,
) -> Result<(), Error> {
	// ensure that it is justification for the expected header
	if (justification.commit.target_hash, justification.commit.target_number) != finalized_target {
		return Err(Error::InvalidJustificationTarget)
	}

	let threshold = authorities_set.threshold().get();
	let mut chain = AncestryChain::new(justification);
	let mut signature_buffer = Vec::new();
	let mut votes = BTreeSet::new();
	let mut cumulative_weight = 0u64;

	for (precommit_idx, signed) in justification.commit.precommits.iter().enumerate() {
		// if we have collected enough precommits, we probabably want to fail/remove extra
		// precommits
		if cumulative_weight >= threshold {
			callbacks.on_redundant_authority_vote(precommit_idx)?;
			continue
		}

		// authority must be in the set
		let authority_info = match authorities_set.get(&signed.id) {
			Some(authority_info) => authority_info,
			None => {
				callbacks.on_unkown_authority(precommit_idx)?;
				continue
			},
		};

		// check if authority has already voted in the same round.
		//
		// there's a lot of code in `validate_commit` and `import_precommit` functions inside
		// `finality-grandpa` crate (mostly related to reporting equivocations). But the only thing
		// that we care about is that only first vote from the authority is accepted
		if votes.contains(&signed.id) {
			callbacks.on_duplicate_authority_vote(precommit_idx)?;
			continue
		}

		// all precommits must be descendants of the target block
		let route =
			match chain.ancestry(&signed.precommit.target_hash, &signed.precommit.target_number) {
				Some(route) => route,
				None => {
					callbacks.on_unrelated_ancestry_vote(precommit_idx)?;
					continue
				},
			};

		// verify authority signature
		if !sp_consensus_grandpa::check_message_signature_with_buffer(
			&finality_grandpa::Message::Precommit(signed.precommit.clone()),
			&signed.id,
			&signed.signature,
			justification.round,
			authorities_set_id,
			&mut signature_buffer,
		) {
			callbacks.on_invalid_authority_signature(precommit_idx)?;
			continue
		}

		// now we can count the vote since we know that it is valid
		votes.insert(signed.id.clone());
		chain.mark_route_as_visited(route);
		cumulative_weight = cumulative_weight.saturating_add(authority_info.weight().get());
	}

	// check that the cumulative weight of validators that voted for the justification target (or
	// one of its descendents) is larger than the required threshold.
	if cumulative_weight < threshold {
		return Err(Error::TooLowCumulativeWeight)
	}

	// check that there are no extra headers in the justification
	if !chain.is_fully_visited() {
		callbacks.on_redundant_votes_ancestries(chain.unvisited)?;
	}

	Ok(())
}

/// Votes ancestries with useful methods.
#[derive(RuntimeDebug)]
pub struct AncestryChain<Header: HeaderT> {
	/// We expect all forks in the ancestry chain to be descendants of base.
	base: HeaderId<Header::Hash, Header::Number>,
	/// Header hash => parent header hash mapping.
	pub parents: BTreeMap<Header::Hash, Header::Hash>,
	/// Hashes of headers that were not visited by `ancestry()`.
	pub unvisited: BTreeSet<Header::Hash>,
}

impl<Header: HeaderT> AncestryChain<Header> {
	/// Create new ancestry chain.
	pub fn new(justification: &GrandpaJustification<Header>) -> AncestryChain<Header> {
		let mut parents = BTreeMap::new();
		let mut unvisited = BTreeSet::new();
		for ancestor in &justification.votes_ancestries {
			let hash = ancestor.hash();
			let parent_hash = *ancestor.parent_hash();
			parents.insert(hash, parent_hash);
			unvisited.insert(hash);
		}
		AncestryChain { base: justification.commit_target_id(), parents, unvisited }
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

			current_hash = match self.parents.get(&current_hash) {
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
