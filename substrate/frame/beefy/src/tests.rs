// This file is part of Substrate.

// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
// 	http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

use codec::Encode;
use std::vec;

use frame_support::{
	assert_err, assert_ok,
	dispatch::{GetDispatchInfo, Pays},
	traits::{Currency, KeyOwnerProofSystem, OnInitialize},
};
use sp_consensus_beefy::{
	check_vote_equivocation_proof,
	known_payloads::MMR_ROOT_ID,
	test_utils::{
		generate_fork_equivocation_proof_sc, generate_fork_equivocation_proof_vote,
		generate_vote_equivocation_proof, Keyring as BeefyKeyring,
	},
	Commitment, Payload, ValidatorSet, KEY_TYPE as BEEFY_KEY_TYPE,
};
use sp_core::offchain::{testing::TestOffchainExt, OffchainDbExt, OffchainWorkerExt};
use sp_runtime::DigestItem;

use crate::{mock::*, Call, Config, Error, Weight, WeightInfo};

fn init_block(block: u64) {
	System::set_block_number(block);
	Session::on_initialize(block);
}

pub fn beefy_log(log: ConsensusLog<BeefyId>) -> DigestItem {
	DigestItem::Consensus(BEEFY_ENGINE_ID, log.encode())
}

#[test]
fn genesis_session_initializes_authorities() {
	let authorities = mock_authorities(vec![1, 2, 3, 4]);
	let want = authorities.clone();

	new_test_ext_raw_authorities(authorities).execute_with(|| {
		let authorities = Beefy::authorities();

		assert_eq!(authorities.len(), 4);
		assert_eq!(want[0], authorities[0]);
		assert_eq!(want[1], authorities[1]);

		assert!(Beefy::validator_set_id() == 0);

		let next_authorities = Beefy::next_authorities();

		assert_eq!(next_authorities.len(), 4);
		assert_eq!(want[0], next_authorities[0]);
		assert_eq!(want[1], next_authorities[1]);
	});
}

#[test]
fn session_change_updates_authorities() {
	let authorities = mock_authorities(vec![1, 2, 3, 4]);
	let want_validators = authorities.clone();

	new_test_ext(vec![1, 2, 3, 4]).execute_with(|| {
		assert!(0 == Beefy::validator_set_id());

		init_block(1);

		assert!(1 == Beefy::validator_set_id());

		let want = beefy_log(ConsensusLog::AuthoritiesChange(
			ValidatorSet::new(want_validators, 1).unwrap(),
		));

		let log = System::digest().logs[0].clone();
		assert_eq!(want, log);

		init_block(2);

		assert!(2 == Beefy::validator_set_id());

		let want = beefy_log(ConsensusLog::AuthoritiesChange(
			ValidatorSet::new(vec![mock_beefy_id(2), mock_beefy_id(3), mock_beefy_id(4)], 2)
				.unwrap(),
		));

		let log = System::digest().logs[1].clone();
		assert_eq!(want, log);
	});
}

#[test]
fn session_change_updates_next_authorities() {
	let want = vec![mock_beefy_id(1), mock_beefy_id(2), mock_beefy_id(3), mock_beefy_id(4)];

	new_test_ext(vec![1, 2, 3, 4]).execute_with(|| {
		let next_authorities = Beefy::next_authorities();

		assert_eq!(next_authorities.len(), 4);
		assert_eq!(want[0], next_authorities[0]);
		assert_eq!(want[1], next_authorities[1]);
		assert_eq!(want[2], next_authorities[2]);
		assert_eq!(want[3], next_authorities[3]);

		init_block(1);

		let next_authorities = Beefy::next_authorities();

		assert_eq!(next_authorities.len(), 3);
		assert_eq!(want[1], next_authorities[0]);
		assert_eq!(want[3], next_authorities[2]);
	});
}

#[test]
fn validator_set_at_genesis() {
	let want = vec![mock_beefy_id(1), mock_beefy_id(2)];

	new_test_ext(vec![1, 2, 3, 4]).execute_with(|| {
		let vs = Beefy::validator_set().unwrap();

		assert_eq!(vs.id(), 0u64);
		assert_eq!(vs.validators()[0], want[0]);
		assert_eq!(vs.validators()[1], want[1]);
	});
}

#[test]
fn validator_set_updates_work() {
	let want = vec![mock_beefy_id(1), mock_beefy_id(2), mock_beefy_id(3), mock_beefy_id(4)];

	new_test_ext(vec![1, 2, 3, 4]).execute_with(|| {
		let vs = Beefy::validator_set().unwrap();
		assert_eq!(vs.id(), 0u64);
		assert_eq!(want[0], vs.validators()[0]);
		assert_eq!(want[1], vs.validators()[1]);
		assert_eq!(want[2], vs.validators()[2]);
		assert_eq!(want[3], vs.validators()[3]);

		init_block(1);

		let vs = Beefy::validator_set().unwrap();

		assert_eq!(vs.id(), 1u64);
		assert_eq!(want[0], vs.validators()[0]);
		assert_eq!(want[1], vs.validators()[1]);

		init_block(2);

		let vs = Beefy::validator_set().unwrap();

		assert_eq!(vs.id(), 2u64);
		assert_eq!(want[1], vs.validators()[0]);
		assert_eq!(want[3], vs.validators()[2]);
	});
}

#[test]
fn cleans_up_old_set_id_session_mappings() {
	new_test_ext(vec![1, 2, 3, 4]).execute_with(|| {
		let max_set_id_session_entries = MaxSetIdSessionEntries::get();

		// we have 3 sessions per era
		let era_limit = max_set_id_session_entries / 3;
		// sanity check against division precision loss
		assert_eq!(0, max_set_id_session_entries % 3);
		// go through `max_set_id_session_entries` sessions
		start_era(era_limit);

		// we should have a session id mapping for all the set ids from
		// `max_set_id_session_entries` eras we have observed
		for i in 1..=max_set_id_session_entries {
			assert!(Beefy::session_for_set(i as u64).is_some());
		}

		// go through another `max_set_id_session_entries` sessions
		start_era(era_limit * 2);

		// we should keep tracking the new mappings for new sessions
		for i in max_set_id_session_entries + 1..=max_set_id_session_entries * 2 {
			assert!(Beefy::session_for_set(i as u64).is_some());
		}

		// but the old ones should have been pruned by now
		for i in 1..=max_set_id_session_entries {
			assert!(Beefy::session_for_set(i as u64).is_none());
		}
	});
}

/// Returns a list with 3 authorities with known keys:
/// Alice, Bob and Charlie.
pub fn test_authorities() -> Vec<BeefyId> {
	let authorities =
		vec![BeefyKeyring::Alice, BeefyKeyring::Bob, BeefyKeyring::Charlie, BeefyKeyring::Dave];
	authorities.into_iter().map(|id| id.public()).collect()
}

#[test]
fn should_sign_and_verify() {
	use sp_runtime::traits::Keccak256;

	let set_id = 3;
	let payload1 = Payload::from_single_entry(MMR_ROOT_ID, vec![42]);
	let payload2 = Payload::from_single_entry(MMR_ROOT_ID, vec![128]);

	// generate an equivocation proof, with two votes in the same round for
	// same payload signed by the same key
	let equivocation_proof = generate_vote_equivocation_proof(
		(1, payload1.clone(), set_id, &BeefyKeyring::Bob),
		(1, payload1.clone(), set_id, &BeefyKeyring::Bob),
	);
	// expect invalid equivocation proof
	assert!(!check_vote_equivocation_proof::<_, _, Keccak256>(&equivocation_proof));

	// generate an equivocation proof, with two votes in different rounds for
	// different payloads signed by the same key
	let equivocation_proof = generate_vote_equivocation_proof(
		(1, payload1.clone(), set_id, &BeefyKeyring::Bob),
		(2, payload2.clone(), set_id, &BeefyKeyring::Bob),
	);
	// expect invalid equivocation proof
	assert!(!check_vote_equivocation_proof::<_, _, Keccak256>(&equivocation_proof));

	// generate an equivocation proof, with two votes by different authorities
	let equivocation_proof = generate_vote_equivocation_proof(
		(1, payload1.clone(), set_id, &BeefyKeyring::Alice),
		(1, payload2.clone(), set_id, &BeefyKeyring::Bob),
	);
	// expect invalid equivocation proof
	assert!(!check_vote_equivocation_proof::<_, _, Keccak256>(&equivocation_proof));

	// generate an equivocation proof, with two votes in different set ids
	let equivocation_proof = generate_vote_equivocation_proof(
		(1, payload1.clone(), set_id, &BeefyKeyring::Bob),
		(1, payload2.clone(), set_id + 1, &BeefyKeyring::Bob),
	);
	// expect invalid equivocation proof
	assert!(!check_vote_equivocation_proof::<_, _, Keccak256>(&equivocation_proof));

	// generate an equivocation proof, with two votes in the same round for
	// different payloads signed by the same key
	let payload2 = Payload::from_single_entry(MMR_ROOT_ID, vec![128]);
	let equivocation_proof = generate_vote_equivocation_proof(
		(1, payload1, set_id, &BeefyKeyring::Bob),
		(1, payload2, set_id, &BeefyKeyring::Bob),
	);
	// expect valid equivocation proof
	assert!(check_vote_equivocation_proof::<_, _, Keccak256>(&equivocation_proof));
}

// vote equivocation report tests
// TODO: deduplicate by extracting common test structure of equivocation classes
#[test]
fn report_vote_equivocation_current_set_works() {
	let authorities = test_authorities();

	new_test_ext_raw_authorities(authorities).execute_with(|| {
		assert_eq!(Staking::current_era(), Some(0));
		assert_eq!(Session::current_index(), 0);

		start_era(1);

		let block_num = System::block_number();
		let validator_set = Beefy::validator_set().unwrap();
		let authorities = validator_set.validators();
		let set_id = validator_set.id();
		let validators = Session::validators();

		// make sure that all validators have the same balance
		for validator in &validators {
			assert_eq!(Balances::total_balance(validator), 10_000_000);
			assert_eq!(Staking::slashable_balance_of(validator), 10_000);

			assert_eq!(
				Staking::eras_stakers(1, &validator),
				pallet_staking::Exposure { total: 10_000, own: 10_000, others: vec![] },
			);
		}

		assert_eq!(authorities.len(), 3);
		let equivocation_authority_index = 1;
		let equivocation_key = &authorities[equivocation_authority_index];
		let equivocation_keyring = BeefyKeyring::from_public(equivocation_key).unwrap();

		let payload1 = Payload::from_single_entry(MMR_ROOT_ID, vec![42]);
		let payload2 = Payload::from_single_entry(MMR_ROOT_ID, vec![128]);
		// generate an equivocation proof, with two votes in the same round for
		// different payloads signed by the same key
		let equivocation_proof = generate_vote_equivocation_proof(
			(block_num, payload1, set_id, &equivocation_keyring),
			(block_num, payload2, set_id, &equivocation_keyring),
		);

		// create the key ownership proof
		let key_owner_proof = Historical::prove((BEEFY_KEY_TYPE, &equivocation_key)).unwrap();

		// report the equivocation and the tx should be dispatched successfully
		assert_ok!(Beefy::report_vote_equivocation_unsigned(
			RuntimeOrigin::none(),
			Box::new(equivocation_proof),
			key_owner_proof,
		),);

		start_era(2);

		// check that the balance of 0-th validator is slashed 100%.
		let equivocation_validator_id = validators[equivocation_authority_index];

		assert_eq!(Balances::total_balance(&equivocation_validator_id), 10_000_000 - 10_000);
		assert_eq!(Staking::slashable_balance_of(&equivocation_validator_id), 0);
		assert_eq!(
			Staking::eras_stakers(2, &equivocation_validator_id),
			pallet_staking::Exposure { total: 0, own: 0, others: vec![] },
		);

		// check that the balances of all other validators are left intact.
		for validator in &validators {
			if *validator == equivocation_validator_id {
				continue
			}

			assert_eq!(Balances::total_balance(validator), 10_000_000);
			assert_eq!(Staking::slashable_balance_of(validator), 10_000);

			assert_eq!(
				Staking::eras_stakers(2, &validator),
				pallet_staking::Exposure { total: 10_000, own: 10_000, others: vec![] },
			);
		}
	});
}

