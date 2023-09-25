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

//! libp2p-related discovery code for litep2p backend.

use crate::{
	config::{NetworkConfiguration, ProtocolId},
	litep2p::peerstore::PeerstoreHandle,
	Multiaddr,
};

use array_bytes::bytes2hex;
use futures::{FutureExt, Stream};
use futures_timer::Delay;
use libp2p::kad::record::Key as KademliaKey;
use litep2p::{
	protocol::{
		libp2p::{
			identify::{Config as IdentifyConfig, IdentifyEvent},
			kademlia::{
				Config as KademliaConfig, ConfigBuilder as KademliaConfigBuilder, KademliaEvent,
				KademliaHandle, QueryId, Quorum, Record, RecordKey,
			},
			ping::{Config as PingConfig, PingEvent},
		},
		mdns::{Config as MdnsConfig, MdnsEvent},
	},
	PeerId, ProtocolName,
};

use std::{
	collections::{HashMap, HashSet},
	pin::Pin,
	task::{Context, Poll},
	time::Duration,
};

/// Logging target for the file.
const LOG_TARGET: &str = "sub-libp2p::discovery";

/// Kademlia query interval.
const KADEMLIA_QUERY_INTERVAL: Duration = Duration::from_secs(5);

/// mDNS query interval.
const MDNS_QUERY_INTERVAL: Duration = Duration::from_secs(30);

/// Discovery events.
#[derive(Debug)]
pub enum DiscoveryEvent {
	/// Ping RTT measured for peer.
	Ping {
		/// Remote peer ID.
		peer: PeerId,

		/// Ping round-trip time.
		rtt: Duration,
	},

	/// Peer identified over `/ipfs/identify/1.0.0` protocol.
	Identified {
		/// Peer ID.
		peer: PeerId,

		/// Observed address.
		observed_address: Multiaddr,

		/// Listen addresses.
		listen_addresses: Vec<Multiaddr>,

		/// Supported protocols.
		supported_protocols: HashSet<String>,
	},

	/// One or more addresses discovered.
	Discovered {
		/// Discovered addresses.
		addresses: Vec<Multiaddr>,
	},

	/// Routing table has been updated.
	RoutingTableUpdate {
		/// Peers that were added to routing table.
		peers: HashSet<PeerId>,
	},

	/// New external address discovered.
	#[allow(unused)]
	ExternalAddressDiscovered {
		/// Discovered addresses.
		address: Multiaddr,
	},

	/// Record was found from the DHT.
	GetRecordSuccess {
		/// Query ID.
		query_id: QueryId,

		/// Record.
		record: Record,
	},

	/// Record was successfully stored on the DHT.
	PutRecordSuccess {
		/// Query ID.
		query_id: QueryId,
	},

	/// Query failed.
	QueryFailed {
		/// Query ID.
		query_id: QueryId,
	},
}

pub struct Discovery {
	/// Ping event stream.
	ping_event_stream: Box<dyn Stream<Item = PingEvent> + Send + Unpin>,

	/// Identify event stream.
	identify_event_stream: Box<dyn Stream<Item = IdentifyEvent> + Send + Unpin>,

	/// mDNS event stream, if enabled.
	mdns_event_stream: Option<Box<dyn Stream<Item = MdnsEvent> + Send + Unpin>>,

	/// Kademlia handle.
	kademlia_handle: KademliaHandle,

	/// `Peerstore` handle.
	_peerstore_handle: PeerstoreHandle,

	/// Next Kademlia query for a random peer ID.
	///
	/// If `None`, there is currently a query pending.
	next_kad_query: Option<Delay>,
}

/// Legacy (fallback) Kademlia protocol name based on `protocol_id`.
fn legacy_kademlia_protocol_name(id: &ProtocolId) -> ProtocolName {
	ProtocolName::from(format!("/{}/kad", id.as_ref()))
}

/// Kademlia protocol name based on `genesis_hash` and `fork_id`.
fn kademlia_protocol_name<Hash: AsRef<[u8]>>(
	genesis_hash: Hash,
	fork_id: Option<&str>,
) -> ProtocolName {
	let genesis_hash_hex = bytes2hex("", genesis_hash.as_ref());
	let protocol = if let Some(fork_id) = fork_id {
		format!("/{}/{}/kad", genesis_hash_hex, fork_id)
	} else {
		format!("/{}/kad", genesis_hash_hex)
	};

	ProtocolName::from(protocol)
}

impl Discovery {
	/// Create new [`Discovery`].
	///
	/// Enables `/ipfs/ping/1.0.0` and `/ipfs/identify/1.0.0` by default and starts
	/// the mDNS peer discovery if it was enabled.
	pub fn new<Hash: AsRef<[u8]>>(
		config: &NetworkConfiguration,
		genesis_hash: Hash,
		fork_id: Option<&str>,
		protocol_id: &ProtocolId,
		known_peers: HashMap<PeerId, Vec<Multiaddr>>,
		_peerstore_handle: PeerstoreHandle,
	) -> (Self, PingConfig, IdentifyConfig, KademliaConfig, Option<MdnsConfig>) {
		let (ping_config, ping_event_stream) = PingConfig::default();
		let (identify_config, identify_event_stream) = IdentifyConfig::new();

		let (mdns_config, mdns_event_stream) = match config.transport {
			crate::config::TransportConfig::Normal { enable_mdns, .. } => match enable_mdns {
				true => {
					let (mdns_config, mdns_event_stream) = MdnsConfig::new(MDNS_QUERY_INTERVAL);
					(Some(mdns_config), Some(mdns_event_stream))
				},
				false => (None, None),
			},
			_ => panic!("memory transport not supported"),
		};

		let (kademlia_config, kademlia_handle) = {
			let protocol_names = vec![
				kademlia_protocol_name(genesis_hash, fork_id),
				legacy_kademlia_protocol_name(protocol_id),
			];

			KademliaConfigBuilder::new()
				.with_known_peers(known_peers)
				.with_protocol_names(protocol_names)
				.build()
		};

		(
			Self {
				ping_event_stream,
				identify_event_stream,
				mdns_event_stream,
				kademlia_handle,
				_peerstore_handle,
				next_kad_query: Some(Delay::new(KADEMLIA_QUERY_INTERVAL)),
			},
			ping_config,
			identify_config,
			kademlia_config,
			mdns_config,
		)
	}

