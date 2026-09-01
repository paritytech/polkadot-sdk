// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: 2023 Snowfork <hello@snowfork.com>
pub use crate::mock::*;
use crate::{
	config::{EPOCHS_PER_SYNC_COMMITTEE_PERIOD, SLOTS_PER_EPOCH, SLOTS_PER_HISTORICAL_ROOT},
	functions::{compute_epoch, compute_period},
	mock::{
		get_message_verification_payload, load_checkpoint_update_fixture,
		load_finalized_header_update_fixture, load_next_finalized_header_update_fixture,
		load_next_sync_committee_update_fixture, load_sync_committee_update_fixture,
	},
	sync_committee_sum, verify_merkle_branch, BeaconHeader, CompactBeaconState, Error,
	FinalizedBeaconState, LatestFinalizedBlockRoot, LatestSyncCommitteeUpdatePeriod,
	NextSyncCommittee, SyncCommitteePrepared,
};
use frame_support::{assert_err, assert_noop, assert_ok, pallet_prelude::Pays};
use hex_literal::hex;
use snowbridge_beacon_primitives::{
	merkle_proof::{generalized_index_length, subtree_index},
	types::deneb,
	Fork, ForkVersions, NextSyncCommitteeUpdate, VersionedExecutionPayloadHeader,
};
use snowbridge_verification_primitives::{VerificationError, Verifier};
use sp_core::H256;
use sp_runtime::DispatchError;

/// Arbitrary hash used for tests and invalid hashes.
const TEST_HASH: [u8; 32] =
	hex!["5f6f02af29218292d21a69b64a794a7c0873b3e0f54611972863706e8cbdf371"];

// UNIT TESTS

#[test]
pub fn sum_sync_committee_participation() {
	new_tester().execute_with(|| {
		assert_eq!(sync_committee_sum(&[0, 1, 0, 1, 1, 0, 1, 0, 1]), 5);
	});
}

#[test]
pub fn compute_domain() {
	new_tester().execute_with(|| {
		let domain = EthereumBeaconClient::compute_domain(
			hex!("07000000").into(),
			hex!("00000001"),
			hex!("5dec7ae03261fde20d5b024dfabce8bac3276c9a4908e23d50ba8c9b50b0adff").into(),
		);

		assert_ok!(&domain);
		assert_eq!(
			domain.unwrap(),
			hex!("0700000046324489ceb6ada6d118eacdbe94f49b1fcb49d5481a685979670c7c").into()
		);
	});
}

#[test]
pub fn compute_signing_root_bls() {
	new_tester().execute_with(|| {
		let signing_root = EthereumBeaconClient::compute_signing_root(
			&BeaconHeader {
				slot: 3529537,
				proposer_index: 192549,
				parent_root: hex!(
					"1f8dc05ea427f78e84e2e2666e13c3befb7106fd1d40ef8a3f67cf615f3f2a4c"
				)
				.into(),
				state_root: hex!(
					"0dfb492a83da711996d2d76b64604f9bca9dc08b6c13cf63b3be91742afe724b"
				)
				.into(),
				body_root: hex!("66fba38f7c8c2526f7ddfe09c1a54dd12ff93bdd4d0df6a0950e88e802228bfa")
					.into(),
			},
			hex!("07000000afcaaba0efab1ca832a15152469bb09bb84641c405171dfa2d3fb45f").into(),
		);

		assert_ok!(&signing_root);
		assert_eq!(
			signing_root.unwrap(),
			hex!("3ff6e9807da70b2f65cdd58ea1b25ed441a1d589025d2c4091182026d7af08fb").into()
		);
	});
}

#[test]
pub fn compute_signing_root() {
	new_tester().execute_with(|| {
		let signing_root = EthereumBeaconClient::compute_signing_root(
			&BeaconHeader {
				slot: 222472,
				proposer_index: 10726,
				parent_root: hex!(
					"5d481a9721f0ecce9610eab51d400d223683d599b7fcebca7e4c4d10cdef6ebb"
				)
				.into(),
				state_root: hex!(
					"14eb4575895f996a84528b789ff2e4d5148242e2983f03068353b2c37015507a"
				)
				.into(),
				body_root: hex!("7bb669c75b12e0781d6fa85d7fc2f32d64eafba89f39678815b084c156e46cac")
					.into(),
			},
			hex!("07000000e7acb21061790987fa1c1e745cccfb358370b33e8af2b2c18938e6c2").into(),
		);

		assert_ok!(&signing_root);
		assert_eq!(
			signing_root.unwrap(),
			hex!("da12b6a6d3516bc891e8a49f82fc1925cec40b9327e06457f695035303f55cd8").into()
		);
	});
}

#[test]
pub fn compute_domain_bls() {
	new_tester().execute_with(|| {
		let domain = EthereumBeaconClient::compute_domain(
			hex!("07000000").into(),
			hex!("01000000"),
			hex!("4b363db94e286120d76eb905340fdd4e54bfe9f06bf33ff6cf5ad27f511bfe95").into(),
		);

		assert_ok!(&domain);
		assert_eq!(
			domain.unwrap(),
			hex!("07000000afcaaba0efab1ca832a15152469bb09bb84641c405171dfa2d3fb45f").into()
		);
	});
}

#[test]
pub fn may_refund_call_fee() {
	let finalized_update = Box::new(load_next_finalized_header_update_fixture());
	let sync_committee_update = Box::new(load_sync_committee_update_fixture());
	new_tester().execute_with(|| {
		let free_headers_interval: u64 = crate::mock::FREE_SLOTS_INTERVAL as u64;
		// Not free, smaller than the allowed free header interval
		assert_eq!(
			EthereumBeaconClient::check_refundable(
				&finalized_update.clone(),
				finalized_update.finalized_header.slot + free_headers_interval
			),
			Pays::Yes
		);
		// Is free, larger than the minimum interval
		assert_eq!(
			EthereumBeaconClient::check_refundable(
				&finalized_update,
				finalized_update.finalized_header.slot - (free_headers_interval + 2)
			),
			Pays::No
		);
		// Is free, valid sync committee update
		assert_eq!(
			EthereumBeaconClient::check_refundable(
				&sync_committee_update,
				finalized_update.finalized_header.slot
			),
			Pays::No
		);
	});
}

#[test]
pub fn verify_merkle_branch_for_finalized_root() {
	new_tester().execute_with(|| {
		assert!(verify_merkle_branch(
			hex!("0000000000000000000000000000000000000000000000000000000000000000").into(),
			&[
				hex!("0000000000000000000000000000000000000000000000000000000000000000").into(),
				hex!("5f6f02af29218292d21a69b64a794a7c0873b3e0f54611972863706e8cbdf371").into(),
				hex!("e7125ff9ab5a840c44bedb4731f440a405b44e15f2d1a89e27341b432fabe13d").into(),
				hex!("002c1fe5bc0bd62db6f299a582f2a80a6d5748ccc82e7ed843eaf0ae0739f74a").into(),
				hex!("d2dc4ba9fd4edff6716984136831e70a6b2e74fca27b8097a820cbbaa5a6e3c3").into(),
				hex!("91f77a19d8afa4a08e81164bb2e570ecd10477b3b65c305566a6d2be88510584").into(),
			],
			subtree_index(crate::config::altair::FINALIZED_ROOT_INDEX),
			generalized_index_length(crate::config::altair::FINALIZED_ROOT_INDEX),
			hex!("e46559327592741956f6beaa0f52e49625eb85dce037a0bd2eff333c743b287f").into()
		));
	});
}

#[test]
pub fn verify_merkle_branch_fails_if_depth_and_branch_dont_match() {
	new_tester().execute_with(|| {
		assert!(!verify_merkle_branch(
			hex!("0000000000000000000000000000000000000000000000000000000000000000").into(),
			&[
				hex!("0000000000000000000000000000000000000000000000000000000000000000").into(),
				hex!("5f6f02af29218292d21a69b64a794a7c0873b3e0f54611972863706e8cbdf371").into(),
				hex!("e7125ff9ab5a840c44bedb4731f440a405b44e15f2d1a89e27341b432fabe13d").into(),
			],
			subtree_index(crate::config::altair::FINALIZED_ROOT_INDEX),
			generalized_index_length(crate::config::altair::FINALIZED_ROOT_INDEX),
			hex!("e46559327592741956f6beaa0f52e49625eb85dce037a0bd2eff333c743b287f").into()
		));
	});
}

#[test]
pub fn sync_committee_participation_is_supermajority() {
	let bits =
		hex!("bffffffff7f1ffdfcfeffeffbfdffffbfffffdffffefefffdffff7f7ffff77fffdf7bff77ffdf7fffafffffff77fefffeff7effffffff5f7fedfffdfb6ddff7b"
	);
	let participation =
		snowbridge_beacon_primitives::decompress_sync_committee_bits::<512, 64>(bits);
	assert_ok!(EthereumBeaconClient::sync_committee_participation_is_supermajority(&participation));
}

#[test]
pub fn sync_committee_participation_is_supermajority_errors_when_not_supermajority() {
	new_tester().execute_with(|| {
		let participation = hex!("0000000000000000000000000000000000000001010100010100000000000000000000000101010101000100010101010101010101010101010101010100010101000000000001010101010100010101000000000000000000000000000101000101010101010001010101010100010101010101010101010101000101010101010100010101010100000000010101010100000000000000000001010101010101010101010101010101010100010101010101010001010101010101010101010101010101000101010101010101010101010100010101010101010101010101010101010101010101010101010101010101010001010100010101010101010101000101010101010101010001010101010101010101000101010100010101010101010101010100010000000000000000000100000000000001010100000001000100010101010100000000000000000000000000000000000000010101010101010100010101010101010101010100010101010001010101010101010101010101010100000000000000000101010101000000000001000000000000000000010000000000000000000101010101010100010001010101010101000101010101010101010101010101010101000101010101010101010101010101010001010101010101010001010001000000000000000000000000000001000000000000");

		assert_err!(
			EthereumBeaconClient::sync_committee_participation_is_supermajority(&participation),
			Error::<Test>::SyncCommitteeParticipantsNotSupermajority
		);
	});
}

#[test]
fn compute_fork_version() {
	let mock_fork_versions = ForkVersions {
		genesis: Fork { version: [0, 0, 0, 0], epoch: 0 },
		altair: Fork { version: [0, 0, 0, 1], epoch: 10 },
		bellatrix: Fork { version: [0, 0, 0, 2], epoch: 20 },
		capella: Fork { version: [0, 0, 0, 3], epoch: 30 },
		deneb: Fork { version: [0, 0, 0, 4], epoch: 40 },
		electra: Fork { version: [0, 0, 0, 5], epoch: 50 },
		fulu: Fork { version: [0, 0, 0, 6], epoch: 60 },
		gloas: Fork { version: [0, 0, 0, 7], epoch: 70 },
	};
	new_tester().execute_with(|| {
		assert_eq!(EthereumBeaconClient::select_fork_version(&mock_fork_versions, 0), [0, 0, 0, 0]);
		assert_eq!(EthereumBeaconClient::select_fork_version(&mock_fork_versions, 1), [0, 0, 0, 0]);
		assert_eq!(
			EthereumBeaconClient::select_fork_version(&mock_fork_versions, 10),
			[0, 0, 0, 1]
		);
		assert_eq!(
			EthereumBeaconClient::select_fork_version(&mock_fork_versions, 21),
			[0, 0, 0, 2]
		);
		assert_eq!(
			EthereumBeaconClient::select_fork_version(&mock_fork_versions, 20),
			[0, 0, 0, 2]
		);
		assert_eq!(
			EthereumBeaconClient::select_fork_version(&mock_fork_versions, 32),
			[0, 0, 0, 3]
		);
		assert_eq!(
			EthereumBeaconClient::select_fork_version(&mock_fork_versions, 40),
			[0, 0, 0, 4]
		);
		assert_eq!(
			EthereumBeaconClient::select_fork_version(&mock_fork_versions, 50),
			[0, 0, 0, 5]
		);
	});
}

#[test]
fn find_absent_keys() {
	let participation: [u8; 32] =
		hex!("0001010101010100010101010101010101010101010101010101010101010101").into();
	let update = load_sync_committee_update_fixture();
	let sync_committee_prepared: SyncCommitteePrepared =
		(&update.next_sync_committee_update.unwrap().next_sync_committee)
			.try_into()
			.unwrap();

	new_tester().execute_with(|| {
		let pubkeys = EthereumBeaconClient::find_pubkeys(
			&participation,
			(*sync_committee_prepared.pubkeys).as_ref(),
			false,
		);
		assert_eq!(pubkeys.len(), 2);
		assert_eq!(pubkeys[0], sync_committee_prepared.pubkeys[0]);
		assert_eq!(pubkeys[1], sync_committee_prepared.pubkeys[7]);
	});
}

