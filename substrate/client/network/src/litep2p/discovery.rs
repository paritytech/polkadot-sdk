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
	config::{
		NetworkConfiguration, ProtocolId, KADEMLIA_MAX_PROVIDER_KEYS, KADEMLIA_PROVIDER_RECORD_TTL,
		KADEMLIA_PROVIDER_REPUBLISH_INTERVAL,
	},
	peer_store::PeerStoreProvider,
};

use array_bytes::bytes2hex;
use futures::{FutureExt, Stream};
use futures_timer::Delay;
use ip_network::IpNetwork;
use litep2p::{
	protocol::{
		libp2p::{
			identify::{Config as IdentifyConfig, IdentifyEvent},
			kademlia::{
				Config as KademliaConfig, ConfigBuilder as KademliaConfigBuilder, ContentProvider,
				IncomingRecordValidationMode, KademliaEvent, KademliaHandle, PeerRecord, QueryId,
				Quorum, Record, RecordKey,
			},
			ping::{Config as PingConfig, PingEvent},
		},
		mdns::{Config as MdnsConfig, MdnsEvent},
	},
	types::multiaddr::{Multiaddr, Protocol},
	PeerId, ProtocolName,
};
use parking_lot::RwLock;
use sc_network_types::kad::Key as KademliaKey;
use schnellru::{ByLength, LruMap};

