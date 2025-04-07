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
mod backend;
mod connected;
mod db;

use std::collections::BTreeSet;

use crate::{
	validator_side_experimental::common::{
		Score, CONNECTED_PEERS_LIMIT, CONNECTED_PEERS_PARA_LIMIT,
	},
	LOG_TARGET,
};
pub use backend::Backend;
use connected::ConnectedPeers;
pub use db::Db;
use polkadot_node_network_protocol::{
	peer_set::{CollationVersion, PeerSet},
	PeerId,
};
use polkadot_node_subsystem::{messages::NetworkBridgeTxMessage, CollatorProtocolSenderTrait};
use polkadot_primitives::Id as ParaId;

use super::common::{PeerInfo, PeerState};

struct ReputationUpdate {
	pub peer_id: PeerId,
	pub para_id: ParaId,
	pub value: Score,
	pub kind: ReputationUpdateKind,
}

enum ReputationUpdateKind {
	Bump,
	Slash,
}

enum TryAcceptOutcome {
	Added,
	Replaced(Vec<PeerId>),
	Rejected,
}

impl TryAcceptOutcome {
	fn combine(self, other: Self) -> Self {
		use TryAcceptOutcome::*;
		match (self, other) {
			(Added, Added) => Added,
			(Rejected, Rejected) => Rejected,
			(Added, Rejected) | (Rejected, Added) => Added,
			(Replaced(mut replaced_a), Replaced(mut replaced_b)) => {
				replaced_a.append(&mut replaced_b);
				Replaced(replaced_a)
			},
			(_, Replaced(replaced)) | (Replaced(replaced), _) => Replaced(replaced),
		}
	}
}

enum DeclarationOutcome {
	Rejected,
	Switched(ParaId),
	Accepted,
}

pub struct PeerManager<B> {
	db: B,
	connected: ConnectedPeers,
}

impl<B: Backend> PeerManager<B> {
	/// Process a potential change of the scheduled paras.
	pub async fn scheduled_paras_update<Sender: CollatorProtocolSenderTrait>(
		&mut self,
		sender: &mut Sender,
		scheduled_paras: BTreeSet<ParaId>,
	) {
		let mut prev_paras_count = 0;
		let mut prev_scheduled_paras = self.connected.scheduled_paras();

		if prev_scheduled_paras.all(|p| {
			prev_paras_count += 1;
			scheduled_paras.contains(p)
		}) {
			// The new set is a superset of the old paras and their lengths are equal, so they are
			// identical.

			if prev_paras_count == scheduled_paras.len() {
				// Nothing to do if the scheduled paras didn't change.
				return
			}
		}

		// Borrow checker can't tell that prev_scheduled_paras is not used anymore.
		std::mem::drop(prev_scheduled_paras);

		// Recreate the connected peers based on the new schedule and try populating it again based
		// on their reputations. Disconnect any peers that couldn't be kept
		let mut new_instance =
			ConnectedPeers::new(scheduled_paras, CONNECTED_PEERS_LIMIT, CONNECTED_PEERS_PARA_LIMIT);

		std::mem::swap(&mut new_instance, &mut self.connected);
		let prev_instance = new_instance;
		let (prev_peers, cached_scores) = prev_instance.consume();

		// Build a closure that can be used to first query the in-memory past reputations of the
		// peers before reaching for the DB.
		// TODO: warm-up the DB for these specific paraids.

		// Borrow these for use in the closure.
		let cached_scores = &cached_scores;
		let db = &self.db;
		let reputation_query_fn = |peer_id: PeerId, para_id: ParaId| async move {
			if let Some(cached_score) =
				cached_scores.get(&para_id).and_then(|per_para| per_para.get_score(&peer_id))
			{
				cached_score
			} else {
				db.query(&peer_id, &para_id).await.unwrap_or_default()
			}
		};

		// See which of the old peers we should keep.
		let mut peers_to_disconnect = vec![];
		for (peer_id, peer_info) in prev_peers {
			let outcome = self.connected.try_accept(reputation_query_fn, peer_id, peer_info).await;

			match outcome {
				TryAcceptOutcome::Rejected => {
					peers_to_disconnect.push(peer_id);
				},
				TryAcceptOutcome::Replaced(replaced_peer_ids) => {
					peers_to_disconnect.extend(replaced_peer_ids);
				},
				TryAcceptOutcome::Added => {},
			}
		}

		// Disconnect peers that couldn't be kept.
		self.disconnect_peers(sender, peers_to_disconnect).await;
	}

