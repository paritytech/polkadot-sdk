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

use crate::validator_side_experimental::{
	common::Score,
	peer_manager::{backend::Backend, ReputationUpdate, ReputationUpdateKind},
};
use async_trait::async_trait;
use polkadot_node_network_protocol::PeerId;
use polkadot_primitives::{BlockNumber, Hash, Id as ParaId};
use std::{
	collections::{btree_map, hash_map, BTreeMap, BTreeSet, HashMap},
	time::{SystemTime, UNIX_EPOCH},
};

/// This is an in-memory temporary implementation for the DB, to be used only for prototyping and
/// testing purposes.
pub struct Db {
	db: BTreeMap<ParaId, HashMap<PeerId, ScoreEntry>>,
	last_finalized: Option<BlockNumber>,
	stored_limit_per_para: u8,
}

impl Db {
	/// Create a new instance of the in-memory DB.
	///
	/// `stored_limit_per_para` is the maximum number of reputations that can be stored per para.
	pub async fn new(stored_limit_per_para: u8) -> Self {
		Self { db: BTreeMap::new(), last_finalized: None, stored_limit_per_para }
	}
}

type Timestamp = u128;

#[derive(Clone, Debug)]
struct ScoreEntry {
	score: Score,
	last_bumped: Timestamp,
}

#[async_trait]
impl Backend for Db {
	async fn processed_finalized_block_number(&self) -> Option<BlockNumber> {
		self.last_finalized
	}

	async fn query(&self, peer_id: &PeerId, para_id: &ParaId) -> Option<Score> {
		self.db.get(para_id).and_then(|per_para| per_para.get(peer_id).map(|e| e.score))
	}

	async fn slash(&mut self, peer_id: &PeerId, para_id: &ParaId, value: Score) {
		if let btree_map::Entry::Occupied(mut per_para_entry) = self.db.entry(*para_id) {
			if let hash_map::Entry::Occupied(mut e) = per_para_entry.get_mut().entry(*peer_id) {
				let score = e.get_mut().score;
				// Remove the entry if it goes to zero.
				if score <= value {
					e.remove();
				} else {
					e.get_mut().score.saturating_sub(value.into());
				}
			}

			// If the per_para length went to 0, remove it completely
			if per_para_entry.get().is_empty() {
				per_para_entry.remove();
			}
		}
	}

	async fn prune_paras(&mut self, registered_paras: BTreeSet<ParaId>) {
		self.db.retain(|para, _| registered_paras.contains(&para));
	}

	async fn process_bumps(
		&mut self,
		leaf_number: BlockNumber,
		bumps: BTreeMap<ParaId, HashMap<PeerId, Score>>,
		decay_value: Option<Score>,
	) -> Vec<ReputationUpdate> {
		if self.last_finalized.unwrap_or(0) >= leaf_number {
			return vec![]
		}

		self.last_finalized = Some(leaf_number);
		self.bump_reputations(bumps, decay_value)
	}
}

impl Db {
	fn bump_reputations(
		&mut self,
		bumps: BTreeMap<ParaId, HashMap<PeerId, Score>>,
		maybe_decay_value: Option<Score>,
	) -> Vec<ReputationUpdate> {
		let mut reported_updates = vec![];
		let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_millis();

		for (para, bumps_per_para) in bumps {
			reported_updates.reserve(bumps_per_para.len());

			for (peer_id, bump) in bumps_per_para.iter() {
				if u16::from(*bump) == 0 {
					continue
				}

				self.db
					.entry(para)
					.or_default()
					.entry(*peer_id)
					.and_modify(|e| {
						e.score.saturating_add(u16::from(*bump));
						e.last_bumped = now;
					})
					.or_insert(ScoreEntry { score: *bump, last_bumped: now });

				reported_updates.push(ReputationUpdate {
					peer_id: *peer_id,
					para_id: para,
					value: *bump,
					kind: ReputationUpdateKind::Bump,
				});
			}

			if let btree_map::Entry::Occupied(mut per_para_entry) = self.db.entry(para) {
				if let Some(decay_value) = maybe_decay_value {
					let peers_to_slash = per_para_entry
						.get()
						.keys()
						.filter(|peer_id| !bumps_per_para.contains_key(peer_id))
						.copied()
						.collect::<Vec<PeerId>>();

					for peer_id in peers_to_slash {
						if let hash_map::Entry::Occupied(mut e) =
							per_para_entry.get_mut().entry(peer_id)
						{
							// Remove the entry if it goes to zero.
							if e.get_mut().score <= decay_value {
								let score = e.remove().score;
								reported_updates.push(ReputationUpdate {
									peer_id,
									para_id: para,
									value: score,
									kind: ReputationUpdateKind::Slash,
								});
							} else {
								e.get_mut().score.saturating_sub(decay_value.into());
								reported_updates.push(ReputationUpdate {
									peer_id,
									para_id: para,
									value: decay_value,
									kind: ReputationUpdateKind::Slash,
								});
							}
						}
					}
				}

				let per_para_limit = self.stored_limit_per_para as usize;
				if per_para_entry.get().is_empty() {
					// If the per_para length went to 0, remove it completely
					per_para_entry.remove();
				} else if per_para_entry.get().len() > per_para_limit {
					// We have exceeded the maximum capacity, in which case we need to prune
					// the least recently bumped values
					let diff = per_para_entry.get().len() - per_para_limit;
					Self::prune_for_para(&para, &mut per_para_entry, diff, &mut reported_updates);
				}
			}
		}

		reported_updates
	}

