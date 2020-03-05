// Copyright 2019-2020 Parity Technologies (UK) Ltd.
// This file is part of Substrate.

// Substrate is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Substrate is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Substrate.  If not, see <http://www.gnu.org/licenses/>.

//! Discovery mechanisms of Substrate.
//!
//! The `DiscoveryBehaviour` struct implements the `NetworkBehaviour` trait of libp2p and is
//! responsible for discovering other nodes that are part of the network.
//!
//! Substrate uses the following mechanisms in order to discover nodes that are part of the network:
//!
//! - Bootstrap nodes. These are hard-coded node identities and addresses passed in the constructor
//! of the `DiscoveryBehaviour`. You can also call `add_known_address` later to add an entry.
//!
//! - mDNS. Discovers nodes on the local network by broadcasting UDP packets.
//!
//! - Kademlia random walk. Once connected, we perform random Kademlia `FIND_NODE` requests in
//! order for nodes to propagate to us their view of the network. This is performed automatically
//! by the `DiscoveryBehaviour`.
//!
//! Additionally, the `DiscoveryBehaviour` is also capable of storing and loading value in the
//! network-wide DHT.
//!
//! ## Usage
//!
//! The `DiscoveryBehaviour` generates events of type `DiscoveryOut`, most notably
//! `DiscoveryOut::Discovered` that is generated whenever we discover a node.
//! Only the identity of the node is returned. The node's addresses are stored within the
//! `DiscoveryBehaviour` and can be queried through the `NetworkBehaviour` trait.
//!
//! **Important**: In order for the discovery mechanism to work properly, there needs to be an
//! active mechanism that asks nodes for the addresses they are listening on. Whenever we learn
//! of a node's address, you must call `add_self_reported_address`.
//!

use futures::prelude::*;
use futures_timer::Delay;
use libp2p::core::{nodes::listeners::ListenerId, ConnectedPoint, Multiaddr, PeerId, PublicKey};
use libp2p::swarm::{ProtocolsHandler, NetworkBehaviour, NetworkBehaviourAction, PollParameters};
use libp2p::kad::{Kademlia, KademliaEvent, Quorum, Record};
use libp2p::kad::GetClosestPeersError;
use libp2p::kad::record::{self, store::MemoryStore};
#[cfg(not(target_os = "unknown"))]
use libp2p::{swarm::toggle::Toggle};
#[cfg(not(target_os = "unknown"))]
use libp2p::mdns::{Mdns, MdnsEvent};
use libp2p::multiaddr::Protocol;
use log::{debug, info, trace, warn, error};
use std::{cmp, collections::VecDeque, time::Duration};
use std::task::{Context, Poll};
use sp_core::hexdisplay::HexDisplay;

/// Implementation of `NetworkBehaviour` that discovers the nodes on the network.
pub struct DiscoveryBehaviour {
	/// User-defined list of nodes and their addresses. Typically includes bootstrap nodes and
	/// reserved nodes.
	user_defined: Vec<(PeerId, Multiaddr)>,
	/// Kademlia requests and answers.
	kademlia: Kademlia<MemoryStore>,
	/// Discovers nodes on the local network.
	#[cfg(not(target_os = "unknown"))]
	mdns: Toggle<Mdns>,
	/// Stream that fires when we need to perform the next random Kademlia query.
	next_kad_random_query: Delay,
	/// After `next_kad_random_query` triggers, the next one triggers after this duration.
	duration_to_next_kad: Duration,
	/// Discovered nodes to return.
	discoveries: VecDeque<PeerId>,
	/// Identity of our local node.
	local_peer_id: PeerId,
	/// Number of nodes we're currently connected to.
	num_connections: u64,
	/// If false, `addresses_of_peer` won't return any private IPv4 address, except for the ones
	/// stored in `user_defined`.
	allow_private_ipv4: bool,
	/// Number of active connections over which we interrupt the discovery process.
	discovery_only_if_under_num: u64,
}

