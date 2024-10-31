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
	peer_store::PeerStoreProvider,
};

use array_bytes::bytes2hex;
use futures::{FutureExt, Stream};
use futures_timer::Delay;
use ip_network::IpNetwork;
use libp2p::kad::record::Key as KademliaKey;
use litep2p::{
	protocol::{
		libp2p::{
			identify::{Config as IdentifyConfig, IdentifyEvent},
			kademlia::{
				Config as KademliaConfig, ConfigBuilder as KademliaConfigBuilder,
				IncomingRecordValidationMode, KademliaEvent, KademliaHandle, QueryId, Quorum,
				Record, RecordKey, RecordsType,
			},
			ping::{Config as PingConfig, PingEvent},
		},
		mdns::{Config as MdnsConfig, MdnsEvent},
	},
	types::multiaddr::{Multiaddr, Protocol},
	PeerId, ProtocolName,
};
use parking_lot::RwLock;
use schnellru::{ByLength, LruMap};

use std::{
	cmp,
	collections::{HashMap, HashSet, VecDeque},
	num::NonZeroUsize,
	pin::Pin,
	sync::Arc,
	task::{Context, Poll},
	time::{Duration, Instant},
};

/// Logging target for the file.
const LOG_TARGET: &str = "sub-libp2p::discovery";

/// Kademlia query interval.
const KADEMLIA_QUERY_INTERVAL: Duration = Duration::from_secs(5);

/// mDNS query interval.
const MDNS_QUERY_INTERVAL: Duration = Duration::from_secs(30);

/// The minimum number of peers we expect an answer before we terminate the request.
const GET_RECORD_REDUNDANCY_FACTOR: usize = 4;

/// The maximum number of tracked external addresses we allow.
const MAX_EXTERNAL_ADDRESSES: u32 = 32;

/// Minimum number of confirmations received before an address is verified.
///
/// Note: all addresses are confirmed by libp2p on the first encounter. This aims to make
/// addresses a bit more robust.
const MIN_ADDRESS_CONFIRMATIONS: usize = 2;

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

		/// Listen addresses.
		listen_addresses: Vec<Multiaddr>,

		/// Supported protocols.
		supported_protocols: HashSet<ProtocolName>,
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
	ExternalAddressDiscovered {
		/// Discovered address.
		address: Multiaddr,
	},

	/// The external address has expired.
	///
	/// This happens when the internal buffers exceed the maximum number of external addresses,
	/// and this address is the oldest one.
	ExternalAddressExpired {
		/// Expired address.
		address: Multiaddr,
	},

	/// Record was found from the DHT.
	GetRecordSuccess {
		/// Query ID.
		query_id: QueryId,

		/// Records.
		records: RecordsType,
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

	/// Incoming record to store.
	IncomingRecord {
		/// Record.
		record: Record,
	},

	/// Started a random Kademlia query.
	RandomKademliaStarted,
}