#[test]
fn report_vote_equivocation_old_set_works() {
	let authorities = test_authorities();

	new_test_ext_raw_authorities(authorities).execute_with(|| {
		start_era(1);

		let block_num = System::block_number();
		let validator_set = Beefy::validator_set().unwrap();
		let authorities = validator_set.validators();
		let validators = Session::validators();
		let old_set_id = validator_set.id();

		assert_eq!(authorities.len(), 3);
		let equivocation_authority_index = 0;
		let equivocation_key = &authorities[equivocation_authority_index];

		// create the key ownership proof in the "old" set
		let key_owner_proof = Historical::prove((BEEFY_KEY_TYPE, &equivocation_key)).unwrap();

		start_era(2);

		// make sure that all authorities have the same balance
		for validator in &validators {
			assert_eq!(Balances::total_balance(validator), 10_000_000);
			assert_eq!(Staking::slashable_balance_of(validator), 10_000);

			assert_eq!(
				Staking::eras_stakers(2, &validator),
				pallet_staking::Exposure { total: 10_000, own: 10_000, others: vec![] },
			);
		}

		let validator_set = Beefy::validator_set().unwrap();
		let new_set_id = validator_set.id();
		assert_eq!(old_set_id + 3, new_set_id);

		let equivocation_keyring = BeefyKeyring::from_public(equivocation_key).unwrap();

		let payload1 = Payload::from_single_entry(MMR_ROOT_ID, vec![42]);
		let payload2 = Payload::from_single_entry(MMR_ROOT_ID, vec![128]);
		// generate an equivocation proof for the old set,
		let equivocation_proof = generate_vote_equivocation_proof(
			(block_num, payload1, old_set_id, &equivocation_keyring),
			(block_num, payload2, old_set_id, &equivocation_keyring),
		);

		// report the equivocation and the tx should be dispatched successfully
		assert_ok!(Beefy::report_vote_equivocation_unsigned(
			RuntimeOrigin::none(),
			Box::new(equivocation_proof),
			key_owner_proof,
		),);

		start_era(3);

		// check that the balance of 0-th validator is slashed 100%.
		let equivocation_validator_id = validators[equivocation_authority_index];

		assert_eq!(Balances::total_balance(&equivocation_validator_id), 10_000_000 - 10_000);
		assert_eq!(Staking::slashable_balance_of(&equivocation_validator_id), 0);
		assert_eq!(
			Staking::eras_stakers(3, &equivocation_validator_id),
			pallet_staking::Exposure { total: 0, own: 0, others: vec![] },
		);

		// check that the balances of all other validators are left intact.
		for validator in &validators {
			if *validator == equivocation_validator_id {
				continue
			}

			assert_eq!(Balances::total_balance(validator), 10_000_000);
			assert_eq!(Staking::slashable_balance_of(validator), 10_000);

			assert_eq!(
				Staking::eras_stakers(3, &validator),
				pallet_staking::Exposure { total: 10_000, own: 10_000, others: vec![] },
			);
		}
	});
}

#[test]
fn report_vote_equivocation_invalid_set_id() {
	let authorities = test_authorities();

	new_test_ext_raw_authorities(authorities).execute_with(|| {
		start_era(1);

		let block_num = System::block_number();
		let validator_set = Beefy::validator_set().unwrap();
		let authorities = validator_set.validators();
		let set_id = validator_set.id();

		let equivocation_authority_index = 0;
		let equivocation_key = &authorities[equivocation_authority_index];
		let equivocation_keyring = BeefyKeyring::from_public(equivocation_key).unwrap();

		let key_owner_proof = Historical::prove((BEEFY_KEY_TYPE, &equivocation_key)).unwrap();

		let payload1 = Payload::from_single_entry(MMR_ROOT_ID, vec![42]);
		let payload2 = Payload::from_single_entry(MMR_ROOT_ID, vec![128]);
		// generate an equivocation for a future set
		let equivocation_proof = generate_vote_equivocation_proof(
			(block_num, payload1, set_id + 1, &equivocation_keyring),
			(block_num, payload2, set_id + 1, &equivocation_keyring),
		);

		// the call for reporting the equivocation should error
		assert_err!(
			Beefy::report_vote_equivocation_unsigned(
				RuntimeOrigin::none(),
				Box::new(equivocation_proof),
				key_owner_proof,
			),
			Error::<Test>::InvalidEquivocationProofSession,
		);
	});
}

#[test]
fn report_vote_equivocation_invalid_session() {
	let authorities = test_authorities();

	new_test_ext_raw_authorities(authorities).execute_with(|| {
		start_era(1);

		let block_num = System::block_number();
		let validator_set = Beefy::validator_set().unwrap();
		let authorities = validator_set.validators();

		let equivocation_authority_index = 0;
		let equivocation_key = &authorities[equivocation_authority_index];
		let equivocation_keyring = BeefyKeyring::from_public(equivocation_key).unwrap();

		// generate a key ownership proof at current era set id
		let key_owner_proof = Historical::prove((BEEFY_KEY_TYPE, &equivocation_key)).unwrap();

		start_era(2);

		let set_id = Beefy::validator_set().unwrap().id();

		let payload1 = Payload::from_single_entry(MMR_ROOT_ID, vec![42]);
		let payload2 = Payload::from_single_entry(MMR_ROOT_ID, vec![128]);
		// generate an equivocation proof at following era set id = 2
		let equivocation_proof = generate_vote_equivocation_proof(
			(block_num, payload1, set_id, &equivocation_keyring),
			(block_num, payload2, set_id, &equivocation_keyring),
		);

		// report an equivocation for the current set using an key ownership
		// proof from the previous set, the session should be invalid.
		assert_err!(
			Beefy::report_vote_equivocation_unsigned(
				RuntimeOrigin::none(),
				Box::new(equivocation_proof),
				key_owner_proof,
			),
			Error::<Test>::InvalidEquivocationProofSession,
		);
	});
}

#[test]
fn report_vote_equivocation_invalid_key_owner_proof() {
	let authorities = test_authorities();

	new_test_ext_raw_authorities(authorities).execute_with(|| {
		start_era(1);

		let block_num = System::block_number();
		let validator_set = Beefy::validator_set().unwrap();
		let authorities = validator_set.validators();
		let set_id = validator_set.id();

		let invalid_owner_authority_index = 1;
		let invalid_owner_key = &authorities[invalid_owner_authority_index];

		// generate a key ownership proof for the authority at index 1
		let invalid_key_owner_proof =
			Historical::prove((BEEFY_KEY_TYPE, &invalid_owner_key)).unwrap();

		let equivocation_authority_index = 0;
		let equivocation_key = &authorities[equivocation_authority_index];
		let equivocation_keyring = BeefyKeyring::from_public(equivocation_key).unwrap();

		let payload1 = Payload::from_single_entry(MMR_ROOT_ID, vec![42]);
		let payload2 = Payload::from_single_entry(MMR_ROOT_ID, vec![128]);
		// generate an equivocation proof for the authority at index 0
		let equivocation_proof = generate_vote_equivocation_proof(
			(block_num, payload1, set_id, &equivocation_keyring),
			(block_num, payload2, set_id, &equivocation_keyring),
		);

		// we need to start a new era otherwise the key ownership proof won't be
		// checked since the authorities are part of the current session
		start_era(2);

		// report an equivocation for the current set using a key ownership
		// proof for a different key than the one in the equivocation proof.
		assert_err!(
			Beefy::report_vote_equivocation_unsigned(
				RuntimeOrigin::none(),
				Box::new(equivocation_proof),
				invalid_key_owner_proof,
			),
			Error::<Test>::InvalidKeyOwnershipProof,
		);
	});
}

#[test]
fn report_vote_equivocation_invalid_equivocation_proof() {
	let authorities = test_authorities();

	new_test_ext_raw_authorities(authorities).execute_with(|| {
		start_era(1);

		let block_num = System::block_number();
		let validator_set = Beefy::validator_set().unwrap();
		let authorities = validator_set.validators();
		let set_id = validator_set.id();

		let equivocation_authority_index = 0;
		let equivocation_key = &authorities[equivocation_authority_index];
		let equivocation_keyring = BeefyKeyring::from_public(equivocation_key).unwrap();

		// generate a key ownership proof at set id in era 1
		let key_owner_proof = Historical::prove((BEEFY_KEY_TYPE, &equivocation_key)).unwrap();

		let assert_invalid_equivocation_proof = |equivocation_proof| {
			assert_err!(
				Beefy::report_vote_equivocation_unsigned(
					RuntimeOrigin::none(),
					Box::new(equivocation_proof),
					key_owner_proof.clone(),
				),
				Error::<Test>::InvalidVoteEquivocationProof,
			);
		};

		start_era(2);

		let payload1 = Payload::from_single_entry(MMR_ROOT_ID, vec![42]);
		let payload2 = Payload::from_single_entry(MMR_ROOT_ID, vec![128]);

		// both votes target the same block number and payload,
		// there is no equivocation.
		assert_invalid_equivocation_proof(generate_vote_equivocation_proof(
			(block_num, payload1.clone(), set_id, &equivocation_keyring),
			(block_num, payload1.clone(), set_id, &equivocation_keyring),
		));

		// votes targeting different rounds, there is no equivocation.
		assert_invalid_equivocation_proof(generate_vote_equivocation_proof(
			(block_num, payload1.clone(), set_id, &equivocation_keyring),
			(block_num + 1, payload2.clone(), set_id, &equivocation_keyring),
		));

		// votes signed with different authority keys
		assert_invalid_equivocation_proof(generate_vote_equivocation_proof(
			(block_num, payload1.clone(), set_id, &equivocation_keyring),
			(block_num, payload1.clone(), set_id, &BeefyKeyring::Charlie),
		));

		// votes signed with a key that isn't part of the authority set
		assert_invalid_equivocation_proof(generate_vote_equivocation_proof(
			(block_num, payload1.clone(), set_id, &equivocation_keyring),
			(block_num, payload1.clone(), set_id, &BeefyKeyring::Dave),
		));

		// votes targeting different set ids
		assert_invalid_equivocation_proof(generate_vote_equivocation_proof(
			(block_num, payload1, set_id, &equivocation_keyring),
			(block_num, payload2, set_id + 1, &equivocation_keyring),
		));
	});
}

