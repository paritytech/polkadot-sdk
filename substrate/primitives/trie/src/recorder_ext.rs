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

//! Extension for the default recorder.

use crate::{RawStorageProof, StorageProof};
use alloc::{collections::BTreeSet, vec::Vec};
use scale_info::TypeInfo;
use sp_core::{Decode, Encode};
use trie_db::{RecordedForKey, Recorder, TrieAccess, TrieHash, TrieLayout, TrieRecorder};

#[derive(Encode, Decode, Clone, Eq, PartialEq, Debug, TypeInfo)]
pub enum Error {
	/// The proof contains duplicate nodes.
	DuplicateNodes,
	/// The proof contains unused nodes.
	UnusedNodes,
}

pub trait RecorderExt<L: TrieLayout>
where
	Self: Sized,
{
	fn into_set(self) -> BTreeSet<Vec<u8>>;

	fn into_optimized_raw_storage_proof(self) -> RawStorageProof {
		// The recorder may record the same trie node multiple times,
		// and we don't want duplicate nodes in our proofs
		// => let's deduplicate it by collecting to a BTreeSet first
		self.into_set().into_iter().collect()
	}
}

impl<L: TrieLayout> RecorderExt<L> for Recorder<L> {
	fn into_set(mut self) -> BTreeSet<Vec<u8>> {
		self.drain().into_iter().map(|record| record.data).collect::<BTreeSet<_>>()
	}
}

pub struct RedundantNodesChecker<L: TrieLayout> {
	proof_nodes_count: usize,
	recorder: Recorder<L>,
}

impl<L: TrieLayout> RedundantNodesChecker<L> {
	pub fn new(raw_proof: RawStorageProof) -> Result<(Self, StorageProof), Error> {
		// We don't want extra items in the storage proof.
		// Let's check this when we are converting our "raw proof" into a `StorageProof` since
		// `StorageProof` is storing all trie nodes in a `BTreeSet`.
		let proof_nodes_count = raw_proof.len();
		let proof = StorageProof::new(raw_proof);
		if proof_nodes_count != proof.iter_nodes().count() {
			return Err(Error::DuplicateNodes)
		}

		Ok((Self { proof_nodes_count, recorder: Recorder::new() }, proof))
	}

	/// Ensure that all the nodes in the proof have been accessed.
	pub fn ensure_no_unused_nodes(self) -> Result<(), Error> {
		if self.proof_nodes_count != self.recorder.into_set().len() {
			return Err(Error::UnusedNodes)
		}

		Ok(())
	}
}

impl<L: TrieLayout> TrieRecorder<TrieHash<L>> for RedundantNodesChecker<L> {
	fn record(&mut self, access: TrieAccess<TrieHash<L>>) {
		self.recorder.record(access)
	}

	fn trie_nodes_recorded_for_key(&self, key: &[u8]) -> RecordedForKey {
		self.recorder.trie_nodes_recorded_for_key(key)
	}
}

#[cfg(test)]
pub mod tests {
	use super::*;
	use trie_db::{TrieDBMutBuilder, TrieMut};

	type MemoryDB = crate::MemoryDB<sp_core::Blake2Hasher>;
	type Layout = crate::LayoutV1<sp_core::Blake2Hasher>;

	const TEST_DATA: &[(&[u8], &[u8])] = &[
		(b"key1", &[1; 64]),
		(b"key2", &[2; 64]),
		(b"key3", &[3; 64]),
		(b"key4", &[4; 64]),
		(b"key11", &[5; 64]),
	];

	pub fn craft_valid_storage_proof() -> (sp_core::H256, RawStorageProof) {
		let mut db = MemoryDB::default();
		let mut root = Default::default();

		let mut recorder = Recorder::<Layout>::new();
		{
			let mut trie = TrieDBMutBuilder::<Layout>::new(&mut db, &mut root)
				.with_recorder(&mut recorder)
				.build();
			for (k, v) in TEST_DATA {
				trie.insert(k, v).expect("Inserts data");
			}
		}

		(root, recorder.drain().into_iter().map(|record| record.data).collect())
	}

	#[test]
	fn proof_with_duplicate_nodes_is_rejected() {
		let (_root, mut raw_proof) = craft_valid_storage_proof();
		raw_proof.push(raw_proof.first().unwrap().clone());

		assert!(matches!(
			RedundantNodesChecker::<Layout>::new(raw_proof),
			Err(Error::DuplicateNodes)
		));
	}

	#[test]
	fn proof_with_unused_nodes_is_rejected() {
		let (mut root, raw_proof) = craft_valid_storage_proof();

		let (mut redundant_nodes_checker, proof) =
			RedundantNodesChecker::<Layout>::new(raw_proof.clone()).unwrap();
		{
			let mut db = proof.into_memory_db();
			let trie = TrieDBMutBuilder::<Layout>::new(&mut db, &mut root)
				.with_recorder(&mut redundant_nodes_checker)
				.build();

			trie.get(b"key1").unwrap();
			trie.get(b"key2").unwrap();
			trie.get(b"key4").unwrap();
			trie.get(b"key22").unwrap();
		}
		assert_eq!(redundant_nodes_checker.ensure_no_unused_nodes(), Ok(()));

		let (redundant_nodes_checker, _proof) =
			RedundantNodesChecker::<Layout>::new(raw_proof).unwrap();
		assert!(matches!(
			redundant_nodes_checker.ensure_no_unused_nodes(),
			Err(Error::UnusedNodes)
		));
	}
}
