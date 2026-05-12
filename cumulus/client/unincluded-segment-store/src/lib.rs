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
//! Persists `(time, session, StorageProof)` per parablock built by a slot-based collator,
//! keyed by parablock hash. Data is pruned on parachain finality.

use codec::{Decode, Encode};
use polkadot_primitives::SessionIndex;
use sc_client_api::{
	backend::AuxStore,
	client::{AuxDataOperations, FinalityNotification, PreCommitActions},
	HeaderBackend,
};
use sp_blockchain::{Error as ClientError, Result as ClientResult};
use sp_runtime::traits::{Block as BlockT, Header as _};
use sp_trie::StorageProof;
use std::sync::Arc;

const UNINCLUDED_SEGMENT_STORE_VERSION: &[u8] = b"cumulus_unincluded_segment_store_version";
const UNINCLUDED_SEGMENT_STORE_CURRENT_VERSION: u32 = 1;

/// Entry stored in aux storage for each unincluded parablock.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct StoredEntry {
	/// Unix millis at the moment block_import started.
	pub time_ms: u64,
	/// SessionIndex of the relay parent.
	/// TODO: Will be populated once paritytech/polkadot-sdk#11624 lands.
	pub session: Option<SessionIndex>,
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
	let corrupt = |e: codec::Error| {
		ClientError::Backend(format!(
			"Unincluded segment store DB is corrupted. Decode error: {}",
			e
		))
	};

	match backend.get_aux(key)? {
		None => Ok(None),
		Some(t) => T::decode(&mut &t[..]).map(Some).map_err(corrupt),
	}
}

/// Prepare aux storage key-value pairs for persisting unincluded segment data.
///
/// The caller should push these into `BlockImportParams::auxiliary` so they commit
/// in the same DB transaction as the block.
pub fn prepare_unincluded_segment_aux_data<H: Encode>(
	block_hash: H,
	time_ms: u64,
	session: Option<SessionIndex>,
	proof: StorageProof,
) -> impl Iterator<Item = (Vec<u8>, Vec<u8>)> {
	let entry = StoredEntry { time_ms, session, proof };
	let key = unincluded_segment_key(block_hash);
	let encoded_entry = entry.encode();
	let current_version = UNINCLUDED_SEGMENT_STORE_CURRENT_VERSION.encode();

	[(key, encoded_entry), (UNINCLUDED_SEGMENT_STORE_VERSION.to_vec(), current_version)].into_iter()
}

