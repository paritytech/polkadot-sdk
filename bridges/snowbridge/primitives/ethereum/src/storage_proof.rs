// This file is part of Substrate.

// Copyright (C) 2020-2022 Parity Technologies (UK) Ltd.
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

//! Storage Proof abstraction

use crate::node_codec;
use alloc::{
	collections::BTreeSet,
	format,
	string::{String, ToString},
	vec,
	vec::Vec,
};
use alloy_rlp::Decodable;
use alloy_rlp_derive::{RlpDecodable, RlpEncodable};
use codec::{Decode, Encode};
use ethereum_types::{H256, U256};
use hash_db::{HashDB, Hasher};
use scale_info::TypeInfo;
use sp_runtime::traits::Keccak256;
use sp_std::marker::PhantomData;
use trie_db::{DBValue, Trie, TrieDBBuilder, TrieLayout};

#[derive(Default, Clone)]
pub struct EIP1186Layout<H>(PhantomData<H>);

impl<H: Hasher<Out = H256>> TrieLayout for EIP1186Layout<H> {
	const USE_EXTENSION: bool = true;
	const ALLOW_EMPTY: bool = false;
	const MAX_INLINE_VALUE: Option<u32> = None;
	type Hash = H;
	type Codec = node_codec::RlpNodeCodec<H>;
}

/// The ethereum account stored in the global state trie.
#[derive(RlpDecodable, RlpEncodable)]
pub struct Account {
	pub nonce: u64,
	pub balance: alloy_primitives::U256,
	pub storage_root: alloy_primitives::B256,
	pub code_hash: alloy_primitives::B256,
}

/// A proof that some set of key-value pairs are included in the storage trie. The proof contains
/// the storage values so that the partial storage backend can be reconstructed by a verifier that
/// does not already have access to the key-value pairs.
///
/// The proof consists of the set of serialized nodes in the storage trie accessed when looking up
/// the keys covered by the proof. Verifying the proof requires constructing the partial trie from
/// the serialized nodes and performing the key lookups.
#[derive(Debug, PartialEq, Eq, Clone, Encode, Decode)]
pub struct StorageProof {
	trie_nodes: BTreeSet<Vec<u8>>,
}

/// Aliased memory db type
pub type MemoryDB<H> = memory_db::MemoryDB<H, memory_db::HashKey<H>, trie_db::DBValue>;

impl StorageProof {
	/// Constructs a storage proof from a subset of encoded trie nodes in a storage backend.
	pub fn new(trie_nodes: impl IntoIterator<Item = Vec<u8>>) -> Self {
		StorageProof { trie_nodes: BTreeSet::from_iter(trie_nodes) }
	}

	/// Returns a new empty proof.
	///
	/// An empty proof is capable of only proving trivial statements (ie. that an empty set of
	/// key-value pairs exist in storage).
	pub fn empty() -> Self {
		StorageProof { trie_nodes: BTreeSet::new() }
	}

	/// Returns whether this is an empty proof.
	pub fn is_empty(&self) -> bool {
		self.trie_nodes.is_empty()
	}

	/// Create an iterator over encoded trie nodes in lexicographical order constructed
	/// from the proof.
	pub fn iter_nodes(self) -> StorageProofNodeIterator {
		StorageProofNodeIterator::new(self)
	}

	/// Convert into plain node vector.
	pub fn into_nodes(self) -> BTreeSet<Vec<u8>> {
		self.trie_nodes
	}

	/// Creates a [`MemoryDB`](memory_db::MemoryDB) from `Self`.
	pub fn into_memory_db<H: Hasher>(self) -> MemoryDB<H> {
		self.into()
	}

	/// Merges multiple storage proofs covering potentially different sets of keys into one proof
	/// covering all keys. The merged proof output may be smaller than the aggregate size of the
	/// input proofs due to deduplication of trie nodes.
	pub fn merge(proofs: impl IntoIterator<Item = Self>) -> Self {
		let trie_nodes = proofs
			.into_iter()
			.flat_map(|proof| proof.iter_nodes())
			.collect::<BTreeSet<_>>()
			.into_iter()
			.collect();

		Self { trie_nodes }
	}
}

impl<H: Hasher> From<StorageProof> for MemoryDB<H> {
	fn from(proof: StorageProof) -> Self {
		let mut db = MemoryDB::default();
		proof.iter_nodes().for_each(|n| {
			db.insert(hash_db::EMPTY_PREFIX, &n);
		});
		db
	}
}

