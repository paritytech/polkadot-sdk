// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Polkadot.

// Polkadot is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Polkadot is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Polkadot.  If not, see <http://www.gnu.org/licenses/>.

//! Disk-backed reputation database for collator protocol.

use crate::{
	validator_side_experimental::{
		common::Score,
		peer_manager::{
			backend::Backend,
			db::{Db, ScoreEntry},
			persistence::{
				decode_para_key, metadata_key, para_reputation_key, PersistenceError,
				StoredMetadata, StoredParaReputations, REPUTATION_PARA_PREFIX,
			},
			ReputationUpdate,
		},
		ReputationConfig,
	},
	LOG_TARGET,
};
use async_trait::async_trait;
use codec::{Decode, Encode};
use futures::Future;
use polkadot_node_network_protocol::PeerId;
use polkadot_node_subsystem_util::database::{DBTransaction, Database};
use polkadot_primitives::{BlockNumber, Id as ParaId};
use std::{
	collections::{BTreeMap, BTreeSet, HashMap},
	pin::Pin,
	sync::{
		atomic::{AtomicBool, Ordering},
		Arc,
	},
};
use tokio::sync::mpsc;

#[derive(Debug)]
pub enum LogInfo {
	Periodic {
		total_entries: usize,
		para_count: usize,
		dirty_para_count: usize,
		last_finalized: Option<BlockNumber>,
	},
	Pruned {
		pruned_count: usize,
		remaining_count: usize,
		registered_count: usize,
	},
	Slash {
		para_id: ParaId,
		peer_id: PeerId,
		value: Score,
	},
}

impl LogInfo {
	fn log(&self) {
		match self {
			LogInfo::Periodic { total_entries, para_count, dirty_para_count, last_finalized } => {
				gum::debug!(
					target: LOG_TARGET,
					total_peer_entries = total_entries,
					para_count,
					dirty_para_count,
					?last_finalized,
					"Periodic persistence completed: reputation DB written to disk"
				);
			},
			LogInfo::Pruned { pruned_count, remaining_count, registered_count } => {
				gum::debug!(
					target: LOG_TARGET,
					pruned_para_count = pruned_count,
					remaining_para_count = remaining_count,
					registered_para_count = registered_count,
					"Prune paras persisted to disk immediately"
				);
			},
			LogInfo::Slash { para_id, peer_id, value } => {
				gum::debug!(
					target: LOG_TARGET,
					?para_id,
					?peer_id,
					slash_value = ?value,
					"Slash persisted to disk immediately"
				);
			},
		}
	}
}

/// Request sent to the background writer task
struct PersistenceRequest {
	updates: Vec<(ParaId, Option<StoredParaReputations>)>,
	metadata: StoredMetadata,
	is_full_dump: bool,
	log_info: Option<LogInfo>,
}

pub type WriterFuture = Pin<Box<dyn Future<Output = ()> + Send + 'static>>;

/// Persistent database implementation for collator reputation.
///
/// This wraps the in-memory `Db` and adds disk persistence capability.
///
/// **Persistence Policy:**
/// - All operations (bumps, decays, queries) happen in-memory only
/// - Disk writes happen:
///   1. On slash operations (immediate, for security)
///   2. When `persist()` is called explicitly by the main loop (periodic timer)
///   3. On paras pruning (immediate)
///
/// - Only modified paras are persisted during periodic persistence
/// - Paras are marked dirty when `process_bumps` modifies their reputation (bumps/decays)
///
/// The main loop is responsible for calling `persist()` periodically (currently, every 10 minutes).
pub struct PersistentDb {
	/// In-memory database (does all the actual logic).
	inner: Db,
	/// Disk database handle.
	disk_db: Arc<dyn Database>,
	/// Column configuration.
	config: ReputationConfig,
	/// Paras whose reputation has changed since last persistence.
	dirty_paras: BTreeSet<ParaId>,
	/// Channel to send updates to the background writer.
	background_tx: mpsc::Sender<PersistenceRequest>,
	/// Shared flag indicating if the disk state is desynchronized
	needs_full_dump: Arc<AtomicBool>,
}

impl PersistentDb {
	/// Create a new persistent DB, loading existing state from disk.
	pub async fn new(
		disk_db: Arc<dyn Database>,
		config: ReputationConfig,
		stored_limit_per_para: u16,
	) -> Result<(Self, WriterFuture), PersistenceError> {
		// Create empty in-memory DB
		let inner = Db::new(stored_limit_per_para).await;

		let (tx, rx) = mpsc::channel(16);
		let needs_full_dump = Arc::new(AtomicBool::new(false));
		// Load data from disk into the in-memory DB
		let mut instance = Self {
			inner,
			disk_db: disk_db.clone(),
			config,
			dirty_paras: BTreeSet::new(),
			background_tx: tx,
			needs_full_dump: needs_full_dump.clone(),
		};
		let (para_count, total_entries) = instance.load_from_disk().await?;

		let last_finalized = instance.inner.processed_finalized_block_number().await;

		gum::info!(
			target: LOG_TARGET,
			?last_finalized,
			para_count,
			total_peer_entries = total_entries,
			"Reputation DB initialized"
		);

		let task = Box::pin(Self::run_background_writer(disk_db, config, rx, needs_full_dump));

		Ok((instance, task))
	}

