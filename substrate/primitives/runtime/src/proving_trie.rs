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

//! Types for a compact base-16 merkle trie used for checking and generating proofs within the
//! runtime. The `sp-trie` crate exposes all of these same functionality (and more), but this
//! library is designed to work more easily with runtime native types, which simply need to
//! implement `Encode`/`Decode`. It also exposes a runtime friendly `TrieError` type which can be
//! use inside of a FRAME Pallet.
//!
//! Proofs are created with latest substrate trie format (`LayoutV1`), and are not compatible with
//! proofs using `LayoutV0`.

use crate::{Decode, DispatchError, Encode, MaxEncodedLen, TypeInfo};
#[cfg(feature = "serde")]
use crate::{Deserialize, Serialize};

use sp_std::vec::Vec;
use sp_trie::{
	trie_types::{TrieDBBuilder, TrieDBMutBuilderV1, TrieError as SpTrieError},
	LayoutV1, MemoryDB, Trie, TrieMut, VerifyError,
};

type HashOf<Hashing> = <Hashing as sp_core::Hasher>::Out;

/// A runtime friendly error type for tries.
#[derive(Eq, PartialEq, Clone, Copy, Encode, Decode, Debug, TypeInfo, MaxEncodedLen)]
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

/// A helper structure for building a basic base-16 merkle trie and creating compact proofs for that
/// trie. Proofs are created with latest substrate trie format (`LayoutV1`), and are not compatible
/// with proofs using `LayoutV0`.
pub struct BasicProvingTrie<Hashing, Key, Value>
where
	Hashing: sp_core::Hasher,
{
	db: MemoryDB<Hashing>,
	root: HashOf<Hashing>,
	_phantom: core::marker::PhantomData<(Key, Value)>,
}

impl<Hashing, Key, Value> BasicProvingTrie<Hashing, Key, Value>
where
	Hashing: sp_core::Hasher,
	Key: Encode,
	Value: Encode,
{
	/// Create a new instance of a `ProvingTrie` using an iterator of key/value pairs.
	pub fn generate_for<I>(items: I) -> Result<Self, DispatchError>
	where
		I: IntoIterator<Item = (Key, Value)>,
	{
		let mut db = MemoryDB::default();
		let mut root = Default::default();

		{
			let mut trie = TrieDBMutBuilderV1::new(&mut db, &mut root).build();
			for (key, value) in items.into_iter() {
				key.using_encoded(|k| value.using_encoded(|v| trie.insert(k, v)))
					.map_err(|_| "failed to insert into trie")?;
			}
		}

		Ok(Self { db, root, _phantom: Default::default() })
	}

	/// Access the underlying trie root.
	pub fn root(&self) -> &HashOf<Hashing> {
		&self.root
	}

	/// Query a value contained within the current trie. Returns `None` if the
	/// nodes within the current `MemoryDB` are insufficient to query the item.
	pub fn query(&self, key: Key) -> Option<Value>
	where
		Value: Decode,
	{
		let trie = TrieDBBuilder::new(&self.db, &self.root).build();
		key.using_encoded(|s| trie.get(s))
			.ok()?
			.and_then(|raw| Value::decode(&mut &*raw).ok())
	}

	/// Create a compact merkle proof needed to prove all `keys` and their values are in the trie.
	/// Returns `None` if the nodes within the current `MemoryDB` are insufficient to create a
	/// proof.
	///
	/// This function makes a proof with latest substrate trie format (`LayoutV1`), and is not
	/// compatible with `LayoutV0`.
	///
	/// When verifying the proof created by this function, you must include all of the keys and
	/// values of the proof, else the verifier will complain that extra nodes are provided in the
	/// proof that are not needed.
	pub fn create_proof(&self, keys: &[Key]) -> Result<Vec<Vec<u8>>, DispatchError> {
		sp_trie::generate_trie_proof::<LayoutV1<Hashing>, _, _, _>(
			&self.db,
			self.root,
			&keys.into_iter().map(|k| k.encode()).collect::<Vec<Vec<u8>>>(),
		)
		.map_err(|err| TrieError::from(*err).into())
	}

	/// Create a compact merkle proof needed to prove a single key and its value are in the trie.
	/// Returns `None` if the nodes within the current `MemoryDB` are insufficient to create a
	/// proof.
	///
	/// This function makes a proof with latest substrate trie format (`LayoutV1`), and is not
	/// compatible with `LayoutV0`.
	pub fn create_single_value_proof(&self, key: Key) -> Result<Vec<Vec<u8>>, DispatchError> {
		self.create_proof(&[key])
	}
}

/// Verify the existence or non-existence of `key` and `value` in a given trie root and proof.
///
/// Proofs must be created with latest substrate trie format (`LayoutV1`).
pub fn verify_single_value_proof<Hashing, Key, Value>(
	root: HashOf<Hashing>,
	proof: &[Vec<u8>],
	key: Key,
	maybe_value: Option<Value>,
) -> Result<(), DispatchError>
where
	Hashing: sp_core::Hasher,
	Key: Encode,
	Value: Encode,
{
	sp_trie::verify_trie_proof::<LayoutV1<Hashing>, _, _, _>(
		&root,
		proof,
		&[(key.encode(), maybe_value.map(|value| value.encode()))],
	)
	.map_err(|err| TrieError::from(err).into())
}