#[test]
fn find_present_keys() {
	let participation: [u8; 32] =
		hex!("0001000000000000010000000000000000000000000000000000010000000100").into();
	let update = load_sync_committee_update_fixture();
	let sync_committee_prepared: SyncCommitteePrepared =
		(&update.next_sync_committee_update.unwrap().next_sync_committee)
			.try_into()
			.unwrap();

	new_tester().execute_with(|| {
		let pubkeys = EthereumBeaconClient::find_pubkeys(
			&participation,
			(*sync_committee_prepared.pubkeys).as_ref(),
			true,
		);
		assert_eq!(pubkeys.len(), 4);
		assert_eq!(pubkeys[0], sync_committee_prepared.pubkeys[1]);
		assert_eq!(pubkeys[1], sync_committee_prepared.pubkeys[8]);
		assert_eq!(pubkeys[2], sync_committee_prepared.pubkeys[26]);
		assert_eq!(pubkeys[3], sync_committee_prepared.pubkeys[30]);
	});
}

// SYNC PROCESS TESTS

#[test]
fn process_initial_checkpoint() {
	let checkpoint = Box::new(load_checkpoint_update_fixture());

	new_tester().execute_with(|| {
		assert_ok!(EthereumBeaconClient::force_checkpoint(
			RuntimeOrigin::root(),
			checkpoint.clone()
		));
		let block_root: H256 = checkpoint.header.hash_tree_root().unwrap();
		assert!(<FinalizedBeaconState<Test>>::contains_key(block_root));
	});
}

#[test]
fn process_initial_checkpoint_with_invalid_sync_committee_proof() {
	let mut checkpoint = Box::new(load_checkpoint_update_fixture());
	checkpoint.current_sync_committee_branch[0] = TEST_HASH.into();

	new_tester().execute_with(|| {
		assert_err!(
			EthereumBeaconClient::force_checkpoint(RuntimeOrigin::root(), checkpoint),
			Error::<Test>::InvalidSyncCommitteeMerkleProof
		);
	});
}

#[test]
fn process_initial_checkpoint_with_invalid_blocks_root_proof() {
	let mut checkpoint = Box::new(load_checkpoint_update_fixture());
	checkpoint.block_roots_branch[0] = TEST_HASH.into();

	new_tester().execute_with(|| {
		assert_err!(
			EthereumBeaconClient::force_checkpoint(RuntimeOrigin::root(), checkpoint),
			Error::<Test>::InvalidBlockRootsRootMerkleProof
		);
	});
}

#[test]
fn submit_update_in_current_period() {
	let checkpoint = Box::new(load_checkpoint_update_fixture());
	let update = Box::new(load_finalized_header_update_fixture());
	let initial_period = compute_period(checkpoint.header.slot);
	let update_period = compute_period(update.finalized_header.slot);
	assert_eq!(initial_period, update_period);

	new_tester().execute_with(|| {
		assert_ok!(EthereumBeaconClient::process_checkpoint_update(&checkpoint));
		let result = EthereumBeaconClient::submit(RuntimeOrigin::signed(1), update.clone());
		assert_ok!(result);
		assert_eq!(result.unwrap().pays_fee, Pays::No);
		let block_root: H256 = update.finalized_header.hash_tree_root().unwrap();
		assert!(<FinalizedBeaconState<Test>>::contains_key(block_root));
	});
}

#[test]
fn submit_update_with_sync_committee_in_current_period() {
	let checkpoint = Box::new(load_checkpoint_update_fixture());
	let update = Box::new(load_sync_committee_update_fixture());
	let init_period = compute_period(checkpoint.header.slot);
	let update_period = compute_period(update.finalized_header.slot);
	assert_eq!(init_period, update_period);

	new_tester().execute_with(|| {
		assert_ok!(EthereumBeaconClient::process_checkpoint_update(&checkpoint));
		assert!(!<NextSyncCommittee<Test>>::exists());
		let result = EthereumBeaconClient::submit(RuntimeOrigin::signed(1), update);
		assert_ok!(result);
		assert_eq!(result.unwrap().pays_fee, Pays::No);
		assert!(<NextSyncCommittee<Test>>::exists());
	});
}

#[test]
fn reject_submit_update_in_next_period() {
	let checkpoint = Box::new(load_checkpoint_update_fixture());
	let sync_committee_update = Box::new(load_sync_committee_update_fixture());
	let update = Box::new(load_next_finalized_header_update_fixture());
	let sync_committee_period = compute_period(sync_committee_update.finalized_header.slot);
	let next_sync_committee_period = compute_period(update.finalized_header.slot);
	assert_eq!(sync_committee_period + 1, next_sync_committee_period);
	let next_sync_committee_update = Box::new(load_next_sync_committee_update_fixture());

	new_tester().execute_with(|| {
		assert_ok!(EthereumBeaconClient::process_checkpoint_update(&checkpoint));
		let result =
			EthereumBeaconClient::submit(RuntimeOrigin::signed(1), sync_committee_update.clone());
		assert_ok!(result);
		assert_eq!(result.unwrap().pays_fee, Pays::No);

		// check an update in the next period is rejected
		let second_result = EthereumBeaconClient::submit(RuntimeOrigin::signed(1), update.clone());
		assert_err!(second_result, Error::<Test>::SyncCommitteeUpdateRequired);
		assert_eq!(second_result.unwrap_err().post_info.pays_fee, Pays::Yes);

		// submit update with next sync committee
		let third_result =
			EthereumBeaconClient::submit(RuntimeOrigin::signed(1), next_sync_committee_update);
		assert_ok!(third_result);
		assert_eq!(third_result.unwrap().pays_fee, Pays::No);
		// check same header in the next period can now be submitted successfully
		assert_ok!(EthereumBeaconClient::submit(RuntimeOrigin::signed(1), update.clone()));
		let block_root: H256 = update.finalized_header.clone().hash_tree_root().unwrap();
		assert!(<FinalizedBeaconState<Test>>::contains_key(block_root));
	});
}

#[test]
fn submit_update_with_invalid_header_proof() {
	let checkpoint = Box::new(load_checkpoint_update_fixture());
	let mut update = Box::new(load_sync_committee_update_fixture());
	let init_period = compute_period(checkpoint.header.slot);
	let update_period = compute_period(update.finalized_header.slot);
	assert_eq!(init_period, update_period);
	update.finality_branch[0] = TEST_HASH.into();

	new_tester().execute_with(|| {
		assert_ok!(EthereumBeaconClient::process_checkpoint_update(&checkpoint));
		assert!(!<NextSyncCommittee<Test>>::exists());
		let result = EthereumBeaconClient::submit(RuntimeOrigin::signed(1), update);
		assert_err!(result, Error::<Test>::InvalidHeaderMerkleProof);
		assert_eq!(result.unwrap_err().post_info.pays_fee, Pays::Yes);
	});
}

#[test]
fn submit_update_with_invalid_block_roots_proof() {
	let checkpoint = Box::new(load_checkpoint_update_fixture());
	let mut update = Box::new(load_sync_committee_update_fixture());
	let init_period = compute_period(checkpoint.header.slot);
	let update_period = compute_period(update.finalized_header.slot);
	assert_eq!(init_period, update_period);
	update.block_roots_branch[0] = TEST_HASH.into();

	new_tester().execute_with(|| {
		assert_ok!(EthereumBeaconClient::process_checkpoint_update(&checkpoint));
		assert!(!<NextSyncCommittee<Test>>::exists());
		let result = EthereumBeaconClient::submit(RuntimeOrigin::signed(1), update);
		assert_err!(result, Error::<Test>::InvalidBlockRootsRootMerkleProof);
		assert_eq!(result.unwrap_err().post_info.pays_fee, Pays::Yes);
	});
}

#[test]
fn submit_update_with_invalid_next_sync_committee_proof() {
	let checkpoint = Box::new(load_checkpoint_update_fixture());
	let mut update = Box::new(load_sync_committee_update_fixture());
	let init_period = compute_period(checkpoint.header.slot);
	let update_period = compute_period(update.finalized_header.slot);
	assert_eq!(init_period, update_period);
	if let Some(ref mut next_sync_committee_update) = update.next_sync_committee_update {
		next_sync_committee_update.next_sync_committee_branch[0] = TEST_HASH.into();
	}

	new_tester().execute_with(|| {
		assert_ok!(EthereumBeaconClient::process_checkpoint_update(&checkpoint));
		assert!(!<NextSyncCommittee<Test>>::exists());
		let result = EthereumBeaconClient::submit(RuntimeOrigin::signed(1), update);
		assert_err!(result, Error::<Test>::InvalidSyncCommitteeMerkleProof);
		assert_eq!(result.unwrap_err().post_info.pays_fee, Pays::Yes);
	});
}

#[test]
fn submit_update_with_skipped_period() {
	let checkpoint = Box::new(load_checkpoint_update_fixture());
	let sync_committee_update = Box::new(load_sync_committee_update_fixture());
	let mut update = Box::new(load_next_finalized_header_update_fixture());
	update.signature_slot += (EPOCHS_PER_SYNC_COMMITTEE_PERIOD * SLOTS_PER_EPOCH) as u64;
	update.attested_header.slot = update.signature_slot - 1;

	new_tester().execute_with(|| {
		assert_ok!(EthereumBeaconClient::process_checkpoint_update(&checkpoint));
		let result =
			EthereumBeaconClient::submit(RuntimeOrigin::signed(1), sync_committee_update.clone());
		assert_ok!(result);
		assert_eq!(result.unwrap().pays_fee, Pays::No);

		let second_result = EthereumBeaconClient::submit(RuntimeOrigin::signed(1), update);
		assert_err!(second_result, Error::<Test>::SkippedSyncCommitteePeriod);
		assert_eq!(second_result.unwrap_err().post_info.pays_fee, Pays::Yes);
	});
}

#[test]
fn submit_update_with_sync_committee_in_next_period() {
	let checkpoint = Box::new(load_checkpoint_update_fixture());
	let update = Box::new(load_sync_committee_update_fixture());
	let next_update = Box::new(load_next_sync_committee_update_fixture());
	let update_period = compute_period(update.finalized_header.slot);
	let next_update_period = compute_period(next_update.finalized_header.slot);
	assert_eq!(update_period + 1, next_update_period);

	new_tester().execute_with(|| {
		assert_ok!(EthereumBeaconClient::process_checkpoint_update(&checkpoint));
		assert!(!<NextSyncCommittee<Test>>::exists());

		let result = EthereumBeaconClient::submit(RuntimeOrigin::signed(1), update.clone());
		assert_ok!(result);
		assert_eq!(result.unwrap().pays_fee, Pays::No);
		assert!(<NextSyncCommittee<Test>>::exists());

		let second_result =
			EthereumBeaconClient::submit(RuntimeOrigin::signed(1), next_update.clone());
		assert_ok!(second_result);
		assert_eq!(second_result.unwrap().pays_fee, Pays::No);
		let last_finalized_state =
			FinalizedBeaconState::<Test>::get(LatestFinalizedBlockRoot::<Test>::get()).unwrap();
		let last_synced_period = compute_period(last_finalized_state.slot);
		assert_eq!(last_synced_period, next_update_period);
	});
}

#[test]
fn submit_update_with_sync_committee_invalid_signature_slot() {
	let checkpoint = Box::new(load_checkpoint_update_fixture());
	let mut update = Box::new(load_sync_committee_update_fixture());

	new_tester().execute_with(|| {
		assert_ok!(EthereumBeaconClient::process_checkpoint_update(&checkpoint));

		// makes an invalid update with signature_slot should be more than attested_slot
		update.signature_slot = update.attested_header.slot;

		let result = EthereumBeaconClient::submit(RuntimeOrigin::signed(1), update);
		assert_err!(result, Error::<Test>::InvalidUpdateSlot);
		assert_eq!(result.unwrap_err().post_info.pays_fee, Pays::Yes);
	});
}

#[test]
fn submit_update_with_skipped_sync_committee_period() {
	let checkpoint = Box::new(load_checkpoint_update_fixture());
	let finalized_update = Box::new(load_next_finalized_header_update_fixture());
	let checkpoint_period = compute_period(checkpoint.header.slot);
	let next_sync_committee_period = compute_period(finalized_update.finalized_header.slot);
	assert_eq!(checkpoint_period + 1, next_sync_committee_period);

	new_tester().execute_with(|| {
		assert_ok!(EthereumBeaconClient::process_checkpoint_update(&checkpoint));
		let result = EthereumBeaconClient::submit(RuntimeOrigin::signed(1), finalized_update);
		assert_err!(result, Error::<Test>::SkippedSyncCommitteePeriod);
		assert_eq!(result.unwrap_err().post_info.pays_fee, Pays::Yes);
	});
}

#[test]
fn submit_irrelevant_update() {
	let checkpoint = Box::new(load_checkpoint_update_fixture());
	let mut update = Box::new(load_next_finalized_header_update_fixture());

	new_tester().execute_with(|| {
		assert_ok!(EthereumBeaconClient::process_checkpoint_update(&checkpoint));

		// makes an invalid update where the attested_header slot value should be greater than the
		// checkpoint slot value
		update.finalized_header.slot = checkpoint.header.slot;
		update.attested_header.slot = checkpoint.header.slot;
		update.signature_slot = checkpoint.header.slot + 1;

		let result = EthereumBeaconClient::submit(RuntimeOrigin::signed(1), update);
		assert_err!(result, Error::<Test>::IrrelevantUpdate);
		assert_eq!(result.unwrap_err().post_info.pays_fee, Pays::Yes);
	});
}