	async fn run_background_writer(
		disk_db: Arc<dyn Database>,
		config: ReputationConfig,
		mut rx: mpsc::Receiver<PersistenceRequest>,
		needs_full_dump: Arc<AtomicBool>,
	) {
		while let Some(req) = rx.recv().await {
			let mut db_transaction = DBTransaction::new();

			// Write metadata
			db_transaction.put_vec(
				config.col_reputation_data,
				metadata_key(),
				req.metadata.encode(),
			);

			// Write updates
			for (para_id, maybe_data) in req.updates {
				let key = para_reputation_key(para_id);
				match maybe_data {
					Some(stored_para_rep) => db_transaction.put(
						config.col_reputation_data,
						&key,
						&stored_para_rep.encode(),
					),
					None => db_transaction.delete(config.col_reputation_data, &key),
				}
			}

			// Commit transaction to disk
			match disk_db.write(db_transaction) {
				Ok(_) => {
					if req.is_full_dump {
						// We can clean the full_dump flag now
						needs_full_dump.store(false, Ordering::Release);
					}
					if let Some(info) = req.log_info {
						info.log();
					}
				},
				Err(e) => {
					gum::warn!(
						target: LOG_TARGET,
						error = ?e,
						"Background persistence write failed. Marking for full dump."
					);
					needs_full_dump.store(true, Ordering::Release);
				},
			}
		}
		gum::debug!(target: LOG_TARGET, "Background reputation writer shutting down");
	}

	/// Load all data from disk into the in-memory DB.
	/// Returns (para_count, total_entries) for logging purposes.
	async fn load_from_disk(&mut self) -> Result<(usize, usize), PersistenceError> {
		gum::trace!(
			target: LOG_TARGET,
			"Starting to load reputation data from disk"
		);

		// Load metadata
		if let Some(meta) = self.load_metadata()? {
			self.inner.set_last_finalized(meta.last_finalized);
			gum::debug!(
				target: LOG_TARGET,
				last_finalized = ?meta.last_finalized,
				"Loaded reputation DB metadata from disk"
			);
		} else {
			gum::debug!(
				target: LOG_TARGET,
				"No existing reputation metadata found on disk (fresh start)"
			);
		}

		// Load all para reputations
		let iter = self
			.disk_db
			.iter_with_prefix(self.config.col_reputation_data, REPUTATION_PARA_PREFIX);

		let mut total_entries = 0;
		let mut para_count = 0;
		for result in iter {
			let (key, value) = result.map_err(PersistenceError::Io)?;
			if let Some(para_id) = decode_para_key(&key) {
				let stored: StoredParaReputations =
					Decode::decode(&mut &value[..]).map_err(PersistenceError::Codec)?;
				let entries: HashMap<PeerId, ScoreEntry> = stored.into();
				let entry_count = entries.len();
				total_entries += entry_count;
				para_count += 1;
				gum::trace!(
					target: LOG_TARGET,
					?para_id,
					peer_count = entry_count,
					"Loaded reputation entries for para from disk"
				);
				self.inner.set_para_reputations(para_id, entries);
			}
		}

		gum::debug!(
			target: LOG_TARGET,
			total_peer_entries = total_entries,
			para_count,
			"Completed loading reputation data from disk"
		);

		Ok((para_count, total_entries))
	}

	/// Load metadata from disk.
	fn load_metadata(&self) -> Result<Option<StoredMetadata>, PersistenceError> {
		match self.disk_db.get(self.config.col_reputation_data, metadata_key())? {
			None => Ok(None),
			Some(raw) => {
				StoredMetadata::decode(&mut &raw[..]).map(Some).map_err(PersistenceError::Codec)
			},
		}
	}

	/// Queue a snapshot of the dirty data to the background writer.
	pub fn persist_async(&mut self, log_info: Option<LogInfo>) {
		let needs_full = self.needs_full_dump.load(Ordering::Acquire);
		let mut updates = Vec::new();
		let mut stats_total_entries = 0;
		let mut stats_para_count = 0;

		let paras_to_snapshot: Vec<ParaId> = if needs_full {
			self.inner.all_reputations().map(|(id, _)| *id).collect()
		} else {
			self.dirty_paras.iter().cloned().collect()
		};

		for para_id in paras_to_snapshot {
			let peer_scores = self.inner.get_para_reputations(&para_id);
			if peer_scores.is_empty() {
				updates.push((para_id, None));
			} else {
				let stored: StoredParaReputations = peer_scores.into();
				if matches!(log_info, Some(LogInfo::Periodic { .. })) {
					stats_total_entries += stored.entries.len();
					stats_para_count += 1;
				}

				updates.push((para_id, Some(stored)));
			}
		}

		// Get the real finalized block from the DB
		let current_finalized = self.inner.get_last_finalized();
		let final_log_info = match log_info {
			Some(LogInfo::Periodic { dirty_para_count, .. }) => Some(LogInfo::Periodic {
				total_entries: stats_total_entries,
				para_count: stats_para_count,
				dirty_para_count,
				last_finalized: current_finalized,
			}),
			other => other,
		};

		let request = PersistenceRequest {
			updates,
			metadata: StoredMetadata { last_finalized: self.inner.get_last_finalized() },
			is_full_dump: needs_full,
			log_info: final_log_info,
		};

		match self.background_tx.try_send(request) {
			Ok(_) => {
				// On success, we assume the data is handed off.
				self.dirty_paras.clear();
			},
			Err(mpsc::error::TrySendError::Full(_)) => {
				gum::warn!(
					target: LOG_TARGET,
					"Reputation persistence channel full. Modifications kept in memory for next retry."
				);
				// We do NOT clear dirty_paras.
			},
			Err(mpsc::error::TrySendError::Closed(_)) => {
				gum::error!(
					target: LOG_TARGET,
					"Reputation persistence channel closed unexpectedly."
				);
			},
		}
	}

