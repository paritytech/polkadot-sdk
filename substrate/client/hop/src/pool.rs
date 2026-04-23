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
	rate_limit::{RateLimitConfig, RateLimiter},
	types::{
		entry_accounted_size, signing_payload, HopBlockNumber, HopEntryMeta, HopError, HopHash,
		PoolStatus, RecipientVec, SenderId, HOP_ACK_CONTEXT, HOP_CLAIM_CONTEXT, MAX_DATA_SIZE,
	},
};
use codec::{Decode, Encode};
use parking_lot::RwLock;
use sp_core::{hashing::blake2_256, H256};
use sp_runtime::{
	traits::{IdentifyAccount, Verify},
	MultiSignature, MultiSigner,
};
use std::{
	collections::{BTreeSet, HashMap},
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
	/// Per-user byte usage tracked by sender id. Counters are shared `Arc`s and
	/// never removed from the map; this avoids a TOCTOU race where a concurrent
	/// insert could increment an orphaned counter after a release removed it.
	user_usage: Arc<RwLock<HashMap<SenderId, Arc<AtomicU64>>>>,
	/// Maximum pool size in bytes (counts both data and per-entry metadata overhead).
	max_size: u64,
	/// Fixed hard per-user quota in bytes.
	max_user_size: u64,
	/// Current pool size in bytes (accounted size — includes metadata overhead).
	current_size: AtomicU64,
	/// Data retention period in blocks.
	retention_blocks: u32,
	/// Root data directory containing blobs/ and meta/ subdirectories.
	data_dir: PathBuf,
	/// Per-account submit rate limiter.
	rate_limiter: Arc<RateLimiter>,
}

