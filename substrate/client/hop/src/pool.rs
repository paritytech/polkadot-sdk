// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.

//! HOP data pool implementation.

use crate::{
	primitives::{HopBlockNumber, HopHash},
	types::{HopError, HopPoolEntry, PoolStatus, MAX_DATA_SIZE},
};
use parking_lot::RwLock;
use sp_core::{crypto::Pair as _, ed25519, hashing::blake2_256, H256};
use std::{
	collections::HashMap,
	sync::{
		atomic::{AtomicU64, Ordering},
		Arc,
	},
};

/// HOP data pool
pub struct HopDataPool {
	/// The actual data
	entries: Arc<RwLock<HashMap<HopHash, HopPoolEntry>>>,
	/// Maximum pool size in bytes
	max_size: u64,
	/// Current pool size in bytes
	current_size: AtomicU64,
	/// Data retention period in blocks
	retention_blocks: u32,
}

impl HopDataPool {
	/// Create a new data pool
	pub fn new(max_size: u64, retention_blocks: u32) -> Result<Self, HopError> {
		Ok(Self {
			entries: Arc::new(RwLock::new(HashMap::new())),
			max_size,
			current_size: AtomicU64::new(0),
			retention_blocks,
		})
	}

	/// Insert data into the pool
	///
	/// Returns the hash of the data
	pub fn insert(
		&self,
		data: Vec<u8>,
		current_block: HopBlockNumber,
		recipients: Vec<[u8; 32]>,
	) -> Result<HopHash, HopError> {
		// Validate recipients
		if recipients.is_empty() {
			return Err(HopError::NoRecipients);
		}

		// Validate data size
		if data.is_empty() {
			return Err(HopError::EmptyData);
		}

		let data_len = data.len() as u64;
		if data_len > MAX_DATA_SIZE {
			return Err(HopError::DataTooLarge(data.len(), MAX_DATA_SIZE));
		}

		// Check pool capacity
		let current_size = self.current_size.load(Ordering::Relaxed);
		if current_size + data_len > self.max_size {
			return Err(HopError::PoolFull(current_size, self.max_size));
		}

		let hash = H256(blake2_256(&data));

		// Check for duplicates
		{
			let entries = self.entries.read();
			if entries.contains_key(&hash) {
				return Err(HopError::DuplicateEntry);
			}
		}

		// Create entry and add it to the pool
		let entry = HopPoolEntry::new(data, current_block, self.retention_blocks, recipients);
		{
			let mut entries = self.entries.write();
			entries.insert(hash, entry);
		}

		// Update size counter
		self.current_size.fetch_add(data_len, Ordering::Relaxed);

		tracing::info!(
			target: "hop",
			hash = ?hex::encode(hash),
			size = data_len,
			expires_at = current_block + self.retention_blocks,
			"Data added to HOP pool"
		);

		Ok(hash)
	}

	/// Get data from the pool by content hash
	pub fn get(&self, hash: &HopHash) -> Option<Vec<u8>> {
		let entries = self.entries.read();
		entries.get(hash).map(|entry| entry.data.clone())
	}

	/// Claim data from the pool. Verifies the signature against recipient public keys.
	/// Returns the data if the signature matches an unclaimed recipient.
	/// Removes the entry once all recipients have claimed.
	pub fn claim(&self, hash: &HopHash, signature: &[u8]) -> Result<Vec<u8>, HopError> {
		let mut entries = self.entries.write();
		let entry = entries.get_mut(hash).ok_or(HopError::NotFound)?;

		// Parse the ed25519 signature (64 bytes)
		let sig = ed25519::Signature::try_from(signature)
			.map_err(|_| HopError::InvalidSignature)?;

		// Find which unclaimed recipient this signature matches
		let recipient_index = entry
			.recipients
			.iter()
			.enumerate()
			.find_map(|(i, pubkey)| {
				if entry.claimed[i] {
					return None;
				}
				let public = ed25519::Public::from_raw(*pubkey);
				if ed25519::Pair::verify(&sig, hash.as_bytes(), &public) {
					Some(i)
				} else {
					None
				}
			})
			.ok_or(HopError::NotRecipient)?;

		entry.claimed[recipient_index] = true;
		let data = entry.data.clone();

		// If all recipients have claimed, remove the entry
		if entry.claimed.iter().all(|&c| c) {
			let size = entry.size;
			entries.remove(hash);
			self.current_size.fetch_sub(size, Ordering::Relaxed);
			tracing::info!(
				target: "hop",
				hash = ?hex::encode(hash),
				"All recipients claimed, data removed"
			);
		} else {
			let claimed_count = entry.claimed.iter().filter(|&&c| c).count();
			tracing::debug!(
				target: "hop",
				hash = ?hex::encode(hash),
				claimed = claimed_count,
				total = entry.recipients.len(),
				"Recipient claimed"
			);
		}

		Ok(data)
	}