	/// Synchronous persist for shutdown
	pub fn persist(&mut self) -> Result<(), PersistenceError> {
		let mut tx = DBTransaction::new();

		// Write metadata
		let meta = StoredMetadata { last_finalized: self.inner.get_last_finalized() };
		tx.put_vec(self.config.col_reputation_data, metadata_key(), meta.encode());

		let needs_full = self.needs_full_dump.load(Ordering::Acquire);

		let paras_to_write: Vec<ParaId> = if needs_full {
			self.inner.all_reputations().map(|(id, _)| *id).collect()
		} else {
			self.dirty_paras.iter().cloned().collect()
		};

		// Write dirty para data
		let mut total_entries = 0;
		let mut para_count = 0;

		for para_id in paras_to_write {
			let key = para_reputation_key(para_id);
			let peer_scores = self.inner.get_para_reputations(&para_id);
			if peer_scores.is_empty() {
				tx.delete(self.config.col_reputation_data, &key);
			} else {
				let stored: StoredParaReputations = (peer_scores).into();
				tx.put_vec(self.config.col_reputation_data, &key, stored.encode());
				total_entries += stored.entries.len();
				para_count += 1;
			}
		}

		self.disk_db.write(tx).map_err(PersistenceError::Io)?;

		gum::debug!(
			target: LOG_TARGET,
			total_peer_entries = total_entries,
			para_count,
			dirty_para_count = self.dirty_paras.len(),
			last_finalized = ?meta.last_finalized,
			"Periodic persistence completed: reputation DB written to disk"
		);

		// Clear dirty set only after we successfully committed
		self.dirty_paras.clear();

		Ok(())
	}

	/// Helper to get dirty paras count without exposing the set.
	pub fn dirty_paras_count(&self) -> usize {
		self.dirty_paras.len()
	}
}

#[async_trait]
impl Backend for PersistentDb {
	async fn processed_finalized_block_number(&self) -> Option<BlockNumber> {
		self.inner.processed_finalized_block_number().await
	}

	async fn query(&self, peer_id: &PeerId, para_id: &ParaId) -> Option<Score> {
		self.inner.query(peer_id, para_id).await
	}

	async fn slash(&mut self, peer_id: &PeerId, para_id: &ParaId, value: Score) {
		// Delegate to inner DB
		self.inner.slash(peer_id, para_id, value).await;

		self.dirty_paras.insert(*para_id);

		// Immediately persist to disk after slash (security-critical)
		self.persist_async(Some(LogInfo::Slash { para_id: *para_id, peer_id: *peer_id, value }));
	}

	async fn prune_paras(&mut self, registered_paras: BTreeSet<ParaId>) {
		// Collects all paras that have reputations and are still registered
		let paras_to_prune: Vec<ParaId> = self
			.inner
			.all_reputations()
			.filter(|(para_id, _)| !registered_paras.contains(para_id))
			.map(|(para_id, _)| *para_id)
			.collect();

		let pruned_count = paras_to_prune.len();

		for para_id in &paras_to_prune {
			self.dirty_paras.insert(*para_id);
		}
		// Prune from in-memory state
		self.inner.prune_paras(registered_paras.clone()).await;
		let paras_after = self.inner.all_reputations().count();

		// Persist with explicit deletion of pruned paras
		let mut tx = DBTransaction::new();

		// Delete pruned paras from disk
		for para_id in &paras_to_prune {
			let key = para_reputation_key(*para_id);
			tx.delete(self.config.col_reputation_data, &key);
		}

		self.persist_async(Some(LogInfo::Pruned {
			pruned_count,
			remaining_count: paras_after,
			registered_count: registered_paras.len(),
		}));
	}

	async fn process_bumps(
		&mut self,
		leaf_number: BlockNumber,
		bumps: BTreeMap<ParaId, HashMap<PeerId, Score>>,
		decay_value: Option<Score>,
	) -> Vec<ReputationUpdate> {
		// Mark all paras in bumps as dirty.
		for para_id in bumps.keys() {
			self.dirty_paras.insert(*para_id);
		}

		// Delegate to inner DB - NO PERSISTENCE HERE
		// Persistence happens via the periodic timer calling persist()
		self.inner.process_bumps(leaf_number, bumps, decay_value).await
	}

