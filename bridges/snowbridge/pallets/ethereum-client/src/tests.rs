// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: 2023 Snowfork <hello@snowfork.com>
use crate::{
	functions::compute_period, pallet::ExecutionHeaders, sync_committee_sum, verify_merkle_branch,
	BeaconHeader, CompactBeaconState, Error, ExecutionHeaderBuffer, FinalizedBeaconState,
	LatestExecutionState, LatestFinalizedBlockRoot, NextSyncCommittee, SyncCommitteePrepared,
};

use crate::mock::{
	get_message_verification_header, get_message_verification_payload,
	load_checkpoint_update_fixture, load_execution_header_update_fixture,
	load_finalized_header_update_fixture, load_next_finalized_header_update_fixture,
	load_next_sync_committee_update_fixture, load_sync_committee_update_fixture,
};

pub use crate::mock::*;

use crate::config::{EPOCHS_PER_SYNC_COMMITTEE_PERIOD, SLOTS_PER_EPOCH};
use frame_support::{assert_err, assert_noop, assert_ok};
use hex_literal::hex;
use primitives::{
	CompactExecutionHeader, ExecutionHeaderState, Fork, ForkVersions, NextSyncCommitteeUpdate,
	VersionedExecutionPayloadHeader,
};
use rand::{thread_rng, Rng};
use snowbridge_core::{
	inbound::{VerificationError, Verifier},
	RingBufferMap,
};
use sp_core::H256;
use sp_runtime::DispatchError;

/// Arbitrary hash used for tests and invalid hashes.
const TEST_HASH: [u8; 32] =
	hex!["5f6f02af29218292d21a69b64a794a7c0873b3e0f54611972863706e8cbdf371"];

/* UNIT TESTS */

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
			crate::config::FINALIZED_ROOT_INDEX,
			crate::config::FINALIZED_ROOT_DEPTH,
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
			crate::config::FINALIZED_ROOT_INDEX,
			crate::config::FINALIZED_ROOT_DEPTH,
			hex!("e46559327592741956f6beaa0f52e49625eb85dce037a0bd2eff333c743b287f").into()
		));
	});
}

#[test]
pub fn sync_committee_participation_is_supermajority() {
	let bits =
		hex!("bffffffff7f1ffdfcfeffeffbfdffffbfffffdffffefefffdffff7f7ffff77fffdf7bff77ffdf7fffafffffff77fefffeff7effffffff5f7fedfffdfb6ddff7b"
	);
	let participation = primitives::decompress_sync_committee_bits::<512, 64>(bits);
	assert_ok!(EthereumBeaconClient::sync_committee_participation_is_supermajority(&participation));
}

#[test]
pub fn sync_committee_participation_is_supermajority_errors_when_not_supermajority() {
	new_tester().execute_with(|| {
		let participation: [u8; 512] = [
			0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 1, 0, 1, 1, 0, 0, 0, 0,
			0, 0, 0, 0, 0, 0, 0, 1, 1, 1, 1, 1, 0, 1, 0, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1,
			1, 1, 1, 0, 1, 1, 1, 0, 0, 0, 0, 0, 1, 1, 1, 1, 1, 1, 0, 1, 1, 1, 0, 0, 0, 0, 0, 0, 0,
			0, 0, 0, 0, 0, 0, 1, 1, 0, 1, 1, 1, 1, 1, 1, 0, 1, 1, 1, 1, 1, 1, 0, 1, 1, 1, 1, 1, 1,
			1, 1, 1, 1, 1, 1, 0, 1, 1, 1, 1, 1, 1, 1, 0, 1, 1, 1, 1, 1, 0, 0, 0, 0, 1, 1, 1, 1, 1,
			0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 0, 1,
			1, 1, 1, 1, 1, 1, 0, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 0, 1, 1, 1, 1, 1,
			1, 1, 1, 1, 1, 1, 1, 1, 0, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1,
			1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 0, 1, 1, 1, 0, 1, 1, 1, 1, 1, 1, 1, 1, 1, 0, 1, 1, 1,
			1, 1, 1, 1, 1, 1, 0, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 0, 1, 1, 1, 1, 0, 1, 1, 1, 1, 1, 1,
			1, 1, 1, 1, 1, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 1, 1, 1, 0, 0, 0,
			1, 0, 1, 0, 1, 1, 1, 1, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1,
			1, 1, 1, 1, 1, 1, 1, 0, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 0, 1, 1, 1, 1, 0, 1, 1, 1, 1,
			1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 1, 1, 1, 0, 0, 0, 0, 0,
			1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 1, 1, 1, 1, 1, 0, 1,
			0, 1, 1, 1, 1, 1, 1, 1, 0, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 0, 1, 1,
			1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 0, 1, 1, 1, 1, 1, 1, 1, 1, 0, 1, 1, 0, 1, 0, 0,
			0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0,
		];

		assert_err!(
			EthereumBeaconClient::sync_committee_participation_is_supermajority(&participation),
			Error::<Test>::SyncCommitteeParticipantsNotSupermajority
		);
	});
}

