// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Cumulus.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

// Cumulus is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Cumulus is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Cumulus. If not, see <https://www.gnu.org/licenses/>.

//! Per-block proof store for unincluded segment resubmission.
//!
//! Persists `(time, StorageProof)` per imported parablock, keyed by parablock hash. Data is
//! pruned on parachain finality.

use codec::{Decode, Encode};
use cumulus_client_consensus_common::old_finalized_hash;
use sc_client_api::{
	backend::AuxStore,
	client::{AuxDataOperations, FinalityNotification, PreCommitActions},
	HeaderBackend,
};
use sp_blockchain::{Error as ClientError, Result as ClientResult};
use sp_runtime::traits::{Block as BlockT, Header as _};
use sp_trie::StorageProof;
use std::{
	marker::PhantomData,
	sync::Arc,
	time::{SystemTime, UNIX_EPOCH},
};

const STORE_VERSION_KEY: &[u8] = b"cumulus_unincluded_segment_store_version";
const STORE_CURRENT_VERSION: u32 = 1;
const STORE_ENTRY_PREFIX: &[u8] = b"cumulus_unincluded_segment_store";

/// Return the current Unix milliseconds timestamp.
pub fn now_unix_ms() -> u64 {
	SystemTime::now()
		.duration_since(UNIX_EPOCH)
		.expect("system clock is before UNIX epoch; qed")
		.as_millis() as u64
}

/// Entry stored in aux storage for each unincluded parablock.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct StoredEntry {
	/// Unix millis at the moment block_import started.
	pub time_ms: u64,
	/// The storage proof captured at block_import.
	pub proof: StorageProof,
}

fn entry_key<H: Encode>(block_hash: H) -> Vec<u8> {
	(STORE_ENTRY_PREFIX, block_hash).encode()
}

/// Per-block proof store backed by `AuxStore`.
pub struct UnincludedSegmentStore<Block: BlockT, B> {
	backend: Arc<B>,
	_marker: PhantomData<fn() -> Block>,
}

impl<Block: BlockT, B> Clone for UnincludedSegmentStore<Block, B> {
	fn clone(&self) -> Self {
		Self { backend: self.backend.clone(), _marker: PhantomData }
	}
}

impl<Block: BlockT, B> UnincludedSegmentStore<Block, B> {
	/// Create a new store over `backend`.
	pub fn new(backend: Arc<B>) -> Self {
		Self { backend, _marker: PhantomData }
	}
}

/// Build the aux-data key/value pairs to commit alongside a block.
///
/// The caller should push these into `BlockImportParams::auxiliary` so they commit in the
/// same DB transaction as the block. Stateless — no backend access required.
pub fn prepare_aux_data<Block: BlockT>(
	block_hash: Block::Hash,
	time_ms: u64,
	proof: &StorageProof,
) -> impl Iterator<Item = (Vec<u8>, Vec<u8>)> {
	let encoded_entry = (time_ms, proof).encode();
	let encoded_version = STORE_CURRENT_VERSION.encode();

	[(entry_key(block_hash), encoded_entry), (STORE_VERSION_KEY.to_vec(), encoded_version)]
		.into_iter()
}

impl<Block: BlockT, B: AuxStore> UnincludedSegmentStore<Block, B> {
	/// Load the entry stored for `block_hash`, if any.
	pub fn load(&self, block_hash: Block::Hash) -> ClientResult<Option<StoredEntry>> {
		let version = self.decode_aux::<u32>(STORE_VERSION_KEY)?;

		match version {
			None => Ok(None),
			Some(STORE_CURRENT_VERSION) => self.decode_aux(entry_key(block_hash).as_slice()),
			Some(other) => Err(ClientError::Backend(format!(
				"Unsupported unincluded segment store DB version: {:?}",
				other
			))),
		}
	}

	fn decode_aux<T: Decode>(&self, key: &[u8]) -> ClientResult<Option<T>> {
		match self.backend.get_aux(key)? {
			None => Ok(None),
			Some(t) => T::decode(&mut &t[..]).map(Some).map_err(|e| {
				ClientError::Backend(format!(
					"Unincluded segment store DB is corrupted. Decode error: {}",
					e
				))
			}),
		}
	}
}

