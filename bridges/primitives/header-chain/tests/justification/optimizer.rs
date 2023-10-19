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

//! Tests for Grandpa Justification optimizer code.

use bp_header_chain::justification::verify_and_optimize_justification;
use bp_test_utils::*;
use finality_grandpa::SignedPrecommit;
use sp_consensus_grandpa::AuthoritySignature;

type TestHeader = sp_runtime::testing::Header;

#[test]
fn optimizer_does_noting_with_minimal_justification() {
	let mut justification = make_default_justification::<TestHeader>(&test_header(1));

	let num_precommits_before = justification.commit.precommits.len();
	verify_and_optimize_justification::<TestHeader>(
		header_id::<TestHeader>(1),
		&verification_context(TEST_GRANDPA_SET_ID),
		&mut justification,
	)
	.unwrap();
	let num_precommits_after = justification.commit.precommits.len();

	assert_eq!(num_precommits_before, num_precommits_after);
}

#[test]
fn unknown_authority_votes_are_removed_by_optimizer() {
	let mut justification = make_default_justification::<TestHeader>(&test_header(1));
	justification.commit.precommits.push(signed_precommit::<TestHeader>(
		&bp_test_utils::Account(42),
		header_id::<TestHeader>(1),
		justification.round,
		TEST_GRANDPA_SET_ID,
	));

	let num_precommits_before = justification.commit.precommits.len();
	verify_and_optimize_justification::<TestHeader>(
		header_id::<TestHeader>(1),
		&verification_context(TEST_GRANDPA_SET_ID),
		&mut justification,
	)
	.unwrap();
	let num_precommits_after = justification.commit.precommits.len();

	assert_eq!(num_precommits_before - 1, num_precommits_after);
}

#[test]
fn duplicate_authority_votes_are_removed_by_optimizer() {
	let mut justification = make_default_justification::<TestHeader>(&test_header(1));
	justification
		.commit
		.precommits
		.push(justification.commit.precommits.first().cloned().unwrap());

	let num_precommits_before = justification.commit.precommits.len();
	verify_and_optimize_justification::<TestHeader>(
		header_id::<TestHeader>(1),
		&verification_context(TEST_GRANDPA_SET_ID),
		&mut justification,
	)
	.unwrap();
	let num_precommits_after = justification.commit.precommits.len();

	assert_eq!(num_precommits_before - 1, num_precommits_after);
}

#[test]
fn invalid_authority_signatures_are_removed_by_optimizer() {
	let mut justification = make_default_justification::<TestHeader>(&test_header(1));

	let target = header_id::<TestHeader>(1);
	let invalid_raw_signature: Vec<u8> = ALICE.sign(b"").to_bytes().into();
	justification.commit.precommits.insert(
		0,
		SignedPrecommit {
			precommit: finality_grandpa::Precommit {
				target_hash: target.0,
				target_number: target.1,
			},
			signature: AuthoritySignature::try_from(invalid_raw_signature).unwrap(),
			id: ALICE.into(),
		},
	);

	let num_precommits_before = justification.commit.precommits.len();
	verify_and_optimize_justification::<TestHeader>(
		header_id::<TestHeader>(1),
		&verification_context(TEST_GRANDPA_SET_ID),
		&mut justification,
	)
	.unwrap();
	let num_precommits_after = justification.commit.precommits.len();

	assert_eq!(num_precommits_before - 1, num_precommits_after);
}

#[test]
fn redundant_authority_votes_are_removed_by_optimizer() {
	let mut justification = make_default_justification::<TestHeader>(&test_header(1));
	justification.commit.precommits.push(signed_precommit::<TestHeader>(
		&EVE,
		header_id::<TestHeader>(1),
		justification.round,
		TEST_GRANDPA_SET_ID,
	));

	let num_precommits_before = justification.commit.precommits.len();
	verify_and_optimize_justification::<TestHeader>(
		header_id::<TestHeader>(1),
		&verification_context(TEST_GRANDPA_SET_ID),
		&mut justification,
	)
	.unwrap();
	let num_precommits_after = justification.commit.precommits.len();

	assert_eq!(num_precommits_before - 1, num_precommits_after);
}

#[test]
fn unrelated_ancestry_votes_are_removed_by_optimizer() {
	let mut justification = make_default_justification::<TestHeader>(&test_header(2));
	justification.commit.precommits.insert(
		0,
		signed_precommit::<TestHeader>(
			&ALICE,
			header_id::<TestHeader>(1),
			justification.round,
			TEST_GRANDPA_SET_ID,
		),
	);

	let num_precommits_before = justification.commit.precommits.len();
	verify_and_optimize_justification::<TestHeader>(
		header_id::<TestHeader>(2),
		&verification_context(TEST_GRANDPA_SET_ID),
		&mut justification,
	)
	.unwrap();
	let num_precommits_after = justification.commit.precommits.len();

	assert_eq!(num_precommits_before - 1, num_precommits_after);
}

#[test]
fn duplicate_votes_ancestries_are_removed_by_optimizer() {
	let mut justification = make_default_justification::<TestHeader>(&test_header(1));
	let optimized_votes_ancestries = justification.votes_ancestries.clone();
	justification.votes_ancestries = justification
		.votes_ancestries
		.into_iter()
		.flat_map(|item| std::iter::repeat(item).take(3))
		.collect();

	verify_and_optimize_justification::<TestHeader>(
		header_id::<TestHeader>(1),
		&verification_context(TEST_GRANDPA_SET_ID),
		&mut justification,
	)
	.unwrap();

	assert_eq!(justification.votes_ancestries, optimized_votes_ancestries);
}

#[test]
fn redundant_votes_ancestries_are_removed_by_optimizer() {
	let mut justification = make_default_justification::<TestHeader>(&test_header(1));
	justification.votes_ancestries.push(test_header(100));

	let num_votes_ancestries_before = justification.votes_ancestries.len();
	verify_and_optimize_justification::<TestHeader>(
		header_id::<TestHeader>(1),
		&verification_context(TEST_GRANDPA_SET_ID),
		&mut justification,
	)
	.unwrap();
	let num_votes_ancestries_after = justification.votes_ancestries.len();

	assert_eq!(num_votes_ancestries_before - 1, num_votes_ancestries_after);
}