#[test]
fn report_vote_equivocation_validate_unsigned_prevents_duplicates() {
	use sp_runtime::transaction_validity::{
		InvalidTransaction, TransactionPriority, TransactionSource, TransactionValidity,
		ValidTransaction,
	};

	let authorities = test_authorities();

	new_test_ext_raw_authorities(authorities).execute_with(|| {
		start_era(1);

		let block_num = System::block_number();
		let validator_set = Beefy::validator_set().unwrap();
		let authorities = validator_set.validators();
		let set_id = validator_set.id();

		// generate and report an equivocation for the validator at index 0
		let equivocation_authority_index = 0;
		let equivocation_key = &authorities[equivocation_authority_index];
		let equivocation_keyring = BeefyKeyring::from_public(equivocation_key).unwrap();

		let payload1 = Payload::from_single_entry(MMR_ROOT_ID, vec![42]);
		let payload2 = Payload::from_single_entry(MMR_ROOT_ID, vec![128]);
		let equivocation_proof = generate_vote_equivocation_proof(
			(block_num, payload1, set_id, &equivocation_keyring),
			(block_num, payload2, set_id, &equivocation_keyring),
		);

		let key_owner_proof = Historical::prove((BEEFY_KEY_TYPE, &equivocation_key)).unwrap();

		let call = Call::report_vote_equivocation_unsigned {
			equivocation_proof: Box::new(equivocation_proof.clone()),
			key_owner_proof: key_owner_proof.clone(),
		};

		// only local/inblock reports are allowed
		assert_eq!(
			<Beefy as sp_runtime::traits::ValidateUnsigned>::validate_unsigned(
				TransactionSource::External,
				&call,
			),
			InvalidTransaction::Call.into(),
		);

		// the transaction is valid when passed as local
		let tx_tag = (equivocation_key, set_id, 3u64);

		assert_eq!(
			<Beefy as sp_runtime::traits::ValidateUnsigned>::validate_unsigned(
				TransactionSource::Local,
				&call,
			),
			TransactionValidity::Ok(ValidTransaction {
				priority: TransactionPriority::max_value(),
				requires: vec![],
				provides: vec![("BeefyEquivocation", tx_tag).encode()],
				longevity: ReportLongevity::get(),
				propagate: false,
			})
		);

		// the pre dispatch checks should also pass
		assert_ok!(<Beefy as sp_runtime::traits::ValidateUnsigned>::pre_dispatch(&call));

		// we submit the report
		Beefy::report_vote_equivocation_unsigned(
			RuntimeOrigin::none(),
			Box::new(equivocation_proof),
			key_owner_proof,
		)
		.unwrap();

		// the report should now be considered stale and the transaction is invalid
		// the check for staleness should be done on both `validate_unsigned` and on `pre_dispatch`
		assert_err!(
			<Beefy as sp_runtime::traits::ValidateUnsigned>::validate_unsigned(
				TransactionSource::Local,
				&call,
			),
			InvalidTransaction::Stale,
		);

		assert_err!(
			<Beefy as sp_runtime::traits::ValidateUnsigned>::pre_dispatch(&call),
			InvalidTransaction::Stale,
		);
	});
}

#[test]
fn report_vote_equivocation_has_valid_weight() {
	// the weight depends on the size of the validator set,
	// but there's a lower bound of 100 validators.
	assert!((1..=100)
		.map(|validators| <Test as Config>::WeightInfo::report_vote_equivocation(validators, 1000))
		.collect::<Vec<_>>()
		.windows(2)
		.all(|w| w[0] == w[1]));

	// after 100 validators the weight should keep increasing
	// with every extra validator.
	assert!((100..=1000)
		.map(|validators| <Test as Config>::WeightInfo::report_vote_equivocation(validators, 1000))
		.collect::<Vec<_>>()
		.windows(2)
		.all(|w| w[0].ref_time() < w[1].ref_time()));
}

#[test]
fn valid_vote_equivocation_reports_dont_pay_fees() {
	let authorities = test_authorities();

	new_test_ext_raw_authorities(authorities).execute_with(|| {
		start_era(1);

		let block_num = System::block_number();
		let validator_set = Beefy::validator_set().unwrap();
		let authorities = validator_set.validators();
		let set_id = validator_set.id();

		let equivocation_authority_index = 0;
		let equivocation_key = &authorities[equivocation_authority_index];
		let equivocation_keyring = BeefyKeyring::from_public(equivocation_key).unwrap();

		// generate equivocation proof
		let payload1 = Payload::from_single_entry(MMR_ROOT_ID, vec![42]);
		let payload2 = Payload::from_single_entry(MMR_ROOT_ID, vec![128]);
		let equivocation_proof = generate_vote_equivocation_proof(
			(block_num, payload1, set_id, &equivocation_keyring),
			(block_num, payload2, set_id, &equivocation_keyring),
		);

		// create the key ownership proof.
		let key_owner_proof = Historical::prove((BEEFY_KEY_TYPE, &equivocation_key)).unwrap();

		// check the dispatch info for the call.
		let info = Call::<Test>::report_vote_equivocation_unsigned {
			equivocation_proof: Box::new(equivocation_proof.clone()),
			key_owner_proof: key_owner_proof.clone(),
		}
		.get_dispatch_info();

		// it should have non-zero weight and the fee has to be paid.
		assert!(info.weight.any_gt(Weight::zero()));
		assert_eq!(info.pays_fee, Pays::Yes);

		// report the equivocation.
		let post_info = Beefy::report_vote_equivocation_unsigned(
			RuntimeOrigin::none(),
			Box::new(equivocation_proof.clone()),
			key_owner_proof.clone(),
		)
		.unwrap();

		// the original weight should be kept, but given that the report
		// is valid the fee is waived.
		assert!(post_info.actual_weight.is_none());
		assert_eq!(post_info.pays_fee, Pays::No);

		// report the equivocation again which is invalid now since it is
		// duplicate.
		let post_info = Beefy::report_vote_equivocation_unsigned(
			RuntimeOrigin::none(),
			Box::new(equivocation_proof),
			key_owner_proof,
		)
		.err()
		.unwrap()
		.post_info;

		// the fee is not waived and the original weight is kept.
		assert!(post_info.actual_weight.is_none());
		assert_eq!(post_info.pays_fee, Pays::Yes);
	})
}

// fork equivocation (via vote) report tests
// TODO: deduplicate by extracting common test structure of equivocation classes
#[test]
fn report_fork_equivocation_vote_current_set_works() {
	let authorities = test_authorities();

	let mut ext = new_test_ext_raw_authorities(authorities);
	let (offchain, _offchain_state) = TestOffchainExt::with_offchain_db(ext.offchain_db());
	ext.register_extension(OffchainDbExt::new(offchain.clone()));
	ext.register_extension(OffchainWorkerExt::new(offchain));

	let mut era = 1;
	let block_num = ext.execute_with(|| {
		assert_eq!(Staking::current_era(), Some(0));
		assert_eq!(Session::current_index(), 0);
		start_era(era);

		let block_num = System::block_number();
		era += 1;
		start_era(era);
		block_num
	});
	ext.persist_offchain_overlay();

	ext.execute_with(|| {
		let validator_set = Beefy::validator_set().unwrap();
		let authorities = validator_set.validators();
		let set_id = validator_set.id();
		let validators = Session::validators();

		// make sure that all validators have the same balance
		for validator in &validators {
			assert_eq!(Balances::total_balance(validator), 10_000_000);
			assert_eq!(Staking::slashable_balance_of(validator), 10_000);

			assert_eq!(
				Staking::eras_stakers(era, validator),
				pallet_staking::Exposure { total: 10_000, own: 10_000, others: vec![] },
			);
		}

		assert_eq!(authorities.len(), 3);
		let equivocation_authority_index = 1;
		let equivocation_key = &authorities[equivocation_authority_index];
		let equivocation_keyring = BeefyKeyring::from_public(equivocation_key).unwrap();

		let payload = Payload::from_single_entry(MMR_ROOT_ID, vec![42]);
		let ancestry_proof = Mmr::generate_ancestry_proof(block_num, None).unwrap();

		// generate an fork equivocation proof, with a vote in the same round for a
		// different payload than finalized
		let equivocation_proof = generate_fork_equivocation_proof_vote(
			(block_num, payload, set_id, &equivocation_keyring),
			None,
			Some(ancestry_proof),
		);

		// create the key ownership proof
		let key_owner_proof = Historical::prove((BEEFY_KEY_TYPE, &equivocation_key)).unwrap();

		// report the equivocation and the tx should be dispatched successfully
		assert_ok!(Beefy::report_fork_equivocation_unsigned(
			RuntimeOrigin::none(),
			Box::new(equivocation_proof),
			vec![key_owner_proof],
		),);

		era += 1;
		start_era(era);

		// check that the balance of 0-th validator is slashed 100%.
		let equivocation_validator_id = validators[equivocation_authority_index];

		assert_eq!(Balances::total_balance(&equivocation_validator_id), 10_000_000 - 10_000);
		assert_eq!(Staking::slashable_balance_of(&equivocation_validator_id), 0);
		assert_eq!(
			Staking::eras_stakers(era, &equivocation_validator_id),
			pallet_staking::Exposure { total: 0, own: 0, others: vec![] },
		);

		// check that the balances of all other validators are left intact.
		for validator in &validators {
			if *validator == equivocation_validator_id {
				continue
			}

			assert_eq!(Balances::total_balance(validator), 10_000_000);
			assert_eq!(Staking::slashable_balance_of(validator), 10_000);

			assert_eq!(
				Staking::eras_stakers(era, validator),
				pallet_staking::Exposure { total: 10_000, own: 10_000, others: vec![] },
			);
		}
	});
}

