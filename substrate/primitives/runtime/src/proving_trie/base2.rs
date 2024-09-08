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

//! Types for a base-2 merkle tree used for checking and generating proofs within the
//! runtime.

use super::TrieError;
use crate::{Decode, DispatchError, Encode};

use sp_std::vec::Vec;
use sp_trie::{
	trie_types::{TrieDBBuilder, TrieDBMutBuilderV1},
	LayoutV1, MemoryDB, Trie, TrieMut,
};

type HashOf<Hashing> = <Hashing as sp_core::Hasher>::Out;

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
	Value: Encode + Decode,
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
	pub fn query(&self, key: Key) -> Option<Value> {
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
