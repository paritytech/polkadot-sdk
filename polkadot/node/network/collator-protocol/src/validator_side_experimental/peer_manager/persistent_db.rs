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

use crate::{
	validator_side_experimental::{
		common::Score,
		peer_manager::{backend::Backend, ReputationUpdateKind},
	},
	LOG_TARGET,
};
use async_trait::async_trait;
use codec::{Decode, Encode};
use polkadot_node_network_protocol::PeerId;
use polkadot_node_subsystem_util::database::{DBTransaction, Database};
use polkadot_primitives::{BlockNumber, Id as ParaId};
use std::{
	collections::{BTreeMap, BTreeSet, HashMap},
	sync::Arc,
	time::{SystemTime, UNIX_EPOCH},
};

use super::ReputationUpdate;

#[derive(Debug, thiserror::Error)]
enum Error {
	#[error("Database IO error: {0}")]
	Io(#[from] std::io::Error),
	#[error("Dailed to decode value from DB: {0}")]
	Decode(#[from] codec::Error),
	#[error("Invalid key for para {para_id:?}")]
	InvalidKey { para_id: Option<ParaId> },
	#[error("Failed to iterate entries: {source}")]
	IterationFailed {
		#[source]
		source: std::io::Error,
	},
}

type Result<T> = std::result::Result<T, Error>;

/// Key format [ParaId|PeerId]
fn reputation_key(para_id: &ParaId, peer_id: &PeerId) -> Vec<u8> {
	let peer_bytes = peer_id.to_bytes();
	let mut key = Vec::with_capacity(4 + peer_bytes.len());
	para_id.using_encoded(|encoded_id| key.extend_from_slice(encoded_id));
	key.extend_from_slice(&peer_bytes);
	key
}

fn para_id_from_key(key: &[u8]) -> Option<ParaId> {
	if key.len() < 4 {
		return None;
	}

	ParaId::decode(&mut &key[0..4]).ok()
}

fn peer_id_from_key(key: &[u8]) -> Option<PeerId> {
	if key.len() <= 4 {
		return None;
	}

	PeerId::from_bytes(&key[4..]).ok()
}

fn load_and_decode<D: Decode>(db: &dyn Database, column: u32, key: &[u8]) -> Result<Option<D>> {
	match db.get(column, key)? {
		None => Ok(None),
		Some(raw) => D::decode(&mut &raw[..]).map(Some).map_err(Into::into),
	}
}

type Timestamp = u128;

/// Configuration for the reputation db.
#[derive(Debug, Clone, Copy)]
pub struct ReputationConfig {
	/// The data column in the store to use for reputation data.
	pub col_reputation_data: u32,
}

#[derive(Clone, Debug, Decode, Encode)]
struct ScoreEntry {
	score: Score,
	last_bumped: Timestamp,
}

/// Key for the last processed finalized block number.
const LAST_FINALIZED_KEY: &[u8] = b"last_finalized";

pub struct PersistentDb {
	db: Arc<dyn Database>,
	reputation_column: u32,
	stored_limit_per_para: u8,
}

#[async_trait]
impl Backend for PersistentDb {
	async fn processed_finalized_block_number(&self) -> Option<polkadot_primitives::BlockNumber> {
		match load_and_decode(self.db.as_ref(), self.reputation_column, LAST_FINALIZED_KEY) {
			Ok(block_number) => block_number,
			Err(e) => {
				gum::error!(target: LOG_TARGET, ?e, "Failed to load the last finalized block number");
				None
			},
		}
	}

	async fn query(&self, peer_id: &PeerId, para_id: &ParaId) -> Option<Score> {
		let key = reputation_key(para_id, peer_id);
		match load_and_decode::<ScoreEntry>(self.db.as_ref(), self.reputation_column, &key) {
			Ok(Some(entry)) => Some(entry.score),
			Ok(None) => None,
			Err(e) => {
				gum::error!(target: LOG_TARGET, ?peer_id, ?para_id, ?e, "Failed to query");
				None
			},
		}
	}

	async fn slash(&mut self, peer_id: &PeerId, para_id: &ParaId, value: Score) {
		if let Err(e) = self.slash_inner(peer_id, para_id, value) {
			gum::error!(target: LOG_TARGET, ?peer_id, ?para_id, ?e, "Failed to slash reputation");
		}
	}

	async fn prune_paras(&mut self, registered_paras: std::collections::BTreeSet<ParaId>) {
		if let Err(e) = self.prune_paras_inner(registered_paras) {
			gum::error!(target: LOG_TARGET, ?e, "Failed to prune paras.");
		}
	}

	async fn process_bumps(
		&mut self,
		leaf_number: polkadot_primitives::BlockNumber,
		bumps: std::collections::BTreeMap<ParaId, std::collections::HashMap<PeerId, Score>>,
		decay_value: Option<Score>,
	) -> Vec<super::ReputationUpdate> {
		match self.process_bumps_inner(leaf_number, bumps, decay_value) {
			Ok(updates) => updates,
			Err(e) => {
				gum::error!(target:LOG_TARGET, ?e, leaf_number, "Failed to process reputation bumps.");
				vec![]
			},
		}
	}

	async fn max_scores_for_paras(
		&self,
		paras: std::collections::BTreeSet<ParaId>,
	) -> std::collections::HashMap<ParaId, Score> {
		let mut max_scores = HashMap::with_capacity(paras.len());
		for para in paras {
			let max_score = match self.get_all_entries_for_para(&para) {
				Ok(score_entries) => score_entries
					.values()
					.map(|score_entry| score_entry.score)
					.max()
					.unwrap_or(Score::new(0).unwrap()),
				Err(e) => {
					gum::error!(
						target: LOG_TARGET,
						?para,
						?e,
						"Failed to get entries, can't determine max"
					);
					Score::new(0).unwrap()
				},
			};
			max_scores.insert(para, max_score);
		}
		max_scores
	}
}

impl PersistentDb {
	pub fn new(db: Arc<dyn Database>, col: u32, stored_limit_per_para: u8) -> Self {
		Self { db, reputation_column: col, stored_limit_per_para }
	}

	fn slash_inner(&mut self, peer_id: &PeerId, para_id: &ParaId, value: Score) -> Result<()> {
		let key = reputation_key(para_id, peer_id);
		// Searching the reputation of peer_id for the para_id
		let mut db_entry =
			match load_and_decode::<ScoreEntry>(self.db.as_ref(), self.reputation_column, &key)? {
				Some(score_entry) => score_entry,
				None => {
					return Ok(());
				},
			};
		let mut db_transaction = self.db.transaction();
		if db_entry.score <= value {
			db_transaction.delete(self.reputation_column, &key);
		} else {
			db_entry.score.saturating_sub(value.into());
			db_transaction.put_vec(self.reputation_column, &key, db_entry.encode());
		}
		self.db.write(db_transaction)?;
		Ok(())
	}

	fn prune_paras_inner(&mut self, registed_paras: BTreeSet<ParaId>) -> Result<()> {
		//
		let mut db_transaction = self.db.transaction();
		let mut paras_to_prune = BTreeSet::new();

		for key in self.db.iter(self.reputation_column) {
			let (key, _) = key.map_err(|e| Error::IterationFailed { source: e })?;
			// Only interested in Para|Peer keys
			if &key[..] == LAST_FINALIZED_KEY {
				continue;
			}
			if let Some(para_id) = para_id_from_key(&key) {
				if !registed_paras.contains(&para_id) {
					paras_to_prune.insert(para_id);
				}
			}
		}

		for para_id in &paras_to_prune {
			let mut prefix_for_para = Vec::new();
			para_id.using_encoded(|encoded| prefix_for_para.extend_from_slice(encoded));
			// Delete all entries from DB starting with the Para ID
			db_transaction.delete_prefix(self.reputation_column, &prefix_for_para);
		}

		if !paras_to_prune.is_empty() {
			gum::debug!(
				target: LOG_TARGET,
				pruned_count = ?paras_to_prune.len(),
				"Pruning reputation data for unregistered paras."
			);
			self.db.write(db_transaction)?;
		}
		Ok(())
	}

	fn process_bumps_inner(
		&mut self,
		leaf_number: BlockNumber,
		bumps: BTreeMap<ParaId, HashMap<PeerId, Score>>,
		decay_value: Option<Score>,
	) -> Result<Vec<ReputationUpdate>> {
		let last_finalized = load_and_decode::<BlockNumber>(
			self.db.as_ref(),
			self.reputation_column,
			LAST_FINALIZED_KEY,
		)?
		.unwrap_or(0);
		if last_finalized >= leaf_number {
			gum::debug!(
				target: LOG_TARGET,
				leaf_number,
				last_finalized,
				"Skipping reputation bumps for old block"
			);
			return Ok(vec![]);
		}
		let mut db_transaction = self.db.transaction();
		db_transaction.put_vec(self.reputation_column, LAST_FINALIZED_KEY, leaf_number.encode());
		self.db.write(db_transaction)?;

		let updates = self.bump_reputations(bumps, decay_value)?;
		Ok(updates)
	}

	fn bump_reputations(
		&mut self,
		bumps: BTreeMap<ParaId, HashMap<PeerId, Score>>,
		maybe_decay_value: Option<Score>,
	) -> Result<Vec<ReputationUpdate>> {
		let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_millis();
		let mut reported_updates = vec![];
		let mut db_transaction = self.db.transaction();

		for (para_id, bumps_per_para) in bumps {
			let mut current_entries_for_para = self.get_all_entries_for_para(&para_id)?;
			let original_entries = current_entries_for_para.keys().cloned().collect();

			// Apply bumps
			let para_bumps =
				self.apply_bumps(&mut current_entries_for_para, &bumps_per_para, now, para_id);
			reported_updates.extend(para_bumps);

			// Apply decay
			if let Some(decay_value) = maybe_decay_value {
				let decay_updates = self.apply_decay(
					&mut current_entries_for_para,
					&bumps_per_para,
					decay_value,
					para_id,
				);
				reported_updates.extend(decay_updates);
			}

			// LRU
			let eviction_updates = self.apply_lru_eviction(&mut current_entries_for_para, para_id);
			reported_updates.extend(eviction_updates);

			//
			self.commit_to_db(
				para_id,
				&original_entries,
				&current_entries_for_para,
				&mut db_transaction,
			);
		}
		self.db.write(db_transaction)?;
		Ok(reported_updates)
	}

	fn apply_bumps(
		&self,
		entries: &mut HashMap<PeerId, ScoreEntry>,
		bumps: &HashMap<PeerId, Score>,
		now: Timestamp,
		para_id: ParaId,
	) -> Vec<ReputationUpdate> {
		let mut updates = Vec::with_capacity(bumps.len());
		for (peer_id, bump) in bumps.iter() {
			if u16::from(*bump) == 0 {
				continue;
			}
			entries
				.entry(*peer_id)
				.and_modify(|score_entry| {
					score_entry.score.saturating_add(u16::from(*bump));
					score_entry.last_bumped = now;
				})
				.or_insert(ScoreEntry { score: *bump, last_bumped: now });
			updates.push(ReputationUpdate {
				peer_id: *peer_id,
				para_id,
				value: *bump,
				kind: ReputationUpdateKind::Bump,
			});
		}
		updates
	}

	fn apply_decay(
		&self,
		entries: &mut HashMap<PeerId, ScoreEntry>,
		bumped_peers: &HashMap<PeerId, Score>,
		decay_value: Score,
		para_id: ParaId,
	) -> Vec<ReputationUpdate> {
		let mut updates = Vec::new();
		entries.retain(|peer_id, score_entry| {
			if bumped_peers.contains_key(peer_id) {
				return true;
			}
			if score_entry.score <= decay_value {
				updates.push(ReputationUpdate {
					peer_id: *peer_id,
					para_id,
					value: score_entry.score,
					kind: ReputationUpdateKind::Slash,
				});
				// Remove entry
				false
			} else {
				score_entry.score.saturating_sub(decay_value.into());
				updates.push(ReputationUpdate {
					peer_id: *peer_id,
					para_id,
					value: decay_value,
					kind: ReputationUpdateKind::Slash,
				});
				true
			}
		});
		updates
	}

	fn apply_lru_eviction(
		&self,
		entries: &mut HashMap<PeerId, ScoreEntry>,
		para_id: ParaId,
	) -> Vec<ReputationUpdate> {
		if entries.len() <= self.stored_limit_per_para.into() {
			return Vec::new();
		}

		let extra_entries = entries.len() - self.stored_limit_per_para as usize;
		gum::debug!(
			target: LOG_TARGET,
			?para_id,
			entries_count = entries.len(),
			limit = self.stored_limit_per_para,
			evicting = extra_entries,
			"Performing LRU eviction"
		);

		let mut sorted: Vec<_> = entries
			.iter()
			.map(|(peer_id, score_entry)| (*peer_id, score_entry.last_bumped))
			.collect();
		sorted.sort_by_key(|(_, last_bumped)| *last_bumped);
		let peers_to_evict: Vec<_> =
			sorted.iter().take(extra_entries).map(|(peer_id, _)| *peer_id).collect();
		let mut updates = Vec::with_capacity(extra_entries);
		for peer_id in peers_to_evict {
			if let Some(score_entry) = entries.remove(&peer_id) {
				updates.push(ReputationUpdate {
					peer_id,
					para_id,
					value: score_entry.score,
					kind: ReputationUpdateKind::Slash,
				});
			}
		}
		updates
	}

	fn commit_to_db(
		&self,
		para_id: ParaId,
		original_peers: &BTreeSet<PeerId>,
		current_state: &HashMap<PeerId, ScoreEntry>,
		db_transaction: &mut DBTransaction,
	) {
		if current_state.is_empty() {
			let mut prefix = Vec::new();
			para_id.using_encoded(|encoded| prefix.extend_from_slice(encoded));
			db_transaction.delete_prefix(self.reputation_column, &prefix);
			gum::debug!(
				target: LOG_TARGET,
				?para_id,
				"Para has no more peers, removing from database"
			);
			return;
		}

		// Delete removed entries
		for peer_id in original_peers {
			if !current_state.contains_key(peer_id) {
				let key = reputation_key(&para_id, peer_id);
				db_transaction.delete(self.reputation_column, &key);
			}
		}

		for (peer_id, score_entry) in current_state {
			let key = reputation_key(&para_id, peer_id);
			db_transaction.put_vec(self.reputation_column, &key, score_entry.encode());
		}
	}

	fn get_all_entries_for_para(&self, para_id: &ParaId) -> Result<HashMap<PeerId, ScoreEntry>> {
		let mut entries: HashMap<PeerId, ScoreEntry> = HashMap::new();
		let mut para_prefix = Vec::new();
		para_id.using_encoded(|encoded| para_prefix.extend_from_slice(encoded));
		for item in self.db.iter_with_prefix(self.reputation_column, &para_prefix) {
			let (key, _) = item.map_err(|e| Error::IterationFailed { source: e })?;
			let peer_id = peer_id_from_key(&key)
				.ok_or_else(|| Error::InvalidKey { para_id: Some(*para_id) })?;
			let entry =
				load_and_decode::<ScoreEntry>(self.db.as_ref(), self.reputation_column, &key)?
					.ok_or_else(|| Error::InvalidKey { para_id: Some(*para_id) })?;
			entries.insert(peer_id, entry);
		}
		Ok(entries)
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use std::time::Duration;

	fn test_db() -> Arc<dyn Database> {
		let db = kvdb_memorydb::create(1);
		let db = polkadot_node_subsystem_util::database::kvdb_impl::DbAdapter::new(db, &[0]);
		Arc::new(db)
	}

	fn para_count(db: &PersistentDb) -> usize {
		let mut unique_paras = BTreeSet::new();
		for item in db.db.iter(0) {
			let (key, _) = item.unwrap();
			if &key[..] == LAST_FINALIZED_KEY {
				continue;
			}

			if let Some(para_id) = para_id_from_key(&key) {
				unique_paras.insert(para_id);
			}
		}
		unique_paras.len()
	}

	#[tokio::test]
	async fn test_prune_paras() {
		let db = test_db();
		let mut db = PersistentDb::new(db, 0, 10);

		db.prune_paras(BTreeSet::new()).await;
		assert_eq!(para_count(&db), 0);

		db.prune_paras([ParaId::from(100), ParaId::from(200)].into_iter().collect())
			.await;
		assert_eq!(para_count(&db), 0);

		let peer_id = PeerId::random();
		let another_peer_id = PeerId::random();
		assert_eq!(
			db.process_bumps(
				1,
				[
					(ParaId::from(100), [(peer_id, Score::new(10).unwrap())].into_iter().collect()),
					(
						ParaId::from(200),
						[(another_peer_id, Score::new(12).unwrap())].into_iter().collect()
					),
					(ParaId::from(300), [(peer_id, Score::new(15).unwrap())].into_iter().collect())
				]
				.into_iter()
				.collect(),
				Some(Score::new(10).unwrap()),
			)
			.await
			.len(),
			3
		);
		assert_eq!(para_count(&db), 3);

		db.prune_paras(
			[ParaId::from(100), ParaId::from(200), ParaId::from(300), ParaId::from(400)]
				.into_iter()
				.collect(),
		)
		.await;
		assert_eq!(para_count(&db), 3);

		assert_eq!(db.query(&peer_id, &ParaId::from(100)).await.unwrap(), Score::new(10).unwrap());
		assert_eq!(
			db.query(&another_peer_id, &ParaId::from(200)).await.unwrap(),
			Score::new(12).unwrap()
		);
		assert_eq!(db.query(&peer_id, &ParaId::from(300)).await.unwrap(), Score::new(15).unwrap());

		// Prunes multiple paras.
		db.prune_paras([ParaId::from(300)].into_iter().collect()).await;
		assert_eq!(para_count(&db), 1);
		assert_eq!(db.query(&peer_id, &ParaId::from(100)).await, None);
		assert_eq!(db.query(&another_peer_id, &ParaId::from(200)).await, None);
		assert_eq!(db.query(&peer_id, &ParaId::from(300)).await.unwrap(), Score::new(15).unwrap());

		// Prunes all paras.
		db.prune_paras(BTreeSet::new()).await;
		assert_eq!(para_count(&db), 0);
		assert_eq!(db.query(&peer_id, &ParaId::from(300)).await, None);
	}

	#[tokio::test]
	async fn test_slash() {
		let db = test_db();
		let mut db = PersistentDb::new(db, 0, 10);

		// Test slash on empty DB
		let peer_id = PeerId::random();
		db.slash(&peer_id, &ParaId::from(100), Score::new(50).unwrap()).await;
		assert_eq!(db.query(&peer_id, &ParaId::from(100)).await, None);

		// Test slash on non-existent para
		let another_peer_id = PeerId::random();
		assert_eq!(
			db.process_bumps(
				1,
				[
					(ParaId::from(100), [(peer_id, Score::new(10).unwrap())].into_iter().collect()),
					(
						ParaId::from(200),
						[(another_peer_id, Score::new(12).unwrap())].into_iter().collect()
					),
					(ParaId::from(300), [(peer_id, Score::new(15).unwrap())].into_iter().collect())
				]
				.into_iter()
				.collect(),
				Some(Score::new(10).unwrap()),
			)
			.await
			.len(),
			3
		);
		assert_eq!(db.query(&peer_id, &ParaId::from(100)).await.unwrap(), Score::new(10).unwrap());
		assert_eq!(
			db.query(&another_peer_id, &ParaId::from(200)).await.unwrap(),
			Score::new(12).unwrap()
		);
		assert_eq!(db.query(&peer_id, &ParaId::from(300)).await.unwrap(), Score::new(15).unwrap());

		db.slash(&peer_id, &ParaId::from(200), Score::new(4).unwrap()).await;
		assert_eq!(db.query(&peer_id, &ParaId::from(100)).await.unwrap(), Score::new(10).unwrap());
		assert_eq!(
			db.query(&another_peer_id, &ParaId::from(200)).await.unwrap(),
			Score::new(12).unwrap()
		);
		assert_eq!(db.query(&peer_id, &ParaId::from(300)).await.unwrap(), Score::new(15).unwrap());

		// Test regular slash
		db.slash(&peer_id, &ParaId::from(100), Score::new(4).unwrap()).await;
		assert_eq!(db.query(&peer_id, &ParaId::from(100)).await.unwrap(), Score::new(6).unwrap());

		// Test slash which removes the entry altogether
		db.slash(&peer_id, &ParaId::from(100), Score::new(8).unwrap()).await;
		assert_eq!(db.query(&peer_id, &ParaId::from(100)).await, None);
		assert_eq!(para_count(&db), 2);
	}

	#[tokio::test]
	async fn test_reputation_updates() {
		let mem_db = test_db();
		let mut db = PersistentDb::new(mem_db.clone(), 0, 10);

		assert_eq!(db.processed_finalized_block_number().await, None);
		assert_eq!(para_count(&db), 0);

		// Test empty update with no decay.
		assert!(db.process_bumps(10, Default::default(), None).await.is_empty());
		assert_eq!(db.processed_finalized_block_number().await, Some(10));
		assert_eq!(para_count(&db), 0);

		// Test a query on a non-existant entry.
		assert_eq!(db.query(&PeerId::random(), &ParaId::from(1000)).await, None);

		// Test empty update with decay.
		assert!(db
			.process_bumps(11, Default::default(), Some(Score::new(1).unwrap()))
			.await
			.is_empty());
		assert_eq!(db.processed_finalized_block_number().await, Some(11));
		assert_eq!(para_count(&db), 0);

		// Test empty update with a leaf number smaller than the latest one.
		assert!(db
			.process_bumps(5, Default::default(), Some(Score::new(1).unwrap()))
			.await
			.is_empty());
		assert_eq!(db.processed_finalized_block_number().await, Some(11));
		assert_eq!(para_count(&db), 0);

		// Test an update with zeroed score.
		assert!(db
			.process_bumps(
				12,
				[(
					ParaId::from(100),
					[(PeerId::random(), Score::new(0).unwrap())].into_iter().collect()
				)]
				.into_iter()
				.collect(),
				Some(Score::new(1).unwrap())
			)
			.await
			.is_empty());
		assert_eq!(db.processed_finalized_block_number().await, Some(12));
		assert_eq!(para_count(&db), 0);

		// Reuse the same 12 block height, it should not be taken into consideration.
		let first_peer_id = PeerId::random();
		let first_para_id = ParaId::from(100);
		assert!(db
			.process_bumps(
				12,
				[(first_para_id, [(first_peer_id, Score::new(10).unwrap())].into_iter().collect())]
					.into_iter()
					.collect(),
				Some(Score::new(1).unwrap())
			)
			.await
			.is_empty());
		assert_eq!(db.processed_finalized_block_number().await, Some(12));
		assert_eq!(para_count(&db), 0);
		assert_eq!(db.query(&first_peer_id, &first_para_id).await, None);

		// Test a non-zero update on an empty DB.
		assert_eq!(
			db.process_bumps(
				13,
				[(first_para_id, [(first_peer_id, Score::new(10).unwrap())].into_iter().collect())]
					.into_iter()
					.collect(),
				Some(Score::new(1).unwrap())
			)
			.await,
			vec![ReputationUpdate {
				peer_id: first_peer_id,
				para_id: first_para_id,
				kind: ReputationUpdateKind::Bump,
				value: Score::new(10).unwrap()
			}]
		);
		assert_eq!(db.processed_finalized_block_number().await, Some(13));
		assert_eq!(para_count(&db), 1);
		assert_eq!(
			db.query(&first_peer_id, &first_para_id).await.unwrap(),
			Score::new(10).unwrap()
		);
		// Query a non-existant peer_id for this para.
		assert_eq!(db.query(&PeerId::random(), &first_para_id).await, None);
		// Query this peer's rep for a different para.
		assert_eq!(db.query(&first_peer_id, &ParaId::from(200)).await, None);

		// Test a subsequent update with a lower block height. Will be ignored.
		assert!(db
			.process_bumps(
				10,
				[(first_para_id, [(first_peer_id, Score::new(10).unwrap())].into_iter().collect())]
					.into_iter()
					.collect(),
				Some(Score::new(1).unwrap())
			)
			.await
			.is_empty());
		assert_eq!(db.processed_finalized_block_number().await, Some(13));
		assert_eq!(para_count(&db), 1);
		assert_eq!(
			db.query(&first_peer_id, &first_para_id).await.unwrap(),
			Score::new(10).unwrap()
		);

		let second_para_id = ParaId::from(200);
		let second_peer_id = PeerId::random();
		// Test a subsequent update with no decay.
		assert_eq!(
			db.process_bumps(
				14,
				[
					(
						first_para_id,
						[(second_peer_id, Score::new(10).unwrap())].into_iter().collect()
					),
					(
						second_para_id,
						[(first_peer_id, Score::new(5).unwrap())].into_iter().collect()
					)
				]
				.into_iter()
				.collect(),
				None
			)
			.await,
			vec![
				ReputationUpdate {
					peer_id: second_peer_id,
					para_id: first_para_id,
					kind: ReputationUpdateKind::Bump,
					value: Score::new(10).unwrap()
				},
				ReputationUpdate {
					peer_id: first_peer_id,
					para_id: second_para_id,
					kind: ReputationUpdateKind::Bump,
					value: Score::new(5).unwrap()
				}
			]
		);
		assert_eq!(para_count(&db), 2);
		assert_eq!(db.processed_finalized_block_number().await, Some(14));
		assert_eq!(
			db.query(&first_peer_id, &first_para_id).await.unwrap(),
			Score::new(10).unwrap()
		);
		assert_eq!(
			db.query(&second_peer_id, &first_para_id).await.unwrap(),
			Score::new(10).unwrap()
		);
		assert_eq!(
			db.query(&first_peer_id, &second_para_id).await.unwrap(),
			Score::new(5).unwrap()
		);

		// Empty update with decay has no effect.
		assert!(db
			.process_bumps(15, Default::default(), Some(Score::new(1).unwrap()))
			.await
			.is_empty());
		assert_eq!(db.processed_finalized_block_number().await, Some(15));
		assert_eq!(para_count(&db), 2);
		assert_eq!(
			db.query(&first_peer_id, &first_para_id).await.unwrap(),
			Score::new(10).unwrap()
		);
		assert_eq!(
			db.query(&second_peer_id, &first_para_id).await.unwrap(),
			Score::new(10).unwrap()
		);
		assert_eq!(
			db.query(&first_peer_id, &second_para_id).await.unwrap(),
			Score::new(5).unwrap()
		);

		// Test a subsequent update with decay.
		assert_eq!(
			db.process_bumps(
				16,
				[
					(
						first_para_id,
						[(first_peer_id, Score::new(10).unwrap())].into_iter().collect()
					),
					(
						second_para_id,
						[(second_peer_id, Score::new(10).unwrap())].into_iter().collect()
					),
				]
				.into_iter()
				.collect(),
				Some(Score::new(1).unwrap())
			)
			.await,
			vec![
				ReputationUpdate {
					peer_id: first_peer_id,
					para_id: first_para_id,
					kind: ReputationUpdateKind::Bump,
					value: Score::new(10).unwrap()
				},
				ReputationUpdate {
					peer_id: second_peer_id,
					para_id: first_para_id,
					kind: ReputationUpdateKind::Slash,
					value: Score::new(1).unwrap()
				},
				ReputationUpdate {
					peer_id: second_peer_id,
					para_id: second_para_id,
					kind: ReputationUpdateKind::Bump,
					value: Score::new(10).unwrap()
				},
				ReputationUpdate {
					peer_id: first_peer_id,
					para_id: second_para_id,
					kind: ReputationUpdateKind::Slash,
					value: Score::new(1).unwrap()
				},
			]
		);
		assert_eq!(db.processed_finalized_block_number().await, Some(16));
		assert_eq!(para_count(&db), 2);
		assert_eq!(
			db.query(&first_peer_id, &first_para_id).await.unwrap(),
			Score::new(20).unwrap()
		);
		assert_eq!(
			db.query(&second_peer_id, &first_para_id).await.unwrap(),
			Score::new(9).unwrap()
		);
		assert_eq!(
			db.query(&first_peer_id, &second_para_id).await.unwrap(),
			Score::new(4).unwrap()
		);
		assert_eq!(
			db.query(&second_peer_id, &second_para_id).await.unwrap(),
			Score::new(10).unwrap()
		);

		// Test a decay that makes the reputation go to 0 (The peer's entry will be removed)
		assert_eq!(
			db.process_bumps(
				17,
				[(
					second_para_id,
					[(second_peer_id, Score::new(10).unwrap())].into_iter().collect()
				),]
				.into_iter()
				.collect(),
				Some(Score::new(5).unwrap())
			)
			.await,
			vec![
				ReputationUpdate {
					peer_id: second_peer_id,
					para_id: second_para_id,
					kind: ReputationUpdateKind::Bump,
					value: Score::new(10).unwrap()
				},
				ReputationUpdate {
					peer_id: first_peer_id,
					para_id: second_para_id,
					kind: ReputationUpdateKind::Slash,
					value: Score::new(4).unwrap()
				}
			]
		);
		assert_eq!(db.processed_finalized_block_number().await, Some(17));
		assert_eq!(para_count(&db), 2);
		assert_eq!(
			db.query(&first_peer_id, &first_para_id).await.unwrap(),
			Score::new(20).unwrap()
		);
		assert_eq!(
			db.query(&second_peer_id, &first_para_id).await.unwrap(),
			Score::new(9).unwrap()
		);
		assert_eq!(db.query(&first_peer_id, &second_para_id).await, None);
		assert_eq!(
			db.query(&second_peer_id, &second_para_id).await.unwrap(),
			Score::new(20).unwrap()
		);

		// Test an update which ends up pruning least recently used entries. The per-para limit is
		// 10.
		let mut db = PersistentDb::new(test_db(), 0, 10);
		let peer_ids = (0..10).map(|_| PeerId::random()).collect::<Vec<_>>();

		// Add an equal reputation for all peers.
		assert_eq!(
			db.process_bumps(
				1,
				[(
					first_para_id,
					peer_ids.iter().map(|peer_id| (*peer_id, Score::new(10).unwrap())).collect()
				)]
				.into_iter()
				.collect(),
				None,
			)
			.await
			.len(),
			10
		);
		assert_eq!(para_count(&db), 1);

		for peer_id in peer_ids.iter() {
			assert_eq!(db.query(peer_id, &first_para_id).await.unwrap(), Score::new(10).unwrap());
		}

		// Now sleep for one second and then bump the reputations of all peers except for the one
		// with 4th index. We need to sleep so that the update time of the 4th peer is older than
		// the rest.
		tokio::time::sleep(Duration::from_millis(100)).await;
		assert_eq!(
			db.process_bumps(
				2,
				[(
					first_para_id,
					peer_ids
						.iter()
						.enumerate()
						.filter_map(
							|(i, peer_id)| (i != 4).then_some((*peer_id, Score::new(10).unwrap()))
						)
						.collect()
				)]
				.into_iter()
				.collect(),
				Some(Score::new(5).unwrap()),
			)
			.await
			.len(),
			10
		);

		for (i, peer_id) in peer_ids.iter().enumerate() {
			if i == 4 {
				assert_eq!(
					db.query(peer_id, &first_para_id).await.unwrap(),
					Score::new(5).unwrap()
				);
			} else {
				assert_eq!(
					db.query(peer_id, &first_para_id).await.unwrap(),
					Score::new(20).unwrap()
				);
			}
		}

		// Now add a 11th peer. It should evict the 4th peer.
		let new_peer = PeerId::random();
		tokio::time::sleep(Duration::from_millis(100)).await;
		assert_eq!(
			db.process_bumps(
				3,
				[(first_para_id, [(new_peer, Score::new(10).unwrap())].into_iter().collect())]
					.into_iter()
					.collect(),
				Some(Score::new(5).unwrap()),
			)
			.await
			.len(),
			11
		);
		for (i, peer_id) in peer_ids.iter().enumerate() {
			if i == 4 {
				assert_eq!(db.query(peer_id, &first_para_id).await, None);
			} else {
				assert_eq!(
					db.query(peer_id, &first_para_id).await.unwrap(),
					Score::new(15).unwrap()
				);
			}
		}
		assert_eq!(db.query(&new_peer, &first_para_id).await.unwrap(), Score::new(10).unwrap());

		// Now try adding yet another peer. The decay would naturally evict the new peer so no need
		// to evict the least recently bumped.
		let yet_another_peer = PeerId::random();
		assert_eq!(
			db.process_bumps(
				4,
				[(
					first_para_id,
					[(yet_another_peer, Score::new(10).unwrap())].into_iter().collect()
				)]
				.into_iter()
				.collect(),
				Some(Score::new(10).unwrap()),
			)
			.await
			.len(),
			11
		);
		for (i, peer_id) in peer_ids.iter().enumerate() {
			if i == 4 {
				assert_eq!(db.query(peer_id, &first_para_id).await, None);
			} else {
				assert_eq!(
					db.query(peer_id, &first_para_id).await.unwrap(),
					Score::new(5).unwrap()
				);
			}
		}
		assert_eq!(db.query(&new_peer, &first_para_id).await, None);
		assert_eq!(
			db.query(&yet_another_peer, &first_para_id).await,
			Some(Score::new(10).unwrap())
		);
	}
}
