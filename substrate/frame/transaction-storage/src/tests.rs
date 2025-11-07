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

//! Tests for transaction-storage pallet.

use frame_support::traits::IntoWithBasicFilter;
use super::{Pallet as TransactionStorage, *};
use crate::mock::*;
use frame_support::{assert_noop, assert_ok};
use frame_system::RawOrigin;
use sp_runtime::{DispatchError, TokenError::FundsUnavailable};
use sp_transaction_storage_proof::{registration::build_proof, CHUNK_SIZE};

const MAX_DATA_SIZE: u32 = DEFAULT_MAX_TRANSACTION_SIZE;

#[test]
fn discards_data() {
	new_test_ext().execute_with(|| {
		run_to_block(1, || None);
		let caller = 1;
		assert_ok!(TransactionStorage::<Test>::store(
			RawOrigin::Signed(caller).into_with_basic_filter(),
			vec![0u8; 2000 as usize]
		));
		assert_ok!(TransactionStorage::<Test>::store(
			RawOrigin::Signed(caller).into_with_basic_filter(),
			vec![0u8; 2000 as usize]
		));
		let proof_provider = || {
			let block_num = frame_system::Pallet::<Test>::block_number();
			if block_num == 11 {
				let parent_hash = frame_system::Pallet::<Test>::parent_hash();
				build_proof(parent_hash.as_ref(), vec![vec![0u8; 2000], vec![0u8; 2000]]).unwrap()
			} else {
				None
			}
		};
		run_to_block(11, proof_provider);
		assert!(Transactions::<Test>::get(1).is_some());
		let transactions = Transactions::<Test>::get(1).unwrap();
		assert_eq!(transactions.len(), 2);
		assert_eq!(TransactionInfo::total_chunks(&transactions), 16);
		run_to_block(12, proof_provider);
		assert!(Transactions::<Test>::get(1).is_none());
	});
}

#[test]
fn burns_fee() {
	new_test_ext().execute_with(|| {
		run_to_block(1, || None);
		let caller = 1;
		assert_noop!(
			TransactionStorage::<Test>::store(
				RawOrigin::Signed(5).into_with_basic_filter(),
				vec![0u8; 2000 as usize]
			),
			DispatchError::Token(FundsUnavailable),
		);
		assert_ok!(TransactionStorage::<Test>::store(
			RawOrigin::Signed(caller).into_with_basic_filter(),
			vec![0u8; 2000 as usize]
		));
		assert_eq!(Balances::free_balance(1), 1_000_000_000 - 2000 * 2 - 200);
	});
}

#[test]
fn checks_proof() {
	new_test_ext().execute_with(|| {
		run_to_block(1, || None);
		let caller = 1;
		assert_ok!(TransactionStorage::<Test>::store(
			RawOrigin::Signed(caller).into_with_basic_filter(),
			vec![0u8; MAX_DATA_SIZE as usize]
		));
		run_to_block(10, || None);
		let parent_hash = frame_system::Pallet::<Test>::parent_hash();
		let proof = build_proof(parent_hash.as_ref(), vec![vec![0u8; MAX_DATA_SIZE as usize]])
			.unwrap()
			.unwrap();
		assert_noop!(
			TransactionStorage::<Test>::check_proof(RuntimeOrigin::none_with_basic_filter(), proof,),
			Error::<Test>::UnexpectedProof,
		);
		run_to_block(11, || None);
		let parent_hash = frame_system::Pallet::<Test>::parent_hash();

		let invalid_proof =
			build_proof(parent_hash.as_ref(), vec![vec![0u8; 1000]]).unwrap().unwrap();
		assert_noop!(
			TransactionStorage::<Test>::check_proof(RuntimeOrigin::none_with_basic_filter(), invalid_proof,),
			Error::<Test>::InvalidProof,
		);

		let proof = build_proof(parent_hash.as_ref(), vec![vec![0u8; MAX_DATA_SIZE as usize]])
			.unwrap()
			.unwrap();
		assert_ok!(TransactionStorage::<Test>::check_proof(RuntimeOrigin::none_with_basic_filter(), proof));
	});
}