impl<Block, B> UnincludedSegmentStore<Block, B>
where
	Block: BlockT,
	B: PreCommitActions<Block> + HeaderBackend<Block> + 'static,
{
	/// Register a finality hook that prunes entries for the just-finalized chain, the
	/// intermediate tree route, the prior finalized head, and any stale forks.
	///
	/// TODO(#12034): also prune entries whose relay-parent session is older than
	/// `max_relay_parent_session_age` relative to the current session. Session info will be
	/// sourced from a separate session-data cache (see #11624), not from per-entry storage.
	pub fn register_cleanup(&self) {
		let client = self.backend.clone();
		let on_finality = move |notification: &FinalityNotification<Block>| -> AuxDataOperations {
			let old_finalized = old_finalized_hash::<_, Block>(
				&*client,
				&notification.tree_route,
				*notification.header.parent_hash(),
			);

			finality_cleanup_ops::<Block>(
				notification.hash,
				old_finalized,
				&notification.tree_route,
				notification.stale_blocks.iter().map(|b| b.hash),
			)
		};

		self.backend.register_finality_action(Box::new(on_finality));
	}
}

/// Compute aux storage cleanup operations.
///
/// Emits deletes for stale-fork blocks, intermediate tree-route blocks, the pre-finality head,
/// and the just-finalized block itself. Once a block is finalized it is no longer in any
/// unincluded segment, so its proof entry is dead weight.
fn finality_cleanup_ops<Block: BlockT>(
	just_finalized_hash: Block::Hash,
	old_finalized_hash: Block::Hash,
	tree_route: &[Block::Hash],
	stale_block_hashes: impl IntoIterator<Item = Block::Hash>,
) -> AuxDataOperations {
	let stale_iter = stale_block_hashes.into_iter();

	let mut ops = Vec::with_capacity(stale_iter.size_hint().0 + tree_route.len() + 2);
	ops.extend(stale_iter.map(|hash| (entry_key(hash), None)));
	ops.extend(tree_route.iter().map(|hash| (entry_key(hash), None)));
	ops.push((entry_key(old_finalized_hash), None));
	ops.push((entry_key(just_finalized_hash), None));

	ops
}

#[cfg(test)]
mod tests {
	use super::*;
	use sc_client_api::backend::AuxStore;

	type Block = substrate_test_runtime::Block;
	type Hash = <Block as BlockT>::Hash;
	type TestBackend = sc_client_api::in_mem::Backend<Block>;
	type Store = UnincludedSegmentStore<Block, TestBackend>;

	fn create_test_entry(time_ms: u64) -> StoredEntry {
		StoredEntry { time_ms, proof: StorageProof::new(vec![vec![1, 2, 3], vec![4, 5, 6]]) }
	}

	fn new_store() -> (Arc<TestBackend>, Store) {
		let backend = Arc::new(TestBackend::new());
		let store = Store::new(backend.clone());
		(backend, store)
	}

	fn write_via_store(backend: &Arc<TestBackend>, hash: Hash, entry: &StoredEntry) {
		let pairs: Vec<_> = prepare_aux_data::<Block>(hash, entry.time_ms, &entry.proof).collect();
		let insert_pairs: Vec<_> =
			pairs.iter().map(|(k, v)| (k.as_slice(), v.as_slice())).collect();
		AuxStore::insert_aux(&**backend, &insert_pairs, &[]).expect("aux insert should succeed");
	}

	#[test]
	fn prepare_produces_expected_key_value_pairs() {
		let hash = Hash::repeat_byte(0xAB);
		let time_ms = 1234567890u64;
		let proof = StorageProof::new(vec![vec![10, 20, 30]]);

		let pairs: Vec<_> = prepare_aux_data::<Block>(hash, time_ms, &proof).collect();

		assert_eq!(pairs.len(), 2);

		let expected_key = (STORE_ENTRY_PREFIX, hash).encode();
		assert_eq!(pairs[0].0, expected_key);

		let decoded_entry =
			StoredEntry::decode(&mut pairs[0].1.as_slice()).expect("entry should decode");
		assert_eq!(decoded_entry.time_ms, time_ms);
		assert_eq!(decoded_entry.proof, proof);

		assert_eq!(pairs[1].0, STORE_VERSION_KEY.to_vec());
		let decoded_version =
			u32::decode(&mut pairs[1].1.as_slice()).expect("version should decode");
		assert_eq!(decoded_version, STORE_CURRENT_VERSION);
	}

