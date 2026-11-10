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
use std::{
	collections::{HashMap, HashSet, VecDeque},
	future::Future,
	sync::Arc,
};

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
/// to each of them. A report is sent to the recipients whose substream is open at that moment.
///
/// The last few peers that stopped being recipients stay reserved. A substream is shared by both
/// sides, and a node that drops a peer from its reservation closes the substream it opened, while
/// the peer may still be sending its reports over it.
pub struct Targets {
	/// Where the peers are reserved.
	reservation: Box<dyn Reservation>,
	/// The peer id of this node, never a target.
	local_peer_id: PeerId,
	/// The recipients, then the peers that were recipients until recently, the most recent
	/// first, with their addresses.
	peers: VecDeque<(PeerId, HashSet<Multiaddr>)>,
	/// How many of `peers`, from the front, are recipients.
	recipients: usize,
	/// How many peers that stopped being recipients stay reserved.
	keep: usize,
	/// The addresses reserved on the protocol.
	reserved: HashSet<Multiaddr>,
	/// Peers with an open substream of the protocol, in either direction.
	open: HashSet<PeerId>,
}

// Helper trait to mock the network service for testing.
/// The reserved peers of the protocol.
trait Reservation: Send + Sync {
	/// Reserve `addresses` in place of the current ones.
	fn set(&self, addresses: HashSet<Multiaddr>) -> Result<(), String>;
}

impl Reservation for (Arc<dyn NetworkService>, ProtocolName) {
	fn set(&self, addresses: HashSet<Multiaddr>) -> Result<(), String> {
		self.0.set_reserved_peers(self.1.clone(), addresses)
	}
}

impl Targets {
	/// No targets yet. The last `keep` peers that stopped being recipients stay reserved.
	pub fn new(network: Arc<dyn NetworkService>, protocol: ProtocolName, keep: usize) -> Self {
		let local_peer_id = network.local_peer_id();
		Self::with(Box::new((network, protocol)), local_peer_id, keep)
	}

	fn with(reservation: Box<dyn Reservation>, local_peer_id: PeerId, keep: usize) -> Self {
		Self {
			reservation,
			local_peer_id,
			keep,
			peers: VecDeque::new(),
			recipients: 0,
			reserved: HashSet::new(),
			open: HashSet::new(),
		}
	}