#[test]
fn submit_update_with_missing_bootstrap() {
	let update = Box::new(load_next_finalized_header_update_fixture());

	new_tester().execute_with(|| {
		let result = EthereumBeaconClient::submit(RuntimeOrigin::signed(1), update);
		assert_err!(result, Error::<Test>::NotBootstrapped);
		assert_eq!(result.unwrap_err().post_info.pays_fee, Pays::Yes);
	});
}

#[test]
fn submit_update_with_invalid_sync_committee_update() {
	let checkpoint = Box::new(load_checkpoint_update_fixture());
	let update = Box::new(load_sync_committee_update_fixture());
	let mut next_update = Box::new(load_next_sync_committee_update_fixture());

	new_tester().execute_with(|| {
		assert_ok!(EthereumBeaconClient::process_checkpoint_update(&checkpoint));

		let result = EthereumBeaconClient::submit(RuntimeOrigin::signed(1), update);
		assert_ok!(result);
		assert_eq!(result.unwrap().pays_fee, Pays::No);

		// makes update with invalid next_sync_committee
		<FinalizedBeaconState<Test>>::mutate(<LatestFinalizedBlockRoot<Test>>::get(), |x| {
			let prev = x.unwrap();
			*x = Some(CompactBeaconState { slot: next_update.attested_header.slot, ..prev });
		});
		next_update.attested_header.slot += 1;
		next_update.signature_slot = next_update.attested_header.slot + 1;
		let next_sync_committee = NextSyncCommitteeUpdate::default();
		next_update.next_sync_committee_update = Some(next_sync_committee);

		let second_result = EthereumBeaconClient::submit(RuntimeOrigin::signed(1), next_update);
		assert_err!(second_result, Error::<Test>::InvalidSyncCommitteeUpdate);
		assert_eq!(second_result.unwrap_err().post_info.pays_fee, Pays::Yes);
	});
}

/// Check that a gap of more than 8192 slots between finalized headers is not allowed.
#[test]
fn submit_finalized_header_update_with_too_large_gap() {
	let checkpoint = Box::new(load_checkpoint_update_fixture());
	let update = Box::new(load_sync_committee_update_fixture());
	let mut next_update = Box::new(load_next_sync_committee_update_fixture());

	// Adds 8193 slots, so that the next update is still in the next sync committee, but the
	// gap between the finalized headers is more than 8192 slots.
	let slot_with_large_gap = checkpoint.header.slot + SLOTS_PER_HISTORICAL_ROOT as u64 + 1;

	next_update.finalized_header.slot = slot_with_large_gap;
	// Adding some slots to the attested header and signature slot since they need to be ahead
	// of the finalized header.
	next_update.attested_header.slot = slot_with_large_gap + 33;
	next_update.signature_slot = slot_with_large_gap + 43;

	new_tester().execute_with(|| {
		assert_ok!(EthereumBeaconClient::process_checkpoint_update(&checkpoint));
		let result = EthereumBeaconClient::submit(RuntimeOrigin::signed(1), update.clone());
		assert_ok!(result);
		assert_eq!(result.unwrap().pays_fee, Pays::No);
		assert!(<NextSyncCommittee<Test>>::exists());

		let second_result =
			EthereumBeaconClient::submit(RuntimeOrigin::signed(1), next_update.clone());
		assert_err!(second_result, Error::<Test>::InvalidFinalizedHeaderGap);
		assert_eq!(second_result.unwrap_err().post_info.pays_fee, Pays::Yes);
	});
}

/// Check that a gap of 8192 slots between finalized headers is allowed.
#[test]
fn submit_finalized_header_update_with_gap_at_limit() {
	let checkpoint = Box::new(load_checkpoint_update_fixture());
	let update = Box::new(load_sync_committee_update_fixture());
	let mut next_update = Box::new(load_next_sync_committee_update_fixture());

	next_update.finalized_header.slot = checkpoint.header.slot + SLOTS_PER_HISTORICAL_ROOT as u64;
	// Adding some slots to the attested header and signature slot since they need to be ahead
	// of the finalized header.
	next_update.attested_header.slot =
		checkpoint.header.slot + SLOTS_PER_HISTORICAL_ROOT as u64 + 33;
	next_update.signature_slot = checkpoint.header.slot + SLOTS_PER_HISTORICAL_ROOT as u64 + 43;

	new_tester().execute_with(|| {
		assert_ok!(EthereumBeaconClient::process_checkpoint_update(&checkpoint));

		let result = EthereumBeaconClient::submit(RuntimeOrigin::signed(1), update.clone());
		assert_ok!(result);
		assert_eq!(result.unwrap().pays_fee, Pays::No);
		assert!(<NextSyncCommittee<Test>>::exists());

		let second_result =
			EthereumBeaconClient::submit(RuntimeOrigin::signed(1), next_update.clone());
		assert_err!(
			second_result,
			// The test should pass the InvalidFinalizedHeaderGap check, and will fail at the
			// next check, the merkle proof, because we changed the next_update slots.
			Error::<Test>::InvalidHeaderMerkleProof
		);
		assert_eq!(second_result.unwrap_err().post_info.pays_fee, Pays::Yes);
	});
}

#[test]
fn duplicate_sync_committee_updates_are_not_free() {
	let checkpoint = Box::new(load_checkpoint_update_fixture());
	let sync_committee_update = Box::new(load_sync_committee_update_fixture());

	new_tester().execute_with(|| {
		assert_ok!(EthereumBeaconClient::process_checkpoint_update(&checkpoint));
		let result =
			EthereumBeaconClient::submit(RuntimeOrigin::signed(1), sync_committee_update.clone());
		assert_ok!(result);
		assert_eq!(result.unwrap().pays_fee, Pays::No);

		// Check that if the same update is submitted, the update is not free.
		let second_result =
			EthereumBeaconClient::submit(RuntimeOrigin::signed(1), sync_committee_update);
		assert_ok!(second_result);
		assert_eq!(second_result.unwrap().pays_fee, Pays::Yes);
	});
}

#[test]
fn sync_committee_update_for_sync_committee_already_imported_are_not_free() {
	let checkpoint = Box::new(load_checkpoint_update_fixture());
	let sync_committee_update = Box::new(load_sync_committee_update_fixture()); // slot 129
	let second_sync_committee_update = load_sync_committee_update_period_0(); // slot 128
	let third_sync_committee_update = load_sync_committee_update_period_0_newer_fixture(); // slot 224
	let fourth_sync_committee_update = load_sync_committee_update_period_0_older_fixture(); // slot 96
	let fith_sync_committee_update = Box::new(load_next_sync_committee_update_fixture()); // slot 8259

	new_tester().execute_with(|| {
		assert_ok!(EthereumBeaconClient::process_checkpoint_update(&checkpoint));
		assert_eq!(<LatestSyncCommitteeUpdatePeriod<Test>>::get(), 0);

		// Check that setting the next sync committee for period 0 is free (it is not set yet).
		let result =
			EthereumBeaconClient::submit(RuntimeOrigin::signed(1), sync_committee_update.clone());
		assert_ok!(result);
		assert_eq!(result.unwrap().pays_fee, Pays::No);
		assert_eq!(<LatestSyncCommitteeUpdatePeriod<Test>>::get(), 0);

		// Check that setting the next sync committee for period 0 again is not free.
		let second_result =
			EthereumBeaconClient::submit(RuntimeOrigin::signed(1), second_sync_committee_update);
		assert_eq!(second_result.unwrap().pays_fee, Pays::Yes);
		assert_eq!(<LatestSyncCommitteeUpdatePeriod<Test>>::get(), 0);

		// Check that setting an update with a sync committee that has already been set, but with a
		// newer finalized header, is free.
		let third_result =
			EthereumBeaconClient::submit(RuntimeOrigin::signed(1), third_sync_committee_update);
		assert_eq!(third_result.unwrap().pays_fee, Pays::No);
		assert_eq!(<LatestSyncCommitteeUpdatePeriod<Test>>::get(), 0);

		// Check that setting the next sync committee for period 0 again with an earlier slot is not
		// free.
		let fourth_result =
			EthereumBeaconClient::submit(RuntimeOrigin::signed(1), fourth_sync_committee_update);
		assert_err!(fourth_result, Error::<Test>::IrrelevantUpdate);
		assert_eq!(fourth_result.unwrap_err().post_info.pays_fee, Pays::Yes);

		// Check that setting the next sync committee for period 1 is free.
		let fith_result =
			EthereumBeaconClient::submit(RuntimeOrigin::signed(1), fith_sync_committee_update);
		assert_eq!(fith_result.unwrap().pays_fee, Pays::No);
		assert_eq!(<LatestSyncCommitteeUpdatePeriod<Test>>::get(), 1);
	});
}

// IMPLS

#[test]
fn verify_message() {
	let (event_log, proof) = get_message_verification_payload();

	new_tester().execute_with(|| {
		assert_ok!(initialize_storage());
		assert_ok!(EthereumBeaconClient::verify(&event_log, &proof));
	});
}

#[test]
fn verify_message_invalid_proof() {
	let (event_log, mut proof) = get_message_verification_payload();
	proof.receipt_proof[0] = TEST_HASH.into();

	new_tester().execute_with(|| {
		assert_ok!(initialize_storage());
		assert_err!(
			EthereumBeaconClient::verify(&event_log, &proof),
			VerificationError::InvalidProof
		);
	});
}

#[test]
fn verify_message_invalid_receipts_root() {
	let (event_log, mut proof) = get_message_verification_payload();
	let mut payload = deneb::ExecutionPayloadHeader::default();
	payload.receipts_root = TEST_HASH.into();
	proof.execution_proof.execution_header = VersionedExecutionPayloadHeader::Deneb(payload);

	new_tester().execute_with(|| {
		assert_ok!(initialize_storage());
		assert_err!(
			EthereumBeaconClient::verify(&event_log, &proof),
			VerificationError::InvalidExecutionProof(
				Error::<Test>::BlockBodyHashTreeRootFailed.into()
			)
		);
	});
}

#[test]
fn verify_message_invalid_log() {
	let (mut event_log, proof) = get_message_verification_payload();
	event_log.topics = vec![H256::zero(); 10];
	new_tester().execute_with(|| {
		assert_ok!(initialize_storage());
		assert_err!(
			EthereumBeaconClient::verify(&event_log, &proof),
			VerificationError::LogNotFound
		);
	});
}

#[test]
fn verify_message_receipt_does_not_contain_log() {
	let (mut event_log, proof) = get_message_verification_payload();
	event_log.data = hex!("f9013c94ee9170abfbf9421ad6dd07f6bdec9d89f2b581e0f863a01b11dcf133cc240f682dab2d3a8e4cd35c5da8c9cf99adac4336f8512584c5ada000000000000000000000000000000000000000000000000000000000000003e8a00000000000000000000000000000000000000000000000000000000000000002b8c000000000000000000000000000000000000000000000000000000000000000200000000000000000000000000000000000000000000000000000000000000068000f000000000000000101d184c103f7acc340847eee82a0b909e3358bc28d440edffa1352b13227e8ee646f3ea37456dec70100000101001cbd2d43530a44705ad088af313e18f80b53ef16b36177cd4b77b846f2a5f07c0000e8890423c78a0000000000000000000000000000000000000000000000000000000000000000").to_vec();

	new_tester().execute_with(|| {
		assert_ok!(initialize_storage());
		assert_err!(
			EthereumBeaconClient::verify(&event_log, &proof),
			VerificationError::LogNotFound
		);
	});
}

#[test]
fn set_operating_mode() {
	let checkpoint = Box::new(load_checkpoint_update_fixture());
	let update = Box::new(load_finalized_header_update_fixture());

	new_tester().execute_with(|| {
		assert_ok!(EthereumBeaconClient::process_checkpoint_update(&checkpoint));

		assert_ok!(EthereumBeaconClient::set_operating_mode(
			RuntimeOrigin::root(),
			snowbridge_core::BasicOperatingMode::Halted
		));

		assert_noop!(
			EthereumBeaconClient::submit(RuntimeOrigin::signed(1), update),
			Error::<Test>::Halted
		);
	});
}

#[test]
fn verify_rejects_when_halted() {
	let (event_log, proof) = get_message_verification_payload();

	new_tester().execute_with(|| {
		assert_ok!(initialize_storage());
		// Sanity: verification succeeds in Normal mode.
		assert_ok!(EthereumBeaconClient::verify(&event_log, &proof));

		assert_ok!(EthereumBeaconClient::set_operating_mode(
			RuntimeOrigin::root(),
			snowbridge_core::BasicOperatingMode::Halted
		));

		// While halted, the verifier refuses all proofs — blocks inbound_queue_v2::submit and
		// outbound_queue_v2::submit_delivery_receipt from paying out against fraudulent proofs.
		assert_err!(EthereumBeaconClient::verify(&event_log, &proof), VerificationError::Halted);

		// Resuming restores verification.
		assert_ok!(EthereumBeaconClient::set_operating_mode(
			RuntimeOrigin::root(),
			snowbridge_core::BasicOperatingMode::Normal
		));
		assert_ok!(EthereumBeaconClient::verify(&event_log, &proof));
	});
}

#[test]
fn set_operating_mode_root_only() {
	new_tester().execute_with(|| {
		assert_noop!(
			EthereumBeaconClient::set_operating_mode(
				RuntimeOrigin::signed(1),
				snowbridge_core::BasicOperatingMode::Halted
			),
			DispatchError::BadOrigin
		);
	});
}

