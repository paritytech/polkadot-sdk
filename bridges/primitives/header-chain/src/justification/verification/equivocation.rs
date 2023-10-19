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

//! Logic for extracting equivocations from multiple GRANDPA Finality Proofs.

use crate::{
	justification::{
		verification::{
			Error as JustificationVerificationError, IterationFlow,
			JustificationVerificationContext, JustificationVerifier, PrecommitError,
			SignedPrecommit,
		},
		GrandpaJustification,
	},
	ChainWithGrandpa, FindEquivocations,
};

use bp_runtime::{BlockNumberOf, HashOf, HeaderOf};
use sp_consensus_grandpa::{AuthorityId, AuthoritySignature, EquivocationProof, Precommit};
use sp_runtime::traits::Header as HeaderT;
use sp_std::{
	collections::{btree_map::BTreeMap, btree_set::BTreeSet},
	prelude::*,
};

enum AuthorityVotes<Header: HeaderT> {
	SingleVote(SignedPrecommit<Header>),
	Equivocation(
		finality_grandpa::Equivocation<AuthorityId, Precommit<Header>, AuthoritySignature>,
	),
}

/// Structure that can extract equivocations from multiple GRANDPA justifications.
pub struct EquivocationsCollector<'a, Header: HeaderT> {
	round: u64,
	context: &'a JustificationVerificationContext,

	votes: BTreeMap<AuthorityId, AuthorityVotes<Header>>,
}

impl<'a, Header: HeaderT> EquivocationsCollector<'a, Header> {
	/// Create a new instance of `EquivocationsCollector`.
	pub fn new(
		context: &'a JustificationVerificationContext,
		base_justification: &GrandpaJustification<Header>,
	) -> Result<Self, JustificationVerificationError> {
		let mut checker = Self { round: base_justification.round, context, votes: BTreeMap::new() };

		checker.verify_justification(
			(base_justification.commit.target_hash, base_justification.commit.target_number),
			checker.context,
			base_justification,
		)?;

		Ok(checker)
	}

	/// Parse additional justifications for equivocations.
	pub fn parse_justifications(&mut self, justifications: &[GrandpaJustification<Header>]) {
		let round = self.round;
		for justification in
			justifications.iter().filter(|justification| round == justification.round)
		{
			// We ignore the Errors received here since we don't care if the proofs are valid.
			// We only care about collecting equivocations.
			let _ = self.verify_justification(
				(justification.commit.target_hash, justification.commit.target_number),
				self.context,
				justification,
			);
		}
	}

	/// Extract the equivocation proofs that have been collected.
	pub fn into_equivocation_proofs(self) -> Vec<EquivocationProof<Header::Hash, Header::Number>> {
		let mut equivocations = vec![];
		for (_authority, vote) in self.votes {
			if let AuthorityVotes::Equivocation(equivocation) = vote {
				equivocations.push(EquivocationProof::new(
					self.context.authority_set_id,
					sp_consensus_grandpa::Equivocation::Precommit(equivocation),
				));
			}
		}

		equivocations
	}
}

impl<'a, Header: HeaderT> JustificationVerifier<Header> for EquivocationsCollector<'a, Header> {
	fn process_duplicate_votes_ancestries(
		&mut self,
		_duplicate_votes_ancestries: Vec<usize>,
	) -> Result<(), JustificationVerificationError> {
		Ok(())
	}

	fn process_redundant_vote(
		&mut self,
		_precommit_idx: usize,
	) -> Result<IterationFlow, PrecommitError> {
		Ok(IterationFlow::Run)
	}

	fn process_known_authority_vote(
		&mut self,
		_precommit_idx: usize,
		_signed: &SignedPrecommit<Header>,
	) -> Result<IterationFlow, PrecommitError> {
		Ok(IterationFlow::Run)
	}

	fn process_unknown_authority_vote(
		&mut self,
		_precommit_idx: usize,
	) -> Result<(), PrecommitError> {
		Ok(())
	}

	fn process_unrelated_ancestry_vote(
		&mut self,
		_precommit_idx: usize,
	) -> Result<IterationFlow, PrecommitError> {
		Ok(IterationFlow::Run)
	}

	fn process_invalid_signature_vote(
		&mut self,
		_precommit_idx: usize,
	) -> Result<(), PrecommitError> {
		Ok(())
	}

	fn process_valid_vote(&mut self, signed: &SignedPrecommit<Header>) {
		match self.votes.get_mut(&signed.id) {
			Some(vote) => match vote {
				AuthorityVotes::SingleVote(first_vote) => {
					if first_vote.precommit != signed.precommit {
						*vote = AuthorityVotes::Equivocation(finality_grandpa::Equivocation {
							round_number: self.round,
							identity: signed.id.clone(),
							first: (first_vote.precommit.clone(), first_vote.signature.clone()),
							second: (signed.precommit.clone(), signed.signature.clone()),
						});
					}
				},
				AuthorityVotes::Equivocation(_) => {},
			},
			None => {
				self.votes.insert(signed.id.clone(), AuthorityVotes::SingleVote(signed.clone()));
			},
		}
	}

	fn process_redundant_votes_ancestries(
		&mut self,
		_redundant_votes_ancestries: BTreeSet<Header::Hash>,
	) -> Result<(), JustificationVerificationError> {
		Ok(())
	}
}

/// Helper struct for finding equivocations in GRANDPA proofs.
pub struct GrandpaEquivocationsFinder<C>(sp_std::marker::PhantomData<C>);

impl<C: ChainWithGrandpa>
	FindEquivocations<
		GrandpaJustification<HeaderOf<C>>,
		JustificationVerificationContext,
		EquivocationProof<HashOf<C>, BlockNumberOf<C>>,
	> for GrandpaEquivocationsFinder<C>
{
	type Error = JustificationVerificationError;

	fn find_equivocations(
		verification_context: &JustificationVerificationContext,
		synced_proof: &GrandpaJustification<HeaderOf<C>>,
		source_proofs: &[GrandpaJustification<HeaderOf<C>>],
	) -> Result<Vec<EquivocationProof<HashOf<C>, BlockNumberOf<C>>>, Self::Error> {
		let mut equivocations_collector =
			EquivocationsCollector::new(verification_context, synced_proof)?;

		equivocations_collector.parse_justifications(source_proofs);

		Ok(equivocations_collector.into_equivocation_proofs())
	}
}
