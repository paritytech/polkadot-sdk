// Copyright 2019-2023 Parity Technologies (UK) Ltd.
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

//! Logic for optimizing GRANDPA Finality Proofs.

use crate::justification::{
	verification::{Error, JustificationVerifier, PrecommitError},
	GrandpaJustification,
};

use crate::justification::verification::{IterationFlow, SignedPrecommit};
use finality_grandpa::voter_set::VoterSet;
use sp_consensus_grandpa::{AuthorityId, SetId};
use sp_runtime::traits::Header as HeaderT;
use sp_std::{collections::btree_set::BTreeSet, prelude::*};

// Verification callbacks for justification optimization.
struct JustificationOptimizer<Header: HeaderT> {
	votes: BTreeSet<AuthorityId>,

	extra_precommits: Vec<usize>,
	redundant_votes_ancestries: BTreeSet<Header::Hash>,
}

impl<Header: HeaderT> JustificationOptimizer<Header> {
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

impl<Header: HeaderT> JustificationVerifier<Header> for JustificationOptimizer<Header> {
	fn process_redundant_vote(
		&mut self,
		precommit_idx: usize,
	) -> Result<IterationFlow, PrecommitError> {
		self.extra_precommits.push(precommit_idx);
		Ok(IterationFlow::Skip)
	}

	fn process_known_authority_vote(
		&mut self,
		precommit_idx: usize,
		signed: &SignedPrecommit<Header>,
	) -> Result<IterationFlow, PrecommitError> {
		// Skip duplicate votes
		if self.votes.contains(&signed.id) {
			self.extra_precommits.push(precommit_idx);
			return Ok(IterationFlow::Skip)
		}

		Ok(IterationFlow::Run)
	}

	fn process_unknown_authority_vote(
		&mut self,
		precommit_idx: usize,
	) -> Result<(), PrecommitError> {
		self.extra_precommits.push(precommit_idx);
		Ok(())
	}

	fn process_unrelated_ancestry_vote(
		&mut self,
		precommit_idx: usize,
	) -> Result<IterationFlow, PrecommitError> {
		self.extra_precommits.push(precommit_idx);
		Ok(IterationFlow::Skip)
	}

	fn process_invalid_signature_vote(
		&mut self,
		precommit_idx: usize,
	) -> Result<(), PrecommitError> {
		self.extra_precommits.push(precommit_idx);
		Ok(())
	}

	fn process_valid_vote(&mut self, signed: &SignedPrecommit<Header>) {
		self.votes.insert(signed.id.clone());
	}

	fn process_redundant_votes_ancestries(
		&mut self,
		redundant_votes_ancestries: BTreeSet<Header::Hash>,
	) -> Result<(), Error> {
		self.redundant_votes_ancestries = redundant_votes_ancestries;
		Ok(())
	}
}

/// Verify and optimize given justification by removing unknown and duplicate votes.
pub fn verify_and_optimize_justification<Header: HeaderT>(
	finalized_target: (Header::Hash, Header::Number),
	authorities_set_id: SetId,
	authorities_set: &VoterSet<AuthorityId>,
	justification: &mut GrandpaJustification<Header>,
) -> Result<(), Error> {
	let mut optimizer = JustificationOptimizer {
		votes: BTreeSet::new(),
		extra_precommits: vec![],
		redundant_votes_ancestries: Default::default(),
	};
	optimizer.verify_justification(
		finalized_target,
		authorities_set_id,
		authorities_set,
		justification,
	)?;
	optimizer.optimize(justification);

	Ok(())
}