#[test]
fn report_fork_equivocation_vote_old_set_works() {
	let authorities = test_authorities();

	let mut ext = new_test_ext_raw_authorities(authorities);
	let (offchain, _offchain_state) = TestOffchainExt::with_offchain_db(ext.offchain_db());
	ext.register_extension(OffchainDbExt::new(offchain.clone()));
	ext.register_extension(OffchainWorkerExt::new(offchain));

	let mut era = 1;
	let (
		block_num,
		validators,
		old_set_id,
		equivocation_authority_index,
		equivocation_key,
		key_owner_proof,
	) = ext.execute_with(|| {
		start_era(era);
		let block_num = System::block_number();
		era += 1;
		start_era(era);

		let validator_set = Beefy::validator_set().unwrap();
		let authorities = validator_set.validators();
		let validators = Session::validators();
		let old_set_id = validator_set.id();

		assert_eq!(authorities.len(), 3);
		let equivocation_authority_index = 0;
		let equivocation_key = authorities[equivocation_authority_index].clone();

		// create the key ownership proof in the "old" set
		let key_owner_proof = Historical::prove((BEEFY_KEY_TYPE, &&equivocation_key)).unwrap();

		era += 1;
		start_era(era);
		(
			block_num,
			validators,
			old_set_id,
			equivocation_authority_index,
			equivocation_key,
			key_owner_proof,
		)
	});
	ext.persist_offchain_overlay();

	ext.execute_with(|| {
		// make sure that all authorities have the same balance
		for validator in &validators {
			assert_eq!(Balances::total_balance(validator), 10_000_000);
			assert_eq!(Staking::slashable_balance_of(validator), 10_000);

			assert_eq!(
				Staking::eras_stakers(2, validator),
				pallet_staking::Exposure { total: 10_000, own: 10_000, others: vec![] },
			);
		}

		let validator_set = Beefy::validator_set().unwrap();
		let new_set_id = validator_set.id();
		assert_eq!(old_set_id + 3, new_set_id);

		let equivocation_keyring = BeefyKeyring::from_public(&equivocation_key).unwrap();

		let payload = Payload::from_single_entry(MMR_ROOT_ID, vec![42]);
		let ancestry_proof = Mmr::generate_ancestry_proof(block_num, None).unwrap();

		// generate an fork equivocation proof, with a vote in the same round for a
		// different payload than finalized
		let equivocation_proof = generate_fork_equivocation_proof_vote(
			(block_num, payload, old_set_id, &equivocation_keyring),
			None,
			Some(ancestry_proof),
		);

		// report the equivocation and the tx should be dispatched successfully
		assert_ok!(Beefy::report_fork_equivocation_unsigned(
			RuntimeOrigin::none(),
			Box::new(equivocation_proof),
			vec![key_owner_proof],
		),);

		era += 1;
		start_era(era);

		// check that the balance of 0-th validator is slashed 100%.
		let equivocation_validator_id = validators[equivocation_authority_index];

		assert_eq!(Balances::total_balance(&equivocation_validator_id), 10_000_000 - 10_000);
		assert_eq!(Staking::slashable_balance_of(&equivocation_validator_id), 0);
		assert_eq!(
			Staking::eras_stakers(era, &equivocation_validator_id),
			pallet_staking::Exposure { total: 0, own: 0, others: vec![] },
		);

		// check that the balances of all other validators are left intact.
		for validator in &validators {
			if *validator == equivocation_validator_id {
				continue
			}

			assert_eq!(Balances::total_balance(validator), 10_000_000);
			assert_eq!(Staking::slashable_balance_of(validator), 10_000);

			assert_eq!(
				Staking::eras_stakers(3, validator),
				pallet_staking::Exposure { total: 10_000, own: 10_000, others: vec![] },
			);
		}
	});
}

#[test]
fn report_fork_equivocation_vote_future_block_works() {
	let authorities = test_authorities();

	let mut ext = new_test_ext_raw_authorities(authorities);
	let (offchain, _offchain_state) = TestOffchainExt::with_offchain_db(ext.offchain_db());
	ext.register_extension(OffchainDbExt::new(offchain.clone()));
	ext.register_extension(OffchainWorkerExt::new(offchain));

	let mut era = 2;
	ext.execute_with(|| {
		assert_eq!(Staking::current_era(), Some(0));
		assert_eq!(Session::current_index(), 0);
		start_era(era);
	});
	ext.persist_offchain_overlay();

	ext.execute_with(|| {
		let validator_set = Beefy::validator_set().unwrap();
		let authorities = validator_set.validators();
		let set_id = validator_set.id();
		let validators = Session::validators();

		// make sure that all validators have the same balance
		for validator in &validators {
			assert_eq!(Balances::total_balance(validator), 10_000_000);
			assert_eq!(Staking::slashable_balance_of(validator), 10_000);

			assert_eq!(
				Staking::eras_stakers(era, validator),
				pallet_staking::Exposure { total: 10_000, own: 10_000, others: vec![] },
			);
		}

		assert_eq!(authorities.len(), 3);
		let equivocation_authority_index = 1;
		let equivocation_key = &authorities[equivocation_authority_index];
		let equivocation_keyring = BeefyKeyring::from_public(equivocation_key).unwrap();

		let payload = Payload::from_single_entry(MMR_ROOT_ID, vec![42]);
		let block_num = System::block_number() + 20;

		// generate an fork equivocation proof, with a vote in the same round for a future block
		let equivocation_proof = generate_fork_equivocation_proof_vote(
			(block_num, payload, set_id, &equivocation_keyring),
			None,
			None,
		);

		// create the key ownership proof
		let key_owner_proof = Historical::prove((BEEFY_KEY_TYPE, &equivocation_key)).unwrap();

		// report the equivocation and the tx should be dispatched successfully
		assert_ok!(Beefy::report_fork_equivocation_unsigned(
			RuntimeOrigin::none(),
			Box::new(equivocation_proof),
			vec![key_owner_proof],
		),);

		era += 1;
		start_era(era);

		// check that the balance of 0-th validator is slashed 100%.
		let equivocation_validator_id = validators[equivocation_authority_index];

		assert_eq!(Balances::total_balance(&equivocation_validator_id), 10_000_000 - 10_000);
		assert_eq!(Staking::slashable_balance_of(&equivocation_validator_id), 0);
		assert_eq!(
			Staking::eras_stakers(era, &equivocation_validator_id),
			pallet_staking::Exposure { total: 0, own: 0, others: vec![] },
		);

		// check that the balances of all other validators are left intact.
		for validator in &validators {
			if *validator == equivocation_validator_id {
				continue
			}

			assert_eq!(Balances::total_balance(validator), 10_000_000);
			assert_eq!(Staking::slashable_balance_of(validator), 10_000);

			assert_eq!(
				Staking::eras_stakers(era, validator),
				pallet_staking::Exposure { total: 10_000, own: 10_000, others: vec![] },
			);
		}
	});
}

#[test]
fn report_fork_equivocation_vote_invalid_set_id() {
	let authorities = test_authorities();

	let mut ext = new_test_ext_raw_authorities(authorities);
	let (offchain, _offchain_state) = TestOffchainExt::with_offchain_db(ext.offchain_db());
	ext.register_extension(OffchainDbExt::new(offchain.clone()));
	ext.register_extension(OffchainWorkerExt::new(offchain));

	let block_num = ext.execute_with(|| {
		let mut era = 1;
		start_era(era);
		let block_num = System::block_number();

		era += 1;
		start_era(era);
		block_num
	});
	ext.persist_offchain_overlay();

	ext.execute_with(|| {
		let validator_set = Beefy::validator_set().unwrap();
		let authorities = validator_set.validators();
		let set_id = validator_set.id();

		let equivocation_authority_index = 0;
		let equivocation_key = &authorities[equivocation_authority_index];
		let equivocation_keyring = BeefyKeyring::from_public(equivocation_key).unwrap();

		let key_owner_proof = Historical::prove((BEEFY_KEY_TYPE, &equivocation_key)).unwrap();

		let payload = Payload::from_single_entry(MMR_ROOT_ID, vec![42]);
		let ancestry_proof = Mmr::generate_ancestry_proof(block_num, None).unwrap();
		// generate an equivocation for a future set
		let equivocation_proof = generate_fork_equivocation_proof_vote(
			(block_num, payload, set_id + 1, &equivocation_keyring),
			None,
			Some(ancestry_proof),
		);

		// the call for reporting the equivocation should error
		assert_err!(
			Beefy::report_fork_equivocation_unsigned(
				RuntimeOrigin::none(),
				Box::new(equivocation_proof),
				vec![key_owner_proof],
			),
			Error::<Test>::InvalidEquivocationProofSession,
		);
	});
}

#[test]
fn report_fork_equivocation_vote_invalid_session() {
	let authorities = test_authorities();

	let mut ext = new_test_ext_raw_authorities(authorities);
	let (offchain, _offchain_state) = TestOffchainExt::with_offchain_db(ext.offchain_db());
	ext.register_extension(OffchainDbExt::new(offchain.clone()));
	ext.register_extension(OffchainWorkerExt::new(offchain));

	let mut era = 1;
	let (block_num, equivocation_keyring, key_owner_proof) = ext.execute_with(|| {
		start_era(era);
		let block_num = System::block_number();

		era += 1;
		start_era(era);

		let validator_set = Beefy::validator_set().unwrap();
		let authorities = validator_set.validators();

		let equivocation_authority_index = 0;
		let equivocation_key = &authorities[equivocation_authority_index];
		let equivocation_keyring = BeefyKeyring::from_public(equivocation_key).unwrap();

		// generate a key ownership proof at current era set id
		let key_owner_proof = Historical::prove((BEEFY_KEY_TYPE, &equivocation_key)).unwrap();

		era += 1;
		start_era(era);
		(block_num, equivocation_keyring, key_owner_proof)
	});
	ext.persist_offchain_overlay();

	ext.execute_with(|| {
		let set_id = Beefy::validator_set().unwrap().id();

		let payload = Payload::from_single_entry(MMR_ROOT_ID, vec![42]);
		let ancestry_proof = Mmr::generate_ancestry_proof(block_num, None).unwrap();
		// generate an equivocation proof at following era set id = 3
		let equivocation_proof = generate_fork_equivocation_proof_vote(
			(block_num, payload, set_id, &equivocation_keyring),
			None,
			Some(ancestry_proof),
		);

		// report an equivocation for the current set using an key ownership
		// proof from the previous set, the session should be invalid.
		assert_err!(
			Beefy::report_fork_equivocation_unsigned(
				RuntimeOrigin::none(),
				Box::new(equivocation_proof),
				vec![key_owner_proof],
			),
			Error::<Test>::InvalidEquivocationProofSession,
		);
	});
}

#[test]
fn report_fork_equivocation_vote_invalid_key_owner_proof() {
	let authorities = test_authorities();

	let mut ext = new_test_ext_raw_authorities(authorities);
	let (offchain, _offchain_state) = TestOffchainExt::with_offchain_db(ext.offchain_db());
	ext.register_extension(OffchainDbExt::new(offchain.clone()));
	ext.register_extension(OffchainWorkerExt::new(offchain));

	let mut era = 1;
	let block_num = ext.execute_with(|| {
		start_era(era);
		let block_num = System::block_number();

		era += 1;
		start_era(era);
		block_num
	});
	ext.persist_offchain_overlay();

	ext.execute_with(|| {
		let validator_set = Beefy::validator_set().unwrap();
		let authorities = validator_set.validators();
		let set_id = validator_set.id();

		let invalid_owner_authority_index = 1;
		let invalid_owner_key = &authorities[invalid_owner_authority_index];

		// generate a key ownership proof for the authority at index 1
		let invalid_key_owner_proof =
			Historical::prove((BEEFY_KEY_TYPE, &invalid_owner_key)).unwrap();

		let equivocation_authority_index = 0;
		let equivocation_key = &authorities[equivocation_authority_index];
		let equivocation_keyring = BeefyKeyring::from_public(equivocation_key).unwrap();

		let payload = Payload::from_single_entry(MMR_ROOT_ID, vec![42]);
		let ancestry_proof = Mmr::generate_ancestry_proof(block_num, None).unwrap();
		// generate an equivocation for a future set
		let equivocation_proof = generate_fork_equivocation_proof_vote(
			(block_num, payload, set_id, &equivocation_keyring),
			None,
			Some(ancestry_proof),
		);

		// we need to start a new era otherwise the key ownership proof won't be
		// checked since the authorities are part of the current session
		era += 1;
		start_era(era);

		// report an equivocation for the current set using a key ownership
		// proof for a different key than the one in the equivocation proof.
		assert_err!(
			Beefy::report_fork_equivocation_unsigned(
				RuntimeOrigin::none(),
				Box::new(equivocation_proof),
				vec![invalid_key_owner_proof],
			),
			Error::<Test>::InvalidKeyOwnershipProof,
		);
	});
}

