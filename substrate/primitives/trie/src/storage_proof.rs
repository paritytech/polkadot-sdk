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

use alloc::{collections::btree_set::BTreeSet, vec::Vec};
use codec::{Decode, DecodeWithMemTracking, Encode};
use core::iter::{DoubleEndedIterator, IntoIterator};
use hash_db::{HashDB, Hasher};
use scale_info::TypeInfo;

// Note that `LayoutV1` usage here (proof compaction) is compatible
// with `LayoutV0`.
use crate::LayoutV1 as Layout;

/// Error associated with the `storage_proof` module.
#[derive(Encode, Decode, Clone, Eq, PartialEq, Debug, TypeInfo)]
pub enum StorageProofError {
	/// The proof contains duplicate nodes.
	DuplicateNodes,
}

/// A proof that some set of key-value pairs are included in the storage trie. The proof contains
/// the storage values so that the partial storage backend can be reconstructed by a verifier that
/// does not already have access to the key-value pairs.
///
/// The proof consists of the set of serialized nodes in the storage trie accessed when looking up
/// the keys covered by the proof. Verifying the proof requires constructing the partial trie from
/// the serialized nodes and performing the key lookups.
#[derive(Debug, PartialEq, Eq, Clone, Encode, Decode, DecodeWithMemTracking, TypeInfo)]
pub struct StorageProof {
	trie_nodes: BTreeSet<Vec<u8>>,
}

impl StorageProof {
	/// Constructs a storage proof from a subset of encoded trie nodes in a storage backend.
	pub fn new(trie_nodes: impl IntoIterator<Item = Vec<u8>>) -> Self {
		StorageProof { trie_nodes: BTreeSet::from_iter(trie_nodes) }
	}

	/// Constructs a storage proof from a subset of encoded trie nodes in a storage backend.
	///
	/// Returns an error if the provided subset of encoded trie nodes contains duplicates.
	pub fn new_with_duplicate_nodes_check(
		trie_nodes: impl IntoIterator<Item = Vec<u8>>,
	) -> Result<Self, StorageProofError> {
		let mut trie_nodes_set = BTreeSet::new();
		for node in trie_nodes {
			if !trie_nodes_set.insert(node) {
				return Err(StorageProofError::DuplicateNodes);
			}
		}

		Ok(StorageProof { trie_nodes: trie_nodes_set })
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

	/// Returns the number of nodes in the proof.
	pub fn len(&self) -> usize {
		self.trie_nodes.len()
	}

	/// Convert into an iterator over encoded trie nodes in lexicographical order constructed
	/// from the proof.
	pub fn into_iter_nodes(self) -> impl Sized + DoubleEndedIterator<Item = Vec<u8>> {
		self.trie_nodes.into_iter()
	}

	/// Create an iterator over encoded trie nodes in lexicographical order constructed
	/// from the proof.
	pub fn iter_nodes(&self) -> impl Sized + DoubleEndedIterator<Item = &Vec<u8>> {
		self.trie_nodes.iter()
	}

	/// Convert into plain node vector.
	pub fn into_nodes(self) -> BTreeSet<Vec<u8>> {
		self.trie_nodes
	}

	/// Creates a [`MemoryDB`](crate::MemoryDB) from `Self`.
	pub fn into_memory_db<H: Hasher>(self) -> crate::MemoryDB<H> {
		self.into()
	}

	/// Creates a [`MemoryDB`](crate::MemoryDB) from `Self` reference.
	pub fn to_memory_db<H: Hasher>(&self) -> crate::MemoryDB<H> {
		self.into()
	}

	/// Merges multiple storage proofs covering potentially different sets of keys into one proof
	/// covering all keys. The merged proof output may be smaller than the aggregate size of the
	/// input proofs due to deduplication of trie nodes.
	pub fn merge(proofs: impl IntoIterator<Item = Self>) -> Self {
		let trie_nodes = proofs
			.into_iter()
			.flat_map(|proof| proof.into_iter_nodes())
			.collect::<BTreeSet<_>>()
			.into_iter()
			.collect();

		Self { trie_nodes }
	}

	/// Encode as a compact proof with default trie layout.
	pub fn into_compact_proof<H: Hasher>(
		self,
		root: H::Out,
	) -> Result<CompactProof, crate::CompactProofError<H::Out, crate::Error<H::Out>>> {
		let db = self.into_memory_db();
		crate::encode_compact::<Layout<H>, crate::MemoryDB<H>>(&db, &root)
	}

	/// Encode as a compact proof with default trie layout.
	pub fn to_compact_proof<H: Hasher>(
		&self,
		root: H::Out,
	) -> Result<CompactProof, crate::CompactProofError<H::Out, crate::Error<H::Out>>> {
		let db = self.to_memory_db();
		crate::encode_compact::<Layout<H>, crate::MemoryDB<H>>(&db, &root)
	}

	/// Returns the estimated encoded size of the compact proof.
	///
	/// Running this operation is a slow operation (build the whole compact proof) and should only
	/// be in non sensitive path.
	///
	/// Return `None` on error.
	pub fn encoded_compact_size<H: Hasher>(self, root: H::Out) -> Option<usize> {
		let compact_proof = self.into_compact_proof::<H>(root);
		compact_proof.ok().map(|p| p.encoded_size())
	}
}

impl<H: Hasher> From<StorageProof> for crate::MemoryDB<H> {
	fn from(proof: StorageProof) -> Self {
		From::from(&proof)
	}
}

impl<H: Hasher> From<&StorageProof> for crate::MemoryDB<H> {
	fn from(proof: &StorageProof) -> Self {
		let mut db = crate::MemoryDB::with_hasher(crate::RandomState::default());
		proof.iter_nodes().for_each(|n| {
			db.insert(crate::EMPTY_PREFIX, &n);
		});
		db
	}
}

/// Storage proof in compact form.
#[derive(Debug, PartialEq, Eq, Clone, Encode, Decode, DecodeWithMemTracking, TypeInfo)]
pub struct CompactProof {
	pub encoded_nodes: Vec<Vec<u8>>,
}

impl CompactProof {
	/// Return an iterator on the compact encoded nodes.
	pub fn iter_compact_encoded_nodes(&self) -> impl Iterator<Item = &[u8]> {
		self.encoded_nodes.iter().map(Vec::as_slice)
	}

