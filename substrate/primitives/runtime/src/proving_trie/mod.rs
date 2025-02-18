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

//! Types for merkle tries compatible with the runtime.

pub mod base16;
pub mod base2;

use crate::{Decode, DecodeWithMemTracking, DispatchError, Encode, MaxEncodedLen, TypeInfo};
#[cfg(feature = "serde")]
use crate::{Deserialize, Serialize};
use alloc::vec::Vec;
use sp_trie::{trie_types::TrieError as SpTrieError, VerifyError};

/// A runtime friendly error type for tries.
#[derive(
	Eq,
	PartialEq,
	Clone,
	Copy,
	Encode,
	Decode,
	DecodeWithMemTracking,
	Debug,
	TypeInfo,
	MaxEncodedLen,
)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub enum TrieError {
	/* From TrieError */
	/// Attempted to create a trie with a state root not in the DB.
	InvalidStateRoot,
	/// Trie item not found in the database,
	IncompleteDatabase,
	/// A value was found in the trie with a nibble key that was not byte-aligned.
	ValueAtIncompleteKey,
	/// Corrupt Trie item.
	DecoderError,
	/// Hash is not value.
	InvalidHash,
	/* From VerifyError */
	/// The statement being verified contains multiple key-value pairs with the same key.
	DuplicateKey,
	/// The proof contains at least one extraneous node.
	ExtraneousNode,
	/// The proof contains at least one extraneous value which should have been omitted from the
	/// proof.
	ExtraneousValue,
	/// The proof contains at least one extraneous hash reference the should have been omitted.
	ExtraneousHashReference,
	/// The proof contains an invalid child reference that exceeds the hash length.
	InvalidChildReference,
	/// The proof indicates that an expected value was not found in the trie.
	ValueMismatch,
	/// The proof is missing trie nodes required to verify.
	IncompleteProof,
	/// The root hash computed from the proof is incorrect.
	RootMismatch,
	/// One of the proof nodes could not be decoded.
	DecodeError,
}

impl<T> From<SpTrieError<T>> for TrieError {
	fn from(error: SpTrieError<T>) -> Self {
		match error {
			SpTrieError::InvalidStateRoot(..) => Self::InvalidStateRoot,
			SpTrieError::IncompleteDatabase(..) => Self::IncompleteDatabase,
			SpTrieError::ValueAtIncompleteKey(..) => Self::ValueAtIncompleteKey,
			SpTrieError::DecoderError(..) => Self::DecoderError,
			SpTrieError::InvalidHash(..) => Self::InvalidHash,
		}
	}
}

impl<T, U> From<VerifyError<T, U>> for TrieError {
	fn from(error: VerifyError<T, U>) -> Self {
		match error {
			VerifyError::DuplicateKey(..) => Self::DuplicateKey,
			VerifyError::ExtraneousNode => Self::ExtraneousNode,
			VerifyError::ExtraneousValue(..) => Self::ExtraneousValue,
			VerifyError::ExtraneousHashReference(..) => Self::ExtraneousHashReference,
			VerifyError::InvalidChildReference(..) => Self::InvalidChildReference,
			VerifyError::ValueMismatch(..) => Self::ValueMismatch,
			VerifyError::IncompleteProof => Self::IncompleteProof,
			VerifyError::RootMismatch(..) => Self::RootMismatch,
			VerifyError::DecodeError(..) => Self::DecodeError,
		}
	}
}

