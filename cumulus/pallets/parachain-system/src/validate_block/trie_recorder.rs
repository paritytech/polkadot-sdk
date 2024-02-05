// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Substrate.

// Substrate is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Substrate is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Cumulus. If not, see <http://www.gnu.org/licenses/>.

//! Provide a specialized trie-recorder and provider for use in validate-block.
//!
//! This file defines two main structs, [`SizeOnlyRecorder`] and
//! [`SizeOnlyRecorderProvider`]. They are used to track the current
//! proof-size without actually recording the accessed nodes themselves.

use codec::Encode;

use sp_std::{
	cell::{RefCell, RefMut},
	collections::{btree_map::BTreeMap, btree_set::BTreeSet},
	rc::Rc,
};
use sp_trie::{NodeCodec, ProofSizeProvider, StorageProof};
use trie_db::{Hasher, RecordedForKey, TrieAccess};

/// A trie recorder that only keeps track of the proof size.
///
/// The internal size counting logic should align
/// with ['sp_trie::recorder::Recorder'].
pub(crate) struct SizeOnlyRecorder<'a, H: Hasher> {
	seen_nodes: RefMut<'a, BTreeSet<H::Out>>,
	encoded_size: RefMut<'a, usize>,
	recorded_keys: RefMut<'a, BTreeMap<Rc<[u8]>, RecordedForKey>>,
}

impl<'a, H: trie_db::Hasher> trie_db::TrieRecorder<H::Out> for SizeOnlyRecorder<'a, H> {
	fn record(&mut self, access: TrieAccess<'_, H::Out>) {
		let mut encoded_size_update = 0;
		match access {
			TrieAccess::NodeOwned { hash, node_owned } =>
				if self.seen_nodes.insert(hash) {
					let node = node_owned.to_encoded::<NodeCodec<H>>();
					encoded_size_update += node.encoded_size();
				},
			TrieAccess::EncodedNode { hash, encoded_node } =>
				if self.seen_nodes.insert(hash) {
					encoded_size_update += encoded_node.encoded_size();
				},
			TrieAccess::Value { hash, value, full_key } => {
				if self.seen_nodes.insert(hash) {
					encoded_size_update += value.encoded_size();
				}
				self.recorded_keys
					.entry(full_key.into())
					.and_modify(|e| *e = RecordedForKey::Value)
					.or_insert_with(|| RecordedForKey::Value);
			},
			TrieAccess::Hash { full_key } => {
				self.recorded_keys
					.entry(full_key.into())
					.or_insert_with(|| RecordedForKey::Hash);
			},
			TrieAccess::NonExisting { full_key } => {
				self.recorded_keys
					.entry(full_key.into())
					.and_modify(|e| *e = RecordedForKey::Value)
					.or_insert_with(|| RecordedForKey::Value);
			},
			TrieAccess::InlineValue { full_key } => {
				self.recorded_keys
					.entry(full_key.into())
					.and_modify(|e| *e = RecordedForKey::Value)
					.or_insert_with(|| RecordedForKey::Value);
			},
		};

		*self.encoded_size += encoded_size_update;
	}

	fn trie_nodes_recorded_for_key(&self, key: &[u8]) -> RecordedForKey {
		self.recorded_keys.get(key).copied().unwrap_or(RecordedForKey::None)
	}
}

#[derive(Clone)]
pub(crate) struct SizeOnlyRecorderProvider<H: Hasher> {
	seen_nodes: Rc<RefCell<BTreeSet<H::Out>>>,
	encoded_size: Rc<RefCell<usize>>,
	recorded_keys: Rc<RefCell<BTreeMap<Rc<[u8]>, RecordedForKey>>>,
}

impl<H: Hasher> SizeOnlyRecorderProvider<H> {
	pub fn new() -> Self {
		Self {
			seen_nodes: Default::default(),
			encoded_size: Default::default(),
			recorded_keys: Default::default(),
		}
	}
}

impl<H: trie_db::Hasher> sp_trie::TrieRecorderProvider<H> for SizeOnlyRecorderProvider<H> {
	type Recorder<'a> = SizeOnlyRecorder<'a, H> where H: 'a;

	fn drain_storage_proof(self) -> Option<StorageProof> {
		None
	}

	fn as_trie_recorder(&self, _storage_root: H::Out) -> Self::Recorder<'_> {
		SizeOnlyRecorder {
			encoded_size: self.encoded_size.borrow_mut(),
			seen_nodes: self.seen_nodes.borrow_mut(),
			recorded_keys: self.recorded_keys.borrow_mut(),
		}
	}
}

impl<H: trie_db::Hasher> ProofSizeProvider for SizeOnlyRecorderProvider<H> {
	fn estimate_encoded_size(&self) -> usize {
		*self.encoded_size.borrow()
	}
}

// This is safe here since we are single-threaded in WASM
unsafe impl<H: Hasher> Send for SizeOnlyRecorderProvider<H> {}
unsafe impl<H: Hasher> Sync for SizeOnlyRecorderProvider<H> {}

#[cfg(test)]
mod tests {
	use rand::Rng;
	use sp_trie::{
		cache::{CacheSize, SharedTrieCache},
		MemoryDB, ProofSizeProvider, TrieRecorderProvider,
	};
	use trie_db::{Trie, TrieDBBuilder, TrieDBMutBuilder, TrieHash, TrieMut, TrieRecorder};
	use trie_standardmap::{Alphabet, StandardMap, ValueMode};

	use super::*;

	type Recorder = sp_trie::recorder::Recorder<sp_core::Blake2Hasher>;