/// Discovery.
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
	_peerstore_handle: Arc<dyn PeerStoreProvider>,

	/// Next Kademlia query for a random peer ID.
	///
	/// If `None`, there is currently a query pending.
	next_kad_query: Option<Delay>,

	/// Active `FIND_NODE` query if it exists.
	find_node_query_id: Option<QueryId>,

	/// Pending events.
	pending_events: VecDeque<DiscoveryEvent>,

	/// Allow non-global addresses in the DHT.
	allow_non_global_addresses: bool,

	/// Protocols supported by the local node.
	local_protocols: HashSet<ProtocolName>,

	/// Public addresses.
	public_addresses: HashSet<Multiaddr>,

	/// Listen addresses.
	listen_addresses: Arc<RwLock<HashSet<Multiaddr>>>,

	/// External address confirmations.
	address_confirmations: LruMap<Multiaddr, HashSet<PeerId>>,

	/// Delay to next `FIND_NODE` query.
	duration_to_next_find_query: Duration,
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
	pub fn new<Hash: AsRef<[u8]> + Clone>(
		config: &NetworkConfiguration,
		genesis_hash: Hash,
		fork_id: Option<&str>,
		protocol_id: &ProtocolId,
		known_peers: HashMap<PeerId, Vec<Multiaddr>>,
		listen_addresses: Arc<RwLock<HashSet<Multiaddr>>>,
		_peerstore_handle: Arc<dyn PeerStoreProvider>,
	) -> (Self, PingConfig, IdentifyConfig, KademliaConfig, Option<MdnsConfig>) {
		let (ping_config, ping_event_stream) = PingConfig::default();
		let user_agent = format!("{} ({})", config.client_version, config.node_name);

		let (identify_config, identify_event_stream) =
			IdentifyConfig::new("/substrate/1.0".to_string(), Some(user_agent));

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
				kademlia_protocol_name(genesis_hash.clone(), fork_id),
				legacy_kademlia_protocol_name(protocol_id),
			];

			KademliaConfigBuilder::new()
				.with_known_peers(known_peers)
				.with_protocol_names(protocol_names)
				.with_incoming_records_validation_mode(IncomingRecordValidationMode::Manual)
				.build()
		};

		(
			Self {
				ping_event_stream,
				identify_event_stream,
				mdns_event_stream,
				kademlia_handle,
				_peerstore_handle,
				listen_addresses,
				find_node_query_id: None,
				pending_events: VecDeque::new(),
				duration_to_next_find_query: Duration::from_secs(1),
				address_confirmations: LruMap::new(ByLength::new(MAX_EXTERNAL_ADDRESSES)),
				allow_non_global_addresses: config.allow_non_globals_in_dht,
				public_addresses: config.public_addresses.iter().cloned().map(Into::into).collect(),
				next_kad_query: Some(Delay::new(KADEMLIA_QUERY_INTERVAL)),
				local_protocols: HashSet::from_iter([kademlia_protocol_name(
					genesis_hash,
					fork_id,
				)]),
			},
			ping_config,
			identify_config,
			kademlia_config,
			mdns_config,
		)
	}

	/// Add known peer to `Kademlia`.
	#[allow(unused)]
	pub async fn add_known_peer(&mut self, peer: PeerId, addresses: Vec<Multiaddr>) {
		self.kademlia_handle.add_known_peer(peer, addresses).await;
	}

	/// Add self-reported addresses to routing table if `peer` supports
	/// at least one of the locally supported DHT protocol.
	pub async fn add_self_reported_address(
		&mut self,
		peer: PeerId,
		supported_protocols: HashSet<ProtocolName>,
		addresses: Vec<Multiaddr>,
	) {
		if self.local_protocols.is_disjoint(&supported_protocols) {
			log::trace!(
				target: LOG_TARGET,
				"Ignoring self-reported address of peer {peer} as remote node is not part of the \
				 Kademlia DHT supported by the local node.",
			);
			return
		}

		let addresses = addresses
			.into_iter()
			.filter_map(|address| {
				if !self.allow_non_global_addresses && !Discovery::can_add_to_dht(&address) {
					log::trace!(
						target: LOG_TARGET,
						"ignoring self-reported non-global address {address} from {peer}."
					);

					return None
				}

				Some(address)
			})
			.collect();

		log::trace!(
			target: LOG_TARGET,
			"add self-reported addresses for {peer:?}: {addresses:?}",
		);

		self.kademlia_handle.add_known_peer(peer, addresses).await;
	}

	/// Start Kademlia `GET_VALUE` query for `key`.
	pub async fn get_value(&mut self, key: KademliaKey) -> QueryId {
		self.kademlia_handle
			.get_record(
				RecordKey::new(&key.to_vec()),
				Quorum::N(NonZeroUsize::new(GET_RECORD_REDUNDANCY_FACTOR).unwrap()),
			)
			.await
	}

	/// Publish value on the DHT using Kademlia `PUT_VALUE`.
	pub async fn put_value(&mut self, key: KademliaKey, value: Vec<u8>) -> QueryId {
		self.kademlia_handle
			.put_record(Record::new(RecordKey::new(&key.to_vec()), value))
			.await
	}

	/// Put record to given peers.
	pub async fn put_value_to_peers(
		&mut self,
		record: Record,
		peers: Vec<sc_network_types::PeerId>,
		update_local_storage: bool,
	) -> QueryId {
		self.kademlia_handle
			.put_record_to_peers(
				record,
				peers.into_iter().map(|peer| peer.into()).collect(),
				update_local_storage,
			)
			.await
	}

	/// Store record in the local DHT store.
	pub async fn store_record(
		&mut self,
		key: KademliaKey,
		value: Vec<u8>,
		publisher: Option<sc_network_types::PeerId>,
		expires: Option<Instant>,
	) {
		log::debug!(
			target: LOG_TARGET,
			"Storing DHT record with key {key:?}, originally published by {publisher:?}, \
			 expires {expires:?}.",
		);

		self.kademlia_handle
			.store_record(Record {
				key: RecordKey::new(&key.to_vec()),
				value,
				publisher: publisher.map(Into::into),
				expires,
			})
			.await;
	}

	/// Check if the observed address is a known address.
	fn is_known_address(known: &Multiaddr, observed: &Multiaddr) -> bool {
		let mut known = known.iter();
		let mut observed = observed.iter();

		loop {
			match (known.next(), observed.next()) {
				(None, None) => return true,
				(None, Some(Protocol::P2p(_))) => return true,
				(Some(Protocol::P2p(_)), None) => return true,
				(known, observed) if known != observed => return false,
				_ => {},
			}
		}
	}

	/// Can `address` be added to DHT.
	fn can_add_to_dht(address: &Multiaddr) -> bool {
		let ip = match address.iter().next() {
			Some(Protocol::Ip4(ip)) => IpNetwork::from(ip),
			Some(Protocol::Ip6(ip)) => IpNetwork::from(ip),
			Some(Protocol::Dns(_)) | Some(Protocol::Dns4(_)) | Some(Protocol::Dns6(_)) =>
				return true,
			_ => return false,
		};

		ip.is_global()
	}

	/// Check if `address` can be considered a new external address.
	///
	/// If this address replaces an older address, the expired address is returned.
	fn is_new_external_address(
		&mut self,
		address: &Multiaddr,
		peer: PeerId,
	) -> (bool, Option<Multiaddr>) {
		log::trace!(target: LOG_TARGET, "verify new external address: {address}");

		// is the address one of our known addresses
		if self
			.listen_addresses
			.read()
			.iter()
			.chain(self.public_addresses.iter())
			.any(|known_address| Discovery::is_known_address(&known_address, &address))
		{
			return (true, None)
		}

		match self.address_confirmations.get(address) {
			Some(confirmations) => {
				confirmations.insert(peer);

				if confirmations.len() >= MIN_ADDRESS_CONFIRMATIONS {
					return (true, None)
				}
			},
			None => {
				let oldest = (self.address_confirmations.len() >=
					self.address_confirmations.limiter().max_length() as usize)
					.then(|| {
						self.address_confirmations.pop_oldest().map(|(address, peers)| {
							if peers.len() >= MIN_ADDRESS_CONFIRMATIONS {
								return Some(address)
							} else {
								None
							}
						})
					})
					.flatten()
					.flatten();

				self.address_confirmations.insert(address.clone(), Default::default());

				return (false, oldest)
			},
		}

		(false, None)
	}
}

