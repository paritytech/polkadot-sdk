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

use crate::validator_side::{
	common::{extract_reputation_updates_from_new_leaves, Score, INACTIVITY_SLASH},
	peer_manager::{ReputationUpdate, ReputationUpdateKind},
};
use futures::channel::oneshot;
use polkadot_node_network_protocol::PeerId;
use polkadot_node_subsystem::{messages::ChainApiMessage, CollatorProtocolSenderTrait};
use polkadot_primitives::{BlockNumber, Hash, Id as ParaId};
use std::{
	collections::{btree_map, hash_map, BTreeMap, HashMap},
	time::{SystemTime, UNIX_EPOCH},
};

const MAX_REPS_PER_PARA: usize = 100;
const MAX_INIT_LOOKBACK: usize = 20;

type Timestamp = u64;

#[derive(Clone, Debug)]
struct Entry {
	score: Score,
	last_bumped: Timestamp,
}

// TODO: this needs to be a proper DB, but for prototyping purposes it's fine to keep it in memory.
#[derive(Default, Debug)]
pub struct ReputationDb {
	db: BTreeMap<ParaId, HashMap<PeerId, Entry>>,
	max_height: BlockNumber,
	initialized: bool,
}

// TODO: we need to purge paras that are no longer registered. They won't get purged by decay since
// they stop producing blocks

impl ReputationDb {
	async fn initialize<Sender: CollatorProtocolSenderTrait>(
		&mut self,
		sender: &mut Sender,
		(max_height, max_leaf): (BlockNumber, Hash),
	) -> Vec<ReputationUpdate> {
		self.initialized = true;

		if max_height <= self.max_height {
			return vec![]
		}
		let height_diff = std::cmp::min(max_height - self.max_height, MAX_INIT_LOOKBACK as u32);

		let mut ancestors = get_ancestors(sender, height_diff as usize, max_leaf).await;
		ancestors.reverse();
		let rep_updates = extract_reputation_updates_from_new_leaves(sender, &ancestors[..]).await;

		self.bump_reputations(rep_updates)
	}

	pub fn query(&self, peer_id: &PeerId, para_id: &ParaId) -> Option<Score> {
		self.db.get(para_id).and_then(|per_para| per_para.get(peer_id).map(|e| e.score))
	}

	pub fn slash_reputation(&mut self, peer_id: &PeerId, para_id: &ParaId, value: Score) {
		if let btree_map::Entry::Occupied(mut per_para_entry) = self.db.entry(*para_id) {
			if let hash_map::Entry::Occupied(mut e) = per_para_entry.get_mut().entry(*peer_id) {
				let score = e.get_mut().score;
				// Remove the entry if it goes to zero.
				if score <= value {
					e.remove();
				} else {
					e.get_mut().score = score.saturating_sub(value);
				}
			}

			// If the per_para length went to 0, remove it completely
			if per_para_entry.get().is_empty() {
				per_para_entry.remove();
			}
		}
	}

	pub async fn active_leaves_update<Sender: CollatorProtocolSenderTrait>(
		&mut self,
		sender: &mut Sender,
		bumps: BTreeMap<ParaId, HashMap<PeerId, Score>>,
		(max_height, max_leaf): (BlockNumber, Hash),
	) -> Vec<ReputationUpdate> {
		if !self.initialized {
			let bumps = self.initialize(sender, (max_height, max_leaf)).await;
			self.max_height = std::cmp::max(max_height, self.max_height);
			bumps
		} else {
			self.max_height = std::cmp::max(max_height, self.max_height);
			self.bump_reputations(bumps)
		}
	}

	fn bump_reputations(
		&mut self,
		bumps: BTreeMap<ParaId, HashMap<PeerId, Score>>,
	) -> Vec<ReputationUpdate> {
		let mut reported_updates = vec![];

		for (para, bumps_per_para) in bumps {
			let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs();
			for (peer_id, bump) in bumps_per_para.iter() {
				if *bump == 0 {
					continue
				}

				self.db
					.entry(para)
					.or_default()
					.entry(*peer_id)
					.and_modify(|e| {
						e.score = e.score.saturating_add(*bump);
						e.last_bumped = now;
					})
					.or_insert(Entry { score: *bump, last_bumped: now });

				reported_updates.push(ReputationUpdate {
					peer_id: *peer_id,
					para_id: para,
					value: *bump,
					kind: ReputationUpdateKind::Bump,
				});
			}

			if let btree_map::Entry::Occupied(mut per_para_entry) = self.db.entry(para) {
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
						let score = e.get_mut().score;
						// Remove the entry if it goes to zero.
						if score <= INACTIVITY_SLASH {
							e.remove();
						} else {
							e.get_mut().score = score.saturating_sub(INACTIVITY_SLASH);
						}

						reported_updates.push(ReputationUpdate {
							peer_id,
							para_id: para,
							value: INACTIVITY_SLASH,
							kind: ReputationUpdateKind::Slash,
						});
					}
				}

				if per_para_entry.get().is_empty() {
					// If the per_para length went to 0, remove it completely
					per_para_entry.remove();
				} else if per_para_entry.get().len() > MAX_REPS_PER_PARA {
					// We have exceeded the maximum capacity, in which case we need to prune
					// the least recently bumped values
					let diff = per_para_entry.get().len() - MAX_REPS_PER_PARA;
					Self::prune_for_para(&para, &mut per_para_entry, diff, &mut reported_updates);
				}
			}
		}

		reported_updates
	}

	fn prune_for_para(
		para_id: &ParaId,
		per_para: &mut btree_map::OccupiedEntry<ParaId, HashMap<PeerId, Entry>>,
		diff: usize,
		reported_updates: &mut Vec<ReputationUpdate>,
	) {
		for _ in 0..diff {
			let (peer_id_to_remove, score) = per_para
				.get()
				.iter()
				.min_by_key(|(_peer, entry)| entry.last_bumped)
				.map(|(peer, entry)| (*peer, entry.score))
				.expect("We know we exceeded there are enough reps over the limit");

			per_para.get_mut().remove(&peer_id_to_remove);

			reported_updates.push(ReputationUpdate {
				peer_id: peer_id_to_remove,
				para_id: *para_id,
				value: score,
				kind: ReputationUpdateKind::Slash,
			});
		}
	}
}

async fn get_ancestors<Sender: CollatorProtocolSenderTrait>(
	sender: &mut Sender,
	k: usize,
	hash: Hash,
) -> Vec<Hash> {
	let (tx, rx) = oneshot::channel();
	sender
		.send_message(ChainApiMessage::Ancestors { hash, k, response_channel: tx })
		.await;

	rx.await.unwrap().unwrap()
}
