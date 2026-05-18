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
	sync::Arc,
	time::{SystemTime, UNIX_EPOCH},
};

const LOG_TARGET: &str = "cumulus-unincluded-segment-store";
const UNINCLUDED_SEGMENT_STORE_VERSION: &[u8] = b"cumulus_unincluded_segment_store_version";
const UNINCLUDED_SEGMENT_STORE_CURRENT_VERSION: u32 = 1;

/// Return the current Unix milliseconds timestamp.
///
/// Falls back to 0 if the system clock is before the Unix epoch.
pub fn now_unix_ms() -> u64 {
	match SystemTime::now().duration_since(UNIX_EPOCH) {
		Ok(d) => d.as_millis() as u64,
		Err(e) => {
			tracing::warn!(target: LOG_TARGET, error = ?e, "system clock is before UNIX epoch; storing time_ms=0");
			0
		},
	}
}

/// Entry stored in aux storage for each unincluded parablock.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct StoredEntry {
	/// Unix millis at the moment block_import started.
	pub time_ms: u64,
	/// The storage proof captured at block_import.
	pub proof: StorageProof,
}

/// The aux storage key used to store the unincluded segment data for the given block hash.
fn unincluded_segment_key<H: Encode>(block_hash: H) -> Vec<u8> {
	(b"cumulus_unincluded_segment_store", block_hash).encode()
}

fn load_decode<B, T>(backend: &B, key: &[u8]) -> ClientResult<Option<T>>
where
	B: AuxStore,
	T: Decode,
{
	match backend.get_aux(key)? {
		None => Ok(None),
		Some(t) => T::decode(&mut &t[..]).map(Some).map_err(|e| {
			ClientError::Backend(format!(
				"Unincluded segment store DB is corrupted. Decode error: {}",
				e
			))
		}),
	}
}

/// Prepare aux storage key-value pairs for persisting unincluded segment data.
///
/// The caller should push these into `BlockImportParams::auxiliary` so they commit
/// in the same DB transaction as the block.
pub fn prepare_unincluded_segment_aux_data<H: Encode>(
	block_hash: H,
	time_ms: u64,
	proof: &StorageProof,
) -> impl Iterator<Item = (Vec<u8>, Vec<u8>)> {
	let key = unincluded_segment_key(block_hash);
	let mut encoded_entry = time_ms.encode();
	proof.encode_to(&mut encoded_entry);
	let current_version = UNINCLUDED_SEGMENT_STORE_CURRENT_VERSION.encode();

	[(key, encoded_entry), (UNINCLUDED_SEGMENT_STORE_VERSION.to_vec(), current_version)].into_iter()
}

/// Load the unincluded segment entry associated with a block.
pub fn load_entry<H: Encode, B: AuxStore>(
	backend: &B,
	block_hash: H,
) -> ClientResult<Option<StoredEntry>> {
	let version = load_decode::<_, u32>(backend, UNINCLUDED_SEGMENT_STORE_VERSION)?;

	match version {
		None => Ok(None),
		Some(UNINCLUDED_SEGMENT_STORE_CURRENT_VERSION) => {
			load_decode(backend, unincluded_segment_key(block_hash).as_slice())
		},
		Some(other) => Err(ClientError::Backend(format!(
			"Unsupported unincluded segment store DB version: {:?}",
			other
		))),
	}
}

/// Compute aux storage cleanup operations.
///
/// Emits deletes for stale-fork blocks, intermediate tree-route blocks, the pre-finality head,
/// and the just-finalized block itself. Once a block is finalized it is no longer in any
/// unincluded segment, so its proof entry is dead weight.
///
/// TODO(#12034): also prune entries whose relay-parent session is older than
/// `max_relay_parent_session_age` relative to the current session. Session info will be sourced
/// from a separate session-data cache (see #11624), not from per-entry storage.
fn aux_storage_cleanup<H>(
	just_finalized_hash: H,
	old_finalized_hash: H,
	tree_route: &[H],
	stale_block_hashes: &[H],
) -> AuxDataOperations
where
	H: Encode,
{
	let mut ops = Vec::with_capacity(stale_block_hashes.len() + tree_route.len() + 2);
	ops.extend(stale_block_hashes.iter().map(|hash| (unincluded_segment_key(hash), None)));
	ops.extend(tree_route.iter().map(|hash| (unincluded_segment_key(hash), None)));
	ops.push((unincluded_segment_key(old_finalized_hash), None));
	ops.push((unincluded_segment_key(just_finalized_hash), None));
	ops
}

