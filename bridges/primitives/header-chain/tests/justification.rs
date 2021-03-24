// Copyright 2020 Parity Technologies (UK) Ltd.
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

//! Tests for Grandpa Justification code.

use bp_header_chain::justification::{verify_justification, Error};
use bp_test_utils::*;
use codec::Encode;

type TestHeader = sp_runtime::testing::Header;

#[test]
fn valid_justification_accepted() {
	let params = JustificationGeneratorParams {
		header: test_header(1),
		round: TEST_GRANDPA_ROUND,
		set_id: TEST_GRANDPA_SET_ID,
		authorities: vec![(ALICE, 1), (BOB, 1), (CHARLIE, 1), (DAVE, 1), (EVE, 1)],
		depth: 5,
		forks: 5,
	};

	assert_eq!(
		verify_justification::<TestHeader>(
			header_id::<TestHeader>(1),
			TEST_GRANDPA_SET_ID,
			&voter_set(),
			&make_justification_for_header::<TestHeader>(params).encode()
		),
		Ok(()),
	);
}

#[test]
fn valid_justification_accepted_with_single_fork() {
	let params = JustificationGeneratorParams {
		header: test_header(1),
		round: TEST_GRANDPA_ROUND,
		set_id: TEST_GRANDPA_SET_ID,
		authorities: vec![(ALICE, 1), (BOB, 1), (CHARLIE, 1), (DAVE, 1), (EVE, 1)],
		depth: 5,
		forks: 1,
	};

	assert_eq!(
		verify_justification::<TestHeader>(
			header_id::<TestHeader>(1),
			TEST_GRANDPA_SET_ID,
			&voter_set(),
			&make_justification_for_header::<TestHeader>(params).encode()
		),
		Ok(()),
	);
}

#[test]
fn valid_justification_accepted_with_arbitrary_number_of_authorities() {
	use finality_grandpa::voter_set::VoterSet;
	use sp_finality_grandpa::AuthorityId;

	let n = 15;
	let authorities = accounts(n).iter().map(|k| (*k, 1)).collect::<Vec<_>>();

	let params = JustificationGeneratorParams {
		header: test_header(1),
		round: TEST_GRANDPA_ROUND,
		set_id: TEST_GRANDPA_SET_ID,
		authorities: authorities.clone(),
		depth: 5,
		forks: n.into(),
	};

	let authorities = authorities
		.iter()
		.map(|(id, w)| (AuthorityId::from(*id), *w))
		.collect::<Vec<(AuthorityId, _)>>();
	let voter_set = VoterSet::new(authorities).unwrap();

	assert_eq!(
		verify_justification::<TestHeader>(
			header_id::<TestHeader>(1),
			TEST_GRANDPA_SET_ID,
			&voter_set,
			&make_justification_for_header::<TestHeader>(params).encode()
		),
		Ok(()),
	);
}

#[test]
fn justification_with_invalid_encoding_rejected() {
	assert_eq!(
		verify_justification::<TestHeader>(header_id::<TestHeader>(1), TEST_GRANDPA_SET_ID, &voter_set(), &[],),
		Err(Error::JustificationDecode),
	);
}

#[test]
fn justification_with_invalid_target_rejected() {
	assert_eq!(
		verify_justification::<TestHeader>(
			header_id::<TestHeader>(2),
			TEST_GRANDPA_SET_ID,
			&voter_set(),
			&make_default_justification::<TestHeader>(&test_header(1)).encode(),
		),
		Err(Error::InvalidJustificationTarget),
	);
}

#[test]
fn justification_with_invalid_commit_rejected() {
	let mut justification = make_default_justification::<TestHeader>(&test_header(1));
	justification.commit.precommits.clear();

	assert_eq!(
		verify_justification::<TestHeader>(
			header_id::<TestHeader>(1),
			TEST_GRANDPA_SET_ID,
			&voter_set(),
			&justification.encode(),
		),
		Err(Error::InvalidJustificationCommit),
	);
}

#[test]
fn justification_with_invalid_authority_signature_rejected() {
	let mut justification = make_default_justification::<TestHeader>(&test_header(1));
	justification.commit.precommits[0].signature = Default::default();

	assert_eq!(
		verify_justification::<TestHeader>(
			header_id::<TestHeader>(1),
			TEST_GRANDPA_SET_ID,
			&voter_set(),
			&justification.encode(),
		),
		Err(Error::InvalidAuthoritySignature),
	);
}

#[test]
fn justification_with_invalid_precommit_ancestry() {
	let mut justification = make_default_justification::<TestHeader>(&test_header(1));
	justification.votes_ancestries.push(test_header(10));

	assert_eq!(
		verify_justification::<TestHeader>(
			header_id::<TestHeader>(1),
			TEST_GRANDPA_SET_ID,
			&voter_set(),
			&justification.encode(),
		),
		Err(Error::InvalidPrecommitAncestries),
	);
}

#[test]
fn justification_is_invalid_if_we_dont_meet_threshold() {
	// Need at least three authorities to sign off or else the voter set threshold can't be reached
	let params = JustificationGeneratorParams {
		header: test_header(1),
		round: TEST_GRANDPA_ROUND,
		set_id: TEST_GRANDPA_SET_ID,
		authorities: vec![(ALICE, 1), (BOB, 1)],
		depth: 2,
		forks: 2,
	};

	assert_eq!(
		verify_justification::<TestHeader>(
			header_id::<TestHeader>(1),
			TEST_GRANDPA_SET_ID,
			&voter_set(),
			&make_justification_for_header::<TestHeader>(params).encode()
		),
		Err(Error::InvalidJustificationCommit),
	);
}