impl Stream for Discovery {
	type Item = DiscoveryEvent;

	fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
		let this = Pin::into_inner(self);

		if let Some(event) = this.pending_events.pop_front() {
			return Poll::Ready(Some(event))
		}

		if let Some(mut delay) = this.next_kad_query.take() {
			match delay.poll_unpin(cx) {
				Poll::Pending => {
					this.next_kad_query = Some(delay);
				},
				Poll::Ready(()) => {
					let peer = PeerId::random();

					log::trace!(target: LOG_TARGET, "start next kademlia query for {peer:?}");

					match this.kademlia_handle.try_find_node(peer) {
						Ok(query_id) => {
							this.find_node_query_id = Some(query_id);
							return Poll::Ready(Some(DiscoveryEvent::RandomKademliaStarted))
						},
						Err(()) => {
							this.duration_to_next_find_query = cmp::min(
								this.duration_to_next_find_query * 2,
								Duration::from_secs(60),
							);
							this.next_kad_query =
								Some(Delay::new(this.duration_to_next_find_query));
						},
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
					peers: peers.into_iter().collect(),
				}))
			},
			Poll::Ready(Some(KademliaEvent::GetRecordSuccess { query_id, records })) => {
				log::trace!(
					target: LOG_TARGET,
					"`GET_RECORD` succeeded for {query_id:?}: {records:?}",
				);

				return Poll::Ready(Some(DiscoveryEvent::GetRecordSuccess { query_id, records }));
			},
			Poll::Ready(Some(KademliaEvent::PutRecordSucess { query_id, key: _ })) =>
				return Poll::Ready(Some(DiscoveryEvent::PutRecordSuccess { query_id })),
			Poll::Ready(Some(KademliaEvent::QueryFailed { query_id })) => {
				match this.find_node_query_id == Some(query_id) {
					true => {
						this.find_node_query_id = None;
						this.duration_to_next_find_query =
							cmp::min(this.duration_to_next_find_query * 2, Duration::from_secs(60));
						this.next_kad_query = Some(Delay::new(this.duration_to_next_find_query));
					},
					false => return Poll::Ready(Some(DiscoveryEvent::QueryFailed { query_id })),
				}
			},
			Poll::Ready(Some(KademliaEvent::IncomingRecord { record })) => {
				log::trace!(
					target: LOG_TARGET,
					"incoming `PUT_RECORD` request with key {:?} from publisher {:?}",
					record.key,
					record.publisher,
				);

				return Poll::Ready(Some(DiscoveryEvent::IncomingRecord { record }))
			},
		}

		match Pin::new(&mut this.identify_event_stream).poll_next(cx) {
			Poll::Pending => {},
			Poll::Ready(None) => return Poll::Ready(None),
			Poll::Ready(Some(IdentifyEvent::PeerIdentified {
				peer,
				listen_addresses,
				supported_protocols,
				observed_address,
				..
			})) => {
				let (is_new, expired_address) =
					this.is_new_external_address(&observed_address, peer);

				if let Some(expired_address) = expired_address {
					log::trace!(
						target: LOG_TARGET,
						"Removing expired external address expired={expired_address} is_new={is_new} observed={observed_address}",
					);

					this.pending_events.push_back(DiscoveryEvent::ExternalAddressExpired {
						address: expired_address,
					});
				}

				if is_new {
					this.pending_events.push_back(DiscoveryEvent::ExternalAddressDiscovered {
						address: observed_address.clone(),
					});
				}

				return Poll::Ready(Some(DiscoveryEvent::Identified {
					peer,
					listen_addresses,
					supported_protocols,
				}));
			},
		}

		match Pin::new(&mut this.ping_event_stream).poll_next(cx) {
			Poll::Pending => {},
			Poll::Ready(None) => return Poll::Ready(None),
			Poll::Ready(Some(PingEvent::Ping { peer, ping })) =>
				return Poll::Ready(Some(DiscoveryEvent::Ping { peer, rtt: ping })),
		}

		if let Some(ref mut mdns_event_stream) = &mut this.mdns_event_stream {
			match Pin::new(mdns_event_stream).poll_next(cx) {
				Poll::Pending => {},
				Poll::Ready(None) => return Poll::Ready(None),
				Poll::Ready(Some(MdnsEvent::Discovered(addresses))) =>
					return Poll::Ready(Some(DiscoveryEvent::Discovered { addresses })),
			}
		}

		Poll::Pending
	}
}
