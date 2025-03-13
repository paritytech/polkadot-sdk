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
	validator_side::{
		common::{
			DeclarationOutcome, DisconnectedPeers, PeerState, Score, CONNECTED_PEERS_LIMIT,
			CONNECTED_PER_PARA_LIMIT,
		},
		peer_manager::{ReputationDb, ReputationUpdate, ReputationUpdateKind},
	},
	LOG_TARGET,
};
use polkadot_node_network_protocol::{peer_set::PeerSet, PeerId};
use polkadot_node_subsystem::{messages::NetworkBridgeTxMessage, CollatorProtocolSenderTrait};
use polkadot_primitives::Id as ParaId;
use std::collections::{BTreeMap, BTreeSet, HashMap};

#[derive(Default)]
pub struct ConnectedPeers {
	per_paraid: BTreeMap<ParaId, PerParaId>,
	peers: HashMap<PeerId, PeerState>,
}

impl ConnectedPeers {
	pub fn new(scheduled_paras: BTreeSet<ParaId>) -> Self {
		let per_para_limit = std::cmp::min(
			(CONNECTED_PEERS_LIMIT as usize).checked_div(scheduled_paras.len()).unwrap_or(0),
			CONNECTED_PER_PARA_LIMIT as usize,
		);

		let mut per_para = BTreeMap::new();
		for para_id in scheduled_paras {
			per_para.insert(
				para_id,
				PerParaId { limit: per_para_limit as usize, scores: HashMap::new() },
			);
		}

		Self { per_paraid: per_para, peers: Default::default() }
	}