#[test]
pub fn execution_header_pruning() {
	new_tester().execute_with(|| {
		let execution_header_prune_threshold = ExecutionHeadersPruneThreshold::get();
		let to_be_deleted = execution_header_prune_threshold / 2;

		let mut stored_hashes = vec![];

		for i in 0..execution_header_prune_threshold {
			let mut hash = H256::default();
			thread_rng().try_fill(&mut hash.0[..]).unwrap();
			EthereumBeaconClient::store_execution_header(
				hash,
				CompactExecutionHeader::default(),
				i as u64,
				hash,
			);
			stored_hashes.push(hash);
		}

		// We should have stored everything until now
		assert_eq!({ ExecutionHeaders::<Test>::iter().count() }, stored_hashes.len());

		// Let's push extra entries so that some of the previous entries are deleted.
		for i in 0..to_be_deleted {
			let mut hash = H256::default();
			thread_rng().try_fill(&mut hash.0[..]).unwrap();
			EthereumBeaconClient::store_execution_header(
				hash,
				CompactExecutionHeader::default(),
				(i + execution_header_prune_threshold) as u64,
				hash,
			);

			stored_hashes.push(hash);
		}

		// We should have only stored upto `execution_header_prune_threshold`
		assert_eq!(
			ExecutionHeaders::<Test>::iter().count() as u32,
			execution_header_prune_threshold
		);

		// First `to_be_deleted` items must be deleted
		for i in 0..to_be_deleted {
			assert!(!ExecutionHeaders::<Test>::contains_key(stored_hashes[i as usize]));
		}

		// Other entries should be part of data
		for i in to_be_deleted..(to_be_deleted + execution_header_prune_threshold) {
			assert!(ExecutionHeaders::<Test>::contains_key(stored_hashes[i as usize]));
		}
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
	});
}

#[test]
fn find_absent_keys() {
	let participation: [u8; 32] = [
		0, 1, 1, 1, 1, 1, 1, 0, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1,
		1, 1,
	];
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
	let participation: [u8; 32] = [
		0, 1, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0,
		1, 0,
	];
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

#[test]
fn cross_check_execution_state() {
	new_tester().execute_with(|| {
		let header_root: H256 = TEST_HASH.into();
		<FinalizedBeaconState<Test>>::insert(
			header_root,
			CompactBeaconState {
				// set slot to period 5
				slot: ((EPOCHS_PER_SYNC_COMMITTEE_PERIOD * SLOTS_PER_EPOCH) * 5) as u64,
				block_roots_root: Default::default(),
			},
		);
		LatestFinalizedBlockRoot::<Test>::set(header_root);
		<LatestExecutionState<Test>>::set(ExecutionHeaderState {
			beacon_block_root: Default::default(),
			// set slot to period 2
			beacon_slot: ((EPOCHS_PER_SYNC_COMMITTEE_PERIOD * SLOTS_PER_EPOCH) * 2) as u64,
			block_hash: Default::default(),
			block_number: 0,
		});

		assert_err!(
			EthereumBeaconClient::cross_check_execution_state(),
			Error::<Test>::ExecutionHeaderTooFarBehind
		);
	});
}

/* SYNC PROCESS TESTS */

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
		assert_ok!(EthereumBeaconClient::submit(RuntimeOrigin::signed(1), update.clone()));
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
		assert_ok!(EthereumBeaconClient::submit(RuntimeOrigin::signed(1), update));
		assert!(<NextSyncCommittee<Test>>::exists());
	});
}

