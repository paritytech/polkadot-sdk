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

//! Tests for Grandpa equivocations collector code.

use bp_header_chain::justification::EquivocationsCollector;
use bp_test_utils::*;
use finality_grandpa::Precommit;
use sp_consensus_grandpa::EquivocationProof;

type TestHeader = sp_runtime::testing::Header;

#[test]
fn duplicate_votes_are_not_considered_equivocations() {
	let verification_context = verification_context(TEST_GRANDPA_SET_ID);
	let base_justification = make_default_justification::<TestHeader>(&test_header(1));

	let mut collector =
		EquivocationsCollector::new(&verification_context, &base_justification).unwrap();
	collector.parse_justifications(&[base_justification.clone()]);

	assert_eq!(collector.into_equivocation_proofs().len(), 0);
}

#[test]
fn equivocations_are_detected_in_base_justification_redundant_votes() {
	let mut base_justification = make_default_justification::<TestHeader>(&test_header(1));

	let first_vote = base_justification.commit.precommits[0].clone();
	let equivocation = signed_precommit::<TestHeader>(
		&ALICE,
		header_id::<TestHeader>(1),
		base_justification.round,
		TEST_GRANDPA_SET_ID,
	);
	base_justification.commit.precommits.push(equivocation.clone());

	let verification_context = verification_context(TEST_GRANDPA_SET_ID);
	let collector =
		EquivocationsCollector::new(&verification_context, &base_justification).unwrap();

	assert_eq!(
		collector.into_equivocation_proofs(),
		vec![EquivocationProof::new(
			1,
			sp_consensus_grandpa::Equivocation::Precommit(finality_grandpa::Equivocation {
				round_number: 1,
				identity: ALICE.into(),
				first: (
					Precommit {
						target_hash: first_vote.precommit.target_hash,
						target_number: first_vote.precommit.target_number
					},
					first_vote.signature
				),
				second: (
					Precommit {
						target_hash: equivocation.precommit.target_hash,
						target_number: equivocation.precommit.target_number
					},
					equivocation.signature
				)
			})
		)]
	);
}

#[test]
fn equivocations_are_detected_in_extra_justification_redundant_votes() {
	let base_justification = make_default_justification::<TestHeader>(&test_header(1));
	let first_vote = base_justification.commit.precommits[0].clone();

	let mut extra_justification = base_justification.clone();
	let equivocation = signed_precommit::<TestHeader>(
		&ALICE,
		header_id::<TestHeader>(1),
		base_justification.round,
		TEST_GRANDPA_SET_ID,
	);
	extra_justification.commit.precommits.push(equivocation.clone());

	let verification_context = verification_context(TEST_GRANDPA_SET_ID);
	let mut collector =
		EquivocationsCollector::new(&verification_context, &base_justification).unwrap();
	collector.parse_justifications(&[extra_justification]);

	assert_eq!(
		collector.into_equivocation_proofs(),
		vec![EquivocationProof::new(
			1,
			sp_consensus_grandpa::Equivocation::Precommit(finality_grandpa::Equivocation {
				round_number: 1,
				identity: ALICE.into(),
				first: (
					Precommit {
						target_hash: first_vote.precommit.target_hash,
						target_number: first_vote.precommit.target_number
					},
					first_vote.signature
				),
				second: (
					Precommit {
						target_hash: equivocation.precommit.target_hash,
						target_number: equivocation.precommit.target_number
					},
					equivocation.signature
				)
			})
		)]
	);
}