	fn prune_for_para(
		para_id: &ParaId,
		per_para: &mut btree_map::OccupiedEntry<ParaId, HashMap<PeerId, ScoreEntry>>,
		diff: usize,
		reported_updates: &mut Vec<ReputationUpdate>,
	) {
		for _ in 0..diff {
			let (peer_id_to_remove, score) = per_para
				.get()
				.iter()
				.min_by_key(|(_peer, entry)| entry.last_bumped)
				.map(|(peer, entry)| (*peer, entry.score))
				.expect("We know there are enough reps over the limit");

			per_para.get_mut().remove(&peer_id_to_remove);

			reported_updates.push(ReputationUpdate {
				peer_id: peer_id_to_remove,
				para_id: *para_id,
				value: score,
				kind: ReputationUpdateKind::Slash,
			});
		}
	}

	#[cfg(test)]
	fn len(&self) -> usize {
		self.db.len()
	}
}

#[cfg(test)]
mod tests {
	use std::time::Duration;

	use super::*;

	#[tokio::test]
	// Test different types of reputation updates and their effects.
	async fn test_reputation_updates() {
		let mut db = Db::new(10).await;
		assert_eq!(db.processed_finalized_block_number().await, None);
		assert_eq!(db.len(), 0);

		// Test empty update with no decay.
		assert!(db.process_bumps(10, Default::default(), None).await.is_empty());
		assert_eq!(db.processed_finalized_block_number().await, Some(10));
		assert_eq!(db.len(), 0);

		// Test a query on a non-existant entry.
		assert_eq!(db.query(&PeerId::random(), &ParaId::from(1000)).await, None);

		// Test empty update with decay.
		assert!(db
			.process_bumps(11, Default::default(), Some(Score::new(1).unwrap()))
			.await
			.is_empty());
		assert_eq!(db.processed_finalized_block_number().await, Some(11));
		assert_eq!(db.len(), 0);

		// Test empty update with a leaf number smaller than the latest one.
		assert!(db
			.process_bumps(5, Default::default(), Some(Score::new(1).unwrap()))
			.await
			.is_empty());
		assert_eq!(db.processed_finalized_block_number().await, Some(11));
		assert_eq!(db.len(), 0);

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
		assert_eq!(db.len(), 0);

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
		assert_eq!(db.len(), 0);
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
		assert_eq!(db.len(), 1);
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
		assert_eq!(db.len(), 1);
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
		assert_eq!(db.len(), 2);
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
		assert_eq!(db.len(), 2);
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
		assert_eq!(db.len(), 2);
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
		assert_eq!(db.len(), 2);
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
		let mut db = Db::new(10).await;
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
		assert_eq!(db.len(), 1);

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

	#[tokio::test]
	// Test reputation slashes.
	async fn test_slash() {
		let mut db = Db::new(10).await;

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
		assert_eq!(db.len(), 2);
	}

	#[tokio::test]
	// Test para pruning.
	async fn test_prune_paras() {
		let mut db = Db::new(10).await;

		db.prune_paras(BTreeSet::new()).await;
		assert_eq!(db.len(), 0);

		db.prune_paras([ParaId::from(100), ParaId::from(200)].into_iter().collect())
			.await;
		assert_eq!(db.len(), 0);

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
		assert_eq!(db.len(), 3);

		// Registered paras include the existing ones. Does nothing
		db.prune_paras(
			[ParaId::from(100), ParaId::from(200), ParaId::from(300), ParaId::from(400)]
				.into_iter()
				.collect(),
		)
		.await;
		assert_eq!(db.len(), 3);

		assert_eq!(db.query(&peer_id, &ParaId::from(100)).await.unwrap(), Score::new(10).unwrap());
		assert_eq!(
			db.query(&another_peer_id, &ParaId::from(200)).await.unwrap(),
			Score::new(12).unwrap()
		);
		assert_eq!(db.query(&peer_id, &ParaId::from(300)).await.unwrap(), Score::new(15).unwrap());

		// Prunes multiple paras.
		db.prune_paras([ParaId::from(300)].into_iter().collect()).await;
		assert_eq!(db.len(), 1);
		assert_eq!(db.query(&peer_id, &ParaId::from(100)).await, None);
		assert_eq!(db.query(&another_peer_id, &ParaId::from(200)).await, None);
		assert_eq!(db.query(&peer_id, &ParaId::from(300)).await.unwrap(), Score::new(15).unwrap());

		// Prunes all paras.
		db.prune_paras(BTreeSet::new()).await;
		assert_eq!(db.len(), 0);
		assert_eq!(db.query(&peer_id, &ParaId::from(300)).await, None);
	}
}