#[test]
fn verify_execution_proof_invalid_ancestry_proof() {
	let checkpoint = Box::new(load_checkpoint_update_fixture());
	let finalized_header_update = Box::new(load_finalized_header_update_fixture());
	let mut execution_header_update = Box::new(load_execution_proof_fixture());
	if let Some(ref mut ancestry_proof) = execution_header_update.ancestry_proof {
		ancestry_proof.header_branch[0] = TEST_HASH.into()
	}

	new_tester().execute_with(|| {
		assert_ok!(EthereumBeaconClient::process_checkpoint_update(&checkpoint));
		assert_ok!(EthereumBeaconClient::submit(RuntimeOrigin::signed(1), finalized_header_update));
		assert_err!(
			EthereumBeaconClient::verify_execution_proof(&execution_header_update),
			Error::<Test>::InvalidAncestryMerkleProof
		);
	});
}

#[test]
fn verify_execution_proof_invalid_execution_header_proof() {
	let checkpoint = Box::new(load_checkpoint_update_fixture());
	let finalized_header_update = Box::new(load_finalized_header_update_fixture());
	let mut execution_header_update = Box::new(load_execution_proof_fixture());
	execution_header_update.execution_branch[0] = TEST_HASH.into();

	new_tester().execute_with(|| {
		assert_ok!(EthereumBeaconClient::process_checkpoint_update(&checkpoint));
		assert_ok!(EthereumBeaconClient::submit(RuntimeOrigin::signed(1), finalized_header_update));
		assert_err!(
			EthereumBeaconClient::verify_execution_proof(&execution_header_update),
			Error::<Test>::InvalidExecutionHeaderProof
		);
	});
}

#[test]
fn verify_execution_proof_that_is_also_finalized_header_which_is_not_stored() {
	let checkpoint = Box::new(load_checkpoint_update_fixture());
	let finalized_header_update = Box::new(load_finalized_header_update_fixture());
	let mut execution_header_update = Box::new(load_execution_proof_fixture());
	execution_header_update.ancestry_proof = None;

	new_tester().execute_with(|| {
		assert_ok!(EthereumBeaconClient::process_checkpoint_update(&checkpoint));
		assert_ok!(EthereumBeaconClient::submit(RuntimeOrigin::signed(1), finalized_header_update));
		assert_err!(
			EthereumBeaconClient::verify_execution_proof(&execution_header_update),
			Error::<Test>::ExpectedFinalizedHeaderNotStored
		);
	});
}

#[test]
fn submit_execution_proof_that_is_also_finalized_header_which_is_stored_but_slots_dont_match() {
	let checkpoint = Box::new(load_checkpoint_update_fixture());
	let finalized_header_update = Box::new(load_finalized_header_update_fixture());
	let mut execution_header_update = Box::new(load_execution_proof_fixture());
	execution_header_update.ancestry_proof = None;

	new_tester().execute_with(|| {
		assert_ok!(EthereumBeaconClient::process_checkpoint_update(&checkpoint));
		assert_ok!(EthereumBeaconClient::submit(RuntimeOrigin::signed(1), finalized_header_update));

		let block_root: H256 = execution_header_update.header.hash_tree_root().unwrap();

		<FinalizedBeaconState<Test>>::insert(
			block_root,
			CompactBeaconState {
				slot: execution_header_update.header.slot + 1,
				block_roots_root: Default::default(),
			},
		);
		LatestFinalizedBlockRoot::<Test>::set(block_root);

		assert_err!(
			EthereumBeaconClient::verify_execution_proof(&execution_header_update),
			Error::<Test>::ExpectedFinalizedHeaderNotStored
		);
	});
}

#[test]
fn verify_execution_proof_not_finalized() {
	let checkpoint = Box::new(load_checkpoint_update_fixture());
	let finalized_header_update = Box::new(load_finalized_header_update_fixture());
	let update = Box::new(load_execution_proof_fixture());

	new_tester().execute_with(|| {
		assert_ok!(EthereumBeaconClient::process_checkpoint_update(&checkpoint));
		assert_ok!(EthereumBeaconClient::submit(RuntimeOrigin::signed(1), finalized_header_update));

		<FinalizedBeaconState<Test>>::mutate(<LatestFinalizedBlockRoot<Test>>::get(), |x| {
			let prev = x.unwrap();
			*x = Some(CompactBeaconState { slot: update.header.slot - 1, ..prev });
		});

		assert_err!(
			EthereumBeaconClient::verify_execution_proof(&update),
			Error::<Test>::HeaderNotFinalized
		);
	});
}

#[test]
fn verify_message_invalid_topic() {
	let (event_log, proof) = get_message_verification_payload();
	let mut event_log_muted = event_log.clone();
	event_log_muted.topics[0] = H256::default();

	new_tester().execute_with(|| {
		assert_ok!(initialize_storage());
		assert_err!(
			EthereumBeaconClient::verify(&event_log_muted, &proof),
			VerificationError::LogNotFound
		);
	});
}

#[test]
fn signing_root_uses_previous_slot_for_fork_version() {
	new_tester().execute_with(|| {
		// Use a signature_slot at a fork boundary (first slot of the fulu epoch).
		// In mock.rs: electra.epoch = 0, fulu.epoch = 100000000
		let fulu_epoch = ChainForkVersions::get().fulu.epoch;
		let signature_slot: u64 = fulu_epoch * (SLOTS_PER_EPOCH as u64);

		// Verify this is the first slot of the epoch
		assert_eq!(signature_slot % (SLOTS_PER_EPOCH as u64), 0);

		let header = BeaconHeader {
			slot: signature_slot - 1,
			proposer_index: 0,
			parent_root: H256::repeat_byte(0x11),
			state_root: H256::repeat_byte(0x22),
			body_root: H256::repeat_byte(0x33),
		};

		let validators_root = H256::repeat_byte(0x44);

		// Get fork versions for comparison
		let fork_version_at_signature_slot = EthereumBeaconClient::compute_fork_version(
			compute_epoch(signature_slot, SLOTS_PER_EPOCH as u64),
		);
		let fork_version_at_previous_slot = EthereumBeaconClient::compute_fork_version(
			compute_epoch(signature_slot.saturating_sub(1), SLOTS_PER_EPOCH as u64),
		);

		// At the fork boundary, these should differ
		assert_ne!(
			fork_version_at_signature_slot, fork_version_at_previous_slot,
			"Test setup error: fork versions should differ at fork boundary"
		);

		// Compute signing roots using both fork versions
		let domain_type = crate::config::DOMAIN_SYNC_COMMITTEE.to_vec();

		let domain_with_previous_slot = EthereumBeaconClient::compute_domain(
			domain_type.clone(),
			fork_version_at_previous_slot,
			validators_root,
		)
		.unwrap();

		let signing_root_with_previous_slot =
			EthereumBeaconClient::compute_signing_root(&header, domain_with_previous_slot).unwrap();

		// The pallet's signing_root should use the previous slot's fork version (per spec)
		let pallet_signing_root =
			EthereumBeaconClient::signing_root(&header, validators_root, signature_slot).unwrap();

		assert_eq!(
			pallet_signing_root, signing_root_with_previous_slot,
			"signing_root should use fork version from signature_slot - 1"
		);
	});
}

#[test]
fn signing_root_handles_signature_slot_zero() {
	// Per spec: fork_version_slot = max(signature_slot, 1) - 1
	// When signature_slot = 0, saturating_sub(1) = 0, which matches max(0, 1) - 1 = 0
	new_tester().execute_with(|| {
		let header = BeaconHeader {
			slot: 0,
			proposer_index: 0,
			parent_root: H256::repeat_byte(0x11),
			state_root: H256::repeat_byte(0x22),
			body_root: H256::repeat_byte(0x33),
		};

		let validators_root = H256::repeat_byte(0x44);

		// Should not panic and should use epoch 0 fork version
		let result = EthereumBeaconClient::signing_root(&header, validators_root, 0);
		assert!(result.is_ok(), "signing_root should handle signature_slot = 0");
	});
}

/// Gloas merkle-branch fixtures, all from real consensus data.
///
/// Source: the [Platåberget](https://plataberget.dev/) public Gloas (ePBS) testnet, block
/// and finalized state at slot 115968. Nothing here is hand-built.
///
/// Both were merkleized locally from the EIP-7495/7916 rules and both reproduced the roots
/// the beacon node reports — the block root
/// `0xe992e2a80c28003ec1c8856043e097708759a6e37d97538b1783c33b711e3e98`
/// and the state root
/// `0x2a6dfcdb7ac509685c815fe141fd7da7f7b24eb8da488aa9d24eb1ff59d410e3`.
/// Those matches are what make these fixtures meaningful: they pin the progressive-container
/// tree shape against a live client rather than against our own model of it, and with it
/// that gindex 2856 addresses `signed_execution_payload_bid.message.parent_block_hash` and
/// gindex 352 addresses `block_roots`.
///
/// 352 in particular has no published spec constant to check against — the ancestry proof is
/// a Snowbridge addition — so real data is the only independent evidence that the Electra
/// value of 69 is wrong for Gloas.
mod gloas_branches {
	use super::*;

	const BODY_ROOT: [u8; 32] =
		hex!("3b72fb6996412d59a20d92dde86abe3a12b94232ba62a61ac301625f1f8950f0");
	/// `signed_execution_payload_bid.message.parent_block_hash`: the execution block the bid
	/// builds on, which is the block a Snowbridge event proof would target.
	const EXECUTION_BLOCK_HASH: [u8; 32] =
		hex!("9af0c93804597fe505c22f3b15abb47acb01f7c6106474b196b52f592fae0e39");

	const STATE_ROOT: [u8; 32] =
		hex!("2a6dfcdb7ac509685c815fe141fd7da7f7b24eb8da488aa9d24eb1ff59d410e3");
	const BLOCK_ROOTS_ROOT: [u8; 32] =
		hex!("df568d1e3e530cb6c53aded1ce8329387a2082ae93556b3658fab61eee147d28");
	const ANCESTRY_LEAF: [u8; 32] =
		hex!("84a3016f712471145043f63fe36f9c9dbf7c16b3a709ce4a2b22678b77acbc80");
	const ANCESTRY_SLOT: u64 = 115967;

	fn body_branch() -> Vec<H256> {
		vec![
			hex!("c54e8868daef8005f4c5e5fc56f214389b5b9fa8b8fb011499de194efd6d7dde").into(),
			hex!("ff0f000000000000000000000000000000000000000000000000000000000000").into(),
			hex!("99f12ce5f778798e053b12f50afff88c3a033718f2b608acb437f42da3f17c69").into(),
			hex!("f5a5fd42d16a20302798ef6ed309979b43003d2320d9f0e8ea9831a92759fb4b").into(),
			hex!("9db0c9c90f13f6a294022960a4e1ed53fcbd365f0a981c644d1a05fee99fcff4").into(),
			hex!("586a75beceb384ee9422a8c5259783a83ffa2740e1e38a76824ed302d8b224da").into(),
			hex!("c78009fdf07fc56a11f122370658a353aaa542ed63e44c4bc15ff4cd105ab33c").into(),
			hex!("0000000000000000000000000000000000000000000000000000000000000000").into(),
			hex!("713d100d5174b496d04cbfd23beae240266e811283d4c2d5b612463444616ff1").into(),
			hex!("5be2625aa3fcff45cc120a25a75f725fb4032d30325b20b9ae568559f5a8c5b9").into(),
			hex!("ff1f000000000000000000000000000000000000000000000000000000000000").into(),
		]
	}

	/// `state_root` -> `block_roots`, gindex 352.
	fn block_roots_branch() -> Vec<H256> {
		vec![
			hex!("786081308651ca939606cdbebd19fb80cb0a19e464067d932330de7aa3d9291f").into(),
			hex!("75cd565f590eb71bc088ad7eb90429ec8ef880f3ef5dc2cfd40bf41038b90b9b").into(),
			hex!("b4d09df25c13aab2c8c92030aab619266aea3c6eedf01a99eccafc5d94a67f0b").into(),
			hex!("fdbf553643a89c4f6f46aec59ad12d616cd4500f05785abcc5e8296e8dbe650e").into(),
			hex!("8d90695adae4bc7421537acd6885775f41ec240d63a934a24b3235cdc13e44e9").into(),
			hex!("da5a43416afb206542968e770914702380a1e1253f7efbf28bbe02e9e1a1436b").into(),
			hex!("c0b17d6a00000000000000000000000000000000000000000000000000000000").into(),
			hex!("ffffffffff3f0000000000000000000000000000000000000000000000000000").into(),
		]
	}

