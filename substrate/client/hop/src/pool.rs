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

//! HOP data pool: parity-db metadata at `<data_dir>/meta-db/` plus
//! content-addressed blob files at `<data_dir>/blobs/{shard}/{hash}.blob`.
//! In-memory counters are caches rebuilt at startup. RMW ops serialise on
//! `rmw_lock` since parity-db has no CAS.
//!
//! The database carries a schema version row in `COL_DB_META`, checked on
//! every open so a database written by a newer binary is rejected rather than
//! misread. Startup also imports any leftover `<data_dir>/meta/` metadata files
//! from the pre-KV-store layout and removes that tree; see
//! `HopDataPool::import_legacy_meta_files`.

use crate::{
	metrics::{removal_reasons, HopMetrics},
	rate_limit::{RateLimitConfig, RateLimiter},
	types::{
		entry_accounted_size, promotion_backoff_blocks, signing_payload, HopBlockNumber,
		HopEntryMeta, HopError, HopHash, PoolStatus, RecipientVec, SenderId, HOP_ACK_CONTEXT,
		HOP_CLAIM_CONTEXT, HOP_META_VERSION, MAX_PROMOTION_ATTEMPTS,
	},
};
use codec::{Decode, Encode};
use parking_lot::{Mutex, RwLock};
use sp_core::H256;
use sp_crypto_hashing::blake2_256;
use sp_runtime::{
	traits::{IdentifyAccount, Verify},
	MultiSignature, MultiSigner,
};
use std::{
	collections::{BTreeSet, HashMap},
	fs,
	ops::Bound,
	path::{Path, PathBuf},
	process,
	sync::{
		atomic::{AtomicU64, Ordering},
		Arc,
	},
	time::{SystemTime, UNIX_EPOCH},
};

/// Disambiguates concurrent atomic writes to the same final blob path so two
/// threads with the same content hash don't share a `<path>.tmp` file.
static TMP_SEQ: AtomicU64 = AtomicU64::new(0);

const BLOBS_DIR: &str = "blobs";
const BLOB_EXT: &str = "blob";
/// Subdirectory of `data_dir` housing the parity-db instance.
const META_DB_DIR: &str = "meta-db";
/// Number of shards used for the `blobs/` directory (one per first-byte value
/// of the content hash: `00`–`ff`). Metadata is no longer sharded since it
/// lives in parity-db.
const SHARD_COUNT: u16 = 256;

/// Pre-KV-store metadata layout: `<data_dir>/meta/{shard}/{hash}.meta`, one
/// SCALE-encoded `HopEntryMeta` per file. Imported into [`COL_META`] and
/// removed on the first startup after the upgrade.
const LEGACY_META_DIR: &str = "meta";
const LEGACY_META_EXT: &str = "meta";

/// parity-db column holding `HopHash` → SCALE-encoded `HopEntryMeta`.
const COL_META: u8 = 0;
/// parity-db column holding pool-wide metadata. Kept separate from
/// [`COL_META`] so the startup scan can iterate entry rows without having to
/// recognise and skip non-entry keys.
const COL_DB_META: u8 = 1;
/// Total number of parity-db columns this pool uses.
const COL_COUNT: u8 = 2;

/// Key of the schema version row in [`COL_DB_META`].
const KEY_DB_VERSION: &[u8] = b"version";
/// On-disk schema version this binary writes and understands.
const CURRENT_DB_VERSION: u32 = 1;
/// Upper bound on rows buffered per `commit` while importing legacy metadata,
/// so a large pool doesn't build one unbounded transaction.
const MIGRATION_CHUNK: usize = 10_000;

/// HOP data pool: parity-db metadata + content-addressed blob files.
pub struct HopDataPool {
	/// Metadata KV store; source of truth for entry existence and state.
	db: Arc<parity_db::Db>,
	/// Serialises get-then-conditional-write pairs (parity-db has no CAS).
	rmw_lock: Mutex<()>,
	/// Per-user byte usage cache, rebuilt at startup.
	user_usage: RwLock<HashMap<SenderId, AtomicU64>>,
	/// Expiry-ordered index of live entries, rebuilt at startup. Lets
	/// `cleanup_expired` and `get_promotable` run as bounded range scans
	/// instead of iterating the entire meta column each tick. Stale or
	/// missing entries are tolerated: maintenance re-reads each candidate
	/// under `rmw_lock` before acting on it.
	expiry_index: RwLock<BTreeSet<(u64, HopHash)>>,
	/// Maximum pool size in bytes (data + per-entry metadata overhead).
	max_size: u64,
	/// Fixed hard per-user quota in bytes.
	max_user_size: u64,
	/// Current accounted pool size in bytes.
	current_size: AtomicU64,
	/// Cached entry count for `status()`.
	entry_count: AtomicU64,
	/// Data retention period in seconds.
	retention_secs: u64,
	/// Root data directory.
	data_dir: PathBuf,
	/// Per-account submit rate limiter.
	rate_limiter: Arc<RateLimiter>,
	/// Prometheus metrics (no-ops without a registry).
	metrics: HopMetrics,
}

impl HopDataPool {
	/// Open or create the metadata DB, rebuild counter caches by iterating it,
	/// and remove orphan `.blob` files in the same pass.
	pub fn new(
		max_size: u64,
		max_user_size: u64,
		retention_secs: u64,
		data_dir: PathBuf,
		rate_limit_cfg: RateLimitConfig,
		metrics: HopMetrics,
	) -> Result<Self, HopError> {
		// Blob shard directories (256 of them, named 00..ff).
		for i in 0..SHARD_COUNT {
			let shard = format!("{:02x}", i as u8);
			fs::create_dir_all(data_dir.join(BLOBS_DIR).join(&shard))?;
		}

		let db_path = data_dir.join(META_DB_DIR);
		fs::create_dir_all(&db_path)?;
		// Column layout upgrades happen while the DB is closed, since parity-db
		// refuses to open a path whose stored column config differs from ours.
		Self::migrate_columns(&db_path)?;
		let db = parity_db::Db::open_or_create(&Self::db_options(&db_path))
			.map_err(|e| HopError::Db(e.to_string()))?;
		Self::check_db_version(&db)?;

		// Import pre-KV-store metadata before the scan below, so the imported
		// rows feed the counter caches and their blobs count as live.
		Self::import_legacy_meta_files(&db, &data_dir)?;

		// Rebuild counters + live-hash set, dropping any unsupported-version rows.
		let mut user_usage: HashMap<SenderId, AtomicU64> = HashMap::new();
		let mut current_size: u64 = 0;
		let mut entry_count: u64 = 0;
		let mut live_hashes: std::collections::HashSet<HopHash> = std::collections::HashSet::new();
		let mut expiry_index: BTreeSet<(u64, HopHash)> = BTreeSet::new();
		let mut stale_keys: Vec<Vec<u8>> = Vec::new();

		{
			let mut iter = db.iter(COL_META).map_err(|e| HopError::Db(e.to_string()))?;
			while let Some((key, value)) = iter.next().map_err(|e| HopError::Db(e.to_string()))? {
				let hash = match <[u8; 32]>::try_from(key.as_slice()) {
					Ok(arr) => H256(arr),
					Err(_) => {
						tracing::warn!(target: "hop", key_len = key.len(), "Dropping meta row with non-32-byte key");
						stale_keys.push(key);
						continue;
					},
				};
				match HopEntryMeta::decode(&mut value.as_slice()) {
					Ok(meta) if meta.version == HOP_META_VERSION => {
						// `insert` writes the blob before committing the meta row, so a
						// committed row must have a sibling blob. A missing blob means a
						// crash persisted the blob unlink but lost the async meta-delete;
						// drop the row here rather than let it hold pool + user quota
						// until expiry.
						if !Self::entry_path(&data_dir, &hash, BLOBS_DIR, BLOB_EXT).exists() {
							tracing::warn!(target: "hop", hash = ?hex::encode(hash), "Dropping meta row with missing blob");
							stale_keys.push(key);
							continue;
						}
						let accounted = entry_accounted_size(meta.size, meta.recipients.len());
						current_size = current_size.saturating_add(accounted);
						entry_count = entry_count.saturating_add(1);
						user_usage
							.entry(meta.sender_id)
							.or_default()
							.fetch_add(accounted, Ordering::Relaxed);
						live_hashes.insert(hash);
						expiry_index.insert((meta.expires_at, hash));
					},
					Ok(meta) => {
						tracing::warn!(
							target: "hop",
							version = meta.version,
							expected = HOP_META_VERSION,
							hash = ?hex::encode(hash),
							"Dropping meta row with unsupported version",
						);
						stale_keys.push(key);
					},
					Err(e) => {
						tracing::warn!(target: "hop", hash = ?hex::encode(hash), error = %e, "Dropping undecodable meta row");
						stale_keys.push(key);
					},
				}
			}
		}

		// Delete stale keys and the corresponding blobs in one batch.
		if !stale_keys.is_empty() {
			let ops: Vec<_> =
				stale_keys.iter().cloned().map(|k| (COL_META, k, None::<Vec<u8>>)).collect();
			db.commit(ops).map_err(|e| HopError::Db(e.to_string()))?;
			for k in &stale_keys {
				if let Ok(arr) = <[u8; 32]>::try_from(k.as_slice()) {
					let _ = fs::remove_file(Self::entry_path(
						&data_dir,
						&H256(arr),
						BLOBS_DIR,
						BLOB_EXT,
					));
				}
			}
		}

		// Reap orphan `.blob` and leftover `.tmp.*` files. Deliberately
		// unconditional: an empty meta column is a legitimate state for a pool
		// that has drained, so refusing to reap when it is empty would leak
		// genuine orphans forever. The case where the column is empty only
		// because metadata hasn't been read yet is prevented upstream, by
		// `import_legacy_meta_files` running before this scan.
		for i in 0..SHARD_COUNT {
			let shard = format!("{:02x}", i as u8);
			let blob_shard_dir = data_dir.join(BLOBS_DIR).join(&shard);
			let Ok(entries) = fs::read_dir(&blob_shard_dir) else { continue };
			for entry in entries.flatten() {
				let path = entry.path();
				if path.extension().and_then(|e| e.to_str()) != Some(BLOB_EXT) {
					if path
						.file_name()
						.and_then(|n| n.to_str())
						.map_or(false, |n| n.contains(".tmp."))
					{
						let _ = fs::remove_file(&path);
					}
					continue;
				}
				let Some(stem) = path.file_stem().and_then(|s| s.to_str()) else { continue };
				let is_orphan = match parse_hex_hash(stem) {
					Some(hash) => !live_hashes.contains(&hash),
					None => true,
				};
				if is_orphan {
					tracing::warn!(target: "hop", hash = ?stem, "Removing orphan .blob (no meta row)");
					let _ = fs::remove_file(&path);
				}
			}
		}

		tracing::info!(
			target: "hop",
			entries = entry_count,
			total_bytes = current_size,
			"Recovered HOP pool from disk"
		);

		// `stale_keys` are the meta rows dropped during recovery (bad key,
		// unsupported version, undecodable, or missing blob). Orphan `.blob`s
		// are not counted: without a meta row they were never a claimable entry.
		metrics.set_pool_status(entry_count, current_size, max_size);
		metrics.record_removed(removal_reasons::STARTUP_DROPPED, stale_keys.len() as u64);

		Ok(Self {
			db: Arc::new(db),
			rmw_lock: Mutex::new(()),
			user_usage: RwLock::new(user_usage),
			expiry_index: RwLock::new(expiry_index),
			max_size,
			max_user_size,
			current_size: AtomicU64::new(current_size),
			entry_count: AtomicU64::new(entry_count),
			retention_secs,
			data_dir,
			rate_limiter: Arc::new(RateLimiter::new(rate_limit_cfg)),
			metrics,
		})
	}

	/// Metrics shared by the pool, RPC server, and maintenance task.
	pub(crate) fn metrics(&self) -> &HopMetrics {
		&self.metrics
	}

	/// Snapshot the pool size gauges after a mutation. Reads the accounted-size
	/// and entry-count atomics, which are updated under `rmw_lock` at every
	/// mutation site, so a gauge publish issued from inside that critical
	/// section reflects the write that just landed.
	fn publish_size_metrics(&self) {
		self.metrics.set_pool_size(
			self.entry_count.load(Ordering::Relaxed),
			self.current_size.load(Ordering::Relaxed),
		);
	}

	/// Column layout this pool expects of its parity-db instance.
	///
	/// `btree_index` on [`COL_META`] is what lets startup recovery iterate
	/// `(key, value)` pairs; [`COL_DB_META`] stays at the default hash index
	/// since it is only ever point-queried.
	fn db_options(db_path: &Path) -> parity_db::Options {
		let mut options = parity_db::Options::with_columns(db_path, COL_COUNT);
		options.columns[COL_META as usize].btree_index = true;
		options
	}

