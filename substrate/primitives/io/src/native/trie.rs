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

//! Native PolkaVM/JAM implementations of the `trie` interface.

use crate::*;
use alloc::vec::Vec;
use hash256_std_hasher::Hash256StdHasher;
use hash_db::Hasher;
use sp_core::{storage::StateVersion, H256};
use sp_trie::{LayoutV0, LayoutV1, TrieConfiguration};
/// Blake2b-256 hasher used by the trie layouts.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Blake2Hasher;

impl Hasher for Blake2Hasher {
	type Out = H256;
	type StdHasher = Hash256StdHasher;
	const LENGTH: usize = 32;

	fn hash(x: &[u8]) -> Self::Out {
		sp_crypto_hashing::blake2_256(x).into()
	}
}

/// Keccak-256 hasher used by the trie layouts.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct KeccakHasher;

impl Hasher for KeccakHasher {
	type Out = H256;
	type StdHasher = Hash256StdHasher;
	const LENGTH: usize = 32;

	fn hash(x: &[u8]) -> Self::Out {
		sp_crypto_hashing::keccak_256(x).into()
	}
}

/// Native PolkaVM/JAM implementation of `blake2_256_ordered_root__raw`.
pub fn blake2_256_ordered_root__raw(input: Vec<Vec<u8>>, version: StateVersion, out: &mut H256) {
	let root = match version {
		StateVersion::V0 => LayoutV0::<Blake2Hasher>::ordered_trie_root(input),
		StateVersion::V1 => LayoutV1::<Blake2Hasher>::ordered_trie_root(input),
	};
	out.0.copy_from_slice(&root.0);
}

/// Native PolkaVM/JAM implementation of `blake2_256_root__raw`.
pub fn blake2_256_root__raw(input: Vec<(Vec<u8>, Vec<u8>)>, version: StateVersion, out: &mut H256) {
	let root = match version {
		StateVersion::V0 => LayoutV0::<Blake2Hasher>::trie_root(input),
		StateVersion::V1 => LayoutV1::<Blake2Hasher>::trie_root(input),
	};
	out.0.copy_from_slice(&root.0);
}

/// Native PolkaVM/JAM implementation of `blake2_256_verify_proof`.
pub fn blake2_256_verify_proof(
	root: H256,
	proof: &[Vec<u8>],
	key: &[u8],
	value: &[u8],
	version: StateVersion,
) -> bool {
	match version {
		StateVersion::V0 => sp_trie::verify_trie_proof::<LayoutV0<Blake2Hasher>, _, _, _>(
			&root,
			proof,
			&[(key, Some(value))],
		)
		.is_ok(),
		StateVersion::V1 => sp_trie::verify_trie_proof::<LayoutV1<Blake2Hasher>, _, _, _>(
			&root,
			proof,
			&[(key, Some(value))],
		)
		.is_ok(),
	}
}

/// Native PolkaVM/JAM implementation of `keccak_256_ordered_root__raw`.
pub fn keccak_256_ordered_root__raw(input: Vec<Vec<u8>>, version: StateVersion, out: &mut H256) {
	let root = match version {
		StateVersion::V0 => LayoutV0::<KeccakHasher>::ordered_trie_root(input),
		StateVersion::V1 => LayoutV1::<KeccakHasher>::ordered_trie_root(input),
	};
	out.0.copy_from_slice(&root.0);
}

/// Native PolkaVM/JAM implementation of `keccak_256_root__raw`.
pub fn keccak_256_root__raw(input: Vec<(Vec<u8>, Vec<u8>)>, version: StateVersion, out: &mut H256) {
	let root = match version {
		StateVersion::V0 => LayoutV0::<KeccakHasher>::trie_root(input),
		StateVersion::V1 => LayoutV1::<KeccakHasher>::trie_root(input),
	};
	out.0.copy_from_slice(&root.0);
}

/// Native PolkaVM/JAM implementation of `keccak_256_verify_proof`.
pub fn keccak_256_verify_proof(
	root: H256,
	proof: &[Vec<u8>],
	key: &[u8],
	value: &[u8],
	version: StateVersion,
) -> bool {
	match version {
		StateVersion::V0 => sp_trie::verify_trie_proof::<LayoutV0<KeccakHasher>, _, _, _>(
			&root,
			proof,
			&[(key, Some(value))],
		)
		.is_ok(),
		StateVersion::V1 => sp_trie::verify_trie_proof::<LayoutV1<KeccakHasher>, _, _, _>(
			&root,
			proof,
			&[(key, Some(value))],
		)
		.is_ok(),
	}
}

/// Native PolkaVM/JAM implementation of `blake2_256_root`.
pub fn blake2_256_root(data: Vec<(Vec<u8>, Vec<u8>)>, state_version: StateVersion) -> H256 {
	let mut root = H256::default();
	blake2_256_root__raw(data, state_version, &mut root);
	root
}

/// Native PolkaVM/JAM implementation of `blake2_256_ordered_root`.
pub fn blake2_256_ordered_root(data: Vec<Vec<u8>>, state_version: StateVersion) -> H256 {
	let mut root = H256::default();
	blake2_256_ordered_root__raw(data, state_version, &mut root);
	root
}

/// Native PolkaVM/JAM implementation of `keccak_256_root`.
pub fn keccak_256_root(data: Vec<(Vec<u8>, Vec<u8>)>, state_version: StateVersion) -> H256 {
	let mut root = H256::default();
	keccak_256_root__raw(data, state_version, &mut root);
	root
}

/// Native PolkaVM/JAM implementation of `keccak_256_ordered_root`.
pub fn keccak_256_ordered_root(data: Vec<Vec<u8>>, state_version: StateVersion) -> H256 {
	let mut root = H256::default();
	keccak_256_ordered_root__raw(data, state_version, &mut root);
	root
}