	/// `block_roots` -> the block root at a slot, depth `BLOCK_ROOT_AT_INDEX_DEPTH`.
	fn ancestry_branch() -> Vec<H256> {
		vec![
			hex!("39315bc08fc62b4582c5c246ff674d7ae35da70e8abb048267bdf24154eb77b1").into(),
			hex!("34f663b44aa3307aa454d7eddbf999f865cd0b4fbe638b7b07b313a3579fde3f").into(),
			hex!("c81e4ea481aab3c2c7d9d8cb5e2b6994a35caff9b5437546ba249e4438d1046d").into(),
			hex!("a5964286cf65763c5682ee2b754d86695cb2857c1bb06387f405a993c076c3ea").into(),
			hex!("112410017a93f6d8dadf4e1d3b32ef8b8ccd9786be5f084c34c648c480e5883a").into(),
			hex!("037fb7647d0e6dcda85b857aa7408f37ba4c61837c7dadc1c6198b7dae49ad13").into(),
			hex!("1251aff786c251583d20ed02b9ae225917cc6115377a6fbfe29244607ea7acca").into(),
			hex!("7c1f59f29dbfc8f663c3818f0f26f6f29fbb218fab0c53ec557c7eb5b4698838").into(),
			hex!("30973fd0636524623dc1a0ed30a216129d7f5bdb88abd08f1837d67108256a55").into(),
			hex!("212fc5af32a93d0159439fa84495a3f822cb7fa12c9af08fc49f48605fcf9f48").into(),
			hex!("e7c3104878f8a42edadbef3628722a51849342490591ea3b07e7c213b2e4533a").into(),
			hex!("cc157066363462a64b246eef7ab342ec0db9caf932b00f0c7f40755dae74f815").into(),
			hex!("bcb1a9d8c416c4e7f187f5f21861bdf5231bf3902c7816f179cdfc5bec9408bd").into(),
		]
	}

	fn gloas_slot() -> u64 {
		ChainForkVersions::get().gloas.epoch * (SLOTS_PER_EPOCH as u64)
	}

	fn ancestry_leaf_index() -> usize {
		((SLOTS_PER_HISTORICAL_ROOT as u64) + (ANCESTRY_SLOT % (SLOTS_PER_HISTORICAL_ROOT as u64)))
			as usize
	}

	/// The fixtures are only meaningful if they sit at the indices the pallet selects.
	#[test]
	fn pallet_selects_the_gloas_indices() {
		new_tester().execute_with(|| {
			assert_eq!(EthereumBeaconClient::execution_commitment_gindex(true), 2856);
			assert_eq!(EthereumBeaconClient::execution_commitment_gindex(false), 25);
			assert_eq!(
				EthereumBeaconClient::block_roots_gindex_at_slot(
					gloas_slot(),
					ChainForkVersions::get()
				),
				352
			);
		});
	}

	/// The execution block hash verifies into the block body at gindex 2856.
	#[test]
	fn execution_commitment_branch_verifies() {
		let g = EthereumBeaconClient::execution_commitment_gindex(true);
		assert_eq!(generalized_index_length(g), 11);
		assert!(verify_merkle_branch(
			EXECUTION_BLOCK_HASH.into(),
			&body_branch(),
			subtree_index(g),
			generalized_index_length(g),
			BODY_ROOT.into(),
		));
	}

	#[test]
	fn execution_commitment_branch_rejects_tampering() {
		let g = EthereumBeaconClient::execution_commitment_gindex(true);
		let (idx, depth) = (subtree_index(g), generalized_index_length(g));

		// A different execution block hash: the whole point of the commitment.
		let mut leaf = EXECUTION_BLOCK_HASH;
		leaf[0] ^= 0x01;
		assert!(!verify_merkle_branch(leaf.into(), &body_branch(), idx, depth, BODY_ROOT.into()));

		// Every sibling must actually be checked.
		for i in 0..depth {
			let mut branch = body_branch();
			branch[i] = H256::repeat_byte(0xff);
			assert!(
				!verify_merkle_branch(
					EXECUTION_BLOCK_HASH.into(),
					&branch,
					idx,
					depth,
					BODY_ROOT.into()
				),
				"sibling {i} was not checked"
			);
		}

		// A short branch must be rejected, not silently accepted.
		let mut short = body_branch();
		short.pop();
		assert!(!verify_merkle_branch(
			EXECUTION_BLOCK_HASH.into(),
			&short,
			idx,
			depth,
			BODY_ROOT.into()
		));

		// The pre-Gloas index must not verify a Gloas branch.
		let legacy = EthereumBeaconClient::execution_commitment_gindex(false);
		assert!(!verify_merkle_branch(
			EXECUTION_BLOCK_HASH.into(),
			&body_branch(),
			subtree_index(legacy),
			generalized_index_length(legacy),
			BODY_ROOT.into()
		));
	}

	/// `block_roots` verifies into the state at gindex 352, as `process_checkpoint_update`
	/// and `submit` both require.
	#[test]
	fn block_roots_branch_verifies() {
		let g = EthereumBeaconClient::block_roots_gindex_at_slot(
			gloas_slot(),
			ChainForkVersions::get(),
		);
		assert_eq!(generalized_index_length(g), 8);
		assert!(verify_merkle_branch(
			BLOCK_ROOTS_ROOT.into(),
			&block_roots_branch(),
			subtree_index(g),
			generalized_index_length(g),
			STATE_ROOT.into(),
		));
	}

	/// The correction this work exists for. On real Gloas data the Electra index does not
	/// verify, so shipping 69 would have broken every ancestry proof and `force_checkpoint`
	/// with it.
	#[test]
	fn electra_block_roots_index_rejects_gloas_data() {
		let electra = 69usize;
		assert!(!verify_merkle_branch(
			BLOCK_ROOTS_ROOT.into(),
			&block_roots_branch(),
			subtree_index(electra),
			generalized_index_length(electra),
			STATE_ROOT.into(),
		));
	}

	/// The inner half of the ancestry proof, indexed exactly as `verify_ancestry_proof` does
	/// it. `BLOCK_ROOT_AT_INDEX_DEPTH` is unchanged by Gloas because `BlockRoots` is still a
	/// plain `Vector[Root, 8192]`; this pins that against real data.
	#[test]
	fn ancestry_branch_verifies() {
		assert_eq!(ancestry_branch().len(), crate::config::BLOCK_ROOT_AT_INDEX_DEPTH);
		assert!(verify_merkle_branch(
			ANCESTRY_LEAF.into(),
			&ancestry_branch(),
			ancestry_leaf_index(),
			crate::config::BLOCK_ROOT_AT_INDEX_DEPTH,
			BLOCK_ROOTS_ROOT.into(),
		));

		// A wrong slot means a wrong index, which must not verify.
		assert!(!verify_merkle_branch(
			ANCESTRY_LEAF.into(),
			&ancestry_branch(),
			ancestry_leaf_index() + 1,
			crate::config::BLOCK_ROOT_AT_INDEX_DEPTH,
			BLOCK_ROOTS_ROOT.into(),
		));
	}

	/// The two halves compose, which is what the pallet actually relies on: `block_roots_root`
	/// is proven out of the state once and cached, then reused as the root for every ancestry
	/// proof. Both legs here are real data from the same slot.
	#[test]
	fn ancestry_path_chains_from_state_root() {
		let g = EthereumBeaconClient::block_roots_gindex_at_slot(
			gloas_slot(),
			ChainForkVersions::get(),
		);
		assert!(verify_merkle_branch(
			BLOCK_ROOTS_ROOT.into(),
			&block_roots_branch(),
			subtree_index(g),
			generalized_index_length(g),
			STATE_ROOT.into(),
		));
		assert!(verify_merkle_branch(
			ANCESTRY_LEAF.into(),
			&ancestry_branch(),
			ancestry_leaf_index(),
			crate::config::BLOCK_ROOT_AT_INDEX_DEPTH,
			BLOCK_ROOTS_ROOT.into(),
		));
	}
}

/// Sync-committee signature verification at, and across, the Gloas fork boundary.
///
/// Two things are covered. The boundary rule is pure logic: per
/// `validate_light_client_update` the fork version comes from `max(signature_slot, 1) - 1`,
/// so an update whose `signature_slot` is the *first* slot of the Gloas epoch still signs
/// under the Fulu version. Getting that backwards would reject every update across the
/// boundary.
///
/// The second is a real Gloas sync aggregate from Platåberget slot 115968, verified under a
/// domain built from that network's real Gloas fork version and genesis validators root.
mod gloas_sync_committee {
	use super::*;
	use snowbridge_beacon_primitives::{
		bls::{fast_aggregate_verify, prepare_milagro_pubkey},
		PublicKey, Signature, SigningData,
	};

	/// `max(signature_slot, 1) - 1` lands in the previous epoch, so the *previous* fork
	/// version signs. This is the case most likely to be got wrong.
	#[test]
	fn first_slot_of_gloas_epoch_still_signs_under_fulu() {
		new_tester().execute_with(|| {
			let forks = ChainForkVersions::get();
			let boundary = forks.gloas.epoch * (SLOTS_PER_EPOCH as u64);
			assert_eq!(boundary % (SLOTS_PER_EPOCH as u64), 0);

			let at_slot = EthereumBeaconClient::compute_fork_version(compute_epoch(
				boundary,
				SLOTS_PER_EPOCH as u64,
			));
			let at_previous = EthereumBeaconClient::compute_fork_version(compute_epoch(
				boundary.saturating_sub(1),
				SLOTS_PER_EPOCH as u64,
			));
			assert_eq!(at_slot, forks.gloas.version);
			assert_eq!(at_previous, forks.fulu.version, "signing crosses back into fulu");

			let header = BeaconHeader {
				slot: boundary - 1,
				proposer_index: 0,
				parent_root: H256::repeat_byte(0x11),
				state_root: H256::repeat_byte(0x22),
				body_root: H256::repeat_byte(0x33),
			};
			let validators_root = H256::repeat_byte(0x44);
			let expected = EthereumBeaconClient::compute_signing_root(
				&header,
				EthereumBeaconClient::compute_domain(
					crate::config::DOMAIN_SYNC_COMMITTEE.to_vec(),
					at_previous,
					validators_root,
				)
				.unwrap(),
			)
			.unwrap();

			assert_eq!(
				EthereumBeaconClient::signing_root(&header, validators_root, boundary).unwrap(),
				expected,
			);
		});
	}

	/// One epoch later the Gloas version is in force for signing too.
	#[test]
	fn epoch_after_the_boundary_signs_under_gloas() {
		new_tester().execute_with(|| {
			let forks = ChainForkVersions::get();
			let slot = (forks.gloas.epoch + 1) * (SLOTS_PER_EPOCH as u64);
			let version = EthereumBeaconClient::compute_fork_version(compute_epoch(
				slot.saturating_sub(1),
				SLOTS_PER_EPOCH as u64,
			));
			assert_eq!(version, forks.gloas.version);
		});
	}

	// ---- real Gloas aggregate, Platåberget slot 115968 ----
	/// Platåberget's Gloas fork version and genesis validators root.
	const GLOAS_FORK_VERSION: [u8; 4] = hex!("80733183");
	const GENESIS_VALIDATORS_ROOT: [u8; 32] =
		hex!("bb4a1a9e3f7f4e10edcd734e4acc3b5ffd4f830efe0af2748fa458cfee5d2658");
	/// The block root the sync committee attested to (the head at the previous slot).
	const ATTESTED_ROOT: [u8; 32] =
		hex!("84a3016f712471145043f63fe36f9c9dbf7c16b3a709ce4a2b22678b77acbc80");
	const AGGREGATE_PUBKEY: [u8; 48] = hex!(
		"a0204ec4a82c619af447bbe55ced39ba43a72be2eed1764d12e086b7f353433b"
		"56399d5e37d4dc2ac868f23dcc846f50"
	);
	/// 511 of 512 participated; `fast_aggregate_verify` subtracts the absent one from the
	/// committee aggregate, which is exactly what the pallet does.
	const ABSENT_PUBKEY: [u8; 48] = hex!(
		"abeaf40cb88549819e7778a1e94bb0aeb26a9a970b2fc1dd98951b4572528778"
		"3ca4b41a148aff6a95b068cb45ca0d94"
	);
	const SIGNATURE: [u8; 96] = hex!(
		"878bad4793e3e804765ba3ed094d671ca1debfdaa2877afc8349427d05e0f1ff"
		"ab526d393bc868211f9160886a430c3c0580879e3be37ed11f9c44187a2368e8"
		"5ce19f5a6a962afb93bd7941e719d1e616bd1d1f7d27df71ff7590df9cf3100f"
	);

	/// A real Gloas sync aggregate verifies under a domain the pallet computed. This is the
	/// end-to-end check that the Gloas fork version, `compute_domain` and the signing root
	/// still compose after EIP-7688 — the sync-committee signature scheme itself is
	/// unchanged, and this pins that.
	#[test]
	fn real_gloas_sync_aggregate_verifies() {
		new_tester().execute_with(|| {
			let domain = EthereumBeaconClient::compute_domain(
				crate::config::DOMAIN_SYNC_COMMITTEE.to_vec(),
				GLOAS_FORK_VERSION,
				GENESIS_VALIDATORS_ROOT.into(),
			)
			.unwrap();

			let signing_root = SigningData { object_root: ATTESTED_ROOT.into(), domain }
				.hash_tree_root()
				.unwrap();

			let aggregate = prepare_milagro_pubkey(&PublicKey(AGGREGATE_PUBKEY)).unwrap();
			let absent = vec![prepare_milagro_pubkey(&PublicKey(ABSENT_PUBKEY)).unwrap()];

			assert_ok!(fast_aggregate_verify(
				&aggregate,
				&absent,
				signing_root,
				&Signature(SIGNATURE),
			));
		});
	}