	/// Append any columns an older on-disk layout is missing.
	///
	/// Must run before the DB is opened. No-op for a path with no database yet
	/// (`open_or_create` will lay down every column) and for one already at
	/// [`COL_COUNT`], so this is safe to call on every startup. Appended
	/// columns take their options from [`Self::db_options`], which keeps the
	/// migration and the open path from ever disagreeing.
	fn migrate_columns(db_path: &Path) -> Result<(), HopError> {
		let Some(metadata) =
			parity_db::Options::load_metadata(db_path).map_err(|e| HopError::Db(e.to_string()))?
		else {
			return Ok(());
		};
		if metadata.columns.len() >= COL_COUNT as usize {
			return Ok(());
		}
		let desired = Self::db_options(db_path).columns;
		let mut migrate_options = parity_db::Options::with_columns(db_path, 0);
		migrate_options.columns = metadata.columns;
		tracing::info!(
			target: "hop",
			from = migrate_options.columns.len(),
			to = COL_COUNT,
			"Extending HOP database column layout",
		);
		// `add_column` takes options by value and pushes onto
		// `migrate_options.columns`, so `skip` is evaluated once against the
		// pre-migration length.
		for column in desired.into_iter().skip(migrate_options.columns.len()) {
			parity_db::Db::add_column(&mut migrate_options, column)
				.map_err(|e| HopError::Db(e.to_string()))?;
		}
		Ok(())
	}

	/// Reconcile the on-disk schema version with [`CURRENT_DB_VERSION`].
	///
	/// A database with no version row is stamped with the current version; one
	/// written by a newer binary is rejected rather than silently misread.
	fn check_db_version(db: &parity_db::Db) -> Result<(), HopError> {
		match db.get(COL_DB_META, KEY_DB_VERSION).map_err(|e| HopError::Db(e.to_string()))? {
			Some(bytes) => {
				let version = u32::from_le_bytes(
					bytes
						.try_into()
						.map_err(|_| HopError::Db("Malformed HOP database version".into()))?,
				);
				if version > CURRENT_DB_VERSION {
					return Err(HopError::Db(format!(
						"Unsupported HOP database version {version}; this binary supports up to \
						 {CURRENT_DB_VERSION}"
					)));
				}
				// Only version 1 exists so far. Future forward migrations
				// dispatch here on `version < CURRENT_DB_VERSION` and stamp the
				// new version last, so a crash mid-migration re-runs it.
				Ok(())
			},
			None => db
				.commit([(
					COL_DB_META,
					KEY_DB_VERSION.to_vec(),
					Some(CURRENT_DB_VERSION.to_le_bytes().to_vec()),
				)])
				.map_err(|e| HopError::Db(e.to_string())),
		}
	}

	/// Import pre-KV-store `<data_dir>/meta/{shard}/{hash}.meta` files
	/// into [`COL_META`], then remove the tree.
	///
	/// Runs before startup recovery iterates the column, so imported rows are
	/// picked up by the normal scan: counters, per-user usage and the expiry
	/// index all come for free, and the orphan pass sees the imported hashes as
	/// live instead of unlinking every blob in the pool.
	///
	/// Gated on the directory existing rather than on the schema version, so an
	/// import interrupted by a crash is retried on the next boot. Idempotent: a
	/// row already in the column always wins over the legacy file. Per-file
	/// problems are logged and skipped, leaving the blob to the orphan pass —
	/// the same outcome the pre-migration recovery scan produced.
	fn import_legacy_meta_files(db: &parity_db::Db, data_dir: &Path) -> Result<(), HopError> {
		let legacy_dir = data_dir.join(LEGACY_META_DIR);
		if !legacy_dir.exists() {
			return Ok(());
		}

		let mut ops: Vec<(u8, Vec<u8>, Option<Vec<u8>>)> = Vec::new();
		let mut imported: u64 = 0;
		let mut skipped: u64 = 0;

		for i in 0..SHARD_COUNT {
			let shard = format!("{:02x}", i as u8);
			let Ok(entries) = fs::read_dir(legacy_dir.join(&shard)) else { continue };
			for entry in entries.flatten() {
				let path = entry.path();
				// Leftover `.tmp.*` files need no special handling: the whole
				// tree is removed once the import completes.
				if path.extension().and_then(|e| e.to_str()) != Some(LEGACY_META_EXT) {
					continue;
				}
				let Some(stem) = path.file_stem().and_then(|s| s.to_str()) else { continue };
				let Some(hash) = parse_hex_hash(stem) else {
					tracing::warn!(target: "hop", path = ?path, "Skipping legacy .meta with invalid name");
					skipped += 1;
					continue;
				};
				let bytes = match fs::read(&path) {
					Ok(b) => b,
					Err(e) => {
						tracing::warn!(target: "hop", path = ?path, error = %e, "Skipping unreadable legacy .meta");
						skipped += 1;
						continue;
					},
				};
				let meta = match HopEntryMeta::decode(&mut bytes.as_slice()) {
					Ok(meta) => meta,
					Err(e) => {
						tracing::warn!(target: "hop", path = ?path, error = %e, "Skipping corrupt legacy .meta");
						skipped += 1;
						continue;
					},
				};
				if meta.version != HOP_META_VERSION {
					tracing::warn!(
						target: "hop",
						path = ?path,
						version = meta.version,
						expected = HOP_META_VERSION,
						"Skipping legacy .meta with unsupported version",
					);
					skipped += 1;
					continue;
				}
				if !Self::entry_path(data_dir, &hash, BLOBS_DIR, BLOB_EXT).exists() {
					tracing::warn!(target: "hop", hash = ?stem, "Skipping legacy .meta with no .blob");
					skipped += 1;
					continue;
				}
				// A row already in the column is newer than the legacy file.
				if db
					.get(COL_META, hash.as_bytes())
					.map_err(|e| HopError::Db(e.to_string()))?
					.is_some()
				{
					skipped += 1;
					continue;
				}

				ops.push((COL_META, hash.as_bytes().to_vec(), Some(bytes)));
				imported += 1;
				if ops.len() >= MIGRATION_CHUNK {
					db.commit(ops.drain(..)).map_err(|e| HopError::Db(e.to_string()))?;
				}
			}
		}

		if !ops.is_empty() {
			db.commit(ops).map_err(|e| HopError::Db(e.to_string()))?;
		}

		// Rows are committed at this point, so a failure here is not fatal: the
		// next boot rescans, skips every file as already-present, and retries.
		if let Err(e) = fs::remove_dir_all(&legacy_dir) {
			tracing::warn!(
				target: "hop",
				path = ?legacy_dir,
				error = %e,
				"Failed to remove legacy meta directory after import",
			);
		}

		// The pre-KV-store build created all 256 shard directories on every
		// boot, so an empty tree is the normal case for an upgraded idle node
		// and must not look like an event.
		if imported > 0 || skipped > 0 {
			tracing::info!(
				target: "hop",
				imported,
				skipped,
				"Imported legacy HOP metadata into the KV store",
			);
		}
		Ok(())
	}

	/// Get + decode a meta row. Decode failures surface as `Db(...)` since the
	/// startup scan should have already dropped corrupt rows.
	fn fetch_meta(&self, hash: &HopHash) -> Result<Option<HopEntryMeta>, HopError> {
		match self
			.db
			.get(COL_META, hash.as_bytes())
			.map_err(|e| HopError::Db(e.to_string()))?
		{
			Some(bytes) => HopEntryMeta::decode(&mut bytes.as_slice())
				.map(Some)
				.map_err(|e| HopError::Db(format!("decoding meta for {}: {e}", hex::encode(hash)))),
			None => Ok(None),
		}
	}

	/// Commit a single (key, optional value) op to the meta column.
	fn commit_meta(&self, hash: &HopHash, value: Option<Vec<u8>>) -> Result<(), HopError> {
		self.db
			.commit([(COL_META, hash.as_bytes().to_vec(), value)])
			.map_err(|e| HopError::Db(e.to_string()))
	}

	/// Charge `accounted` bytes against `sender_id`'s per-user quota, creating
	/// a zero-initialized counter if absent. The read guard held across the
	/// `fetch_add` excludes the reclamation pass in `cleanup_expired` (which
	/// takes `user_usage.write()`), so the counter cannot be reclaimed
	/// between lookup and increment.
	fn charge_user(&self, sender_id: &SenderId, accounted: u64) -> Result<(), HopError> {
		// Fast path: sender already in map, a read guard is enough.
		{
			let usage = self.user_usage.read();
			if let Some(counter) = usage.get(sender_id) {
				return self.try_charge(counter, accounted);
			}
		}
		// Cold path: first insert from this sender — take the write guard.
		let mut usage = self.user_usage.write();
		let counter = usage.entry(*sender_id).or_default();
		self.try_charge(counter, accounted)
	}

	/// Atomically increment `counter` by `accounted`, rolling back on cap
	/// overflow. `saturating_add` clamps to `u64::MAX` if concurrent failing
	/// charges briefly inflate the previous value past the wrap point,
	/// ensuring overflow always falls into the "exceeds cap" branch.
	fn try_charge(&self, counter: &AtomicU64, accounted: u64) -> Result<(), HopError> {
		let previous = counter.fetch_add(accounted, Ordering::Relaxed);
		if previous.saturating_add(accounted) > self.max_user_size {
			counter.fetch_sub(accounted, Ordering::Relaxed);
			return Err(HopError::UserQuotaExceeded { used: previous, limit: self.max_user_size });
		}
		Ok(())
	}

	/// Decrement a user's usage counter. Counters are never removed by this
	/// path; reclamation happens only in the per-sender pass at the end of
	/// `cleanup_expired`.
	fn release_user_quota(&self, sender_id: &SenderId, accounted: u64) {
		if let Some(counter) = self.user_usage.read().get(sender_id) {
			saturating_release(counter, accounted);
		}
	}

	/// Path to a file within a shard subdirectory rooted at `data_dir`.
	fn entry_path(data_dir: &Path, hash: &HopHash, subdir: &str, ext: &str) -> PathBuf {
		let hex = hex::encode(hash);
		data_dir.join(subdir).join(&hex[..2]).join(format!("{}.{}", hex, ext))
	}

	/// Path to the blob file for a given hash.
	fn blob_path(&self, hash: &HopHash) -> PathBuf {
		Self::entry_path(&self.data_dir, hash, BLOBS_DIR, BLOB_EXT)
	}

	/// Atomically write data to a file (write to a unique .tmp path, then rename).
	///
	/// The tmp suffix encodes process id + a per-process atomic counter so two
	/// threads writing the same final path (i.e. same content-addressed hash)
	/// do not race on a shared tmp file. Removes the tmp file on failure so a
	/// failed write never leaves an orphan.
	fn write_atomic(path: &Path, data: &[u8]) -> Result<(), HopError> {
		let suffix = format!("tmp.{}.{}", process::id(), TMP_SEQ.fetch_add(1, Ordering::Relaxed));
		let tmp_path = path.with_extension(suffix);
		if let Err(e) = fs::write(&tmp_path, data) {
			let _ = fs::remove_file(&tmp_path);
			return Err(e.into());
		}
		if let Err(e) = fs::rename(&tmp_path, path) {
			let _ = fs::remove_file(&tmp_path);
			return Err(e.into());
		}
		Ok(())
	}