impl HopDataPool {
	/// Create a new disk-backed data pool.
	///
	/// Creates shard directories under `data_dir` and rebuilds the in-memory index
	/// from existing `.meta` files on disk (recovery after restart).
	pub fn new(
		max_size: u64,
		max_user_size: u64,
		retention_blocks: u32,
		data_dir: PathBuf,
		rate_limit_cfg: RateLimitConfig,
	) -> Result<Self, HopError> {
		// Create shard directories (256 each for blobs/ and meta/).
		for i in 0u8..=255 {
			let shard = format!("{:02x}", i);
			fs::create_dir_all(data_dir.join(BLOBS_DIR).join(&shard))?;
			fs::create_dir_all(data_dir.join(META_DIR).join(&shard))?;
		}

		let mut index = HashMap::new();
		let mut user_usage: HashMap<SenderId, Arc<AtomicU64>> = HashMap::new();
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

					let accounted = entry_accounted_size(meta.size, meta.recipients.len());
					current_size += accounted;
					user_usage
						.entry(meta.sender_id)
						.or_insert_with(|| Arc::new(AtomicU64::new(0)))
						.fetch_add(accounted, Ordering::Relaxed);
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
			max_user_size,
			current_size: AtomicU64::new(current_size),
			retention_blocks,
			data_dir,
			rate_limiter: Arc::new(RateLimiter::new(rate_limit_cfg)),
		})
	}

	/// Return the per-user shared counter, creating a zero-initialized one if absent.
	fn user_counter(&self, sender_id: &SenderId) -> Arc<AtomicU64> {
		if let Some(c) = self.user_usage.read().get(sender_id).cloned() {
			return c;
		}
		self.user_usage
			.write()
			.entry(*sender_id)
			.or_insert_with(|| Arc::new(AtomicU64::new(0)))
			.clone()
	}

	/// Decrement a user's usage counter. Counters are never removed from the map
	/// to prevent a TOCTOU race with concurrent reservations.
	fn release_user_quota(&self, sender_id: &SenderId, accounted: u64) {
		if let Some(c) = self.user_usage.read().get(sender_id) {
			let prev = c.load(Ordering::Relaxed);
			c.fetch_sub(accounted.min(prev), Ordering::Relaxed);
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
		fs::write(&tmp_path, data)?;
		fs::rename(&tmp_path, path)?;
		Ok(())
	}

	/// Insert data into the pool.
	///
	/// Returns the hash of the data.
	pub fn insert(
		&self,
		data: Vec<u8>,
		current_block: HopBlockNumber,
		recipients: RecipientVec,
		sender_id: SenderId,
	) -> Result<HopHash, HopError> {
		if recipients.is_empty() {
			return Err(HopError::NoRecipients);
		}
		let unique: BTreeSet<&MultiSigner> = recipients.iter().collect();
		if unique.len() != recipients.len() {
			return Err(HopError::DuplicateRecipient);
		}

		if data.is_empty() {
			return Err(HopError::EmptyData);
		}

		let data_len = data.len() as u64;
		if data_len > MAX_DATA_SIZE {
			return Err(HopError::DataTooLarge(data.len(), MAX_DATA_SIZE));
		}

		// Rejected requests never reserve capacity — check before any atomic bump.
		if let Err(retry_after_secs) = self.rate_limiter.check(&sender_id, data_len) {
			return Err(HopError::RateLimited { retry_after_secs });
		}

		// Total accounted size includes bounded per-recipient metadata overhead so
		// a submitter cannot inflate memory via large recipient lists while the
		// capacity counter only tracks `data.len()`.
		let accounted = entry_accounted_size(data_len, recipients.len());

		let prev_size = self.current_size.fetch_add(accounted, Ordering::Relaxed);
		if prev_size + accounted > self.max_size {
			self.current_size.fetch_sub(accounted, Ordering::Relaxed);
			return Err(HopError::PoolFull(prev_size, self.max_size));
		}

		let user_counter = self.user_counter(&sender_id);
		let prev_user = user_counter.fetch_add(accounted, Ordering::Relaxed);
		if prev_user + accounted > self.max_user_size {
			user_counter.fetch_sub(accounted, Ordering::Relaxed);
			self.current_size.fetch_sub(accounted, Ordering::Relaxed);
			return Err(HopError::UserQuotaExceeded { used: prev_user, limit: self.max_user_size });
		}

		let hash = H256(blake2_256(&data));

		// First duplicate check (read lock only).
		{
			let index = self.index.read();
			if index.contains_key(&hash) {
				user_counter.fetch_sub(accounted, Ordering::Relaxed);
				self.current_size.fetch_sub(accounted, Ordering::Relaxed);
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
			user_counter.fetch_sub(accounted, Ordering::Relaxed);
			self.current_size.fetch_sub(accounted, Ordering::Relaxed);
			return Err(e);
		}

		let meta_path = self.meta_path(&hash);
		if let Err(e) = Self::write_atomic(&meta_path, &meta_bytes) {
			let _ = fs::remove_file(meta_path.with_extension("tmp"));
			let _ = fs::remove_file(&blob_path);
			user_counter.fetch_sub(accounted, Ordering::Relaxed);
			self.current_size.fetch_sub(accounted, Ordering::Relaxed);
			return Err(e);
		}

		// Acquire write lock: double-check duplicate, insert meta.
		{
			let mut index = self.index.write();
			if index.contains_key(&hash) {
				// Race: another thread inserted the same data while we were writing.
				let _ = fs::remove_file(&meta_path);
				let _ = fs::remove_file(&blob_path);
				user_counter.fetch_sub(accounted, Ordering::Relaxed);
				self.current_size.fetch_sub(accounted, Ordering::Relaxed);
				return Err(HopError::DuplicateEntry);
			}
			index.insert(hash, meta);
		}

		tracing::info!(
			target: "hop",
			hash = ?hex::encode(hash),
			size = data_len,
			accounted,
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
	///
	/// `context` is the operation-specific domain separator (claim vs. ack) so that
	/// a signature produced for one operation cannot be replayed in another.
	/// The scan is bounded by `MAX_RECIPIENTS` (enforced at insert time).
	fn find_recipient(
		meta: &HopEntryMeta,
		hash: &HopHash,
		signature: &[u8],
		context: &[u8],
	) -> Result<usize, HopError> {
		let multi_sig =
			MultiSignature::decode(&mut &signature[..]).map_err(|_| HopError::InvalidSignature)?;
		let payload = signing_payload(context, hash);

		meta.recipients
			.iter()
			.enumerate()
			.find_map(|(i, signer)| {
				let account_id = signer.clone().into_account();
				if multi_sig.verify(&payload[..], &account_id) {
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
		let recipient_index = Self::find_recipient(meta, hash, signature, HOP_CLAIM_CONTEXT)?;

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
				HopError::IoError(e)
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
			let idx = Self::find_recipient(meta, hash, signature, HOP_ACK_CONTEXT)?;

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
			let accounted = entry_accounted_size(meta.size, meta.recipients.len());
			let sender = meta.sender_id;
			index.remove(hash);
			self.current_size.fetch_sub(accounted, Ordering::Relaxed);
			self.release_user_quota(&sender, accounted);
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
			let accounted = entry_accounted_size(meta.size, meta.recipients.len());
			self.current_size.fetch_sub(accounted, Ordering::Relaxed);
			self.release_user_quota(&meta.sender_id, accounted);

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
		let freed: u64 = expired
			.iter()
			.map(|(_, meta)| entry_accounted_size(meta.size, meta.recipients.len()))
			.sum();
		self.current_size.fetch_sub(freed, Ordering::Relaxed);

		{
			let usage = self.user_usage.read();
			for (_, meta) in &expired {
				if let Some(c) = usage.get(&meta.sender_id) {
					let accounted = entry_accounted_size(meta.size, meta.recipients.len());
					let prev = c.load(Ordering::Relaxed);
					c.fetch_sub(accounted.min(prev), Ordering::Relaxed);
				}
			}
		}

		// Phase 3: Delete files from disk (best-effort, no locks held).
		for (hash, _) in &expired {
			let _ = fs::remove_file(self.blob_path(hash));
			let _ = fs::remove_file(self.meta_path(hash));
		}

		// Let the rate limiter shed stale per-sender state on the same cadence.
		self.rate_limiter.evict_stale();

		freed
	}

	/// Return hashes of entries within `buffer_blocks` of expiry that have not yet been promoted.
	/// Returns up to `limit` hashes. Use [`Self::get`] to read blob data when needed.
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
	use crate::types::MAX_RECIPIENTS;
	use sp_core::{crypto::Pair, ed25519, sr25519};
	use sp_runtime::MultiSigner;
	use tempfile::TempDir;

	const SENDER_A: SenderId = [1u8; 32];
	const SENDER_B: SenderId = [2u8; 32];

	/// Accounted cost of an entry with `data_size` bytes and `num_recipients` recipients.
	fn acct(data_size: u64, num_recipients: usize) -> u64 {
		entry_accounted_size(data_size, num_recipients)
	}

	fn make_pool(max_size: u64, retention_blocks: u32) -> (HopDataPool, TempDir) {
		let dir = TempDir::new().unwrap();
		let pool = HopDataPool::new(
			max_size,
			max_size,
			retention_blocks,
			dir.path().to_path_buf(),
			RateLimitConfig::disabled(),
		)
		.unwrap();
		(pool, dir)
	}

	fn make_pool_with_user_cap(
		max_size: u64,
		max_user_size: u64,
		retention_blocks: u32,
	) -> (HopDataPool, TempDir) {
		let dir = TempDir::new().unwrap();
		let pool = HopDataPool::new(
			max_size,
			max_user_size,
			retention_blocks,
			dir.path().to_path_buf(),
			RateLimitConfig::disabled(),
		)
		.unwrap();
		(pool, dir)
	}

	fn create_test_pool() -> (HopDataPool, TempDir) {
		make_pool(1024 * 1024, 100)
	}

	fn test_recipient() -> (ed25519::Pair, MultiSigner) {
		let pair = ed25519::Pair::from_seed(&[1u8; 32]);
		let signer = MultiSigner::Ed25519(pair.public());
		(pair, signer)
	}

	fn sign_ed(pair: &ed25519::Pair, context: &[u8], hash: &HopHash) -> Vec<u8> {
		let payload = signing_payload(context, hash);
		MultiSignature::Ed25519(pair.sign(&payload)).encode()
	}

	fn sign_sr(pair: &sr25519::Pair, context: &[u8], hash: &HopHash) -> Vec<u8> {
		let payload = signing_payload(context, hash);
		MultiSignature::Sr25519(pair.sign(&payload)).encode()
	}

	fn user_usage(pool: &HopDataPool, sender: &SenderId) -> u64 {
		pool.user_usage
			.read()
			.get(sender)
			.map(|c| c.load(Ordering::Relaxed))
			.unwrap_or(0)
	}

	/// Convert a `Vec<MultiSigner>` into a `RecipientVec` for test ergonomics; panics
	/// only if a test exceeds `MAX_RECIPIENTS` (in which case the test is wrong).
	fn bv(v: Vec<MultiSigner>) -> RecipientVec {
		RecipientVec::try_from(v).expect("test recipient list exceeds MAX_RECIPIENTS")
	}

	#[test]
	fn test_insert_and_get() {
		let (pool, _dir) = create_test_pool();
		let (_, signer) = test_recipient();
		let data = vec![1, 2, 3, 4, 5];
		let hash = pool.insert(data.clone(), 0, bv(vec![signer]), SENDER_A).unwrap();

		let retrieved = pool.get(&hash).unwrap();
		assert_eq!(data, retrieved);
	}

	#[test]
	fn test_insert_no_recipients() {
		let (pool, _dir) = create_test_pool();
		let data = vec![1, 2, 3, 4, 5];
		let result = pool.insert(data, 0, bv(vec![]), SENDER_A);
		assert!(matches!(result, Err(HopError::NoRecipients)));
	}

	#[test]
	fn test_duplicate_insert() {
		let (pool, _dir) = create_test_pool();
		let (_, signer) = test_recipient();
		let data = vec![1, 2, 3, 4, 5];

		pool.insert(data.clone(), 0, bv(vec![signer.clone()]), SENDER_A).unwrap();
		let result = pool.insert(data, 0, bv(vec![signer]), SENDER_A);

		assert!(matches!(result, Err(HopError::DuplicateEntry)));
	}

	#[test]
	fn test_data_too_large() {
		let (pool, _dir) = create_test_pool();
		let (_, signer) = test_recipient();
		let data = vec![0u8; (MAX_DATA_SIZE + 1) as usize];

		let result = pool.insert(data, 0, bv(vec![signer]), SENDER_A);
		assert!(matches!(result, Err(HopError::DataTooLarge(_, _))));
	}

	#[test]
	fn test_too_many_recipients_rejected_at_type_level() {
		// Construction of a `RecipientVec` with more than `MAX_RECIPIENTS` entries
		// fails at `try_from`; callers (like the RPC) turn that into a
		// `TooManyRecipients` error before reaching the pool.
		let recipients: Vec<MultiSigner> = (0..=MAX_RECIPIENTS as u64)
			.map(|i| {
				let mut seed = [0u8; 32];
				seed[..8].copy_from_slice(&i.to_le_bytes());
				MultiSigner::Ed25519(ed25519::Pair::from_seed(&seed).public())
			})
			.collect();
		assert_eq!(recipients.len(), MAX_RECIPIENTS as usize + 1);
		assert!(RecipientVec::try_from(recipients).is_err());
	}

	#[test]
	fn test_duplicate_recipient_rejected() {
		let (pool, _dir) = create_test_pool();
		let (_, signer) = test_recipient();
		let result = pool.insert(vec![1, 2, 3], 0, bv(vec![signer.clone(), signer]), SENDER_A);
		assert!(matches!(result, Err(HopError::DuplicateRecipient)));
	}

	#[test]
	fn test_pool_full() {
		// Capacity exactly holds one 60-byte entry with one recipient (60 + 40 = 100).
		let (pool, _dir) = make_pool(acct(60, 1), 100);
		let (_, signer) = test_recipient();

		let data1 = vec![0u8; 60];
		let data2 = vec![1u8; 50];

		pool.insert(data1, 0, bv(vec![signer.clone()]), SENDER_A).unwrap();
		let result = pool.insert(data2, 0, bv(vec![signer]), SENDER_A);

		assert!(matches!(result, Err(HopError::PoolFull(_, _))));
	}

	#[test]
	fn test_remove() {
		let (pool, _dir) = create_test_pool();
		let (_, signer) = test_recipient();
		let data = vec![1, 2, 3, 4, 5];
		let hash = pool.insert(data, 0, bv(vec![signer]), SENDER_A).unwrap();

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

		pool.insert(data1.clone(), 0, bv(vec![signer.clone()]), SENDER_A).unwrap();
		pool.insert(data2.clone(), 0, bv(vec![signer]), SENDER_A).unwrap();

		let status = pool.status();
		assert_eq!(status.entry_count, 2);
		assert_eq!(status.total_bytes, acct(data1.len() as u64, 1) + acct(data2.len() as u64, 1));
	}

	#[test]
	fn test_claim_valid_signature() {
		let (pool, _dir) = create_test_pool();
		let (pair, signer) = test_recipient();
		let data = vec![1, 2, 3, 4, 5];
		let hash = pool.insert(data.clone(), 0, bv(vec![signer]), SENDER_A).unwrap();

		let claim = sign_ed(&pair, HOP_CLAIM_CONTEXT, &hash);
		let ack = sign_ed(&pair, HOP_ACK_CONTEXT, &hash);
		let result = pool.claim(&hash, &claim).unwrap();
		assert_eq!(data, result);

		// Entry still exists until ack.
		assert!(pool.has(&hash));

		pool.ack(&hash, &ack).unwrap();
		assert!(!pool.has(&hash));
	}

	#[test]
	fn test_claim_sig_rejected_on_ack() {
		// Domain separation: a claim signature cannot be replayed as an ack.
		let (pool, _dir) = create_test_pool();
		let (pair, signer) = test_recipient();
		let hash = pool.insert(vec![1, 2, 3], 0, bv(vec![signer]), SENDER_A).unwrap();

		let claim = sign_ed(&pair, HOP_CLAIM_CONTEXT, &hash);
		pool.claim(&hash, &claim).unwrap();
		assert!(matches!(pool.ack(&hash, &claim), Err(HopError::NotRecipient)));
	}

	#[test]
	fn test_claim_invalid_signature() {
		let (pool, _dir) = create_test_pool();
		let (_, signer) = test_recipient();
		let data = vec![1, 2, 3, 4, 5];
		let hash = pool.insert(data, 0, bv(vec![signer]), SENDER_A).unwrap();

		// Use invalid SCALE bytes — cannot decode as MultiSignature
		let result = pool.claim(&hash, &[0u8; 3]);
		assert!(matches!(result, Err(HopError::InvalidSignature)));
	}

	#[test]
	fn test_claim_wrong_key() {
		let (pool, _dir) = create_test_pool();
		let (_, signer) = test_recipient();
		let hash = pool.insert(vec![1, 2, 3, 4, 5], 0, bv(vec![signer]), SENDER_A).unwrap();

		let wrong_pair = ed25519::Pair::from_seed(&[99u8; 32]);
		let wrong_claim = sign_ed(&wrong_pair, HOP_CLAIM_CONTEXT, &hash);
		assert!(matches!(pool.claim(&hash, &wrong_claim), Err(HopError::NotRecipient)));
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
		let hash = pool.insert(data.clone(), 0, bv(vec![signer1, signer2]), SENDER_A).unwrap();

		let claim1 = sign_ed(&pair1, HOP_CLAIM_CONTEXT, &hash);
		let ack1 = sign_ed(&pair1, HOP_ACK_CONTEXT, &hash);
		assert_eq!(data, pool.claim(&hash, &claim1).unwrap());
		pool.ack(&hash, &ack1).unwrap();
		assert!(pool.has(&hash));

		let claim2 = sign_ed(&pair2, HOP_CLAIM_CONTEXT, &hash);
		let ack2 = sign_ed(&pair2, HOP_ACK_CONTEXT, &hash);
		assert_eq!(data, pool.claim(&hash, &claim2).unwrap());
		pool.ack(&hash, &ack2).unwrap();
		assert!(!pool.has(&hash));
		assert_eq!(pool.status().total_bytes, 0);
	}

	#[test]
	fn test_claim_after_ack_returns_already_claimed() {
		let (pool, _dir) = create_test_pool();
		let (pair, signer) = test_recipient();
		let pair2 = ed25519::Pair::from_seed(&[2u8; 32]);
		let signer2 = MultiSigner::Ed25519(pair2.public());

		let hash = pool
			.insert(vec![1, 2, 3, 4, 5], 0, bv(vec![signer, signer2]), SENDER_A)
			.unwrap();

		let claim = sign_ed(&pair, HOP_CLAIM_CONTEXT, &hash);
		let ack = sign_ed(&pair, HOP_ACK_CONTEXT, &hash);
		pool.claim(&hash, &claim).unwrap();
		pool.ack(&hash, &ack).unwrap();

		// Same recipient claims again — already acked.
		assert!(matches!(pool.claim(&hash, &claim), Err(HopError::AlreadyClaimed)));
	}

	#[test]
	fn test_claim_not_found() {
		let (pool, _dir) = create_test_pool();
		let fake_hash = H256([0u8; 32]);
		let result = pool.claim(&fake_hash, &[0u8; 64]);
		assert!(matches!(result, Err(HopError::NotFound)));
	}

	#[test]
	fn test_per_user_cap_is_hard_limit() {
		// Pool big enough for multiple users; user cap sized to one 60-byte entry (+ metadata).
		let (pool, _dir) = make_pool_with_user_cap(10_000, acct(60, 1), 100);
		let (_, signer) = test_recipient();

		pool.insert(vec![0u8; 60], 0, bv(vec![signer.clone()]), SENDER_A).unwrap();

		// User A is at the cap; next insert is rejected regardless of pool headroom.
		let result = pool.insert(vec![1u8; 10], 0, bv(vec![signer.clone()]), SENDER_A);
		assert!(matches!(result, Err(HopError::UserQuotaExceeded { .. })));

		// User B has their own independent cap.
		pool.insert(vec![2u8; 60], 0, bv(vec![signer]), SENDER_B).unwrap();
	}

	#[test]
	fn test_quota_released_after_ack() {
		let (pool, _dir) = make_pool_with_user_cap(10_000, acct(100, 1), 100);
		let (pair, signer) = test_recipient();

		let hash = pool.insert(vec![0u8; 100], 0, bv(vec![signer.clone()]), SENDER_A).unwrap();

		// At cap; next insert rejected.
		let result = pool.insert(vec![1u8; 10], 0, bv(vec![signer.clone()]), SENDER_A);
		assert!(matches!(result, Err(HopError::UserQuotaExceeded { .. })));

		let claim = sign_ed(&pair, HOP_CLAIM_CONTEXT, &hash);
		let ack = sign_ed(&pair, HOP_ACK_CONTEXT, &hash);
		pool.claim(&hash, &claim).unwrap();
		pool.ack(&hash, &ack).unwrap();

		// Quota freed — user can insert again.
		pool.insert(vec![2u8; 100], 0, bv(vec![signer]), SENDER_A).unwrap();
	}

	#[test]
	fn test_cleanup_expired_releases_quota() {
		let (pool, _dir) = make_pool(10_000, 10);
		let (_, signer) = test_recipient();

		pool.insert(vec![0u8; 100], 0, bv(vec![signer]), SENDER_A).unwrap();
		let charged = acct(100, 1);
		assert_eq!(user_usage(&pool, &SENDER_A), charged);

		let freed = pool.cleanup_expired(10);
		assert_eq!(freed, charged);
		assert_eq!(pool.status().total_bytes, 0);
		assert_eq!(user_usage(&pool, &SENDER_A), 0);
	}

	#[test]
	fn test_user_counter_preserved_when_zero() {
		// TOCTOU fix: counters are not removed from the map to avoid racing with
		// in-flight inserts. The value must drop to zero but the entry may remain.
		let (pool, _dir) = create_test_pool();
		let (pair, signer) = test_recipient();

		let hash = pool.insert(vec![0u8; 50], 0, bv(vec![signer]), SENDER_A).unwrap();
		assert!(pool.user_usage.read().contains_key(&SENDER_A));

		let claim = sign_ed(&pair, HOP_CLAIM_CONTEXT, &hash);
		let ack = sign_ed(&pair, HOP_ACK_CONTEXT, &hash);
		pool.claim(&hash, &claim).unwrap();
		pool.ack(&hash, &ack).unwrap();

		assert_eq!(user_usage(&pool, &SENDER_A), 0);
	}

	#[test]
	fn test_restart_recovery() {
		let dir = TempDir::new().unwrap();
		let (_, signer) = test_recipient();
		let expected_accounted = acct(100, 1);

		let hash;
		{
			let pool = HopDataPool::new(
				1024 * 1024,
				1024 * 1024,
				100,
				dir.path().to_path_buf(),
				RateLimitConfig::disabled(),
			)
			.unwrap();
			hash = pool.insert(vec![42u8; 100], 0, bv(vec![signer]), SENDER_A).unwrap();
			assert!(pool.has(&hash));
			assert_eq!(pool.status().entry_count, 1);
			assert_eq!(pool.status().total_bytes, expected_accounted);
		}

		{
			let pool = HopDataPool::new(
				1024 * 1024,
				1024 * 1024,
				100,
				dir.path().to_path_buf(),
				RateLimitConfig::disabled(),
			)
			.unwrap();
			assert!(pool.has(&hash));
			assert_eq!(pool.status().entry_count, 1);
			assert_eq!(pool.status().total_bytes, expected_accounted);

			let data = pool.get(&hash).unwrap();
			assert_eq!(data, vec![42u8; 100]);
			assert_eq!(user_usage(&pool, &SENDER_A), expected_accounted);
		}
	}

	#[test]
	fn test_orphan_blob_cleanup() {
		let dir = TempDir::new().unwrap();
		{
			let _pool = HopDataPool::new(
				1024 * 1024,
				1024 * 1024,
				100,
				dir.path().to_path_buf(),
				RateLimitConfig::disabled(),
			)
			.unwrap();
		}

		let orphan_hash = "aa".to_string() + &"bb".repeat(15);
		let blob_path = dir.path().join("blobs").join("aa").join(format!("{}.blob", orphan_hash));
		fs::write(&blob_path, b"orphan data").unwrap();
		assert!(blob_path.exists());

		let _pool = HopDataPool::new(
			1024 * 1024,
			1024 * 1024,
			100,
			dir.path().to_path_buf(),
			RateLimitConfig::disabled(),
		)
		.unwrap();
		assert!(!blob_path.exists());
	}

	#[test]
	fn test_corrupt_meta_cleanup() {
		let dir = TempDir::new().unwrap();
		{
			let _pool = HopDataPool::new(
				1024 * 1024,
				1024 * 1024,
				100,
				dir.path().to_path_buf(),
				RateLimitConfig::disabled(),
			)
			.unwrap();
		}

		let fake_hash = "bb".to_string() + &"cc".repeat(15);
		let meta_path = dir.path().join("meta").join("bb").join(format!("{}.meta", fake_hash));
		fs::write(&meta_path, b"not valid SCALE data").unwrap();
		assert!(meta_path.exists());

		let pool = HopDataPool::new(
			1024 * 1024,
			1024 * 1024,
			100,
			dir.path().to_path_buf(),
			RateLimitConfig::disabled(),
		)
		.unwrap();
		assert!(!meta_path.exists());
		assert_eq!(pool.status().entry_count, 0);
	}

	#[test]
	fn test_claim_sr25519() {
		let (pool, _dir) = create_test_pool();
		let pair = sr25519::Pair::from_seed(&[3u8; 32]);
		let signer = MultiSigner::Sr25519(pair.public());

		let data = vec![10, 20, 30];
		let hash = pool.insert(data.clone(), 0, bv(vec![signer]), SENDER_A).unwrap();

		let claim = sign_sr(&pair, HOP_CLAIM_CONTEXT, &hash);
		let ack = sign_sr(&pair, HOP_ACK_CONTEXT, &hash);
		assert_eq!(data, pool.claim(&hash, &claim).unwrap());
		pool.ack(&hash, &ack).unwrap();
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
		let hash = pool.insert(data.clone(), 0, bv(vec![ed_signer, sr_signer]), SENDER_A).unwrap();

		let sr_claim = sign_sr(&sr_pair, HOP_CLAIM_CONTEXT, &hash);
		let sr_ack = sign_sr(&sr_pair, HOP_ACK_CONTEXT, &hash);
		assert_eq!(data, pool.claim(&hash, &sr_claim).unwrap());
		pool.ack(&hash, &sr_ack).unwrap();
		assert!(pool.has(&hash));

		let ed_claim = sign_ed(&ed_pair, HOP_CLAIM_CONTEXT, &hash);
		let ed_ack = sign_ed(&ed_pair, HOP_ACK_CONTEXT, &hash);
		assert_eq!(data, pool.claim(&hash, &ed_claim).unwrap());
		pool.ack(&hash, &ed_ack).unwrap();
		assert!(!pool.has(&hash));
	}

	#[test]
	fn test_claim_is_repeatable() {
		let (pool, _dir) = create_test_pool();
		let (pair, signer) = test_recipient();
		let data = vec![1, 2, 3, 4, 5];
		let hash = pool.insert(data.clone(), 0, bv(vec![signer]), SENDER_A).unwrap();

		let claim = sign_ed(&pair, HOP_CLAIM_CONTEXT, &hash);
		assert_eq!(data, pool.claim(&hash, &claim).unwrap());
		assert_eq!(data, pool.claim(&hash, &claim).unwrap());
		assert!(pool.has(&hash));
	}

	#[test]
	fn test_ack_idempotent() {
		let (pool, _dir) = create_test_pool();
		let (pair, signer) = test_recipient();
		let pair2 = ed25519::Pair::from_seed(&[2u8; 32]);
		let signer2 = MultiSigner::Ed25519(pair2.public());

		let hash = pool
			.insert(vec![1, 2, 3, 4, 5], 0, bv(vec![signer, signer2]), SENDER_A)
			.unwrap();
		let ack = sign_ed(&pair, HOP_ACK_CONTEXT, &hash);

		pool.ack(&hash, &ack).unwrap();
		pool.ack(&hash, &ack).unwrap();
		assert!(pool.has(&hash));
	}

	#[test]
	fn test_multi_recipient_partial_ack() {
		let (pool, _dir) = create_test_pool();
		let pair1 = ed25519::Pair::from_seed(&[1u8; 32]);
		let pair2 = ed25519::Pair::from_seed(&[2u8; 32]);
		let signer1 = MultiSigner::Ed25519(pair1.public());
		let signer2 = MultiSigner::Ed25519(pair2.public());

		let data = vec![1, 2, 3, 4, 5];
		let hash = pool.insert(data.clone(), 0, bv(vec![signer1, signer2]), SENDER_A).unwrap();

		let claim1 = sign_ed(&pair1, HOP_CLAIM_CONTEXT, &hash);
		let ack1 = sign_ed(&pair1, HOP_ACK_CONTEXT, &hash);
		let claim2 = sign_ed(&pair2, HOP_CLAIM_CONTEXT, &hash);
		let ack2 = sign_ed(&pair2, HOP_ACK_CONTEXT, &hash);

		assert_eq!(data, pool.claim(&hash, &claim1).unwrap());
		pool.ack(&hash, &ack1).unwrap();
		assert!(pool.has(&hash));

		assert_eq!(data, pool.claim(&hash, &claim2).unwrap());
		pool.ack(&hash, &ack2).unwrap();
		assert!(!pool.has(&hash));
		assert_eq!(pool.status().total_bytes, 0);
	}

	#[test]
	fn test_concurrent_inserts_respect_capacity() {
		use std::{sync::Barrier, thread};

		let (_, signer) = test_recipient();
		// Capacity for exactly 4 entries of 50 bytes (accounted = 90 each).
		let (pool, _dir) = make_pool(acct(50, 1) * 4, 100);
		let pool = Arc::new(pool);
		let barrier = Arc::new(Barrier::new(10));

		let handles: Vec<_> = (0..10u8)
			.map(|i| {
				let pool = pool.clone();
				let signer = signer.clone();
				let barrier = barrier.clone();
				thread::spawn(move || {
					barrier.wait();
					pool.insert(vec![i; 50], 0, bv(vec![signer]), SENDER_A)
				})
			})
			.collect();

		let results: Vec<_> = handles.into_iter().map(|h| h.join().unwrap()).collect();
		let successes = results.iter().filter(|r| r.is_ok()).count();

		assert!(successes <= 4, "Got {} successes, max should be 4", successes);
		assert!(pool.status().total_bytes <= acct(50, 1) * 4);
	}

	#[test]
	fn test_concurrent_inserts_respect_user_quota() {
		use std::{sync::Barrier, thread};

		let (_, signer) = test_recipient();
		// Per-user cap holds 3 entries of 100 bytes. Pool has plenty of room so the
		// *user* cap is what actually constrains the test.
		let per_entry = acct(100, 1);
		let (pool, _dir) = make_pool_with_user_cap(per_entry * 20, per_entry * 3, 100);
		let pool = Arc::new(pool);
		let barrier = Arc::new(Barrier::new(10));

		let handles: Vec<_> = (0..10u8)
			.map(|i| {
				let pool = pool.clone();
				let signer = signer.clone();
				let barrier = barrier.clone();
				thread::spawn(move || {
					barrier.wait();
					pool.insert(vec![i; 100], 0, bv(vec![signer]), SENDER_A)
				})
			})
			.collect();

		let results: Vec<_> = handles.into_iter().map(|h| h.join().unwrap()).collect();
		let successes = results.iter().filter(|r| r.is_ok()).count();

		// Hard per-user cap: at most 3 inserts may succeed regardless of concurrency.
		assert!(successes <= 3, "hard per-user cap violated: {} successes", successes);
		assert!(user_usage(&pool, &SENDER_A) <= per_entry * 3);
	}

	#[test]
	fn test_concurrent_claim_and_ack() {
		use std::{sync::Barrier, thread};

		let (pool, _dir) = create_test_pool();
		let pool = Arc::new(pool);

		let pairs: Vec<_> = (1..=5u8)
			.map(|i| {
				let pair = ed25519::Pair::from_seed(&[i; 32]);
				let signer = MultiSigner::Ed25519(pair.public());
				(pair, signer)
			})
			.collect();

		let signers: Vec<_> = pairs.iter().map(|(_, s)| s.clone()).collect();
		let data = vec![42u8; 100];
		let hash = pool.insert(data.clone(), 0, bv(signers), SENDER_A).unwrap();

		let barrier = Arc::new(Barrier::new(5));

		let handles: Vec<_> = pairs
			.into_iter()
			.map(|(pair, _)| {
				let pool = pool.clone();
				let barrier = barrier.clone();
				let data = data.clone();
				thread::spawn(move || {
					barrier.wait();
					let claim = sign_ed(&pair, HOP_CLAIM_CONTEXT, &hash);
					let ack = sign_ed(&pair, HOP_ACK_CONTEXT, &hash);

					let claimed = pool.claim(&hash, &claim).unwrap();
					assert_eq!(data, claimed);
					pool.ack(&hash, &ack).unwrap();
				})
			})
			.collect();

		for h in handles {
			h.join().unwrap();
		}

		assert!(!pool.has(&hash));
		assert_eq!(pool.status().total_bytes, 0);
	}

	#[test]
	fn test_get_promotable_within_buffer() {
		let (pool, _dir) = make_pool(1024 * 1024, 100);
		let (_, signer) = test_recipient();

		let hash = pool.insert(vec![1, 2, 3], 0, bv(vec![signer]), SENDER_A).unwrap();

		let promotable = pool.get_promotable(50, 30, usize::MAX);
		assert!(promotable.is_empty());

		let promotable = pool.get_promotable(80, 30, usize::MAX);
		assert_eq!(promotable.len(), 1);
		assert_eq!(promotable[0], hash);
	}

	#[test]
	fn test_get_promotable_excludes_promoted() {
		let (pool, _dir) = make_pool(1024 * 1024, 100);
		let (_, signer) = test_recipient();

		let hash = pool.insert(vec![1, 2, 3], 0, bv(vec![signer]), SENDER_A).unwrap();

		let promotable = pool.get_promotable(80, 30, usize::MAX);
		assert_eq!(promotable.len(), 1);

		pool.mark_promoted(&hash);

		let promotable = pool.get_promotable(80, 30, usize::MAX);
		assert!(promotable.is_empty());
	}

	#[test]
	fn test_mark_promoted_persists_across_restart() {
		let dir = TempDir::new().unwrap();
		let (_, signer) = test_recipient();

		let hash;
		{
			let pool = HopDataPool::new(
				1024 * 1024,
				1024 * 1024,
				100,
				dir.path().to_path_buf(),
				RateLimitConfig::disabled(),
			)
			.unwrap();
			hash = pool.insert(vec![42u8; 10], 0, bv(vec![signer]), SENDER_A).unwrap();
			pool.mark_promoted(&hash);
		}

		{
			let pool = HopDataPool::new(
				1024 * 1024,
				1024 * 1024,
				100,
				dir.path().to_path_buf(),
				RateLimitConfig::disabled(),
			)
			.unwrap();
			let promotable = pool.get_promotable(80, 30, usize::MAX);
			assert!(promotable.is_empty(), "promoted entry should not be promotable after restart");
			assert!(pool.has(&hash), "entry should still exist");
		}
	}

	#[test]
	fn test_cleanup_expired_removes_promoted() {
		let (pool, _dir) = make_pool(1024 * 1024, 10);
		let (_, signer) = test_recipient();

		let hash = pool.insert(vec![1, 2, 3], 0, bv(vec![signer]), SENDER_A).unwrap();
		pool.mark_promoted(&hash);
		assert!(pool.has(&hash));

		let freed = pool.cleanup_expired(10);
		assert!(freed > 0);
		assert!(!pool.has(&hash));
	}

	#[test]
	fn test_rate_limit_rejects_burst_overflow() {
		let dir = TempDir::new().unwrap();
		let cfg = RateLimitConfig {
			enabled: true,
			submit_rate_per_min: 60,
			submit_burst: 2,
			bandwidth_per_min: 1_000_000,
			bandwidth_burst: 1_000_000,
		};
		let pool =
			HopDataPool::new(1024 * 1024, 1024 * 1024, 100, dir.path().to_path_buf(), cfg).unwrap();
		let (_, signer) = test_recipient();

		pool.insert(vec![1, 2, 3], 0, bv(vec![signer.clone()]), SENDER_A).unwrap();
		pool.insert(vec![4, 5, 6], 0, bv(vec![signer.clone()]), SENDER_A).unwrap();
		assert!(matches!(
			pool.insert(vec![7, 8, 9], 0, bv(vec![signer]), SENDER_A),
			Err(HopError::RateLimited { .. })
		));
	}
}