	/// Process a declaration message of a peer.
	pub async fn declared<Sender: CollatorProtocolSenderTrait>(
		&mut self,
		sender: &mut Sender,
		peer_id: PeerId,
		para_id: ParaId,
	) {
		let outcome = self.connected.declared(peer_id, para_id);

		match outcome {
			DeclarationOutcome::Accepted => {
				gum::debug!(
					target: LOG_TARGET,
					?para_id,
					?peer_id,
					"Peer declared",
				);
			},
			DeclarationOutcome::Switched(old_para_id) => {
				gum::debug!(
					target: LOG_TARGET,
					?para_id,
					?old_para_id,
					?peer_id,
					"Peer switched collating paraid",
				);
			},
			DeclarationOutcome::Rejected => {
				gum::debug!(
					target: LOG_TARGET,
					?para_id,
					?peer_id,
					"Peer declared but rejected. Going to disconnect.",
				);

				self.disconnect_peers(sender, vec![peer_id]).await;
			},
		}
	}

	/// Slash a peer's reputation for this paraid.
	pub async fn slash_reputation(&mut self, peer_id: &PeerId, para_id: &ParaId, value: Score) {
		gum::debug!(
			target: LOG_TARGET,
			?peer_id,
			?para_id,
			?value,
			"Slashing peer's reputation",
		);

		self.db.slash(peer_id, para_id, value).await;
		self.connected.update_reputation(ReputationUpdate {
			peer_id: *peer_id,
			para_id: *para_id,
			value,
			kind: ReputationUpdateKind::Slash,
		});
	}

	/// Handle new leaves update, by updating peer reputations.
	// pub async fn update_reputations_on_new_leaves<Sender: CollatorProtocolSenderTrait>(
	// 	&mut self,
	// 	sender: &mut Sender,
	// 	bumps: BTreeMap<ParaId, HashMap<PeerId, Score>>,
	// 	max_leaf: (BlockNumber, Hash),
	// ) {
	// 	let updates = self.db.active_leaves_update(sender, bumps, max_leaf).await;
	// 	for update in updates {
	// 		self.connected.update_rep(&update);
	// 	}
	// }

	/// Process a peer disconnected event coming from the network.
	pub fn disconnected(&mut self, peer_id: &PeerId) {
		self.connected.remove(peer_id);
	}

	/// A connection was made, triage it. Return whether or not is was kept.
	pub async fn try_accept_connection<Sender: CollatorProtocolSenderTrait>(
		&mut self,
		sender: &mut Sender,
		peer_id: PeerId,
		version: CollationVersion,
	) -> bool {
		let db = &self.db;
		let reputation_query_fn = |peer_id: PeerId, para_id: ParaId| async move {
			// Go straight to the DB. We only store in-memory the reputations of connected peers.
			db.query(&peer_id, &para_id).await.unwrap_or_default()
		};

		let outcome = self
			.connected
			.try_accept(
				reputation_query_fn,
				peer_id,
				PeerInfo { version, state: PeerState::Connected },
			)
			.await;

		match outcome {
			TryAcceptOutcome::Added => true,
			TryAcceptOutcome::Replaced(other_peers) => {
				gum::trace!(
					target: LOG_TARGET,
					"Peer {:?} replaced the connection slots of other peers: {:?}",
					peer_id,
					&other_peers
				);
				self.disconnect_peers(sender, other_peers).await;
				true
			},
			TryAcceptOutcome::Rejected => {
				gum::debug!(
					target: LOG_TARGET,
					?peer_id,
					"Peer connection was rejected",
				);
				self.disconnect_peers(sender, vec![peer_id]).await;
				false
			},
		}
	}

	/// Retrieve the score of the connected peer. We assume the peer is declared for this paraid.
	pub fn connected_peer_score(&self, peer_id: &PeerId, para_id: &ParaId) -> Option<Score> {
		self.connected.peer_score(peer_id, para_id)
	}

	async fn disconnect_peers<Sender: CollatorProtocolSenderTrait>(
		&self,
		sender: &mut Sender,
		peers: Vec<PeerId>,
	) {
		gum::trace!(
			target: LOG_TARGET,
			?peers,
			"Disconnecting peers",
		);

		sender
			.send_message(NetworkBridgeTxMessage::DisconnectPeers(peers, PeerSet::Collation))
			.await;
	}
}
