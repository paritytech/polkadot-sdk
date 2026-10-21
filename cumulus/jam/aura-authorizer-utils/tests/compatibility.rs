// This file is part of Cumulus.

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

//! Fixed protocol vectors for SCALE encoding and the zero-padded Merkle tree.

use codec::Encode;
use cumulus_aura_authorizer_utils::{build_collator_proof, AuthConfig, AuthToken, AuthTrace, ParaId};
use primitive_types::H256;

#[test]
fn padded_tree_vector_works() {
	let (root, proof) =
		build_collator_proof(&[[1; 32], [2; 32], [3; 32]], 2).expect("valid test collator set");

	assert_eq!(
		root,
		H256::from([
			85, 243, 163, 207, 64, 253, 63, 18, 253, 224, 77, 156, 237, 59, 240, 105, 126, 129,
			244, 235, 59, 193, 124, 86, 112, 220, 83, 111, 50, 97, 27, 205
		])
	);

	assert_eq!(
		proof,
		vec![
			H256::zero(),
			H256::from([
				180, 31, 7, 124, 161, 36, 245, 239, 83, 83, 112, 218, 244, 28, 116, 162, 188, 72,
				186, 126, 184, 104, 231, 51, 124, 193, 247, 244, 171, 255, 28, 174
			])
		]
	);
}

#[test]
fn scale_wire_vectors_works() {
	let config = AuthConfig {
		para_ids: vec![ParaId::new(1000)],
		parachain_service: 1337,
		collator_set_root: H256::zero(),
		collator_set_size: core::num::NonZero::new(3).expect("nonzero configuration"),
		slot_duration: core::num::NonZero::new(2).expect("nonzero configuration"),
	};

	let mut expected = vec![4, 232, 3, 0, 0, 57, 5, 0, 0];
	expected.extend_from_slice(&[0; 32]);
	expected.extend_from_slice(&[3, 0, 0, 0, 2, 0, 0, 0]);
	assert_eq!(config.encode(), expected);

	let token = AuthToken { proof: vec![H256::repeat_byte(7)], key: [8; 32], signature: [9; 64] };
	let mut expected = vec![4];
	expected.extend_from_slice(&[7; 32]);
	expected.extend_from_slice(&[8; 32]);
	expected.extend_from_slice(&[9; 64]);
	assert_eq!(token.encode(), expected);
	assert_eq!(AuthTrace { author_key: [8; 32] }.encode(), vec![8; 32]);
}
