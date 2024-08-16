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

use std::vec;

use codec::{Decode, Encode};
use sp_consensus_beefy::{
	known_payloads,
	mmr::{BeefyNextAuthoritySet, MmrLeafVersion},
	AncestryHelper, Commitment, Payload, ValidatorSet,
};

use sp_core::H256;
use sp_io::TestExternalities;
use sp_runtime::{traits::Keccak256, DigestItem};

use frame_support::traits::OnInitialize;

use crate::mock::*;

fn init_block(block: u64, maybe_parent_hash: Option<H256>) {
	let parent_hash = maybe_parent_hash.unwrap_or(H256::repeat_byte(block as u8));
	System::initialize(&block, &parent_hash, &Default::default());
	Session::on_initialize(block);
	Mmr::on_initialize(block);
}

pub fn beefy_log(log: ConsensusLog<BeefyId>) -> DigestItem {
	DigestItem::Consensus(BEEFY_ENGINE_ID, log.encode())
}

fn read_mmr_leaf(ext: &mut TestExternalities, key: Vec<u8>) -> MmrLeaf {
	type Node = pallet_mmr::primitives::DataOrHash<Keccak256, MmrLeaf>;
	ext.persist_offchain_overlay();
	let offchain_db = ext.offchain_db();
	offchain_db
		.get(&key)
		.map(|d| Node::decode(&mut &*d).unwrap())
		.map(|n| match n {
			Node::Data(d) => d,
			_ => panic!("Unexpected MMR node."),
		})
		.unwrap()
}

#[test]
fn should_contain_mmr_digest() {
	let mut ext = new_test_ext(vec![1, 2, 3, 4]);
	ext.execute_with(|| {
		init_block(1, None);
		assert_eq!(
			System::digest().logs,
			vec![
				beefy_log(ConsensusLog::AuthoritiesChange(
					ValidatorSet::new(vec![mock_beefy_id(1), mock_beefy_id(2)], 1).unwrap()
				)),
				beefy_log(ConsensusLog::MmrRoot(H256::from_slice(&[
					117, 0, 56, 25, 185, 195, 71, 232, 67, 213, 27, 178, 64, 168, 137, 220, 64,
					184, 64, 240, 83, 245, 18, 93, 185, 202, 125, 205, 17, 254, 18, 143
				])))
			]
		);

		// unique every time
		init_block(2, None);
		assert_eq!(
			System::digest().logs,
			vec![
				beefy_log(ConsensusLog::AuthoritiesChange(
					ValidatorSet::new(vec![mock_beefy_id(3), mock_beefy_id(4)], 2).unwrap()
				)),
				beefy_log(ConsensusLog::MmrRoot(H256::from_slice(&[
					193, 246, 48, 7, 89, 204, 186, 109, 167, 226, 188, 211, 8, 243, 203, 154, 234,
					235, 136, 210, 245, 7, 209, 27, 241, 90, 156, 113, 137, 65, 191, 139
				]))),
			]
		);
	});
}