#[test]
fn submit_update_in_next_period() {
	let checkpoint = Box::new(load_checkpoint_update_fixture());
	let sync_committee_update = Box::new(load_sync_committee_update_fixture());
	let update = Box::new(load_next_finalized_header_update_fixture());
	let sync_committee_period = compute_period(sync_committee_update.finalized_header.slot);
	let next_sync_committee_period = compute_period(update.finalized_header.slot);
	assert_eq!(sync_committee_period + 1, next_sync_committee_period);

	new_tester().execute_with(|| {
		assert_ok!(EthereumBeaconClient::process_checkpoint_update(&checkpoint));
		assert_ok!(EthereumBeaconClient::submit(
			RuntimeOrigin::signed(1),
			sync_committee_update.clone()
		));
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
		assert_err!(
			EthereumBeaconClient::submit(RuntimeOrigin::signed(1), update),
			Error::<Test>::InvalidHeaderMerkleProof
		);
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
		assert_err!(
			EthereumBeaconClient::submit(RuntimeOrigin::signed(1), update),
			Error::<Test>::InvalidBlockRootsRootMerkleProof
		);
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
		assert_err!(
			EthereumBeaconClient::submit(RuntimeOrigin::signed(1), update),
			Error::<Test>::InvalidSyncCommitteeMerkleProof
		);
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
		assert_ok!(EthereumBeaconClient::submit(
			RuntimeOrigin::signed(1),
			sync_committee_update.clone()
		));
		assert_err!(
			EthereumBeaconClient::submit(RuntimeOrigin::signed(1), update),
			Error::<Test>::SkippedSyncCommitteePeriod
		);
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
		assert_ok!(EthereumBeaconClient::submit(RuntimeOrigin::signed(1), update.clone()));
		assert!(<NextSyncCommittee<Test>>::exists());
		assert_ok!(EthereumBeaconClient::submit(RuntimeOrigin::signed(1), next_update.clone()));
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

		// makes a invalid update with signature_slot should be more than attested_slot
		update.signature_slot = update.attested_header.slot;

		assert_err!(
			EthereumBeaconClient::submit(RuntimeOrigin::signed(1), update),
			Error::<Test>::InvalidUpdateSlot
		);
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
		assert_err!(
			EthereumBeaconClient::submit(RuntimeOrigin::signed(1), finalized_update),
			Error::<Test>::SkippedSyncCommitteePeriod
		);
	});
}

#[test]
fn submit_update_execution_headers_too_far_behind() {
	let checkpoint = Box::new(load_checkpoint_update_fixture());
	let finalized_header_update = Box::new(load_finalized_header_update_fixture());
	let execution_header_update = Box::new(load_execution_header_update_fixture());
	let next_update = Box::new(load_next_sync_committee_update_fixture());

	new_tester().execute_with(|| {
		let far_ahead_finalized_header_slot = finalized_header_update.finalized_header.slot +
			(EPOCHS_PER_SYNC_COMMITTEE_PERIOD * SLOTS_PER_EPOCH * 2) as u64;
		assert_ok!(EthereumBeaconClient::process_checkpoint_update(&checkpoint));
		assert_ok!(EthereumBeaconClient::submit(RuntimeOrigin::signed(1), finalized_header_update));
		assert_ok!(EthereumBeaconClient::submit_execution_header(
			RuntimeOrigin::signed(1),
			execution_header_update
		));

		let header_root: H256 = TEST_HASH.into();
		<FinalizedBeaconState<Test>>::insert(
			header_root,
			CompactBeaconState {
				slot: far_ahead_finalized_header_slot,
				block_roots_root: Default::default(),
			},
		);
		LatestFinalizedBlockRoot::<Test>::set(header_root);

		assert_err!(
			EthereumBeaconClient::submit(RuntimeOrigin::signed(1), next_update),
			Error::<Test>::ExecutionHeaderTooFarBehind
		);
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

		assert_err!(
			EthereumBeaconClient::submit(RuntimeOrigin::signed(1), update),
			Error::<Test>::IrrelevantUpdate
		);
	});
}