/// Register a finality action for cleaning up unincluded segment data.
///
/// This should be called during consensus initialization to automatically clean up
/// unincluded segment data when blocks are finalized.
pub fn register_unincluded_segment_cleanup<C, Block>(client: Arc<C>)
where
	C: PreCommitActions<Block> + HeaderBackend<Block> + 'static,
	Block: BlockT,
{
	let client_for_closure = client.clone();
	let on_finality = move |notification: &FinalityNotification<Block>| -> AuxDataOperations {
		let old_finalized = old_finalized_hash::<_, Block>(
			&*client_for_closure,
			&notification.tree_route,
			*notification.header.parent_hash(),
		);

		let stale_block_hashes: Vec<_> = notification.stale_blocks.iter().map(|b| b.hash).collect();
		let tree_route: Vec<_> = notification.tree_route.iter().copied().collect();

		aux_storage_cleanup(notification.hash, old_finalized, &tree_route, &stale_block_hashes)
	};

	client.register_finality_action(Box::new(on_finality));
}

#[cfg(test)]
mod tests {
	use super::*;
	use sc_client_api::backend::AuxStore;

	type Block = substrate_test_runtime::Block;
	type Hash = <Block as BlockT>::Hash;
	type TestBackend = sc_client_api::in_mem::Backend<Block>;

	fn create_test_entry(time_ms: u64) -> StoredEntry {
		StoredEntry { time_ms, proof: StorageProof::new(vec![vec![1, 2, 3], vec![4, 5, 6]]) }
	}

	fn write_aux_data(backend: &TestBackend, data: impl Iterator<Item = (Vec<u8>, Vec<u8>)>) {
		let pairs: Vec<_> = data.collect();
		let insert_pairs: Vec<_> =
			pairs.iter().map(|(k, v)| (k.as_slice(), v.as_slice())).collect();
		AuxStore::insert_aux(backend, &insert_pairs, &[]).expect("aux insert should succeed");
	}

	#[test]
	fn prepare_produces_expected_key_value_pairs() {
		let hash = Hash::repeat_byte(0xAB);
		let time_ms = 1234567890u64;
		let proof = StorageProof::new(vec![vec![10, 20, 30]]);

		let pairs: Vec<_> = prepare_unincluded_segment_aux_data(hash, time_ms, &proof).collect();

		assert_eq!(pairs.len(), 2);

		// First pair is the entry
		let expected_key = (b"cumulus_unincluded_segment_store", hash).encode();
		assert_eq!(pairs[0].0, expected_key);

		// Verify entry decodes correctly
		let decoded_entry =
			StoredEntry::decode(&mut pairs[0].1.as_slice()).expect("entry should decode");
		assert_eq!(decoded_entry.time_ms, time_ms);
		assert_eq!(decoded_entry.proof, proof);

		// Second pair is the version
		assert_eq!(pairs[1].0, UNINCLUDED_SEGMENT_STORE_VERSION.to_vec());
		let decoded_version =
			u32::decode(&mut pairs[1].1.as_slice()).expect("version should decode");
		assert_eq!(decoded_version, UNINCLUDED_SEGMENT_STORE_CURRENT_VERSION);
	}

	#[test]
	fn load_returns_none_when_no_entry_exists() {
		let backend = TestBackend::new();
		let hash = Hash::repeat_byte(0xEF);

		let loaded = load_entry(&backend, hash).expect("load should succeed");
		assert_eq!(loaded, None);
	}

	#[test]
	fn cleanup_combines_all_categories() {
		let stale_1 = Hash::repeat_byte(0xAA);
		let stale_2 = Hash::repeat_byte(0xBB);
		let route_1 = Hash::repeat_byte(0xC1);
		let route_2 = Hash::repeat_byte(0xC2);
		let old_finalized = Hash::repeat_byte(0xF0);
		let just_finalized = Hash::repeat_byte(0xFF);

		let ops = aux_storage_cleanup(
			just_finalized,
			old_finalized,
			&[route_1, route_2],
			&[stale_1, stale_2],
		);

		let keys: Vec<_> = ops.iter().map(|(k, _)| k.clone()).collect();

		// Verify all expected keys are present (including the just-finalized block).
		assert!(keys.contains(&unincluded_segment_key(stale_1)));
		assert!(keys.contains(&unincluded_segment_key(stale_2)));
		assert!(keys.contains(&unincluded_segment_key(route_1)));
		assert!(keys.contains(&unincluded_segment_key(route_2)));
		assert!(keys.contains(&unincluded_segment_key(old_finalized)));
		assert!(keys.contains(&unincluded_segment_key(just_finalized)));

		// Verify all values are None (deletes)
		assert!(ops.iter().all(|(_, v)| v.is_none()));
	}