use std::{
	cmp,
	collections::{HashMap, HashSet, VecDeque},
	iter,
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

/// Number of times observed address is received from different peers before it is confirmed as
/// external.
const MIN_ADDRESS_CONFIRMATIONS: usize = 3;

/// Quorum threshold to interpret `PUT_VALUE` & `ADD_PROVIDER` as successful.
///
/// As opposed to libp2p, litep2p does not finish the query as soon as the required number of
/// peers have reached. Instead, it tries to put the record to all target peers (typically 20) and
/// uses the quorum setting only to determine the success of the query.
///
/// We set the threshold to 50% of the target peers to account for unreachable peers. The actual
/// number of stored records may be higher.
const QUORUM_THRESHOLD: NonZeroUsize = NonZeroUsize::new(10).expect("10 > 0; qed");

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
	///
	/// This event is emitted when a new peer is discovered over mDNS.
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

	/// `FIND_NODE` query succeeded.
	FindNodeSuccess {
		/// Query ID.
		query_id: QueryId,

		/// Target.
		target: PeerId,

		/// Found peers.
		peers: Vec<(PeerId, Vec<Multiaddr>)>,
	},

	/// `GetRecord` query succeeded.
	GetRecordSuccess {
		/// Query ID.
		query_id: QueryId,
	},

	/// Record was found from the DHT.
	GetRecordPartialResult {
		/// Query ID.
		query_id: QueryId,

		/// Record.
		record: PeerRecord,
	},

	/// Record was successfully stored on the DHT.
	PutRecordSuccess {
		/// Query ID.
		query_id: QueryId,
	},

	/// Providers were successfully retrieved.
	GetProvidersSuccess {
		/// Query ID.
		query_id: QueryId,
		/// Found providers sorted by distance to provided key.
		providers: Vec<ContentProvider>,
	},

	/// Provider was successfully published.
	AddProviderSuccess {
		/// Query ID.
		query_id: QueryId,
		/// Provided key.
		provided_key: RecordKey,
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
	/// Local peer ID.
	local_peer_id: litep2p::PeerId,

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
	random_walk_query_id: Option<QueryId>,

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
		local_peer_id: litep2p::PeerId,
		config: &NetworkConfiguration,
		genesis_hash: Hash,
		fork_id: Option<&str>,
		protocol_id: &ProtocolId,
		known_peers: HashMap<PeerId, Vec<Multiaddr>>,
		listen_addresses: Arc<RwLock<HashSet<Multiaddr>>>,
		_peerstore_handle: Arc<dyn PeerStoreProvider>,
	) -> (Self, PingConfig, IdentifyConfig, KademliaConfig, Option<MdnsConfig>) {
		let (ping_config, ping_event_stream) = PingConfig::default();
		let user_agent = format!("{} ({}) (litep2p)", config.client_version, config.node_name);

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
				.with_provider_record_ttl(KADEMLIA_PROVIDER_RECORD_TTL)
				.with_provider_refresh_interval(KADEMLIA_PROVIDER_REPUBLISH_INTERVAL)
				.with_max_provider_keys(KADEMLIA_MAX_PROVIDER_KEYS)
				.build()
		};

		(
			Self {
				local_peer_id,
				ping_event_stream,
				identify_event_stream,
				mdns_event_stream,
				kademlia_handle,
				_peerstore_handle,
				listen_addresses,
				random_walk_query_id: None,
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
			return;
		}

		let addresses = addresses
			.into_iter()
			.filter_map(|address| {
				if !self.allow_non_global_addresses && !Discovery::can_add_to_dht(&address) {
					log::trace!(
						target: LOG_TARGET,
						"ignoring self-reported non-global address {address} from {peer}."
					);

					return None;
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

	/// Start Kademlia `FIND_NODE` query for `target`.
	pub async fn find_node(&mut self, target: PeerId) -> QueryId {
		self.kademlia_handle.find_node(target).await
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
			.put_record(
				Record::new(RecordKey::new(&key.to_vec()), value),
				Quorum::N(QUORUM_THRESHOLD),
			)
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
				// These are the peers that just returned the record to us in authority-discovery,
				// so we assume they are all reachable.
				Quorum::All,
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

	/// Start providing `key`.
	pub async fn start_providing(&mut self, key: KademliaKey) -> QueryId {
		self.kademlia_handle
			.start_providing(key.into(), Quorum::N(QUORUM_THRESHOLD))
			.await
	}

	/// Stop providing `key`.
	pub async fn stop_providing(&mut self, key: KademliaKey) {
		self.kademlia_handle.stop_providing(key.into()).await;
	}

	/// Get providers for `key`.
	pub async fn get_providers(&mut self, key: KademliaKey) -> QueryId {
		self.kademlia_handle.get_providers(key.into()).await
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
			Some(Protocol::Dns(_)) | Some(Protocol::Dns4(_)) | Some(Protocol::Dns6(_)) => {
				return true;
			},
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

		if !self.allow_non_global_addresses && !Discovery::can_add_to_dht(&address) {
			log::trace!(
				target: LOG_TARGET,
				"ignoring externally reported non-global address {address} from {peer}."
			);

			return (false, None);
		}

		// is the address one of our known addresses
		if self
			.listen_addresses
			.read()
			.iter()
			.chain(self.public_addresses.iter())
			.any(|known_address| Discovery::is_known_address(&known_address, &address))
		{
			return (true, None);
		}

		match self.address_confirmations.get(address) {
			Some(confirmations) => {
				confirmations.insert(peer);

				if confirmations.len() >= MIN_ADDRESS_CONFIRMATIONS {
					return (true, None);
				}
			},
			None => {
				let oldest = (self.address_confirmations.len() >=
					self.address_confirmations.limiter().max_length() as usize)
					.then(|| {
						self.address_confirmations.pop_oldest().map(|(address, peers)| {
							if peers.len() >= MIN_ADDRESS_CONFIRMATIONS {
								return Some(address);
							} else {
								None
							}
						})
					})
					.flatten()
					.flatten();

				self.address_confirmations.insert(address.clone(), iter::once(peer).collect());

				return (false, oldest);
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
			return Poll::Ready(Some(event));
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
							this.random_walk_query_id = Some(query_id);
							return Poll::Ready(Some(DiscoveryEvent::RandomKademliaStarted));
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
			Poll::Ready(Some(KademliaEvent::FindNodeSuccess { query_id, peers, .. }))
				if Some(query_id) == this.random_walk_query_id =>
			{
				// the addresses are already inserted into the DHT and in `TransportManager` so
				// there is no need to add them again. The found peers must be registered to
				// `Peerstore` so other protocols are aware of them through `Peerset`.
				log::trace!(target: LOG_TARGET, "dht random walk yielded {} peers", peers.len());

				this.next_kad_query = Some(Delay::new(KADEMLIA_QUERY_INTERVAL));

				return Poll::Ready(Some(DiscoveryEvent::RoutingTableUpdate {
					peers: peers.into_iter().map(|(peer, _)| peer).collect(),
				}));
			},
			Poll::Ready(Some(KademliaEvent::FindNodeSuccess { query_id, target, peers })) => {
				log::trace!(target: LOG_TARGET, "find node query yielded {} peers", peers.len());

				return Poll::Ready(Some(DiscoveryEvent::FindNodeSuccess {
					query_id,
					target,
					peers,
				}));
			},
			Poll::Ready(Some(KademliaEvent::RoutingTableUpdate { peers })) => {
				log::trace!(target: LOG_TARGET, "routing table update, discovered {} peers", peers.len());

				return Poll::Ready(Some(DiscoveryEvent::RoutingTableUpdate {
					peers: peers.into_iter().collect(),
				}));
			},
			Poll::Ready(Some(KademliaEvent::GetRecordSuccess { query_id })) => {
				log::trace!(
					target: LOG_TARGET,
					"`GET_RECORD` succeeded for {query_id:?}",
				);

				return Poll::Ready(Some(DiscoveryEvent::GetRecordSuccess { query_id }));
			},
			Poll::Ready(Some(KademliaEvent::GetRecordPartialResult { query_id, record })) => {
				log::trace!(
					target: LOG_TARGET,
					"`GET_RECORD` intermediary succeeded for {query_id:?}: {record:?}",
				);

				return Poll::Ready(Some(DiscoveryEvent::GetRecordPartialResult {
					query_id,
					record,
				}));
			},
			Poll::Ready(Some(KademliaEvent::PutRecordSuccess { query_id, key: _ })) => {
				return Poll::Ready(Some(DiscoveryEvent::PutRecordSuccess { query_id }));
			},
			Poll::Ready(Some(KademliaEvent::QueryFailed { query_id })) => {
				match this.random_walk_query_id == Some(query_id) {
					true => {
						this.random_walk_query_id = None;
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

				return Poll::Ready(Some(DiscoveryEvent::IncomingRecord { record }));
			},
			Poll::Ready(Some(KademliaEvent::GetProvidersSuccess {
				provided_key,
				providers,
				query_id,
			})) => {
				log::trace!(
					target: LOG_TARGET,
					"`GET_PROVIDERS` for {query_id:?} with {provided_key:?} yielded {providers:?}",
				);

				return Poll::Ready(Some(DiscoveryEvent::GetProvidersSuccess {
					query_id,
					providers,
				}));
			},
			Poll::Ready(Some(KademliaEvent::AddProviderSuccess { query_id, provided_key })) => {
				log::trace!(
					target: LOG_TARGET,
					"`ADD_PROVIDER` for {query_id:?} with {provided_key:?} succeeded",
				);

				return Poll::Ready(Some(DiscoveryEvent::AddProviderSuccess {
					query_id,
					provided_key,
				}));
			},
			// We do not validate incoming providers.
			Poll::Ready(Some(KademliaEvent::IncomingProvider { .. })) => {},
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
				let observed_address =
					if let Some(Protocol::P2p(peer_id)) = observed_address.iter().last() {
						if peer_id != this.local_peer_id.into() {
							log::warn!(
								target: LOG_TARGET,
								"Discovered external address for a peer that is not us: {observed_address}",
							);
							None
						} else {
							Some(observed_address)
						}
					} else {
						Some(observed_address.with(Protocol::P2p(this.local_peer_id.into())))
					};

				// Ensure that an external address with a different peer ID does not have
				// side effects of evicting other external addresses via `ExternalAddressExpired`.
				if let Some(observed_address) = observed_address {
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
			Poll::Ready(Some(PingEvent::Ping { peer, ping })) => {
				return Poll::Ready(Some(DiscoveryEvent::Ping { peer, rtt: ping }));
			},
		}

		if let Some(ref mut mdns_event_stream) = &mut this.mdns_event_stream {
			match Pin::new(mdns_event_stream).poll_next(cx) {
				Poll::Pending => {},
				Poll::Ready(None) => return Poll::Ready(None),
				Poll::Ready(Some(MdnsEvent::Discovered(addresses))) => {
					return Poll::Ready(Some(DiscoveryEvent::Discovered { addresses }));
				},
			}
		}

		Poll::Pending
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};

	use crate::{
		config::ProtocolId,
		peer_store::{PeerStore, PeerStoreProvider},
	};
	use futures::{stream::FuturesUnordered, StreamExt};
	use sp_core::H256;
	use sp_tracing::tracing_subscriber;

	use litep2p::{
		config::ConfigBuilder as Litep2pConfigBuilder, transport::tcp::config::Config as TcpConfig,
		Litep2p,
	};

	#[tokio::test]
	async fn litep2p_discovery_works() {
		let _ = tracing_subscriber::fmt()
			.with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
			.try_init();

		let mut known_peers = HashMap::new();
		let genesis_hash = H256::from_low_u64_be(1);
		let fork_id = Some("test-fork-id");
		let protocol_id = ProtocolId::from("dot");

		// Build backends such that the first peer is known to all other peers.
		let backends = (0..10)
			.map(|i| {
				let keypair = litep2p::crypto::ed25519::Keypair::generate();
				let peer_id: PeerId = keypair.public().to_peer_id().into();

				let listen_addresses = Arc::new(RwLock::new(HashSet::new()));

				let peer_store = PeerStore::new(vec![], None);
				let peer_store_handle: Arc<dyn PeerStoreProvider> = Arc::new(peer_store.handle());

				let (discovery, ping_config, identify_config, kademlia_config, _mdns) =
					Discovery::new(
						peer_id,
						&NetworkConfiguration::new_local(),
						genesis_hash,
						fork_id,
						&protocol_id,
						known_peers.clone(),
						listen_addresses.clone(),
						peer_store_handle,
					);

				let config = Litep2pConfigBuilder::new()
					.with_keypair(keypair)
					.with_tcp(TcpConfig {
						listen_addresses: vec!["/ip6/::1/tcp/0".parse().unwrap()],
						..Default::default()
					})
					.with_libp2p_ping(ping_config)
					.with_libp2p_identify(identify_config)
					.with_libp2p_kademlia(kademlia_config)
					.build();

				let mut litep2p = Litep2p::new(config).unwrap();

				let addresses = litep2p.listen_addresses().cloned().collect::<Vec<_>>();
				// Propagate addresses to discovery.
				addresses.iter().for_each(|address| {
					listen_addresses.write().insert(address.clone());
				});

				// Except the first peer, all other peers know the first peer addresses.
				if i == 0 {
					log::info!(target: LOG_TARGET, "First peer is {peer_id:?} with addresses {addresses:?}");
					known_peers.insert(peer_id, addresses.clone());
				} else {
					let (peer, addresses) = known_peers.iter().next().unwrap();

					let result = litep2p.add_known_address(*peer, addresses.into_iter().cloned());

					log::info!(target: LOG_TARGET, "{peer_id:?}: Adding known peer {peer:?} with addresses {addresses:?} result={result:?}");

				}

				(peer_id, litep2p, discovery)
			})
			.collect::<Vec<_>>();

		let total_peers = backends.len() as u32;
		let remaining_peers =
			backends.iter().map(|(peer_id, _, _)| *peer_id).collect::<HashSet<_>>();

		let first_peer = *known_peers.iter().next().unwrap().0;

		// Each backend must discover the whole network.
		let mut futures = FuturesUnordered::new();
		let num_finished = Arc::new(AtomicU32::new(0));

		for (peer_id, mut litep2p, mut discovery) in backends {
			// Remove the local peer id from the set.
			let mut remaining_peers = remaining_peers.clone();
			remaining_peers.remove(&peer_id);

			let num_finished = num_finished.clone();

			let future = async move {
				log::info!(target: LOG_TARGET, "{peer_id:?} starting loop");

				if peer_id != first_peer {
					log::info!(target: LOG_TARGET, "{peer_id:?} dialing {first_peer:?}");
					litep2p.dial(&first_peer).await.unwrap();
				}

				loop {
					// We need to keep the network alive until all peers are discovered.
					if num_finished.load(std::sync::atomic::Ordering::Relaxed) == total_peers {
						log::info!(target: LOG_TARGET, "{peer_id:?} all peers discovered");
						break;
					}

					tokio::select! {
						// Drive litep2p backend forward.
						event = litep2p.next_event() => {
							log::info!(target: LOG_TARGET, "{peer_id:?} Litep2p event: {event:?}");
						},

						// Detect discovery events.
						event = discovery.next() => {
							match event.unwrap() {
								// We have discovered the peer via kademlia and established
								// a connection on the identify protocol.
								DiscoveryEvent::Identified { peer, .. } => {
									log::info!(target: LOG_TARGET, "{peer_id:?} Peer {peer} identified");

									remaining_peers.remove(&peer);

									if remaining_peers.is_empty() {
										log::info!(target: LOG_TARGET, "{peer_id:?} All peers discovered");

										num_finished.fetch_add(1, std::sync::atomic::Ordering::AcqRel);
									}
								},

								event => {
									log::info!(target: LOG_TARGET, "{peer_id:?} Discovery event: {event:?}");
								}
							}
						}
					}
				}
			};

			futures.push(future);
		}

		// Futures will exit when all peers are discovered.
		tokio::time::timeout(Duration::from_secs(60), futures.next())
			.await
			.expect("All peers should finish within 60 seconds");
	}

	/// Coverage experiment for the claim: "running `next_kad_random_query` for enough random keys
	/// causes `RoutingTableUpdate` to be called for almost all nodes in the network".
	///
	/// Each node accumulates the *uncapped union* of peers ever surfaced via
	/// `DiscoveryEvent::RoutingTableUpdate` (NOT the 20/bucket-capped retained table) and counts
	/// the random Kademlia queries it issues (`DiscoveryEvent::RandomKademliaStarted`). We then
	/// report coverage as a function of issued queries (machine-independent) and wall-clock, plus
	/// the real-distributed-network time estimate `queries * KADEMLIA_QUERY_INTERVAL`.
	///
	/// Connections self-bound via litep2p's idle-reaping (~10s), so the steady-state CONNECTED
	/// count stays ~tens even as the surfaced union approaches all N — we do NOT hard-cap dials
	/// (litep2p rejects, rather than evicts, at the cap, which would make Kademlia permanently
	/// skip un-dialable peers and corrupt the metric).
	///
	/// Parametrized via env vars so the feasibility ramp (10 -> 1000) needs no recompile:
	///   `KAD_COV_N`      number of nodes               (default 10)
	///   `KAD_COV_SECS`   run duration in seconds; the MAX (safety) cap when a target is set (def
	/// 30) `KAD_COV_TARGET` stop once the network MEAN coverage reaches this fraction (default
	/// 1.0 =                    run to the cap). A coordinator polls live coverage and reports the
	/// time the                    target is reached, plus the worst node and the count of nodes
	/// >= target.   `KAD_COV_FNAT`   fraction of NAT-unreachable nodes in [0,1) (default 0.0).
	/// These nodes dial                    out and discover peers but have no dialable address, so
	/// they are never                    surfaced; absolute coverage of all N then plateaus near
	/// `1 - f_nat`.
	///
	/// Ignored by default (spawns N full litep2p stacks). Example (run until mean reaches 99%):
	///   `ulimit -n 1048576; KAD_COV_N=1000 KAD_COV_TARGET=0.99 KAD_COV_SECS=1800 cargo test \`
	///   `  -p sc-network --lib -- --ignored --nocapture \`
	///   `  --exact 'litep2p::discovery::tests::litep2p_discovery_coverage'`
	#[tokio::test(flavor = "multi_thread")]
	#[ignore]
	async fn litep2p_discovery_coverage() {
		let _ = tracing_subscriber::fmt()
			.with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
			.try_init();

		// Per-node results. All coverage/connection figures are fractions of the relevant node
		// count.
		struct NodeStat {
			idx: usize,      // node index (0 = bootstrap seed)
			peer_id: PeerId, // random ed25519/OsRng peer id
			queries: u64,    // random Kademlia queries issued over the run
			cov: f64,        // RoutingTableUpdate-union coverage of the reachable set
			idcov: f64,      // Identified coverage of the reachable set
			abs: f64,        // surfaced fraction of ALL N-1 others (incl. NAT'd)
			rtu_events: u64,
			conn_peak: usize,     // max simultaneous connections
			conn_distinct: usize, // distinct peers EVER connected to (in or out)
			dialed_out: usize,    // distinct peers WE dialed (outbound, our queries)
			dialed_in: usize,     /* distinct peers that dialed US (inbound, their
			                       * queries) */
			timeline: Vec<(u64, u64, f64)>, // (elapsed_secs, queries, coverage_fraction)
		}

		let n: usize = std::env::var("KAD_COV_N").ok().and_then(|s| s.parse().ok()).unwrap_or(10);
		// `KAD_COV_SECS` is the MAX (safety) cap; the run stops earlier once the coverage target is
		// reached.
		let run_for = Duration::from_secs(
			std::env::var("KAD_COV_SECS").ok().and_then(|s| s.parse().ok()).unwrap_or(30),
		);
		// Stop as soon as the network MEAN coverage reaches this fraction (default 1.0 = run to the
		// cap). The worst node and the count of nodes >= target are reported at the stop point.
		let target: f64 =
			std::env::var("KAD_COV_TARGET").ok().and_then(|s| s.parse().ok()).unwrap_or(1.0);
		// Fraction of nodes that are NAT-unreachable: they dial out and discover peers, but have no
		// dialable listen address to advertise, so peers never retain/return them via the DHT (they
		// can never appear in any `RoutingTableUpdate`). This is the production discoverability
		// ceiling: absolute coverage of all N plateaus near `1 - f_nat`.
		let f_nat: f64 =
			std::env::var("KAD_COV_FNAT").ok().and_then(|s| s.parse().ok()).unwrap_or(0.0);
		assert!(n >= 2, "need at least 2 nodes");
		assert!((0.0..1.0).contains(&f_nat), "KAD_COV_FNAT must be in [0, 1)");
		// NAT'd nodes are indices `1..=n_nat`; the seed (index 0) always stays reachable so it can
		// serve as the bootstrap hub.
		let n_nat = ((n - 1) as f64 * f_nat).round() as usize;

		let mut known_peers = HashMap::new();
		let genesis_hash = H256::from_low_u64_be(1);
		let fork_id = Some("test-fork-id");
		let protocol_id = ProtocolId::from("dot");

		// Single-seed bootstrap: peer 0 is known to all other peers.
		let backends = (0..n)
			.map(|i| {
				let keypair = litep2p::crypto::ed25519::Keypair::generate();
				let peer_id: PeerId = keypair.public().to_peer_id().into();
				let listen_addresses = Arc::new(RwLock::new(HashSet::new()));
				let peer_store = PeerStore::new(vec![], None);
				let peer_store_handle: Arc<dyn PeerStoreProvider> = Arc::new(peer_store.handle());

				let (discovery, ping_config, identify_config, kademlia_config, _mdns) =
					Discovery::new(
						peer_id,
						&NetworkConfiguration::new_local(),
						genesis_hash,
						fork_id,
						&protocol_id,
						known_peers.clone(),
						listen_addresses.clone(),
						peer_store_handle,
					);

				// NAT'd nodes (indices `1..=n_nat`) listen on nothing: outbound-only, no dialable
				// address to advertise.
				let is_nat = i >= 1 && i <= n_nat;
				let tcp_listen =
					if is_nat { vec![] } else { vec!["/ip6/::1/tcp/0".parse().unwrap()] };

				let config = Litep2pConfigBuilder::new()
					.with_keypair(keypair)
					.with_tcp(TcpConfig { listen_addresses: tcp_listen, ..Default::default() })
					.with_libp2p_ping(ping_config)
					.with_libp2p_identify(identify_config)
					.with_libp2p_kademlia(kademlia_config)
					.build();

				let mut litep2p = Litep2p::new(config).unwrap();
				let addresses = litep2p.listen_addresses().cloned().collect::<Vec<_>>();
				addresses.iter().for_each(|address| {
					listen_addresses.write().insert(address.clone());
				});

				if i == 0 {
					known_peers.insert(peer_id, addresses.clone());
				} else {
					let (peer, addresses) = known_peers.iter().next().unwrap();
					let _ = litep2p.add_known_address(*peer, addresses.iter().cloned());
				}

				(peer_id, litep2p, discovery, is_nat)
			})
			.collect::<Vec<_>>();

		// Reachable target set = every node that is NOT NAT'd (these are the peers that can ever be
		// surfaced via the DHT). Coverage is measured against this set.
		let reachable_peers: HashSet<PeerId> = backends
			.iter()
			.filter(|(.., is_nat)| !*is_nat)
			.map(|(peer_id, ..)| *peer_id)
			.collect();
		let first_peer = *known_peers.iter().next().unwrap().0;
		println!(
			"nodes={n}  NAT-unreachable={n_nat} (f_nat={f_nat})  reachable={}",
			reachable_peers.len(),
		);

		// Spawn each node's event loop as a separate task on the multi-threaded runtime so the
		// loops (and litep2p's internal transport tasks) run across real cores rather than
		// serializing on one thread — important at large N, where a single-thread driver
		// collapses the effective per-node query cadence. Each task runs until the deadline, then
		// returns its coverage stats.
		// Shared live-coverage state for the coordinator: each node publishes its coverage in basis
		// points (×10000); the coordinator stops the whole run once the network MEAN reaches the
		// target, so we measure exactly when 99% (etc.) is reached rather than cutting off at a
		// fixed time. `run_for` is the safety cap.
		let stop = Arc::new(AtomicBool::new(false));
		let cov_bp: Arc<Vec<AtomicU32>> = Arc::new((0..n).map(|_| AtomicU32::new(0)).collect());

		let mut handles = Vec::new();
		for (idx, (peer_id, mut litep2p, mut discovery, _is_nat)) in
			backends.into_iter().enumerate()
		{
			let target_set = reachable_peers.clone();
			// Reachable peers other than self.
			let denom =
				(target_set.len() - usize::from(target_set.contains(&peer_id))).max(1) as f64;
			let stop = stop.clone();
			let cov_bp = cov_bp.clone();
			handles.push(tokio::spawn(async move {
				if peer_id != first_peer {
					litep2p.dial(&first_peer).await.expect("bootstrap dial to first peer");
				}

				let mut surfaced: HashSet<PeerId> = HashSet::new();
				let mut identified: HashSet<PeerId> = HashSet::new();
				let mut rtu_events: u64 = 0;
				let mut queries: u64 = 0;
				// Live connection count and its peak (steady-state CONNECTED quantity), plus the
				// cumulative set of DISTINCT peers ever connected to (the connection footprint over
				// the whole run — distinct from the simultaneous peak and from the surfaced union).
				let mut conns: usize = 0;
				let mut conn_peak: usize = 0;
				let mut connected_ever: HashSet<PeerId> = HashSet::new();
				// Split the footprint by who initiated: `dialed_out` = we dialed them (caused by
				// our own Kademlia queries), `dialed_in` = they dialed us (their queries
				// reaching us).
				let mut dialed_out: HashSet<PeerId> = HashSet::new();
				let mut dialed_in: HashSet<PeerId> = HashSet::new();
				// Timeline samples: (elapsed_secs, queries_issued, coverage_fraction_of_reachable).
				let mut timeline: Vec<(u64, u64, f64)> = Vec::new();

				let start = tokio::time::Instant::now();
				let mut last_sample = 0u64;

				loop {
					// Stop on the safety cap or when the coordinator signals the target was
					// reached.
					if start.elapsed() >= run_for || stop.load(Ordering::Relaxed) {
						break;
					}
					let elapsed = start.elapsed().as_secs();
					if elapsed > last_sample {
						last_sample = elapsed;
						let frac = surfaced.iter().filter(|p| target_set.contains(*p)).count()
							as f64 / denom;
						cov_bp[idx].store((frac * 10_000.0) as u32, Ordering::Relaxed);
						timeline.push((elapsed, queries, frac));
					}

					// Identify info to feed into the Kademlia routing table after the borrow of
					// `discovery` from `discovery.next()` is released (mirrors the real network
					// worker at `litep2p/mod.rs:1124`).
					let mut to_add: Option<(PeerId, HashSet<ProtocolName>, Vec<Multiaddr>)> = None;

					tokio::select! {
						// Periodic wake (≈1s) so the loop promptly re-checks the stop flag / cap even
						// when no protocol events are flowing; one branch among many, so it does not
						// starve the event branches.
						_ = tokio::time::sleep(Duration::from_secs(1)) => {},
						event = litep2p.next_event() => {
							match event {
								Some(litep2p::Litep2pEvent::ConnectionEstablished {
									peer,
									endpoint,
								}) => {
									conns += 1;
									conn_peak = conn_peak.max(conns);
									connected_ever.insert(peer);
									// `Listener` = inbound (they dialed us); otherwise outbound.
									if endpoint.is_listener() {
										dialed_in.insert(peer);
									} else {
										dialed_out.insert(peer);
									}
								},
								Some(litep2p::Litep2pEvent::ConnectionClosed { .. }) => {
									conns = conns.saturating_sub(1);
								},
								_ => {},
							}
						},
						event = discovery.next() => {
							match event {
								Some(DiscoveryEvent::RoutingTableUpdate { peers }) => {
									rtu_events += 1;
									surfaced.extend(peers);
								},
								Some(DiscoveryEvent::Identified {
									peer,
									listen_addresses,
									supported_protocols,
								}) => {
									identified.insert(peer);
									to_add = Some((peer, supported_protocols, listen_addresses));
								},
								Some(DiscoveryEvent::RandomKademliaStarted) => {
									queries += 1;
								},
								Some(_) => {},
								None => break,
							}
						},
					}

					// Feed self-reported (identify) addresses into the Kademlia routing table, as
					// the real network worker does. Without this the routing tables never gain
					// dialable addresses and discovery cannot cascade past the bootstrap peer.
					if let Some((peer, protos, addrs)) = to_add {
						discovery.add_self_reported_address(peer, protos, addrs).await;
					}
				}

				let others = (n - 1) as f64;
				NodeStat {
					idx,
					peer_id,
					queries,
					cov: surfaced.iter().filter(|p| target_set.contains(*p)).count() as f64 / denom,
					idcov: identified.iter().filter(|p| target_set.contains(*p)).count() as f64 /
						denom,
					abs: surfaced.len() as f64 / others,
					rtu_events,
					conn_peak,
					conn_distinct: connected_ever.len(),
					dialed_out: dialed_out.len(),
					dialed_in: dialed_in.len(),
					timeline,
				}
			}));
		}

		// Coordinator: poll live coverage every few seconds; stop the run as soon as the network
		// mean reaches the target (or the safety cap fires).
		let coord_start = tokio::time::Instant::now();
		let mut reached_at: Option<u64> = None;
		while coord_start.elapsed() < run_for {
			tokio::time::sleep(Duration::from_secs(5)).await;
			let vals: Vec<f64> =
				cov_bp.iter().map(|a| a.load(Ordering::Relaxed) as f64 / 10_000.0).collect();
			let mean = vals.iter().sum::<f64>() / n as f64;
			let worst = vals.iter().copied().fold(f64::INFINITY, f64::min);
			let at_target = vals.iter().filter(|&&c| c >= target).count();
			let secs = coord_start.elapsed().as_secs();
			println!(
				"  [coord t={secs:>4}s] mean={:>5.1}%  worst={:>5.1}%  nodes>={:.0}%: {at_target}/{n}",
				mean * 100.0,
				worst * 100.0,
				target * 100.0,
			);
			if mean >= target {
				reached_at = Some(secs);
				stop.store(true, Ordering::Relaxed);
				break;
			}
		}
		match reached_at {
			Some(s) => {
				println!("*** target mean coverage {:.0}% reached at t={s}s ***", target * 100.0)
			},
			None => println!(
				"*** target mean {:.0}% NOT reached within cap {}s ***",
				target * 100.0,
				run_for.as_secs()
			),
		}

		let mut results = Vec::new();
		for h in handles {
			results.push(h.await.unwrap());
		}

		// ---- analysis ----
		let cadence = KADEMLIA_QUERY_INTERVAL.as_secs();
		println!(
			"\n=== Kademlia RoutingTableUpdate coverage: N={n}, run={}s ===",
			run_for.as_secs()
		);
		let mut finals: Vec<f64> = results.iter().map(|r| r.cov).collect();
		finals.sort_by(|a, b| a.partial_cmp(b).unwrap());
		let mean_final = finals.iter().sum::<f64>() / finals.len() as f64;
		let worst_final = finals.first().copied().unwrap_or(0.0);
		let total_queries: u64 = results.iter().map(|r| r.queries).sum();
		let total_rtu: u64 = results.iter().map(|r| r.rtu_events).sum();
		let mut id_finals: Vec<f64> = results.iter().map(|r| r.idcov).collect();
		id_finals.sort_by(|a, b| a.partial_cmp(b).unwrap());
		let mean_id = id_finals.iter().sum::<f64>() / id_finals.len() as f64;
		println!(
			"final RoutingTableUpdate-union coverage: mean={:.1}%  worst={:.1}%  median={:.1}%",
			mean_final * 100.0,
			worst_final * 100.0,
			finals[finals.len() / 2] * 100.0,
		);
		println!(
			"final Identified coverage:               mean={:.1}%  worst={:.1}%",
			mean_id * 100.0,
			id_finals.first().copied().unwrap_or(0.0) * 100.0,
		);
		let mut abs_finals: Vec<f64> = results.iter().map(|r| r.abs).collect();
		abs_finals.sort_by(|a, b| a.partial_cmp(b).unwrap());
		let mean_abs = abs_finals.iter().sum::<f64>() / abs_finals.len() as f64;
		println!(
			"absolute coverage (all N, incl. NAT'd):  mean={:.1}%  worst={:.1}%  (ceiling ~{:.1}%)",
			mean_abs * 100.0,
			abs_finals.first().copied().unwrap_or(0.0) * 100.0,
			(reachable_peers.len() as f64 - 1.0).max(0.0) / (n - 1) as f64 * 100.0,
		);
		println!(
			"total random queries={total_queries}  total RoutingTableUpdate events={total_rtu}  \
			 (avg {:.1} events/node)",
			total_rtu as f64 / n as f64,
		);

		// CONNECTED vs RETAINED vs CALLED: peak simultaneous connections should stay ~tens even
		// though the surfaced union -> ~all N (the retained k-bucket table caps at ~k*log2(N)).
		let mut conn_peaks: Vec<usize> = results.iter().map(|r| r.conn_peak).collect();
		conn_peaks.sort_unstable();
		println!(
			"peak simultaneous connections per node (CONNECTED): mean={:.1}  median={}  max={}",
			conn_peaks.iter().sum::<usize>() as f64 / conn_peaks.len() as f64,
			conn_peaks[conn_peaks.len() / 2],
			conn_peaks.last().copied().unwrap_or(0),
		);

		// Distinct peers EVER connected to over the whole run (the connection footprint), as a
		// fraction of all other nodes. Bigger than the simultaneous peak (connections are reaped
		// and re-dialed) but typically smaller than the surfaced union (you surface peers from
		// responses without ever connecting to them).
		let others_f = (n - 1) as f64;
		let mut conn_distincts: Vec<f64> =
			results.iter().map(|r| r.conn_distinct as f64 / others_f).collect();
		conn_distincts.sort_by(|a, b| a.partial_cmp(b).unwrap());
		let mean_pct = |sel: fn(&NodeStat) -> usize| {
			results.iter().map(|r| sel(r) as f64).sum::<f64>() / results.len() as f64 / others_f *
				100.0
		};
		println!(
			"distinct peers ever connected to: mean={:.1}%  median={:.1}%  worst={:.1}%  best={:.1}% (of {} others)",
			conn_distincts.iter().sum::<f64>() / conn_distincts.len() as f64 * 100.0,
			conn_distincts[conn_distincts.len() / 2] * 100.0,
			conn_distincts.first().copied().unwrap_or(0.0) * 100.0,
			conn_distincts.last().copied().unwrap_or(0.0) * 100.0,
			n - 1,
		);
		// The footprint splits into peers WE dialed (caused by our own Kademlia FIND_NODE queries
		// reaching out to candidates) and peers that dialed US (their queries reaching us). A peer
		// can be in both sets, so these can sum to more than the distinct total.
		println!(
			"  split (distinct, mean): dialed-OUT (our queries)={:.1}%  dialed-IN (their queries)={:.1}%",
			mean_pct(|r| r.dialed_out),
			mean_pct(|r| r.dialed_in),
		);

		for thr in [0.90_f64, 0.95, 0.99] {
			let mut secs_to: Vec<u64> = Vec::new();
			let mut queries_to: Vec<u64> = Vec::new();
			for r in &results {
				if let Some((s, q, _)) = r.timeline.iter().find(|(_, _, cov)| *cov >= thr) {
					secs_to.push(*s);
					queries_to.push(*q);
				}
			}
			let reached = queries_to.len();
			let avg =
				|v: &[u64]| if v.is_empty() { 0 } else { v.iter().sum::<u64>() / v.len() as u64 };
			let mean_q = avg(&queries_to);
			let worst_q = queries_to.iter().copied().max().unwrap_or(0);
			println!(
				"coverage>={:.0}%: {reached}/{n} nodes | queries-to mean={mean_q} worst={worst_q} | \
				 wall-clock-s mean={} worst={} | est real-net time (queries*{cadence}s) mean={}s worst={}s",
				thr * 100.0,
				avg(&secs_to),
				secs_to.iter().copied().max().unwrap_or(0),
				mean_q * cadence,
				worst_q * cadence,
			);
		}

		// Aggregate coverage-vs-time curve (mean and worst node), ~12 points across the run.
		let max_t = run_for.as_secs();
		let step = (max_t / 12).max(1);
		println!("coverage curve (RoutingTableUpdate union, fraction of reachable):");
		let mut t = 0;
		while t <= max_t {
			let mut covs: Vec<f64> = Vec::with_capacity(results.len());
			for r in &results {
				let cov = r
					.timeline
					.iter()
					.take_while(|(s, _, _)| *s <= t)
					.last()
					.map(|(_, _, c)| *c)
					.unwrap_or(0.0);
				covs.push(cov);
			}
			let mean = covs.iter().sum::<f64>() / covs.len() as f64;
			let worst = covs.iter().copied().fold(f64::INFINITY, f64::min);
			println!("  t={t:>4}s  mean={:>5.1}%  worst={:>5.1}%", mean * 100.0, worst * 100.0);
			t += step;
		}

		// ---- per-node statistics ----
		// One line per node. `to-X%` = (queries issued / wall-clock secs) when that node first
		// surfaced X% of the reachable set; `-` if it never reached X within the run. The peer ids
		// are random ed25519 keys (OsRng), so the Kademlia keyspace positions are uniform.
		// `dial_out`/`dial_in` = distinct peers this node dialed / was dialed by; `peak` = max
		// simultaneous connections; `distinct` = distinct peers ever connected to (either
		// direction).
		let mut sorted: Vec<&NodeStat> = results.iter().collect();
		sorted.sort_by_key(|r| r.idx);
		println!(
			"\n--- per-node statistics (N={n}, {} reachable others) ---",
			reachable_peers.len()
		);
		println!(
			"{:>4}  {:<52} {:>12} {:>12} {:>12} | {:>8} {:>7} {:>4} {:>8} {:>6}",
			"node",
			"peer_id",
			"to-90%",
			"to-95%",
			"to-99%",
			"dial_out",
			"dial_in",
			"peak",
			"distinct",
			"final",
		);
		for r in &sorted {
			let cross = |thr: f64| {
				r.timeline
					.iter()
					.find(|(_, _, c)| *c >= thr)
					.map(|(s, q, _)| format!("{q}q/{s}s"))
					.unwrap_or_else(|| "-".to_string())
			};
			println!(
				"{:>4}  {:<52} {:>12} {:>12} {:>12} | {:>8} {:>7} {:>4} {:>8} {:>5.1}%",
				r.idx,
				r.peer_id.to_string(),
				cross(0.90),
				cross(0.95),
				cross(0.99),
				r.dialed_out,
				r.dialed_in,
				r.conn_peak,
				r.conn_distinct,
				r.cov * 100.0,
			);
		}
	}
}