	pub fn peers<'a>(&'a self) -> impl Iterator<Item = (&'a PeerId, &'a PeerState)> + 'a {
		self.peers.iter()
	}

	pub fn contains(&self, peer_id: &PeerId) -> bool {
		self.peers.contains_key(peer_id)
	}

	pub async fn disconnect<Sender: CollatorProtocolSenderTrait>(
		&mut self,
		sender: &mut Sender,
		mut peers_to_disconnect: Vec<PeerId>,
	) -> DisconnectedPeers {
		peers_to_disconnect.retain(|peer| !self.contains(&peer));
		for peer in peers_to_disconnect.iter() {
			self.peers.remove(peer);
		}

		gum::trace!(
			target: LOG_TARGET,
			?peers_to_disconnect,
			"Disconnecting peers",
		);
		sender
			.send_message(NetworkBridgeTxMessage::DisconnectPeers(
				peers_to_disconnect.clone(),
				PeerSet::Collation,
			))
			.await;

		peers_to_disconnect
	}

	pub fn disconnected(&mut self, peer_id: PeerId) {
		for per_para_id in self.per_paraid.values_mut() {
			per_para_id.scores.remove(&peer_id);
		}

		self.peers.remove(&peer_id);
	}

	pub fn assigned_paras<'a>(&'a self) -> impl Iterator<Item = ParaId> + 'a {
		self.per_paraid.keys().copied()
	}

	pub fn try_add(
		&mut self,
		reputation_db: &ReputationDb,
		peer_id: PeerId,
		already_declared: Option<ParaId>,
	) -> DisconnectedPeers {
		if self.contains(&peer_id) {
			return vec![]
		}

		let mut kept = false;
		let mut peers_to_disconnect = vec![];

		match already_declared {
			Some(para_id) => {
				let past_reputation = reputation_db.query(&peer_id, &para_id).unwrap_or(0);
				if let Some(per_para_id) = self.per_paraid.get_mut(&para_id) {
					let res = per_para_id.try_add(peer_id, past_reputation);
					if res.0 {
						kept = true;

						if let Some(to_disconnect) = res.1 {
							peers_to_disconnect.push(to_disconnect);
						}
					}
				}
			},
			None =>
				for (para_id, per_para_id) in self.per_paraid.iter_mut() {
					let past_reputation = reputation_db.query(&peer_id, para_id).unwrap_or(0);
					let res = per_para_id.try_add(peer_id, past_reputation);
					if res.0 {
						kept = true;

						if let Some(to_disconnect) = res.1 {
							peers_to_disconnect.push(to_disconnect);
						}
					}
				},
		}

		if !kept {
			peers_to_disconnect.push(peer_id);
		} else {
			let peer_state = match already_declared {
				Some(para_id) => PeerState::Collating(para_id),
				None => PeerState::Connected,
			};
			self.peers.insert(peer_id, peer_state);
		}

		peers_to_disconnect
	}

	pub async fn declared<Sender: CollatorProtocolSenderTrait>(
		&mut self,
		sender: &mut Sender,
		peer_id: PeerId,
		para_id: ParaId,
	) -> DeclarationOutcome {
		let mut outcome = DeclarationOutcome::Disconnected;

		let Some(state) = self.peers.get_mut(&peer_id) else { return outcome };

		match state {
			PeerState::Connected =>
				for (para, per_para_id) in self.per_paraid.iter_mut() {
					if para != &para_id {
						per_para_id.scores.remove(&peer_id);
					} else {
						outcome = DeclarationOutcome::Accepted;
					}
				},
			PeerState::Collating(old_para_id) if old_para_id == &para_id => {
				// Redundant, already collating for this para.
				outcome = DeclarationOutcome::Accepted;
			},
			PeerState::Collating(old_para_id) => {
				if let Some(old_per_paraid) = self.per_paraid.get_mut(&old_para_id) {
					old_per_paraid.scores.remove(&peer_id);
				}
				if let Some(per_para_id) = self.per_paraid.get(&para_id) {
					if per_para_id.scores.contains_key(&peer_id) {
						outcome = DeclarationOutcome::Switched(*old_para_id);
					}
				}
			},
		}

		if matches!(outcome, DeclarationOutcome::Disconnected) {
			self.disconnect(sender, vec![peer_id]).await;
		} else {
			*state = PeerState::Collating(para_id);
		}

		outcome
	}

	pub fn peer_state(&self, peer_id: &PeerId) -> Option<&PeerState> {
		self.peers.get(&peer_id)
	}

	pub fn update_rep(&mut self, update: &ReputationUpdate) {
		let Some(per_para) = self.per_paraid.get_mut(&update.para_id) else { return };
		let Some(score) = per_para.scores.get_mut(&update.peer_id) else { return };

		*score = match update.kind {
			ReputationUpdateKind::Slash => score.saturating_sub(update.value),
			ReputationUpdateKind::Bump => score.saturating_add(update.value),
		};
	}

	pub fn peer_rep(&self, para_id: &ParaId, peer_id: &PeerId) -> Option<Score> {
		self.per_paraid.get(para_id).unwrap().scores.get(peer_id).copied()
	}
}

#[derive(Default)]
struct PerParaId {
	limit: usize,
	// TODO: Probably implement the priority queue using a min-heap
	scores: HashMap<PeerId, Score>,
}

impl PerParaId {
	// TODO: this should return some custom Outcome type
	fn try_add(&mut self, peer_id: PeerId, reputation: Score) -> (bool, Option<PeerId>) {
		// If we've got enough room, add it. Otherwise, see if it has a higher reputation than any
		// other connected peer.
		if self.scores.len() < self.limit {
			self.scores.insert(peer_id, reputation);
			(true, None)
		} else {
			let Some(min_score) = self.min_score() else { return (false, None) };

			if min_score >= reputation {
				(false, None)
			} else {
				self.scores.insert(peer_id, reputation);
				(true, self.pop_min_score().map(|x| x.0))
			}
		}
	}

	fn min_score(&self) -> Option<Score> {
		self.scores.values().min().copied()
	}

	fn pop_min_score(&mut self) -> Option<(PeerId, Score)> {
		self.scores
			.iter()
			.min_by_key(|(_peer_id, score)| **score)
			.map(|(k, v)| (*k, *v))
	}
}
