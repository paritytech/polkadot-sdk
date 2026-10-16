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

//! In-guest trie operations, delegating to `sp_trie`.
//!
//! JAM does not provide trie host calls; roots and proof checks run inside the runtime. The
//! public functions keep the exact signatures of `crate::relay_chain::trie` (newest version
//! each).

use alloc::vec::Vec;
use hash256_std_hasher::Hash256StdHasher;
use hash_db::Hasher;
use sp_core::{storage::StateVersion, H256};
use sp_trie::{LayoutV0, LayoutV1, TrieConfiguration};

/// A concrete `Hasher` implementation using Blake2b 256-bit hashes.
///
/// `Blake2Hasher` is gated behind `not(substrate_runtime)`, so the in-guest trie code
/// needs its own copy (the hash is the same `sp_crypto_hashing::blake2_256`).
#[derive(Debug, Clone)]
pub struct Blake2Hasher;

impl Hasher for Blake2Hasher {
	type Out = H256;
	type StdHasher = Hash256StdHasher;
	const LENGTH: usize = 32;

	fn hash(x: &[u8]) -> Self::Out {
		sp_crypto_hashing::blake2_256(x).into()
	}
}

/// A concrete `Hasher` implementation using Keccak 256-bit hashes.
///
/// `KeccakHasher` is gated behind `not(substrate_runtime)`, so the in-guest trie code
/// needs its own copy (the hash is the same `sp_crypto_hashing::keccak_256`).
#[derive(Debug, Clone)]
pub struct KeccakHasher;

impl Hasher for KeccakHasher {
	type Out = H256;
	type StdHasher = Hash256StdHasher;
	const LENGTH: usize = 32;

	fn hash(x: &[u8]) -> Self::Out {
		sp_crypto_hashing::keccak_256(x).into()
	}
}

/// A trie root formed from the iterated items.
pub fn blake2_256_root(input: Vec<(Vec<u8>, Vec<u8>)>, version: StateVersion) -> H256 {
	match version {
		StateVersion::V0 => LayoutV0::<Blake2Hasher>::trie_root(input),
		StateVersion::V1 => LayoutV1::<Blake2Hasher>::trie_root(input),
	}
}

/// A trie root formed from the enumerated items.
pub fn blake2_256_ordered_root(input: Vec<Vec<u8>>, version: StateVersion) -> H256 {
	match version {
		StateVersion::V0 => LayoutV0::<Blake2Hasher>::ordered_trie_root(input),
		StateVersion::V1 => LayoutV1::<Blake2Hasher>::ordered_trie_root(input),
	}
}

/// A trie root formed from the iterated items.
pub fn keccak_256_root(input: Vec<(Vec<u8>, Vec<u8>)>, version: StateVersion) -> H256 {
	match version {
		StateVersion::V0 => LayoutV0::<KeccakHasher>::trie_root(input),
		StateVersion::V1 => LayoutV1::<KeccakHasher>::trie_root(input),
	}
}

/// A trie root formed from the enumerated items.
pub fn keccak_256_ordered_root(input: Vec<Vec<u8>>, version: StateVersion) -> H256 {
	match version {
		StateVersion::V0 => LayoutV0::<KeccakHasher>::ordered_trie_root(input),
		StateVersion::V1 => LayoutV1::<KeccakHasher>::ordered_trie_root(input),
	}
}

/// Verify trie proof
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

/// Verify trie proof
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

#[cfg(test)]
mod tests {
	use super::*;
	use sp_trie::{MemoryDB, TrieMut};

	fn input() -> Vec<(Vec<u8>, Vec<u8>)> {
		vec![
			(b"alpha".to_vec(), b"one".to_vec()),
			(b"beta".to_vec(), b"two".to_vec()),
			(b"gamma".to_vec(), vec![42u8; 40]),
			(b"delta".to_vec(), Vec::new()),
		]
	}

	#[test]
	fn blake2_256_root_matches_sp_trie() {
		let input = input();
		assert_eq!(
			blake2_256_root(input.clone(), StateVersion::V0),
			LayoutV0::<Blake2Hasher>::trie_root(input.clone()),
		);
		assert_eq!(
			blake2_256_root(input.clone(), StateVersion::V1),
			LayoutV1::<Blake2Hasher>::trie_root(input),
		);
	}

	#[test]
	fn blake2_256_ordered_root_matches_sp_trie() {
		let values: Vec<Vec<u8>> = input().into_iter().map(|(_, v)| v).collect();
		assert_eq!(
			blake2_256_ordered_root(values.clone(), StateVersion::V0),
			LayoutV0::<Blake2Hasher>::ordered_trie_root(values.clone()),
		);
		assert_eq!(
			blake2_256_ordered_root(values.clone(), StateVersion::V1),
			LayoutV1::<Blake2Hasher>::ordered_trie_root(values),
		);
	}