#[test]
fn report_fork_equivocation_vote_invalid_equivocation_proof() {
	let authorities = test_authorities();

	let mut ext = new_test_ext_raw_authorities(authorities);
	let (offchain, _offchain_state) = TestOffchainExt::with_offchain_db(ext.offchain_db());
	ext.register_extension(OffchainDbExt::new(offchain.clone()));
	ext.register_extension(OffchainWorkerExt::new(offchain));

	let mut era = 1;
	let (block_num, set_id, equivocation_keyring, key_owner_proof) = ext.execute_with(|| {
		start_era(era);
		let block_num = System::block_number();

		let validator_set = Beefy::validator_set().unwrap();
		let authorities = validator_set.validators();
		let set_id = validator_set.id();

		let equivocation_authority_index = 0;
		let equivocation_key = &authorities[equivocation_authority_index];
		let equivocation_keyring = BeefyKeyring::from_public(equivocation_key).unwrap();

		// generate a key ownership proof at set id in era 1
		let key_owner_proof = Historical::prove((BEEFY_KEY_TYPE, &equivocation_key)).unwrap();

		era += 1;
		start_era(era);
		(block_num, set_id, equivocation_keyring, key_owner_proof)
	});
	ext.persist_offchain_overlay();

	ext.execute_with(|| {
		let payload = Payload::from_single_entry(MMR_ROOT_ID, vec![42]);
		let ancestry_proof = Mmr::generate_ancestry_proof(block_num, None).unwrap();

		// vote targets different round than finalized payload, there is no equivocation.
		let equivocation_proof = generate_fork_equivocation_proof_vote(
			(block_num + 1, payload.clone(), set_id, &equivocation_keyring),
			None,
			Some(ancestry_proof.clone()),
		);
		assert_err!(
			Beefy::report_fork_equivocation_unsigned(
				RuntimeOrigin::none(),
				Box::new(equivocation_proof),
				vec![key_owner_proof.clone()],
			),
			Error::<Test>::InvalidForkEquivocationProof,
		);

		// vote signed with a key that isn't part of the authority set
		let equivocation_proof = generate_fork_equivocation_proof_vote(
			(block_num, payload.clone(), set_id, &BeefyKeyring::Dave),
			None,
			Some(ancestry_proof.clone()),
		);
		assert_err!(
			Beefy::report_fork_equivocation_unsigned(
				RuntimeOrigin::none(),
				Box::new(equivocation_proof),
				vec![key_owner_proof.clone()],
			),
			Error::<Test>::InvalidKeyOwnershipProof,
		);

		// vote targets future set id
		let equivocation_proof = generate_fork_equivocation_proof_vote(
			(block_num, payload.clone(), set_id + 1, &equivocation_keyring),
			None,
			Some(ancestry_proof),
		);
		assert_err!(
			Beefy::report_fork_equivocation_unsigned(
				RuntimeOrigin::none(),
				Box::new(equivocation_proof),
				vec![key_owner_proof.clone()],
			),
			Error::<Test>::InvalidEquivocationProofSession,
		);
	});
}

#[test]
fn report_fork_equivocation_vote_validate_unsigned_prevents_duplicates() {
	use sp_runtime::transaction_validity::{
		InvalidTransaction, TransactionPriority, TransactionSource, TransactionValidity,
		ValidTransaction,
	};

	let authorities = test_authorities();

	let mut ext = new_test_ext_raw_authorities(authorities);
	let (offchain, _offchain_state) = TestOffchainExt::with_offchain_db(ext.offchain_db());
	ext.register_extension(OffchainDbExt::new(offchain.clone()));
	ext.register_extension(OffchainWorkerExt::new(offchain));

	let mut era = 1;
	let block_num = ext.execute_with(|| {
		start_era(era);
		let block_num = System::block_number();

		era += 1;
		start_era(era);
		block_num
	});
	ext.persist_offchain_overlay();

	ext.execute_with(|| {
		let validator_set = Beefy::validator_set().unwrap();
		let authorities = validator_set.validators();
		let set_id = validator_set.id();

		// generate and report an equivocation for the validator at index 0
		let equivocation_authority_index = 0;
		let equivocation_key = &authorities[equivocation_authority_index];
		let equivocation_keyring = BeefyKeyring::from_public(equivocation_key).unwrap();

		let payload = Payload::from_single_entry(MMR_ROOT_ID, vec![42]);
		let ancestry_proof = Mmr::generate_ancestry_proof(block_num, None).unwrap();
		let equivocation_proof = generate_fork_equivocation_proof_vote(
			(block_num, payload, set_id, &equivocation_keyring),
			None,
			Some(ancestry_proof),
		);

		let key_owner_proof = Historical::prove((BEEFY_KEY_TYPE, &equivocation_key)).unwrap();

		let call = Call::report_fork_equivocation_unsigned {
			equivocation_proof: Box::new(equivocation_proof.clone()),
			key_owner_proofs: vec![key_owner_proof.clone()],
		};

		// only local/inblock reports are allowed
		assert_eq!(
			<Beefy as sp_runtime::traits::ValidateUnsigned>::validate_unsigned(
				TransactionSource::External,
				&call,
			),
			InvalidTransaction::Call.into(),
		);

		// the transaction is valid when passed as local
		let tx_tag = (vec![equivocation_key], set_id, 3u64);

		let call_result = <Beefy as sp_runtime::traits::ValidateUnsigned>::validate_unsigned(
			TransactionSource::Local,
			&call,
		);

		assert_eq!(
			call_result,
			TransactionValidity::Ok(ValidTransaction {
				priority: TransactionPriority::max_value(),
				requires: vec![],
				provides: vec![("BeefyEquivocation", tx_tag.clone()).encode()],
				longevity: ReportLongevity::get(),
				propagate: false,
			})
		);

		assert_ok!(<Beefy as sp_runtime::traits::ValidateUnsigned>::pre_dispatch(&call));

		// we submit the report
		Beefy::report_fork_equivocation_unsigned(
			RuntimeOrigin::none(),
			Box::new(equivocation_proof),
			vec![key_owner_proof],
		)
		.unwrap();

		// the report should now be considered stale and the transaction is invalid
		// the check for staleness should be done on both `validate_unsigned` and on `pre_dispatch`
		assert_err!(
			<Beefy as sp_runtime::traits::ValidateUnsigned>::validate_unsigned(
				TransactionSource::Local,
				&call,
			),
			InvalidTransaction::Stale,
		);

		assert_err!(
			<Beefy as sp_runtime::traits::ValidateUnsigned>::pre_dispatch(&call),
			InvalidTransaction::Stale,
		);
	});
}

#[test]
fn valid_fork_equivocation_vote_reports_dont_pay_fees() {
	let authorities = test_authorities();

	let mut ext = new_test_ext_raw_authorities(authorities);
	let (offchain, _offchain_state) = TestOffchainExt::with_offchain_db(ext.offchain_db());
	ext.register_extension(OffchainDbExt::new(offchain.clone()));
	ext.register_extension(OffchainWorkerExt::new(offchain));

	let mut era = 1;
	let block_num = ext.execute_with(|| {
		start_era(era);
		let block_num = System::block_number();

		era += 1;
		start_era(era);
		block_num
	});
	ext.persist_offchain_overlay();

	ext.execute_with(|| {
		let validator_set = Beefy::validator_set().unwrap();
		let authorities = validator_set.validators();
		let set_id = validator_set.id();

		let equivocation_authority_index = 0;
		let equivocation_key = &authorities[equivocation_authority_index];
		let equivocation_keyring = BeefyKeyring::from_public(equivocation_key).unwrap();

		// generate equivocation proof
		let payload = Payload::from_single_entry(MMR_ROOT_ID, vec![42]);
		let ancestry_proof = Mmr::generate_ancestry_proof(block_num, None).unwrap();
		let equivocation_proof = generate_fork_equivocation_proof_vote(
			(block_num, payload, set_id, &equivocation_keyring),
			None,
			Some(ancestry_proof),
		);

		// create the key ownership proof.
		let key_owner_proof = Historical::prove((BEEFY_KEY_TYPE, &equivocation_key)).unwrap();

		// check the dispatch info for the call.
		let info = Call::<Test>::report_fork_equivocation_unsigned {
			equivocation_proof: Box::new(equivocation_proof.clone()),
			key_owner_proofs: vec![key_owner_proof.clone()],
		}
		.get_dispatch_info();

		// it should have non-zero weight and the fee has to be paid.
		assert!(info.weight.any_gt(Weight::zero()));
		assert_eq!(info.pays_fee, Pays::Yes);

		// report the equivocation.
		let post_info = Beefy::report_fork_equivocation_unsigned(
			RuntimeOrigin::none(),
			Box::new(equivocation_proof.clone()),
			vec![key_owner_proof.clone()],
		)
		.unwrap();

		// the original weight should be kept, but given that the report
		// is valid the fee is waived.
		assert!(post_info.actual_weight.is_none());
		assert_eq!(post_info.pays_fee, Pays::No);

		// report the equivocation again which is invalid now since it is
		// duplicate.
		let post_info = Beefy::report_fork_equivocation_unsigned(
			RuntimeOrigin::none(),
			Box::new(equivocation_proof.clone()),
			vec![key_owner_proof.clone()],
		)
		.err()
		.unwrap()
		.post_info;

		// the fee is not waived and the original weight is kept.
		assert!(post_info.actual_weight.is_none());
		assert_eq!(post_info.pays_fee, Pays::Yes);
	})
}