#[test]
fn submit_update_with_missing_bootstrap() {
	let update = Box::new(load_next_finalized_header_update_fixture());

	new_tester().execute_with(|| {
		assert_err!(
			EthereumBeaconClient::submit(RuntimeOrigin::signed(1), update),
			Error::<Test>::NotBootstrapped
		);
	});
}

#[test]
fn submit_update_with_invalid_sync_committee_update() {
	let checkpoint = Box::new(load_checkpoint_update_fixture());
	let update = Box::new(load_sync_committee_update_fixture());
	let mut next_update = Box::new(load_next_sync_committee_update_fixture());

	new_tester().execute_with(|| {
		assert_ok!(EthereumBeaconClient::process_checkpoint_update(&checkpoint));

		assert_ok!(EthereumBeaconClient::submit(RuntimeOrigin::signed(1), update));

		// makes update with invalid next_sync_committee
		<FinalizedBeaconState<Test>>::mutate(<LatestFinalizedBlockRoot<Test>>::get(), |x| {
			let prev = x.unwrap();
			*x = Some(CompactBeaconState { slot: next_update.attested_header.slot, ..prev });
		});
		next_update.attested_header.slot += 1;
		next_update.signature_slot = next_update.attested_header.slot + 1;
		let next_sync_committee = NextSyncCommitteeUpdate::default();
		next_update.next_sync_committee_update = Some(next_sync_committee);

		assert_err!(
			EthereumBeaconClient::submit(RuntimeOrigin::signed(1), next_update),
			Error::<Test>::InvalidSyncCommitteeUpdate
		);
	});
}

#[test]
fn submit_execution_header_update() {
	let checkpoint = Box::new(load_checkpoint_update_fixture());
	let finalized_header_update = Box::new(load_finalized_header_update_fixture());
	let execution_header_update = Box::new(load_execution_header_update_fixture());

	new_tester().execute_with(|| {
		assert_ok!(EthereumBeaconClient::process_checkpoint_update(&checkpoint));
		assert_ok!(EthereumBeaconClient::submit(RuntimeOrigin::signed(1), finalized_header_update));
		assert_ok!(EthereumBeaconClient::submit_execution_header(
			RuntimeOrigin::signed(1),
			execution_header_update.clone()
		));
		assert!(<ExecutionHeaders<Test>>::contains_key(
			execution_header_update.execution_header.block_hash()
		));
	});
}

#[test]
fn submit_execution_header_update_invalid_ancestry_proof() {
	let checkpoint = Box::new(load_checkpoint_update_fixture());
	let finalized_header_update = Box::new(load_finalized_header_update_fixture());
	let mut execution_header_update = Box::new(load_execution_header_update_fixture());
	if let Some(ref mut ancestry_proof) = execution_header_update.ancestry_proof {
		ancestry_proof.header_branch[0] = TEST_HASH.into()
	}

	new_tester().execute_with(|| {
		assert_ok!(EthereumBeaconClient::process_checkpoint_update(&checkpoint));
		assert_ok!(EthereumBeaconClient::submit(RuntimeOrigin::signed(1), finalized_header_update));
		assert_err!(
			EthereumBeaconClient::submit_execution_header(
				RuntimeOrigin::signed(1),
				execution_header_update
			),
			Error::<Test>::InvalidAncestryMerkleProof
		);
	});
}

#[test]
fn submit_execution_header_update_invalid_execution_header_proof() {
	let checkpoint = Box::new(load_checkpoint_update_fixture());
	let finalized_header_update = Box::new(load_finalized_header_update_fixture());
	let mut execution_header_update = Box::new(load_execution_header_update_fixture());
	execution_header_update.execution_branch[0] = TEST_HASH.into();

	new_tester().execute_with(|| {
		assert_ok!(EthereumBeaconClient::process_checkpoint_update(&checkpoint));
		assert_ok!(EthereumBeaconClient::submit(RuntimeOrigin::signed(1), finalized_header_update));
		assert_err!(
			EthereumBeaconClient::submit_execution_header(
				RuntimeOrigin::signed(1),
				execution_header_update
			),
			Error::<Test>::InvalidExecutionHeaderProof
		);
	});
}