	#[test]
	fn load_returns_none_when_no_entry_exists() {
		let (_backend, store) = new_store();
		let hash = Hash::repeat_byte(0xEF);

		assert_eq!(store.load(hash).expect("load should succeed"), None);
	}

	#[test]
	fn cleanup_combines_all_categories() {
		let stale_1 = Hash::repeat_byte(0xAA);
		let stale_2 = Hash::repeat_byte(0xBB);
		let route_1 = Hash::repeat_byte(0xC1);
		let route_2 = Hash::repeat_byte(0xC2);
		let old_finalized = Hash::repeat_byte(0xF0);
		let just_finalized = Hash::repeat_byte(0xFF);

		let ops = finality_cleanup_ops::<Block>(
			just_finalized,
			old_finalized,
			&[route_1, route_2],
			[stale_1, stale_2],
		);

		let keys: Vec<_> = ops.iter().map(|(k, _)| k.clone()).collect();

		assert!(keys.contains(&entry_key(stale_1)));
		assert!(keys.contains(&entry_key(stale_2)));
		assert!(keys.contains(&entry_key(route_1)));
		assert!(keys.contains(&entry_key(route_2)));
		assert!(keys.contains(&entry_key(old_finalized)));
		assert!(keys.contains(&entry_key(just_finalized)));

		assert!(ops.iter().all(|(_, v)| v.is_none()));
	}

	#[test]
	fn cleanup_handles_empty_inputs() {
		let just_finalized = Hash::repeat_byte(0xFF);
		let old_finalized = Hash::repeat_byte(0xF0);

		let ops = finality_cleanup_ops::<Block>(
			just_finalized,
			old_finalized,
			&[],
			std::iter::empty::<Hash>(),
		);

		assert_eq!(ops.len(), 2);
		assert!(ops.iter().all(|(_, v)| v.is_none()));
	}

	#[test]
	fn stored_entry_encoding_hex_snapshot() {
		let entry =
			StoredEntry { time_ms: 1234567890u64, proof: StorageProof::new(vec![vec![1, 2, 3]]) };

		let encoded = entry.encode();
		// Snapshot of the SCALE encoding. If this assertion fires, the on-disk format of
		// `StoredEntry` has changed and existing aux entries written by older builds will fail
		// to decode — bump `STORE_CURRENT_VERSION` and add a migration before updating this
		// snapshot.
		let encoded_hex = hex::encode(&encoded);
		// time_ms = 1234567890 little-endian u64 = d2 02 96 49 00 00 00 00
		// proof   = Vec<Vec<u8>> with one element [1,2,3]: outer len 1 (SCALE compact = 04),
		//           inner len 3 (compact = 0c), bytes 01 02 03
		let expected_hex = "d20296490000000004 0c 010203".replace(' ', "");
		assert_eq!(encoded_hex, expected_hex, "StoredEntry encoding changed!");

		let decoded = StoredEntry::decode(&mut encoded.as_slice()).expect("decode should succeed");
		assert_eq!(entry, decoded);
	}

	#[test]
	fn decode_corrupted_entry_body() {
		let (backend, store) = new_store();
		let hash = Hash::repeat_byte(0xAB);

		// Write correct version.
		let version_encoded = STORE_CURRENT_VERSION.encode();
		AuxStore::insert_aux(&*backend, &[(STORE_VERSION_KEY, version_encoded.as_slice())], &[])
			.expect("aux insert should succeed");

		// Write bogus entry body.
		let key = entry_key(hash);
		let bogus_data = vec![0xFF, 0xAA, 0xBB];
		AuxStore::insert_aux(&*backend, &[(&key[..], bogus_data.as_slice())], &[])
			.expect("aux insert should succeed");

		let result = store.load(hash);
		assert!(result.is_err());
		let err_msg = result.unwrap_err().to_string();
		assert!(
			err_msg.contains("DB is corrupted") && err_msg.contains("Decode error"),
			"unexpected error: {}",
			err_msg
		);
	}