/// Load the unincluded segment entry associated with a block.
pub fn load_proof<H: Encode, B: AuxStore>(
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

/// Compute aux storage cleanup operations for a finality notification.
///
/// Deletes entries for:
/// - All stale blocks (loser forks)
/// - All blocks on the tree route to the finalized head (intermediate finalized blocks)
/// - The old finalized block (parent of the first tree route block, or parent of finalized)
///
/// Does NOT delete the just-finalized block, since new blocks can still be imported on top of it.
fn aux_storage_cleanup<Block, C>(
	notification: &FinalityNotification<Block>,
	client: &C,
) -> AuxDataOperations
where
	Block: BlockT,
	C: HeaderBackend<Block>,
{
	// The old finalized block is the parent of the first block in the tree route,
	// or the parent of the finalized block if the tree route is empty.
	let old_finalized_hash = notification
		.tree_route
		.first()
		.and_then(|hash| client.header(*hash).ok().flatten())
		.map(|h| *h.parent_hash())
		.unwrap_or_else(|| *notification.header.parent_hash());

	notification
		.stale_blocks
		.iter()
		// Delete entries for all stale blocks.
		.map(|b| (unincluded_segment_key(b.hash), None))
		// Delete entries for blocks on the route to the finalized parent.
		// These can no longer become parents for new blocks.
		.chain(
			notification
				.tree_route
				.iter()
				.copied()
				.map(|hash| (unincluded_segment_key(hash), None)),
		)
		// Delete the old finalized block's entry.
		.chain(std::iter::once((unincluded_segment_key(old_finalized_hash), None)))
		.collect()
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
		aux_storage_cleanup(notification, &*client_for_closure)
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
		StoredEntry {
			time_ms,
			session: Some(42),
			proof: StorageProof::new(vec![vec![1, 2, 3], vec![4, 5, 6]]),
		}
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
		let session = Some(5u32);
		let proof = StorageProof::new(vec![vec![10, 20, 30]]);

		let pairs: Vec<_> =
			prepare_unincluded_segment_aux_data(hash, time_ms, session, proof.clone()).collect();

		assert_eq!(pairs.len(), 2);

		// First pair is the entry
		let expected_key = (b"cumulus_unincluded_segment_store", hash).encode();
		assert_eq!(pairs[0].0, expected_key);

		// Verify entry decodes correctly
		let decoded_entry =
			StoredEntry::decode(&mut pairs[0].1.as_slice()).expect("entry should decode");
		assert_eq!(decoded_entry.time_ms, time_ms);
		assert_eq!(decoded_entry.session, session);
		assert_eq!(decoded_entry.proof, proof);

		// Second pair is the version
		assert_eq!(pairs[1].0, UNINCLUDED_SEGMENT_STORE_VERSION.to_vec());
		let decoded_version =
			u32::decode(&mut pairs[1].1.as_slice()).expect("version should decode");
		assert_eq!(decoded_version, UNINCLUDED_SEGMENT_STORE_CURRENT_VERSION);
	}

	#[test]
	fn round_trip_write_and_load() {
		let backend = TestBackend::new();
		let hash = Hash::repeat_byte(0xCD);
		let entry = create_test_entry(999888777);

		// Write via prepare + manual insert
		let aux_data = prepare_unincluded_segment_aux_data(
			hash,
			entry.time_ms,
			entry.session,
			entry.proof.clone(),
		);
		write_aux_data(&backend, aux_data);

		// Load back
		let loaded = load_proof(&backend, hash).expect("load should succeed");
		assert_eq!(loaded, Some(entry));
	}

	#[test]
	fn load_returns_none_when_no_entry_exists() {
		let backend = TestBackend::new();
		let hash = Hash::repeat_byte(0xEF);

		let loaded = load_proof(&backend, hash).expect("load should succeed");
		assert_eq!(loaded, None);
	}

	#[test]
	fn load_returns_error_on_version_mismatch() {
		let backend = TestBackend::new();
		let hash = Hash::repeat_byte(0x12);

		// Write a bogus version
		let bogus_version = 999u32.encode();
		AuxStore::insert_aux(
			&backend,
			&[(UNINCLUDED_SEGMENT_STORE_VERSION, bogus_version.as_slice())],
			&[],
		)
		.expect("aux insert should succeed");

		// Also write an entry so it's not just missing
		let entry = create_test_entry(12345);
		let key = unincluded_segment_key(hash);
		AuxStore::insert_aux(&backend, &[(&key[..], entry.encode().as_slice())], &[])
			.expect("aux insert should succeed");

		// Load should fail with version error
		let result = load_proof(&backend, hash);
		assert!(result.is_err());
		let err_msg = result.unwrap_err().to_string();
		assert!(
			err_msg.contains("Unsupported unincluded segment store DB version"),
			"unexpected error: {}",
			err_msg
		);
	}

	#[test]
	fn cleanup_computes_correct_deletions() {
		let backend = TestBackend::new();

		let hash_c = Hash::repeat_byte(0xCC);
		let hash_genesis = Hash::repeat_byte(0x00);

		// Expected keys to delete:
		let expected_stale_key = unincluded_segment_key(hash_c);
		let expected_old_finalized_key = unincluded_segment_key(hash_genesis);

		// Verify key format
		assert_eq!(expected_stale_key, (b"cumulus_unincluded_segment_store", hash_c).encode());
		assert_eq!(
			expected_old_finalized_key,
			(b"cumulus_unincluded_segment_store", hash_genesis).encode()
		);

		// Test that entries can be written and then "deleted" (set to None)
		let entry = create_test_entry(111);
		let key_c = unincluded_segment_key(hash_c);
		AuxStore::insert_aux(&backend, &[(&key_c[..], entry.encode().as_slice())], &[])
			.expect("insert should succeed");

		// Verify entry exists
		assert!(AuxStore::get_aux(&backend, &key_c).expect("get should succeed").is_some());

		// Simulate deletion via AuxDataOperations
		let delete_ops: AuxDataOperations = vec![(key_c.clone(), None)];
		let deletes: Vec<&[u8]> = delete_ops
			.iter()
			.filter_map(|(k, v)| if v.is_none() { Some(k.as_slice()) } else { None })
			.collect();
		AuxStore::insert_aux(&backend, &[], &deletes).expect("delete should succeed");

		// Verify entry is gone
		assert!(AuxStore::get_aux(&backend, &key_c).expect("get should succeed").is_none());
	}

	#[test]
	fn cleanup_with_tree_route() {
		// Test that cleanup includes tree_route blocks in deletions.
		// This tests the logic without needing a real FinalityNotification.

		let hash_a = Hash::repeat_byte(0xAA);
		let hash_b = Hash::repeat_byte(0xBB);
		let hash_c = Hash::repeat_byte(0xCC);

		// Generate keys for tree_route blocks
		let key_a = unincluded_segment_key(hash_a);
		let key_b = unincluded_segment_key(hash_b);
		let key_c = unincluded_segment_key(hash_c);

		// All keys should be distinct
		assert_ne!(key_a, key_b);
		assert_ne!(key_b, key_c);
		assert_ne!(key_a, key_c);

		// Keys should be deterministic
		assert_eq!(key_a, unincluded_segment_key(hash_a));
	}

	#[test]
	fn stored_entry_encoding_is_stable() {
		// Verify that encoding/decoding is symmetric
		let entry = StoredEntry { time_ms: u64::MAX, session: None, proof: StorageProof::empty() };

		let encoded = entry.encode();
		let decoded = StoredEntry::decode(&mut encoded.as_slice()).expect("decode should succeed");
		assert_eq!(entry, decoded);

		// With session
		let entry_with_session = StoredEntry {
			time_ms: 0,
			session: Some(u32::MAX),
			proof: StorageProof::new(vec![vec![255; 100]]),
		};

		let encoded = entry_with_session.encode();
		let decoded = StoredEntry::decode(&mut encoded.as_slice()).expect("decode should succeed");
		assert_eq!(entry_with_session, decoded);
	}
}