	async fn max_scores_for_paras(&self, paras: BTreeSet<ParaId>) -> HashMap<ParaId, Score> {
		self.inner.max_scores_for_paras(paras).await
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use polkadot_node_subsystem_util::database::kvdb_impl::DbAdapter;
	use std::time::Duration;
	use tokio::time::sleep;

	const DATA_COL: u32 = 0;
	const NUM_COLUMNS: u32 = 1;

	fn make_db() -> Arc<dyn Database> {
		let db = kvdb_memorydb::create(NUM_COLUMNS);
		let db = DbAdapter::new(db, &[DATA_COL]);
		Arc::new(db)
	}

	fn make_config() -> ReputationConfig {
		ReputationConfig { col_reputation_data: DATA_COL, persist_interval: None }
	}

	/// Returns the DB handle and the JoinHandle for the background task.
	async fn create_and_spawn_db(
		disk_db: Arc<dyn Database>,
		config: ReputationConfig,
	) -> (PersistentDb, tokio::task::JoinHandle<()>) {
		let (db, task) =
			PersistentDb::new(disk_db, config, 100).await.expect("failed to create db");
		let handle = tokio::spawn(task);
		(db, handle)
	}

	#[tokio::test]
	async fn load_from_empty_disk_fresh_start() {
		// Test that PersistentDb can be created from an empty database (fresh start)
		let disk_db = make_db();
		let config = make_config();

		let (db, _) = PersistentDb::new(disk_db, config, 100).await.expect("should create db");

		// Fresh start should have no finalized block
		assert_eq!(db.processed_finalized_block_number().await, None);

		assert_eq!(db.inner.len(), 0);
	}

	#[tokio::test]
	async fn load_from_disk_with_existing_data() {
		// Test that PersistentDb correctly loads existing data from disk
		let disk_db = make_db();
		let config = make_config();

		let peer1 = PeerId::random();
		let peer2 = PeerId::random();
		let para_id_100 = ParaId::from(100);
		let para_id_200 = ParaId::from(200);

		// First, create a DB, add some data, and persist it
		{
			let (mut db, _) =
				PersistentDb::new(disk_db.clone(), config, 100).await.expect("should create db");

			// Process some bumps to add reputation data
			let bumps = [
				(para_id_100, [(peer1, Score::new(50).unwrap())].into_iter().collect()),
				(para_id_200, [(peer2, Score::new(75).unwrap())].into_iter().collect()),
			]
			.into_iter()
			.collect();

			db.process_bumps(10, bumps, None).await;

			// Persist to disk
			db.persist().expect("should persist");
		}

		// Now create a new DB instance and verify data was loaded
		{
			let (db, _) = PersistentDb::new(disk_db, config, 100).await.expect("should create db");

			// Verify data was loaded correctly
			assert_eq!(db.processed_finalized_block_number().await, Some(10));
			assert_eq!(db.query(&peer1, &para_id_100).await, Some(Score::new(50).unwrap()));
			assert_eq!(db.query(&peer2, &para_id_200).await, Some(Score::new(75).unwrap()));
			// Non-existent queries should return None
			assert_eq!(db.query(&peer1, &para_id_200).await, None);
			assert_eq!(db.query(&peer2, &para_id_100).await, None);
		}
	}

	#[tokio::test]
	async fn slash_persists_immediately() {
		// Test that slash operations persist to disk immediately
		let disk_db = make_db();
		let config = make_config();

		let peer = PeerId::random();
		let para_id = ParaId::from(100);

		// Create DB and add some reputation
		{
			let (mut db, _) =
				PersistentDb::new(disk_db.clone(), config.clone(), 100).await.expect("create");

			let bumps = [(para_id, [(peer, Score::new(100).unwrap())].into_iter().collect())]
				.into_iter()
				.collect();
			db.process_bumps(10, bumps, None).await;

			// Persist initial state
			db.persist().expect("should persist");
		}

		{
			// 2. Slash (Async)
			let (mut db, handle) = create_and_spawn_db(disk_db.clone(), config.clone()).await;

			db.slash(&peer, &para_id, Score::new(30).unwrap()).await;

			sleep(Duration::from_millis(50)).await;
			handle.abort();
		}

		// 3. Verify
		let (db, _) = PersistentDb::new(disk_db, config, 100).await.expect("reload");
		assert_eq!(db.query(&peer, &para_id).await, Some(Score::new(70).unwrap()));
	}

	#[tokio::test]
	async fn slash_that_removes_entry_persists_immediately() {
		// Test that a slash that reduces score to zero (removing entry) persists immediately
		let disk_db = make_db();
		let config = make_config();

		let peer = PeerId::random();
		let para_id = ParaId::from(100);

		// Create DB and add some reputation
		{
			let (mut db, _) =
				PersistentDb::new(disk_db.clone(), config.clone(), 100).await.expect("create");
			let bumps = [(para_id, [(peer, Score::new(50).unwrap())].into_iter().collect())]
				.into_iter()
				.collect();
			db.process_bumps(10, bumps, None).await;
			db.persist().expect("setup persist");
		}

		// Slash more than the current score - should remove entry
		{
			let (mut db, handle) = create_and_spawn_db(disk_db.clone(), config.clone()).await;

			db.slash(&peer, &para_id, Score::new(100).unwrap()).await;

			sleep(Duration::from_millis(50)).await;
			handle.abort();
		}

		// Create new DB instance and verify entry was removed
		{
			let (db, _) = PersistentDb::new(disk_db, config, 100).await.expect("should create db");

			// Entry should be gone
			assert_eq!(db.query(&peer, &para_id).await, None);
		}
	}

	#[tokio::test]
	async fn prune_paras_persists_immediately() {
		// Test that prune_paras persists immediately
		let disk_db = make_db();
		let config = make_config();

		let peer1 = PeerId::random();
		let peer2 = PeerId::random();
		let para_id_100 = ParaId::from(100);
		let para_id_200 = ParaId::from(200);
		let para_id_300 = ParaId::from(300);

		// Create DB and add reputation for multiple paras
		{
			let (mut db, _) =
				PersistentDb::new(disk_db.clone(), config, 100).await.expect("should create db");

			let bumps = [
				(para_id_100, [(peer1, Score::new(50).unwrap())].into_iter().collect()),
				(para_id_200, [(peer2, Score::new(75).unwrap())].into_iter().collect()),
				(para_id_300, [(peer1, Score::new(25).unwrap())].into_iter().collect()),
			]
			.into_iter()
			.collect();
			db.process_bumps(10, bumps, None).await;
			db.persist().expect("should persist");
		}

		// Prune - only keep para 200 registered
		{
			let (mut db, handle) = create_and_spawn_db(disk_db.clone(), config.clone()).await;
			let registered_paras = [para_id_200].into_iter().collect();

			db.prune_paras(registered_paras).await;
			sleep(Duration::from_millis(50)).await;
			handle.abort();
		}

		// Create new DB instance and verify pruning was persisted
		{
			let (db, _) = PersistentDb::new(disk_db, config, 100).await.expect("should create db");

			// Only para 200 should remain
			assert_eq!(db.query(&peer1, &para_id_100).await, None);
			assert_eq!(db.query(&peer2, &para_id_200).await, Some(Score::new(75).unwrap()));
			assert_eq!(db.query(&peer1, &para_id_300).await, None);
		}
	}

	#[tokio::test]
	async fn periodic_persist_writes_all_data() {
		// Test that persist() correctly writes all in-memory data
		let disk_db = make_db();
		let config = make_config();

		let peer1 = PeerId::random();
		let peer2 = PeerId::random();
		let para_id_100 = ParaId::from(100);
		let para_id_200 = ParaId::from(200);

		// Create DB, add data, but DON'T persist yet
		{
			let (mut db, _) =
				PersistentDb::new(disk_db.clone(), config, 100).await.expect("should create db");

			// Add reputation via bumps (these don't trigger immediate persistence)
			let bumps = [
				(para_id_100, [(peer1, Score::new(50).unwrap())].into_iter().collect()),
				(para_id_200, [(peer2, Score::new(75).unwrap())].into_iter().collect()),
			]
			.into_iter()
			.collect();
			db.process_bumps(15, bumps, None).await;

			// Now call periodic persist
			db.persist().expect("should persist");
		}

		// Reload and verify
		{
			let (db, _) = PersistentDb::new(disk_db, config, 100).await.expect("should create db");

			assert_eq!(db.processed_finalized_block_number().await, Some(15));
			assert_eq!(db.query(&peer1, &para_id_100).await, Some(Score::new(50).unwrap()));
			assert_eq!(db.query(&peer2, &para_id_200).await, Some(Score::new(75).unwrap()));
		}
	}

	#[tokio::test]
	async fn data_survives_simulated_restart() {
		// Test full restart scenario: create, populate, persist, drop, reload
		let disk_db = make_db();
		let config = make_config();

		let peer1 = PeerId::random();
		let peer2 = PeerId::random();
		let peer3 = PeerId::random();
		let para_id_100 = ParaId::from(100);
		let para_id_200 = ParaId::from(200);

		// Session 1: Create and populate
		{
			let (mut db, _) =
				PersistentDb::new(disk_db.clone(), config, 100).await.expect("should create db");

			let bumps = [
				(
					para_id_100,
					[(peer1, Score::new(100).unwrap()), (peer2, Score::new(50).unwrap())]
						.into_iter()
						.collect(),
				),
				(para_id_200, [(peer3, Score::new(200).unwrap())].into_iter().collect()),
			]
			.into_iter()
			.collect();
			db.process_bumps(20, bumps, None).await;

			// Slash peer2
			db.slash(&peer2, &para_id_100, Score::new(25).unwrap()).await;

			// Final persist before "shutdown"
			db.persist().expect("should persist");
		}

		// Session 2: "Restart" - create new instance
		{
			let (mut db, _) =
				PersistentDb::new(disk_db.clone(), config, 100).await.expect("should create db");

			// Verify all data survived
			assert_eq!(db.processed_finalized_block_number().await, Some(20));
			assert_eq!(db.query(&peer1, &para_id_100).await, Some(Score::new(100).unwrap()));
			assert_eq!(db.query(&peer2, &para_id_100).await, Some(Score::new(25).unwrap()));
			assert_eq!(db.query(&peer3, &para_id_200).await, Some(Score::new(200).unwrap()));

			// Continue with more operations
			let bumps = [(para_id_100, [(peer1, Score::new(50).unwrap())].into_iter().collect())]
				.into_iter()
				.collect();
			db.process_bumps(25, bumps, None).await;
			db.persist().expect("should persist");
		}

		// Session 3: Verify continued state
		{
			let (db, _) = PersistentDb::new(disk_db, config, 100).await.expect("should create db");

			assert_eq!(db.processed_finalized_block_number().await, Some(25));
			// peer1 should now have 100 + 50 = 150
			assert_eq!(db.query(&peer1, &para_id_100).await, Some(Score::new(150).unwrap()));
		}
	}

	#[tokio::test]
	async fn roundtrip_serialization_correctness() {
		// Test that data roundtrips correctly through serialization
		let disk_db = make_db();
		let config = make_config();

		// Create peers with specific scores to verify exact values
		let peers: Vec<_> = (0..10).map(|_| PeerId::random()).collect();
		let para_id = ParaId::from(42);

		let original_scores: HashMap<PeerId, Score> = peers
			.iter()
			.enumerate()
			.map(|(i, peer)| (*peer, Score::new((i as u16 + 1) * 100).unwrap()))
			.collect();

		// Store data
		{
			let (mut db, _) =
				PersistentDb::new(disk_db.clone(), config, 100).await.expect("should create db");

			let bumps =
				[(para_id, original_scores.iter().map(|(peer, score)| (*peer, *score)).collect())]
					.into_iter()
					.collect();
			db.process_bumps(100, bumps, None).await;
			db.persist().expect("should persist");
		}

		// Reload and verify exact values
		{
			let (db, _) = PersistentDb::new(disk_db, config, 100).await.expect("should create db");

			for (peer, expected_score) in &original_scores {
				let actual_score = db.query(peer, &para_id).await;
				assert_eq!(
					actual_score,
					Some(*expected_score),
					"Score mismatch for peer after roundtrip"
				);
			}
		}
	}

	#[tokio::test]
	async fn bumps_without_persist_not_saved() {
		// Test that bumps without explicit persist are NOT saved to disk
		// (they only persist via periodic timer or slash)
		let disk_db = make_db();
		let config = make_config();

		let peer = PeerId::random();
		let para_id = ParaId::from(100);

		// Create DB and add bumps, but DON'T persist
		{
			let (mut db, _) =
				PersistentDb::new(disk_db.clone(), config, 100).await.expect("should create db");

			let bumps = [(para_id, [(peer, Score::new(100).unwrap())].into_iter().collect())]
				.into_iter()
				.collect();
			db.process_bumps(10, bumps, None).await;

			// Verify in-memory state
			assert_eq!(db.query(&peer, &para_id).await, Some(Score::new(100).unwrap()));

			// Don't call persist - just drop
		}

		// Create new instance - data should NOT be there
		{
			let (db, _) = PersistentDb::new(disk_db, config, 100).await.expect("should create db");

			// Data was never persisted
			assert_eq!(db.query(&peer, &para_id).await, None);
			assert_eq!(db.processed_finalized_block_number().await, None);
		}
	}

	#[tokio::test]
	async fn multiple_paras_multiple_peers() {
		// Test handling of multiple paras with multiple peers each
		let disk_db = make_db();
		let config = make_config();

		let peers: Vec<_> = (0..5).map(|_| PeerId::random()).collect();
		let paras: Vec<_> = (100..105).map(ParaId::from).collect();

		// Create complex state
		{
			let (mut db, _) =
				PersistentDb::new(disk_db.clone(), config, 100).await.expect("should create db");

			let bumps: BTreeMap<ParaId, HashMap<PeerId, Score>> = paras
				.iter()
				.enumerate()
				.map(|(para_idx, para_id)| {
					let peer_scores: HashMap<PeerId, Score> = peers
						.iter()
						.enumerate()
						.map(|(peer_idx, peer)| {
							let score = ((para_idx + 1) * 10 + peer_idx) as u16;
							(*peer, Score::new(score).unwrap())
						})
						.collect();
					(*para_id, peer_scores)
				})
				.collect();

			db.process_bumps(50, bumps, None).await;
			db.persist().expect("should persist");
		}

		// Verify all data
		{
			let (db, _) = PersistentDb::new(disk_db, config, 100).await.expect("should create db");

			for (para_idx, para_id) in paras.iter().enumerate() {
				for (peer_idx, peer) in peers.iter().enumerate() {
					let expected_score = ((para_idx + 1) * 10 + peer_idx) as u16;
					assert_eq!(
						db.query(peer, para_id).await,
						Some(Score::new(expected_score).unwrap()),
						"Mismatch for para {} peer {}",
						para_idx,
						peer_idx
					);
				}
			}
		}
	}

	#[tokio::test]
	async fn dirty_tracking_only_persists_modified_paras() {
		let disk_db = make_db();
		let config = make_config();

		let peer1 = PeerId::random();
		let para_id_100 = ParaId::from(100);
		let para_id_200 = ParaId::from(200);

		let (mut db, _) =
			PersistentDb::new(disk_db.clone(), config, 100).await.expect("should create db");

		assert!(db.dirty_paras.is_empty(), "Fresh DB should have no dirty paras");

		let bumps_para_100 =
			[(para_id_100, [(peer1, Score::new(50).unwrap())].into_iter().collect())]
				.into_iter()
				.collect();
		db.process_bumps(10, bumps_para_100, None).await;

		assert!(db.dirty_paras.contains(&para_id_100), "Para 100 should be dirty after bump");
		assert!(!db.dirty_paras.contains(&para_id_200), "Para 200 should NOT be dirty");
		assert_eq!(db.dirty_paras.len(), 1, "Only one para should be dirty");

		db.persist().expect("should persist");

		assert!(db.dirty_paras.is_empty(), "Dirty paras should be cleared after persist");

		drop(db);

		let (reloaded_db, _) =
			PersistentDb::new(disk_db, config, 100).await.expect("should reload db");

		assert_eq!(
			reloaded_db.query(&peer1, &para_id_100).await,
			Some(Score::new(50).unwrap()),
			"Para 100 data should be persisted correctly"
		);

		assert_eq!(reloaded_db.inner.len(), 1);

		assert_eq!(
			reloaded_db.processed_finalized_block_number().await,
			Some(10),
			"Last finalized block should be 20"
		);
	}

	#[tokio::test]
	async fn dirty_tracking_cleared_after_prune() {
		let disk_db = make_db();
		let config = make_config();

		let peer1 = PeerId::random();
		let para_id_100 = ParaId::from(100);
		let para_id_200 = ParaId::from(200);

		let (mut db, handle) = create_and_spawn_db(disk_db.clone(), config.clone()).await;

		let bumps: BTreeMap<ParaId, HashMap<PeerId, Score>> = [
			(para_id_100, [(peer1, Score::new(50).unwrap())].into_iter().collect()),
			(para_id_200, [(peer1, Score::new(75).unwrap())].into_iter().collect()),
		]
		.into_iter()
		.collect();
		db.process_bumps(10, bumps, None).await;

		assert_eq!(db.dirty_paras.len(), 2);
		assert!(db.dirty_paras.contains(&para_id_100));
		assert!(db.dirty_paras.contains(&para_id_200));

		let registered = [para_id_100].into_iter().collect();
		db.prune_paras(registered).await;

		assert!(
			db.dirty_paras.is_empty(),
			"Dirty paras should be cleared after prune_paras persists"
		);

		assert_eq!(db.query(&peer1, &para_id_100).await, Some(Score::new(50).unwrap()));
		assert_eq!(db.query(&peer1, &para_id_200).await, None);

		sleep(Duration::from_millis(50)).await;
		handle.abort();
		drop(db);
		let (reloaded, _) =
			PersistentDb::new(disk_db, config, 100).await.expect("should reload db");

		assert_eq!(reloaded.query(&peer1, &para_id_100).await, Some(Score::new(50).unwrap()));
		assert_eq!(reloaded.query(&peer1, &para_id_200).await, None);
	}

	#[tokio::test]
	async fn dirty_tracking_cleared_after_slash() {
		let disk_db = make_db();
		let config = make_config();

		let peer1 = PeerId::random();
		let peer2 = PeerId::random();
		let para_id_100 = ParaId::from(100);
		let para_id_200 = ParaId::from(200);

		let (mut db, handle) = create_and_spawn_db(disk_db.clone(), config.clone()).await;

		let bumps: BTreeMap<ParaId, HashMap<PeerId, Score>> = [
			(para_id_100, [(peer1, Score::new(50).unwrap())].into_iter().collect()),
			(para_id_200, [(peer2, Score::new(75).unwrap())].into_iter().collect()),
		]
		.into_iter()
		.collect();
		db.process_bumps(10, bumps, None).await;

		assert!(db.dirty_paras.contains(&para_id_100));
		assert!(db.dirty_paras.contains(&para_id_200));
		assert_eq!(db.dirty_paras.len(), 2);

		db.slash(&peer1, &para_id_100, Score::new(30).unwrap()).await;

		assert!(!db.dirty_paras.contains(&para_id_100));
		assert!(!db.dirty_paras.contains(&para_id_200));
		assert_eq!(db.dirty_paras.len(), 0);

		assert_eq!(db.query(&peer1, &para_id_100).await, Some(Score::new(20).unwrap()));
		assert_eq!(db.query(&peer2, &para_id_200).await, Some(Score::new(75).unwrap()));

		sleep(Duration::from_millis(50)).await;
		handle.abort();
		drop(db);
		let (reloaded, _) =
			PersistentDb::new(disk_db, config, 100).await.expect("should reload db");

		assert_eq!(reloaded.query(&peer1, &para_id_100).await, Some(Score::new(20).unwrap()));
		assert_eq!(reloaded.query(&peer2, &para_id_200).await, Some(Score::new(75).unwrap()));
	}

	#[tokio::test]
	async fn crash_before_persist_loses_bumps_but_not_slashes() {
		let disk_db = make_db();
		let config = make_config();
		let peer1 = PeerId::random();
		let para_id_100 = ParaId::from(100);

		let (mut db, handle) = create_and_spawn_db(disk_db.clone(), config.clone()).await;

		// 1. Initial bump + Async Persist
		let bumps1 = [(para_id_100, [(peer1, Score::new(50).unwrap())].into_iter().collect())]
			.into_iter()
			.collect();
		db.process_bumps(10, bumps1, None).await;
		db.persist_async(None);
		sleep(Duration::from_millis(50)).await;

		// 2. Slash (Persists immediately)
		db.slash(&peer1, &para_id_100, Score::new(20).unwrap()).await;
		sleep(Duration::from_millis(50)).await;

		// 3. New Bump (Memory only)
		let bumps2 = [(para_id_100, [(peer1, Score::new(15).unwrap())].into_iter().collect())]
			.into_iter()
			.collect();
		db.process_bumps(20, bumps2, None).await;

		// "Crash" (Abort handle, drop DB)
		handle.abort();
		drop(db);

		// 4. Verify Disk State
		let (db, _) = PersistentDb::new(disk_db, config, 100).await.expect("reload");
		// Should have: 50 (initial) - 20 (slash) = 30. The +15 bump was lost.
		assert_eq!(db.query(&peer1, &para_id_100).await, Some(Score::new(30).unwrap()));
	}

	#[tokio::test]
	async fn corrupted_metadata_returns_error() {
		// Test that corrupted metadata in the database returns a codec error
		let disk_db = make_db();
		let config = make_config();

		// Write some corrupted metadata directly to disk
		let mut tx = DBTransaction::new();
		tx.put_vec(config.col_reputation_data, metadata_key(), vec![0xff, 0xff, 0xff]);
		disk_db.write(tx).expect("should write corrupted data");

		// Attempt to create PersistentDb - should fail with codec error
		let err = PersistentDb::new(disk_db, config, 100).await.err().unwrap();
		assert!(matches!(err, PersistenceError::Codec(_)));
	}

	#[tokio::test]
	async fn corrupted_para_reputation_returns_error() {
		// Test that corrupted para reputation data returns a codec error
		let disk_db = make_db();
		let config = make_config();
		let para_id = ParaId::from(100);

		// Write some corrupted para reputation data directly to disk
		let mut tx = DBTransaction::new();
		let key = para_reputation_key(para_id);
		tx.put_vec(config.col_reputation_data, &key, vec![0xde, 0xad, 0xbe, 0xef]);
		disk_db.write(tx).expect("should write corrupted data");

		// Attempt to create PersistentDb - should fail with codec error
		let err = PersistentDb::new(disk_db, config, 100).await.err().unwrap();
		assert!(matches!(err, PersistenceError::Codec(_)));
	}

	#[tokio::test]
	async fn partial_corruption_fails_entire_load() {
		// Test that even if only one para's data is corrupted, the entire load fails
		let disk_db = make_db();
		let config = make_config();
		let para_id_100 = ParaId::from(100);
		let para_id_200 = ParaId::from(200);

		// First, write valid data for para 100
		{
			let (mut db, _) =
				PersistentDb::new(disk_db.clone(), config, 100).await.expect("should create db");
			let peer = PeerId::random();
			let bumps = [(para_id_100, [(peer, Score::new(50).unwrap())].into_iter().collect())]
				.into_iter()
				.collect();
			db.process_bumps(10, bumps, None).await;
			db.persist().expect("should persist");
		}

		// Now corrupt para 200's data
		let mut tx = DBTransaction::new();
		let key = para_reputation_key(para_id_200);
		tx.put_vec(config.col_reputation_data, &key, vec![0xba, 0xad, 0xf0, 0x0d]);
		disk_db.write(tx).expect("should write corrupted data");

		// Attempt to reload - should fail because of corrupted para 200
		let err = PersistentDb::new(disk_db, config, 100).await.err().unwrap();
		assert!(matches!(err, PersistenceError::Codec(_)));
	}

	#[tokio::test]
	async fn async_slash_persists_via_background_task() {
		let disk_db = make_db();
		let config = make_config();
		let peer = PeerId::random();
		let para_id = ParaId::from(100);

		// NEW: Manually spawn task for this test
		let (mut db, task) =
			PersistentDb::new(disk_db.clone(), config, 100).await.expect("should create db");
		let handle = tokio::spawn(task);

		// Add data and persist baseline
		let bumps = [(para_id, [(peer, Score::new(100).unwrap())].into_iter().collect())]
			.into_iter()
			.collect();
		db.process_bumps(10, bumps, None).await;

		db.persist_async(None);
		tokio::time::sleep(std::time::Duration::from_millis(50)).await;

		// Slash
		db.slash(&peer, &para_id, Score::new(30).unwrap()).await;

		// Wait for background task
		tokio::time::sleep(std::time::Duration::from_millis(50)).await;

		// Verify on new DB instance
		let (db2, _) = PersistentDb::new(disk_db, config, 100).await.expect("reload");
		// Score should be 100 - 30 = 70
		assert_eq!(db2.query(&peer, &para_id).await, Some(Score::new(70).unwrap()));

		handle.abort();
	}
}