#[test]
fn submit_execution_header_update_that_skips_block() {
	let checkpoint = Box::new(load_checkpoint_update_fixture());
	let finalized_header_update = Box::new(load_finalized_header_update_fixture());
	let execution_header_update = Box::new(load_execution_header_update_fixture());
	let mut skipped_block_execution_header_update =
		Box::new(load_execution_header_update_fixture());
	let mut skipped_execution_header =
		skipped_block_execution_header_update.execution_header.clone();

	skipped_execution_header = match skipped_execution_header {
		VersionedExecutionPayloadHeader::Capella(execution_payload_header) => {
			let mut mut_execution_payload_header = execution_payload_header.clone();
			mut_execution_payload_header.block_number = execution_payload_header.block_number + 2;
			VersionedExecutionPayloadHeader::Capella(mut_execution_payload_header)
		},
		VersionedExecutionPayloadHeader::Deneb(execution_payload_header) => {
			let mut mut_execution_payload_header = execution_payload_header.clone();
			mut_execution_payload_header.block_number = execution_payload_header.block_number + 2;
			VersionedExecutionPayloadHeader::Deneb(mut_execution_payload_header)
		},
	};

	skipped_block_execution_header_update.execution_header = skipped_execution_header;

	new_tester().execute_with(|| {
		assert_ok!(EthereumBeaconClient::process_checkpoint_update(&checkpoint));
		assert_ok!(EthereumBeaconClient::submit(RuntimeOrigin::signed(1), finalized_header_update));
		assert_ok!(EthereumBeaconClient::submit_execution_header(
			RuntimeOrigin::signed(1),
			execution_header_update.clone()
		));
		assert!(<ExecutionHeaders<Test>>::contains_key(
			execution_header_update.execution_header.block_hash()
		));
		assert_err!(
			EthereumBeaconClient::submit_execution_header(
				RuntimeOrigin::signed(1),
				skipped_block_execution_header_update
			),
			Error::<Test>::ExecutionHeaderSkippedBlock
		);
	});
}

#[test]
fn submit_execution_header_update_that_is_also_finalized_header_which_is_not_stored() {
	let checkpoint = Box::new(load_checkpoint_update_fixture());
	let finalized_header_update = Box::new(load_finalized_header_update_fixture());
	let mut execution_header_update = Box::new(load_execution_header_update_fixture());
	execution_header_update.ancestry_proof = None;

	new_tester().execute_with(|| {
		assert_ok!(EthereumBeaconClient::process_checkpoint_update(&checkpoint));
		assert_ok!(EthereumBeaconClient::submit(RuntimeOrigin::signed(1), finalized_header_update));
		assert_err!(
			EthereumBeaconClient::submit_execution_header(
				RuntimeOrigin::signed(1),
				execution_header_update
			),
			Error::<Test>::ExpectedFinalizedHeaderNotStored
		);
	});
}

#[test]
fn submit_execution_header_update_that_is_also_finalized_header_which_is_stored_but_slots_dont_match(
) {
	let checkpoint = Box::new(load_checkpoint_update_fixture());
	let finalized_header_update = Box::new(load_finalized_header_update_fixture());
	let mut execution_header_update = Box::new(load_execution_header_update_fixture());
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
			EthereumBeaconClient::submit_execution_header(
				RuntimeOrigin::signed(1),
				execution_header_update
			),
			Error::<Test>::ExpectedFinalizedHeaderNotStored
		);
	});
}