#[test]
fn should_contain_valid_leaf_data() {
	fn node_offchain_key(pos: usize, parent_hash: H256) -> Vec<u8> {
		(<Test as pallet_mmr::Config>::INDEXING_PREFIX, pos as u64, parent_hash).encode()
	}

	let mut ext = new_test_ext(vec![1, 2, 3, 4]);
	let parent_hash = ext.execute_with(|| {
		init_block(1, None);
		frame_system::Pallet::<Test>::parent_hash()
	});

	let mmr_leaf = read_mmr_leaf(&mut ext, node_offchain_key(0, parent_hash));
	assert_eq!(
		mmr_leaf,
		MmrLeaf {
			version: MmrLeafVersion::new(1, 5),
			parent_number_and_hash: (0_u64, H256::repeat_byte(1)),
			beefy_next_authority_set: BeefyNextAuthoritySet {
				id: 2,
				len: 2,
				keyset_commitment: array_bytes::hex_n_into_unchecked(
					"9c6b2c1b0d0b25a008e6c882cc7b415f309965c72ad2b944ac0931048ca31cd5"
				)
			},
			leaf_extra: array_bytes::hex2bytes_unchecked(
				"55b8e9e1cc9f0db7776fac0ca66318ef8acfb8ec26db11e373120583e07ee648"
			)
		}
	);

	// build second block on top
	let parent_hash = ext.execute_with(|| {
		init_block(2, None);
		frame_system::Pallet::<Test>::parent_hash()
	});

	let mmr_leaf = read_mmr_leaf(&mut ext, node_offchain_key(1, parent_hash));
	assert_eq!(
		mmr_leaf,
		MmrLeaf {
			version: MmrLeafVersion::new(1, 5),
			parent_number_and_hash: (1_u64, H256::repeat_byte(2)),
			beefy_next_authority_set: BeefyNextAuthoritySet {
				id: 3,
				len: 2,
				keyset_commitment: array_bytes::hex_n_into_unchecked(
					"9c6b2c1b0d0b25a008e6c882cc7b415f309965c72ad2b944ac0931048ca31cd5"
				)
			},
			leaf_extra: array_bytes::hex2bytes_unchecked(
				"55b8e9e1cc9f0db7776fac0ca66318ef8acfb8ec26db11e373120583e07ee648"
			)
		}
	);
}

#[test]
fn should_update_authorities() {
	new_test_ext(vec![1, 2, 3, 4]).execute_with(|| {
		let auth_set = BeefyMmr::authority_set_proof();
		let next_auth_set = BeefyMmr::next_authority_set_proof();

		// check current authority set
		assert_eq!(0, auth_set.id);
		assert_eq!(2, auth_set.len);
		let want = array_bytes::hex_n_into_unchecked::<_, H256, 32>(
			"176e73f1bf656478b728e28dd1a7733c98621b8acf830bff585949763dca7a96",
		);
		assert_eq!(want, auth_set.keyset_commitment);

		// next authority set should have same validators but different id
		assert_eq!(1, next_auth_set.id);
		assert_eq!(auth_set.len, next_auth_set.len);
		assert_eq!(auth_set.keyset_commitment, next_auth_set.keyset_commitment);

		let announced_set = next_auth_set;
		init_block(1, None);
		let auth_set = BeefyMmr::authority_set_proof();
		let next_auth_set = BeefyMmr::next_authority_set_proof();

		// check new auth are expected ones
		assert_eq!(announced_set, auth_set);
		assert_eq!(1, auth_set.id);
		// check next auth set
		assert_eq!(2, next_auth_set.id);
		let want = array_bytes::hex_n_into_unchecked::<_, H256, 32>(
			"9c6b2c1b0d0b25a008e6c882cc7b415f309965c72ad2b944ac0931048ca31cd5",
		);
		assert_eq!(2, next_auth_set.len);
		assert_eq!(want, next_auth_set.keyset_commitment);

		let announced_set = next_auth_set;
		init_block(2, None);
		let auth_set = BeefyMmr::authority_set_proof();
		let next_auth_set = BeefyMmr::next_authority_set_proof();

		// check new auth are expected ones
		assert_eq!(announced_set, auth_set);
		assert_eq!(2, auth_set.id);
		// check next auth set
		assert_eq!(3, next_auth_set.id);
		let want = array_bytes::hex_n_into_unchecked::<_, H256, 32>(
			"9c6b2c1b0d0b25a008e6c882cc7b415f309965c72ad2b944ac0931048ca31cd5",
		);
		assert_eq!(2, next_auth_set.len);
		assert_eq!(want, next_auth_set.keyset_commitment);
	});
}