#[test]
fn verify_chunk_proof_works() {
	new_test_ext().execute_with(|| {
		// Prepare a bunch of transactions with variable chunk sizes.
		let transactions = vec![
			vec![0u8; CHUNK_SIZE - 1],
			vec![1u8; CHUNK_SIZE],
			vec![2u8; CHUNK_SIZE + 1],
			vec![3u8; 2 * CHUNK_SIZE - 1],
			vec![3u8; 2 * CHUNK_SIZE],
			vec![3u8; 2 * CHUNK_SIZE + 1],
			vec![4u8; 7 * CHUNK_SIZE - 1],
			vec![4u8; 7 * CHUNK_SIZE],
			vec![4u8; 7 * CHUNK_SIZE + 1],
		];
		let expected_total_chunks =
			transactions.iter().map(|t| t.len().div_ceil(CHUNK_SIZE) as u32).sum::<u32>();

		// Store a couple of transactions in one block.
		run_to_block(1, || None);
		let caller = 1;
		for transaction in transactions.clone() {
			assert_ok!(TransactionStorage::<Test>::store(
				RawOrigin::Signed(caller).into_with_basic_filter(),
				transaction
			));
		}
		run_to_block(2, || None);

		// Read all the block transactions metadata.
		let tx_infos = Transactions::<Test>::get(1).unwrap();
		let total_chunks = TransactionInfo::total_chunks(&tx_infos);
		assert_eq!(expected_total_chunks, total_chunks);
		assert_eq!(9, tx_infos.len());

		// Verify proofs for all possible chunk indexes.
		for chunk_index in 0..total_chunks {
			// chunk index randomness
			let mut random_hash = [0u8; 32];
			random_hash[..8].copy_from_slice(&(chunk_index as u64).to_be_bytes());
			let selected_chunk_index = random_chunk(random_hash.as_ref(), total_chunks);
			assert_eq!(selected_chunk_index, chunk_index);

			// build/check chunk proof roundtrip
			let proof = build_proof(random_hash.as_ref(), transactions.clone())
				.expect("valid proof")
				.unwrap();
			assert_ok!(TransactionStorage::<Test>::verify_chunk_proof(
				proof,
				random_hash.as_ref(),
				tx_infos.to_vec()
			));
		}
	});
}

#[test]
fn renews_data() {
	new_test_ext().execute_with(|| {
		run_to_block(1, || None);
		let caller = 1;
		assert_noop!(
			TransactionStorage::<Test>::store(RawOrigin::Signed(caller).into_with_basic_filter(), vec![]),
			Error::<Test>::EmptyTransaction
		);
		assert_ok!(TransactionStorage::<Test>::store(
			RawOrigin::Signed(caller).into_with_basic_filter(),
			vec![0u8; 2000]
		));
		let info = BlockTransactions::<Test>::get().last().unwrap().clone();
		run_to_block(6, || None);
		assert_ok!(TransactionStorage::<Test>::renew(
			RawOrigin::Signed(caller).into_with_basic_filter(),
			1, // block
			0, // transaction
		));
		assert_eq!(Balances::free_balance(1), 1_000_000_000 - 4000 * 2 - 200 * 2);
		let proof_provider = || {
			let block_num = frame_system::Pallet::<Test>::block_number();
			if block_num == 11 || block_num == 16 {
				let parent_hash = frame_system::Pallet::<Test>::parent_hash();
				build_proof(parent_hash.as_ref(), vec![vec![0u8; 2000]]).unwrap()
			} else {
				None
			}
		};
		run_to_block(16, proof_provider);
		assert!(Transactions::<Test>::get(1).is_none());
		assert_eq!(Transactions::<Test>::get(6).unwrap().get(0), Some(info).as_ref());
		run_to_block(17, proof_provider);
		assert!(Transactions::<Test>::get(6).is_none());
	});
}
