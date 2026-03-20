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

//! HOP data pool implementation with disk-backed storage.

use crate::{
	primitives::{HopBlockNumber, HopHash},
	types::{Alias, HopEntryMeta, HopError, PoolStatus, MAX_DATA_SIZE},
};
use codec::{Decode, Encode};
use parking_lot::RwLock;
use sp_core::{hashing::blake2_256, H256};
use sp_runtime::{
	traits::{IdentifyAccount, Verify},
	MultiSignature, MultiSigner,
};
use std::{
	collections::HashMap,
	fs,
	path::{Path, PathBuf},
	sync::{
		atomic::{AtomicU64, Ordering},
		Arc,
	},
};

/// HOP data pool with disk-backed blob storage and in-memory metadata index.
pub struct HopDataPool {
	/// In-memory metadata index (no blobs).
	index: Arc<RwLock<HashMap<HopHash, HopEntryMeta>>>,
	/// Per-user byte usage tracked by alias.
	user_usage: Arc<RwLock<HashMap<Alias, u64>>>,
	/// Maximum pool size in bytes.
	max_size: u64,
	/// Current pool size in bytes.
	current_size: AtomicU64,
	/// Data retention period in blocks.
	retention_blocks: u32,
	/// Root data directory containing blobs/ and meta/ subdirectories.
	data_dir: PathBuf,
}

impl HopDataPool {
	/// Create a new disk-backed data pool.
	///
	/// Creates shard directories under `data_dir` and rebuilds the in-memory index
	/// from existing `.meta` files on disk (recovery after restart).
	pub fn new(max_size: u64, retention_blocks: u32, data_dir: PathBuf) -> Result<Self, HopError> {
		// Create shard directories (256 each for blobs/ and meta/).
		for i in 0u8..=255 {
			let shard = format!("{:02x}", i);
			fs::create_dir_all(data_dir.join("blobs").join(&shard))
				.map_err(|e| HopError::IoError(format!("create blobs/{}: {}", shard, e)))?;
			fs::create_dir_all(data_dir.join("meta").join(&shard))
				.map_err(|e| HopError::IoError(format!("create meta/{}: {}", shard, e)))?;
		}

		let mut index = HashMap::new();
		let mut user_usage: HashMap<Alias, u64> = HashMap::new();
		let mut current_size = 0u64;

		// Rebuild index from .meta files on disk.
		for i in 0u8..=255 {
			let shard = format!("{:02x}", i);
			let meta_shard_dir = data_dir.join("meta").join(&shard);
			let entries = match fs::read_dir(&meta_shard_dir) {
				Ok(entries) => entries,
				Err(_) => continue,
			};

			for entry in entries.flatten() {
				let path = entry.path();
				if path.extension().and_then(|e| e.to_str()) != Some("meta") {
					continue;
				}

				let stem = match path.file_stem().and_then(|s| s.to_str()) {
					Some(s) => s.to_string(),
					None => continue,
				};

				// Parse hash from filename.
				let hash_bytes = match hex::decode(&stem) {
					Ok(b) if b.len() == 32 => {
						let mut arr = [0u8; 32];
						arr.copy_from_slice(&b);
						arr
					},
					_ => {
						tracing::warn!(target: "hop", path = ?path, "Removing .meta with invalid name");
						let _ = fs::remove_file(&path);
						continue;
					},
				};
				let hash = H256(hash_bytes);

				// Decode metadata.
				let meta_bytes = match fs::read(&path) {
					Ok(b) => b,
					Err(e) => {
						tracing::warn!(target: "hop", path = ?path, error = %e, "Removing unreadable .meta");
						let _ = fs::remove_file(&path);
						continue;
					},
				};
				let meta = match HopEntryMeta::decode(&mut &meta_bytes[..]) {
					Ok(m) => m,
					Err(e) => {
						tracing::warn!(target: "hop", path = ?path, error = %e, "Removing corrupt .meta");
						let _ = fs::remove_file(&path);
						continue;
					},
				};

				// Verify corresponding .blob exists.
				let blob_path = data_dir.join("blobs").join(&shard).join(format!("{}.blob", stem));
				if !blob_path.exists() {
					tracing::warn!(target: "hop", hash = ?stem, "Removing orphan .meta (no .blob)");
					let _ = fs::remove_file(&path);
					continue;
				}

				current_size += meta.size;
				*user_usage.entry(meta.sender_alias).or_insert(0) += meta.size;
				index.insert(hash, meta);
			}
		}

		// Clean orphan .blob files (blobs without corresponding .meta).
		for i in 0u8..=255 {
			let shard = format!("{:02x}", i);
			let blob_shard_dir = data_dir.join("blobs").join(&shard);
			let entries = match fs::read_dir(&blob_shard_dir) {
				Ok(entries) => entries,
				Err(_) => continue,
			};

			for entry in entries.flatten() {
				let path = entry.path();
				if path.extension().and_then(|e| e.to_str()) != Some("blob") {
					continue;
				}
				let stem = match path.file_stem().and_then(|s| s.to_str()) {
					Some(s) => s.to_string(),
					None => continue,
				};
				let meta_path = data_dir.join("meta").join(&shard).join(format!("{}.meta", stem));
				if !meta_path.exists() {
					tracing::warn!(target: "hop", hash = ?stem, "Removing orphan .blob (no .meta)");
					let _ = fs::remove_file(&path);
				}
			}
		}

		if !index.is_empty() {
			tracing::info!(
				target: "hop",
				entries = index.len(),
				total_bytes = current_size,
				"Recovered HOP pool from disk"
			);
		}

		Ok(Self {
			index: Arc::new(RwLock::new(index)),
			user_usage: Arc::new(RwLock::new(user_usage)),
			max_size,
			current_size: AtomicU64::new(current_size),
			retention_blocks,
			data_dir,
		})
	}