	/// The same aggregate must not verify under the Fulu version. Without this, a wrong
	/// fork-version arm would pass the test above unnoticed.
	#[test]
	fn real_gloas_aggregate_rejects_the_wrong_fork_version() {
		new_tester().execute_with(|| {
			let domain = EthereumBeaconClient::compute_domain(
				crate::config::DOMAIN_SYNC_COMMITTEE.to_vec(),
				hex!("70733183"), // Platåberget's fulu version
				GENESIS_VALIDATORS_ROOT.into(),
			)
			.unwrap();
			let signing_root = SigningData { object_root: ATTESTED_ROOT.into(), domain }
				.hash_tree_root()
				.unwrap();
			let aggregate = prepare_milagro_pubkey(&PublicKey(AGGREGATE_PUBKEY)).unwrap();
			let absent = vec![prepare_milagro_pubkey(&PublicKey(ABSENT_PUBKEY)).unwrap()];
			assert!(fast_aggregate_verify(
				&aggregate,
				&absent,
				signing_root,
				&Signature(SIGNATURE)
			)
			.is_err());
		});
	}
}

/// A full post-Gloas message proof, end to end, on real data.
///
/// Everything below comes from one slot of the [Platåberget](https://plataberget.dev/)
/// Gloas testnet: beacon block 115968, and the execution block it committed to
/// (EL block 113202, hash 0x9af0c93804597fe5…).
///
/// The chain the pallet walks is:
///
/// ```text
/// stored finalized beacon header
///   -- SSZ branch at 2856 --> keccak256(execution header rlp)
///   -- rlp element 5       --> receipts_root
///   -- receipt MPT         --> the receipt carrying the log
/// ```
///
/// The execution header is a genuine Gloas one — 23 RLP fields, including the
/// `block_access_list_hash` and `slot_number` that Gloas appends — so this also exercises
/// the parser against a header shape that does not exist pre-Gloas.
mod gloas_end_to_end {
	use super::*;
	use snowbridge_beacon_primitives::{
		AncestryProof, ExecutionProof, VersionedExecutionPayloadHeader,
	};
	use snowbridge_verification_primitives::{Log, Proof};
	use sp_core::H160;

	const BEACON_SLOT: u64 = 115968;
	const PROPOSER_INDEX: u64 = 58638;
	const PARENT_ROOT: [u8; 32] =
		hex!("84a3016f712471145043f63fe36f9c9dbf7c16b3a709ce4a2b22678b77acbc80");
	const BEACON_STATE_ROOT: [u8; 32] =
		hex!("2a6dfcdb7ac509685c815fe141fd7da7f7b24eb8da488aa9d24eb1ff59d410e3");
	const BODY_ROOT: [u8; 32] =
		hex!("3b72fb6996412d59a20d92dde86abe3a12b94232ba62a61ac301625f1f8950f0");
	const BLOCK_ROOTS_ROOT: [u8; 32] =
		hex!("df568d1e3e530cb6c53aded1ce8329387a2082ae93556b3658fab61eee147d28");

	/// Canonical RLP of the Gloas execution header. `keccak256` of these bytes is the value
	/// committed at gindex 2856.
	const EXECUTION_HEADER_RLP: [u8; 660] = hex!(
		"f90291a0faba146ad08c3f771eb6608436b92bb4c4689915540fde06a4b068b8"
		"57e592daa01dcc4de8dec75d7aab85b567b6ccd41ad312451b948a7413f0a142"
		"fd40d4934794f4e8263979a89dc357d7f9f79533febc7f3e287ba0e6b0dc3741"
		"5b8d711a5b79dbd90549f08302c4e3f3adf15a3154c7979cf7816ea03be2fa4f"
		"53c84d6f4c5dad717b27e46498681de378480ea74837d8a3e0b99defa06e8de8"
		"0294645f0e074c0e3d322adb756aa9c1896282dd4d9be22739b8006542b90100"
		"1038c501421a261a961aa2148a90855040c5100073e400a2405c19c29c0ea0a4"
		"009c08f4080c1a892528f0815eae90a2086100906c227745124f06c0319a2620"
		"d6433fa2002d1019c0a0220c04205566620c200a845012aa00056a1695982040"
		"1c418998964fb00627aa49768120289348e0234c48440e2011a0059804988220"
		"0229100d12c5428ae4531c0800040120041a4021b860441ea0633dee400681c9"
		"0d6a98ad00a2231208c812691e08864680164a1013240991be8102bc04629d2c"
		"83010c4300570b21c9690a254600c00286369a2d51a804b91046027e12003665"
		"36da3a2102021b91482e1cc026a90d6a7803988e344c61d402a468906304974a"
		"808301ba32840be8c71183ef5bf6846a92edb480a037f7ae9acf1b53b7a17ba5"
		"5feb12af034c029040ad32f9da0ebd740c7fa942d48800000000000000008477"
		"9aa3d8a0b263ef4022e60825091500b81487a1b9b2aba0bc91866e93330562b4"
		"aaf466d3832a000084107fe7bfa039315bc08fc62b4582c5c246ff674d7ae35d"
		"a70e8abb048267bdf24154eb77b1a0e3b0c44298fc1c149afbf4c8996fb92427"
		"ae41e4649b934ca495991b7852b855a0a30f7b39ef8f14ee6a00671e43e77087"
		"a9afa61273745ad85025a0974a165e098301c4ff"
	);

	const TX_INDEX: u64 = 23;
	const LOG_ADDRESS: [u8; 20] = hex!("fffffffffffffffffffffffffffffffffffffffe");

	fn beacon_header() -> BeaconHeader {
		BeaconHeader {
			slot: BEACON_SLOT,
			proposer_index: PROPOSER_INDEX,
			parent_root: PARENT_ROOT.into(),
			state_root: BEACON_STATE_ROOT.into(),
			body_root: BODY_ROOT.into(),
		}
	}

	fn execution_branch() -> Vec<H256> {
		vec![
			hex!("c54e8868daef8005f4c5e5fc56f214389b5b9fa8b8fb011499de194efd6d7dde").into(),
			hex!("ff0f000000000000000000000000000000000000000000000000000000000000").into(),
			hex!("99f12ce5f778798e053b12f50afff88c3a033718f2b608acb437f42da3f17c69").into(),
			hex!("f5a5fd42d16a20302798ef6ed309979b43003d2320d9f0e8ea9831a92759fb4b").into(),
			hex!("9db0c9c90f13f6a294022960a4e1ed53fcbd365f0a981c644d1a05fee99fcff4").into(),
			hex!("586a75beceb384ee9422a8c5259783a83ffa2740e1e38a76824ed302d8b224da").into(),
			hex!("c78009fdf07fc56a11f122370658a353aaa542ed63e44c4bc15ff4cd105ab33c").into(),
			hex!("0000000000000000000000000000000000000000000000000000000000000000").into(),
			hex!("713d100d5174b496d04cbfd23beae240266e811283d4c2d5b612463444616ff1").into(),
			hex!("5be2625aa3fcff45cc120a25a75f725fb4032d30325b20b9ae568559f5a8c5b9").into(),
			hex!("ff1f000000000000000000000000000000000000000000000000000000000000").into(),
		]
	}

	pub(super) fn receipt_proof() -> Vec<Vec<u8>> {
		vec![
			hex!(
				"f90131a0903b88892b17f8222d05e8333e67e096f68e54f70b43f0297dc92ba6"
				"b7251db7a084eb52a0c305d93eb8c8619e784663e1f46fafad17ab0d8df1d094"
				"b56c513d1ba067ff249ede5afe895e3b878ce5aa1287539766a0276756274f1a"
				"5bf9660c1e21a0218ead72552341649784764b99be3a644235e75a8c8b4c2155"
				"e98e10f21ce762a05f396763737eb138c49e1d08124b0c9e9067f1dccb2ce03d"
				"965c9d5d511da6b2a04b0d5c3e3d7d81fe95597ace2d5c7f17d0a57eaed7e212"
				"e113df8e43e9ecf835a0ffdd8e9c9dc6765ef99d7fcce229346ef59595aa3a55"
				"d4f35af117cfdba9a7ada07e6312e7e5b2eefbe2727e5e16b7f23de0fc93589d"
				"fe23d5d5f87c59adb30ad8a0ce974e78f0c61c552ad183f215afee72654cf989"
				"34754b07350286dd52ef4a2b8080808080808080"
			)
			.to_vec(),
			hex!(
				"f90211a087645467651c8763aca2fbcc2ccea7837742ee1c69e5e36bef9cb17b"
				"e299baefa033c52b0565ca769a3058a99d27eb176f29086c7a39bd6569442da9"
				"c1d52a2c7ba024aadb584ae746385d080c700a4842d245d7cb4d418b97673b90"
				"eaaaf4194cc2a0a92e9b070fca8b8938e5c7940eddf25eb724668d3a703a6051"
				"e494201e36ab88a0eed4feaf1e72168c8f3f01fef84839d56db013809c0941fb"
				"b49184842899e1aea071cee6d75eac871ad8abf86046c31721090b2fed66a8d5"
				"e4ea3d1c33c5f11053a0cefbbbe29993bc71b0c814755427b7e1bf397c55a1d3"
				"299e07b1f6b6bd622480a00756e5a046d501eb86b6f6367214b839af6b0778bd"
				"12db6fed9c5a4bd2e2448da036d649ef33828a30baafd6d7ccafb3c97ffb5bbe"
				"4b8fd19f694196e1df265cbca093552f74728f30135798242cf3d7178d8ef5be"
				"093ceb99765de9e3b9cc94cb1fa00dc11d3237e13698f371cd2d310260089409"
				"53663af8153fdd80fb178c0fc153a0a6aeaf28ea0b6f85d21c71c7a0115cdafc"
				"5450df036f80210438684eafc434b4a0683019ce1048534a40aff740be022bb8"
				"f7a9f4d887dc218bf3aa88f2c087015da0e07dea6f9ee7b09f60d6363716499f"
				"c4a66558c4cdd1a44bf1f9a8bd4867fd54a03787d62f43b5cbd0fa47d5b98014"
				"ea7c9070c4ca7184c8cfb3344e681a185bc2a0e188a0d7993590d4dcd9f715bc"
				"53fe9703a52dec45cc938d33b67211521a71bb80"
			)
			.to_vec(),
			hex!(
				"f901af20b901ab02f901a7018306d437b9010000000000000000000000000000"
				"0000000000000000000000000000000000000000000000000000000000000000"
				"0000000000008000000000000400000000000000000000000000100000000800"
				"0000000000000000000000000000000000000000000000000000002000000000"
				"0000000000000000000000000000100000000000000000100000004000000000"
				"0000000000000000000000000000000000000004000000000000000000000000"
				"0000000000000000000000000000000000000000000002000000000008000000"
				"0000000000000000000000000000000000004000000000000000000000000000"
				"00000000000000000000000000000000000000f89df89b94ffffffffffffffff"
				"fffffffffffffffffffffffef863a0ddf252ad1be2c89b69c2b068fc378daa95"
				"2ba7f163c4a11628f55a4df523b3efa000000000000000000000000093e7442e"
				"b39ab925e9c88b734fc3f73653b6bb2ca000000000000000000000000013bb43"
				"66ee032cc15d7ac1c55ff5d9977d6d8feba00000000000000000000000000000"
				"000000000000000000000000001c7cca26f3"
			)
			.to_vec(),
		]
	}

	pub(super) fn log() -> Log {
		Log {
			address: H160(LOG_ADDRESS),
			topics: vec![
				hex!("ddf252ad1be2c89b69c2b068fc378daa952ba7f163c4a11628f55a4df523b3ef").into(),
				hex!("00000000000000000000000093e7442eb39ab925e9c88b734fc3f73653b6bb2c").into(),
				hex!("00000000000000000000000013bb4366ee032cc15d7ac1c55ff5d9977d6d8feb").into(),
			],
			data: hex!("0000000000000000000000000000000000000000000000000000001c7cca26f3").to_vec(),
			tx_index: TX_INDEX,
		}
	}

	pub(super) fn execution_proof() -> ExecutionProof {
		ExecutionProof {
			header: beacon_header(),
			ancestry_proof: None::<AncestryProof>,
			execution_header: VersionedExecutionPayloadHeader::Gloas(
				EXECUTION_HEADER_RLP.to_vec().try_into().expect("fits the bound; qed"),
			),
			execution_branch: execution_branch(),
		}
	}

	fn store_header() {
		assert_ok!(EthereumBeaconClient::store_finalized_header(
			beacon_header(),
			BLOCK_ROOTS_ROOT.into(),
		));
	}

	/// The slot must land in the gloas era of the mock's fork schedule, or the variant/era
	/// cross-check rejects the proof before any of this is exercised.
	#[test]
	fn fixture_slot_is_in_the_gloas_era() {
		new_tester().execute_with(|| {
			assert!(
				compute_epoch(BEACON_SLOT, SLOTS_PER_EPOCH as u64) >=
					ChainForkVersions::get().gloas.epoch
			);
		});
	}

	#[test]
	fn verifies_a_real_gloas_message() {
		new_tester().execute_with(|| {
			store_header();
			assert_ok!(EthereumBeaconClient::verify(
				&log(),
				&Proof { receipt_proof: receipt_proof(), execution_proof: execution_proof() },
			));
		});
	}