// fork equivocation (via signed commitment) report tests
// TODO: deduplicate by extracting common test structure of equivocation classes
#[test]
fn report_fork_equivocation_sc_current_set_works() {
	let authorities = test_authorities();

	let mut ext = new_test_ext_raw_authorities(authorities);
	let (offchain, _offchain_state) = TestOffchainExt::with_offchain_db(ext.offchain_db());
	ext.register_extension(OffchainDbExt::new(offchain.clone()));
	ext.register_extension(OffchainWorkerExt::new(offchain));

	let mut era = 1;
	let block_num = ext.execute_with(|| {
		start_era(era);
		let block_num = System::block_number();

		era += 1;
		start_era(era);
		block_num
	});
	ext.persist_offchain_overlay();

	ext.execute_with(|| {
		let validator_set = Beefy::validator_set().unwrap();
		let authorities = validator_set.validators();
		let set_id = validator_set.id();
		let validators = Session::validators();

		// make sure that all validators have the same balance
		for validator in &validators {
			assert_eq!(Balances::total_balance(validator), 10_000_000);
			assert_eq!(Staking::slashable_balance_of(validator), 10_000);

			assert_eq!(
				Staking::eras_stakers(era, validator),
				pallet_staking::Exposure { total: 10_000, own: 10_000, others: vec![] },
			);
		}

		assert_eq!(authorities.len(), 3);
		let equivocation_authority_indices = [0, 2];
		let equivocation_keys = equivocation_authority_indices
			.iter()
			.map(|i| &authorities[*i])
			.collect::<Vec<_>>();
		let equivocation_keyrings = equivocation_keys
			.iter()
			.map(|k| BeefyKeyring::from_public(k.clone()).unwrap())
			.collect();

		let payload = Payload::from_single_entry(MMR_ROOT_ID, vec![42]);
		let ancestry_proof = Mmr::generate_ancestry_proof(block_num, None).unwrap();
		let commitment = Commitment { validator_set_id: set_id, block_number: block_num, payload };
		// generate an fork equivocation proof, with a vote in the same round for a
		// different payload than finalized
		let equivocation_proof = generate_fork_equivocation_proof_sc(
			commitment,
			equivocation_keyrings,
			None,
			Some(ancestry_proof),
		);

		// create the key ownership proof
		let key_owner_proofs = equivocation_keys
			.iter()
			.map(|k| Historical::prove((BEEFY_KEY_TYPE, &k)).unwrap())
			.collect();

		// report the equivocation and the tx should be dispatched successfully
		assert_ok!(Beefy::report_fork_equivocation_unsigned(
			RuntimeOrigin::none(),
			Box::new(equivocation_proof),
			key_owner_proofs,
		),);

		era += 1;
		start_era(era);

		// check that the balance of equivocating validators is slashed 100%.
		let equivocation_validator_ids = equivocation_authority_indices
			.iter()
			.map(|i| validators[*i])
			.collect::<Vec<_>>();

		for equivocation_validator_id in &equivocation_validator_ids {
			assert_eq!(Balances::total_balance(&equivocation_validator_id), 10_000_000 - 10_000);
			assert_eq!(Staking::slashable_balance_of(&equivocation_validator_id), 0);
			assert_eq!(
				Staking::eras_stakers(era, equivocation_validator_id),
				pallet_staking::Exposure { total: 0, own: 0, others: vec![] },
			);
		}

		// check that the balances of all other validators are left intact.
		for validator in &validators {
			if equivocation_validator_ids.contains(&validator) {
				continue
			}

			assert_eq!(Balances::total_balance(validator), 10_000_000);
			assert_eq!(Staking::slashable_balance_of(validator), 10_000);

			assert_eq!(
				Staking::eras_stakers(era, validator),
				pallet_staking::Exposure { total: 10_000, own: 10_000, others: vec![] },
			);
		}
	});
}

#[test]
fn report_fork_equivocation_sc_old_set_works() {
	let authorities = test_authorities();

	let mut ext = new_test_ext_raw_authorities(authorities);
	let (offchain, _offchain_state) = TestOffchainExt::with_offchain_db(ext.offchain_db());
	ext.register_extension(OffchainDbExt::new(offchain.clone()));
	ext.register_extension(OffchainWorkerExt::new(offchain));

	let mut era = 1;
	let (
		block_num,
		validators,
		old_set_id,
		equivocation_authority_indices,
		equivocation_keys,
		key_owner_proofs,
	) = ext.execute_with(|| {
		start_era(era);
		let block_num = System::block_number();

		era += 1;
		start_era(era);

		let validator_set = Beefy::validator_set().unwrap();
		let authorities = validator_set.validators();
		let validators = Session::validators();
		let old_set_id = validator_set.id();

		assert_eq!(authorities.len(), 3);
		let equivocation_authority_indices = [0, 2];
		let equivocation_keys = equivocation_authority_indices
			.iter()
			.map(|i| authorities[*i].clone())
			.collect::<Vec<_>>();

		// create the key ownership proofs in the "old" set
		let key_owner_proofs = equivocation_keys
			.iter()
			.map(|k| Historical::prove((BEEFY_KEY_TYPE, &k)).unwrap())
			.collect();

		era += 1;
		start_era(era);

		(
			block_num,
			validators,
			old_set_id,
			equivocation_authority_indices,
			equivocation_keys,
			key_owner_proofs,
		)
	});
	ext.persist_offchain_overlay();

	ext.execute_with(|| {
		// make sure that all authorities have the same balance
		for validator in &validators {
			assert_eq!(Balances::total_balance(validator), 10_000_000);
			assert_eq!(Staking::slashable_balance_of(validator), 10_000);

			assert_eq!(
				Staking::eras_stakers(2, validator),
				pallet_staking::Exposure { total: 10_000, own: 10_000, others: vec![] },
			);
		}

		let validator_set = Beefy::validator_set().unwrap();
		let new_set_id = validator_set.id();
		assert_eq!(old_set_id + 3, new_set_id);

		let equivocation_keyrings = equivocation_keys
			.iter()
			.map(|k| BeefyKeyring::from_public(k).unwrap())
			.collect();

		let payload = Payload::from_single_entry(MMR_ROOT_ID, vec![42]);
		let ancestry_proof = Mmr::generate_ancestry_proof(block_num, None).unwrap();
		// generate an fork equivocation proof, with a vote in the same round for a
		// different payload than finalized
		let commitment =
			Commitment { validator_set_id: old_set_id, block_number: block_num, payload };
		// generate an fork equivocation proof, with a vote in the same round for a
		// different payload than finalized
		let equivocation_proof = generate_fork_equivocation_proof_sc(
			commitment,
			equivocation_keyrings,
			None,
			Some(ancestry_proof),
		);

		// report the equivocation and the tx should be dispatched successfully
		assert_ok!(Beefy::report_fork_equivocation_unsigned(
			RuntimeOrigin::none(),
			Box::new(equivocation_proof),
			key_owner_proofs,
		),);

		era += 1;
		start_era(era);

		// check that the balance of equivocating validators is slashed 100%.
		let equivocation_validator_ids = equivocation_authority_indices
			.iter()
			.map(|i| validators[*i])
			.collect::<Vec<_>>();

		for equivocation_validator_id in &equivocation_validator_ids {
			assert_eq!(Balances::total_balance(&equivocation_validator_id), 10_000_000 - 10_000);
			assert_eq!(Staking::slashable_balance_of(&equivocation_validator_id), 0);
			assert_eq!(
				Staking::eras_stakers(era, equivocation_validator_id),
				pallet_staking::Exposure { total: 0, own: 0, others: vec![] },
			);
		}

		// check that the balances of all other validators are left intact.
		for validator in &validators {
			if equivocation_validator_ids.contains(&validator) {
				continue
			}

			assert_eq!(Balances::total_balance(validator), 10_000_000);
			assert_eq!(Staking::slashable_balance_of(validator), 10_000);

			assert_eq!(
				Staking::eras_stakers(3, validator),
				pallet_staking::Exposure { total: 10_000, own: 10_000, others: vec![] },
			);
		}
	});
}

#[test]
fn report_fork_equivocation_sc_future_block_works() {
	let authorities = test_authorities();

	let mut ext = new_test_ext_raw_authorities(authorities);
	let (offchain, _offchain_state) = TestOffchainExt::with_offchain_db(ext.offchain_db());
	ext.register_extension(OffchainDbExt::new(offchain.clone()));
	ext.register_extension(OffchainWorkerExt::new(offchain));

	let mut era = 2;
	ext.execute_with(|| {
		start_era(era);
	});
	ext.persist_offchain_overlay();

	ext.execute_with(|| {
		let validator_set = Beefy::validator_set().unwrap();
		let authorities = validator_set.validators();
		let set_id = validator_set.id();
		let validators = Session::validators();

		// make sure that all validators have the same balance
		for validator in &validators {
			assert_eq!(Balances::total_balance(validator), 10_000_000);
			assert_eq!(Staking::slashable_balance_of(validator), 10_000);

			assert_eq!(
				Staking::eras_stakers(era, validator),
				pallet_staking::Exposure { total: 10_000, own: 10_000, others: vec![] },
			);
		}

		assert_eq!(authorities.len(), 3);
		let equivocation_authority_indices = [0, 2];
		let equivocation_keys = equivocation_authority_indices
			.iter()
			.map(|i| &authorities[*i])
			.collect::<Vec<_>>();
		let equivocation_keyrings = equivocation_keys
			.iter()
			.map(|k| BeefyKeyring::from_public(k.clone()).unwrap())
			.collect();

		let payload = Payload::from_single_entry(MMR_ROOT_ID, vec![42]);
		let block_num = System::block_number() + 20;
		// create commitment to a future block
		let commitment = Commitment { validator_set_id: set_id, block_number: block_num, payload };
		// generate an fork equivocation proof, with a vote in the same round but for a
		// future block
		let equivocation_proof =
			generate_fork_equivocation_proof_sc(commitment, equivocation_keyrings, None, None);

		// create the key ownership proof
		let key_owner_proofs = equivocation_keys
			.iter()
			.map(|k| Historical::prove((BEEFY_KEY_TYPE, &k)).unwrap())
			.collect();

		// report the equivocation and the tx should be dispatched successfully
		assert_ok!(Beefy::report_fork_equivocation_unsigned(
			RuntimeOrigin::none(),
			Box::new(equivocation_proof),
			key_owner_proofs,
		),);

		era += 1;
		start_era(era);

		// check that the balance of equivocating validators is slashed 100%.
		let equivocation_validator_ids = equivocation_authority_indices
			.iter()
			.map(|i| validators[*i])
			.collect::<Vec<_>>();

		for equivocation_validator_id in &equivocation_validator_ids {
			assert_eq!(Balances::total_balance(&equivocation_validator_id), 10_000_000 - 10_000);
			assert_eq!(Staking::slashable_balance_of(&equivocation_validator_id), 0);
			assert_eq!(
				Staking::eras_stakers(era, equivocation_validator_id),
				pallet_staking::Exposure { total: 0, own: 0, others: vec![] },
			);
		}

		// check that the balances of all other validators are left intact.
		for validator in &validators {
			if equivocation_validator_ids.contains(&validator) {
				continue
			}

			assert_eq!(Balances::total_balance(validator), 10_000_000);
			assert_eq!(Staking::slashable_balance_of(validator), 10_000);

			assert_eq!(
				Staking::eras_stakers(era, validator),
				pallet_staking::Exposure { total: 10_000, own: 10_000, others: vec![] },
			);
		}
	});
}

#[test]
fn report_fork_equivocation_sc_invalid_set_id() {
	let authorities = test_authorities();

	let mut ext = new_test_ext_raw_authorities(authorities);
	let (offchain, _offchain_state) = TestOffchainExt::with_offchain_db(ext.offchain_db());
	ext.register_extension(OffchainDbExt::new(offchain.clone()));
	ext.register_extension(OffchainWorkerExt::new(offchain));

	let mut era = 1;
	let block_num = ext.execute_with(|| {
		start_era(era);
		let block_num = System::block_number();

		era += 1;
		start_era(era);
		block_num
	});
	ext.persist_offchain_overlay();

	ext.execute_with(|| {
		let validator_set = Beefy::validator_set().unwrap();
		let authorities = validator_set.validators();
		let set_id = validator_set.id();

		let equivocation_authority_indices = [0, 2];
		let equivocation_keys = equivocation_authority_indices
			.iter()
			.map(|i| &authorities[*i])
			.collect::<Vec<_>>();
		let equivocation_keyrings = equivocation_keys
			.iter()
			.map(|k| BeefyKeyring::from_public(k.clone()).unwrap())
			.collect();

		let key_owner_proofs = equivocation_keys
			.iter()
			.map(|k| Historical::prove((BEEFY_KEY_TYPE, &k)).unwrap())
			.collect();

		let payload = Payload::from_single_entry(MMR_ROOT_ID, vec![42]);
		let ancestry_proof = Mmr::generate_ancestry_proof(block_num, None).unwrap();
		// generate an equivocation for a future set
		let commitment =
			Commitment { validator_set_id: set_id + 1, block_number: block_num, payload };
		let equivocation_proof = generate_fork_equivocation_proof_sc(
			commitment,
			equivocation_keyrings,
			None,
			Some(ancestry_proof),
		);

		// the call for reporting the equivocation should error
		assert_err!(
			Beefy::report_fork_equivocation_unsigned(
				RuntimeOrigin::none(),
				Box::new(equivocation_proof),
				key_owner_proofs,
			),
			Error::<Test>::InvalidEquivocationProofSession,
		);
	});
}

