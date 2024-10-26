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
use sp_runtime::DispatchError;

// Re-export the `proving_trie` types and traits.
pub use sp_runtime::proving_trie::*;

/// Something that can verify the existence of some data in a given proof.
pub trait VerifyExistenceProof {
	/// The proof type.
	type Proof;
	/// The hash type.
	type Hash;

	/// Verify the given `proof`.
	///
	/// Ensures that the `proof` was build for `root` and returns the proved data.
	fn verify_proof(proof: Self::Proof, root: &Self::Hash) -> Result<Vec<u8>, DispatchError>;
}

/// Implements [`VerifyExistenceProof`] using a binary merkle tree.
pub struct BinaryMerkleTreeProver<H>(core::marker::PhantomData<H>);

impl<H: Hasher> VerifyExistenceProof for BinaryMerkleTreeProver<H>
where
	H::Out: Decode + Encode,
{
	type Proof = binary_merkle_tree::MerkleProof<H::Out, Vec<u8>>;
	type Hash = H::Out;

	fn verify_proof(proof: Self::Proof, root: &Self::Hash) -> Result<Vec<u8>, DispatchError> {
		if proof.root != *root {
			return Err(TrieError::RootMismatch.into());
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
			Err(TrieError::IncompleteProof.into())
		}
	}
}

impl<H: Hasher> ProofToHashes for BinaryMerkleTreeProver<H> {
	type Proof = binary_merkle_tree::MerkleProof<H::Out, Vec<u8>>;

	// This base 2 merkle trie includes a `proof` field which is a `Vec<Hash>`.
	// The length of this vector tells us the depth of the proof, and how many
	// hashes we need to calculate.
	fn proof_to_hashes(proof: &Self::Proof) -> Result<u32, DispatchError> {
		let depth = proof.proof.len();
		Ok(depth as u32)
	}
}

/// Proof used by [`SixteenPatriciaMerkleTreeProver`] for [`VerifyExistenceProof`].
#[derive(Encode, Decode, Clone)]
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

	fn verify_proof(proof: Self::Proof, root: &Self::Hash) -> Result<Vec<u8>, DispatchError> {
		sp_trie::verify_trie_proof::<sp_trie::LayoutV1<H>, _, _, _>(
			&root,
			&proof.proof,
			[&(&proof.key, Some(&proof.value))],
		)
		.map_err(|err| TrieError::from(err).into())
		.map(|_| proof.value)
	}
}

impl<H: Hasher> ProofToHashes for SixteenPatriciaMerkleTreeProver<H> {
	type Proof = SixteenPatriciaMerkleTreeExistenceProof;

	// This base 16 trie uses a raw proof of `Vec<Vec<u8>`, where the length of the first `Vec`
	// is the depth of the trie. We can use this to predict the number of hashes.
	fn proof_to_hashes(proof: &Self::Proof) -> Result<u32, DispatchError> {
		let depth = proof.proof.len();
		Ok(depth as u32)
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use sp_runtime::{
		proving_trie::{base16::BasicProvingTrie, ProvingTrie},
		traits::BlakeTwo256,
	};

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
		let trie = BasicProvingTrie::<BlakeTwo256, u32, _>::generate_for(vec![
			(0u32, String::from("hey")),
			(1u32, String::from("yes")),
		])
		.unwrap();
		let proof = trie.create_proof(&1u32).unwrap();
		let structured_proof: Vec<Vec<u8>> = Decode::decode(&mut &proof[..]).unwrap();
		let root = *trie.root();

		let proof = SixteenPatriciaMerkleTreeExistenceProof {
			key: 1u32.encode(),
			value: String::from("yes").encode(),
			proof: structured_proof,
		};

		assert_eq!(
			SixteenPatriciaMerkleTreeProver::<BlakeTwo256>::verify_proof(proof, &root).unwrap(),
			String::from("yes").encode()
		);
	}

	#[test]
	fn proof_to_hashes_sixteen() {
		let mut i: u32 = 1;

		// Compute log base 16 and round up
		let log16 = |x: u32| -> u32 {
			let x_f64 = x as f64;
			let log16_x = (x_f64.ln() / 16_f64.ln()).ceil();
			log16_x as u32
		};

		while i < 10_000_000 {
			let trie = BasicProvingTrie::<BlakeTwo256, u32, _>::generate_for(
				(0..i).map(|i| (i, u128::from(i))),
			)
			.unwrap();
			let proof = trie.create_proof(&0).unwrap();
			let structured_proof: Vec<Vec<u8>> = Decode::decode(&mut &proof[..]).unwrap();
			let root = *trie.root();

			let proof = SixteenPatriciaMerkleTreeExistenceProof {
				key: 0u32.encode(),
				value: 0u128.encode(),
				proof: structured_proof,
			};
			let hashes =
				SixteenPatriciaMerkleTreeProver::<BlakeTwo256>::proof_to_hashes(&proof).unwrap();
			let log16 = log16(i).max(1);
			assert_eq!(hashes, log16);

			assert_eq!(
				SixteenPatriciaMerkleTreeProver::<BlakeTwo256>::verify_proof(proof.clone(), &root)
					.unwrap(),
				proof.value
			);

			i = i * 10;
		}
	}

	#[test]
	fn proof_to_hashes_binary() {
		let mut i: u32 = 1;
		while i < 10_000_000 {
			let proof = binary_merkle_tree::merkle_proof::<BlakeTwo256, _, _>(
				(0..i).map(|i| u128::from(i).encode()),
				0,
			);
			let root = proof.root;

			let hashes = BinaryMerkleTreeProver::<BlakeTwo256>::proof_to_hashes(&proof).unwrap();
			let log2 = (i as f64).log2().ceil() as u32;
			assert_eq!(hashes, log2);

			assert_eq!(
				BinaryMerkleTreeProver::<BlakeTwo256>::verify_proof(proof, &root).unwrap(),
				0u128.encode()
			);

			i = i * 10;
		}
	}
}