	/// Altering the execution header changes its keccak hash, so the gindex-2856 branch no
	/// longer verifies. This is what stops a submitter pairing a genuine beacon proof with a
	/// header of their own.
	#[test]
	fn rejects_a_tampered_execution_header() {
		new_tester().execute_with(|| {
			store_header();
			let mut rlp = EXECUTION_HEADER_RLP.to_vec();
			let last = rlp.len() - 1;
			rlp[last] ^= 0x01;
			let mut proof = execution_proof();
			proof.execution_header =
				VersionedExecutionPayloadHeader::Gloas(rlp.try_into().expect("fits; qed"));
			assert!(EthereumBeaconClient::verify(
				&log(),
				&Proof { receipt_proof: receipt_proof(), execution_proof: proof },
			)
			.is_err());
		});
	}

	/// A receipt proof for one transaction index must not satisfy a log claiming another.
	#[test]
	fn rejects_a_wrong_transaction_index() {
		new_tester().execute_with(|| {
			store_header();
			let mut l = log();
			l.tx_index += 1;
			assert!(EthereumBeaconClient::verify(
				&l,
				&Proof { receipt_proof: receipt_proof(), execution_proof: execution_proof() },
			)
			.is_err());
		});
	}

	/// A log that is not in the proven receipt must be rejected even though every proof in
	/// the chain is genuine.
	#[test]
	fn rejects_a_log_that_is_not_in_the_receipt() {
		new_tester().execute_with(|| {
			store_header();
			let mut l = log();
			l.address = H160::repeat_byte(0xaa);
			assert!(EthereumBeaconClient::verify(
				&l,
				&Proof { receipt_proof: receipt_proof(), execution_proof: execution_proof() },
			)
			.is_err());
		});
	}

	/// Without a stored finalized header there is nothing to anchor the proof to.
	#[test]
	fn rejects_when_not_bootstrapped() {
		new_tester().execute_with(|| {
			assert!(EthereumBeaconClient::verify(
				&log(),
				&Proof { receipt_proof: receipt_proof(), execution_proof: execution_proof() },
			)
			.is_err());
		});
	}
}

/// The checkpoint and ancestry halves, driven through the pallet's own code rather than
/// through `verify_merkle_branch` directly.
///
/// `force_checkpoint` is where the gindex-352 `block_roots` arm is actually consumed: it
/// proves `block_roots_root` out of the checkpoint state and caches it, and every ancestry
/// proof afterwards is rooted at that cached value. Testing 352 standalone leaves that
/// wiring unproven, which is why this exists.
///
/// Real data throughout: checkpoint at Platåberget slot 116768, whose state merkleizes to
/// the `state_root` its own block commits to, and which carries the slot-115968 block root
/// used by the execution proof above.
mod gloas_checkpoint_and_ancestry {
	use super::*;
	use snowbridge_beacon_primitives::AncestryProof;
	use snowbridge_verification_primitives::Proof;

	/// Root of the checkpoint block at slot 116768.
	const CHECKPOINT_ROOT: [u8; 32] =
		hex!("948810303d3811804809e3a34bbb3fac86a499dcbd51563e209694b445ae43dd");

	fn ancestry_branch() -> Vec<H256> {
		vec![
			hex!("34ad70e7bfdefa97ae4845db070be1eb12ba1d6ec2d7376a2fb008e44615318e").into(),
			hex!("2d1332677711812806ef7b0ff6c6c50d773f15d23c7e2c2d3ee142724d78b0f7").into(),
			hex!("38bbced91f735a809a4450ece25d083c028a68b7e4e582cc47959316ab6d9c0a").into(),
			hex!("2c655ea1f2b030b743acbb1a1e70a42fd07990d75519931638b2a119be441a68").into(),
			hex!("5be4bc07b3f859f3db1434ae7f82c18c0731c12c0a739751bff0383acde38ce8").into(),
			hex!("137dd6af3e39676c2e3adef993d610d9923d85cad031bfe94209e7d8dd4eb3e0").into(),
			hex!("fdb41ed3757dfb8744426a1650eed685434f03d63a95fbea97dfa6b5891e8443").into(),
			hex!("f0f5812e4857a5e5fa64bc219af78db11442fd966f1454479f15779d6f92ef03").into(),
			hex!("625ecdda8d467e5bf99b94be3979ad06c912cb47f56bf504c001ee4dfaed47ce").into(),
			hex!("22f277d0bbc2180a1ecc572f58fece1e5411a37b689a9c7add7d97d0396024a0").into(),
			hex!("e7c3104878f8a42edadbef3628722a51849342490591ea3b07e7c213b2e4533a").into(),
			hex!("d56719ecf7cb71d26f2a4ad68090c85fa98c792c3cbc4d9aa67b67adc405863c").into(),
			hex!("bcb1a9d8c416c4e7f187f5f21861bdf5231bf3902c7816f179cdfc5bec9408bd").into(),
		]
	}

	/// The Gloas checkpoint verifies through `force_checkpoint`, which is what proves the
	/// gindex-2945 sync-committee branch and the gindex-352 `block_roots` branch.
	#[test]
	fn gloas_checkpoint_is_accepted() {
		new_tester().execute_with(|| {
			let update = load_gloas_checkpoint_fixture();
			assert_ok!(EthereumBeaconClient::process_checkpoint_update(&update));
			assert_eq!(<LatestFinalizedBlockRoot<Test>>::get(), CHECKPOINT_ROOT.into());
			let stored = <FinalizedBeaconState<Test>>::get(H256::from(CHECKPOINT_ROOT)).unwrap();
			assert_eq!(stored.block_roots_root, update.block_roots_root);
		});
	}

	/// A Gloas checkpoint at the Electra `block_roots` index must not verify. This is the
	/// 352-versus-69 correction, asserted through the pallet rather than against
	/// `verify_merkle_branch` in isolation.
	#[test]
	fn gloas_checkpoint_fails_if_block_roots_branch_is_wrong() {
		new_tester().execute_with(|| {
			let mut update = load_gloas_checkpoint_fixture();
			update.block_roots_branch.pop();
			assert_err!(
				EthereumBeaconClient::process_checkpoint_update(&update),
				Error::<Test>::InvalidBlockRootsRootMerkleProof
			);
		});
	}

	/// The full path with a real ancestry proof: the execution proof's beacon header is not
	/// the finalized header, so `verify_ancestry_proof` runs against the `block_roots_root`
	/// the checkpoint cached.
	#[test]
	fn verifies_a_gloas_message_via_ancestry_proof() {
		new_tester().execute_with(|| {
			assert_ok!(EthereumBeaconClient::process_checkpoint_update(
				&load_gloas_checkpoint_fixture()
			));

			let mut proof = super::gloas_end_to_end::execution_proof();
			proof.ancestry_proof = Some(AncestryProof {
				header_branch: ancestry_branch(),
				finalized_block_root: CHECKPOINT_ROOT.into(),
			});

			assert_ok!(EthereumBeaconClient::verify(
				&super::gloas_end_to_end::log(),
				&Proof {
					receipt_proof: super::gloas_end_to_end::receipt_proof(),
					execution_proof: proof,
				},
			));
		});
	}

	/// A corrupted ancestry branch must be rejected even though the checkpoint and the
	/// execution proof are both genuine.
	#[test]
	fn rejects_a_corrupted_ancestry_branch() {
		new_tester().execute_with(|| {
			assert_ok!(EthereumBeaconClient::process_checkpoint_update(
				&load_gloas_checkpoint_fixture()
			));
			let mut branch = ancestry_branch();
			branch[0] = H256::repeat_byte(0xff);
			let mut proof = super::gloas_end_to_end::execution_proof();
			proof.ancestry_proof = Some(AncestryProof {
				header_branch: branch,
				finalized_block_root: CHECKPOINT_ROOT.into(),
			});
			assert!(EthereumBeaconClient::verify(
				&super::gloas_end_to_end::log(),
				&Proof {
					receipt_proof: super::gloas_end_to_end::receipt_proof(),
					execution_proof: proof,
				},
			)
			.is_err());
		});
	}
}

/// Fork isolation: adding Gloas must not break the legacy path, and neither variant may
/// be presented for the other fork's era.
///
/// The variant/era cross-check in `verify_execution_proof` is what enforces the second
/// half. Until now it was only asserted at the merkle level — a Gloas branch not verifying
/// at gindex 25 — which is a different claim: that says the *proof* fails, not that the
/// pallet refuses to try. These drive it through `verify_execution_proof` itself.
mod gloas_fork_isolation {
	use super::*;
	fn gloas_era_slot() -> u64 {
		ChainForkVersions::get().gloas.epoch * (SLOTS_PER_EPOCH as u64)
	}

	fn is_gloas_era(slot: u64) -> bool {
		compute_epoch(slot, SLOTS_PER_EPOCH as u64) >= ChainForkVersions::get().gloas.epoch
	}

	/// The legacy path still works with Gloas configured. Guards against the Gloas arms
	/// silently capturing pre-Gloas proofs.
	#[test]
	fn legacy_proof_still_verifies_after_gloas_is_configured() {
		let mut proof = Box::new(load_execution_proof_fixture());
		proof.ancestry_proof = None;

		new_tester().execute_with(|| {
			// The assertion is only meaningful if the fixture really is pre-Gloas.
			assert!(!is_gloas_era(proof.header.slot));
			assert!(!proof.execution_header.commitment().unwrap().is_gloas());

			assert_ok!(EthereumBeaconClient::store_finalized_header(
				proof.header,
				H256::repeat_byte(0x99),
			));
			assert_ok!(EthereumBeaconClient::verify_execution_proof(&proof));
		});
	}

	/// A Gloas proof presented for a pre-Gloas header is refused on the era check, before
	/// the branch is even looked at. Without this a submitter could pick their own
	/// verification path.
	///
	/// This is also what protects a chain that has not scheduled Gloas. The runtimes ship
	/// `gloas.epoch = u64::MAX` until the fork is announced, which makes *every* slot
	/// pre-Gloas era, so a Gloas-variant proof reduces to exactly this case and is refused
	/// rather than mis-verified.
	#[test]
	fn gloas_variant_is_refused_for_a_pre_gloas_header() {
		let mut proof = super::gloas_end_to_end::execution_proof();
		proof.ancestry_proof = None;
		proof.header.slot = 64; // pre-Gloas in the mock's schedule

		new_tester().execute_with(|| {
			assert!(!is_gloas_era(proof.header.slot));
			assert!(proof.execution_header.commitment().unwrap().is_gloas());

			assert_ok!(EthereumBeaconClient::store_finalized_header(
				proof.header,
				H256::repeat_byte(0x99),
			));
			assert_err!(
				EthereumBeaconClient::verify_execution_proof(&proof),
				Error::<Test>::InvalidExecutionHeaderProof
			);
		});
	}

	/// And the reverse: a legacy proof cannot be replayed against a Gloas-era header to
	/// smuggle a post-Gloas claim onto the cheaper pre-Gloas gindex.
	#[test]
	fn legacy_variant_is_refused_for_a_gloas_era_header() {
		let mut proof = Box::new(load_execution_proof_fixture());
		proof.ancestry_proof = None;
		proof.header.slot = gloas_era_slot();

		new_tester().execute_with(|| {
			assert!(is_gloas_era(proof.header.slot));
			assert!(!proof.execution_header.commitment().unwrap().is_gloas());

			assert_ok!(EthereumBeaconClient::store_finalized_header(
				proof.header,
				H256::repeat_byte(0x99),
			));
			assert_err!(
				EthereumBeaconClient::verify_execution_proof(&proof),
				Error::<Test>::InvalidExecutionHeaderProof
			);
		});
	}
}

/// A second, independent end-to-end message — different beacon block, different execution
/// block, different transaction.
///
/// One fixture can pass for reasons peculiar to itself. This one differs from
/// `gloas_end_to_end` in ways that matter: the payload bid carries **11 blob commitments**
/// where the other carried none, so the branch to gindex 2856 runs through a different
/// subtree; the execution header has 29 bytes of `extra_data` and a non-zero `blob_gas_used`,
/// so the RLP walk sees different item widths; and the receipt is proven at a different
/// transaction index, against a log that is not the first in its receipt.
///
/// Platåberget beacon block 128608, execution block 125689.
mod gloas_end_to_end_second {
	use super::*;
	use snowbridge_beacon_primitives::{
		AncestryProof, ExecutionProof, VersionedExecutionPayloadHeader,
	};
	use snowbridge_verification_primitives::{Log, Proof};
	use sp_core::H160;