#[test]
fn report_fork_equivocation_sc_invalid_session() {
	let authorities = test_authorities();

	let mut ext = new_test_ext_raw_authorities(authorities);
	let (offchain, _offchain_state) = TestOffchainExt::with_offchain_db(ext.offchain_db());
	ext.register_extension(OffchainDbExt::new(offchain.clone()));
	ext.register_extension(OffchainWorkerExt::new(offchain));

	let mut era = 1;
	let (block_num, equivocation_keyrings, key_owner_proofs) = ext.execute_with(|| {
		start_era(era);
		let block_num = System::block_number();

		era += 1;
		start_era(era);

		let validator_set = Beefy::validator_set().unwrap();
		let authorities = validator_set.validators();

		let equivocation_authority_indices = [0, 2];
		let equivocation_keys = equivocation_authority_indices
			.iter()
			.map(|i| authorities[*i].clone())
			.collect::<Vec<_>>();
		let equivocation_keyrings = equivocation_keys
			.iter()
			.map(|k| BeefyKeyring::from_public(k).unwrap())
			.collect();

		// generate key ownership proofs at current era set id
		let key_owner_proofs = equivocation_keys
			.iter()
			.map(|k| Historical::prove((BEEFY_KEY_TYPE, &k)).unwrap())
			.collect();

		era += 1;
		start_era(era);
		(block_num, equivocation_keyrings, key_owner_proofs)
	});
	ext.persist_offchain_overlay();

	ext.execute_with(|| {
		let set_id = Beefy::validator_set().unwrap().id();

		let payload = Payload::from_single_entry(MMR_ROOT_ID, vec![42]);
		let ancestry_proof = Mmr::generate_ancestry_proof(block_num, None).unwrap();
		// generate an equivocation proof at following era set id = 3
		let commitment = Commitment { validator_set_id: set_id, block_number: block_num, payload };
		let equivocation_proof = generate_fork_equivocation_proof_sc(
			commitment,
			equivocation_keyrings,
			None,
			Some(ancestry_proof),
		);

		// report an equivocation for the current set using an key ownership
		// proof from the previous set, the session should be invalid.
		assert_err!(
			Beefy::report_fork_equivocation_unsigned(
				RuntimeOrigin::none(),
				Box::new(equivocation_proof),
				key_owner_proofs,
			),
			Error::<Test>::InvalidEquivocationProofSession,
		);
	});
}

#[test]
fn report_fork_equivocation_sc_invalid_key_owner_proof() {
	let authorities = test_authorities();

	let mut ext = new_test_ext_raw_authorities(authorities);
	let (offchain, _offchain_state) = TestOffchainExt::with_offchain_db(ext.offchain_db());
	ext.register_extension(OffchainDbExt::new(offchain.clone()));
	ext.register_extension(OffchainWorkerExt::new(offchain));

	let mut era = 1;
	let block_num = ext.execute_with(|| {
		start_era(era);
		let block_num = System::block_number();

		era += 1;
		start_era(era);
		block_num
	});
	ext.persist_offchain_overlay();

	ext.execute_with(|| {
		let validator_set = Beefy::validator_set().unwrap();
		let authorities = validator_set.validators();
		let set_id = validator_set.id();

		let invalid_owner_authority_index = 1;
		let invalid_owner_key = &authorities[invalid_owner_authority_index];
		let valid_owner_authority_index = 0;
		let valid_owner_key = &authorities[valid_owner_authority_index];

		// generate a key ownership proof for the authority at index 1
		let invalid_key_owner_proof =
			Historical::prove((BEEFY_KEY_TYPE, &invalid_owner_key)).unwrap();
		// generate a key ownership proof for the authority at index 1
		let valid_key_owner_proof = Historical::prove((BEEFY_KEY_TYPE, &valid_owner_key)).unwrap();

		let equivocation_authority_indices = [0, 2];
		let equivocation_keys = equivocation_authority_indices
			.iter()
			.map(|i| &authorities[*i])
			.collect::<Vec<_>>();
		let equivocation_keyrings = equivocation_keys
			.iter()
			.map(|k| BeefyKeyring::from_public(k.clone()).unwrap())
			.collect();

		let payload = Payload::from_single_entry(MMR_ROOT_ID, vec![42]);
		let ancestry_proof = Mmr::generate_ancestry_proof(block_num, None).unwrap();
		// generate an equivocation proof for the authorities at indices [0, 2]
		let commitment = Commitment { validator_set_id: set_id, block_number: block_num, payload };
		let equivocation_proof = generate_fork_equivocation_proof_sc(
			commitment,
			equivocation_keyrings,
			None,
			Some(ancestry_proof),
		);

		// we need to start a new era otherwise the key ownership proof won't be
		// checked since the authorities are part of the current session
		era += 1;
		start_era(era);

		// report an equivocation for the current set using a key ownership
		// proof for a different key than the ones in the equivocation proof.
		assert_err!(
			Beefy::report_fork_equivocation_unsigned(
				RuntimeOrigin::none(),
				Box::new(equivocation_proof),
				vec![valid_key_owner_proof, invalid_key_owner_proof],
			),
			Error::<Test>::InvalidKeyOwnershipProof,
		);
	});
}

#[test]
fn report_fork_equivocation_sc_invalid_equivocation_proof() {
	let authorities = test_authorities();

	let mut ext = new_test_ext_raw_authorities(authorities);
	let (offchain, _offchain_state) = TestOffchainExt::with_offchain_db(ext.offchain_db());
	ext.register_extension(OffchainDbExt::new(offchain.clone()));
	ext.register_extension(OffchainWorkerExt::new(offchain));

	let mut era = 1;
	let block_num = ext.execute_with(|| {
		start_era(era);
		let block_num = System::block_number();

		era += 1;
		start_era(era);
		block_num
	});
	ext.persist_offchain_overlay();

	ext.execute_with(|| {
		let validator_set = Beefy::validator_set().unwrap();
		let authorities = validator_set.validators();
		let set_id = validator_set.id();

		let equivocation_authority_indices = [0, 2];
		let equivocation_keys = equivocation_authority_indices
			.iter()
			.map(|i| &authorities[*i])
			.collect::<Vec<_>>();
		let equivocation_keyrings: Vec<_> = equivocation_keys
			.iter()
			.map(|k| BeefyKeyring::from_public(k.clone()).unwrap())
			.collect();

		// generate a key ownership proof at set id in era 1
		let key_owner_proofs: Vec<_> = equivocation_keys
			.iter()
			.map(|k| Historical::prove((BEEFY_KEY_TYPE, &k)).unwrap())
			.collect();

		start_era(2);

		let payload = Payload::from_single_entry(MMR_ROOT_ID, vec![42]);
		let ancestry_proof = Mmr::generate_ancestry_proof(block_num, None).unwrap();

		// commitment targets different round than finalized payload, there is no equivocation.
		let equivocation_proof = generate_fork_equivocation_proof_sc(
			Commitment {
				validator_set_id: set_id,
				block_number: block_num + 1,
				payload: payload.clone(),
			},
			equivocation_keyrings.clone(),
			None,
			Some(ancestry_proof.clone()),
		);
		assert_err!(
			Beefy::report_fork_equivocation_unsigned(
				RuntimeOrigin::none(),
				Box::new(equivocation_proof),
				key_owner_proofs.clone(),
			),
			Error::<Test>::InvalidForkEquivocationProof,
		);

		// commitment signed with a key that isn't part of the authority set
		let equivocation_proof = generate_fork_equivocation_proof_sc(
			Commitment {
				validator_set_id: set_id,
				block_number: block_num,
				payload: payload.clone(),
			},
			vec![BeefyKeyring::Eve],
			None,
			Some(ancestry_proof.clone()),
		);
		assert_err!(
			Beefy::report_fork_equivocation_unsigned(
				RuntimeOrigin::none(),
				Box::new(equivocation_proof),
				key_owner_proofs.clone(),
			),
			Error::<Test>::InvalidKeyOwnershipProof,
		);

		// commitment targets future set id
		let equivocation_proof = generate_fork_equivocation_proof_sc(
			Commitment {
				validator_set_id: set_id + 1,
				block_number: block_num,
				payload: payload.clone(),
			},
			equivocation_keyrings,
			None,
			Some(ancestry_proof),
		);
		assert_err!(
			Beefy::report_fork_equivocation_unsigned(
				RuntimeOrigin::none(),
				Box::new(equivocation_proof),
				key_owner_proofs,
			),
			Error::<Test>::InvalidEquivocationProofSession,
		);
	});
}

#[test]
fn report_fork_equivocation_sc_validate_unsigned_prevents_duplicates() {
	use sp_runtime::transaction_validity::{
		InvalidTransaction, TransactionPriority, TransactionSource, TransactionValidity,
		ValidTransaction,
	};

	let authorities = test_authorities();

	let mut ext = new_test_ext_raw_authorities(authorities);
	let (offchain, _offchain_state) = TestOffchainExt::with_offchain_db(ext.offchain_db());
	ext.register_extension(OffchainDbExt::new(offchain.clone()));
	ext.register_extension(OffchainWorkerExt::new(offchain));

	let mut era = 1;
	let block_num = ext.execute_with(|| {
		start_era(era);
		let block_num = System::block_number();

		era += 1;
		start_era(era);
		block_num
	});
	ext.persist_offchain_overlay();

	ext.execute_with(|| {
		let validator_set = Beefy::validator_set().unwrap();
		let authorities = validator_set.validators();
		let set_id = validator_set.id();

		// generate and report an equivocation for the validator at index 0
		let equivocation_authority_index = 0;
		let equivocation_key = &authorities[equivocation_authority_index];
		let equivocation_keyring = BeefyKeyring::from_public(equivocation_key).unwrap();

		let payload = Payload::from_single_entry(MMR_ROOT_ID, vec![42]);
		let ancestry_proof = Mmr::generate_ancestry_proof(block_num, None).unwrap();
		let equivocation_proof = generate_fork_equivocation_proof_vote(
			(block_num, payload, set_id, &equivocation_keyring),
			None,
			Some(ancestry_proof),
		);

		let key_owner_proof = Historical::prove((BEEFY_KEY_TYPE, &equivocation_key)).unwrap();

		let call = Call::report_fork_equivocation_unsigned {
			equivocation_proof: Box::new(equivocation_proof.clone()),
			key_owner_proofs: vec![key_owner_proof.clone()],
		};

		// only local/inblock reports are allowed
		assert_eq!(
			<Beefy as sp_runtime::traits::ValidateUnsigned>::validate_unsigned(
				TransactionSource::External,
				&call,
			),
			InvalidTransaction::Call.into(),
		);

		// the transaction is valid when passed as local
		let tx_tag = (vec![equivocation_key], set_id, 3u64);

		let call_result = <Beefy as sp_runtime::traits::ValidateUnsigned>::validate_unsigned(
			TransactionSource::Local,
			&call,
		);

		assert_eq!(
			call_result,
			TransactionValidity::Ok(ValidTransaction {
				priority: TransactionPriority::max_value(),
				requires: vec![],
				provides: vec![("BeefyEquivocation", tx_tag.clone()).encode()],
				longevity: ReportLongevity::get(),
				propagate: false,
			})
		);

		assert_ok!(<Beefy as sp_runtime::traits::ValidateUnsigned>::pre_dispatch(&call));

		// we submit the report
		Beefy::report_fork_equivocation_unsigned(
			RuntimeOrigin::none(),
			Box::new(equivocation_proof),
			vec![key_owner_proof],
		)
		.unwrap();

		// the report should now be considered stale and the transaction is invalid
		// the check for staleness should be done on both `validate_unsigned` and on `pre_dispatch`
		assert_err!(
			<Beefy as sp_runtime::traits::ValidateUnsigned>::validate_unsigned(
				TransactionSource::Local,
				&call,
			),
			InvalidTransaction::Stale,
		);

		assert_err!(
			<Beefy as sp_runtime::traits::ValidateUnsigned>::pre_dispatch(&call),
			InvalidTransaction::Stale,
		);
	});
}