	/// Path to the blob file for a given hash.
	fn blob_path(&self, hash: &HopHash) -> PathBuf {
		let hex = hex::encode(hash);
		self.data_dir.join("blobs").join(&hex[..2]).join(format!("{}.blob", hex))
	}

	/// Path to the meta file for a given hash.
	fn meta_path(&self, hash: &HopHash) -> PathBuf {
		let hex = hex::encode(hash);
		self.data_dir.join("meta").join(&hex[..2]).join(format!("{}.meta", hex))
	}

	/// Atomically write data to a file (write to .tmp, then rename).
	fn write_atomic(path: &Path, data: &[u8]) -> Result<(), HopError> {
		let tmp_path = path.with_extension("tmp");
		fs::write(&tmp_path, data)
			.map_err(|e| HopError::IoError(format!("write {}: {}", tmp_path.display(), e)))?;
		fs::rename(&tmp_path, path)
			.map_err(|e| HopError::IoError(format!("rename {}: {}", path.display(), e)))?;
		Ok(())
	}

	/// Insert data into the pool.
	///
	/// Returns the hash of the data.
	pub fn insert(
		&self,
		data: Vec<u8>,
		current_block: HopBlockNumber,
		recipients: Vec<MultiSigner>,
		sender_alias: Alias,
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

		// Per-user quota enforcement
		let usage_map = self.user_usage.read();
		let current_usage = usage_map.get(&sender_alias).copied().unwrap_or(0);
		let is_new_user = current_usage == 0;
		let active_users =
			if is_new_user { usage_map.len() as u64 + 1 } else { usage_map.len() as u64 };
		let per_user_limit = self.max_size / active_users.max(1);
		drop(usage_map);

		if current_usage + data_len > per_user_limit {
			return Err(HopError::UserQuotaExceeded { used: current_usage, limit: per_user_limit });
		}

		let hash = H256(blake2_256(&data));

		// First duplicate check (read lock only).
		{
			let index = self.index.read();
			if index.contains_key(&hash) {
				return Err(HopError::DuplicateEntry);
			}
		}

		// Write blob and meta to disk (no lock held during I/O).
		let blob_path = self.blob_path(&hash);
		let meta = HopEntryMeta::new(
			data_len,
			current_block,
			self.retention_blocks,
			recipients,
			sender_alias,
		);
		let meta_bytes = meta.encode();

		if let Err(e) = Self::write_atomic(&blob_path, &data) {
			let _ = fs::remove_file(blob_path.with_extension("tmp"));
			return Err(e);
		}

		let meta_path = self.meta_path(&hash);
		if let Err(e) = Self::write_atomic(&meta_path, &meta_bytes) {
			let _ = fs::remove_file(meta_path.with_extension("tmp"));
			// Clean up the blob we already wrote.
			let _ = fs::remove_file(&blob_path);
			return Err(e);
		}

		// Acquire write lock: double-check duplicate, insert meta, update counters.
		{
			let mut index = self.index.write();
			if index.contains_key(&hash) {
				// Another thread inserted while we were writing to disk.
				let _ = fs::remove_file(&blob_path);
				let _ = fs::remove_file(&meta_path);
				return Err(HopError::DuplicateEntry);
			}
			index.insert(hash, meta);
		}

		// Update size counter and user usage.
		self.current_size.fetch_add(data_len, Ordering::Relaxed);
		*self.user_usage.write().entry(sender_alias).or_insert(0) += data_len;

		tracing::info!(
			target: "hop",
			hash = ?hex::encode(hash),
			size = data_len,
			expires_at = current_block + self.retention_blocks,
			"Data added to HOP pool"
		);

		Ok(hash)
	}

