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

//! Collator membership proofs, including padded trees and invalid membership.

use cumulus_aura_authorizer::{build_collator_tree, AuthConfig, AuthToken, CollatorKey, ParaId};
use primitive_types::H256;

fn keys(count: u32) -> Vec<CollatorKey> {
	(0..count).map(|index| [index as u8 + 1; 32]).collect()
}

fn config(keys: &[CollatorKey]) -> (AuthConfig, Vec<Vec<H256>>) {
	let (collator_set_root, proofs) = build_collator_tree(keys);
	let config = AuthConfig {
		para_ids: vec![ParaId::new(0)],
		parachain_service: 1337,
		collator_set_root,
		collator_set_size: keys.len() as u32,
		slot_duration: 1,
	};
	(config, proofs)
}

fn token(key: CollatorKey, proof: Vec<H256>) -> AuthToken {
	AuthToken { proof, key, signature: [0u8; 64] }
}

#[test]
fn built_proofs_works() {
	for size in [1u32, 2, 3, 4, 5, 15, 16, 17] {
		let keys = keys(size);
		let (config, proofs) = config(&keys);
		for index in 0..size {
			let token = token(keys[index as usize], proofs[index as usize].clone());
			assert!(
				token.check_proof(&config, index).is_ok(),
				"set of {size}: collator {index}'s own proof was rejected"
			);
		}
	}
}

#[test]
fn wrong_index_errors() {
	let keys = keys(4);
	let (config, proofs) = config(&keys);
	let token = token(keys[1], proofs[1].clone());
	assert!(token.check_proof(&config, 1).is_ok());
	for index in [0u32, 2, 3] {
		assert!(token.check_proof(&config, index).is_err(), "accepted at index {index}");
	}
}

/// A key outside the set has no proof, and neither the root nor a borrowed proof can supply one.
#[test]
fn nonmember_errors() {
	let keys = keys(2);
	let (config, proofs) = config(&keys);
	let outsider = token([0xaa; 32], proofs[0].clone());
	assert!(outsider.check_proof(&config, 0).is_err());
}

#[test]
fn malformed_proof_errors() {
	let keys = keys(3);
	let (config, proofs) = config(&keys);
	let mut token = token(keys[2], proofs[2].clone());
	token.proof.pop();
	assert!(token.check_proof(&config, 2).is_err());
	token.proof = proofs[2].clone();
	token.proof.push(H256::zero());
	assert!(token.check_proof(&config, 2).is_err());
	token.proof = proofs[2].clone();
	token.proof[0] = H256::repeat_byte(1);
	assert!(token.check_proof(&config, 2).is_err());
}

#[test]
fn invalid_set_size_and_index_errors() {
	let keys = keys(1);
	let (mut config, proofs) = config(&keys);
	let token = token(keys[0], proofs[0].clone());
	assert!(token.check_proof(&config, 1).is_err());
	config.collator_set_size = 0;
	assert!(token.check_proof(&config, 0).is_err());
	config.collator_set_size = u32::MAX;
	let token = AuthToken { proof: vec![H256::zero(); 32], ..token };
	assert!(token.check_proof(&config, 0).is_err());
}