#[test]
fn valid_fork_equivocation_sc_reports_dont_pay_fees() {
	let authorities = test_authorities();

	let mut ext = new_test_ext_raw_authorities(authorities);
	let (offchain, _offchain_state) = TestOffchainExt::with_offchain_db(ext.offchain_db());
	ext.register_extension(OffchainDbExt::new(offchain.clone()));
	ext.register_extension(OffchainWorkerExt::new(offchain));

	let mut era = 1;
	let block_num = ext.execute_with(|| {
		start_era(era);
		let block_num = System::block_number();

		era += 1;
		start_era(era);
		block_num
	});
	ext.persist_offchain_overlay();

	ext.execute_with(|| {
		let validator_set = Beefy::validator_set().unwrap();
		let authorities = validator_set.validators();
		let set_id = validator_set.id();

		let equivocation_authority_index = 0;
		let equivocation_key = &authorities[equivocation_authority_index];
		let equivocation_keyring = BeefyKeyring::from_public(equivocation_key).unwrap();

		// generate equivocation proof
		let payload = Payload::from_single_entry(MMR_ROOT_ID, vec![42]);
		let ancestry_proof = Mmr::generate_ancestry_proof(block_num, None).unwrap();
		let equivocation_proof = generate_fork_equivocation_proof_vote(
			(block_num, payload, set_id, &equivocation_keyring),
			None,
			Some(ancestry_proof),
		);

		// create the key ownership proof.
		let key_owner_proof = Historical::prove((BEEFY_KEY_TYPE, &equivocation_key)).unwrap();

		// check the dispatch info for the call.
		let info = Call::<Test>::report_fork_equivocation_unsigned {
			equivocation_proof: Box::new(equivocation_proof.clone()),
			key_owner_proofs: vec![key_owner_proof.clone()],
		}
		.get_dispatch_info();

		// it should have non-zero weight and the fee has to be paid.
		assert!(info.weight.any_gt(Weight::zero()));
		assert_eq!(info.pays_fee, Pays::Yes);

		// report the equivocation.
		let post_info = Beefy::report_fork_equivocation_unsigned(
			RuntimeOrigin::none(),
			Box::new(equivocation_proof.clone()),
			vec![key_owner_proof.clone()],
		)
		.unwrap();

		// the original weight should be kept, but given that the report
		// is valid the fee is waived.
		assert!(post_info.actual_weight.is_none());
		assert_eq!(post_info.pays_fee, Pays::No);

		// report the equivocation again which is invalid now since it is
		// duplicate.
		let post_info = Beefy::report_fork_equivocation_unsigned(
			RuntimeOrigin::none(),
			Box::new(equivocation_proof.clone()),
			vec![key_owner_proof.clone()],
		)
		.err()
		.unwrap()
		.post_info;

		// the fee is not waived and the original weight is kept.
		assert!(post_info.actual_weight.is_none());
		assert_eq!(post_info.pays_fee, Pays::Yes);
	})
}

#[test]
fn report_fork_equivocation_sc_stacked_reports_stack_correctly() {
	let authorities = test_authorities();

	let mut ext = new_test_ext_raw_authorities(authorities);
	let (offchain, _offchain_state) = TestOffchainExt::with_offchain_db(ext.offchain_db());
	ext.register_extension(OffchainDbExt::new(offchain.clone()));
	ext.register_extension(OffchainWorkerExt::new(offchain));

	let mut era = 1;
	let block_num = ext.execute_with(|| {
		assert_eq!(Staking::current_era(), Some(0));
		assert_eq!(Session::current_index(), 0);
		start_era(era);
		let block_num = System::block_number();

		era += 1;
		start_era(era);
		block_num
	});
	ext.persist_offchain_overlay();

	let (
		commitment,
		validators,
		equivocation_keyrings,
		equivocation_authority_indices,
		key_owner_proofs,
	) = ext.execute_with(|| {
		let validator_set = Beefy::validator_set().unwrap();
		let authorities = validator_set.validators();
		let set_id = validator_set.id();
		let validators = Session::validators();

		// make sure that all validators have the same balance
		for validator in &validators {
			assert_eq!(Balances::total_balance(validator), 10_000_000);
			assert_eq!(Staking::slashable_balance_of(validator), 10_000);

			assert_eq!(
				Staking::eras_stakers(era, validator),
				pallet_staking::Exposure { total: 10_000, own: 10_000, others: vec![] },
			);
		}

		assert_eq!(authorities.len(), 3);
		let equivocation_authority_indices = [0, 2];
		let equivocation_keys = equivocation_authority_indices
			.iter()
			.map(|i| &authorities[*i])
			.collect::<Vec<_>>();
		let equivocation_keyrings: Vec<_> = equivocation_keys
			.iter()
			.map(|k| BeefyKeyring::from_public(k.clone()).unwrap())
			.collect();

		let payload = Payload::from_single_entry(MMR_ROOT_ID, vec![42]);
		let ancestry_proof = Mmr::generate_ancestry_proof(block_num, None).unwrap();
		let commitment = Commitment { validator_set_id: set_id, block_number: block_num, payload };
		// generate two fork equivocation proofs with a signed commitment in the same round for a
		// different payload than finalized
		// 1. the first equivocation proof is only for Alice
		// 2. the second equivocation proof is for all equivocators
		let equivocation_proof_singleton = generate_fork_equivocation_proof_sc(
			commitment.clone(),
			vec![equivocation_keyrings[0].clone()],
			None,
			Some(ancestry_proof.clone()),
		);

		// create the key ownership proof
		let key_owner_proofs: Vec<_> = equivocation_keys
			.iter()
			.map(|k| Historical::prove((BEEFY_KEY_TYPE, &k)).unwrap())
			.collect();

		// only report a single equivocator and the tx should be dispatched successfully
		assert_ok!(Beefy::report_fork_equivocation_unsigned(
			RuntimeOrigin::none(),
			Box::new(equivocation_proof_singleton),
			vec![key_owner_proofs[0].clone()],
		),);
		era += 1;
		start_era(era);
		(
			commitment,
			validators,
			equivocation_keyrings,
			equivocation_authority_indices,
			key_owner_proofs,
		)
	});
	ext.persist_offchain_overlay();

	ext.execute_with(|| {
		let ancestry_proof = Mmr::generate_ancestry_proof(block_num, None).unwrap();
		let equivocation_proof_full = generate_fork_equivocation_proof_sc(
			commitment,
			equivocation_keyrings,
			None,
			Some(ancestry_proof),
		);

		// check that the balance of the reported equivocating validator is slashed 100%.
		let equivocation_validator_ids = equivocation_authority_indices
			.iter()
			.map(|i| validators[*i])
			.collect::<Vec<_>>();

		assert_eq!(Balances::total_balance(&equivocation_validator_ids[0]), 10_000_000 - 10_000);
		assert_eq!(Staking::slashable_balance_of(&equivocation_validator_ids[0]), 0);
		assert_eq!(
			Staking::eras_stakers(era, &equivocation_validator_ids[0]),
			pallet_staking::Exposure { total: 0, own: 0, others: vec![] },
		);

		// check that the balances of all other validators are left intact.
		for validator in &validators {
			if equivocation_validator_ids[0] == *validator {
				continue
			}

			assert_eq!(Balances::total_balance(validator), 10_000_000);
			assert_eq!(Staking::slashable_balance_of(validator), 10_000);

			assert_eq!(
				Staking::eras_stakers(era, validator),
				pallet_staking::Exposure { total: 10_000, own: 10_000, others: vec![] },
			);
		}

		// report the full equivocation and the tx should be dispatched successfully
		assert_ok!(Beefy::report_fork_equivocation_unsigned(
			RuntimeOrigin::none(),
			Box::new(equivocation_proof_full),
			key_owner_proofs,
		),);

		era += 1;
		start_era(era);

		let equivocation_validator_ids = equivocation_authority_indices
			.iter()
			.map(|i| validators[*i])
			.collect::<Vec<_>>();

		// check that the balance of equivocating validators is slashed 100%, and the validator
		// already reported isn't slashed again
		for equivocation_validator_id in &equivocation_validator_ids {
			assert_eq!(Balances::total_balance(&equivocation_validator_id), 10_000_000 - 10_000);
			assert_eq!(Staking::slashable_balance_of(&equivocation_validator_id), 0);
			assert_eq!(
				Staking::eras_stakers(era, equivocation_validator_id),
				pallet_staking::Exposure { total: 0, own: 0, others: vec![] },
			);
		}

		// check that the balances of all other validators are left intact.
		for validator in &validators {
			if equivocation_validator_ids.contains(&validator) {
				continue
			}

			assert_eq!(Balances::total_balance(validator), 10_000_000);
			assert_eq!(Staking::slashable_balance_of(validator), 10_000);

			assert_eq!(
				Staking::eras_stakers(era, validator),
				pallet_staking::Exposure { total: 10_000, own: 10_000, others: vec![] },
			);
		}
	});
}

#[test]
fn set_new_genesis_works() {
	let authorities = test_authorities();

	new_test_ext_raw_authorities(authorities).execute_with(|| {
		start_era(1);

		let new_genesis_delay = 10u64;
		// the call for setting new genesis should work
		assert_ok!(Beefy::set_new_genesis(RuntimeOrigin::root(), new_genesis_delay,));
		let expected = System::block_number() + new_genesis_delay;
		// verify new genesis was set
		assert_eq!(Beefy::genesis_block(), Some(expected));

		// setting delay < 1 should fail
		assert_err!(
			Beefy::set_new_genesis(RuntimeOrigin::root(), 0u64,),
			Error::<Test>::InvalidConfiguration,
		);
	});
}