	#[test]
	fn end_to_end_write_cleanup_load() {
		let (backend, store) = new_store();

		let hash1 = Hash::repeat_byte(0x01);
		let hash2 = Hash::repeat_byte(0x02);
		let hash3 = Hash::repeat_byte(0x03);

		let entry1 = create_test_entry(1000);
		let entry2 = create_test_entry(2000);
		let entry3 = create_test_entry(3000);

		write_via_store(&backend, hash1, &entry1);
		write_via_store(&backend, hash2, &entry2);
		write_via_store(&backend, hash3, &entry3);

		assert_eq!(store.load(hash1).expect("load"), Some(entry1));
		assert_eq!(store.load(hash2).expect("load"), Some(entry2));
		assert_eq!(store.load(hash3).expect("load"), Some(entry3.clone()));

		// Generate cleanup that deletes hash1 (just-finalized) and hash2 (in tree route).
		let ops = finality_cleanup_ops::<Block>(
			hash1,
			Hash::repeat_byte(0xF0),
			&[hash2],
			std::iter::empty::<Hash>(),
		);
		let delete_keys: Vec<_> =
			ops.iter().filter_map(|(k, v)| v.is_none().then(|| k.as_slice())).collect();

		AuxStore::insert_aux(&*backend, &[], &delete_keys).expect("delete should succeed");

		assert_eq!(store.load(hash1).expect("load"), None, "hash1 should be deleted");
		assert_eq!(store.load(hash2).expect("load"), None, "hash2 should be deleted");
		assert_eq!(store.load(hash3).expect("load"), Some(entry3), "hash3 should survive");
	}

	#[test]
	fn entries_survive_disk_restart() {
		use sc_client_db::{
			Backend as DbBackend, BlocksPruning, DatabaseSettings, DatabaseSource, PruningMode,
		};

		fn with_backend<R>(
			path: &std::path::Path,
			f: impl FnOnce(&Arc<DbBackend<Block>>) -> R,
		) -> R {
			let backend = Arc::new(
				DbBackend::<Block>::new(
					DatabaseSettings {
						trie_cache_maximum_size: Some(16 * 1024 * 1024),
						state_pruning: Some(PruningMode::ArchiveAll),
						blocks_pruning: BlocksPruning::KeepAll,
						pruning_filters: Default::default(),
						source: DatabaseSource::ParityDb { path: path.to_path_buf() },
						metrics_registry: None,
					},
					0,
				)
				.expect("open backend"),
			);
			let result = f(&backend);
			// `backend` (and any clones held by the closure) drop here, closing parity-db.
			result
		}

		let tmp = tempfile::tempdir().expect("tempdir");
		let path = tmp.path();

		let hash_a = Hash::repeat_byte(0x10);
		let hash_b = Hash::repeat_byte(0x20);
		let entry_a = create_test_entry(10_000);
		let entry_b = create_test_entry(20_000);

		// Write `a` and `b` via the same path block-import uses, then close.
		with_backend(path, |backend| {
			let pairs: Vec<_> = prepare_aux_data::<Block>(hash_a, entry_a.time_ms, &entry_a.proof)
				.chain(prepare_aux_data::<Block>(hash_b, entry_b.time_ms, &entry_b.proof))
				.collect();
			let refs: Vec<_> = pairs.iter().map(|(k, v)| (k.as_slice(), v.as_slice())).collect();
			AuxStore::insert_aux(&**backend, &refs, &[]).expect("aux insert");
		});

		// Restart: confirm both entries survived, then apply a finality-style delete of `a`.
		with_backend(path, |backend| {
			let store = UnincludedSegmentStore::<Block, _>::new(backend.clone());
			assert_eq!(store.load(hash_a).expect("load a"), Some(entry_a.clone()));
			assert_eq!(store.load(hash_b).expect("load b"), Some(entry_b.clone()));

			let ops = finality_cleanup_ops::<Block>(
				hash_a,
				Hash::repeat_byte(0xF0),
				&[],
				std::iter::empty::<Hash>(),
			);
			let delete_keys: Vec<_> =
				ops.iter().filter_map(|(k, v)| v.is_none().then(|| k.as_slice())).collect();
			AuxStore::insert_aux(&**backend, &[], &delete_keys).expect("delete");
		});

		// Restart: the delete must have persisted; `b` must still be there.
		with_backend(path, |backend| {
			let store = UnincludedSegmentStore::<Block, _>::new(backend.clone());
			assert_eq!(store.load(hash_a).expect("load a"), None, "hash_a delete must persist");
			assert_eq!(store.load(hash_b).expect("load b"), Some(entry_b));
		});

		// `tmp` drops here, recursively removing the parity-db directory.
	}
}