#[test]
fn extract_validation_context_should_work_correctly() {
	let mut ext = new_test_ext(vec![1, 2]);

	ext.execute_with(|| {
		init_block(1, None);
		let h1 = System::finalize();
		init_block(2, Some(h1.hash()));
		let h2 = System::finalize();

		// Check the MMR root log
		let expected_mmr_root: [u8; 32] = array_bytes::hex_n_into_unchecked(
			"b2106eff9894288bc212b3a9389caa54efd37962c3a7b71b3b0b06a0911b88a5",
		);
		assert_eq!(
			System::digest().logs,
			vec![beefy_log(ConsensusLog::MmrRoot(H256::from_slice(&expected_mmr_root)))]
		);

		// Make sure that all the info about h2 was stored on-chain
		init_block(3, Some(h2.hash()));

		// `extract_validation_context` should return the MMR root when the provided header
		// is part of the chain,
		assert_eq!(
			BeefyMmr::extract_validation_context(h2.clone()),
			Some(H256::from_slice(&expected_mmr_root))
		);

		// `extract_validation_context` should return `None` when the provided header
		// is not part of the chain.
		let mut fork_h2 = h2;
		fork_h2.state_root = H256::repeat_byte(0);
		assert_eq!(BeefyMmr::extract_validation_context(fork_h2), None);
	});
}

#[test]
fn is_non_canonical_should_work_correctly() {
	let mut ext = new_test_ext(vec![1, 2]);

	let mut prev_roots = vec![];
	ext.execute_with(|| {
		for block_num in 1..=500 {
			init_block(block_num, None);
			prev_roots.push(Mmr::mmr_root())
		}
	});
	ext.persist_offchain_overlay();

	ext.execute_with(|| {
		let valid_proof = BeefyMmr::generate_proof(250, None).unwrap();
		let mut invalid_proof = valid_proof.clone();
		invalid_proof.items.push((300, Default::default()));

		// The commitment is invalid if it has no MMR root payload and the proof is valid.
		assert_eq!(
			BeefyMmr::is_non_canonical(
				&Commitment {
					payload: Payload::from_single_entry([0, 0], vec![]),
					block_number: 250,
					validator_set_id: 0
				},
				valid_proof.clone(),
				Mmr::mmr_root(),
			),
			true
		);

		// If the `commitment.payload` contains an MMR root that doesn't match the ancestry proof,
		// it's non-canonical.
		assert_eq!(
			BeefyMmr::is_non_canonical(
				&Commitment {
					payload: Payload::from_single_entry(
						known_payloads::MMR_ROOT_ID,
						H256::repeat_byte(0).encode(),
					),
					block_number: 250,
					validator_set_id: 0,
				},
				valid_proof.clone(),
				Mmr::mmr_root(),
			),
			true
		);

		// Should return false if the proof is invalid, no matter the payload.
		assert_eq!(
			BeefyMmr::is_non_canonical(
				&Commitment {
					payload: Payload::from_single_entry(
						known_payloads::MMR_ROOT_ID,
						H256::repeat_byte(0).encode(),
					),
					block_number: 250,
					validator_set_id: 0
				},
				invalid_proof,
				Mmr::mmr_root(),
			),
			false
		);

		// Can't prove that the commitment is non-canonical if the `commitment.block_number`
		// doesn't match the ancestry proof.
		assert_eq!(
			BeefyMmr::is_non_canonical(
				&Commitment {
					payload: Payload::from_single_entry(
						known_payloads::MMR_ROOT_ID,
						prev_roots[250 - 1].encode(),
					),
					block_number: 300,
					validator_set_id: 0,
				},
				valid_proof,
				Mmr::mmr_root(),
			),
			false
		);

		// For each previous block, the check:
		// - should return false, if the commitment is targeting the canonical chain
		// - should return true if the commitment is NOT targeting the canonical chain
		for prev_block_number in 1usize..=500 {
			let proof = BeefyMmr::generate_proof(prev_block_number as u64, None).unwrap();

			assert_eq!(
				BeefyMmr::is_non_canonical(
					&Commitment {
						payload: Payload::from_single_entry(
							known_payloads::MMR_ROOT_ID,
							prev_roots[prev_block_number - 1].encode(),
						),
						block_number: prev_block_number as u64,
						validator_set_id: 0,
					},
					proof.clone(),
					Mmr::mmr_root(),
				),
				false
			);

			assert_eq!(
				BeefyMmr::is_non_canonical(
					&Commitment {
						payload: Payload::from_single_entry(
							known_payloads::MMR_ROOT_ID,
							H256::repeat_byte(0).encode(),
						),
						block_number: prev_block_number as u64,
						validator_set_id: 0,
					},
					proof,
					Mmr::mmr_root(),
				),
				true
			)
		}
	});
}