	/// Check if data exists in the pool
	pub fn has(&self, hash: &HopHash) -> bool {
		let entries = self.entries.read();
		entries.contains_key(hash)
	}

	/// Remove data from the pool
	pub fn remove(&self, hash: &HopHash) -> Result<(), HopError> {
		let entry = {
			let mut entries = self.entries.write();
			entries.remove(hash)
		};

		if let Some(entry) = entry {
			// Update size counter
			self.current_size.fetch_sub(entry.size, Ordering::Relaxed);

			tracing::debug!(
				target: "hop",
				hash = ?hex::encode(hash),
				"Data removed from pool"
			);

			Ok(())
		} else {
			Err(HopError::NotFound)
		}
	}

	/// Remove expired entries from the pool and return them.
	/// Callers should attempt to promote these entries to on-chain storage.
	pub fn drain_expired(&self, current_block: HopBlockNumber) -> Vec<(HopHash, HopPoolEntry)> {
		let mut entries = self.entries.write();
		let expired: Vec<HopHash> = entries
			.iter()
			.filter(|(_, e)| current_block >= e.expires_at)
			.map(|(h, _)| *h)
			.collect();

		let mut drained = Vec::new();
		for hash in expired {
			if let Some(entry) = entries.remove(&hash) {
				self.current_size.fetch_sub(entry.size, Ordering::Relaxed);
				drained.push((hash, entry));
			}
		}
		drained
	}

