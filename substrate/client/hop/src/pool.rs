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
	types::{HopEntryMeta, HopError, PoolStatus, SenderId, MAX_DATA_SIZE},
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

const BLOBS_DIR: &str = "blobs";
const META_DIR: &str = "meta";
const BLOB_EXT: &str = "blob";
const META_EXT: &str = "meta";

/// HOP data pool with disk-backed blob storage and in-memory metadata index.
pub struct HopDataPool {
	/// In-memory metadata index (no blobs).
	index: Arc<RwLock<HashMap<HopHash, HopEntryMeta>>>,
	/// Per-user byte usage tracked by alias.
	user_usage: Arc<RwLock<HashMap<SenderId, u64>>>,
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
			fs::create_dir_all(data_dir.join(BLOBS_DIR).join(&shard))
				.map_err(|e| HopError::IoError(format!("create blobs/{}: {}", shard, e)))?;
			fs::create_dir_all(data_dir.join(META_DIR).join(&shard))
				.map_err(|e| HopError::IoError(format!("create meta/{}: {}", shard, e)))?;
		}

		let mut index = HashMap::new();
		let mut user_usage: HashMap<SenderId, u64> = HashMap::new();
		let mut current_size = 0u64;

		// Rebuild index from .meta files and clean orphan .blobs in a single pass.
		for i in 0u8..=255 {
			let shard = format!("{:02x}", i);

			// Scan .meta files → rebuild index (removes corrupt/orphan .meta files).
			let meta_shard_dir = data_dir.join(META_DIR).join(&shard);
			if let Ok(entries) = fs::read_dir(&meta_shard_dir) {
				for entry in entries.flatten() {
					let path = entry.path();
					if path.extension().and_then(|e| e.to_str()) != Some(META_EXT) {
						continue;
					}

					let stem = match path.file_stem().and_then(|s| s.to_str()) {
						Some(s) => s.to_string(),
						None => continue,
					};

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

					let blob_path = data_dir
						.join(BLOBS_DIR)
						.join(&shard)
						.join(format!("{}.{}", stem, BLOB_EXT));
					if !blob_path.exists() {
						tracing::warn!(target: "hop", hash = ?stem, "Removing orphan .meta (no .blob)");
						let _ = fs::remove_file(&path);
						continue;
					}

					current_size += meta.size;
					*user_usage.entry(meta.sender_id).or_insert(0) += meta.size;
					index.insert(hash, meta);
				}
			}

			// Scan .blob files → remove orphans (blobs without corresponding .meta).
			let blob_shard_dir = data_dir.join(BLOBS_DIR).join(&shard);
			if let Ok(entries) = fs::read_dir(&blob_shard_dir) {
				for entry in entries.flatten() {
					let path = entry.path();
					if path.extension().and_then(|e| e.to_str()) != Some(BLOB_EXT) {
						continue;
					}
					let stem = match path.file_stem().and_then(|s| s.to_str()) {
						Some(s) => s.to_string(),
						None => continue,
					};
					let meta_path =
						data_dir.join(META_DIR).join(&shard).join(format!("{}.{}", stem, META_EXT));
					if !meta_path.exists() {
						tracing::warn!(target: "hop", hash = ?stem, "Removing orphan .blob (no .meta)");
						let _ = fs::remove_file(&path);
					}
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

	/// Release user quota, removing the user entry when usage drops to zero.
	fn release_user_quota(&self, sender_id: &SenderId, size: u64) {
		let mut usage = self.user_usage.write();
		if let Some(u) = usage.get_mut(sender_id) {
			*u = u.saturating_sub(size);
			if *u == 0 {
				usage.remove(sender_id);
			}
		}
	}

	/// Path to a file within a shard subdirectory.
	fn entry_path(&self, hash: &HopHash, subdir: &str, ext: &str) -> PathBuf {
		let hex = hex::encode(hash);
		self.data_dir.join(subdir).join(&hex[..2]).join(format!("{}.{}", hex, ext))
	}

	/// Path to the blob file for a given hash.
	fn blob_path(&self, hash: &HopHash) -> PathBuf {
		self.entry_path(hash, BLOBS_DIR, BLOB_EXT)
	}

	/// Path to the meta file for a given hash.
	fn meta_path(&self, hash: &HopHash) -> PathBuf {
		self.entry_path(hash, META_DIR, META_EXT)
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
		sender_id: SenderId,
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

		// Eagerly reserve pool capacity. Roll back on any subsequent failure.
		let prev_size = self.current_size.fetch_add(data_len, Ordering::Relaxed);
		if prev_size + data_len > self.max_size {
			self.current_size.fetch_sub(data_len, Ordering::Relaxed);
			return Err(HopError::PoolFull(prev_size, self.max_size));
		}

		// Per-user quota enforcement (soft limit — the pool capacity above is the
		// hard safety net; two concurrent inserts from the same user could both pass
		// this check, but they cannot exceed max_size).
		{
			let usage_map = self.user_usage.read();
			let current_usage = usage_map.get(&sender_id).copied().unwrap_or(0);
			let is_new_user = current_usage == 0;
			let active_users =
				if is_new_user { usage_map.len() as u64 + 1 } else { usage_map.len() as u64 };
			let per_user_limit = self.max_size / active_users.max(1);

			if current_usage + data_len > per_user_limit {
				self.current_size.fetch_sub(data_len, Ordering::Relaxed);
				return Err(HopError::UserQuotaExceeded {
					used: current_usage,
					limit: per_user_limit,
				});
			}
		}

		let hash = H256(blake2_256(&data));

		// First duplicate check (read lock only).
		{
			let index = self.index.read();
			if index.contains_key(&hash) {
				self.current_size.fetch_sub(data_len, Ordering::Relaxed);
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
			sender_id,
		);
		let meta_bytes = meta.encode();

		if let Err(e) = Self::write_atomic(&blob_path, &data) {
			let _ = fs::remove_file(blob_path.with_extension("tmp"));
			self.current_size.fetch_sub(data_len, Ordering::Relaxed);
			return Err(e);
		}

		let meta_path = self.meta_path(&hash);
		if let Err(e) = Self::write_atomic(&meta_path, &meta_bytes) {
			let _ = fs::remove_file(meta_path.with_extension("tmp"));
			let _ = fs::remove_file(&blob_path);
			self.current_size.fetch_sub(data_len, Ordering::Relaxed);
			return Err(e);
		}

		// Acquire write lock: double-check duplicate, insert meta.
		{
			let mut index = self.index.write();
			if index.contains_key(&hash) {
				// Race: another thread inserted the same data while we were writing.
				let _ = fs::remove_file(&meta_path);
				let _ = fs::remove_file(&blob_path);
				self.current_size.fetch_sub(data_len, Ordering::Relaxed);
				return Err(HopError::DuplicateEntry);
			}
			index.insert(hash, meta);
		}

		// Update user usage (pool capacity already reserved above).
		*self.user_usage.write().entry(sender_id).or_insert(0) += data_len;

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
	/// Returns `AlreadyClaimed` if the recipient has already acked (data may be deleted).
	pub fn claim(&self, hash: &HopHash, signature: &[u8]) -> Result<Vec<u8>, HopError> {
		let index = self.index.read();
		let meta = index.get(hash).ok_or(HopError::NotFound)?;
		let recipient_index = Self::find_recipient(meta, hash, signature)?;

		// If this recipient already acked, the data may be gone.
		if meta.claimed[recipient_index] {
			return Err(HopError::AlreadyClaimed);
		}

		let blob_path = self.blob_path(hash);
		drop(index);

		// Read blob from disk (may be gone if concurrently acked and deleted).
		let data = fs::read(&blob_path).map_err(|e| {
			if e.kind() == std::io::ErrorKind::NotFound {
				HopError::NotFound
			} else {
				HopError::IoError(format!("read blob: {}", e))
			}
		})?;

		Ok(data)
	}

	/// Acknowledge receipt of claimed data. Marks the recipient as claimed and triggers
	/// cleanup when all recipients have acked.
	///
	/// Idempotent: acking a recipient that already acked returns `Ok(())`.
	pub fn ack(&self, hash: &HopHash, signature: &[u8]) -> Result<(), HopError> {
		// Phase 1: find recipient under read lock (crypto verification happens here).
		let recipient_index = {
			let index = self.index.read();
			let meta = index.get(hash).ok_or(HopError::NotFound)?;
			let idx = Self::find_recipient(meta, hash, signature)?;

			// Fast path: already acked (idempotent).
			if meta.claimed[idx] {
				return Ok(());
			}
			idx
		};

		// Phase 2: write lock — re-verify and update atomically.
		let mut index = self.index.write();
		let meta = index.get_mut(hash).ok_or(HopError::NotFound)?;

		// Re-check under write lock (concurrent ack may have beaten us).
		if meta.claimed[recipient_index] {
			return Ok(());
		}

		meta.claimed[recipient_index] = true;

		// If all recipients have acked, remove the entry entirely.
		if meta.claimed.iter().all(|&c| c) {
			let size = meta.size;
			let sender = meta.sender_id;
			index.remove(hash);
			self.current_size.fetch_sub(size, Ordering::Relaxed);
			self.release_user_quota(&sender, size);
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
			self.current_size.fetch_sub(meta.size, Ordering::Relaxed);
			self.release_user_quota(&meta.sender_id, meta.size);

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
		// Phase 1: Under index write lock — collect and remove expired entries.
		let expired: Vec<(HopHash, HopEntryMeta)> = {
			let mut index = self.index.write();
			let expired_keys: Vec<HopHash> = index
				.iter()
				.filter(|(_, m)| current_block >= m.expires_at)
				.map(|(h, _)| *h)
				.collect();

			expired_keys
				.into_iter()
				.filter_map(|hash| index.remove(&hash).map(|meta| (hash, meta)))
				.collect()
		};
		// index write lock released here.

		if expired.is_empty() {
			return 0;
		}

		// Phase 2: Update counters and batch user-quota release (single lock acquisition).
		let freed: u64 = expired.iter().map(|(_, meta)| meta.size).sum();
		self.current_size.fetch_sub(freed, Ordering::Relaxed);

		{
			let mut usage = self.user_usage.write();
			for (_, meta) in &expired {
				if let Some(u) = usage.get_mut(&meta.sender_id) {
					*u = u.saturating_sub(meta.size);
					if *u == 0 {
						usage.remove(&meta.sender_id);
					}
				}
			}
		}

		// Phase 3: Delete files from disk (best-effort, no locks held).
		for (hash, _) in &expired {
			let _ = fs::remove_file(self.blob_path(hash));
			let _ = fs::remove_file(self.meta_path(hash));
		}

		freed
	}

	/// Return hashes of entries within `buffer_blocks` of expiry that have not yet been promoted.
	/// Returns up to `limit` hashes. Use [`get`] to read blob data when needed.
	/// The maintenance task runs periodically, so remaining entries are picked up next cycle.
	pub fn get_promotable(
		&self,
		current_block: HopBlockNumber,
		buffer_blocks: u32,
		limit: usize,
	) -> Vec<HopHash> {
		let index = self.index.read();
		index
			.iter()
			.filter(|(_, meta)| {
				!meta.promoted && current_block.saturating_add(buffer_blocks) >= meta.expires_at
			})
			.map(|(h, _)| *h)
			.take(limit)
			.collect()
	}

	/// Mark an entry as promoted to permanent on-chain storage.
	/// Persists the updated metadata to disk.
	pub fn mark_promoted(&self, hash: &HopHash) {
		let mut index = self.index.write();
		if let Some(meta) = index.get_mut(hash) {
			meta.promoted = true;
			let meta_bytes = meta.encode();
			let meta_path = self.meta_path(hash);
			drop(index);

			if let Err(e) = Self::write_atomic(&meta_path, &meta_bytes) {
				tracing::error!(
					target: "hop",
					hash = ?hex::encode(hash),
					error = %e,
					"Failed to persist promoted state"
				);
			}
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use sp_core::{crypto::Pair, ed25519, sr25519};
	use tempfile::TempDir;

	const SENDER_A: SenderId = [1u8; 32];
	const SENDER_B: SenderId = [2u8; 32];

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
		let hash = pool.insert(data.clone(), 0, vec![signer], SENDER_A).unwrap();

		let retrieved = pool.get(&hash).unwrap();
		assert_eq!(data, retrieved);
	}

	#[test]
	fn test_insert_no_recipients() {
		let (pool, _dir) = create_test_pool();
		let data = vec![1, 2, 3, 4, 5];
		let result = pool.insert(data, 0, vec![], SENDER_A);
		assert!(matches!(result, Err(HopError::NoRecipients)));
	}

	#[test]
	fn test_duplicate_insert() {
		let (pool, _dir) = create_test_pool();
		let (_, signer) = test_recipient();
		let data = vec![1, 2, 3, 4, 5];

		pool.insert(data.clone(), 0, vec![signer.clone()], SENDER_A).unwrap();
		let result = pool.insert(data, 0, vec![signer], SENDER_A);

		assert!(matches!(result, Err(HopError::DuplicateEntry)));
	}

	#[test]
	fn test_data_too_large() {
		let (pool, _dir) = create_test_pool();
		let (_, signer) = test_recipient();
		let data = vec![0u8; (MAX_DATA_SIZE + 1) as usize];

		let result = pool.insert(data, 0, vec![signer], SENDER_A);
		assert!(matches!(result, Err(HopError::DataTooLarge(_, _))));
	}

	#[test]
	fn test_pool_full() {
		let dir = TempDir::new().unwrap();
		let pool = HopDataPool::new(100, 100, dir.path().to_path_buf()).unwrap();
		let (_, signer) = test_recipient();

		let data1 = vec![0u8; 60];
		let data2 = vec![1u8; 50];

		pool.insert(data1, 0, vec![signer.clone()], SENDER_A).unwrap();
		let result = pool.insert(data2, 0, vec![signer], SENDER_A);

		assert!(matches!(result, Err(HopError::PoolFull(_, _))));
	}

	#[test]
	fn test_remove() {
		let (pool, _dir) = create_test_pool();
		let (_, signer) = test_recipient();
		let data = vec![1, 2, 3, 4, 5];
		let hash = pool.insert(data, 0, vec![signer], SENDER_A).unwrap();

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

		pool.insert(data1.clone(), 0, vec![signer.clone()], SENDER_A).unwrap();
		pool.insert(data2.clone(), 0, vec![signer], SENDER_A).unwrap();

		let status = pool.status();
		assert_eq!(status.entry_count, 2);
		assert_eq!(status.total_bytes, (data1.len() + data2.len()) as u64);
	}

	#[test]
	fn test_claim_valid_signature() {
		let (pool, _dir) = create_test_pool();
		let (pair, signer) = test_recipient();
		let data = vec![1, 2, 3, 4, 5];
		let hash = pool.insert(data.clone(), 0, vec![signer], SENDER_A).unwrap();

		let sig = pair.sign(hash.as_bytes());
		let multi_sig = MultiSignature::Ed25519(sig);
		let encoded_sig = multi_sig.encode();
		let result = pool.claim(&hash, &encoded_sig).unwrap();
		assert_eq!(data, result);

		// Entry still exists until ack
		assert!(pool.has(&hash));

		pool.ack(&hash, &encoded_sig).unwrap();
		assert!(!pool.has(&hash));
	}

	#[test]
	fn test_claim_invalid_signature() {
		let (pool, _dir) = create_test_pool();
		let (_, signer) = test_recipient();
		let data = vec![1, 2, 3, 4, 5];
		let hash = pool.insert(data, 0, vec![signer], SENDER_A).unwrap();

		// Use invalid SCALE bytes — cannot decode as MultiSignature
		let result = pool.claim(&hash, &[0u8; 3]);
		assert!(matches!(result, Err(HopError::InvalidSignature)));
	}

	#[test]
	fn test_claim_wrong_key() {
		let (pool, _dir) = create_test_pool();
		let (_, signer) = test_recipient();
		let data = vec![1, 2, 3, 4, 5];
		let hash = pool.insert(data, 0, vec![signer], SENDER_A).unwrap();

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
		let hash = pool.insert(data.clone(), 0, vec![signer1, signer2], SENDER_A).unwrap();

		// First recipient claims and acks
		let sig1 = pair1.sign(hash.as_bytes());
		let multi_sig1 = MultiSignature::Ed25519(sig1);
		let encoded_sig1 = multi_sig1.encode();
		let result1 = pool.claim(&hash, &encoded_sig1).unwrap();
		assert_eq!(data, result1);
		pool.ack(&hash, &encoded_sig1).unwrap();
		assert!(pool.has(&hash)); // still exists, second recipient hasn't acked

		// Second recipient claims and acks
		let sig2 = pair2.sign(hash.as_bytes());
		let multi_sig2 = MultiSignature::Ed25519(sig2);
		let encoded_sig2 = multi_sig2.encode();
		let result2 = pool.claim(&hash, &encoded_sig2).unwrap();
		assert_eq!(data, result2);
		pool.ack(&hash, &encoded_sig2).unwrap();
		assert!(!pool.has(&hash)); // now removed

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
		let hash = pool.insert(data.clone(), 0, vec![signer, signer2], SENDER_A).unwrap();

		// Claim and ack succeeds
		let sig = pair.sign(hash.as_bytes());
		let multi_sig = MultiSignature::Ed25519(sig);
		let encoded_sig = multi_sig.encode();
		pool.claim(&hash, &encoded_sig).unwrap();
		pool.ack(&hash, &encoded_sig).unwrap();

		// Same recipient tries to claim again — should fail (already acked)
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
		pool.insert(vec![0u8; 90], 0, vec![signer.clone()], SENDER_A).unwrap();

		// User B inserts 90 bytes — now 2 users, limit is 200/2 = 100 each
		pool.insert(vec![1u8; 90], 0, vec![signer.clone()], SENDER_B).unwrap();

		// User A tries to insert 20 more — would be 110 total, limit is 100
		let result = pool.insert(vec![2u8; 20], 0, vec![signer.clone()], SENDER_A);
		assert!(matches!(result, Err(HopError::UserQuotaExceeded { .. })));

		// User B tries to insert 20 more — would be 110 total, limit is 100
		let result = pool.insert(vec![3u8; 20], 0, vec![signer], SENDER_B);
		assert!(matches!(result, Err(HopError::UserQuotaExceeded { .. })));
	}

	#[test]
	fn test_new_user_counted_in_denominator() {
		// Pool of 200 bytes
		let dir = TempDir::new().unwrap();
		let pool = HopDataPool::new(200, 100, dir.path().to_path_buf()).unwrap();
		let (_, signer) = test_recipient();

		// User A inserts 90 bytes (sole user, limit = 200)
		pool.insert(vec![0u8; 90], 0, vec![signer.clone()], SENDER_A).unwrap();

		// New user B tries to insert 110 bytes — B is new, so active_users = 2,
		// per_user_limit = 100, and 110 > 100
		let result = pool.insert(vec![1u8; 110], 0, vec![signer.clone()], SENDER_B);
		assert!(matches!(result, Err(HopError::UserQuotaExceeded { .. })));

		// But B can insert 100 bytes (exactly at limit)
		pool.insert(vec![2u8; 100], 0, vec![signer], SENDER_B).unwrap();
	}

	#[test]
	fn test_quota_released_after_ack() {
		let dir = TempDir::new().unwrap();
		let pool = HopDataPool::new(200, 100, dir.path().to_path_buf()).unwrap();
		let (pair, signer) = test_recipient();

		// User A inserts 100 bytes
		let hash = pool.insert(vec![0u8; 100], 0, vec![signer.clone()], SENDER_A).unwrap();

		// User A can't insert 110 more (would be 210, limit = 200 for sole user)
		let result = pool.insert(vec![1u8; 110], 0, vec![signer.clone()], SENDER_A);
		assert!(matches!(result, Err(HopError::PoolFull(_, _))));

		// Claim and ack the first entry — frees 100 bytes of user quota
		let sig = pair.sign(hash.as_bytes());
		let multi_sig = MultiSignature::Ed25519(sig);
		let encoded_sig = multi_sig.encode();
		pool.claim(&hash, &encoded_sig).unwrap();
		pool.ack(&hash, &encoded_sig).unwrap();

		// Now user A can insert again
		pool.insert(vec![2u8; 100], 0, vec![signer], SENDER_A).unwrap();
	}

	#[test]
	fn test_cleanup_expired_releases_quota() {
		let dir = TempDir::new().unwrap();
		let pool = HopDataPool::new(200, 10, dir.path().to_path_buf()).unwrap();
		let (_, signer) = test_recipient();

		// User A inserts at block 0, expires at block 10
		pool.insert(vec![0u8; 100], 0, vec![signer], SENDER_A).unwrap();

		// Verify usage is tracked
		assert_eq!(pool.user_usage.read().get(&SENDER_A).copied().unwrap_or(0), 100);

		// Cleanup at block 10 — entry has expired
		let freed = pool.cleanup_expired(10);
		assert_eq!(freed, 100);
		assert_eq!(pool.status().total_bytes, 0);

		// User quota should be released
		assert_eq!(pool.user_usage.read().get(&SENDER_A), None);
	}

	#[test]
	fn test_user_removed_when_usage_drops_to_zero() {
		let (pool, _dir) = create_test_pool();
		let (pair, signer) = test_recipient();

		let hash = pool.insert(vec![0u8; 50], 0, vec![signer], SENDER_A).unwrap();
		assert!(pool.user_usage.read().contains_key(&SENDER_A));

		// Claim and ack removes the entry
		let sig = pair.sign(hash.as_bytes());
		let multi_sig = MultiSignature::Ed25519(sig);
		let encoded_sig = multi_sig.encode();
		pool.claim(&hash, &encoded_sig).unwrap();
		pool.ack(&hash, &encoded_sig).unwrap();

		// User A should no longer be in usage map
		assert!(!pool.user_usage.read().contains_key(&SENDER_A));
	}

	#[test]
	fn test_restart_recovery() {
		let dir = TempDir::new().unwrap();
		let (_, signer) = test_recipient();

		let hash;
		// Create pool, insert data, then drop pool.
		{
			let pool = HopDataPool::new(1024 * 1024, 100, dir.path().to_path_buf()).unwrap();
			hash = pool.insert(vec![42u8; 100], 0, vec![signer], SENDER_A).unwrap();
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
			assert_eq!(pool.user_usage.read().get(&SENDER_A).copied().unwrap_or(0), 100);
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
		let hash = pool.insert(data.clone(), 0, vec![signer], SENDER_A).unwrap();

		let sig = pair.sign(hash.as_bytes());
		let multi_sig = MultiSignature::Sr25519(sig);
		let encoded_sig = multi_sig.encode();
		let result = pool.claim(&hash, &encoded_sig).unwrap();
		assert_eq!(data, result);

		pool.ack(&hash, &encoded_sig).unwrap();
		assert!(!pool.has(&hash));
	}

	#[test]
	fn test_claim_mixed_key_types() {
		let (pool, _dir) = create_test_pool();
		let ed_pair = ed25519::Pair::from_seed(&[4u8; 32]);
		let sr_pair = sr25519::Pair::from_seed(&[5u8; 32]);
		let ed_signer = MultiSigner::Ed25519(ed_pair.public());
		let sr_signer = MultiSigner::Sr25519(sr_pair.public());

		let data = vec![42, 43, 44];
		let hash = pool.insert(data.clone(), 0, vec![ed_signer, sr_signer], SENDER_A).unwrap();

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

	#[test]
	fn test_claim_is_repeatable() {
		let (pool, _dir) = create_test_pool();
		let (pair, signer) = test_recipient();
		let data = vec![1, 2, 3, 4, 5];
		let hash = pool.insert(data.clone(), 0, vec![signer], SENDER_A).unwrap();

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
		let hash = pool.insert(data, 0, vec![signer, signer2], SENDER_A).unwrap();

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
		let hash = pool.insert(data.clone(), 0, vec![signer1, signer2], SENDER_A).unwrap();

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
	fn test_concurrent_inserts_respect_capacity() {
		use std::{sync::Barrier, thread};

		let dir = TempDir::new().unwrap();
		// Pool of 200 bytes.
		let pool = Arc::new(HopDataPool::new(200, 100, dir.path().to_path_buf()).unwrap());
		let (_, signer) = test_recipient();
		let barrier = Arc::new(Barrier::new(10));

		let handles: Vec<_> = (0..10u8)
			.map(|i| {
				let pool = pool.clone();
				let signer = signer.clone();
				let barrier = barrier.clone();
				thread::spawn(move || {
					barrier.wait();
					// Each thread tries to insert 50 bytes with unique data.
					pool.insert(vec![i; 50], 0, vec![signer], SENDER_A)
				})
			})
			.collect();

		let results: Vec<_> = handles.into_iter().map(|h| h.join().unwrap()).collect();
		let successes = results.iter().filter(|r| r.is_ok()).count();

		// At most 4 inserts of 50 bytes each can fit in 200 bytes.
		assert!(successes <= 4, "Got {} successes, max should be 4", successes);
		assert!(pool.status().total_bytes <= 200);
	}

	#[test]
	fn test_concurrent_inserts_respect_user_quota() {
		use std::{sync::Barrier, thread};

		let dir = TempDir::new().unwrap();
		// Pool of 1000 bytes; two users each get 500.
		let pool = Arc::new(HopDataPool::new(1000, 100, dir.path().to_path_buf()).unwrap());
		let (_, signer) = test_recipient();

		// Pre-seed user B so that user A's quota is 500.
		pool.insert(vec![255u8; 10], 0, vec![signer.clone()], SENDER_B).unwrap();

		let barrier = Arc::new(Barrier::new(10));

		let handles: Vec<_> = (0..10u8)
			.map(|i| {
				let pool = pool.clone();
				let signer = signer.clone();
				let barrier = barrier.clone();
				thread::spawn(move || {
					barrier.wait();
					// Each thread: user A inserts 100 bytes.
					pool.insert(vec![i; 100], 0, vec![signer], SENDER_A)
				})
			})
			.collect();

		let results: Vec<_> = handles.into_iter().map(|h| h.join().unwrap()).collect();
		let successes = results.iter().filter(|r| r.is_ok()).count();

		// User quota is a soft limit (checked under read lock), so concurrent
		// inserts may exceed it. The hard safety net is pool capacity (1000 bytes).
		assert!(successes >= 1, "At least one insert should succeed");
		assert!(pool.status().total_bytes <= 1000);
	}

	#[test]
	fn test_concurrent_claim_and_ack() {
		use std::{sync::Barrier, thread};

		let dir = Arc::new(TempDir::new().unwrap());
		let pool = Arc::new(HopDataPool::new(1024 * 1024, 100, dir.path().to_path_buf()).unwrap());

		// Create 5 recipients.
		let pairs: Vec<_> = (1..=5u8)
			.map(|i| {
				let pair = ed25519::Pair::from_seed(&[i; 32]);
				let signer = MultiSigner::Ed25519(pair.public());
				(pair, signer)
			})
			.collect();

		let signers: Vec<_> = pairs.iter().map(|(_, s)| s.clone()).collect();
		let data = vec![42u8; 100];
		let hash = pool.insert(data.clone(), 0, signers, SENDER_A).unwrap();

		let barrier = Arc::new(Barrier::new(5));

		// Each recipient claims and acks concurrently.
		let handles: Vec<_> = pairs
			.into_iter()
			.map(|(pair, _)| {
				let pool = pool.clone();
				let barrier = barrier.clone();
				let data = data.clone();
				thread::spawn(move || {
					barrier.wait();
					let sig = pair.sign(hash.as_bytes());
					let multi_sig = MultiSignature::Ed25519(sig);
					let encoded = multi_sig.encode();

					let claimed = pool.claim(&hash, &encoded).unwrap();
					assert_eq!(data, claimed);

					pool.ack(&hash, &encoded).unwrap();
				})
			})
			.collect();

		for h in handles {
			h.join().unwrap();
		}

		// All recipients acked — entry should be gone.
		assert!(!pool.has(&hash));
		assert_eq!(pool.status().total_bytes, 0);
	}

	#[test]
	fn test_get_promotable_within_buffer() {
		let dir = TempDir::new().unwrap();
		// retention=100 blocks, so entries added at block 0 expire at block 100
		let pool = HopDataPool::new(1024 * 1024, 100, dir.path().to_path_buf()).unwrap();
		let (_, signer) = test_recipient();

		let hash = pool.insert(vec![1, 2, 3], 0, vec![signer], SENDER_A).unwrap();

		// At block 50 with buffer=30 → 50+30=80 < 100 → not promotable
		let promotable = pool.get_promotable(50, 30, usize::MAX);
		assert!(promotable.is_empty());

		// At block 80 with buffer=30 → 80+30=110 >= 100 → promotable
		let promotable = pool.get_promotable(80, 30, usize::MAX);
		assert_eq!(promotable.len(), 1);
		assert_eq!(promotable[0], hash);
	}

	#[test]
	fn test_get_promotable_excludes_promoted() {
		let dir = TempDir::new().unwrap();
		let pool = HopDataPool::new(1024 * 1024, 100, dir.path().to_path_buf()).unwrap();
		let (_, signer) = test_recipient();

		let hash = pool.insert(vec![1, 2, 3], 0, vec![signer], SENDER_A).unwrap();

		// Entry is promotable
		let promotable = pool.get_promotable(80, 30, usize::MAX);
		assert_eq!(promotable.len(), 1);

		// Mark promoted
		pool.mark_promoted(&hash);

		// Now excluded
		let promotable = pool.get_promotable(80, 30, usize::MAX);
		assert!(promotable.is_empty());
	}

	#[test]
	fn test_mark_promoted_persists_across_restart() {
		let dir = TempDir::new().unwrap();
		let (_, signer) = test_recipient();

		let hash;
		{
			let pool = HopDataPool::new(1024 * 1024, 100, dir.path().to_path_buf()).unwrap();
			hash = pool.insert(vec![42u8; 10], 0, vec![signer], SENDER_A).unwrap();
			pool.mark_promoted(&hash);
		}

		// Re-create pool — promoted state should be recovered
		{
			let pool = HopDataPool::new(1024 * 1024, 100, dir.path().to_path_buf()).unwrap();
			let promotable = pool.get_promotable(80, 30, usize::MAX);
			assert!(promotable.is_empty(), "promoted entry should not be promotable after restart");
			assert!(pool.has(&hash), "entry should still exist");
		}
	}

	#[test]
	fn test_cleanup_expired_removes_promoted() {
		let dir = TempDir::new().unwrap();
		let pool = HopDataPool::new(1024 * 1024, 10, dir.path().to_path_buf()).unwrap();
		let (_, signer) = test_recipient();

		let hash = pool.insert(vec![1, 2, 3], 0, vec![signer], SENDER_A).unwrap();
		pool.mark_promoted(&hash);

		// Entry is promoted but still in pool
		assert!(pool.has(&hash));

		// Cleanup at block 10 — entry has expired, should be removed even though promoted
		let freed = pool.cleanup_expired(10);
		assert!(freed > 0);
		assert!(!pool.has(&hash));
	}
}