impl DiscoveryBehaviour {
	/// Builds a new `DiscoveryBehaviour`.
	///
	/// `user_defined` is a list of known address for nodes that never expire.
	pub async fn new(
		local_public_key: PublicKey,
		user_defined: Vec<(PeerId, Multiaddr)>,
		enable_mdns: bool,
		allow_private_ipv4: bool,
		discovery_only_if_under_num: u64,
	) -> Self {
		if enable_mdns {
			#[cfg(target_os = "unknown")]
			warn!(target: "sub-libp2p", "mDNS is not available on this platform");
		}

		let local_id = local_public_key.clone().into_peer_id();
		let store = MemoryStore::new(local_id.clone());
		let mut kademlia = Kademlia::new(local_id.clone(), store);
		for (peer_id, addr) in &user_defined {
			kademlia.add_address(peer_id, addr.clone());
		}

		DiscoveryBehaviour {
			user_defined,
			kademlia,
			next_kad_random_query: Delay::new(Duration::new(0, 0)),
			duration_to_next_kad: Duration::from_secs(1),
			discoveries: VecDeque::new(),
			local_peer_id: local_public_key.into_peer_id(),
			num_connections: 0,
			allow_private_ipv4,
			discovery_only_if_under_num,
			#[cfg(not(target_os = "unknown"))]
			mdns: if enable_mdns {
				match Mdns::new() {
					Ok(mdns) => Some(mdns).into(),
					Err(err) => {
						warn!(target: "sub-libp2p", "Failed to initialize mDNS: {:?}", err);
						None.into()
					}
				}
			} else {
				None.into()
			},
		}
	}

	/// Returns the list of nodes that we know exist in the network.
	pub fn known_peers(&mut self) -> impl Iterator<Item = &PeerId> {
		self.kademlia.kbuckets_entries()
	}

	/// Adds a hard-coded address for the given peer, that never expires.
	///
	/// This adds an entry to the parameter that was passed to `new`.
	///
	/// If we didn't know this address before, also generates a `Discovered` event.
	pub fn add_known_address(&mut self, peer_id: PeerId, addr: Multiaddr) {
		if self.user_defined.iter().all(|(p, a)| *p != peer_id && *a != addr) {
			self.discoveries.push_back(peer_id.clone());
			self.user_defined.push((peer_id, addr));
		}
	}

	/// Call this method when a node reports an address for itself.
	///
	/// **Note**: It is important that you call this method, otherwise the discovery mechanism will
	/// not properly work.
	pub fn add_self_reported_address(&mut self, peer_id: &PeerId, addr: Multiaddr) {
		self.kademlia.add_address(peer_id, addr);
	}

	/// Start fetching a record from the DHT.
	///
	/// A corresponding `ValueFound` or `ValueNotFound` event will later be generated.
	pub fn get_value(&mut self, key: &record::Key) {
		self.kademlia.get_record(key, Quorum::One)
	}

	/// Start putting a record into the DHT. Other nodes can later fetch that value with
	/// `get_value`.
	///
	/// A corresponding `ValuePut` or `ValuePutFailed` event will later be generated.
	pub fn put_value(&mut self, key: record::Key, value: Vec<u8>) {
		self.kademlia.put_record(Record::new(key, value), Quorum::All);
	}

	/// Returns the number of nodes that are in the Kademlia k-buckets.
	pub fn num_kbuckets_entries(&mut self) -> usize {
		self.kademlia.kbuckets_entries().count()
	}
}

/// Event generated by the `DiscoveryBehaviour`.
pub enum DiscoveryOut {
	/// The address of a peer has been added to the Kademlia routing table.
	///
	/// Can be called multiple times with the same identity.
	Discovered(PeerId),

	/// A peer connected to this node for whom no listen address is known.
	///
	/// In order for the peer to be added to the Kademlia routing table, a known
	/// listen address must be added via [`DiscoveryBehaviour::add_self_reported_address`],
	/// e.g. obtained through the `identify` protocol.
	UnroutablePeer(PeerId),