	/// Decode to a full storage_proof.
	pub fn to_storage_proof<H: Hasher>(
		&self,
		expected_root: Option<&H::Out>,
	) -> Result<(StorageProof, H::Out), crate::CompactProofError<H::Out, crate::Error<H::Out>>> {
		let mut db = crate::MemoryDB::<H>::new(&[]);
		let root = crate::decode_compact::<Layout<H>, _, _>(
			&mut db,
			self.iter_compact_encoded_nodes(),
			expected_root,
		)?;
		Ok((
			StorageProof::new(db.drain().into_iter().filter_map(|kv| {
				if (kv.1).1 > 0 {
					Some((kv.1).0)
				} else {
					None
				}
			})),
			root,
		))
	}

	/// Convert self into a [`MemoryDB`](crate::MemoryDB).
	///
	/// `expected_root` is the expected root of this compact proof.
	///
	/// Returns the memory db and the root of the trie.
	pub fn to_memory_db<H: Hasher>(
		&self,
		expected_root: Option<&H::Out>,
	) -> Result<(crate::MemoryDB<H>, H::Out), crate::CompactProofError<H::Out, crate::Error<H::Out>>>
	{
		let mut db = crate::MemoryDB::<H>::new(&[]);
		let root = crate::decode_compact::<Layout<H>, _, _>(
			&mut db,
			self.iter_compact_encoded_nodes(),
			expected_root,
		)?;

		Ok((db, root))
	}
}

#[cfg(test)]
pub mod tests {
	use super::*;
	use crate::{tests::create_storage_proof, StorageProof};

	type Hasher = sp_core::Blake2Hasher;
	type Layout = crate::LayoutV1<Hasher>;

	const TEST_DATA: &[(&[u8], &[u8])] =
		&[(b"key1", &[1; 64]), (b"key2", &[2; 64]), (b"key3", &[3; 64]), (b"key11", &[4; 64])];

	#[test]
	fn proof_with_duplicate_nodes_is_rejected() {
		let (raw_proof, _root) = create_storage_proof::<Layout>(TEST_DATA);
		assert!(matches!(
			StorageProof::new_with_duplicate_nodes_check(raw_proof),
			Err(StorageProofError::DuplicateNodes)
		));
	}

	#[test]
	fn invalid_compact_proof_does_not_panic_when_decoding() {
		let invalid_proof = CompactProof { encoded_nodes: vec![vec![135]] };
		let result = invalid_proof.to_memory_db::<Hasher>(None);
		assert!(result.is_err());
	}