	const BODY_ROOT: [u8; 32] =
		hex!("6c4326c786538f0f75e8f7985d8f792487914611b660cbf46b9434fdfb5ff42f");
	const EXECUTION_BLOCK_HASH: [u8; 32] =
		hex!("7232474f8d4a21d2909ecb502c27b5ae3443c74db80095ec15ef086f5a56c057");
	const EXECUTION_HEADER_RLP: [u8; 690] = hex!(
		"f902afa03b1156a0e9a12472425d03c4bfd448b9c2026b1fdf088cf3289f4538"
		"78416aa9a01dcc4de8dec75d7aab85b567b6ccd41ad312451b948a7413f0a142"
		"fd40d4934794f97e180c050e5ab072211ad2c213eb5aee4df134a073b1854c5e"
		"f1543279fb5bcbfff718e5b91d79cd9d52d5c5a136b65fb98d37e2a0a4db3d7b"
		"b625dac6a0a59fac73c1b2f3c8c068875b9d9f68fac57a9c8adae106a037c3ba"
		"655f8213c2f6cf3ab0579bb012a5c60bfe4ca4f328b2ff0c89642a67bdb90100"
		"1023400464cf0442872a028dc8d6051dc3c51018f3408920405038c21c0d3085"
		"101c00e0000c8889090d188102850008004320c505243b47420c4681b21e2630"
		"de4b0806b808113de081320c463014666280612a06d4502200426a2e93900008"
		"0805899c9a46e0026d40050424080883c244a1405854461429000110041a06a6"
		"8201010e1045420a4491102a00440182941a49012984195a80812ceb5008c088"
		"0c6820ac01a2050a08cc32791628254a4054461ca20001f10442029c82528900"
		"03000f460444090340690aa74423c0600206074541f9043111658a1228002666"
		"66100e610c01b191080e08803411076c7c4db800020c00c50a856b916210b550"
		"808301eaf9840be8c59584062c3d90846a953e349d6275696c646f6f722f2f4e"
		"65746865726d696e642076312e34302e3061a0b7e4e8580ba3ae933eb98b977f"
		"a92435f836f656fd3518e87109a7c8291fcaae880000000000000000847453c2"
		"b3a0c6a36bc236ea27593da15251d91d556c925730f7407e30afe2ff0c3a0c1b"
		"bf3c83140000840cf131afa04bb04546b509ced8dedb770bbaceebedd99f2c70"
		"e17c7a26f4f4e8850ed34758a0e3b0c44298fc1c149afbf4c8996fb92427ae41"
		"e4649b934ca495991b7852b855a0df196f581609037bc82f7d74be3a82ca120b"
		"7b3f3c5d65f0ab1a4ab5426a3f178301f65f"
	);
	const TX_INDEX: u64 = 8;
	const LOG_ADDRESS: [u8; 20] = hex!("fffffffffffffffffffffffffffffffffffffffe");

	fn beacon_header() -> BeaconHeader {
		BeaconHeader {
			slot: 128608,
			proposer_index: 1950,
			parent_root: hex!("e13eeebecc2670a233de5c1acb51aa3ac5e70c47e073aa89e8b41246591b31e9")
				.into(),
			state_root: hex!("885c2ca09e76e2c2000f43d2df3f0f4928ce9933af49cec81aefe416cf3ed86b")
				.into(),
			body_root: BODY_ROOT.into(),
		}
	}

	fn execution_branch() -> Vec<H256> {
		vec![
			hex!("3076238706742ac39fd1c1d66f57a0fce1815dd1c17d9ecda4907450d46bea5c").into(),
			hex!("ff0f000000000000000000000000000000000000000000000000000000000000").into(),
			hex!("5aa023f6916c2b0a79745eb081b620c9aef6d4e60120a9334b845b3fb92381f9").into(),
			hex!("f5a5fd42d16a20302798ef6ed309979b43003d2320d9f0e8ea9831a92759fb4b").into(),
			hex!("95dd9c9499ed213ca60a2dc33466b09605e2335c95c70797ae46837ec2a7da4f").into(),
			hex!("2a63671a5217bf2d114e4db54dc674b5b15601503d52ec7126840d39d530cc7b").into(),
			hex!("c78009fdf07fc56a11f122370658a353aaa542ed63e44c4bc15ff4cd105ab33c").into(),
			hex!("0000000000000000000000000000000000000000000000000000000000000000").into(),
			hex!("886c3ce370b103f718f2dc5d56d021431d97e2bb491aa7cb42cf9677047bd4ed").into(),
			hex!("18382df1ac8fcf9ea8521c85ae3bf7605c7447f42730f2048ed98b798256455d").into(),
			hex!("ff1f000000000000000000000000000000000000000000000000000000000000").into(),
		]
	}

	fn receipt_proof() -> Vec<Vec<u8>> {
		vec![
			hex!(
				"f90111a00d70e377b5d3d6ec5d3554869060fdf637975f6c37e3b08a2aef7abd"
				"bef6d874a046264bf2c3c180b9f5f94f0f92242b10a2b9db23ef965f785510b3"
				"986f2346eea0d82ec4ef58949250aad74dcfa396ff9359359356eebc91c35de8"
				"f58a2bc06f6fa0313617bbe4d71c509564b5b13f9eb09b3d1341d6a03d5a9ff0"
				"9ff96a227980fea0a6add39b96a29c5c278cbed379ad965a685623a75de84891"
				"d418de01efbf4a1aa0f56b145a735d454e288c686612bfdefa76822b658948e9"
				"ef7c9335e881e7db46a09b9a9059cb75519fa5698aedc526661fc33fc4b749e6"
				"c7ecebba781966577aa080a00c7043cf862a4186ac2f25e7ad3bcad16ff920d6"
				"a35771a699a9c40b490eb2338080808080808080"
			)
			.to_vec(),
			hex!(
				"f901f180a053a139fc9d77162e3ca49b36512ea8da5c026d244a7f60fc40115f"
				"d19342e5dda004cf3a2035ea68c142b389fcf46e7edc879bf0f4c76684c320f5"
				"8bfdac65b527a0864a04f6599921df1e6337c37de299720ab7320de2dafdd6d3"
				"21244b8bafe6e5a066a3e24f416b5634a68cf8fe180629e4ec3641f2fa4186b4"
				"dd63e69e5431e847a000954567937c701711a16e2a0f52684235d53f872a376a"
				"61011209e1973e4476a0a468dc682349c174d5b6ade2a666bf9277872eaf3684"
				"b8edf4c6a593e1686027a0e919ef7004fed8162d3ebf202d8de7a8b71d660d60"
				"7cf0c707e068509aef21e8a003c50073c61048d4ed37699116258473b8fde6f4"
				"e3819828c6e92eb200a8d983a072424026cb4c1e505d72f07cc256784122af13"
				"db8df1fe473acd0c3f2c3740b9a0a8c9360bbeeca020b09f6a002aa7fc7033a2"
				"0b65369bd302a19ba5616a810418a013fb872b14b995d06b07544f714ba5482a"
				"72b3d941713f6a40b52fe7a6a68f09a03518772dc585012084a5910281689eb2"
				"0c21b23881c7fd52ea0771c382d969e2a0a35bed0d5391fb1b2dcf9f91adb37d"
				"ad20d9d5ed971d6bf82b9772efdeda4c02a00feb2a13dc9eb9632af012ee2827"
				"ae5356f5f4678f9cbf71513f0ac29020c4ada0e548ca0a3feead1822d622e7a5"
				"9b83e19e3b95affd9c2d6c66d135f9e0ce2fb580"
			)
			.to_vec(),
			hex!(
				"f9057c20b9057802f90574018305d309b9010000200000000000000002000080"
				"0000000000100040000000000000800000000000000000000000000000000000"
				"8000000000000000000100000000000000000000020000000000000000000800"
				"0000204000000200100000000000008000000000000000000000002000000000"
				"0000000000000000000000000000100000000000000000000040000000000000"
				"0000000000000100000008000000400000000000080004000000000000000000"
				"0000000000000000000001000000000000080000000402000008000008000000"
				"0000000000000000080010000000000000004000100000000000000000000004"
				"00000000000000000000400000000000000100f90469f89b94ffffffffffffff"
				"fffffffffffffffffffffffffef863a0ddf252ad1be2c89b69c2b068fc378daa"
				"952ba7f163c4a11628f55a4df523b3efa0000000000000000000000000fc0d12"
				"6d830db7f740475de336c28cca26e36994a00000000000000000000000005401"
				"1a28e45cbbeeaaa778c66f9fafe6fea1fcbca000000000000000000000000000"
				"00000000000000000000000028ef2f6b021ebaf89b94ffffffffffffffffffff"
				"fffffffffffffffffffef863a0ddf252ad1be2c89b69c2b068fc378daa952ba7"
				"f163c4a11628f55a4df523b3efa000000000000000000000000054011a28e45c"
				"bbeeaaa778c66f9fafe6fea1fcbca00000000000000000000000007225fa654d"
				"238fc50271d3ad9737213cd0111b69a000000000000000000000000000000000"
				"00000000000000000028ef2f6b021ebaf87a947225fa654d238fc50271d3ad97"
				"37213cd0111b69f842a0e1fffcc4923d04b559f4d29a8bfc6cda04eb5b0d3c46"
				"0751c2402c5c5cc9109ca000000000000000000000000054011a28e45cbbeeaa"
				"a778c66f9fafe6fea1fcbca00000000000000000000000000000000000000000"
				"000000000028ef2f6b021ebaf89b947225fa654d238fc50271d3ad9737213cd0"
				"111b69f863a0ddf252ad1be2c89b69c2b068fc378daa952ba7f163c4a11628f5"
				"5a4df523b3efa000000000000000000000000054011a28e45cbbeeaaa778c66f"
				"9fafe6fea1fcbca0000000000000000000000000c7332d9f3a0f4786ce661925"
				"008148733564fb0ba00000000000000000000000000000000000000000000000"
				"000028ef2f6b021ebaf89b94029e947be3f40981b7058573e4870dd6960ae794"
				"f863a0ddf252ad1be2c89b69c2b068fc378daa952ba7f163c4a11628f55a4df5"
				"23b3efa0000000000000000000000000c7332d9f3a0f4786ce66192500814873"
				"3564fb0ba0000000000000000000000000fc0d126d830db7f740475de336c28c"
				"ca26e36994a0000000000000000000000000000000000000000000000005de57"
				"c3a40dbe8283f87994c7332d9f3a0f4786ce661925008148733564fb0be1a01c"
				"411e9a96e071241c2f21f7726b17ae89e3cab4c78be50e062b03a9fffbbad1b8"
				"40000000000000000000000000000000000000000000084d47582a398be9ca60"
				"c2000000000000000000000000000000000000000000000039bbfe12699a8ab8"
				"93f8fc94c7332d9f3a0f4786ce661925008148733564fb0bf863a0d78ad95fa4"
				"6c994b6551d0da85fc275fe613ce37657fb8d5e3d130840159d822a000000000"
				"000000000000000054011a28e45cbbeeaaa778c66f9fafe6fea1fcbca0000000"
				"000000000000000000fc0d126d830db7f740475de336c28cca26e36994b88000"
				"0000000000000000000000000000000000000000000000000000000000000000"
				"00000000000000000000000000000000000000000000000028ef2f6b021eba00"
				"0000000000000000000000000000000000000000000005de57c3a40dbe828300"
				"00000000000000000000000000000000000000000000000000000000000000"
			)
			.to_vec(),
		]
	}

	fn log() -> Log {
		Log {
			address: H160(LOG_ADDRESS),
			topics: vec![
				hex!("ddf252ad1be2c89b69c2b068fc378daa952ba7f163c4a11628f55a4df523b3ef").into(),
				hex!("00000000000000000000000054011a28e45cbbeeaaa778c66f9fafe6fea1fcbc").into(),
				hex!("0000000000000000000000007225fa654d238fc50271d3ad9737213cd0111b69").into(),
			],
			data: hex!("0000000000000000000000000000000000000000000000000028ef2f6b021eba").to_vec(),
			tx_index: TX_INDEX,
		}
	}

	fn execution_proof() -> ExecutionProof {
		ExecutionProof {
			header: beacon_header(),
			ancestry_proof: None::<AncestryProof>,
			execution_header: VersionedExecutionPayloadHeader::Gloas(
				EXECUTION_HEADER_RLP.to_vec().try_into().expect("fits the bound; qed"),
			),
			execution_branch: execution_branch(),
		}
	}

	/// The gindex-2856 leaf is the execution block hash, through a bid with a non-empty
	/// blob-commitment list.
	#[test]
	fn commitment_leaf_is_the_execution_block_hash() {
		let g = EthereumBeaconClient::execution_commitment_gindex(true);
		assert!(verify_merkle_branch(
			EXECUTION_BLOCK_HASH.into(),
			&execution_branch(),
			subtree_index(g),
			generalized_index_length(g),
			BODY_ROOT.into(),
		));
	}

	#[test]
	fn verifies_a_second_real_gloas_message() {
		new_tester().execute_with(|| {
			assert_ok!(EthereumBeaconClient::store_finalized_header(
				beacon_header(),
				H256::repeat_byte(0x99),
			));
			assert_ok!(EthereumBeaconClient::verify(
				&log(),
				&Proof { receipt_proof: receipt_proof(), execution_proof: execution_proof() },
			));
		});
	}

	/// The two fixtures must not be interchangeable: this message's receipt proof against the
	/// other message's execution proof is rejected.
	#[test]
	fn fixtures_are_not_interchangeable() {
		new_tester().execute_with(|| {
			assert_ok!(EthereumBeaconClient::store_finalized_header(
				beacon_header(),
				H256::repeat_byte(0x99),
			));
			assert!(EthereumBeaconClient::verify(
				&log(),
				&Proof {
					receipt_proof: super::gloas_end_to_end::receipt_proof(),
					execution_proof: execution_proof(),
				},
			)
			.is_err());
		});
	}
}
