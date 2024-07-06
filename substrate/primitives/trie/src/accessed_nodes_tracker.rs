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

//! Helpers for checking for duplicate nodes.

use alloc::collections::BTreeSet;
use core::hash::Hash;
use scale_info::TypeInfo;
use sp_core::{Decode, Encode};
use trie_db::{RecordedForKey, TrieAccess, TrieRecorder};

/// Error associated with the `AccessedNodesTracker` module.
#[derive(Encode, Decode, Clone, Eq, PartialEq, Debug, TypeInfo)]
pub enum Error {
	/// The proof contains unused nodes.
	UnusedNodes,
}

/// Helper struct used to ensure that a storage proof doesn't contain duplicate or unused nodes.
///
/// The struct needs to be used as a `TrieRecorder` and `ensure_no_unused_nodes()` has to be called
/// to actually perform the check.
pub struct AccessedNodesTracker<H: Hash> {
	proof_nodes_count: usize,
	recorder: BTreeSet<H>,
}

impl<H: Hash> AccessedNodesTracker<H> {
	/// Create a new instance of `RedundantNodesChecker`, starting from a `RawStorageProof`.
	pub fn new(proof_nodes_count: usize) -> Self {
		Self { proof_nodes_count, recorder: BTreeSet::new() }
	}

	/// Ensure that all the nodes in the proof have been accessed.
	pub fn ensure_no_unused_nodes(self) -> Result<(), Error> {
		if self.proof_nodes_count != self.recorder.len() {
			return Err(Error::UnusedNodes)
		}

		Ok(())
	}
}

impl<H: Hash + Ord> TrieRecorder<H> for AccessedNodesTracker<H> {
	fn record(&mut self, access: TrieAccess<H>) {
		match access {
			TrieAccess::NodeOwned { hash, .. } |
			TrieAccess::EncodedNode { hash, .. } |
			TrieAccess::Value { hash, .. } => {
				self.recorder.insert(hash);
			},
			_ => {},
		}
	}

	fn trie_nodes_recorded_for_key(&self, _key: &[u8]) -> RecordedForKey {
		RecordedForKey::None
	}
}

#[cfg(test)]
pub mod tests {
	use super::*;
	use crate::{tests::create_storage_proof, StorageProof};
	use hash_db::Hasher;
	use trie_db::{Trie, TrieDBBuilder};

	type Hash = <sp_core::Blake2Hasher as Hasher>::Out;
	type Layout = crate::LayoutV1<sp_core::Blake2Hasher>;

	const TEST_DATA: &[(&[u8], &[u8])] =
		&[(b"key1", &[1; 64]), (b"key2", &[2; 64]), (b"key3", &[3; 64])];

	#[test]
	fn proof_with_unused_nodes_is_rejected() {
		let (raw_proof, root) = create_storage_proof::<Layout>(TEST_DATA);
		let proof = StorageProof::new(raw_proof.clone());
		let proof_nodes_count = proof.len();

		let mut accessed_nodes_tracker = AccessedNodesTracker::<Hash>::new(proof_nodes_count);
		{
			let db = proof.clone().into_memory_db();
			let trie = TrieDBBuilder::<Layout>::new(&db, &root)
				.with_recorder(&mut accessed_nodes_tracker)
				.build();

			trie.get(b"key1").unwrap().unwrap();
			trie.get(b"key2").unwrap().unwrap();
			trie.get(b"key3").unwrap().unwrap();
		}
		assert_eq!(accessed_nodes_tracker.ensure_no_unused_nodes(), Ok(()));

		let mut accessed_nodes_tracker = AccessedNodesTracker::<Hash>::new(proof_nodes_count);
		{
			let db = proof.into_memory_db();
			let trie = TrieDBBuilder::<Layout>::new(&db, &root)
				.with_recorder(&mut accessed_nodes_tracker)
				.build();

			trie.get(b"key1").unwrap().unwrap();
			trie.get(b"key2").unwrap().unwrap();
		}
		assert_eq!(accessed_nodes_tracker.ensure_no_unused_nodes(), Err(Error::UnusedNodes));
	}
}