	#[test]
	fn keccak_roots_match_sp_trie() {
		let input = input();
		let values: Vec<Vec<u8>> = input.iter().map(|(_, v)| v.clone()).collect();
		assert_eq!(
			keccak_256_root(input.clone(), StateVersion::V0),
			LayoutV0::<KeccakHasher>::trie_root(input.clone()),
		);
		assert_eq!(
			keccak_256_root(input, StateVersion::V1),
			LayoutV1::<KeccakHasher>::trie_root(
				[
					(b"alpha".to_vec(), b"one".to_vec()),
					(b"beta".to_vec(), b"two".to_vec()),
					(b"gamma".to_vec(), vec![42u8; 40]),
					(b"delta".to_vec(), Vec::new()),
				]
				.to_vec(),
			),
		);
		assert_eq!(
			keccak_256_ordered_root(values.clone(), StateVersion::V0),
			LayoutV0::<KeccakHasher>::ordered_trie_root(values.clone()),
		);
		assert_eq!(
			keccak_256_ordered_root(values.clone(), StateVersion::V1),
			LayoutV1::<KeccakHasher>::ordered_trie_root(values),
		);
	}

	/// Build a trie in a `MemoryDB` and generate a proof for `key`, returning root and proof.
	fn blake2_trie_and_proof(version: StateVersion) -> (H256, Vec<Vec<u8>>) {
		let mut db = MemoryDB::<Blake2Hasher>::default();
		let mut root = H256::default();
		match version {
			StateVersion::V0 => {
				let mut trie =
					sp_trie::trie_types::TrieDBMutBuilderV0::new(&mut db, &mut root).build();
				for (k, v) in input() {
					trie.insert(&k, &v).expect("insert into a fresh in-memory trie works; qed");
				}
			},
			StateVersion::V1 => {
				let mut trie =
					sp_trie::trie_types::TrieDBMutBuilderV1::new(&mut db, &mut root).build();
				for (k, v) in input() {
					trie.insert(&k, &v).expect("insert into a fresh in-memory trie works; qed");
				}
			},
		}
		let proof = match version {
			StateVersion::V0 => sp_trie::generate_trie_proof::<LayoutV0<Blake2Hasher>, _, _, _>(
				&db,
				root,
				[b"beta"],
			)
			.expect("proof generation for an existing key works; qed"),
			StateVersion::V1 => sp_trie::generate_trie_proof::<LayoutV1<Blake2Hasher>, _, _, _>(
				&db,
				root,
				[b"beta"],
			)
			.expect("proof generation for an existing key works; qed"),
		};
		(root, proof)
	}

	#[test]
	fn blake2_256_verify_proof_accepts_valid_proof() {
		for version in [StateVersion::V0, StateVersion::V1] {
			let (root, proof) = blake2_trie_and_proof(version);
			assert!(blake2_256_verify_proof(root, &proof, b"beta", b"two", version));
		}
	}

	#[test]
	fn blake2_256_verify_proof_rejects_tampered_value() {
		for version in [StateVersion::V0, StateVersion::V1] {
			let (root, proof) = blake2_trie_and_proof(version);
			assert!(!blake2_256_verify_proof(root, &proof, b"beta", b"tampered", version));
		}
	}

	#[test]
	fn blake2_256_verify_proof_rejects_foreign_root() {
		// A proof built for one root must not verify against another root.
		let (_root, proof) = blake2_trie_and_proof(StateVersion::V0);
		let other_root =
			blake2_256_root(vec![(b"alpha".to_vec(), b"changed".to_vec())], StateVersion::V0);
		assert!(!blake2_256_verify_proof(other_root, &proof, b"beta", b"two", StateVersion::V0));

		let (_root, proof) = blake2_trie_and_proof(StateVersion::V1);
		let other_root =
			blake2_256_root(vec![(b"alpha".to_vec(), b"changed".to_vec())], StateVersion::V1);
		assert!(!blake2_256_verify_proof(other_root, &proof, b"beta", b"two", StateVersion::V1));
	}

	#[test]
	fn keccak_256_verify_proof_accepts_valid_proof() {
		let mut db = MemoryDB::<KeccakHasher>::default();
		let mut root = H256::default();
		{
			let mut trie = sp_trie::trie_types::TrieDBMutBuilderV0::new(&mut db, &mut root).build();
			for (k, v) in input() {
				trie.insert(&k, &v).expect("insert into a fresh in-memory trie works; qed");
			}
		}
		let proof =
			sp_trie::generate_trie_proof::<LayoutV0<KeccakHasher>, _, _, _>(&db, root, [b"beta"])
				.expect("proof generation for an existing key works; qed");
		assert!(keccak_256_verify_proof(root, &proof, b"beta", b"two", StateVersion::V0));
		assert!(!keccak_256_verify_proof(root, &proof, b"beta", b"tampered", StateVersion::V0));
	}
}
