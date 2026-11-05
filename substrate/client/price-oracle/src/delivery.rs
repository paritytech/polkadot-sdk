// This file is part of Substrate.

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

//! Delivery of price reports to the coming block authors.

use crate::LOG_TARGET;
use sc_network::{
	config::{NonReservedPeerMode, SetConfig},
	service::traits::{NetworkService, NotificationService},
	Multiaddr, NetworkBackend, NotificationMetrics, PeerId, ProtocolName,
};
use sp_runtime::traits::Block as BlockT;
use std::{collections::HashSet, future::Future, sync::Arc};

/// Suffix of the protocol name, after the genesis hash and the optional fork id.
const PROTOCOL_SUFFIX: &str = "/price-oracle/1";
/// Largest message accepted, in bytes.
const MAX_MESSAGE_SIZE: u64 = 64 * 1024;
/// Inbound slots of the protocol. Every signer opens a substream to each author it sends to, so
/// an author needs a slot per signer. Sized for a few hundred signers, with room for peers that
/// are no longer, or not yet, signers.
const IN_PEERS: u32 = 512;

/// Name of the protocol: `/{genesis_hash}[/{fork_id}]/price-oracle/1`.
pub fn protocol_name<Hash: AsRef<[u8]>>(genesis_hash: Hash, fork_id: Option<&str>) -> ProtocolName {
	let genesis_hash = array_bytes::bytes2hex("", genesis_hash.as_ref());
	match fork_id {
		Some(fork_id) => format!("/{genesis_hash}/{fork_id}{PROTOCOL_SUFFIX}").into(),
		None => format!("/{genesis_hash}{PROTOCOL_SUFFIX}").into(),
	}
}

/// Register the protocol with the network.
///
/// Returns the protocol configuration to add to the network configuration, the notification
/// service and the protocol name to hand to the service. The node dials only the peers the
/// service reserves, and accepts inbound substreams from anyone.
pub fn peers_set_config<Block: BlockT, Net: NetworkBackend<Block, Block::Hash>>(
	genesis_hash: Block::Hash,
	fork_id: Option<&str>,
	metrics: NotificationMetrics,
	peer_store: Arc<dyn sc_network::peer_store::PeerStoreProvider>,
) -> (Net::NotificationProtocolConfig, Box<dyn NotificationService>, ProtocolName) {
	let name = protocol_name(genesis_hash, fork_id);
	let (config, notification_service) = Net::notification_config(
		name.clone(),
		Vec::new(),
		MAX_MESSAGE_SIZE,
		None,
		SetConfig {
			in_peers: IN_PEERS,
			out_peers: 0,
			reserved_nodes: Vec::new(),
			non_reserved_mode: NonReservedPeerMode::Accept,
		},
		metrics,
		peer_store,
	);
	(config, notification_service, name)
}

/// Where the reports of a tick are sent.
pub trait ReportRecipients<Block: BlockT>: Send + Sync {
	/// The peer addresses of the authorities expected to author soon, as of `best`. Every
	/// address ends with the `/p2p/` component of its peer.
	fn recipients(&self, best: Block::Hash) -> impl Future<Output = Vec<Multiaddr>> + Send;
}

/// The peers the reports go to.
///
/// The recipients of a tick are reserved on the protocol, so the network keeps a substream open
/// to each of them. A report is sent to the reserved peers whose substream is open at that
/// moment.
pub struct Targets {
	/// The network the peers are reserved on.
	network: Arc<dyn NetworkService>,
	/// The protocol the peers are reserved for.
	protocol: ProtocolName,
	/// The peer id of this node, never a target.
	local_peer_id: PeerId,
	/// The addresses reserved on the protocol.
	reserved: HashSet<Multiaddr>,
	/// The peers of the reserved addresses.
	peers: HashSet<PeerId>,
	/// Peers with an open substream of the protocol, in either direction.
	open: HashSet<PeerId>,
}

impl Targets {
	/// No targets yet.
	pub fn new(network: Arc<dyn NetworkService>, protocol: ProtocolName) -> Self {
		let local_peer_id = network.local_peer_id();
		Self {
			network,
			protocol,
			local_peer_id,
			reserved: HashSet::new(),
			peers: HashSet::new(),
			open: HashSet::new(),
		}
	}

	/// Reserve `addresses` on the protocol in place of the current ones. Addresses without a peer
	/// id and those of the local peer are dropped. The network is only told when the set
	/// changed; if it refuses, the current reservation stays.
	pub fn reserve(&mut self, addresses: Vec<Multiaddr>) {
		let (reserved, peers) = select_peers(addresses, &self.local_peer_id);
		if reserved == self.reserved {
			return;
		}
		match self.network.set_reserved_peers(self.protocol.clone(), reserved.clone()) {
			Ok(()) => {
				log::debug!(target: LOG_TARGET, "Reports go to {} peers", peers.len());
				self.reserved = reserved;
				self.peers = peers;
			},
			Err(e) => log::warn!(target: LOG_TARGET, "Cannot reserve the recipients: {e}"),
		}
	}

	/// A substream of the protocol opened with `peer`.
	pub fn opened(&mut self, peer: PeerId) {
		self.open.insert(peer);
	}

	/// The substream of the protocol with `peer` closed.
	pub fn closed(&mut self, peer: &PeerId) {
		self.open.remove(peer);
	}

	/// The targets a report can be sent to now.
	pub fn reachable(&self) -> impl Iterator<Item = &PeerId> {
		self.peers.iter().filter(|peer| self.open.contains(peer))
	}

	/// The number of targets.
	pub fn len(&self) -> usize {
		self.peers.len()
	}
}

/// The addresses worth reserving, and their peers: those with a peer id other than
/// `local`.
fn select_peers(
	addresses: Vec<Multiaddr>,
	local: &PeerId,
) -> (HashSet<Multiaddr>, HashSet<PeerId>) {
	let mut reserved = HashSet::new();
	let mut peers = HashSet::new();
	for address in addresses {
		match PeerId::try_from_multiaddr(&address) {
			Some(peer) if peer != *local => {
				reserved.insert(address);
				peers.insert(peer);
			},
			_ => {},
		}
	}
	(reserved, peers)
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn select_peers_drops_the_local_peer_and_addresses_without_one() {
		let (local, other) = (PeerId::random(), PeerId::random());
		let with = |peer: &PeerId| -> Multiaddr {
			format!("/ip4/127.0.0.1/tcp/30333/p2p/{peer}").parse().unwrap()
		};
		let without: Multiaddr = "/ip4/127.0.0.1/tcp/30333".parse().unwrap();
		let (reserved, peers) =
			select_peers(vec![with(&local), with(&other), without, with(&other)], &local);
		assert_eq!(reserved, HashSet::from([with(&other)]));
		assert_eq!(peers, HashSet::from([other]));
	}

	#[test]
	fn protocol_name_includes_fork_id() {
		assert_eq!(protocol_name([0xab, 0xcd], None).to_string(), "/abcd/price-oracle/1");
		assert_eq!(
			protocol_name([0xab, 0xcd], Some("fork")).to_string(),
			"/abcd/fork/price-oracle/1"
		);
	}
}