	#[test]
	fn cleanup_handles_empty_inputs() {
		let just_finalized = Hash::repeat_byte(0xFF);
		let old_finalized = Hash::repeat_byte(0xF0);

		let ops = aux_storage_cleanup(just_finalized, old_finalized, &[], &[]);

		assert_eq!(ops.len(), 2);
		assert!(ops.iter().all(|(_, v)| v.is_none()));
	}

	#[test]
	fn stored_entry_encoding_hex_snapshot() {
		// Canonical entry for hex snapshot
		let entry =
			StoredEntry { time_ms: 1234567890u64, proof: StorageProof::new(vec![vec![1, 2, 3]]) };

		let encoded = entry.encode();
		// Snapshot of the SCALE encoding. If this assertion fires, the on-disk format of
		// `StoredEntry` has changed and existing aux entries written by older builds will fail to
		// decode — bump `UNINCLUDED_SEGMENT_STORE_CURRENT_VERSION` and add a migration before
		// updating this snapshot.
		let encoded_hex = hex::encode(&encoded);
		// time_ms = 1234567890 little-endian u64 = d2 02 96 49 00 00 00 00
		// proof   = Vec<Vec<u8>> with one element [1,2,3]: outer len 1 (SCALE compact = 04),
		//           inner len 3 (compact = 0c), bytes 01 02 03
		let expected_hex = "d20296490000000004 0c 010203".replace(' ', "");
		assert_eq!(encoded_hex, expected_hex, "StoredEntry encoding changed!");

		// Verify it decodes back correctly
		let decoded = StoredEntry::decode(&mut encoded.as_slice()).expect("decode should succeed");
		assert_eq!(entry, decoded);
	}

	#[test]
	fn decode_corrupted_entry_body() {
		let backend = TestBackend::new();
		let hash = Hash::repeat_byte(0xAB);

		// Write correct version
		let version_encoded = UNINCLUDED_SEGMENT_STORE_CURRENT_VERSION.encode();
		AuxStore::insert_aux(
			&backend,
			&[(UNINCLUDED_SEGMENT_STORE_VERSION, version_encoded.as_slice())],
			&[],
		)
		.expect("aux insert should succeed");

		// Write bogus entry body
		let key = unincluded_segment_key(hash);
		let bogus_data = vec![0xFF, 0xAA, 0xBB]; // Invalid SCALE encoding
		AuxStore::insert_aux(&backend, &[(&key[..], bogus_data.as_slice())], &[])
			.expect("aux insert should succeed");

		// Load should fail with decode error
		let result = load_entry(&backend, hash);
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
		let backend = TestBackend::new();

		// Write three entries
		let hash1 = Hash::repeat_byte(0x01);
		let hash2 = Hash::repeat_byte(0x02);
		let hash3 = Hash::repeat_byte(0x03);

		let entry1 = create_test_entry(1000);
		let entry2 = create_test_entry(2000);
		let entry3 = create_test_entry(3000);

		write_aux_data(
			&backend,
			prepare_unincluded_segment_aux_data(hash1, entry1.time_ms, &entry1.proof),
		);
		write_aux_data(
			&backend,
			prepare_unincluded_segment_aux_data(hash2, entry2.time_ms, &entry2.proof),
		);
		write_aux_data(
			&backend,
			prepare_unincluded_segment_aux_data(hash3, entry3.time_ms, &entry3.proof),
		);

		// Verify all three round-trip with value equality.
		assert_eq!(load_entry(&backend, hash1).expect("load"), Some(entry1));
		assert_eq!(load_entry(&backend, hash2).expect("load"), Some(entry2));
		assert_eq!(load_entry(&backend, hash3).expect("load"), Some(entry3.clone()));

		// Generate cleanup that deletes hash1 (just-finalized) and hash2 (in tree route).
		let ops = aux_storage_cleanup(hash1, Hash::repeat_byte(0xF0), &[hash2], &[]);
		let delete_keys: Vec<_> =
			ops.iter().filter_map(|(k, v)| v.is_none().then(|| k.as_slice())).collect();

		// Apply deletes
		AuxStore::insert_aux(&backend, &[], &delete_keys).expect("delete should succeed");

		// Verify hash1 and hash2 deleted, hash3 survives with original value intact.
		assert_eq!(load_entry(&backend, hash1).expect("load"), None, "hash1 should be deleted");
		assert_eq!(load_entry(&backend, hash2).expect("load"), None, "hash2 should be deleted");
		assert_eq!(
			load_entry(&backend, hash3).expect("load"),
			Some(entry3),
			"hash3 should survive"
		);
	}
}