	/// Insert data into the pool.
	///
	/// Returns the hash of the data.
	pub fn insert(
		&self,
		data: Vec<u8>,
		recipients: RecipientVec,
		sender_id: SenderId,
		signer: MultiSigner,
		signature: MultiSignature,
		submit_timestamp: u64,
	) -> Result<HopHash, HopError> {
		if recipients.is_empty() {
			return Err(HopError::NoRecipients);
		}
		let unique: BTreeSet<&MultiSigner> = recipients.iter().map(|r| &r.signer).collect();
		if unique.len() != recipients.len() {
			return Err(HopError::DuplicateRecipient);
		}

		if data.is_empty() {
			return Err(HopError::EmptyData);
		}

		let data_len = data.len() as u64;

		// Total accounted size includes bounded per-recipient metadata overhead so
		// a submitter cannot inflate memory via large recipient lists while the
		// capacity counter only tracks `data.len()`. Charge the rate limiter the
		// same accounted size, otherwise a 1-byte payload with 256 recipients
		// would cost ~10 KiB of pool capacity while only spending 1 byte of
		// bandwidth tokens — making the bandwidth dimension non-functional for
		// fan-out-heavy entries.
		let accounted = entry_accounted_size(data_len, recipients.len());

		// Rejected requests never reserve capacity — check before any atomic bump.
		if let Err(retry_after_secs) = self.rate_limiter.check(&sender_id, accounted) {
			return Err(HopError::RateLimited { retry_after_secs });
		}

		let previous_size = self.current_size.fetch_add(accounted, Ordering::Relaxed);
		if previous_size.saturating_add(accounted) > self.max_size {
			self.current_size.fetch_sub(accounted, Ordering::Relaxed);
			return Err(HopError::PoolFull(previous_size, self.max_size));
		}

		if let Err(e) = self.charge_user(&sender_id, accounted) {
			self.current_size.fetch_sub(accounted, Ordering::Relaxed);
			return Err(e);
		}

		let hash = H256(blake2_256(&data));

		// Best-effort duplicate check; authoritative check happens under rmw_lock.
		match self.fetch_meta(&hash) {
			Ok(Some(_)) => {
				self.release_user_quota(&sender_id, accounted);
				self.current_size.fetch_sub(accounted, Ordering::Relaxed);
				return Err(HopError::DuplicateEntry);
			},
			Ok(None) => (),
			Err(e) => {
				self.release_user_quota(&sender_id, accounted);
				self.current_size.fetch_sub(accounted, Ordering::Relaxed);
				return Err(e);
			},
		}

		// Blob first; an orphan from a crash before commit is reaped on next startup.
		let blob_path = self.blob_path(&hash);
		if let Err(e) = Self::write_atomic(&blob_path, &data) {
			self.release_user_quota(&sender_id, accounted);
			self.current_size.fetch_sub(accounted, Ordering::Relaxed);
			return Err(e);
		}

		let expires_at = SystemTime::now()
			.duration_since(UNIX_EPOCH)
			.unwrap_or_default()
			.as_secs()
			.saturating_add(self.retention_secs);
		let meta = HopEntryMeta::new(
			data_len,
			expires_at,
			recipients,
			sender_id,
			signer,
			signature,
			submit_timestamp,
		);
		let meta_bytes = meta.encode();

		// Authoritative dup-check + commit (CAS substitute).
		{
			let _guard = self.rmw_lock.lock();
			match self.fetch_meta(&hash) {
				Ok(Some(_)) => {
					drop(_guard);
					// Winner's blob is byte-identical (content addressing); leave it.
					tracing::debug!(
						target: "hop",
						hash = ?hex::encode(hash),
						"Duplicate insert race lost; keeping winner's blob"
					);
					self.release_user_quota(&sender_id, accounted);
					self.current_size.fetch_sub(accounted, Ordering::Relaxed);
					return Err(HopError::DuplicateEntry);
				},
				Ok(None) => (),
				Err(e) => {
					drop(_guard);
					let _ = fs::remove_file(&blob_path);
					self.release_user_quota(&sender_id, accounted);
					self.current_size.fetch_sub(accounted, Ordering::Relaxed);
					return Err(e);
				},
			}
			if let Err(e) = self.commit_meta(&hash, Some(meta_bytes)) {
				drop(_guard);
				let _ = fs::remove_file(&blob_path);
				self.release_user_quota(&sender_id, accounted);
				self.current_size.fetch_sub(accounted, Ordering::Relaxed);
				return Err(e);
			}
		}

		self.expiry_index.write().insert((expires_at, hash));
		self.entry_count.fetch_add(1, Ordering::Relaxed);
		self.metrics.record_inserted_bytes(accounted);
		self.publish_size_metrics();

		tracing::info!(
			target: "hop",
			hash = ?hex::encode(hash),
			size = data_len,
			accounted,
			expires_at,
			"Data added to HOP pool"
		);

		Ok(hash)
	}

	/// Read a blob from disk and verify its content hash.
	///
	/// Content addressing means `blake2_256(data) == *hash` is an invariant
	/// — corruption (bit rot, partial write, local tampering) violates it.
	/// On integrity failure the caller-facing result is the same as a missing
	/// blob and the broken entry is purged so subsequent reads converge.
	fn read_and_verify_blob(&self, hash: &HopHash) -> Result<Vec<u8>, HopError> {
		let blob_path = self.blob_path(hash);
		let data = fs::read(&blob_path).map_err(|e| {
			if e.kind() == std::io::ErrorKind::NotFound {
				HopError::NotFound
			} else {
				HopError::IoError(e)
			}
		})?;
		if H256(blake2_256(&data)) != *hash {
			tracing::error!(
				target: "hop",
				hash = ?hex::encode(hash),
				size = data.len(),
				"Blob integrity check failed; purging entry"
			);
			self.purge_corrupt_entry(hash);
			return Err(HopError::NotFound);
		}
		Ok(data)
	}

	/// Remove a corrupt entry's meta row and best-effort delete its blob.
	/// The accounted size is released back to the pool and the user quota.
	fn purge_corrupt_entry(&self, hash: &HopHash) {
		let removed = {
			let _guard = self.rmw_lock.lock();
			match self.fetch_meta(hash) {
				Ok(Some(meta)) => {
					if let Err(e) = self.commit_meta(hash, None) {
						tracing::error!(
							target: "hop",
							hash = ?hex::encode(hash),
							error = %e,
							"Failed to delete corrupt meta row",
						);
						None
					} else {
						Some(meta)
					}
				},
				Ok(None) => None,
				Err(e) => {
					tracing::error!(target: "hop", hash = ?hex::encode(hash), error = %e, "Failed to read meta during corrupt-entry purge");
					None
				},
			}
		};
		if let Some(meta) = removed {
			let accounted = entry_accounted_size(meta.size, meta.recipients.len());
			self.expiry_index.write().remove(&(meta.expires_at, *hash));
			self.current_size.fetch_sub(accounted, Ordering::Relaxed);
			self.entry_count.fetch_sub(1, Ordering::Relaxed);
			self.release_user_quota(&meta.sender_id, accounted);
			self.metrics.record_removed(removal_reasons::CORRUPT, 1);
			self.publish_size_metrics();
		}
		let _ = fs::remove_file(self.blob_path(hash));
	}

	/// Read and verify a blob, returning `None` for missing entries and logging
	/// any other failure. Shared by [`Self::get`] and [`Self::get_with_auth`].
	fn read_or_log(&self, hash: &HopHash) -> Option<Vec<u8>> {
		match self.read_and_verify_blob(hash) {
			Ok(data) => Some(data),
			Err(HopError::NotFound) => None,
			Err(e) => {
				tracing::error!(
					target: "hop",
					hash = ?hex::encode(hash),
					error = ?e,
					"Failed to read blob from disk"
				);
				None
			},
		}
	}

	/// Get data from the pool by content hash.
	pub fn get(&self, hash: &HopHash) -> Option<Vec<u8>> {
		match self.fetch_meta(hash) {
			Ok(Some(_)) => self.read_or_log(hash),
			Ok(None) => None,
			Err(e) => {
				tracing::error!(target: "hop", hash = ?hex::encode(hash), error = %e, "Failed to read meta for get");
				None
			},
		}
	}

	/// Get data alongside the submitter's `MultiSigner`, `hop_submit` signature,
	/// and submit timestamp.
	///
	/// Used by the promoter so the unsigned promotion extrinsic can carry the
	/// user's submit-time signature for runtime-side verification.
	pub fn get_with_auth(
		&self,
		hash: &HopHash,
	) -> Option<(Vec<u8>, MultiSigner, MultiSignature, u64)> {
		let meta = match self.fetch_meta(hash) {
			Ok(Some(m)) => m,
			Ok(None) => return None,
			Err(e) => {
				tracing::error!(target: "hop", hash = ?hex::encode(hash), error = %e, "Failed to read meta for get_with_auth");
				return None;
			},
		};
		let data = self.read_or_log(hash)?;
		Some((data, meta.signer, meta.signature, meta.submit_timestamp))
	}

	/// Decode `signature` and return the index of the matching recipient in
	/// `meta.recipients`. `context` is the operation's domain separator (claim
	/// / ack). Returning an index keeps a single implementation for both
	/// shared- and exclusive-borrow callers (`meta.recipients[idx]` works in
	/// either case).
	fn find_recipient_idx(
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
			.position(|r| multi_sig.verify(&payload[..], &r.signer.clone().into_account()))
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
		let meta = self.fetch_meta(hash)?.ok_or(HopError::NotFound)?;
		// Map NotRecipient → NotFound so callers cannot probe whether a hash
		// exists by observing different error codes.
		let idx = Self::find_recipient_idx(&meta, hash, signature, HOP_CLAIM_CONTEXT)
			.map_err(|_| HopError::NotFound)?;

		// If this recipient already acked, the data may be gone.
		if meta.recipients[idx].claimed {
			return Err(HopError::AlreadyClaimed);
		}
		// Read blob from disk and verify its content hash. May be gone if
		// concurrently acked and deleted, in which case we surface NotFound.
		self.read_and_verify_blob(hash)
	}

