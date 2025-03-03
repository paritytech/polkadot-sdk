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

use polkadot_node_network_protocol::PeerId;
use polkadot_primitives::Id as ParaId;
use std::collections::BTreeSet;

use crate::validator_side::common::{DisconnectedPeers, PeerState, ReputationBump, Score};
use connected_peers::ConnectedPeers;
use db::ReputationDb;

mod connected_peers;
mod db;

#[derive(Default)]
pub struct PeerManager {
	reputation_db: ReputationDb,
	connected_peers: ConnectedPeers,
}

impl PeerManager {
	pub fn scheduled_paras_update(
		&mut self,
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
		// TODO: should we have them sorted or shuffle them at this point?
		for peer_id in old_connected_peers.peer_ids() {
			peers_to_disconnect
				.extend(self.connected_peers.try_add(&self.reputation_db, *peer_id).into_iter());
			if !self.connected_peers.contains(peer_id) {
				peers_to_disconnect.push(*peer_id);
			}
		}

		self.connected_peers.disconnect(peers_to_disconnect)
	}

	pub fn declared(&mut self, peer_id: PeerId, para_id: ParaId) {
		self.connected_peers.declared(peer_id, para_id);
	}

	pub fn process_bumps(&mut self, rep_bumps: Vec<ReputationBump>) {
		for bump in rep_bumps {
			self.connected_peers.bump_rep(bump);
			// TODO: also add bumps to reputation db
		}
	}

	pub fn process_decrease(&mut self, rep_bumps: Vec<ReputationBump>) {
		for bump in rep_bumps {
			self.connected_peers.decrease_rep(bump);
			// TODO: also add bumps to reputation db
		}
	}

	pub fn handle_disconnected(&mut self, peer_id: PeerId) {
		self.connected_peers.disconnected(peer_id);
	}

	pub fn try_accept(&mut self, peer_id: PeerId) -> DisconnectedPeers {
		let peers_to_disconnect = self.connected_peers.try_add(&self.reputation_db, peer_id);
		self.connected_peers.disconnect(peers_to_disconnect)
	}

	pub fn peer_state(&self, peer_id: &PeerId) -> Option<&PeerState> {
		self.connected_peers.peer_state(peer_id)
	}

	pub fn connected_peer_rep(&self, para_id: &ParaId, peer_id: &PeerId) -> Option<Score> {
		self.connected_peers.peer_rep(para_id, peer_id)
	}
}