/// Verify the existence or non-existence of multiple `items` in a given trie root and proof.
///
/// Proofs must be created with latest substrate trie format (`LayoutV1`).
pub fn verify_proof<Hashing, Key, Value>(
	root: HashOf<Hashing>,
	proof: &[Vec<u8>],
	items: &[(Key, Option<Value>)],
) -> Result<(), DispatchError>
where
	Hashing: sp_core::Hasher,
	Key: Encode,
	Value: Encode,
{
	let items_encoded = items
		.into_iter()
		.map(|(key, maybe_value)| (key.encode(), maybe_value.as_ref().map(|value| value.encode())))
		.collect::<Vec<(Vec<u8>, Option<Vec<u8>>)>>();

	sp_trie::verify_trie_proof::<LayoutV1<Hashing>, _, _, _>(&root, proof, &items_encoded)
		.map_err(|err| TrieError::from(err).into())
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::traits::BlakeTwo256;
	use sp_core::H256;
	use sp_std::collections::btree_map::BTreeMap;

	// A trie which simulates a trie of accounts (u32) and balances (u128).
	type BalanceTrie = BasicProvingTrie<BlakeTwo256, u32, u128>;

	// The expected root hash for an empty trie.
	fn empty_root() -> H256 {
		sp_trie::empty_trie_root::<LayoutV1<BlakeTwo256>>()
	}

	fn create_balance_trie() -> BalanceTrie {
		// Create a map of users and their balances.
		let mut map = BTreeMap::<u32, u128>::new();
		for i in 0..100u32 {
			map.insert(i, i.into());
		}

		// Put items into the trie.
		let balance_trie = BalanceTrie::generate_for(map).unwrap();

		// Root is changed.
		let root = *balance_trie.root();
		assert!(root != empty_root());

		// Assert valid keys are queryable.
		assert_eq!(balance_trie.query(6u32), Some(6u128));
		assert_eq!(balance_trie.query(9u32), Some(9u128));
		assert_eq!(balance_trie.query(69u32), Some(69u128));
		// Invalid key returns none.
		assert_eq!(balance_trie.query(6969u32), None);

		balance_trie
	}

	#[test]
	fn empty_trie_works() {
		let empty_trie = BalanceTrie::generate_for(Vec::new()).unwrap();
		assert_eq!(*empty_trie.root(), empty_root());
	}

	#[test]
	fn basic_end_to_end_single_value() {
		let balance_trie = create_balance_trie();
		let root = *balance_trie.root();

		// Create a proof for a valid key.
		let proof = balance_trie.create_single_value_proof(6u32).unwrap();

		// Assert key is provable, all other keys are invalid.
		for i in 0..200u32 {
			if i == 6 {
				assert_eq!(
					verify_single_value_proof::<BlakeTwo256, _, _>(
						root,
						&proof,
						i,
						Some(u128::from(i))
					),
					Ok(())
				);
				// Wrong value is invalid.
				assert_eq!(
					verify_single_value_proof::<BlakeTwo256, _, _>(
						root,
						&proof,
						i,
						Some(u128::from(i + 1))
					),
					Err(TrieError::RootMismatch.into())
				);
			} else {
				assert!(verify_single_value_proof::<BlakeTwo256, _, _>(
					root,
					&proof,
					i,
					Some(u128::from(i))
				)
				.is_err());
				assert!(verify_single_value_proof::<BlakeTwo256, _, _>(
					root,
					&proof,
					i,
					None::<u128>
				)
				.is_err());
			}
		}
	}

	#[test]
	fn basic_end_to_end_multi_value() {
		let balance_trie = create_balance_trie();
		let root = *balance_trie.root();

		// Create a proof for a valid and invalid key.
		let proof = balance_trie.create_proof(&[6u32, 69u32, 6969u32]).unwrap();
		let items = [(6u32, Some(6u128)), (69u32, Some(69u128)), (6969u32, None)];

		assert_eq!(verify_proof::<BlakeTwo256, _, _>(root, &proof, &items), Ok(()));
	}

	#[test]
	fn proof_fails_with_bad_data() {
		let balance_trie = create_balance_trie();
		let root = *balance_trie.root();

		// Create a proof for a valid key.
		let proof = balance_trie.create_single_value_proof(6u32).unwrap();

		// Correct data verifies successfully
		assert_eq!(
			verify_single_value_proof::<BlakeTwo256, _, _>(root, &proof, 6u32, Some(6u128)),
			Ok(())
		);

		// Fail to verify proof with wrong root
		assert_eq!(
			verify_single_value_proof::<BlakeTwo256, _, _>(
				Default::default(),
				&proof,
				6u32,
				Some(6u128)
			),
			Err(TrieError::RootMismatch.into())
		);

		// Fail to verify proof with wrong data
		assert_eq!(
			verify_single_value_proof::<BlakeTwo256, _, _>(root, &[], 6u32, Some(6u128)),
			Err(TrieError::IncompleteProof.into())
		);
	}
}