	/// Make the peers at `addresses` the recipients. Reserve them and the last `self.keep` past
	/// recipients on the protocol. Addresses without a peer id and those of the local peer are
	/// dropped. The network is only told when the reserved addresses changed.
	pub fn reserve(&mut self, addresses: Vec<Multiaddr>) {
		// Group the addresses by peer, dropping those without a peer id and those of this node.
		let mut recipients: HashMap<PeerId, HashSet<Multiaddr>> = HashMap::new();
		for address in addresses {
			match PeerId::try_from_multiaddr(&address) {
				Some(peer) if peer != self.local_peer_id => {
					recipients.entry(peer).or_default().insert(address);
				},
				_ => {},
			}
		}
		// Put the recipients at the front, ahead of the peers that were recipients before, and
		// keep at most `keep` of them.
		self.recipients = recipients.len();
		self.peers.retain(|(peer, _)| !recipients.contains_key(peer));
		for recipient in recipients {
			self.peers.push_front(recipient);
		}
		self.peers.truncate(self.recipients + self.keep);
		// Reserve the recipients and the kept peers on the network, unless nothing changed.
		let reserved: HashSet<Multiaddr> =
			self.peers.iter().flat_map(|(_, addresses)| addresses).cloned().collect();
		if reserved == self.reserved {
			return;
		}
		match self.reservation.set(reserved.clone()) {
			Ok(()) => {
				log::debug!(
					target: LOG_TARGET,
					"Reports go to {} peers, {} more kept connected",
					self.recipients,
					self.peers.len() - self.recipients,
				);
				self.reserved = reserved;
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
		self.peers
			.iter()
			.take(self.recipients)
			.map(|(peer, _)| peer)
			.filter(|peer| self.open.contains(peer))
	}

	/// The number of targets.
	pub fn len(&self) -> usize {
		self.recipients
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use std::sync::Mutex;

	/// Records every set of addresses reserved.
	#[derive(Default)]
	struct Recorded(Arc<Mutex<Vec<HashSet<Multiaddr>>>>);

	impl Reservation for Recorded {
		fn set(&self, addresses: HashSet<Multiaddr>) -> Result<(), String> {
			self.0.lock().unwrap().push(addresses);
			Ok(())
		}
	}

	fn address(peer: &PeerId) -> Multiaddr {
		format!("/ip4/127.0.0.1/tcp/30333/p2p/{peer}").parse().unwrap()
	}

	fn targets(keep: usize) -> (Targets, PeerId, Arc<Mutex<Vec<HashSet<Multiaddr>>>>) {
		let local = PeerId::random();
		let recorded = Recorded::default();
		let sets = recorded.0.clone();
		(Targets::with(Box::new(recorded), local, keep), local, sets)
	}

	#[test]
	fn the_local_peer_and_addresses_without_a_peer_id_are_dropped() {
		let (mut targets, local, sets) = targets(0);
		let other = PeerId::random();
		let without: Multiaddr = "/ip4/127.0.0.1/tcp/30333".parse().unwrap();
		targets.reserve(vec![address(&local), address(&other), without, address(&other)]);
		assert_eq!(targets.len(), 1);
		assert_eq!(*sets.lock().unwrap(), vec![HashSet::from([address(&other)])]);
	}

	#[test]
	fn the_last_past_recipients_are_kept() {
		let (mut targets, _, _) = targets(2);
		let [a, b, c, d, e] = [(); 5].map(|_| PeerId::random());
		// The peers after the recipients, the most recent first.
		let mut tick = |recipients: &[PeerId]| -> Vec<PeerId> {
			targets.reserve(recipients.iter().map(address).collect());
			assert_eq!(targets.len(), recipients.len());
			targets.peers.iter().skip(recipients.len()).map(|(p, _)| *p).collect()
		};
		assert_eq!(tick(&[a, b]), vec![]);
		// The window moves on by one each time: the one that leaves is kept, most recent first.
		assert_eq!(tick(&[b, c]), vec![a]);
		assert_eq!(tick(&[c, d]), vec![b, a]);
		// Past `keep`, the oldest is dropped.
		assert_eq!(tick(&[d, e]), vec![c, b]);
		// A kept peer that is a recipient again is no longer kept.
		assert_eq!(tick(&[e, b]), vec![d, c]);
	}

	#[test]
	fn the_network_is_told_when_the_reserved_addresses_change() {
		let (mut targets, _, sets) = targets(1);
		let (a, b) = (PeerId::random(), PeerId::random());
		targets.reserve(vec![address(&a)]);
		targets.reserve(vec![address(&a)]);
		assert_eq!(sets.lock().unwrap().len(), 1);
		// `a` is kept, so the reservation grows.
		targets.reserve(vec![address(&b)]);
		assert_eq!(
			*sets.lock().unwrap(),
			vec![HashSet::from([address(&a)]), HashSet::from([address(&a), address(&b)])]
		);
	}

	#[test]
	fn reachable_are_the_recipients_with_an_open_substream() {
		let (mut targets, _, _) = targets(1);
		let (a, b) = (PeerId::random(), PeerId::random());
		targets.reserve(vec![address(&a), address(&b)]);
		assert_eq!(targets.reachable().count(), 0);
		targets.opened(a);
		targets.opened(b);
		assert_eq!(targets.reachable().count(), 2);
		targets.closed(&a);
		assert_eq!(targets.reachable().collect::<Vec<_>>(), vec![&b]);
		// A kept peer is not reachable.
		targets.reserve(vec![address(&a)]);
		targets.opened(a);
		assert_eq!(targets.reachable().collect::<Vec<_>>(), vec![&a]);
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