impl From<TrieError> for &'static str {
	fn from(e: TrieError) -> &'static str {
		match e {
			TrieError::InvalidStateRoot => "The state root is not in the database.",
			TrieError::IncompleteDatabase => "A trie item was not found in the database.",
			TrieError::ValueAtIncompleteKey =>
				"A value was found with a key that is not byte-aligned.",
			TrieError::DecoderError => "A corrupt trie item was encountered.",
			TrieError::InvalidHash => "The hash does not match the expected value.",
			TrieError::DuplicateKey => "The proof contains duplicate keys.",
			TrieError::ExtraneousNode => "The proof contains extraneous nodes.",
			TrieError::ExtraneousValue => "The proof contains extraneous values.",
			TrieError::ExtraneousHashReference => "The proof contains extraneous hash references.",
			TrieError::InvalidChildReference => "The proof contains an invalid child reference.",
			TrieError::ValueMismatch => "The proof indicates a value mismatch.",
			TrieError::IncompleteProof => "The proof is incomplete.",
			TrieError::RootMismatch => "The root hash computed from the proof is incorrect.",
			TrieError::DecodeError => "One of the proof nodes could not be decoded.",
		}
	}
}

/// An interface for creating, interacting with, and creating proofs in a merkle trie.
pub trait ProvingTrie<Hashing, Key, Value>
where
	Self: Sized,
	Hashing: sp_core::Hasher,
{
	/// Create a new instance of a `ProvingTrie` using an iterator of key/value pairs.
	fn generate_for<I>(items: I) -> Result<Self, DispatchError>
	where
		I: IntoIterator<Item = (Key, Value)>;
	/// Access the underlying trie root.
	fn root(&self) -> &Hashing::Out;
	/// Query a value contained within the current trie. Returns `None` if the
	/// the value does not exist in the trie.
	fn query(&self, key: &Key) -> Option<Value>;
	/// Create a proof that can be used to verify a key and its value are in the trie.
	fn create_proof(&self, key: &Key) -> Result<Vec<u8>, DispatchError>;
	/// Verify the existence of `key` and `value` in a given trie root and proof.
	fn verify_proof(
		root: &Hashing::Out,
		proof: &[u8],
		key: &Key,
		value: &Value,
	) -> Result<(), DispatchError>;
}

/// This trait is one strategy that can be used to benchmark a trie proof verification for the
/// runtime. This strategy assumes that the majority complexity of verifying a merkle proof comes
/// from computing hashes to recreate the merkle root. This trait converts the the proof, some
/// bytes, to the number of hashes we expect to execute to verify that proof.
pub trait ProofToHashes {
	/// The Proof type we will use to determine the number of hashes.
	type Proof: ?Sized;
	/// This function returns the number of hashes we expect to calculate based on the
	/// size of the proof. This is used for benchmarking, so for worst case scenario, we should
	/// round up.
	///
	/// The major complexity of doing a `verify_proof` is computing the hashes needed
	/// to calculate the merkle root. For tries, it should be easy to predict the depth
	/// of the trie (which is equivalent to the hashes), by looking at the length of the proof.
	fn proof_to_hashes(proof: &Self::Proof) -> Result<u32, DispatchError>;
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::traits::BlakeTwo256;

	// A trie which simulates a trie of accounts (u32) and balances (u128).
	type BalanceTrie2 = base2::BasicProvingTrie<BlakeTwo256, u32, u128>;
	type BalanceTrie16 = base16::BasicProvingTrie<BlakeTwo256, u32, u128>;

	#[test]
	fn basic_api_usage_base_2() {
		let balance_trie = BalanceTrie2::generate_for((0..100u32).map(|i| (i, i.into()))).unwrap();
		let root = *balance_trie.root();
		assert_eq!(balance_trie.query(&69), Some(69));
		assert_eq!(balance_trie.query(&6969), None);
		let proof = balance_trie.create_proof(&69u32).unwrap();
		assert_eq!(BalanceTrie2::verify_proof(&root, &proof, &69u32, &69u128), Ok(()));
	}

	#[test]
	fn basic_api_usage_base_16() {
		let balance_trie = BalanceTrie16::generate_for((0..100u32).map(|i| (i, i.into()))).unwrap();
		let root = *balance_trie.root();
		assert_eq!(balance_trie.query(&69), Some(69));
		assert_eq!(balance_trie.query(&6969), None);
		let proof = balance_trie.create_proof(&69u32).unwrap();
		assert_eq!(BalanceTrie16::verify_proof(&root, &proof, &69u32, &69u128), Ok(()));
	}
}