	/// Add known peer to `Kademlia`.
	pub async fn add_known_peer(&mut self, peer: PeerId, addresses: Vec<Multiaddr>) {
		self.kademlia_handle.add_known_peer(peer, addresses).await;
	}

	/// Start Kademlia `GET_VALUE` query for `key`.
	pub async fn get_value(&mut self, key: KademliaKey) -> QueryId {
		self.kademlia_handle
			.get_record(RecordKey::new(&key.to_vec()), Quorum::One)
			.await
	}

	/// Publish value on the DHT using Kademlia `PUT_VALUE`.
	pub async fn put_value(&mut self, key: KademliaKey, value: Vec<u8>) -> QueryId {
		self.kademlia_handle
			.put_record(Record::new(RecordKey::new(&key.to_vec()), value))
			.await
	}
}

impl Stream for Discovery {
	type Item = DiscoveryEvent;

	fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
		let this = Pin::into_inner(self);

		if let Some(mut delay) = this.next_kad_query.take() {
			match delay.poll_unpin(cx) {
				Poll::Pending => {
					this.next_kad_query = Some(delay);
				},
				Poll::Ready(()) => {
					log::trace!(target: LOG_TARGET, "start next kademlia query");

					if let Err(()) = this.kademlia_handle.try_find_node(PeerId::random()) {
						this.next_kad_query = Some(Delay::new(KADEMLIA_QUERY_INTERVAL));
					}
				},
			}
		}

		match Pin::new(&mut this.kademlia_handle).poll_next(cx) {
			Poll::Pending => {},
			Poll::Ready(None) => return Poll::Ready(None),
			Poll::Ready(Some(KademliaEvent::FindNodeSuccess { peers, .. })) => {
				// the addresses are already inserted into the DHT and in `TransportManager` so
				// there is no need to add them again. The found peers must be registered to
				// `Peerstore` so other protocols are aware of them through `Peerset`.
				log::trace!(target: LOG_TARGET, "dht random walk yielded {} peers", peers.len());

				this.next_kad_query = Some(Delay::new(KADEMLIA_QUERY_INTERVAL));

				return Poll::Ready(Some(DiscoveryEvent::RoutingTableUpdate {
					peers: peers.into_iter().map(|(peer, _)| peer).collect(),
				}))
			},
			Poll::Ready(Some(KademliaEvent::RoutingTableUpdate { peers })) => {
				log::trace!(target: LOG_TARGET, "routing table update, discovered {} peers", peers.len());

				return Poll::Ready(Some(DiscoveryEvent::RoutingTableUpdate {
					peers: peers.into_iter().map(|peer| peer).collect(),
				}))
			},
			Poll::Ready(Some(KademliaEvent::GetRecordSuccess { query_id, record })) => {
				log::trace!(
					target: LOG_TARGET,
					"`GET_RECORD` succeeded for {query_id:?}: {record:?}",
				);

				return Poll::Ready(Some(DiscoveryEvent::GetRecordSuccess { query_id, record }));
			},
			Poll::Ready(Some(KademliaEvent::PutRecordSucess { query_id, key: _ })) =>
				return Poll::Ready(Some(DiscoveryEvent::PutRecordSuccess { query_id })),
			Poll::Ready(Some(KademliaEvent::QueryFailed { query_id })) =>
				return Poll::Ready(Some(DiscoveryEvent::QueryFailed { query_id })),
		}

		match Pin::new(&mut this.ping_event_stream).poll_next(cx) {
			Poll::Ready(None) => return Poll::Ready(None),
			Poll::Ready(Some(PingEvent::Ping { peer, ping })) =>
				return Poll::Ready(Some(DiscoveryEvent::Ping { peer, rtt: ping })),
			_ => {},
		}

		match Pin::new(&mut this.identify_event_stream).poll_next(cx) {
			Poll::Ready(None) => return Poll::Ready(None),
			Poll::Ready(Some(IdentifyEvent::PeerIdentified {
				peer,
				listen_addresses,
				supported_protocols,
				observed_address,
			})) =>
				return Poll::Ready(Some(DiscoveryEvent::Identified {
					peer,
					listen_addresses,
					observed_address,
					supported_protocols,
				})),
			_ => {},
		}

		if let Some(ref mut mdns_event_stream) = &mut this.mdns_event_stream {
			match Pin::new(mdns_event_stream).poll_next(cx) {
				Poll::Ready(None) => return Poll::Ready(None),
				Poll::Ready(Some(MdnsEvent::Discovered(addresses))) =>
					return Poll::Ready(Some(DiscoveryEvent::Discovered { addresses })),
				_ => {},
			}
		}

		Poll::Pending
	}
}