	/// Get data from the pool by content hash.
	pub fn get(&self, hash: &HopHash) -> Option<Vec<u8>> {
		let index = self.index.read();
		if !index.contains_key(hash) {
			return None;
		}
		drop(index);

		match fs::read(self.blob_path(hash)) {
			Ok(data) => Some(data),
			Err(e) => {
				tracing::error!(target: "hop", hash = ?hex::encode(hash), error = %e, "Failed to read blob from disk");
				None
			},
		}
	}

	/// Decode a signature and find the matching recipient index.
	fn find_recipient(
		meta: &HopEntryMeta,
		hash: &HopHash,
		signature: &[u8],
	) -> Result<usize, HopError> {
		let multi_sig =
			MultiSignature::decode(&mut &signature[..]).map_err(|_| HopError::InvalidSignature)?;

		meta.recipients
			.iter()
			.enumerate()
			.find_map(|(i, signer)| {
				let account_id = signer.clone().into_account();
				if multi_sig.verify(hash.as_bytes(), &account_id) {
					Some(i)
				} else {
					None
				}
			})
			.ok_or(HopError::NotRecipient)
	}

	/// Claim data from the pool (read-only). Verifies the signature against recipient
	/// public keys. Returns the data if the signature matches a recipient.
	///
	/// This does NOT mark the recipient as claimed — call `ack` after receiving the data
	/// to confirm receipt.
	///
	/// Returns `AlreadyClaimed` if the recipient has already ack'd (data may be deleted).
	pub fn claim(&self, hash: &HopHash, signature: &[u8]) -> Result<Vec<u8>, HopError> {
		let index = self.index.read();
		let meta = index.get(hash).ok_or(HopError::NotFound)?;
		let recipient_index = Self::find_recipient(meta, hash, signature)?;

		// If this recipient already ack'd, the data may be gone.
		if meta.claimed[recipient_index] {
			return Err(HopError::AlreadyClaimed);
		}

		let blob_path = self.blob_path(hash);
		drop(index);

		// Read blob from disk.
		let data =
			fs::read(blob_path).map_err(|e| HopError::IoError(format!("read blob: {}", e)))?;

		Ok(data)
	}

	/// Acknowledge receipt of claimed data. Marks the recipient as claimed and triggers
	/// cleanup when all recipients have ack'd.
	///
	/// Idempotent: acking a recipient that already ack'd returns `Ok(())`.
	pub fn ack(&self, hash: &HopHash, signature: &[u8]) -> Result<(), HopError> {
		let mut index = self.index.write();
		let meta = index.get_mut(hash).ok_or(HopError::NotFound)?;
		let recipient_index = Self::find_recipient(meta, hash, signature)?;

		// Idempotent: already ack'd is fine.
		if meta.claimed[recipient_index] {
			return Ok(());
		}

		meta.claimed[recipient_index] = true;

		// If all recipients have ack'd, remove the entry entirely.
		if meta.claimed.iter().all(|&c| c) {
			let size = meta.size;
			let alias = meta.sender_alias;
			index.remove(hash);
			// Release locks before disk I/O where possible, but counters first.
			self.current_size.fetch_sub(size, Ordering::Relaxed);
			let mut usage = self.user_usage.write();
			if let Some(u) = usage.get_mut(&alias) {
				*u = u.saturating_sub(size);
				if *u == 0 {
					usage.remove(&alias);
				}
			}
			drop(usage);
			drop(index);

			// Delete files from disk (best-effort; orphans cleaned on restart).
			let _ = fs::remove_file(self.blob_path(hash));
			let _ = fs::remove_file(self.meta_path(hash));

			tracing::info!(
				target: "hop",
				hash = ?hex::encode(hash),
				"All recipients acked, data removed"
			);
		} else {
			let claimed_count = meta.claimed.iter().filter(|&&c| c).count();
			// Persist updated claimed state to disk.
			let meta_bytes = meta.encode();
			let meta_path = self.meta_path(hash);
			drop(index);

			if let Err(e) = Self::write_atomic(&meta_path, &meta_bytes) {
				tracing::error!(target: "hop", hash = ?hex::encode(hash), error = %e, "Failed to persist ack state");
			}

			tracing::debug!(
				target: "hop",
				hash = ?hex::encode(hash),
				claimed = claimed_count,
				"Recipient acked"
			);
		}

		Ok(())
	}

	/// Check if data exists in the pool.
	pub fn has(&self, hash: &HopHash) -> bool {
		let index = self.index.read();
		index.contains_key(hash)
	}

