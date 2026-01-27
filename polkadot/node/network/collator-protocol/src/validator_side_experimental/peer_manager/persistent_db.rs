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

use async_trait::async_trait;
use codec::{Decode, Encode};
use polkadot_node_network_protocol::PeerId;
use polkadot_node_subsystem_util::database::{DBTransaction, Database};
use polkadot_primitives::{BlockNumber, Id as ParaId};
use std::{
	collections::{BTreeMap, BTreeSet, HashMap},
	sync::Arc,
};

use crate::{
	validator_side_experimental::{
		common::Score,
		peer_manager::{
			backend::Backend,
			db::Db,
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
/// The main loop is responsible for calling `persist()` periodically (currently, every 10 minutes).
pub struct PersistentDb {
	/// In-memory database (does all the actual logic).
	inner: Db,
	/// Disk database handle.
	disk_db: Arc<dyn Database>,
	/// Column configuration.
	config: ReputationConfig,
}

impl PersistentDb {
	/// Create a new persistent DB, loading existing state from disk.
	pub async fn new(
		disk_db: Arc<dyn Database>,
		config: ReputationConfig,
		stored_limit_per_para: u16,
	) -> Result<Self, PersistenceError> {
		// Create empty in-memory DB
		let inner = Db::new(stored_limit_per_para).await;

		// Load data from disk into the in-memory DB
		let mut instance = Self { inner, disk_db, config };
		let (para_count, total_entries) = instance.load_from_disk().await?;

		let last_finalized = instance.inner.processed_finalized_block_number().await;

		// Use info level for clear observability in tests and production
		if para_count > 0 || last_finalized.is_some() {
			gum::trace!(
				target: LOG_TARGET,
				?last_finalized,
				para_count,
				total_peer_entries = total_entries,
				"Loaded existing reputation DB from disk"
			);
		} else {
			gum::trace!(
				target: LOG_TARGET,
				"Reputation DB initialized fresh (no existing data on disk)"
			);
		}

		Ok(instance)
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
			// We need to directly access the inner fields to restore state
			// This requires making Db fields pub(super) or adding setter methods
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
				let entries = stored.to_hashmap();
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
			Some(raw) =>
				StoredMetadata::decode(&mut &raw[..]).map(Some).map_err(PersistenceError::Codec),
		}
	}

	/// Persist a single para's data to disk (called immediately after slash).
	fn persist_para(&self, para_id: &ParaId) -> Result<(), PersistenceError> {
		let mut tx = DBTransaction::new();
		let key = para_reputation_key(*para_id);

		if let Some(peer_scores) = self.inner.get_para_reputations(para_id) {
			if peer_scores.is_empty() {
				tx.delete(self.config.col_reputation_data, &key);
				gum::trace!(
					target: LOG_TARGET,
					?para_id,
					"Deleted empty para reputation entry from disk"
				);
			} else {
				let stored = StoredParaReputations::from_hashmap(&peer_scores);
				tx.put_vec(self.config.col_reputation_data, &key, stored.encode());
				gum::trace!(
					target: LOG_TARGET,
					?para_id,
					peers = peer_scores.len(),
					"Persisted para reputation to disk"
				);
			}
		} else {
			tx.delete(self.config.col_reputation_data, &key);
			gum::trace!(
				target: LOG_TARGET,
				?para_id,
				"Deleted removed para reputation entry from disk"
			);
		}

		self.disk_db.write(tx).map_err(PersistenceError::Io)
	}

	/// Persist all in-memory data to disk.
	///
	/// This should be called periodically by the main loop (currently, every 10 minutes).
	/// It writes all reputation data and metadata in a single transaction.
	pub fn persist(&self) -> Result<(), PersistenceError> {
		let mut tx = DBTransaction::new();

		// Write metadata
		let meta = StoredMetadata { last_finalized: self.inner.get_last_finalized() };
		tx.put_vec(self.config.col_reputation_data, metadata_key(), meta.encode());

		// Write all para data
		let mut total_entries = 0;
		let mut para_count = 0;
		let all_reps: Vec<_> = self.inner.all_reputations().collect();

		for (para_id, peer_scores) in all_reps {
			let key = para_reputation_key(*para_id);
			if peer_scores.is_empty() {
				tx.delete(self.config.col_reputation_data, &key);
			} else {
				let stored = StoredParaReputations::from_hashmap(peer_scores);
				tx.put_vec(self.config.col_reputation_data, &key, stored.encode());
				total_entries += peer_scores.len();
				para_count += 1;
			}
		}

		self.disk_db.write(tx).map_err(PersistenceError::Io)?;

		gum::debug!(
			target: LOG_TARGET,
			total_peer_entries = total_entries,
			para_count,
			last_finalized = ?meta.last_finalized,
			"Periodic persistence completed: reputation DB written to disk"
		);

		Ok(())
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

		// Immediately persist to disk after slash (security-critical)
		match self.persist_para(para_id) {
			Ok(()) => {
				gum::debug!(
					target: LOG_TARGET,
					?para_id,
					?peer_id,
					slash_value = ?value,
					"Slash persisted to disk immediately"
				);
			},
			Err(e) => {
				gum::error!(
					target: LOG_TARGET,
					?para_id,
					?peer_id,
					error = ?e,
					"CRITICAL: Failed to persist reputation after slash to disk. \
					Slash is recorded in-memory and will be persisted by periodic timer."
				);
			},
		}
	}

	async fn prune_paras(&mut self, registered_paras: BTreeSet<ParaId>) {
		// Collect paras to prune before modifying state
		let paras_to_prune: Vec<ParaId> = self
			.inner
			.all_reputations()
			.filter(|(para_id, _)| !registered_paras.contains(para_id))
			.map(|(para_id, _)| *para_id)
			.collect();
		gum::trace!(target: LOG_TARGET, ?paras_to_prune, "Alex: prune_paras");

		let pruned_count = paras_to_prune.len();

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

		// Write remaining paras and metadata
		let meta = StoredMetadata { last_finalized: self.inner.get_last_finalized() };
		tx.put_vec(self.config.col_reputation_data, metadata_key(), meta.encode());

		for (para_id, peer_scores) in self.inner.all_reputations() {
			let key = para_reputation_key(*para_id);
			if !peer_scores.is_empty() {
				let stored = StoredParaReputations::from_hashmap(peer_scores);
				tx.put_vec(self.config.col_reputation_data, &key, stored.encode());
			}
		}

		match self.disk_db.write(tx).map_err(PersistenceError::Io) {
			Ok(()) => {
				gum::debug!(
					target: LOG_TARGET,
					pruned_para_count = pruned_count,
					remaining_para_count = paras_after,
					registered_para_count = registered_paras.len(),
					"Prune paras persisted to disk immediately"
				);
			},
			Err(e) => {
				gum::error!(
					target: LOG_TARGET,
					error = ?e,
					"Failed to persist reputation after pruning paras. \
					Pruned data is removed from memory and will be persisted by periodic timer."
				);
			},
		}
	}

	async fn process_bumps(
		&mut self,
		leaf_number: BlockNumber,
		bumps: BTreeMap<ParaId, HashMap<PeerId, Score>>,
		decay_value: Option<Score>,
	) -> Vec<ReputationUpdate> {
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

	const DATA_COL: u32 = 0;
	const NUM_COLUMNS: u32 = 1;

	fn make_db() -> Arc<dyn Database> {
		let db = kvdb_memorydb::create(NUM_COLUMNS);
		let db = DbAdapter::new(db, &[DATA_COL]);
		Arc::new(db)
	}

	fn make_config() -> ReputationConfig {
		ReputationConfig { col_reputation_data: DATA_COL }
	}

	#[tokio::test]
	async fn load_from_empty_disk_fresh_start() {
		// Test that PersistentDb can be created from an empty database (fresh start)
		let disk_db = make_db();
		let config = make_config();

		let db = PersistentDb::new(disk_db, config, 100).await.expect("should create db");

		// Fresh start should have no finalized block
		assert_eq!(db.processed_finalized_block_number().await, None);
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
			let mut db =
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
			let db = PersistentDb::new(disk_db, config, 100).await.expect("should create db");

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
			let mut db =
				PersistentDb::new(disk_db.clone(), config, 100).await.expect("should create db");

			let bumps = [(para_id, [(peer, Score::new(100).unwrap())].into_iter().collect())]
				.into_iter()
				.collect();
			db.process_bumps(10, bumps, None).await;

			// Persist initial state
			db.persist().expect("should persist");

			// Now slash - this should persist immediately
			db.slash(&peer, &para_id, Score::new(30).unwrap()).await;
		}

		// Create new DB instance and verify slash was persisted
		{
			let db = PersistentDb::new(disk_db, config, 100).await.expect("should create db");

			// Score should be 100 - 30 = 70
			assert_eq!(db.query(&peer, &para_id).await, Some(Score::new(70).unwrap()));
		}
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
			let mut db =
				PersistentDb::new(disk_db.clone(), config, 100).await.expect("should create db");

			let bumps = [(para_id, [(peer, Score::new(50).unwrap())].into_iter().collect())]
				.into_iter()
				.collect();
			db.process_bumps(10, bumps, None).await;
			db.persist().expect("should persist");

			// Slash more than the current score - should remove entry
			db.slash(&peer, &para_id, Score::new(100).unwrap()).await;
		}

		// Create new DB instance and verify entry was removed
		{
			let db = PersistentDb::new(disk_db, config, 100).await.expect("should create db");

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
			let mut db =
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

			// Prune - only keep para 200 registered
			let registered_paras = [para_id_200].into_iter().collect();
			db.prune_paras(registered_paras).await;
		}

		// Create new DB instance and verify pruning was persisted
		{
			let db = PersistentDb::new(disk_db, config, 100).await.expect("should create db");

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
			let mut db =
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
			let db = PersistentDb::new(disk_db, config, 100).await.expect("should create db");

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
			let mut db =
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
			let mut db =
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
			let db = PersistentDb::new(disk_db, config, 100).await.expect("should create db");

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
			let mut db =
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
			let db = PersistentDb::new(disk_db, config, 100).await.expect("should create db");

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
			let mut db =
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
			let db = PersistentDb::new(disk_db, config, 100).await.expect("should create db");

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
			let mut db =
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
			let db = PersistentDb::new(disk_db, config, 100).await.expect("should create db");

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
}