	/// Acknowledge receipt of claimed data. Marks the recipient as claimed and triggers
	/// cleanup when all recipients have acked.
	///
	/// Idempotent: acking a recipient that already acked returns `Ok(())`.
	pub fn ack(&self, hash: &HopHash, signature: &[u8]) -> Result<(), HopError> {
		// Phase 1: idempotent fast-path read; no lock acquired.
		{
			let meta = self.fetch_meta(hash)?.ok_or(HopError::NotFound)?;
			let idx = Self::find_recipient_idx(&meta, hash, signature, HOP_ACK_CONTEXT)
				.map_err(|_| HopError::NotFound)?;
			if meta.recipients[idx].claimed {
				return Ok(());
			}
		}

		// Phase 2: RMW under rmw_lock; re-run the lookup as the meta may have changed.
		let _guard = self.rmw_lock.lock();
		let mut meta = self.fetch_meta(hash)?.ok_or(HopError::NotFound)?;
		let idx = Self::find_recipient_idx(&meta, hash, signature, HOP_ACK_CONTEXT)
			.map_err(|_| HopError::NotFound)?;

		if meta.recipients[idx].claimed {
			return Ok(());
		}

		meta.recipients[idx].claimed = true;

		if meta.recipients.iter().all(|r| r.claimed) {
			let accounted = entry_accounted_size(meta.size, meta.recipients.len());
			let sender = meta.sender_id;
			let expires_at = meta.expires_at;
			self.commit_meta(hash, None)?;
			drop(_guard);

			self.expiry_index.write().remove(&(expires_at, *hash));
			self.current_size.fetch_sub(accounted, Ordering::Relaxed);
			self.entry_count.fetch_sub(1, Ordering::Relaxed);
			self.release_user_quota(&sender, accounted);
			self.metrics.record_removed(removal_reasons::ACKED, 1);
			self.publish_size_metrics();

			// Blob delete is best-effort; orphans get reaped on restart.
			let _ = fs::remove_file(self.blob_path(hash));

			tracing::info!(
				target: "hop",
				hash = ?hex::encode(hash),
				"All recipients acked, data removed"
			);
		} else {
			let claimed_count = meta.recipients.iter().filter(|r| r.claimed).count();
			self.commit_meta(hash, Some(meta.encode()))?;
			drop(_guard);

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
	#[cfg(test)]
	pub fn has(&self, hash: &HopHash) -> bool {
		matches!(self.fetch_meta(hash), Ok(Some(_)))
	}

	/// Remove data from the pool.
	#[cfg(test)]
	pub fn remove(&self, hash: &HopHash) -> Result<(), HopError> {
		let meta = {
			let _guard = self.rmw_lock.lock();
			let Some(meta) = self.fetch_meta(hash)? else {
				return Err(HopError::NotFound);
			};
			self.commit_meta(hash, None)?;
			meta
		};

		let accounted = entry_accounted_size(meta.size, meta.recipients.len());
		self.expiry_index.write().remove(&(meta.expires_at, *hash));
		self.current_size.fetch_sub(accounted, Ordering::Relaxed);
		self.entry_count.fetch_sub(1, Ordering::Relaxed);
		self.release_user_quota(&meta.sender_id, accounted);

		let _ = fs::remove_file(self.blob_path(hash));

		tracing::debug!(
			target: "hop",
			hash = ?hex::encode(hash),
			"Data removed from pool"
		);

		Ok(())
	}

	/// Get pool status.
	pub fn status(&self) -> PoolStatus {
		PoolStatus {
			entry_count: self.entry_count.load(Ordering::Relaxed) as usize,
			total_bytes: self.current_size.load(Ordering::Relaxed),
			max_bytes: self.max_size,
		}
	}

	/// Remove expired entries, release their quotas, return total bytes freed.
	///
	/// Uses `expiry_index` to enumerate expired hashes in O(K), not O(N) over
	/// the meta column. Entries are processed in batches so a long outage
	/// can't buffer the whole expired set in RAM before any progress lands.
	///
	/// `promotion_buffer_secs` does not affect what is cleaned up; as in
	/// [`Self::get_promotable`] the caller owns the promotion window. Here it
	/// only scopes the promotion-backlog gauge, snapshot after the sweep.
	pub fn cleanup_expired(&self, promotion_buffer_secs: u64) -> u64 {
		const CLEANUP_BATCH_SIZE: usize = 10_000;
		let now_secs = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs();

		let mut total_freed: u64 = 0;

		loop {
			// Snapshot one batch of expired index entries. The range is
			// inclusive of `(now, max-hash)` so an entry expiring exactly at
			// `now` is reaped this tick.
			let batch: Vec<(u64, HopHash)> = {
				let guard = self.expiry_index.read();
				guard
					.range((Bound::Unbounded, Bound::Included(&(now_secs, H256([0xff; 32])))))
					.take(CLEANUP_BATCH_SIZE)
					.copied()
					.collect()
			};

			if batch.is_empty() {
				break;
			}

			// Phase 2: re-read under rmw_lock (entries may have changed since
			// the snapshot), commit deletions, collect metas for accounting.
			let mut processed: Vec<(HopHash, HopEntryMeta)> = Vec::with_capacity(batch.len());
			{
				let _guard = self.rmw_lock.lock();
				let mut ops: Vec<(u8, Vec<u8>, Option<Vec<u8>>)> = Vec::with_capacity(batch.len());
				for (_, hash) in &batch {
					match self.fetch_meta(hash) {
						Ok(Some(meta)) if now_secs >= meta.expires_at => {
							ops.push((COL_META, hash.as_bytes().to_vec(), None));
							processed.push((*hash, meta));
						},
						Ok(_) => (), // gone or refreshed by a concurrent op
						Err(e) => {
							tracing::error!(target: "hop", hash = ?hex::encode(hash), error = %e, "cleanup_expired: failed to re-read meta");
						},
					}
				}
				if !ops.is_empty() {
					if let Err(e) = self.db.commit(ops) {
						tracing::error!(target: "hop", error = %e, "cleanup_expired: batch commit failed");
						break;
					}
				}
			}

			// Phase 3: drop every batch entry from the index — both processed
			// ones and any stale snapshots from a racing op — to guarantee
			// forward progress. Releasing the read guard before taking write
			// avoids a deadlock since the snapshot already finished above.
			{
				let mut index = self.expiry_index.write();
				for entry in &batch {
					index.remove(entry);
				}
			}

			if processed.is_empty() {
				continue;
			}

			// Entries expiring unpromoted are the data-loss case; count them
			// separately from ones that made it on-chain before expiry.
			let mut freed = 0u64;
			let mut promoted = 0u64;
			let mut unpromoted = 0u64;
			for (_, meta) in &processed {
				freed = freed.saturating_add(entry_accounted_size(meta.size, meta.recipients.len()));
				if meta.promoted {
					promoted = promoted.saturating_add(1);
				} else {
					unpromoted = unpromoted.saturating_add(1);
				}
			}
			self.current_size.fetch_sub(freed, Ordering::Relaxed);
			self.entry_count.fetch_sub(processed.len() as u64, Ordering::Relaxed);
			total_freed = total_freed.saturating_add(freed);
			self.metrics.record_removed(removal_reasons::EXPIRED_PROMOTED, promoted);
			self.metrics.record_removed(removal_reasons::EXPIRED_UNPROMOTED, unpromoted);

			{
				let usage = self.user_usage.read();
				for (_, meta) in &processed {
					if let Some(counter) = usage.get(&meta.sender_id) {
						let accounted = entry_accounted_size(meta.size, meta.recipients.len());
						saturating_release(counter, accounted);
					}
				}
			}

			for (hash, _) in &processed {
				let _ = fs::remove_file(self.blob_path(hash));
			}
		}

		// Phase 4: drop per-sender counters that have settled to 0. A live
		// sender's counter is kept above 0 by `charge_user`'s read guard,
		// which excludes this write guard, so usage=0 means no live entries.
		{
			let mut usage = self.user_usage.write();
			usage.retain(|_, counter| counter.load(Ordering::Relaxed) > 0);
		}

		// Let the rate limiter shed stale per-sender state on the same cadence.
		self.rate_limiter.evict_stale();

		// Snapshot the size gauges and the promotion backlog. The backlog walk
		// re-reads the meta column for every entry in the promotion window, so
		// it is gated on metrics being enabled; `promotion_buffer_secs` only
		// scopes this gauge and does not change what was cleaned up above.
		self.publish_size_metrics();
		let backlog = if self.metrics.is_enabled() {
			let frontier = now_secs.saturating_add(promotion_buffer_secs);
			let candidates: Vec<HopHash> = {
				let guard = self.expiry_index.read();
				guard
					.range((Bound::Unbounded, Bound::Included(&(frontier, H256([0xff; 32])))))
					.map(|(_, hash)| *hash)
					.collect()
			};
			candidates
				.into_iter()
				.filter(|hash| {
					matches!(
						self.fetch_meta(hash),
						Ok(Some(meta))
							if Self::in_promotion_window(&meta, now_secs, promotion_buffer_secs)
					)
				})
				.count() as u64
		} else {
			0
		};
		self.metrics.set_promotion_backlog(backlog);

		total_freed
	}

	/// Outstanding promotion candidate: unpromoted, near expiry, attempts left.
	/// Ignores the back-off deadline — a backing-off entry is still outstanding.
	fn in_promotion_window(meta: &HopEntryMeta, now_secs: u64, buffer_secs: u64) -> bool {
		!meta.promoted &&
			now_secs.saturating_add(buffer_secs) >= meta.expires_at &&
			meta.promotion_attempts < MAX_PROMOTION_ATTEMPTS
	}

	/// Return hashes of entries within `buffer_secs` of expiry that have not yet been promoted.
	/// Returns up to `limit` hashes. Use [`Self::get`] to read blob data when needed.
	/// The maintenance task runs periodically, so remaining entries are picked up next cycle.
	///
	/// Uses `expiry_index` to walk only entries inside the promotion window;
	/// the meta column is touched only for candidates that pass the window
	/// filter, so cost scales with the window size, not the pool.
	pub fn get_promotable(
		&self,
		current_block: HopBlockNumber,
		buffer_secs: u64,
		limit: usize,
	) -> Vec<HopHash> {
		let now_secs = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs();
		let frontier = now_secs.saturating_add(buffer_secs);

		// Snapshot candidate hashes inside the window so the read guard isn't
		// held across `fetch_meta` calls.
		let candidates: Vec<HopHash> = {
			let guard = self.expiry_index.read();
			guard
				.range((Bound::Unbounded, Bound::Included(&(frontier, H256([0xff; 32])))))
				.map(|(_, hash)| *hash)
				.collect()
		};

		let mut out: Vec<HopHash> = Vec::new();
		for hash in candidates {
			if out.len() >= limit {
				break;
			}
			match self.fetch_meta(&hash) {
				Ok(Some(meta))
					if Self::in_promotion_window(&meta, now_secs, buffer_secs) &&
						current_block >= meta.next_promotion_attempt_at =>
				{
					out.push(hash);
				},
				_ => (),
			}
		}
		out
	}

	/// Mark an entry as promoted to permanent on-chain storage.
	pub fn mark_promoted(&self, hash: &HopHash) {
		let _guard = self.rmw_lock.lock();
		let Ok(Some(mut meta)) = self.fetch_meta(hash) else { return };
		// Count the transition, not the call: this setter is idempotent.
		let newly_promoted = !meta.promoted;
		meta.promoted = true;
		if let Err(e) = self.commit_meta(hash, Some(meta.encode())) {
			tracing::error!(
				target: "hop",
				hash = ?hex::encode(hash),
				error = %e,
				"Failed to persist promoted state"
			);
			return;
		}
		if newly_promoted {
			self.metrics.record_promotion_confirmed();
		}
	}

	/// Record a promotion attempt: bumps the per-entry attempt counter and
	/// schedules the next eligible block via exponential back-off. The
	/// maintenance task will skip the entry until then. Once
	/// `MAX_PROMOTION_ATTEMPTS` is reached the entry is left to expire.
	///
	/// Called on **both** an `Err` from `submit_local` (the tx pool rejected
	/// us) and an `Ok` followed by a runtime check that the data is not yet
	/// on-chain (the tx was accepted into the pool but never included). The
	/// backoff schedule is identical for both cases.
	pub fn record_promotion_attempt(
		&self,
		hash: &HopHash,
		current_block: HopBlockNumber,
		check_interval_blocks: u32,
	) {
		let _guard = self.rmw_lock.lock();
		let Ok(Some(mut meta)) = self.fetch_meta(hash) else { return };
		meta.promotion_attempts = meta.promotion_attempts.saturating_add(1);
		let backoff = promotion_backoff_blocks(meta.promotion_attempts, check_interval_blocks);
		meta.next_promotion_attempt_at = current_block.saturating_add(backoff);
		if let Err(e) = self.commit_meta(hash, Some(meta.encode())) {
			tracing::error!(
				target: "hop",
				hash = ?hex::encode(hash),
				error = %e,
				"Failed to persist promotion-attempt state"
			);
		}
	}
}

/// Decode a 64-char hex stem into a `HopHash`. Returns `None` for any
/// non-32-byte stem (corrupt name, wrong length, non-hex chars).
fn parse_hex_hash(stem: &str) -> Option<HopHash> {
	let bytes = hex::decode(stem).ok()?;
	let arr: [u8; 32] = bytes.try_into().ok()?;
	Some(H256(arr))
}

/// Atomically subtract `accounted` from `counter`, clamped so the counter
/// cannot underflow. The CAS retry inside `fetch_update` keeps the clamp
/// value fresh — a plain `counter.fetch_sub(accounted.min(counter.load()), …)`
/// would race with concurrent releases on the same counter and could wrap
/// to near `u64::MAX`.
fn saturating_release(counter: &AtomicU64, accounted: u64) {
	let _ = counter.fetch_update(Ordering::Relaxed, Ordering::Relaxed, |previous| {
		Some(previous - accounted.min(previous))
	});
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::types::{Recipient, MAX_RECIPIENTS};
	use sp_core::{crypto::Pair, ed25519, sr25519};
	use sp_runtime::MultiSigner;
	use tempfile::TempDir;

	const SENDER_A: SenderId = [1u8; 32];
	const SENDER_B: SenderId = [2u8; 32];

	/// Accounted cost of an entry with `data_size` bytes and `num_recipients` recipients.
	fn acct(data_size: u64, num_recipients: usize) -> u64 {
		entry_accounted_size(data_size, num_recipients)
	}

	fn make_pool(max_size: u64, retention_secs: u64) -> (HopDataPool, TempDir) {
		let dir = TempDir::new().unwrap();
		let pool = HopDataPool::new(
			max_size,
			max_size,
			retention_secs,
			dir.path().to_path_buf(),
			RateLimitConfig::disabled(),
			HopMetrics::disabled(),
		)
		.unwrap();
		(pool, dir)
	}

	fn make_pool_with_user_cap(
		max_size: u64,
		max_user_size: u64,
		retention_secs: u64,
	) -> (HopDataPool, TempDir) {
		let dir = TempDir::new().unwrap();
		let pool = HopDataPool::new(
			max_size,
			max_user_size,
			retention_secs,
			dir.path().to_path_buf(),
			RateLimitConfig::disabled(),
			HopMetrics::disabled(),
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

	/// Deterministic placeholder `(MultiSigner, MultiSignature)` for tests that
	/// don't exercise submit-signature semantics. The actual values are never
	/// verified by these tests.
	fn dummy_auth() -> (MultiSigner, MultiSignature) {
		let pair = ed25519::Pair::from_seed(&[7u8; 32]);
		let signer = MultiSigner::Ed25519(pair.public());
		let sig = MultiSignature::Ed25519(pair.sign(&[]));
		(signer, sig)
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

	/// Convert a `Vec<MultiSigner>` into a `RecipientVec` (with `claimed=false` for
	/// each) for test ergonomics; panics only if a test exceeds `MAX_RECIPIENTS`.
	fn bv(v: Vec<MultiSigner>) -> RecipientVec {
		let recipients: Vec<Recipient> =
			v.into_iter().map(|signer| Recipient { signer, claimed: false }).collect();
		RecipientVec::try_from(recipients).expect("test recipient list exceeds MAX_RECIPIENTS")
	}

	/// A `HopEntryMeta` matching `data` for `sender`, expiring far in the future.
	fn legacy_meta(data: &[u8], sender: SenderId) -> HopEntryMeta {
		let (_, signer) = test_recipient();
		HopEntryMeta::new(
			data.len() as u64,
			u64::MAX,
			bv(vec![signer]),
			sender,
			dummy_auth().0,
			dummy_auth().1,
			0,
		)
	}

	/// Lay down a pre-KV-store entry by hand: `blobs/{shard}/{hash}.blob` plus a
	/// SCALE-encoded `meta/{shard}/{hash}.meta` companion file, with no `meta-db/`
	/// involved. Returns the content hash.
	fn write_legacy_entry(dir: &Path, data: &[u8], meta_bytes: &[u8]) -> HopHash {
		let hash = H256(blake2_256(data));
		let blob_path = HopDataPool::entry_path(dir, &hash, BLOBS_DIR, BLOB_EXT);
		fs::create_dir_all(blob_path.parent().unwrap()).unwrap();
		fs::write(&blob_path, data).unwrap();
		write_legacy_meta_only(dir, &hash, meta_bytes);
		hash
	}

	/// Write only the legacy `.meta` file for `hash`, with no blob.
	fn write_legacy_meta_only(dir: &Path, hash: &HopHash, meta_bytes: &[u8]) {
		let meta_path = HopDataPool::entry_path(dir, hash, LEGACY_META_DIR, LEGACY_META_EXT);
		fs::create_dir_all(meta_path.parent().unwrap()).unwrap();
		fs::write(&meta_path, meta_bytes).unwrap();
	}

	/// Read the raw schema version row, or `None` if it is absent.
	fn read_db_version(dir: &Path) -> Option<u32> {
		let db = parity_db::Db::open_or_create(&HopDataPool::db_options(&dir.join(META_DB_DIR)))
			.unwrap();
		db.get(COL_DB_META, KEY_DB_VERSION)
			.unwrap()
			.map(|bytes| u32::from_le_bytes(bytes.try_into().unwrap()))
	}

	#[test]
	fn test_insert_and_get() {
		let (pool, _dir) = create_test_pool();
		let (_, signer) = test_recipient();
		let data = vec![1, 2, 3, 4, 5];
		let hash = pool
			.insert(data.clone(), bv(vec![signer]), SENDER_A, dummy_auth().0, dummy_auth().1, 0)
			.unwrap();

		let retrieved = pool.get(&hash).unwrap();
		assert_eq!(data, retrieved);
	}

	#[test]
	fn test_insert_no_recipients() {
		let (pool, _dir) = create_test_pool();
		let data = vec![1, 2, 3, 4, 5];
		let result = pool.insert(data, bv(vec![]), SENDER_A, dummy_auth().0, dummy_auth().1, 0);
		assert!(matches!(result, Err(HopError::NoRecipients)));
	}

	#[test]
	fn test_duplicate_insert() {
		let (pool, _dir) = create_test_pool();
		let (_, signer) = test_recipient();
		let data = vec![1, 2, 3, 4, 5];

		pool.insert(
			data.clone(),
			bv(vec![signer.clone()]),
			SENDER_A,
			dummy_auth().0,
			dummy_auth().1,
			0,
		)
		.unwrap();
		let result =
			pool.insert(data, bv(vec![signer]), SENDER_A, dummy_auth().0, dummy_auth().1, 0);

		assert!(matches!(result, Err(HopError::DuplicateEntry)));
	}

	#[test]
	fn test_too_many_recipients_rejected_at_type_level() {
		// Construction of a `RecipientVec` with more than `MAX_RECIPIENTS` entries
		// fails at `try_from`; callers (like the RPC) turn that into a
		// `TooManyRecipients` error before reaching the pool.
		let recipients: Vec<Recipient> = (0..=MAX_RECIPIENTS as u64)
			.map(|i| {
				let mut seed = [0u8; 32];
				seed[..8].copy_from_slice(&i.to_le_bytes());
				Recipient {
					signer: MultiSigner::Ed25519(ed25519::Pair::from_seed(&seed).public()),
					claimed: false,
				}
			})
			.collect();
		assert_eq!(recipients.len(), MAX_RECIPIENTS as usize + 1);
		assert!(RecipientVec::try_from(recipients).is_err());
	}

	#[test]
	fn test_duplicate_recipient_rejected() {
		let (pool, _dir) = create_test_pool();
		let (_, signer) = test_recipient();
		let result = pool.insert(
			vec![1, 2, 3],
			bv(vec![signer.clone(), signer]),
			SENDER_A,
			dummy_auth().0,
			dummy_auth().1,
			0,
		);
		assert!(matches!(result, Err(HopError::DuplicateRecipient)));
	}

	#[test]
	fn test_pool_full() {
		// Capacity exactly holds one 60-byte entry with one recipient (60 + 40 = 100).
		let (pool, _dir) = make_pool(acct(60, 1), 100);
		let (_, signer) = test_recipient();

		let data1 = vec![0u8; 60];
		let data2 = vec![1u8; 50];

		pool.insert(data1, bv(vec![signer.clone()]), SENDER_A, dummy_auth().0, dummy_auth().1, 0)
			.unwrap();
		let result =
			pool.insert(data2, bv(vec![signer]), SENDER_A, dummy_auth().0, dummy_auth().1, 0);

		assert!(matches!(result, Err(HopError::PoolFull(_, _))));
	}

	#[test]
	fn test_remove() {
		let (pool, _dir) = create_test_pool();
		let (_, signer) = test_recipient();
		let data = vec![1, 2, 3, 4, 5];
		let hash = pool
			.insert(data, bv(vec![signer]), SENDER_A, dummy_auth().0, dummy_auth().1, 0)
			.unwrap();

		assert!(pool.has(&hash));
		pool.remove(&hash).unwrap();
		assert!(!pool.has(&hash));

		// Blob file should be cleaned up; meta lives in parity-db (not a file).
		assert!(!pool.blob_path(&hash).exists());
	}

	#[test]
	fn test_status() {
		let (pool, _dir) = create_test_pool();
		let (_, signer) = test_recipient();
		let data1 = vec![1, 2, 3, 4, 5];
		let data2 = vec![6, 7, 8];

		pool.insert(
			data1.clone(),
			bv(vec![signer.clone()]),
			SENDER_A,
			dummy_auth().0,
			dummy_auth().1,
			0,
		)
		.unwrap();
		pool.insert(data2.clone(), bv(vec![signer]), SENDER_A, dummy_auth().0, dummy_auth().1, 0)
			.unwrap();

		let status = pool.status();
		assert_eq!(status.entry_count, 2);
		assert_eq!(status.total_bytes, acct(data1.len() as u64, 1) + acct(data2.len() as u64, 1));
	}

	#[test]
	fn test_claim_valid_signature() {
		let (pool, _dir) = create_test_pool();
		let (pair, signer) = test_recipient();
		let data = vec![1, 2, 3, 4, 5];
		let hash = pool
			.insert(data.clone(), bv(vec![signer]), SENDER_A, dummy_auth().0, dummy_auth().1, 0)
			.unwrap();

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
		let hash = pool
			.insert(vec![1, 2, 3], bv(vec![signer]), SENDER_A, dummy_auth().0, dummy_auth().1, 0)
			.unwrap();

		let claim = sign_ed(&pair, HOP_CLAIM_CONTEXT, &hash);
		pool.claim(&hash, &claim).unwrap();
		assert!(matches!(pool.ack(&hash, &claim), Err(HopError::NotFound)));
	}

	#[test]
	fn test_claim_invalid_signature() {
		let (pool, _dir) = create_test_pool();
		let (_, signer) = test_recipient();
		let data = vec![1, 2, 3, 4, 5];
		let hash = pool
			.insert(data, bv(vec![signer]), SENDER_A, dummy_auth().0, dummy_auth().1, 0)
			.unwrap();

		// Use invalid SCALE bytes — cannot decode as MultiSignature
		let result = pool.claim(&hash, &[0u8; 3]);
		assert!(matches!(result, Err(HopError::NotFound)));
	}

	#[test]
	fn test_claim_wrong_key() {
		let (pool, _dir) = create_test_pool();
		let (_, signer) = test_recipient();
		let hash = pool
			.insert(
				vec![1, 2, 3, 4, 5],
				bv(vec![signer]),
				SENDER_A,
				dummy_auth().0,
				dummy_auth().1,
				0,
			)
			.unwrap();

		let wrong_pair = ed25519::Pair::from_seed(&[99u8; 32]);
		let wrong_claim = sign_ed(&wrong_pair, HOP_CLAIM_CONTEXT, &hash);
		assert!(matches!(pool.claim(&hash, &wrong_claim), Err(HopError::NotFound)));
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
		let hash = pool
			.insert(
				data.clone(),
				bv(vec![signer1, signer2]),
				SENDER_A,
				dummy_auth().0,
				dummy_auth().1,
				0,
			)
			.unwrap();

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
			.insert(
				vec![1, 2, 3, 4, 5],
				bv(vec![signer, signer2]),
				SENDER_A,
				dummy_auth().0,
				dummy_auth().1,
				0,
			)
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

		pool.insert(
			vec![0u8; 60],
			bv(vec![signer.clone()]),
			SENDER_A,
			dummy_auth().0,
			dummy_auth().1,
			0,
		)
		.unwrap();

		// User A is at the cap; next insert is rejected regardless of pool headroom.
		let result = pool.insert(
			vec![1u8; 10],
			bv(vec![signer.clone()]),
			SENDER_A,
			dummy_auth().0,
			dummy_auth().1,
			0,
		);
		assert!(matches!(result, Err(HopError::UserQuotaExceeded { .. })));

		// User B has their own independent cap.
		pool.insert(vec![2u8; 60], bv(vec![signer]), SENDER_B, dummy_auth().0, dummy_auth().1, 0)
			.unwrap();
	}

	#[test]
	fn test_quota_released_after_ack() {
		let (pool, _dir) = make_pool_with_user_cap(10_000, acct(100, 1), 100);
		let (pair, signer) = test_recipient();

		let hash = pool
			.insert(
				vec![0u8; 100],
				bv(vec![signer.clone()]),
				SENDER_A,
				dummy_auth().0,
				dummy_auth().1,
				0,
			)
			.unwrap();

		// At cap; next insert rejected.
		let result = pool.insert(
			vec![1u8; 10],
			bv(vec![signer.clone()]),
			SENDER_A,
			dummy_auth().0,
			dummy_auth().1,
			0,
		);
		assert!(matches!(result, Err(HopError::UserQuotaExceeded { .. })));

		let claim = sign_ed(&pair, HOP_CLAIM_CONTEXT, &hash);
		let ack = sign_ed(&pair, HOP_ACK_CONTEXT, &hash);
		pool.claim(&hash, &claim).unwrap();
		pool.ack(&hash, &ack).unwrap();

		// Quota freed — user can insert again.
		pool.insert(vec![2u8; 100], bv(vec![signer]), SENDER_A, dummy_auth().0, dummy_auth().1, 0)
			.unwrap();
	}

	#[test]
	fn test_cleanup_expired_releases_quota() {
		let (pool, _dir) = make_pool(10_000, 0);
		let (_, signer) = test_recipient();

		pool.insert(vec![0u8; 100], bv(vec![signer]), SENDER_A, dummy_auth().0, dummy_auth().1, 0)
			.unwrap();
		let charged = acct(100, 1);
		assert_eq!(user_usage(&pool, &SENDER_A), charged);

		let freed = pool.cleanup_expired(0);
		assert_eq!(freed, charged);
		assert_eq!(pool.status().total_bytes, 0);
		assert_eq!(user_usage(&pool, &SENDER_A), 0);
	}

	#[test]
	fn test_cleanup_expired_honors_wall_clock_retention() {
		// Retention is measured in real seconds, not blocks: insert with a 1 s
		// retention, sleep past it, and assert cleanup reaps the entry.
		let (pool, _dir) = make_pool(10_000, 1);
		let (_, signer) = test_recipient();

		let hash = pool
			.insert(vec![0u8; 100], bv(vec![signer]), SENDER_A, dummy_auth().0, dummy_auth().1, 0)
			.unwrap();

		// Not yet expired — cleanup must be a no-op.
		assert_eq!(
			pool.cleanup_expired(0),
			0,
			"entry should still be live before retention elapses"
		);
		assert!(pool.has(&hash));

		std::thread::sleep(std::time::Duration::from_millis(1_200));

		assert!(
			pool.cleanup_expired(0) > 0,
			"entry should be reaped once wall-clock retention elapses"
		);
		assert!(!pool.has(&hash));
	}

	#[test]
	fn test_user_counter_preserved_until_cleanup() {
		// release_user_quota does not remove the map entry — only cleanup_expired
		// reclaims stale per-sender slots. Until then the slot remains at 0 so a
		// concurrent insert would not orphan its `Arc`.
		let (pool, _dir) = create_test_pool();
		let (pair, signer) = test_recipient();

		let hash = pool
			.insert(vec![0u8; 50], bv(vec![signer]), SENDER_A, dummy_auth().0, dummy_auth().1, 0)
			.unwrap();
		assert!(pool.user_usage.read().contains_key(&SENDER_A));

		let claim = sign_ed(&pair, HOP_CLAIM_CONTEXT, &hash);
		let ack = sign_ed(&pair, HOP_ACK_CONTEXT, &hash);
		pool.claim(&hash, &claim).unwrap();
		pool.ack(&hash, &ack).unwrap();

		assert_eq!(user_usage(&pool, &SENDER_A), 0);
		assert!(pool.user_usage.read().contains_key(&SENDER_A));
	}

	#[test]
	fn test_cleanup_expired_evicts_idle_user_counters() {
		// After cleanup_expired runs and a sender has no live entries with a
		// non-zero counter, their map slot must be removed so the map cannot
		// grow unbounded across the lifetime of a long-running node.
		let (pool, _dir) = make_pool(10_000, 10);
		let (pair, signer) = test_recipient();

		let hash = pool
			.insert(vec![0u8; 50], bv(vec![signer]), SENDER_A, dummy_auth().0, dummy_auth().1, 0)
			.unwrap();
		let claim = sign_ed(&pair, HOP_CLAIM_CONTEXT, &hash);
		let ack = sign_ed(&pair, HOP_ACK_CONTEXT, &hash);
		pool.claim(&hash, &claim).unwrap();
		pool.ack(&hash, &ack).unwrap();
		assert!(pool.user_usage.read().contains_key(&SENDER_A));

		pool.cleanup_expired(0);
		assert!(!pool.user_usage.read().contains_key(&SENDER_A));
	}

	#[test]
	fn test_cleanup_expired_keeps_active_user_counters() {
		// A sender with live (non-expired) entries must keep their counter
		// even when the counter dropped to 0 between submissions — otherwise
		// concurrent in-flight inserts could orphan their `Arc`.
		let (pool, _dir) = make_pool(10_000, 100);
		let (_, signer) = test_recipient();

		pool.insert(vec![0u8; 50], bv(vec![signer]), SENDER_A, dummy_auth().0, dummy_auth().1, 0)
			.unwrap();
		// Cleanup at a block where the entry is not yet expired must not
		// reclaim the sender's slot — a concurrent insert would otherwise
		// orphan its `Arc`.
		pool.cleanup_expired(0);
		assert!(pool.user_usage.read().contains_key(&SENDER_A));
	}

	#[test]
	fn test_cleanup_expired_processes_more_than_one_batch() {
		// Cleanup batch size is 10_000 — feed it 25_000 entries that all expire,
		// confirm every entry is removed (proving the loop terminates rather
		// than leaving leftovers from the first batch).
		const BATCHES: u32 = 2;
		const PER_BATCH: u32 = 10_000 + 1; // > one batch each
		let total = BATCHES * PER_BATCH;

		let dir = TempDir::new().unwrap();
		// Pool sized for ~25k tiny entries (4 bytes each + recipient overhead).
		let entry_bytes = std::mem::size_of::<u32>() as u64;
		let pool = HopDataPool::new(
			(acct(entry_bytes, 1) * total as u64) + 1024,
			u64::MAX,
			0,
			dir.path().to_path_buf(),
			RateLimitConfig::disabled(),
			HopMetrics::disabled(),
		)
		.unwrap();
		let (_, signer) = test_recipient();

		for i in 0..total {
			let mut sender = SENDER_A;
			sender[0] = (i & 0xff) as u8;
			sender[1] = ((i >> 8) & 0xff) as u8;
			sender[2] = ((i >> 16) & 0xff) as u8;
			// Data must be unique per entry — content-addressing means equal
			// bytes hash to the same key and the second insert hits
			// DuplicateEntry. Embed `i` so each blob is distinct.
			let data = i.to_le_bytes().to_vec();
			pool.insert(data, bv(vec![signer.clone()]), sender, dummy_auth().0, dummy_auth().1, 0)
				.unwrap();
		}
		assert_eq!(pool.status().entry_count, total as usize);

		pool.cleanup_expired(0);
		assert_eq!(pool.status().entry_count, 0);
		assert_eq!(pool.status().total_bytes, 0);
		assert!(pool.user_usage.read().is_empty());
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
				HopMetrics::disabled(),
			)
			.unwrap();
			hash = pool
				.insert(
					vec![42u8; 100],
					bv(vec![signer]),
					SENDER_A,
					dummy_auth().0,
					dummy_auth().1,
					0,
				)
				.unwrap();
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
				HopMetrics::disabled(),
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
				HopMetrics::disabled(),
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
			HopMetrics::disabled(),
		)
		.unwrap();
		assert!(!blob_path.exists());
	}

	#[test]
	fn test_corrupt_meta_cleanup() {
		let dir = TempDir::new().unwrap();
		// Boot once to create the parity-db on-disk layout.
		{
			let _pool = HopDataPool::new(
				1024 * 1024,
				1024 * 1024,
				100,
				dir.path().to_path_buf(),
				RateLimitConfig::disabled(),
				HopMetrics::disabled(),
			)
			.unwrap();
		}

		// Inject an undecodable meta row directly into the column.
		let fake_hash = H256([0xbbu8; 32]);
		{
			let db_path = dir.path().join(META_DB_DIR);
			let db = parity_db::Db::open_or_create(&HopDataPool::db_options(&db_path)).unwrap();
			db.commit([(
				COL_META,
				fake_hash.as_bytes().to_vec(),
				Some(b"not valid SCALE data".to_vec()),
			)])
			.unwrap();
		}

		let pool = HopDataPool::new(
			1024 * 1024,
			1024 * 1024,
			100,
			dir.path().to_path_buf(),
			RateLimitConfig::disabled(),
			HopMetrics::disabled(),
		)
		.unwrap();
		// The corrupt row should have been dropped on startup.
		assert!(!pool.has(&fake_hash));
		assert_eq!(pool.status().entry_count, 0);
	}

	#[test]
	fn test_claim_sr25519() {
		let (pool, _dir) = create_test_pool();
		let pair = sr25519::Pair::from_seed(&[3u8; 32]);
		let signer = MultiSigner::Sr25519(pair.public());

		let data = vec![10, 20, 30];
		let hash = pool
			.insert(data.clone(), bv(vec![signer]), SENDER_A, dummy_auth().0, dummy_auth().1, 0)
			.unwrap();

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
		let hash = pool
			.insert(
				data.clone(),
				bv(vec![ed_signer, sr_signer]),
				SENDER_A,
				dummy_auth().0,
				dummy_auth().1,
				0,
			)
			.unwrap();

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
		let hash = pool
			.insert(data.clone(), bv(vec![signer]), SENDER_A, dummy_auth().0, dummy_auth().1, 0)
			.unwrap();

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
			.insert(
				vec![1, 2, 3, 4, 5],
				bv(vec![signer, signer2]),
				SENDER_A,
				dummy_auth().0,
				dummy_auth().1,
				0,
			)
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
		let hash = pool
			.insert(
				data.clone(),
				bv(vec![signer1, signer2]),
				SENDER_A,
				dummy_auth().0,
				dummy_auth().1,
				0,
			)
			.unwrap();

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
					pool.insert(
						vec![i; 50],
						bv(vec![signer]),
						SENDER_A,
						dummy_auth().0,
						dummy_auth().1,
						0,
					)
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
					pool.insert(
						vec![i; 100],
						bv(vec![signer]),
						SENDER_A,
						dummy_auth().0,
						dummy_auth().1,
						0,
					)
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
		let hash = pool
			.insert(data.clone(), bv(signers), SENDER_A, dummy_auth().0, dummy_auth().1, 0)
			.unwrap();

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
	fn test_concurrent_duplicate_insert_preserves_files() {
		use std::{sync::Barrier, thread};

		// Two threads insert identical content concurrently. The race-loser must
		// not delete the winner's blob or evict the winner's meta row; the
		// winning hash must remain readable via claim().
		let (kp, signer) = test_recipient();
		let (pool, _dir) = make_pool(1024 * 1024, 100);
		let pool = Arc::new(pool);
		let data = vec![0xABu8; 4096];
		let barrier = Arc::new(Barrier::new(2));

		let handles: Vec<_> = (0..2)
			.map(|_| {
				let pool = pool.clone();
				let barrier = barrier.clone();
				let signer = signer.clone();
				let data = data.clone();
				thread::spawn(move || {
					barrier.wait();
					pool.insert(data, bv(vec![signer]), SENDER_A, dummy_auth().0, dummy_auth().1, 0)
				})
			})
			.collect();
		let results: Vec<_> = handles.into_iter().map(|h| h.join().unwrap()).collect();

		let oks: Vec<_> = results.iter().filter_map(|r| r.as_ref().ok()).collect();
		let dupes = results.iter().filter(|r| matches!(r, Err(HopError::DuplicateEntry))).count();
		assert_eq!(oks.len(), 1, "exactly one insert must win the race");
		assert_eq!(dupes, 1, "the other must report DuplicateEntry");

		let hash = *oks[0];
		let sig = sign_ed(&kp, HOP_CLAIM_CONTEXT, &hash);
		let claimed = pool.claim(&hash, &sig).expect("claim must succeed");
		assert_eq!(claimed, data);
	}

	#[test]
	fn test_concurrent_duplicate_insert_keeps_winner_meta_on_disk() {
		use std::{sync::Barrier, thread};

		// Same content, different senders. The race-loser's meta must not end
		// up in the parity-db column; otherwise restart recovery would silently
		// load it as canonical for the entry.
		let dir = TempDir::new().unwrap();
		let pool = Arc::new(
			HopDataPool::new(
				1024 * 1024,
				1024 * 1024,
				100,
				dir.path().to_path_buf(),
				RateLimitConfig::disabled(),
				HopMetrics::disabled(),
			)
			.unwrap(),
		);

		let signer_a = MultiSigner::Ed25519(ed25519::Pair::from_seed(&[11u8; 32]).public());
		let signer_b = MultiSigner::Ed25519(ed25519::Pair::from_seed(&[22u8; 32]).public());
		let auth_a = dummy_auth();
		let auth_b = {
			let pair = ed25519::Pair::from_seed(&[33u8; 32]);
			(MultiSigner::Ed25519(pair.public()), MultiSignature::Ed25519(pair.sign(b"x")))
		};
		let data = vec![0xCDu8; 4096];

		let barrier = Arc::new(Barrier::new(2));
		let (p1, d1, b1, s1, a1) =
			(pool.clone(), data.clone(), barrier.clone(), signer_a.clone(), auth_a.clone());
		let h1 = thread::spawn(move || {
			b1.wait();
			p1.insert(d1, bv(vec![s1]), SENDER_A, a1.0, a1.1, 0)
		});
		let (p2, d2, b2, s2, a2) =
			(pool.clone(), data.clone(), barrier.clone(), signer_b.clone(), auth_b.clone());
		let h2 = thread::spawn(move || {
			b2.wait();
			p2.insert(d2, bv(vec![s2]), SENDER_B, a2.0, a2.1, 0)
		});

		let r1 = h1.join().unwrap();
		let r2 = h2.join().unwrap();

		let (winner_hash, winner_sender_auth) = match (&r1, &r2) {
			(Ok(h), Err(HopError::DuplicateEntry)) => (*h, auth_a.0.clone()),
			(Err(HopError::DuplicateEntry), Ok(h)) => (*h, auth_b.0.clone()),
			other => panic!("expected exactly one winner and one DuplicateEntry, got {other:?}"),
		};

		// Simulate restart: drop the pool, reopen the same data dir so the new
		// pool reconstructs its caches from the parity-db meta column.
		drop(pool);
		let pool2 = HopDataPool::new(
			1024 * 1024,
			1024 * 1024,
			100,
			dir.path().to_path_buf(),
			RateLimitConfig::disabled(),
			HopMetrics::disabled(),
		)
		.unwrap();

		let (_data, recovered_auth_signer, _sig, _ts) =
			pool2.get_with_auth(&winner_hash).expect("winner's entry must survive restart");
		assert_eq!(
			recovered_auth_signer, winner_sender_auth,
			"meta in parity-db diverged from the winning insert; loser's meta overwrote the winner's",
		);
	}

	#[test]
	fn test_saturating_release_concurrent_no_underflow() {
		use std::{sync::Barrier, thread};

		// Many threads each release a fixed amount that sums to exactly the
		// initial counter. With a non-atomic load-then-clamp-then-fetch_sub,
		// stale clamps would let the counter wrap to ~u64::MAX.
		// `saturating_release` must keep the result clamped at 0.
		const THREADS: u64 = 32;
		const RELEASE_PER_THREAD: u64 = 7;
		let counter = Arc::new(AtomicU64::new(THREADS * RELEASE_PER_THREAD));
		let barrier = Arc::new(Barrier::new(THREADS as usize));

		let handles: Vec<_> = (0..THREADS)
			.map(|_| {
				let counter = counter.clone();
				let barrier = barrier.clone();
				thread::spawn(move || {
					barrier.wait();
					saturating_release(&counter, RELEASE_PER_THREAD);
				})
			})
			.collect();
		for h in handles {
			h.join().unwrap();
		}

		assert_eq!(counter.load(Ordering::Relaxed), 0, "counter underflowed or did not reach zero");

		// Releasing more than the remaining balance must clamp to 0, never wrap.
		saturating_release(&counter, u64::MAX);
		assert_eq!(counter.load(Ordering::Relaxed), 0);
	}

	#[test]
	fn test_get_promotable_within_buffer() {
		// retention=3600s; a freshly-inserted entry is in the promotion window only
		// if the buffer is at least as large as the time-to-expiry.
		let (pool, _dir) = make_pool(1024 * 1024, 3600);
		let (_, signer) = test_recipient();

		let hash = pool
			.insert(vec![1, 2, 3], bv(vec![signer]), SENDER_A, dummy_auth().0, dummy_auth().1, 0)
			.unwrap();

		// Small buffer (180s ≪ 3600s retention): not promotable yet.
		let promotable = pool.get_promotable(50, 180, usize::MAX);
		assert!(promotable.is_empty());

		// Large buffer (6000s > 3600s retention): within the window.
		let promotable = pool.get_promotable(0, 6000, usize::MAX);
		assert_eq!(promotable.len(), 1);
		assert_eq!(promotable[0], hash);
	}

	#[test]
	fn test_get_promotable_excludes_promoted() {
		let (pool, _dir) = make_pool(1024 * 1024, 100);
		let (_, signer) = test_recipient();

		let hash = pool
			.insert(vec![1, 2, 3], bv(vec![signer]), SENDER_A, dummy_auth().0, dummy_auth().1, 0)
			.unwrap();

		let promotable = pool.get_promotable(80, 180, usize::MAX);
		assert_eq!(promotable.len(), 1);

		pool.mark_promoted(&hash);

		let promotable = pool.get_promotable(80, 180, usize::MAX);
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
				HopMetrics::disabled(),
			)
			.unwrap();
			hash = pool
				.insert(
					vec![42u8; 10],
					bv(vec![signer]),
					SENDER_A,
					dummy_auth().0,
					dummy_auth().1,
					0,
				)
				.unwrap();
			pool.mark_promoted(&hash);
		}

		{
			let pool = HopDataPool::new(
				1024 * 1024,
				1024 * 1024,
				100,
				dir.path().to_path_buf(),
				RateLimitConfig::disabled(),
				HopMetrics::disabled(),
			)
			.unwrap();
			let promotable = pool.get_promotable(80, 180, usize::MAX);
			assert!(promotable.is_empty(), "promoted entry should not be promotable after restart");
			assert!(pool.has(&hash), "entry should still exist");
		}
	}

	#[test]
	fn test_cleanup_expired_removes_promoted() {
		let (pool, _dir) = make_pool(1024 * 1024, 0);
		let (_, signer) = test_recipient();

		let hash = pool
			.insert(vec![1, 2, 3], bv(vec![signer]), SENDER_A, dummy_auth().0, dummy_auth().1, 0)
			.unwrap();
		pool.mark_promoted(&hash);
		assert!(pool.has(&hash));

		let freed = pool.cleanup_expired(0);
		assert!(freed > 0);
		assert!(!pool.has(&hash));
	}

	#[test]
	fn test_rate_limit_rejects_burst_overflow() {
		let dir = TempDir::new().unwrap();
		// submit_burst=2 so the 3rd request is rate-limited by submit count.
		// Bandwidth is sized comfortably above the 3-byte test payloads so the
		// rejection comes from the request bucket, not the bandwidth bucket.
		let cfg = RateLimitConfig {
			enabled: true,
			submit_rate_per_min: 60,
			submit_burst: 2,
			bandwidth_per_min: 1024 * 1024 * 60,
			bandwidth_burst: 1024 * 1024,
		};
		let pool = HopDataPool::new(
			1024 * 1024,
			1024 * 1024,
			100,
			dir.path().to_path_buf(),
			cfg,
			HopMetrics::disabled(),
		)
		.unwrap();
		let (_, signer) = test_recipient();

		pool.insert(
			vec![1, 2, 3],
			bv(vec![signer.clone()]),
			SENDER_A,
			dummy_auth().0,
			dummy_auth().1,
			0,
		)
		.unwrap();
		pool.insert(
			vec![4, 5, 6],
			bv(vec![signer.clone()]),
			SENDER_A,
			dummy_auth().0,
			dummy_auth().1,
			0,
		)
		.unwrap();
		assert!(matches!(
			pool.insert(
				vec![7, 8, 9],
				bv(vec![signer]),
				SENDER_A,
				dummy_auth().0,
				dummy_auth().1,
				0,
			),
			Err(HopError::RateLimited { .. })
		));
	}

	#[test]
	fn test_meta_version_mismatch_rejected() {
		// Persist a HopEntryMeta with version 0 (an older / future schema) into
		// the meta column, write its matching blob, then boot a fresh pool and
		// assert the row is wiped, the blob is reaped, and the entry never
		// surfaces.
		let dir = TempDir::new().unwrap();

		// Boot once to create the parity-db layout.
		{
			let _pool = HopDataPool::new(
				1024 * 1024,
				1024 * 1024,
				100,
				dir.path().to_path_buf(),
				RateLimitConfig::disabled(),
				HopMetrics::disabled(),
			)
			.unwrap();
		}

		let (_, signer) = test_recipient();
		let recipients = bv(vec![signer.clone()]);
		let mut meta =
			HopEntryMeta::new(100, 0, recipients, SENDER_A, dummy_auth().0, dummy_auth().1, 0);
		meta.version = 0;

		let fake_hash = H256([0xeeu8; 32]);
		let blob_shard = dir.path().join(BLOBS_DIR).join("ee");
		fs::create_dir_all(&blob_shard).unwrap();
		let blob_path = blob_shard.join(format!("{}.blob", hex::encode(fake_hash)));
		fs::write(&blob_path, b"x").unwrap();

		{
			let db_path = dir.path().join(META_DB_DIR);
			let db = parity_db::Db::open_or_create(&HopDataPool::db_options(&db_path)).unwrap();
			db.commit([(COL_META, fake_hash.as_bytes().to_vec(), Some(meta.encode()))])
				.unwrap();
		}

		let pool = HopDataPool::new(
			1024 * 1024,
			1024 * 1024,
			100,
			dir.path().to_path_buf(),
			RateLimitConfig::disabled(),
			HopMetrics::disabled(),
		)
		.unwrap();
		assert!(!pool.has(&fake_hash), "stale-version row should be dropped");
		assert!(!blob_path.exists(), "matching .blob should also be removed");
		assert_eq!(pool.status().entry_count, 0);
	}

	#[test]
	fn test_meta_row_without_blob_dropped_on_recovery() {
		// A crash can persist a blob unlink but lose the async parity-db
		// meta-delete, leaving a valid current-version meta row whose blob is
		// gone. Boot a pool with exactly such a row and assert it is dropped and
		// its pool + user quota is not accounted, rather than leaking until
		// expiry.
		let dir = TempDir::new().unwrap();

		// Boot once to create the parity-db layout.
		{
			let _pool = HopDataPool::new(
				1024 * 1024,
				1024 * 1024,
				100,
				dir.path().to_path_buf(),
				RateLimitConfig::disabled(),
				HopMetrics::disabled(),
			)
			.unwrap();
		}

		let (_, signer) = test_recipient();
		let recipients = bv(vec![signer]);
		// Current-version meta (as `HopEntryMeta::new` produces), so only the
		// missing-blob check can reject it.
		let meta =
			HopEntryMeta::new(100, 0, recipients, SENDER_A, dummy_auth().0, dummy_auth().1, 0);
		assert_eq!(meta.version, HOP_META_VERSION);

		// Commit the meta row but deliberately write no blob.
		let fake_hash = H256([0xabu8; 32]);
		{
			let db_path = dir.path().join(META_DB_DIR);
			let mut opts = parity_db::Options::with_columns(&db_path, COL_COUNT);
			opts.columns[COL_META as usize].btree_index = true;
			let db = parity_db::Db::open_or_create(&opts).unwrap();
			db.commit([(COL_META, fake_hash.as_bytes().to_vec(), Some(meta.encode()))])
				.unwrap();
		}

		let pool = HopDataPool::new(
			1024 * 1024,
			1024 * 1024,
			100,
			dir.path().to_path_buf(),
			RateLimitConfig::disabled(),
			HopMetrics::disabled(),
		)
		.unwrap();

		assert!(!pool.has(&fake_hash), "meta row with no blob should be dropped");
		assert_eq!(pool.status().entry_count, 0, "dropped row must not count toward entries");
		assert_eq!(pool.status().total_bytes, 0, "dropped row must not hold pool quota");
		// The meta row is actually gone from the DB, not just uncounted.
		assert!(matches!(pool.fetch_meta(&fake_hash), Ok(None)));
	}

	#[test]
	fn test_metrics_track_insert_promotion_and_expiry() {
		// Registered metrics so counters read back (disabled() metrics are no-ops).
		let dir = TempDir::new().unwrap();
		let registry = prometheus_endpoint::Registry::new();
		let pool = HopDataPool::new(
			1024 * 1024,
			1024 * 1024,
			/* retention = */ 0, // entries expire immediately
			dir.path().to_path_buf(),
			RateLimitConfig::disabled(),
			HopMetrics::new(Some(&registry)).unwrap(),
		)
		.unwrap();
		let (_, signer) = test_recipient();

		// Insert publishes the size gauges.
		let hash = pool
			.insert(vec![1u8; 50], bv(vec![signer.clone()]), SENDER_A, dummy_auth().0, dummy_auth().1, 0)
			.unwrap();
		assert_eq!(pool.metrics().pool_gauges(), (1, acct(50, 1)));

		// Promotion is counted once, on the false->true transition.
		pool.mark_promoted(&hash);
		pool.mark_promoted(&hash);
		assert_eq!(pool.metrics().promotions_confirmed(), 1);

		// Expiring a promoted entry counts as EXPIRED_PROMOTED and clears gauges.
		pool.cleanup_expired(0);
		assert_eq!(pool.metrics().removed_count(removal_reasons::EXPIRED_PROMOTED), 1);
		assert_eq!(pool.metrics().pool_gauges(), (0, 0));

		// An unpromoted entry expiring is the data-loss case: EXPIRED_UNPROMOTED.
		pool.insert(vec![2u8; 30], bv(vec![signer]), SENDER_A, dummy_auth().0, dummy_auth().1, 0)
			.unwrap();
		pool.cleanup_expired(0);
		assert_eq!(pool.metrics().removed_count(removal_reasons::EXPIRED_UNPROMOTED), 1);
		assert_eq!(pool.metrics().pool_gauges(), (0, 0));
	}

	#[test]
	fn test_metrics_record_startup_dropped() {
		// Boot once to lay down the DB, commit a meta row with no sibling blob,
		// then reopen with registered metrics and assert the dropped row is
		// counted under STARTUP_DROPPED.
		let dir = TempDir::new().unwrap();
		{
			let _pool = HopDataPool::new(
				1024 * 1024,
				1024 * 1024,
				100,
				dir.path().to_path_buf(),
				RateLimitConfig::disabled(),
				HopMetrics::disabled(),
			)
			.unwrap();
		}

		let (_, signer) = test_recipient();
		let meta =
			HopEntryMeta::new(100, 0, bv(vec![signer]), SENDER_A, dummy_auth().0, dummy_auth().1, 0);
		let fake_hash = H256([0xcdu8; 32]);
		{
			let db_path = dir.path().join(META_DB_DIR);
			let mut opts = parity_db::Options::with_columns(&db_path, COL_COUNT);
			opts.columns[COL_META as usize].btree_index = true;
			let db = parity_db::Db::open_or_create(&opts).unwrap();
			db.commit([(COL_META, fake_hash.as_bytes().to_vec(), Some(meta.encode()))]).unwrap();
		}

		let registry = prometheus_endpoint::Registry::new();
		let pool = HopDataPool::new(
			1024 * 1024,
			1024 * 1024,
			100,
			dir.path().to_path_buf(),
			RateLimitConfig::disabled(),
			HopMetrics::new(Some(&registry)).unwrap(),
		)
		.unwrap();

		assert_eq!(pool.metrics().removed_count(removal_reasons::STARTUP_DROPPED), 1);
		assert_eq!(pool.metrics().pool_gauges(), (0, 0));
	}

	#[test]
	fn test_promotion_backoff_skips_until_due_then_gives_up() {
		use crate::types::MAX_PROMOTION_ATTEMPTS;

		let (pool, _dir) = make_pool(1024 * 1024, /* retention = */ 100);
		let (_, signer) = test_recipient();
		let hash = pool
			.insert(vec![1u8; 100], bv(vec![signer]), SENDER_A, dummy_auth().0, dummy_auth().1, 0)
			.unwrap();

		// Inside the buffer window (>= retention=100s) so the entry is promotable
		// in principle.
		let buffer = 300_u64;
		let current = 60;
		assert_eq!(pool.get_promotable(current, buffer, 10), vec![hash]);

		// First failure schedules next attempt at current + 1× check_interval_blocks.
		let check_interval_blocks: u32 = 10;
		pool.record_promotion_attempt(&hash, current, check_interval_blocks);
		assert!(
			pool.get_promotable(current, buffer, 10).is_empty(),
			"entry should be skipped until back-off elapses"
		);
		assert_eq!(pool.get_promotable(current + 10, buffer, 10), vec![hash]);

		// Burn through the remaining attempts; once at MAX, the entry stays out
		// of the promotable set forever (regardless of how far we advance time).
		// Schedule after first failure: 1×, 2×, 4×, 8×, 16× check_interval.
		let mut now = current + 10;
		for next_attempt in 2..=MAX_PROMOTION_ATTEMPTS {
			pool.record_promotion_attempt(&hash, now, check_interval_blocks);
			let shift = (next_attempt - 1).min(5);
			let backoff = check_interval_blocks << shift;
			now += backoff;
		}
		assert!(
			pool.get_promotable(now + 10_000, buffer, 10).is_empty(),
			"entry should give up after MAX_PROMOTION_ATTEMPTS"
		);
	}

	#[test]
	fn test_legacy_meta_files_migrated() {
		// The upgrade path: a data dir holding only the pre-KV-store layout, no
		// `meta-db/` at all. Both entries must survive the first boot rather
		// than being reaped as orphans.
		let dir = TempDir::new().unwrap();
		let data_a = vec![1u8; 100];
		let data_b = vec![2u8; 250];
		let hash_a =
			write_legacy_entry(dir.path(), &data_a, &legacy_meta(&data_a, SENDER_A).encode());
		let hash_b =
			write_legacy_entry(dir.path(), &data_b, &legacy_meta(&data_b, SENDER_B).encode());

		let pool = HopDataPool::new(
			1024 * 1024,
			1024 * 1024,
			100,
			dir.path().to_path_buf(),
			RateLimitConfig::disabled(),
			HopMetrics::disabled(),
		)
		.unwrap();

		assert!(pool.has(&hash_a));
		assert!(pool.has(&hash_b));
		assert_eq!(pool.get(&hash_a).unwrap(), data_a);
		assert_eq!(pool.get(&hash_b).unwrap(), data_b);
		assert_eq!(pool.status().entry_count, 2);
		assert_eq!(pool.status().total_bytes, acct(100, 1) + acct(250, 1));
		assert_eq!(user_usage(&pool, &SENDER_A), acct(100, 1));
		assert_eq!(user_usage(&pool, &SENDER_B), acct(250, 1));
		// The legacy tree is reclaimed, not left to leak on disk.
		assert!(!dir.path().join(LEGACY_META_DIR).exists());
	}

	#[test]
	fn test_legacy_meta_survives_second_restart() {
		// Restart durability must hold across the upgrade boundary, not just for
		// the boot that performed the import.
		let dir = TempDir::new().unwrap();
		let data = vec![7u8; 64];
		let hash = write_legacy_entry(dir.path(), &data, &legacy_meta(&data, SENDER_A).encode());

		for _ in 0..2 {
			let pool = HopDataPool::new(
				1024 * 1024,
				1024 * 1024,
				100,
				dir.path().to_path_buf(),
				RateLimitConfig::disabled(),
				HopMetrics::disabled(),
			)
			.unwrap();
			assert!(pool.has(&hash));
			assert_eq!(pool.get(&hash).unwrap(), data);
			assert_eq!(pool.status().entry_count, 1);
			assert_eq!(pool.status().total_bytes, acct(64, 1));
		}
	}

	#[test]
	fn test_legacy_meta_without_blob_skipped() {
		let dir = TempDir::new().unwrap();
		let orphan_hash = H256([0x5au8; 32]);
		write_legacy_meta_only(dir.path(), &orphan_hash, &legacy_meta(&[], SENDER_A).encode());

		let pool = HopDataPool::new(
			1024 * 1024,
			1024 * 1024,
			100,
			dir.path().to_path_buf(),
			RateLimitConfig::disabled(),
			HopMetrics::disabled(),
		)
		.unwrap();

		assert!(!pool.has(&orphan_hash));
		assert_eq!(pool.status().entry_count, 0);
		assert!(!dir.path().join(LEGACY_META_DIR).exists());
	}

	#[test]
	fn test_legacy_meta_version_mismatch_dropped() {
		// An unsupported record version is not imported, and the blob it points
		// at is then reaped by the normal orphan pass.
		let dir = TempDir::new().unwrap();
		let data = vec![3u8; 32];
		let mut meta = legacy_meta(&data, SENDER_A);
		meta.version = 0;
		let hash = write_legacy_entry(dir.path(), &data, &meta.encode());
		let blob_path = HopDataPool::entry_path(dir.path(), &hash, BLOBS_DIR, BLOB_EXT);
		assert!(blob_path.exists());

		let pool = HopDataPool::new(
			1024 * 1024,
			1024 * 1024,
			100,
			dir.path().to_path_buf(),
			RateLimitConfig::disabled(),
			HopMetrics::disabled(),
		)
		.unwrap();

		assert!(!pool.has(&hash));
		assert_eq!(pool.status().entry_count, 0);
		assert!(!blob_path.exists());
		assert!(!dir.path().join(LEGACY_META_DIR).exists());
	}

	#[test]
	fn test_legacy_meta_corrupt_file_skipped() {
		// An undecodable `.meta` file must not abort startup.
		let dir = TempDir::new().unwrap();
		let data = vec![9u8; 16];
		let hash = write_legacy_entry(dir.path(), &data, b"not valid SCALE data");

		let pool = HopDataPool::new(
			1024 * 1024,
			1024 * 1024,
			100,
			dir.path().to_path_buf(),
			RateLimitConfig::disabled(),
			HopMetrics::disabled(),
		)
		.unwrap();

		assert!(!pool.has(&hash));
		assert_eq!(pool.status().entry_count, 0);
		assert!(!dir.path().join(LEGACY_META_DIR).exists());
	}

	#[test]
	fn test_legacy_meta_does_not_clobber_existing_row() {
		// A row already in the KV store is newer than any leftover `.meta` file, so
		// the import must not overwrite it.
		let dir = TempDir::new().unwrap();
		let data = vec![4u8; 48];
		let (_, signer) = test_recipient();

		let hash;
		let live_expires_at;
		{
			let pool = HopDataPool::new(
				1024 * 1024,
				1024 * 1024,
				100,
				dir.path().to_path_buf(),
				RateLimitConfig::disabled(),
				HopMetrics::disabled(),
			)
			.unwrap();
			hash = pool
				.insert(data.clone(), bv(vec![signer]), SENDER_A, dummy_auth().0, dummy_auth().1, 0)
				.unwrap();
			live_expires_at = pool.fetch_meta(&hash).unwrap().unwrap().expires_at;
		}

		// Same hash, deliberately different expiry, written as a legacy `.meta` file.
		let mut stale = legacy_meta(&data, SENDER_A);
		stale.expires_at = live_expires_at.wrapping_add(999_999);
		write_legacy_meta_only(dir.path(), &hash, &stale.encode());

		let pool = HopDataPool::new(
			1024 * 1024,
			1024 * 1024,
			100,
			dir.path().to_path_buf(),
			RateLimitConfig::disabled(),
			HopMetrics::disabled(),
		)
		.unwrap();

		assert!(pool.has(&hash));
		assert_eq!(pool.status().entry_count, 1);
		assert_eq!(pool.fetch_meta(&hash).unwrap().unwrap().expires_at, live_expires_at);
		assert!(!dir.path().join(LEGACY_META_DIR).exists());
	}

	#[test]
	fn test_legacy_empty_meta_dir_removed() {
		// What an upgraded but idle node has on disk: 256 empty shard dirs. The
		// boot must be uneventful and the tree reclaimed anyway.
		let dir = TempDir::new().unwrap();
		for i in 0..SHARD_COUNT {
			fs::create_dir_all(dir.path().join(LEGACY_META_DIR).join(format!("{:02x}", i as u8)))
				.unwrap();
		}

		let pool = HopDataPool::new(
			1024 * 1024,
			1024 * 1024,
			100,
			dir.path().to_path_buf(),
			RateLimitConfig::disabled(),
			HopMetrics::disabled(),
		)
		.unwrap();

		assert_eq!(pool.status().entry_count, 0);
		assert!(!dir.path().join(LEGACY_META_DIR).exists());
	}

	#[test]
	fn test_db_version_stamped_on_fresh_pool() {
		let dir = TempDir::new().unwrap();
		{
			let _pool = HopDataPool::new(
				1024 * 1024,
				1024 * 1024,
				100,
				dir.path().to_path_buf(),
				RateLimitConfig::disabled(),
				HopMetrics::disabled(),
			)
			.unwrap();
		}
		assert_eq!(read_db_version(dir.path()), Some(CURRENT_DB_VERSION));
	}

	#[test]
	fn test_future_db_version_rejected() {
		// A database written by a newer binary must be refused, not misread.
		let dir = TempDir::new().unwrap();
		{
			let _pool = HopDataPool::new(
				1024 * 1024,
				1024 * 1024,
				100,
				dir.path().to_path_buf(),
				RateLimitConfig::disabled(),
				HopMetrics::disabled(),
			)
			.unwrap();
		}
		{
			let db = parity_db::Db::open_or_create(&HopDataPool::db_options(
				&dir.path().join(META_DB_DIR),
			))
			.unwrap();
			db.commit([(
				COL_DB_META,
				KEY_DB_VERSION.to_vec(),
				Some((CURRENT_DB_VERSION + 1).to_le_bytes().to_vec()),
			)])
			.unwrap();
		}

		let result = HopDataPool::new(
			1024 * 1024,
			1024 * 1024,
			100,
			dir.path().to_path_buf(),
			RateLimitConfig::disabled(),
			HopMetrics::disabled(),
		);
		assert!(matches!(result, Err(HopError::Db(_))), "future db version must be rejected");
	}

	#[test]
	fn test_column_layout_migration() {
		// A database laid down with the previous single-column layout must be
		// extended in place, with its rows intact.
		let dir = TempDir::new().unwrap();
		let data = vec![8u8; 80];
		let hash = H256(blake2_256(&data));
		let blob_path = HopDataPool::entry_path(dir.path(), &hash, BLOBS_DIR, BLOB_EXT);
		fs::create_dir_all(blob_path.parent().unwrap()).unwrap();
		fs::write(&blob_path, &data).unwrap();

		let db_path = dir.path().join(META_DB_DIR);
		fs::create_dir_all(&db_path).unwrap();
		{
			let mut options = parity_db::Options::with_columns(&db_path, 1);
			options.columns[COL_META as usize].btree_index = true;
			let db = parity_db::Db::open_or_create(&options).unwrap();
			db.commit([(
				COL_META,
				hash.as_bytes().to_vec(),
				Some(legacy_meta(&data, SENDER_A).encode()),
			)])
			.unwrap();
		}

		let pool = HopDataPool::new(
			1024 * 1024,
			1024 * 1024,
			100,
			dir.path().to_path_buf(),
			RateLimitConfig::disabled(),
			HopMetrics::disabled(),
		)
		.unwrap();

		assert!(pool.has(&hash));
		assert_eq!(pool.get(&hash).unwrap(), data);
		assert_eq!(pool.status().entry_count, 1);
		drop(pool);
		assert_eq!(read_db_version(dir.path()), Some(CURRENT_DB_VERSION));
	}
}