	// Demonstrates https://github.com/paritytech/polkadot-sdk/issues/12565.
	//
	// The `Recorder`'s `estimate_encoded_size()` (which state sync uses as the cutoff for how much
	// to serve, see `prove_range_read_with_child_with_size_on_trie_backend`) reflects the size of
	// the *deduplicated* set of accessed trie nodes. A value >= `TRIE_VALUE_NODE_THRESHOLD` is
	// stored in `LayoutV1` as a separate, hash-addressed node, so many keys sharing the same value
	// all reference the *same* node and it is counted only once.
	//
	// The bytes actually sent over the wire, however, are a `CompactProof`, and `encode_compact`
	// re-emits that shared value once *per referencing leaf*. So the estimate can massively
	// under-report the real payload size, which is exactly how a "2 MiB" proof becomes a 26 MiB
	// response that exceeds the 16 MiB request-response limit.
	#[test]
	fn compact_proof_duplicates_shared_values_beyond_the_estimate() {
		use crate::{recorder::Recorder, MemoryDB};
		use codec::Encode;
		use trie_db::{Trie, TrieDBBuilder, TrieDBMutBuilder, TrieMut};

		// Many distinct keys, all mapping to the *same* large value. `1024 >= 33` so in `LayoutV1`
		// the value is stored as a separate, hash-addressed node; identical values across keys
		// therefore all reference the *same* value node.
		//
		// The keys are hashed so they spread across the key space, mirroring a real state trie
		// (e.g. `Blake2_128Concat`-keyed storage): every key ends in its own, separately-hashed
		// leaf node rather than collapsing into a few branches with inlined leaves. This keeps the
		// blow-up attributable purely to the duplicated *value*.
		const NUM_KEYS: u32 = 2_000;
		let shared_value = vec![0xaa_u8; 1024];
		let data: Vec<(Vec<u8>, Vec<u8>)> = (0..NUM_KEYS)
			.map(|i| {
				let key = <sp_core::Blake2Hasher as hash_db::Hasher>::hash(&i.to_le_bytes());
				(key.0.to_vec(), shared_value.clone())
			})
			.collect();

		// Build the trie.
		let mut db = MemoryDB::<Hasher>::default();
		let mut root = Default::default();
		{
			let mut trie = TrieDBMutBuilder::<Layout>::new(&mut db, &mut root).build();
			for (k, v) in &data {
				trie.insert(k, v).unwrap();
			}
		}

		// Record a read of every key, mirroring what the state-sync serving node does while
		// collecting a range of the trie.
		let recorder = Recorder::<Hasher>::default();
		{
			let mut trie_recorder = recorder.as_trie_recorder(root);
			let trie =
				TrieDBBuilder::<Layout>::new(&db, &root).with_recorder(&mut trie_recorder).build();
			for (k, _) in &data {
				assert_eq!(trie.get(k).unwrap().as_ref(), Some(&shared_value));
			}
		}

		// (1) The value used as the serving cutoff in
		//     `prove_range_read_with_child_with_size_on_trie_backend`. It is derived from the
		//     *deduplicated* set of accessed nodes, so the 1 KiB shared value is counted once.
		let estimated_size = recorder.estimate_encoded_size();

		// (2) The deduplicated `StorageProof` (a `BTreeSet`) matches the estimate up to the outer
		//     length prefix, confirming the estimate really is the deduplicated size. The shared
		//     value is present exactly once.
		let proof = recorder.to_storage_proof();
		let storage_proof_size = proof.encoded_size();
		assert!(
			estimated_size.abs_diff(storage_proof_size) < 1024,
			"estimate ({estimated_size}) should track the deduplicated storage proof size \
			 ({storage_proof_size})",
		);
		let dedup_value_copies =
			proof.iter_nodes().filter(|n| n.as_slice() == shared_value.as_slice()).count();
		assert_eq!(dedup_value_copies, 1, "the deduplicated proof holds the shared value once");

		// (3) The `CompactProof` is what is actually sent on the wire. `encode_compact` re-emits
		//     the shared value once per referencing leaf.
		let compact = proof.into_compact_proof::<Hasher>(root).unwrap();
		let compact_size = compact.encoded_size();
		let compact_value_copies =
			compact.encoded_nodes.iter().filter(|n| n.as_slice() == shared_value.as_slice()).count();

		eprintln!(
			"estimate={estimated_size} storage_proof={storage_proof_size} compact={compact_size} \
			 ratio={:.1}x value_copies_on_wire={compact_value_copies}",
			compact_size as f64 / estimated_size as f64,
		);

		// The same value that appears once in the deduplicated proof appears once *per leaf* on the
		// wire — that is the whole blow-up.
		assert_eq!(
			compact_value_copies, NUM_KEYS as usize,
			"the shared value should appear once per leaf in the compact proof",
		);

		// ...even though the compact proof still round-trips to the exact same root: the
		// duplication carries no extra information — the verifier rebuilds a deduplicated db.
		let (_db, decoded_root) = compact.to_memory_db::<Hasher>(Some(&root)).unwrap();
		assert_eq!(decoded_root, root);

		// The crux of #12565: the cutoff (deduplicated estimate) measures something far smaller than
		// the payload it actually produces. The wire payload is dominated by the duplicated value
		// and dwarfs the estimate (~9.5x here; ~13x in the real asset-hub-kusama report).
		assert!(
			compact_size > NUM_KEYS as usize * shared_value.len(),
			"compact proof ({compact_size}) should be dominated by ~{NUM_KEYS} value copies",
		);
		assert!(
			compact_size > 5 * estimated_size,
			"compact proof ({compact_size}) dwarfs the estimated/deduplicated size \
			 ({estimated_size}); the serving cutoff is measuring the wrong thing",
		);
	}
}
