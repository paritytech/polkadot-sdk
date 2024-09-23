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

//! Provides functionality for verifying proofs.

use alloc::vec::Vec;
use codec::{Decode, Encode};
use sp_core::Hasher;

/// Something that can verify the existence of some data in a given proof.
pub trait VerifyExistenceProof {
	/// The proof type.
	type Proof;
	/// The hash type.
	type Hash;

	/// Verify the given `proof`.
	///
	/// Ensures that the `proof` was build for `root` and returns the proved data.
	fn verify_proof(proof: Self::Proof, root: &Self::Hash) -> Result<Vec<u8>, ()>;
}

/// Implements [`VerifyExistenceProof`] using a binary merkle tree.
pub struct BinaryMerkleTreeProver<H>(core::marker::PhantomData<H>);

impl<H: Hasher> VerifyExistenceProof for BinaryMerkleTreeProver<H>
where
	H::Out: Decode + Encode,
{
	type Proof = binary_merkle_tree::MerkleProof<H::Out, Vec<u8>>;
	type Hash = H::Out;

	fn verify_proof(proof: Self::Proof, root: &Self::Hash) -> Result<Vec<u8>, ()> {
		if proof.root != *root {
			return Err(());
		}

		if binary_merkle_tree::verify_proof::<H, _, _>(
			&proof.root,
			proof.proof,
			proof.number_of_leaves,
			proof.leaf_index,
			&proof.leaf,
		) {
			Ok(proof.leaf)
		} else {
			Err(())
		}
	}
}

/// Proof used by [`SixteenPatriciaMerkleTreeProver`] for [`VerifyExistenceProof`].
#[derive(Encode, Decode)]
pub struct SixteenPatriciaMerkleTreeExistenceProof {
	/// The key of the value to prove.
	pub key: Vec<u8>,
	/// The value for that the existence is proved.
	pub value: Vec<u8>,
	/// The encoded nodes to prove the existence of the data under `key`.
	pub proof: Vec<Vec<u8>>,
}

/// Implements [`VerifyExistenceProof`] using a 16-patricia merkle tree.
pub struct SixteenPatriciaMerkleTreeProver<H>(core::marker::PhantomData<H>);

impl<H: Hasher> VerifyExistenceProof for SixteenPatriciaMerkleTreeProver<H> {
	type Proof = SixteenPatriciaMerkleTreeExistenceProof;
	type Hash = H::Out;

	fn verify_proof(proof: Self::Proof, root: &Self::Hash) -> Result<Vec<u8>, ()> {
		sp_trie::verify_trie_proof::<sp_trie::LayoutV1<H>, _, _, _>(
			&root,
			&proof.proof,
			[&(&proof.key, Some(&proof.value))],
		)
		.map_err(drop)
		.map(|_| proof.value)
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use sp_runtime::{proving_trie::BasicProvingTrie, traits::BlakeTwo256};

	#[test]
	fn verify_binary_merkle_tree_prover_works() {
		let proof = binary_merkle_tree::merkle_proof::<BlakeTwo256, _, _>(
			vec![b"hey".encode(), b"yes".encode()],
			1,
		);
		let root = proof.root;

		assert_eq!(
			BinaryMerkleTreeProver::<BlakeTwo256>::verify_proof(proof, &root).unwrap(),
			b"yes".encode()
		);
	}

	#[test]
	fn verify_sixteen_patricia_merkle_tree_prover_works() {
		let trie = BasicProvingTrie::<BlakeTwo256, u32, &[u8]>::generate_for(vec![
			(0u32, &b"hey"[..]),
			(1u32, &b"yes"[..]),
		])
		.unwrap();
		let proof = trie.create_single_value_proof(1u32).unwrap();
		let root = *trie.root();

		let proof = SixteenPatriciaMerkleTreeExistenceProof {
			key: 1u32.encode(),
			value: b"yes"[..].encode(),
			proof,
		};

		assert_eq!(
			SixteenPatriciaMerkleTreeProver::<BlakeTwo256>::verify_proof(proof, &root).unwrap(),
			b"yes"[..].encode()
		);
	}
}