	/// The DHT yielded results for the record request, grouped in (key, value) pairs.
	ValueFound(Vec<(record::Key, Vec<u8>)>),

	/// The record requested was not found in the DHT.
	ValueNotFound(record::Key),

	/// The record with a given key was successfully inserted into the DHT.
	ValuePut(record::Key),

	/// Inserting a value into the DHT failed.
	ValuePutFailed(record::Key),

	/// Started a random Kademlia query.
	RandomKademliaStarted,
}

impl NetworkBehaviour for DiscoveryBehaviour {
	type ProtocolsHandler = <Kademlia<MemoryStore> as NetworkBehaviour>::ProtocolsHandler;
	type OutEvent = DiscoveryOut;

	fn new_handler(&mut self) -> Self::ProtocolsHandler {
		NetworkBehaviour::new_handler(&mut self.kademlia)
	}

	fn addresses_of_peer(&mut self, peer_id: &PeerId) -> Vec<Multiaddr> {
		let mut list = self.user_defined.iter()
			.filter_map(|(p, a)| if p == peer_id { Some(a.clone()) } else { None })
			.collect::<Vec<_>>();

		{
			let mut list_to_filter = self.kademlia.addresses_of_peer(peer_id);
			#[cfg(not(target_os = "unknown"))]
			list_to_filter.extend(self.mdns.addresses_of_peer(peer_id));

			if !self.allow_private_ipv4 {
				list_to_filter.retain(|addr| {
					if let Some(Protocol::Ip4(addr)) = addr.iter().next() {
						if addr.is_private() {
							return false;
						}
					}

					true
				});
			}

			list.extend(list_to_filter);
		}

		trace!(target: "sub-libp2p", "Addresses of {:?} are {:?}", peer_id, list);
		if list.is_empty() {
			if self.kademlia.kbuckets_entries().any(|p| p == peer_id) {
				debug!(target: "sub-libp2p", "Requested dialing to {:?} (peer in k-buckets), \
					and no address was found", peer_id);
			} else {
				debug!(target: "sub-libp2p", "Requested dialing to {:?} (peer not in k-buckets), \
					and no address was found", peer_id);
			}
		}
		list
	}

	fn inject_connected(&mut self, peer_id: PeerId, endpoint: ConnectedPoint) {
		self.num_connections += 1;
		NetworkBehaviour::inject_connected(&mut self.kademlia, peer_id, endpoint)
	}

	fn inject_disconnected(&mut self, peer_id: &PeerId, endpoint: ConnectedPoint) {
		self.num_connections -= 1;
		NetworkBehaviour::inject_disconnected(&mut self.kademlia, peer_id, endpoint)
	}

	fn inject_replaced(&mut self, peer_id: PeerId, closed: ConnectedPoint, opened: ConnectedPoint) {
		NetworkBehaviour::inject_replaced(&mut self.kademlia, peer_id, closed, opened)
	}

	fn inject_addr_reach_failure(
		&mut self,
		peer_id: Option<&PeerId>,
		addr: &Multiaddr,
		error: &dyn std::error::Error
	) {
		NetworkBehaviour::inject_addr_reach_failure(&mut self.kademlia, peer_id, addr, error)
	}

	fn inject_node_event(
		&mut self,
		peer_id: PeerId,
		event: <Self::ProtocolsHandler as ProtocolsHandler>::OutEvent,
	) {
		NetworkBehaviour::inject_node_event(&mut self.kademlia, peer_id, event)
	}

	fn inject_new_external_addr(&mut self, addr: &Multiaddr) {
		let new_addr = addr.clone()
			.with(Protocol::P2p(self.local_peer_id.clone().into()));
		info!(target: "sub-libp2p", "Discovered new external address for our node: {}", new_addr);
		NetworkBehaviour::inject_new_external_addr(&mut self.kademlia, addr)
	}

