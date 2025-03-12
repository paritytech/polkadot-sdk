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

use crate::validator_side::common::{
	DeclarationOutcome, DisconnectedPeers, PeerState, ReputationUpdate, Score,
};
use connected_peers::ConnectedPeers;
use db::ReputationDb;
use polkadot_node_network_protocol::PeerId;
use polkadot_node_subsystem::CollatorProtocolSenderTrait;
use polkadot_primitives::Id as ParaId;
use std::collections::{BTreeMap, BTreeSet, HashMap};

use super::common::ReputationUpdateKind;

mod connected_peers;
mod db;

#[derive(Default)]
pub struct PeerManager {
	pub reputation_db: ReputationDb,
	connected_peers: ConnectedPeers,
}

impl PeerManager {
	pub async fn scheduled_paras_update<Sender: CollatorProtocolSenderTrait>(
		&mut self,
		sender: &mut Sender,
		scheduled_paras: BTreeSet<ParaId>,
	) -> DisconnectedPeers {
		let old_scheduled_paras = self.connected_peers.assigned_paras().collect::<BTreeSet<_>>();
		if old_scheduled_paras == scheduled_paras {
			// Nothing to do if the scheduled paras didn't change.
			return vec![]
		}

		let mut connected_peers = ConnectedPeers::new(scheduled_paras);

		std::mem::swap(&mut connected_peers, &mut self.connected_peers);
		let old_connected_peers = connected_peers;

		let mut peers_to_disconnect = vec![];
		// See which of the old peers we should keep.
		for (peer_id, peer_state) in old_connected_peers.peers() {
			let already_collating = peer_state.para_id();
			peers_to_disconnect.extend(
				self.connected_peers
					.try_add(&self.reputation_db, *peer_id, already_collating)
					.into_iter(),
			);
		}

		self.connected_peers.disconnect(sender, peers_to_disconnect).await
	}

	pub async fn declared<Sender: CollatorProtocolSenderTrait>(
		&mut self,
		sender: &mut Sender,
		peer_id: PeerId,
		para_id: ParaId,
	) -> DeclarationOutcome {
		self.connected_peers.declared(sender, peer_id, para_id).await
	}

	pub fn slash_reputation(&mut self, peer_id: &PeerId, para_id: &ParaId, value: Score) {
		let update = ReputationUpdate {
			peer_id: *peer_id,
			para_id: *para_id,
			value,
			kind: ReputationUpdateKind::Slash,
		};
		self.reputation_db.slash_reputation(peer_id, para_id, value);
		self.connected_peers.update_rep(&update);
	}

	pub fn update_reputations_on_new_leaf(
		&mut self,
		bumps: BTreeMap<ParaId, HashMap<PeerId, Score>>,
	) {
		let updates = self.reputation_db.bump_reputations(bumps);
		for update in updates {
			self.connected_peers.update_rep(&update);
		}
	}

	pub fn handle_disconnected(&mut self, peer_id: PeerId) {
		self.connected_peers.disconnected(peer_id);
	}

	pub async fn try_accept<Sender: CollatorProtocolSenderTrait>(
		&mut self,
		sender: &mut Sender,
		peer_id: PeerId,
	) -> DisconnectedPeers {
		let peers_to_disconnect = self.connected_peers.try_add(&self.reputation_db, peer_id, None);
		self.connected_peers.disconnect(sender, peers_to_disconnect).await
	}

	pub fn peer_state(&self, peer_id: &PeerId) -> Option<&PeerState> {
		self.connected_peers.peer_state(peer_id)
	}

	pub fn connected_peer_rep(&self, para_id: &ParaId, peer_id: &PeerId) -> Option<Score> {
		self.connected_peers.peer_rep(para_id, peer_id)
	}
}