/// An iterator over trie nodes constructed from a storage proof. The nodes are not guaranteed to
/// be traversed in any particular order.
pub struct StorageProofNodeIterator {
	inner: <BTreeSet<Vec<u8>> as IntoIterator>::IntoIter,
}

impl StorageProofNodeIterator {
	fn new(proof: StorageProof) -> Self {
		StorageProofNodeIterator { inner: proof.trie_nodes.into_iter() }
	}
}

impl Iterator for StorageProofNodeIterator {
	type Item = Vec<u8>;

	fn next(&mut self) -> Option<Self::Item> {
		self.inner.next()
	}
}

/// Errors that may be encountered by the ISMP module
#[derive(Debug, Eq, PartialEq, Encode, Decode, TypeInfo)]
pub enum Error {
	/// Custom error: {0}
	Decode(String),
}

pub fn get_contract_account(
	contract_account_proof: Vec<Vec<u8>>,
	contract_address: &[u8],
	root: H256,
) -> Result<Account, Error> {
	let db = StorageProof::new(contract_account_proof).into_memory_db::<Keccak256>();
	let trie = TrieDBBuilder::<EIP1186Layout<Keccak256>>::new(&db, &root).build();
	let key = Keccak256::hash(contract_address).0;
	let result = trie
		.get(&key)
		.map_err(|_| Error::Decode("Invalid contract account proof".to_string()))?
		.ok_or_else(|| Error::Decode("Contract account is not present in proof".to_string()))?;

	let contract_account = <Account as Decodable>::decode(&mut &*result).map_err(|_| {
		Error::Decode(format!("Error decoding contract account from value {:?}", &result))
	})?;

	Ok(contract_account)
}

pub fn derive_map_key(mut key: Vec<u8>, slot: u64) -> H256 {
	key.extend_from_slice(&U256::from(slot).to_big_endian());
	Keccak256::hash(&key).0.into()
}

pub fn derive_map_key_with_offset(mut key: Vec<u8>, slot: u64, offset: u64) -> H256 {
	key.extend_from_slice(&U256::from(slot).to_big_endian());
	let root_key = Keccak256::hash(&key).0;
	let number = U256::from_big_endian(root_key.as_slice()) + U256::from(offset);
	Keccak256::hash(&number.to_big_endian()).0.into()
}

pub fn derive_unhashed_map_key(mut key: Vec<u8>, slot: u64) -> H256 {
	key.extend_from_slice(&U256::from(slot).to_big_endian());
	Keccak256::hash(&key).0.into()
}

pub fn add_off_set_to_map_key(key: &[u8], offset: u64) -> H256 {
	let number = U256::from_big_endian(key) + U256::from(offset);
	H256(number.to_big_endian())
}

pub fn derive_array_item_key(slot: u64, index: u64, offset: u64) -> Vec<u8> {
	let hash_result = Keccak256::hash(&U256::from(slot).to_big_endian());

	let array_pos = U256::from_big_endian(&hash_result.0);
	let item_pos = array_pos + U256::from(index * 2) + U256::from(offset);

	Keccak256::hash(&item_pos.to_big_endian()).0.to_vec()
}

pub fn get_values_from_proof(
	keys: Vec<Vec<u8>>,
	root: H256,
	proof: Vec<Vec<u8>>,
) -> Result<Vec<Option<DBValue>>, Error> {
	let mut values = vec![];
	let proof_db = StorageProof::new(proof).into_memory_db::<Keccak256>();
	let trie = TrieDBBuilder::<EIP1186Layout<Keccak256>>::new(&proof_db, &root).build();
	for key in keys {
		let val = trie
			.get(&key)
			.map_err(|e| Error::Decode(format!("Error reading proof db {:?}", e)))?;
		values.push(val);
	}

	Ok(values)
}

pub fn get_value_from_proof(
	key: Vec<u8>,
	root: H256,
	proof: Vec<Vec<u8>>,
) -> Result<Option<DBValue>, Error> {
	let proof_db = StorageProof::new(proof).into_memory_db::<Keccak256>();
	let trie = TrieDBBuilder::<EIP1186Layout<Keccak256>>::new(&proof_db, &root).build();
	let val = trie
		.get(&key)
		.map_err(|e| Error::Decode(format!("Error reading proof db {:?}", e)))?;

	Ok(val)
}