	fn inject_expired_listen_addr(&mut self, addr: &Multiaddr) {
		info!(target: "sub-libp2p", "No longer listening on {}", addr);
		NetworkBehaviour::inject_expired_listen_addr(&mut self.kademlia, addr)
	}

	fn inject_dial_failure(&mut self, peer_id: &PeerId) {
		NetworkBehaviour::inject_dial_failure(&mut self.kademlia, peer_id)
	}

	fn inject_new_listen_addr(&mut self, addr: &Multiaddr) {
		NetworkBehaviour::inject_new_listen_addr(&mut self.kademlia, addr)
	}

	fn inject_listener_error(&mut self, id: ListenerId, err: &(dyn std::error::Error + 'static)) {
		error!(target: "sub-libp2p", "Error on libp2p listener {:?}: {}", id, err);
		NetworkBehaviour::inject_listener_error(&mut self.kademlia, id, err);
	}

	fn inject_listener_closed(&mut self, id: ListenerId) {
		error!(target: "sub-libp2p", "Libp2p listener {:?} closed", id);
		NetworkBehaviour::inject_listener_closed(&mut self.kademlia, id);
	}

	fn poll(
		&mut self,
		cx: &mut Context,
		params: &mut impl PollParameters,
	) -> Poll<
		NetworkBehaviourAction<
			<Self::ProtocolsHandler as ProtocolsHandler>::InEvent,
			Self::OutEvent,
		>,
	> {
		// Immediately process the content of `discovered`.
		if let Some(peer_id) = self.discoveries.pop_front() {
			let ev = DiscoveryOut::Discovered(peer_id);
			return Poll::Ready(NetworkBehaviourAction::GenerateEvent(ev));
		}

		// Poll the stream that fires when we need to start a random Kademlia query.
		while let Poll::Ready(_) = self.next_kad_random_query.poll_unpin(cx) {
			let actually_started = if self.num_connections < self.discovery_only_if_under_num {
				let random_peer_id = PeerId::random();
				debug!(target: "sub-libp2p", "Libp2p <= Starting random Kademlia request for \
					{:?}", random_peer_id);

				self.kademlia.get_closest_peers(random_peer_id);
				true

			} else {
				debug!(
					target: "sub-libp2p",
					"Kademlia paused due to high number of connections ({})",
					self.num_connections
				);
				false
			};

			// Schedule the next random query with exponentially increasing delay,
			// capped at 60 seconds.
			self.next_kad_random_query = Delay::new(self.duration_to_next_kad);
			self.duration_to_next_kad = cmp::min(self.duration_to_next_kad * 2,
				Duration::from_secs(60));

			if actually_started {
				let ev = DiscoveryOut::RandomKademliaStarted;
				return Poll::Ready(NetworkBehaviourAction::GenerateEvent(ev));
			}
		}

		// Poll Kademlia.
		while let Poll::Ready(ev) = self.kademlia.poll(cx, params) {
			match ev {
				NetworkBehaviourAction::GenerateEvent(ev) => match ev {
					KademliaEvent::UnroutablePeer { peer, .. } => {
						let ev = DiscoveryOut::UnroutablePeer(peer);
						return Poll::Ready(NetworkBehaviourAction::GenerateEvent(ev));
					}
					KademliaEvent::RoutingUpdated { peer, .. } => {
						let ev = DiscoveryOut::Discovered(peer);
						return Poll::Ready(NetworkBehaviourAction::GenerateEvent(ev));
					}
					KademliaEvent::GetClosestPeersResult(res) => {
						match res {
							Err(GetClosestPeersError::Timeout { key, peers }) => {
								debug!(target: "sub-libp2p",
									"Libp2p => Query for {:?} timed out with {} results",
									HexDisplay::from(&key), peers.len());
							},
							Ok(ok) => {
								trace!(target: "sub-libp2p",
									"Libp2p => Query for {:?} yielded {:?} results",
									HexDisplay::from(&ok.key), ok.peers.len());
								if ok.peers.is_empty() && self.num_connections != 0 {
									debug!(target: "sub-libp2p", "Libp2p => Random Kademlia query has yielded empty \
										results");
								}
							}
						}
					}
					KademliaEvent::GetRecordResult(res) => {
						let ev = match res {
							Ok(ok) => {
								let results = ok.records
									.into_iter()
									.map(|r| (r.key, r.value))
									.collect();

								DiscoveryOut::ValueFound(results)
							}
							Err(e @ libp2p::kad::GetRecordError::NotFound { .. }) => {
								trace!(target: "sub-libp2p",
									"Libp2p => Failed to get record: {:?}", e);
								DiscoveryOut::ValueNotFound(e.into_key())
							}
							Err(e) => {
								warn!(target: "sub-libp2p",
									"Libp2p => Failed to get record: {:?}", e);
								DiscoveryOut::ValueNotFound(e.into_key())
							}
						};
						return Poll::Ready(NetworkBehaviourAction::GenerateEvent(ev));
					}
					KademliaEvent::PutRecordResult(res) => {
						let ev = match res {
							Ok(ok) => DiscoveryOut::ValuePut(ok.key),
							Err(e) => {
								warn!(target: "sub-libp2p",
									"Libp2p => Failed to put record: {:?}", e);
								DiscoveryOut::ValuePutFailed(e.into_key())
							}
						};
						return Poll::Ready(NetworkBehaviourAction::GenerateEvent(ev));
					}
					KademliaEvent::RepublishRecordResult(res) => {
						match res {
							Ok(ok) => debug!(target: "sub-libp2p",
								"Libp2p => Record republished: {:?}",
								ok.key),
							Err(e) => warn!(target: "sub-libp2p",
								"Libp2p => Republishing of record {:?} failed with: {:?}",
								e.key(), e)
						}
					}
					KademliaEvent::Discovered { .. } => {
						// We are not interested in these events at the moment.
					}
					// We never start any other type of query.
					e => {
						warn!(target: "sub-libp2p", "Libp2p => Unhandled Kademlia event: {:?}", e)
					}
				},
				NetworkBehaviourAction::DialAddress { address } =>
					return Poll::Ready(NetworkBehaviourAction::DialAddress { address }),
				NetworkBehaviourAction::DialPeer { peer_id } =>
					return Poll::Ready(NetworkBehaviourAction::DialPeer { peer_id }),
				NetworkBehaviourAction::SendEvent { peer_id, event } =>
					return Poll::Ready(NetworkBehaviourAction::SendEvent { peer_id, event }),
				NetworkBehaviourAction::ReportObservedAddr { address } =>
					return Poll::Ready(NetworkBehaviourAction::ReportObservedAddr { address }),
			}
		}

		// Poll mDNS.
		#[cfg(not(target_os = "unknown"))]
		while let Poll::Ready(ev) = self.mdns.poll(cx, params) {
			match ev {
				NetworkBehaviourAction::GenerateEvent(event) => {
					match event {
						MdnsEvent::Discovered(list) => {
							if self.num_connections >= self.discovery_only_if_under_num {
								continue;
							}

							self.discoveries.extend(list.into_iter().map(|(peer_id, _)| peer_id));
							if let Some(peer_id) = self.discoveries.pop_front() {
								let ev = DiscoveryOut::Discovered(peer_id);
								return Poll::Ready(NetworkBehaviourAction::GenerateEvent(ev));
							}
						},
						MdnsEvent::Expired(_) => {}
					}
				},
				NetworkBehaviourAction::DialAddress { address } =>
					return Poll::Ready(NetworkBehaviourAction::DialAddress { address }),
				NetworkBehaviourAction::DialPeer { peer_id } =>
					return Poll::Ready(NetworkBehaviourAction::DialPeer { peer_id }),
				NetworkBehaviourAction::SendEvent { event, .. } =>
					match event {},		// `event` is an enum with no variant
				NetworkBehaviourAction::ReportObservedAddr { address } =>
					return Poll::Ready(NetworkBehaviourAction::ReportObservedAddr { address }),
			}
		}

		Poll::Pending
	}
}

#[cfg(test)]
mod tests {
	use futures::prelude::*;
	use libp2p::identity::Keypair;
	use libp2p::Multiaddr;
	use libp2p::core::upgrade;
	use libp2p::core::transport::{Transport, MemoryTransport};
	use libp2p::core::upgrade::{InboundUpgradeExt, OutboundUpgradeExt};
	use libp2p::swarm::Swarm;
	use std::{collections::HashSet, task::Poll};
	use super::{DiscoveryBehaviour, DiscoveryOut};