	/// Get pool status
	pub fn status(&self) -> PoolStatus {
		let entries = self.entries.read();
		PoolStatus {
			entry_count: entries.len(),
			total_bytes: self.current_size.load(Ordering::Relaxed),
			max_bytes: self.max_size,
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use sp_core::Pair;

	fn create_test_pool() -> HopDataPool {
		HopDataPool::new(1024 * 1024, 100).unwrap()
	}

	fn test_recipient() -> (ed25519::Pair, [u8; 32]) {
		let pair = ed25519::Pair::from_seed(&[1u8; 32]);
		let pubkey: [u8; 32] = pair.public().0;
		(pair, pubkey)
	}

	#[test]
	fn test_insert_and_get() {
		let pool = create_test_pool();
		let (_, pubkey) = test_recipient();
		let data = vec![1, 2, 3, 4, 5];
		let hash = pool.insert(data.clone(), 0, vec![pubkey]).unwrap();

		let retrieved = pool.get(&hash).unwrap();
		assert_eq!(data, retrieved);
	}

	#[test]
	fn test_insert_no_recipients() {
		let pool = create_test_pool();
		let data = vec![1, 2, 3, 4, 5];
		let result = pool.insert(data, 0, vec![]);
		assert!(matches!(result, Err(HopError::NoRecipients)));
	}

	#[test]
	fn test_duplicate_insert() {
		let pool = create_test_pool();
		let (_, pubkey) = test_recipient();
		let data = vec![1, 2, 3, 4, 5];

		pool.insert(data.clone(), 0, vec![pubkey]).unwrap();
		let result = pool.insert(data, 0, vec![pubkey]);

		assert!(matches!(result, Err(HopError::DuplicateEntry)));
	}

	#[test]
	fn test_data_too_large() {
		let pool = create_test_pool();
		let (_, pubkey) = test_recipient();
		let data = vec![0u8; (MAX_DATA_SIZE + 1) as usize];

		let result = pool.insert(data, 0, vec![pubkey]);
		assert!(matches!(result, Err(HopError::DataTooLarge(_, _))));
	}

	#[test]
	fn test_pool_full() {
		let pool = HopDataPool::new(100, 100).unwrap();
		let (_, pubkey) = test_recipient();

		let data1 = vec![0u8; 60];
		let data2 = vec![1u8; 50];

		pool.insert(data1, 0, vec![pubkey]).unwrap();
		let result = pool.insert(data2, 0, vec![pubkey]);

		assert!(matches!(result, Err(HopError::PoolFull(_, _))));
	}

	#[test]
	fn test_remove() {
		let pool = create_test_pool();
		let (_, pubkey) = test_recipient();
		let data = vec![1, 2, 3, 4, 5];
		let hash = pool.insert(data, 0, vec![pubkey]).unwrap();

		assert!(pool.has(&hash));
		pool.remove(&hash).unwrap();
		assert!(!pool.has(&hash));
	}

	#[test]
	fn test_status() {
		let pool = create_test_pool();
		let (_, pubkey) = test_recipient();
		let data1 = vec![1, 2, 3, 4, 5];
		let data2 = vec![6, 7, 8];

		pool.insert(data1.clone(), 0, vec![pubkey]).unwrap();
		pool.insert(data2.clone(), 0, vec![pubkey]).unwrap();

		let status = pool.status();
		assert_eq!(status.entry_count, 2);
		assert_eq!(status.total_bytes, (data1.len() + data2.len()) as u64);
	}

	#[test]
	fn test_claim_valid_signature() {
		let pool = create_test_pool();
		let (pair, pubkey) = test_recipient();
		let data = vec![1, 2, 3, 4, 5];
		let hash = pool.insert(data.clone(), 0, vec![pubkey]).unwrap();

		let sig = pair.sign(hash.as_bytes());
		let result = pool.claim(&hash, sig.as_ref()).unwrap();
		assert_eq!(data, result);

		// Entry should be removed after sole recipient claims
		assert!(!pool.has(&hash));
	}

	#[test]
	fn test_claim_invalid_signature() {
		let pool = create_test_pool();
		let (_, pubkey) = test_recipient();
		let data = vec![1, 2, 3, 4, 5];
		let hash = pool.insert(data, 0, vec![pubkey]).unwrap();

		// Use a bad signature (wrong length)
		let result = pool.claim(&hash, &[0u8; 32]);
		assert!(matches!(result, Err(HopError::InvalidSignature)));
	}

	#[test]
	fn test_claim_wrong_key() {
		let pool = create_test_pool();
		let (_, pubkey) = test_recipient();
		let data = vec![1, 2, 3, 4, 5];
		let hash = pool.insert(data, 0, vec![pubkey]).unwrap();

		// Sign with a different keypair
		let wrong_pair = ed25519::Pair::from_seed(&[99u8; 32]);
		let sig = wrong_pair.sign(hash.as_bytes());
		let result = pool.claim(&hash, sig.as_ref());
		assert!(matches!(result, Err(HopError::NotRecipient)));

		// Entry should still exist
		assert!(pool.has(&hash));
	}

	#[test]
	fn test_claim_multi_recipient() {
		let pool = create_test_pool();
		let pair1 = ed25519::Pair::from_seed(&[1u8; 32]);
		let pair2 = ed25519::Pair::from_seed(&[2u8; 32]);
		let pubkey1: [u8; 32] = pair1.public().0;
		let pubkey2: [u8; 32] = pair2.public().0;

		let data = vec![1, 2, 3, 4, 5];
		let hash = pool.insert(data.clone(), 0, vec![pubkey1, pubkey2]).unwrap();

		// First recipient claims
		let sig1 = pair1.sign(hash.as_bytes());
		let result1 = pool.claim(&hash, sig1.as_ref()).unwrap();
		assert_eq!(data, result1);
		assert!(pool.has(&hash)); // still exists, second recipient hasn't claimed

		// Second recipient claims
		let sig2 = pair2.sign(hash.as_bytes());
		let result2 = pool.claim(&hash, sig2.as_ref()).unwrap();
		assert_eq!(data, result2);
		assert!(!pool.has(&hash)); // now removed

		// Pool size should be back to 0
		assert_eq!(pool.status().total_bytes, 0);
	}

	#[test]
	fn test_claim_already_claimed_recipient() {
		let pool = create_test_pool();
		let (pair, pubkey) = test_recipient();
		let pair2 = ed25519::Pair::from_seed(&[2u8; 32]);
		let pubkey2: [u8; 32] = pair2.public().0;

		let data = vec![1, 2, 3, 4, 5];
		let hash = pool.insert(data.clone(), 0, vec![pubkey, pubkey2]).unwrap();

		// First claim succeeds
		let sig = pair.sign(hash.as_bytes());
		pool.claim(&hash, sig.as_ref()).unwrap();

		// Same recipient tries to claim again — should fail (already claimed)
		let result = pool.claim(&hash, sig.as_ref());
		assert!(matches!(result, Err(HopError::NotRecipient)));
	}

	#[test]
	fn test_claim_not_found() {
		let pool = create_test_pool();
		let fake_hash = H256([0u8; 32]);
		let result = pool.claim(&fake_hash, &[0u8; 64]);
		assert!(matches!(result, Err(HopError::NotFound)));
	}
}