	fn create_trie() -> (
		sp_trie::MemoryDB<sp_core::Blake2Hasher>,
		TrieHash<sp_trie::LayoutV1<sp_core::Blake2Hasher>>,
		Vec<(Vec<u8>, Vec<u8>)>,
	) {
		let mut db = MemoryDB::default();
		let mut root = Default::default();

		let mut seed = Default::default();
		let test_data: Vec<(Vec<u8>, Vec<u8>)> = StandardMap {
			alphabet: Alphabet::Low,
			min_key: 16,
			journal_key: 0,
			value_mode: ValueMode::Random,
			count: 1000,
		}
		.make_with(&mut seed)
		.into_iter()
		.map(|(k, v)| {
			// Double the length so we end up with some values of 2 bytes and some of 64
			let v = [v.clone(), v].concat();
			(k, v)
		})
		.collect();

		// Fill database with values
		{
			let mut trie = TrieDBMutBuilder::<sp_trie::LayoutV1<sp_core::Blake2Hasher>>::new(
				&mut db, &mut root,
			)
			.build();
			for (k, v) in &test_data {
				trie.insert(k, v).expect("Inserts data");
			}
		}

		(db, root, test_data)
	}

	#[test]
	fn recorder_equivalence_cache() {
		let (db, root, test_data) = create_trie();

		let mut rng = rand::thread_rng();
		for _ in 1..10 {
			let reference_recorder = Recorder::default();
			let recorder_for_test: SizeOnlyRecorderProvider<sp_core::Blake2Hasher> =
				SizeOnlyRecorderProvider::new();
			let reference_cache: SharedTrieCache<sp_core::Blake2Hasher> =
				SharedTrieCache::new(CacheSize::new(1024 * 5));
			let cache_for_test: SharedTrieCache<sp_core::Blake2Hasher> =
				SharedTrieCache::new(CacheSize::new(1024 * 5));
			{
				let local_cache = cache_for_test.local_cache();
				let mut trie_cache_for_reference = local_cache.as_trie_db_cache(root);
				let mut reference_trie_recorder = reference_recorder.as_trie_recorder(root);
				let reference_trie =
					TrieDBBuilder::<sp_trie::LayoutV1<sp_core::Blake2Hasher>>::new(&db, &root)
						.with_recorder(&mut reference_trie_recorder)
						.with_cache(&mut trie_cache_for_reference)
						.build();

				let local_cache_for_test = reference_cache.local_cache();
				let mut trie_cache_for_test = local_cache_for_test.as_trie_db_cache(root);
				let mut trie_recorder_under_test = recorder_for_test.as_trie_recorder(root);
				let test_trie =
					TrieDBBuilder::<sp_trie::LayoutV1<sp_core::Blake2Hasher>>::new(&db, &root)
						.with_recorder(&mut trie_recorder_under_test)
						.with_cache(&mut trie_cache_for_test)
						.build();

				// Access random values from the test data
				for _ in 0..100 {
					let index: usize = rng.gen_range(0..test_data.len());
					test_trie.get(&test_data[index].0).unwrap().unwrap();
					reference_trie.get(&test_data[index].0).unwrap().unwrap();
				}

				// Check that we have the same nodes recorded for both recorders
				for (key, _) in test_data.iter() {
					let reference = reference_trie_recorder.trie_nodes_recorded_for_key(key);
					let test_value = trie_recorder_under_test.trie_nodes_recorded_for_key(key);
					assert_eq!(format!("{:?}", reference), format!("{:?}", test_value));
				}
			}

			// Check that we have the same size recorded for both recorders
			assert_eq!(
				reference_recorder.estimate_encoded_size(),
				recorder_for_test.estimate_encoded_size()
			);
		}
	}

	#[test]
	fn recorder_equivalence_no_cache() {
		let (db, root, test_data) = create_trie();

		let mut rng = rand::thread_rng();
		for _ in 1..10 {
			let reference_recorder = Recorder::default();
			let recorder_for_test: SizeOnlyRecorderProvider<sp_core::Blake2Hasher> =
				SizeOnlyRecorderProvider::new();
			{
				let mut reference_trie_recorder = reference_recorder.as_trie_recorder(root);
				let reference_trie =
					TrieDBBuilder::<sp_trie::LayoutV1<sp_core::Blake2Hasher>>::new(&db, &root)
						.with_recorder(&mut reference_trie_recorder)
						.build();

				let mut trie_recorder_under_test = recorder_for_test.as_trie_recorder(root);
				let test_trie =
					TrieDBBuilder::<sp_trie::LayoutV1<sp_core::Blake2Hasher>>::new(&db, &root)
						.with_recorder(&mut trie_recorder_under_test)
						.build();

				for _ in 0..200 {
					let index: usize = rng.gen_range(0..test_data.len());
					test_trie.get(&test_data[index].0).unwrap().unwrap();
					reference_trie.get(&test_data[index].0).unwrap().unwrap();
				}

				// Check that we have the same nodes recorded for both recorders
				for (key, _) in test_data.iter() {
					let reference = reference_trie_recorder.trie_nodes_recorded_for_key(key);
					let test_value = trie_recorder_under_test.trie_nodes_recorded_for_key(key);
					assert_eq!(format!("{:?}", reference), format!("{:?}", test_value));
				}
			}

			// Check that we have the same size recorded for both recorders
			assert_eq!(
				reference_recorder.estimate_encoded_size(),
				recorder_for_test.estimate_encoded_size()
			);
		}
	}
}