#[test]
fn submit_execution_header_not_finalized() {
	let checkpoint = Box::new(load_checkpoint_update_fixture());
	let finalized_header_update = Box::new(load_finalized_header_update_fixture());
	let update = Box::new(load_execution_header_update_fixture());

	new_tester().execute_with(|| {
		assert_ok!(EthereumBeaconClient::process_checkpoint_update(&checkpoint));
		assert_ok!(EthereumBeaconClient::submit(RuntimeOrigin::signed(1), finalized_header_update));

		<FinalizedBeaconState<Test>>::mutate(<LatestFinalizedBlockRoot<Test>>::get(), |x| {
			let prev = x.unwrap();
			*x = Some(CompactBeaconState { slot: update.header.slot - 1, ..prev });
		});

		assert_err!(
			EthereumBeaconClient::submit_execution_header(RuntimeOrigin::signed(1), update),
			Error::<Test>::HeaderNotFinalized
		);
	});
}

/* IMPLS */

#[test]
fn verify_message() {
	let header = get_message_verification_header();
	let (event_log, proof) = get_message_verification_payload();
	let block_hash = proof.block_hash;

	new_tester().execute_with(|| {
		<ExecutionHeaderBuffer<Test>>::insert(block_hash, header);
		assert_ok!(EthereumBeaconClient::verify(&event_log, &proof));
	});
}

#[test]
fn verify_message_missing_header() {
	let (event_log, proof) = get_message_verification_payload();

	new_tester().execute_with(|| {
		assert_err!(
			EthereumBeaconClient::verify(&event_log, &proof),
			VerificationError::HeaderNotFound
		);
	});
}

#[test]
fn verify_message_invalid_proof() {
	let header = get_message_verification_header();
	let (event_log, mut proof) = get_message_verification_payload();
	proof.data.1[0] = TEST_HASH.into();
	let block_hash = proof.block_hash;

	new_tester().execute_with(|| {
		<ExecutionHeaderBuffer<Test>>::insert(block_hash, header);
		assert_err!(
			EthereumBeaconClient::verify(&event_log, &proof),
			VerificationError::InvalidProof
		);
	});
}

#[test]
fn verify_message_invalid_receipts_root() {
	let mut header = get_message_verification_header();
	let (event_log, proof) = get_message_verification_payload();
	let block_hash = proof.block_hash;
	header.receipts_root = TEST_HASH.into();

	new_tester().execute_with(|| {
		<ExecutionHeaderBuffer<Test>>::insert(block_hash, header);
		assert_err!(
			EthereumBeaconClient::verify(&event_log, &proof),
			VerificationError::InvalidProof
		);
	});
}

#[test]
fn verify_message_invalid_log() {
	let header = get_message_verification_header();
	let (mut event_log, proof) = get_message_verification_payload();
	let block_hash = proof.block_hash;
	event_log.topics = vec![H256::zero(); 10];

	new_tester().execute_with(|| {
		<ExecutionHeaderBuffer<Test>>::insert(block_hash, header);
		assert_err!(
			EthereumBeaconClient::verify(&event_log, &proof),
			VerificationError::InvalidLog
		);
	});
}

#[test]
fn verify_message_receipt_does_not_contain_log() {
	let header = get_message_verification_header();
	let (mut event_log, proof) = get_message_verification_payload();
	let block_hash = proof.block_hash;
	event_log.data = hex!("f9013c94ee9170abfbf9421ad6dd07f6bdec9d89f2b581e0f863a01b11dcf133cc240f682dab2d3a8e4cd35c5da8c9cf99adac4336f8512584c5ada000000000000000000000000000000000000000000000000000000000000003e8a00000000000000000000000000000000000000000000000000000000000000002b8c000000000000000000000000000000000000000000000000000000000000000200000000000000000000000000000000000000000000000000000000000000068000f000000000000000101d184c103f7acc340847eee82a0b909e3358bc28d440edffa1352b13227e8ee646f3ea37456dec70100000101001cbd2d43530a44705ad088af313e18f80b53ef16b36177cd4b77b846f2a5f07c0000e8890423c78a0000000000000000000000000000000000000000000000000000000000000000").to_vec();

	new_tester().execute_with(|| {
		<ExecutionHeaderBuffer<Test>>::insert(block_hash, header);
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
	let execution_header_update = Box::new(load_execution_header_update_fixture());

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

		assert_noop!(
			EthereumBeaconClient::submit_execution_header(
				RuntimeOrigin::signed(1),
				execution_header_update
			),
			Error::<Test>::Halted
		);
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