	/// Remove data from the pool.
	pub fn remove(&self, hash: &HopHash) -> Result<(), HopError> {
		let meta = {
			let mut index = self.index.write();
			index.remove(hash)
		};

		if let Some(meta) = meta {
			// Update size counter
			self.current_size.fetch_sub(meta.size, Ordering::Relaxed);
			// Release user quota
			let mut usage = self.user_usage.write();
			if let Some(u) = usage.get_mut(&meta.sender_alias) {
				*u = u.saturating_sub(meta.size);
				if *u == 0 {
					usage.remove(&meta.sender_alias);
				}
			}
			drop(usage);

			// Delete files from disk (best-effort).
			let _ = fs::remove_file(self.blob_path(hash));
			let _ = fs::remove_file(self.meta_path(hash));

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

	/// Get pool status.
	pub fn status(&self) -> PoolStatus {
		let index = self.index.read();
		PoolStatus {
			entry_count: index.len(),
			total_bytes: self.current_size.load(Ordering::Relaxed),
			max_bytes: self.max_size,
		}
	}

	/// Remove expired entries and release their user quotas.
	/// Returns the total bytes freed.
	pub fn cleanup_expired(&self, current_block: HopBlockNumber) -> u64 {
		let mut index = self.index.write();
		let expired: Vec<(HopHash, HopEntryMeta)> = index
			.iter()
			.filter(|(_, m)| current_block >= m.expires_at)
			.map(|(h, m)| (*h, m.clone()))
			.collect();

		let mut freed = 0u64;
		for (hash, meta) in &expired {
			index.remove(hash);
			freed += meta.size;
			let mut usage = self.user_usage.write();
			if let Some(u) = usage.get_mut(&meta.sender_alias) {
				*u = u.saturating_sub(meta.size);
				if *u == 0 {
					usage.remove(&meta.sender_alias);
				}
			}
		}
		self.current_size.fetch_sub(freed, Ordering::Relaxed);
		drop(index);

		// Delete files from disk (best-effort).
		for (hash, _) in &expired {
			let _ = fs::remove_file(self.blob_path(hash));
			let _ = fs::remove_file(self.meta_path(hash));
		}

		freed
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use sp_core::{crypto::Pair, ed25519, sr25519};
	use tempfile::TempDir;

	const ALIAS_A: Alias = [1u8; 32];
	const ALIAS_B: Alias = [2u8; 32];

	fn create_test_pool() -> (HopDataPool, TempDir) {
		let dir = TempDir::new().unwrap();
		let pool = HopDataPool::new(1024 * 1024, 100, dir.path().to_path_buf()).unwrap();
		(pool, dir)
	}

	fn test_recipient() -> (ed25519::Pair, MultiSigner) {
		let pair = ed25519::Pair::from_seed(&[1u8; 32]);
		let signer = MultiSigner::Ed25519(pair.public());
		(pair, signer)
	}

	#[test]
	fn test_insert_and_get() {
		let (pool, _dir) = create_test_pool();
		let (_, signer) = test_recipient();
		let data = vec![1, 2, 3, 4, 5];
		let hash = pool.insert(data.clone(), 0, vec![signer], ALIAS_A).unwrap();

		let retrieved = pool.get(&hash).unwrap();
		assert_eq!(data, retrieved);
	}

	#[test]
	fn test_insert_no_recipients() {
		let (pool, _dir) = create_test_pool();
		let data = vec![1, 2, 3, 4, 5];
		let result = pool.insert(data, 0, vec![], ALIAS_A);
		assert!(matches!(result, Err(HopError::NoRecipients)));
	}

	#[test]
	fn test_duplicate_insert() {
		let (pool, _dir) = create_test_pool();
		let (_, signer) = test_recipient();
		let data = vec![1, 2, 3, 4, 5];

		pool.insert(data.clone(), 0, vec![signer.clone()], ALIAS_A).unwrap();
		let result = pool.insert(data, 0, vec![signer], ALIAS_A);

		assert!(matches!(result, Err(HopError::DuplicateEntry)));
	}

	#[test]
	fn test_data_too_large() {
		let (pool, _dir) = create_test_pool();
		let (_, signer) = test_recipient();
		let data = vec![0u8; (MAX_DATA_SIZE + 1) as usize];

		let result = pool.insert(data, 0, vec![signer], ALIAS_A);
		assert!(matches!(result, Err(HopError::DataTooLarge(_, _))));
	}

	#[test]
	fn test_pool_full() {
		let dir = TempDir::new().unwrap();
		let pool = HopDataPool::new(100, 100, dir.path().to_path_buf()).unwrap();
		let (_, signer) = test_recipient();

		let data1 = vec![0u8; 60];
		let data2 = vec![1u8; 50];

		pool.insert(data1, 0, vec![signer.clone()], ALIAS_A).unwrap();
		let result = pool.insert(data2, 0, vec![signer], ALIAS_A);

		assert!(matches!(result, Err(HopError::PoolFull(_, _))));
	}

	#[test]
	fn test_remove() {
		let (pool, _dir) = create_test_pool();
		let (_, signer) = test_recipient();
		let data = vec![1, 2, 3, 4, 5];
		let hash = pool.insert(data, 0, vec![signer], ALIAS_A).unwrap();

		assert!(pool.has(&hash));
		pool.remove(&hash).unwrap();
		assert!(!pool.has(&hash));

		// Files should be cleaned up.
		assert!(!pool.blob_path(&hash).exists());
		assert!(!pool.meta_path(&hash).exists());
	}

	#[test]
	fn test_status() {
		let (pool, _dir) = create_test_pool();
		let (_, signer) = test_recipient();
		let data1 = vec![1, 2, 3, 4, 5];
		let data2 = vec![6, 7, 8];

		pool.insert(data1.clone(), 0, vec![signer.clone()], ALIAS_A).unwrap();
		pool.insert(data2.clone(), 0, vec![signer], ALIAS_A).unwrap();

		let status = pool.status();
		assert_eq!(status.entry_count, 2);
		assert_eq!(status.total_bytes, (data1.len() + data2.len()) as u64);
	}

	#[test]
	fn test_claim_valid_signature() {
		let (pool, _dir) = create_test_pool();
		let (pair, signer) = test_recipient();
		let data = vec![1, 2, 3, 4, 5];
		let hash = pool.insert(data.clone(), 0, vec![signer], ALIAS_A).unwrap();

		let sig = pair.sign(hash.as_bytes());
		let multi_sig = MultiSignature::Ed25519(sig);
		let encoded_sig = multi_sig.encode();

		// Claim returns data but does NOT remove the entry.
		let result = pool.claim(&hash, &encoded_sig).unwrap();
		assert_eq!(data, result);
		assert!(pool.has(&hash));

		// Ack removes the entry.
		pool.ack(&hash, &encoded_sig).unwrap();
		assert!(!pool.has(&hash));
	}

	#[test]
	fn test_claim_invalid_signature() {
		let (pool, _dir) = create_test_pool();
		let (_, signer) = test_recipient();
		let data = vec![1, 2, 3, 4, 5];
		let hash = pool.insert(data, 0, vec![signer], ALIAS_A).unwrap();

		// Use invalid SCALE bytes — cannot decode as MultiSignature
		let result = pool.claim(&hash, &[0u8; 3]);
		assert!(matches!(result, Err(HopError::InvalidSignature)));
	}

	#[test]
	fn test_claim_wrong_key() {
		let (pool, _dir) = create_test_pool();
		let (_, signer) = test_recipient();
		let data = vec![1, 2, 3, 4, 5];
		let hash = pool.insert(data, 0, vec![signer], ALIAS_A).unwrap();

		// Sign with a different keypair
		let wrong_pair = ed25519::Pair::from_seed(&[99u8; 32]);
		let sig = wrong_pair.sign(hash.as_bytes());
		let multi_sig = MultiSignature::Ed25519(sig);
		let result = pool.claim(&hash, &multi_sig.encode());
		assert!(matches!(result, Err(HopError::NotRecipient)));

		// Entry should still exist
		assert!(pool.has(&hash));
	}

	#[test]
	fn test_claim_multi_recipient() {
		let (pool, _dir) = create_test_pool();
		let pair1 = ed25519::Pair::from_seed(&[1u8; 32]);
		let pair2 = ed25519::Pair::from_seed(&[2u8; 32]);
		let signer1 = MultiSigner::Ed25519(pair1.public());
		let signer2 = MultiSigner::Ed25519(pair2.public());

		let data = vec![1, 2, 3, 4, 5];
		let hash = pool.insert(data.clone(), 0, vec![signer1, signer2], ALIAS_A).unwrap();

		let sig1 = pair1.sign(hash.as_bytes());
		let multi_sig1 = MultiSignature::Ed25519(sig1);
		let encoded_sig1 = multi_sig1.encode();
		let sig2 = pair2.sign(hash.as_bytes());
		let multi_sig2 = MultiSignature::Ed25519(sig2);
		let encoded_sig2 = multi_sig2.encode();

		// Both recipients claim — entry still exists (claims are read-only).
		let result1 = pool.claim(&hash, &encoded_sig1).unwrap();
		assert_eq!(data, result1);
		assert!(pool.has(&hash));

		let result2 = pool.claim(&hash, &encoded_sig2).unwrap();
		assert_eq!(data, result2);
		assert!(pool.has(&hash));

		// First ack — entry still exists.
		pool.ack(&hash, &encoded_sig1).unwrap();
		assert!(pool.has(&hash));

		// Second ack — entry removed.
		pool.ack(&hash, &encoded_sig2).unwrap();
		assert!(!pool.has(&hash));

		// Pool size should be back to 0
		assert_eq!(pool.status().total_bytes, 0);
	}

	#[test]
	fn test_claim_after_ack_returns_already_claimed() {
		let (pool, _dir) = create_test_pool();
		let (pair, signer) = test_recipient();
		let pair2 = ed25519::Pair::from_seed(&[2u8; 32]);
		let signer2 = MultiSigner::Ed25519(pair2.public());

		let data = vec![1, 2, 3, 4, 5];
		let hash = pool.insert(data.clone(), 0, vec![signer, signer2], ALIAS_A).unwrap();

		let sig = pair.sign(hash.as_bytes());
		let multi_sig = MultiSignature::Ed25519(sig);
		let encoded_sig = multi_sig.encode();

		// Claim and ack succeed.
		pool.claim(&hash, &encoded_sig).unwrap();
		pool.ack(&hash, &encoded_sig).unwrap();

		// Claiming again after ack returns AlreadyClaimed.
		let result = pool.claim(&hash, &encoded_sig);
		assert!(matches!(result, Err(HopError::AlreadyClaimed)));
	}

	#[test]
	fn test_claim_not_found() {
		let (pool, _dir) = create_test_pool();
		let fake_hash = H256([0u8; 32]);
		let result = pool.claim(&fake_hash, &[0u8; 64]);
		assert!(matches!(result, Err(HopError::NotFound)));
	}

	#[test]
	fn test_two_users_get_fair_share() {
		// Pool of 200 bytes, two users should each get 100
		let dir = TempDir::new().unwrap();
		let pool = HopDataPool::new(200, 100, dir.path().to_path_buf()).unwrap();
		let (_, signer) = test_recipient();

		// User A inserts 90 bytes — within their 200/1 = 200 limit (only user so far)
		pool.insert(vec![0u8; 90], 0, vec![signer.clone()], ALIAS_A).unwrap();

		// User B inserts 90 bytes — now 2 users, limit is 200/2 = 100 each
		pool.insert(vec![1u8; 90], 0, vec![signer.clone()], ALIAS_B).unwrap();

		// User A tries to insert 20 more — would be 110 total, limit is 100
		let result = pool.insert(vec![2u8; 20], 0, vec![signer.clone()], ALIAS_A);
		assert!(matches!(result, Err(HopError::UserQuotaExceeded { .. })));

		// User B tries to insert 20 more — would be 110 total, limit is 100
		let result = pool.insert(vec![3u8; 20], 0, vec![signer], ALIAS_B);
		assert!(matches!(result, Err(HopError::UserQuotaExceeded { .. })));
	}

	#[test]
	fn test_new_user_counted_in_denominator() {
		// Pool of 200 bytes
		let dir = TempDir::new().unwrap();
		let pool = HopDataPool::new(200, 100, dir.path().to_path_buf()).unwrap();
		let (_, signer) = test_recipient();

		// User A inserts 90 bytes (sole user, limit = 200)
		pool.insert(vec![0u8; 90], 0, vec![signer.clone()], ALIAS_A).unwrap();

		// New user B tries to insert 110 bytes — B is new, so active_users = 2,
		// per_user_limit = 100, and 110 > 100
		let result = pool.insert(vec![1u8; 110], 0, vec![signer.clone()], ALIAS_B);
		assert!(matches!(result, Err(HopError::UserQuotaExceeded { .. })));

		// But B can insert 100 bytes (exactly at limit)
		pool.insert(vec![2u8; 100], 0, vec![signer], ALIAS_B).unwrap();
	}

	#[test]
	fn test_quota_released_after_ack() {
		let dir = TempDir::new().unwrap();
		let pool = HopDataPool::new(200, 100, dir.path().to_path_buf()).unwrap();
		let (pair, signer) = test_recipient();

		// User A inserts 100 bytes
		let hash = pool.insert(vec![0u8; 100], 0, vec![signer.clone()], ALIAS_A).unwrap();

		// User A can't insert 110 more (would be 210, limit = 200 for sole user)
		let result = pool.insert(vec![1u8; 110], 0, vec![signer.clone()], ALIAS_A);
		assert!(matches!(result, Err(HopError::PoolFull(_, _))));

		// Claim does NOT free quota.
		let sig = pair.sign(hash.as_bytes());
		let multi_sig = MultiSignature::Ed25519(sig);
		let encoded_sig = multi_sig.encode();
		pool.claim(&hash, &encoded_sig).unwrap();
		let result = pool.insert(vec![1u8; 110], 0, vec![signer.clone()], ALIAS_A);
		assert!(matches!(result, Err(HopError::PoolFull(_, _))));

		// Ack frees 100 bytes of user quota.
		pool.ack(&hash, &encoded_sig).unwrap();

		// Now user A can insert again
		pool.insert(vec![2u8; 100], 0, vec![signer], ALIAS_A).unwrap();
	}

	#[test]
	fn test_cleanup_expired_releases_quota() {
		let dir = TempDir::new().unwrap();
		let pool = HopDataPool::new(200, 10, dir.path().to_path_buf()).unwrap();
		let (_, signer) = test_recipient();

		// User A inserts at block 0, expires at block 10
		pool.insert(vec![0u8; 100], 0, vec![signer], ALIAS_A).unwrap();

		// Verify usage is tracked
		assert_eq!(pool.user_usage.read().get(&ALIAS_A).copied().unwrap_or(0), 100);

		// Cleanup at block 10 — entry has expired
		let freed = pool.cleanup_expired(10);
		assert_eq!(freed, 100);
		assert_eq!(pool.status().total_bytes, 0);

		// User quota should be released
		assert_eq!(pool.user_usage.read().get(&ALIAS_A), None);
	}

	#[test]
	fn test_user_removed_when_usage_drops_to_zero() {
		let (pool, _dir) = create_test_pool();
		let (pair, signer) = test_recipient();

		let hash = pool.insert(vec![0u8; 50], 0, vec![signer], ALIAS_A).unwrap();
		assert!(pool.user_usage.read().contains_key(&ALIAS_A));

		// Claim does not remove the user from usage map.
		let sig = pair.sign(hash.as_bytes());
		let multi_sig = MultiSignature::Ed25519(sig);
		let encoded_sig = multi_sig.encode();
		pool.claim(&hash, &encoded_sig).unwrap();
		assert!(pool.user_usage.read().contains_key(&ALIAS_A));

		// Ack removes the entry and the user from usage map.
		pool.ack(&hash, &encoded_sig).unwrap();
		assert!(!pool.user_usage.read().contains_key(&ALIAS_A));
	}

	#[test]
	fn test_restart_recovery() {
		let dir = TempDir::new().unwrap();
		let (_, signer) = test_recipient();

		let hash;
		// Create pool, insert data, then drop pool.
		{
			let pool = HopDataPool::new(1024 * 1024, 100, dir.path().to_path_buf()).unwrap();
			hash = pool.insert(vec![42u8; 100], 0, vec![signer], ALIAS_A).unwrap();
			assert!(pool.has(&hash));
			assert_eq!(pool.status().entry_count, 1);
			assert_eq!(pool.status().total_bytes, 100);
		}

		// Re-create pool from same directory — should recover.
		{
			let pool = HopDataPool::new(1024 * 1024, 100, dir.path().to_path_buf()).unwrap();
			assert!(pool.has(&hash));
			assert_eq!(pool.status().entry_count, 1);
			assert_eq!(pool.status().total_bytes, 100);

			// Data should be readable.
			let data = pool.get(&hash).unwrap();
			assert_eq!(data, vec![42u8; 100]);

			// User usage should be recovered.
			assert_eq!(pool.user_usage.read().get(&ALIAS_A).copied().unwrap_or(0), 100);
		}
	}

	#[test]
	fn test_orphan_blob_cleanup() {
		let dir = TempDir::new().unwrap();

		// Create shard directories first.
		{
			let _pool = HopDataPool::new(1024 * 1024, 100, dir.path().to_path_buf()).unwrap();
		}

		// Manually create an orphan .blob (no corresponding .meta).
		let orphan_hash = "aa".to_string() + &"bb".repeat(15);
		let blob_path = dir.path().join("blobs").join("aa").join(format!("{}.blob", orphan_hash));
		fs::write(&blob_path, b"orphan data").unwrap();
		assert!(blob_path.exists());

		// Re-create pool — orphan should be cleaned up.
		let _pool = HopDataPool::new(1024 * 1024, 100, dir.path().to_path_buf()).unwrap();
		assert!(!blob_path.exists());
	}

	#[test]
	fn test_corrupt_meta_cleanup() {
		let dir = TempDir::new().unwrap();

		// Create shard directories first.
		{
			let _pool = HopDataPool::new(1024 * 1024, 100, dir.path().to_path_buf()).unwrap();
		}

		// Write corrupt bytes to a .meta file.
		let fake_hash = "bb".to_string() + &"cc".repeat(15);
		let meta_path = dir.path().join("meta").join("bb").join(format!("{}.meta", fake_hash));
		fs::write(&meta_path, b"not valid SCALE data").unwrap();
		assert!(meta_path.exists());

		// Re-create pool — corrupt .meta should be cleaned up gracefully.
		let pool = HopDataPool::new(1024 * 1024, 100, dir.path().to_path_buf()).unwrap();
		assert!(!meta_path.exists());
		assert_eq!(pool.status().entry_count, 0);
	}

	#[test]
	fn test_claim_sr25519() {
		let (pool, _dir) = create_test_pool();
		let pair = sr25519::Pair::from_seed(&[3u8; 32]);
		let signer = MultiSigner::Sr25519(pair.public());

		let data = vec![10, 20, 30];
		let hash = pool.insert(data.clone(), 0, vec![signer], ALIAS_A).unwrap();

		let sig = pair.sign(hash.as_bytes());
		let multi_sig = MultiSignature::Sr25519(sig);
		let encoded_sig = multi_sig.encode();
		let result = pool.claim(&hash, &encoded_sig).unwrap();
		assert_eq!(data, result);
		assert!(pool.has(&hash));

		pool.ack(&hash, &encoded_sig).unwrap();
		assert!(!pool.has(&hash));
	}

	#[test]
	fn test_claim_is_repeatable() {
		let (pool, _dir) = create_test_pool();
		let (pair, signer) = test_recipient();
		let data = vec![1, 2, 3, 4, 5];
		let hash = pool.insert(data.clone(), 0, vec![signer], ALIAS_A).unwrap();

		let sig = pair.sign(hash.as_bytes());
		let multi_sig = MultiSignature::Ed25519(sig);
		let encoded_sig = multi_sig.encode();

		// Calling claim twice returns the same data both times.
		let result1 = pool.claim(&hash, &encoded_sig).unwrap();
		let result2 = pool.claim(&hash, &encoded_sig).unwrap();
		assert_eq!(data, result1);
		assert_eq!(data, result2);
		assert!(pool.has(&hash));
	}

	#[test]
	fn test_ack_idempotent() {
		let (pool, _dir) = create_test_pool();
		let (pair, signer) = test_recipient();
		let pair2 = ed25519::Pair::from_seed(&[2u8; 32]);
		let signer2 = MultiSigner::Ed25519(pair2.public());

		let data = vec![1, 2, 3, 4, 5];
		let hash = pool.insert(data, 0, vec![signer, signer2], ALIAS_A).unwrap();

		let sig = pair.sign(hash.as_bytes());
		let multi_sig = MultiSignature::Ed25519(sig);
		let encoded_sig = multi_sig.encode();

		// Acking twice succeeds silently.
		pool.ack(&hash, &encoded_sig).unwrap();
		pool.ack(&hash, &encoded_sig).unwrap();
		assert!(pool.has(&hash)); // second recipient hasn't acked
	}

	#[test]
	fn test_multi_recipient_partial_ack() {
		let (pool, _dir) = create_test_pool();
		let pair1 = ed25519::Pair::from_seed(&[1u8; 32]);
		let pair2 = ed25519::Pair::from_seed(&[2u8; 32]);
		let signer1 = MultiSigner::Ed25519(pair1.public());
		let signer2 = MultiSigner::Ed25519(pair2.public());

		let data = vec![1, 2, 3, 4, 5];
		let hash = pool.insert(data.clone(), 0, vec![signer1, signer2], ALIAS_A).unwrap();

		let sig1 = pair1.sign(hash.as_bytes());
		let multi_sig1 = MultiSignature::Ed25519(sig1);
		let encoded_sig1 = multi_sig1.encode();
		let sig2 = pair2.sign(hash.as_bytes());
		let multi_sig2 = MultiSignature::Ed25519(sig2);
		let encoded_sig2 = multi_sig2.encode();

		// R1 claims and acks.
		let result1 = pool.claim(&hash, &encoded_sig1).unwrap();
		assert_eq!(data, result1);
		pool.ack(&hash, &encoded_sig1).unwrap();
		assert!(pool.has(&hash));

		// R2 can still claim.
		let result2 = pool.claim(&hash, &encoded_sig2).unwrap();
		assert_eq!(data, result2);

		// R2 acks — entry deleted.
		pool.ack(&hash, &encoded_sig2).unwrap();
		assert!(!pool.has(&hash));
		assert_eq!(pool.status().total_bytes, 0);
	}

	#[test]
	fn test_claim_mixed_key_types() {
		let (pool, _dir) = create_test_pool();
		let ed_pair = ed25519::Pair::from_seed(&[4u8; 32]);
		let sr_pair = sr25519::Pair::from_seed(&[5u8; 32]);
		let ed_signer = MultiSigner::Ed25519(ed_pair.public());
		let sr_signer = MultiSigner::Sr25519(sr_pair.public());

		let data = vec![42, 43, 44];
		let hash = pool.insert(data.clone(), 0, vec![ed_signer, sr_signer], ALIAS_A).unwrap();

		// sr25519 recipient claims and acks first
		let sr_sig = sr_pair.sign(hash.as_bytes());
		let sr_multi = MultiSignature::Sr25519(sr_sig);
		let sr_encoded = sr_multi.encode();
		let result1 = pool.claim(&hash, &sr_encoded).unwrap();
		assert_eq!(data, result1);
		pool.ack(&hash, &sr_encoded).unwrap();
		assert!(pool.has(&hash)); // ed25519 recipient hasn't acked yet

		// ed25519 recipient claims and acks second
		let ed_sig = ed_pair.sign(hash.as_bytes());
		let ed_multi = MultiSignature::Ed25519(ed_sig);
		let ed_encoded = ed_multi.encode();
		let result2 = pool.claim(&hash, &ed_encoded).unwrap();
		assert_eq!(data, result2);
		pool.ack(&hash, &ed_encoded).unwrap();
		assert!(!pool.has(&hash)); // all acked
	}
}