	#[test]
	fn discovery_working() {
		let mut user_defined = Vec::new();

		// Build swarms whose behaviour is `DiscoveryBehaviour`.
		let mut swarms = (0..25).map(|_| {
			let keypair = Keypair::generate_ed25519();
			let keypair2 = keypair.clone();

			let transport = MemoryTransport
				.and_then(move |out, endpoint| {
					let secio = libp2p::secio::SecioConfig::new(keypair2);
					libp2p::core::upgrade::apply(
						out,
						secio,
						endpoint,
						upgrade::Version::V1
					)
				})
				.and_then(move |(peer_id, stream), endpoint| {
					let peer_id2 = peer_id.clone();
					let upgrade = libp2p::yamux::Config::default()
						.map_inbound(move |muxer| (peer_id, muxer))
						.map_outbound(move |muxer| (peer_id2, muxer));
					upgrade::apply(stream, upgrade, endpoint, upgrade::Version::V1)
				});

			let behaviour = futures::executor::block_on({
				let user_defined = user_defined.clone();
				let keypair_public = keypair.public();
				async move {
					DiscoveryBehaviour::new(keypair_public, user_defined, false, true, 50).await
				}
			});
			let mut swarm = Swarm::new(transport, behaviour, keypair.public().into_peer_id());
			let listen_addr: Multiaddr = format!("/memory/{}", rand::random::<u64>()).parse().unwrap();

			if user_defined.is_empty() {
				user_defined.push((keypair.public().into_peer_id(), listen_addr.clone()));
			}

			Swarm::listen_on(&mut swarm, listen_addr.clone()).unwrap();
			(swarm, listen_addr)
		}).collect::<Vec<_>>();

		// Build a `Vec<HashSet<PeerId>>` with the list of nodes remaining to be discovered.
		let mut to_discover = (0..swarms.len()).map(|n| {
			(0..swarms.len()).filter(|p| *p != n)
				.map(|p| Swarm::local_peer_id(&swarms[p].0).clone())
				.collect::<HashSet<_>>()
		}).collect::<Vec<_>>();

		let fut = futures::future::poll_fn(move |cx| {
			'polling: loop {
				for swarm_n in 0..swarms.len() {
					match swarms[swarm_n].0.poll_next_unpin(cx) {
						Poll::Ready(Some(e)) => {
							match e {
								DiscoveryOut::UnroutablePeer(other) => {
									// Call `add_self_reported_address` to simulate identify happening.
									let addr = swarms.iter().find_map(|(s, a)|
										if s.local_peer_id == other {
											Some(a.clone())
										} else {
											None
										})
										.unwrap();
									swarms[swarm_n].0.add_self_reported_address(&other, addr);
								},
								DiscoveryOut::Discovered(other) => {
									to_discover[swarm_n].remove(&other);
								}
								_ => {}
							}
							continue 'polling
						}
						_ => {}
					}
				}
				break
			}

			if to_discover.iter().all(|l| l.is_empty()) {
				Poll::Ready(())
			} else {
				Poll::Pending
			}
		});

		futures::executor::block_on(fut);
	}
}
