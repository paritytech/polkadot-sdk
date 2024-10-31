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

//! `NetworkBackend` implementation for `litep2p`.

use crate::{
	config::{
		FullNetworkConfiguration, IncomingRequest, NodeKeyConfig, NotificationHandshake, Params,
		SetConfig, TransportConfig,
	},
	error::Error,
	event::{DhtEvent, Event},
	litep2p::{
		discovery::{Discovery, DiscoveryEvent},
		peerstore::Peerstore,
		service::{Litep2pNetworkService, NetworkServiceCommand},
		shim::{
			bitswap::BitswapServer,
			notification::{
				config::{NotificationProtocolConfig, ProtocolControlHandle},
				peerset::PeersetCommand,
			},
			request_response::{RequestResponseConfig, RequestResponseProtocol},
		},
	},
	peer_store::PeerStoreProvider,
	protocol,
	service::{
		metrics::{register_without_sources, MetricSources, Metrics, NotificationMetrics},
		out_events,
		traits::{BandwidthSink, NetworkBackend, NetworkService},
	},
	NetworkStatus, NotificationService, ProtocolName,
};

use codec::Encode;
use futures::StreamExt;
use libp2p::kad::{PeerRecord, Record as P2PRecord, RecordKey};
use litep2p::{
	config::ConfigBuilder,
	crypto::ed25519::Keypair,
	error::{DialError, NegotiationError},
	executor::Executor,
	protocol::{
		libp2p::{
			bitswap::Config as BitswapConfig,
			kademlia::{QueryId, Record, RecordsType},
		},
		request_response::ConfigBuilder as RequestResponseConfigBuilder,
	},
	transport::{
		tcp::config::Config as TcpTransportConfig,
		websocket::config::Config as WebSocketTransportConfig, ConnectionLimitsConfig, Endpoint,
	},
	types::{
		multiaddr::{Multiaddr, Protocol},
		ConnectionId,
	},
	Litep2p, Litep2pEvent, ProtocolName as Litep2pProtocolName,
};
use prometheus_endpoint::Registry;

use sc_client_api::BlockBackend;
use sc_network_common::{role::Roles, ExHashT};
use sc_network_types::PeerId;
use sc_utils::mpsc::{tracing_unbounded, TracingUnboundedReceiver};
use sp_runtime::traits::Block as BlockT;

use std::{
	cmp,
	collections::{hash_map::Entry, HashMap, HashSet},
	fs,
	future::Future,
	iter,
	pin::Pin,
	sync::{
		atomic::{AtomicUsize, Ordering},
		Arc,
	},
	time::{Duration, Instant},
};

mod discovery;
mod peerstore;
mod service;
mod shim;

/// Litep2p bandwidth sink.
struct Litep2pBandwidthSink {
	sink: litep2p::BandwidthSink,
}

impl BandwidthSink for Litep2pBandwidthSink {
	fn total_inbound(&self) -> u64 {
		self.sink.inbound() as u64
	}

	fn total_outbound(&self) -> u64 {
		self.sink.outbound() as u64
	}
}

/// Litep2p task executor.
struct Litep2pExecutor {
	/// Executor.
	executor: Box<dyn Fn(Pin<Box<dyn Future<Output = ()> + Send>>) + Send + Sync>,
}

impl Executor for Litep2pExecutor {
	fn run(&self, future: Pin<Box<dyn Future<Output = ()> + Send>>) {
		(self.executor)(future)
	}

	fn run_with_name(&self, _: &'static str, future: Pin<Box<dyn Future<Output = ()> + Send>>) {
		(self.executor)(future)
	}
}

/// Logging target for the file.
const LOG_TARGET: &str = "sub-libp2p";

/// Peer context.
struct ConnectionContext {
	/// Peer endpoints.
	endpoints: HashMap<ConnectionId, Endpoint>,

	/// Number of active connections.
	num_connections: usize,
}

/// Networking backend for `litep2p`.
pub struct Litep2pNetworkBackend {
	/// Main `litep2p` object.
	litep2p: Litep2p,

	/// `NetworkService` implementation for `Litep2pNetworkBackend`.
	network_service: Arc<dyn NetworkService>,

	/// RX channel for receiving commands from `Litep2pNetworkService`.
	cmd_rx: TracingUnboundedReceiver<NetworkServiceCommand>,

	/// `Peerset` handles to notification protocols.
	peerset_handles: HashMap<ProtocolName, ProtocolControlHandle>,

	/// Pending `GET_VALUE` queries.
	pending_get_values: HashMap<QueryId, (RecordKey, Instant)>,

	/// Pending `PUT_VALUE` queries.
	pending_put_values: HashMap<QueryId, (RecordKey, Instant)>,

	/// Discovery.
	discovery: Discovery,

	/// Number of connected peers.
	num_connected: Arc<AtomicUsize>,

	/// Connected peers.
	peers: HashMap<litep2p::PeerId, ConnectionContext>,

	/// Peerstore.
	peerstore_handle: Arc<dyn PeerStoreProvider>,

	/// Block announce protocol name.
	block_announce_protocol: ProtocolName,

	/// Sender for DHT events.
	event_streams: out_events::OutChannels,

	/// Prometheus metrics.
	metrics: Option<Metrics>,
}

impl Litep2pNetworkBackend {
	/// From an iterator of multiaddress(es), parse and group all addresses of peers
	/// so that litep2p can consume the information easily.
	fn parse_addresses(
		addresses: impl Iterator<Item = Multiaddr>,
	) -> HashMap<PeerId, Vec<Multiaddr>> {
		addresses
			.into_iter()
			.filter_map(|address| match address.iter().next() {
				Some(
					Protocol::Dns(_) |
					Protocol::Dns4(_) |
					Protocol::Dns6(_) |
					Protocol::Ip6(_) |
					Protocol::Ip4(_),
				) => match address.iter().find(|protocol| std::matches!(protocol, Protocol::P2p(_)))
				{
					Some(Protocol::P2p(multihash)) => PeerId::from_multihash(multihash.into())
						.map_or(None, |peer| Some((peer, Some(address)))),
					_ => None,
				},
				Some(Protocol::P2p(multihash)) =>
					PeerId::from_multihash(multihash.into()).map_or(None, |peer| Some((peer, None))),
				_ => None,
			})
			.fold(HashMap::new(), |mut acc, (peer, maybe_address)| {
				let entry = acc.entry(peer).or_default();
				maybe_address.map(|address| entry.push(address));

				acc
			})
	}

	/// Add new known addresses to `litep2p` and return the parsed peer IDs.
	fn add_addresses(&mut self, peers: impl Iterator<Item = Multiaddr>) -> HashSet<PeerId> {
		Self::parse_addresses(peers.into_iter())
			.into_iter()
			.filter_map(|(peer, addresses)| {
				// `peers` contained multiaddress in the form `/p2p/<peer ID>`
				if addresses.is_empty() {
					return Some(peer)
				}

				if self.litep2p.add_known_address(peer.into(), addresses.clone().into_iter()) == 0 {
					log::warn!(
						target: LOG_TARGET,
						"couldn't add any addresses for {peer:?} and it won't be added as reserved peer",
					);
					return None
				}

				self.peerstore_handle.add_known_peer(peer);
				Some(peer)
			})
			.collect()
	}
}

impl Litep2pNetworkBackend {
	/// Get `litep2p` keypair from `NodeKeyConfig`.
	fn get_keypair(node_key: &NodeKeyConfig) -> Result<(Keypair, litep2p::PeerId), Error> {
		let secret: litep2p::crypto::ed25519::SecretKey =
			node_key.clone().into_keypair()?.secret().into();

		let local_identity = Keypair::from(secret);
		let local_public = local_identity.public();
		let local_peer_id = local_public.to_peer_id();

		Ok((local_identity, local_peer_id))
	}

	/// Configure transport protocols for `Litep2pNetworkBackend`.
	fn configure_transport<B: BlockT + 'static, H: ExHashT>(
		config: &FullNetworkConfiguration<B, H, Self>,
	) -> ConfigBuilder {
		let _ = match config.network_config.transport {
			TransportConfig::MemoryOnly => panic!("memory transport not supported"),
			TransportConfig::Normal { .. } => false,
		};
		let config_builder = ConfigBuilder::new();

		// The yamux buffer size limit is configured to be equal to the maximum frame size
		// of all protocols. 10 bytes are added to each limit for the length prefix that
		// is not included in the upper layer protocols limit but is still present in the
		// yamux buffer. These 10 bytes correspond to the maximum size required to encode
		// a variable-length-encoding 64bits number. In other words, we make the
		// assumption that no notification larger than 2^64 will ever be sent.
		let yamux_maximum_buffer_size = {
			let requests_max = config
				.request_response_protocols
				.iter()
				.map(|cfg| usize::try_from(cfg.max_request_size).unwrap_or(usize::MAX));
			let responses_max = config
				.request_response_protocols
				.iter()
				.map(|cfg| usize::try_from(cfg.max_response_size).unwrap_or(usize::MAX));
			let notifs_max = config
				.notification_protocols
				.iter()
				.map(|cfg| usize::try_from(cfg.max_notification_size()).unwrap_or(usize::MAX));

			// A "default" max is added to cover all the other protocols: ping, identify,
			// kademlia, block announces, and transactions.
			let default_max = cmp::max(
				1024 * 1024,
				usize::try_from(protocol::BLOCK_ANNOUNCES_TRANSACTIONS_SUBSTREAM_SIZE)
					.unwrap_or(usize::MAX),
			);

			iter::once(default_max)
				.chain(requests_max)
				.chain(responses_max)
				.chain(notifs_max)
				.max()
				.expect("iterator known to always yield at least one element; qed")
				.saturating_add(10)
		};

		let yamux_config = {
			let mut yamux_config = litep2p::yamux::Config::default();
			// Enable proper flow-control: window updates are only sent when
			// buffered data has been consumed.
			yamux_config.set_window_update_mode(litep2p::yamux::WindowUpdateMode::OnRead);
			yamux_config.set_max_buffer_size(yamux_maximum_buffer_size);

			if let Some(yamux_window_size) = config.network_config.yamux_window_size {
				yamux_config.set_receive_window(yamux_window_size);
			}

			yamux_config
		};

		let (tcp, websocket): (Vec<Option<_>>, Vec<Option<_>>) = config
			.network_config
			.listen_addresses
			.iter()
			.filter_map(|address| {
				use sc_network_types::multiaddr::Protocol;

				let mut iter = address.iter();

				match iter.next() {
					Some(Protocol::Ip4(_) | Protocol::Ip6(_)) => {},
					protocol => {
						log::error!(
							target: LOG_TARGET,
							"unknown protocol {protocol:?}, ignoring {address:?}",
						);

						return None
					},
				}

				match iter.next() {
					Some(Protocol::Tcp(_)) => match iter.next() {
						Some(Protocol::Ws(_) | Protocol::Wss(_)) =>
							Some((None, Some(address.clone()))),
						Some(Protocol::P2p(_)) | None => Some((Some(address.clone()), None)),
						protocol => {
							log::error!(
								target: LOG_TARGET,
								"unknown protocol {protocol:?}, ignoring {address:?}",
							);
							None
						},
					},
					protocol => {
						log::error!(
							target: LOG_TARGET,
							"unknown protocol {protocol:?}, ignoring {address:?}",
						);
						None
					},
				}
			})
			.unzip();

		config_builder
			.with_websocket(WebSocketTransportConfig {
				listen_addresses: websocket.into_iter().flatten().map(Into::into).collect(),
				yamux_config: yamux_config.clone(),
				nodelay: true,
				..Default::default()
			})
			.with_tcp(TcpTransportConfig {
				listen_addresses: tcp.into_iter().flatten().map(Into::into).collect(),
				yamux_config,
				nodelay: true,
				..Default::default()
			})
	}
}

#[async_trait::async_trait]
impl<B: BlockT + 'static, H: ExHashT> NetworkBackend<B, H> for Litep2pNetworkBackend {
	type NotificationProtocolConfig = NotificationProtocolConfig;
	type RequestResponseProtocolConfig = RequestResponseConfig;
	type NetworkService<Block, Hash> = Arc<Litep2pNetworkService>;
	type PeerStore = Peerstore;
	type BitswapConfig = BitswapConfig;

	fn new(mut params: Params<B, H, Self>) -> Result<Self, Error>
	where
		Self: Sized,
	{
		let (keypair, local_peer_id) =
			Self::get_keypair(&params.network_config.network_config.node_key)?;
		let (cmd_tx, cmd_rx) = tracing_unbounded("mpsc_network_worker", 100_000);

		params.network_config.network_config.boot_nodes = params
			.network_config
			.network_config
			.boot_nodes
			.into_iter()
			.filter(|boot_node| boot_node.peer_id != local_peer_id.into())
			.collect();
		params.network_config.network_config.default_peers_set.reserved_nodes = params
			.network_config
			.network_config
			.default_peers_set
			.reserved_nodes
			.into_iter()
			.filter(|reserved_node| {
				if reserved_node.peer_id == local_peer_id.into() {
					log::warn!(
						target: LOG_TARGET,
						"Local peer ID used in reserved node, ignoring: {reserved_node}",
					);
					false
				} else {
					true
				}
			})
			.collect();

		if let Some(path) = &params.network_config.network_config.net_config_path {
			fs::create_dir_all(path)?;
		}

		log::info!(target: LOG_TARGET, "Local node identity is: {local_peer_id}");
		log::info!(target: LOG_TARGET, "Running litep2p network backend");

		params.network_config.sanity_check_addresses()?;
		params.network_config.sanity_check_bootnodes()?;

		let mut config_builder =
			Self::configure_transport(&params.network_config).with_keypair(keypair.clone());
		let known_addresses = params.network_config.known_addresses();
		let peer_store_handle = params.network_config.peer_store_handle();
		let executor = Arc::new(Litep2pExecutor { executor: params.executor });

		let FullNetworkConfiguration {
			notification_protocols,
			request_response_protocols,
			network_config,
			..
		} = params.network_config;

		// initialize notification protocols
		//
		// pass the protocol configuration to `Litep2pConfigBuilder` and save the TX channel
		// to the protocol's `Peerset` together with the protocol name to allow other subsystems
		// of Polkadot SDK to control connectivity of the notification protocol
		let block_announce_protocol = params.block_announce_config.protocol_name().clone();
		let mut notif_protocols = HashMap::from_iter([(
			params.block_announce_config.protocol_name().clone(),
			params.block_announce_config.handle,
		)]);

		// handshake for all but the syncing protocol is set to node role
		config_builder = notification_protocols
			.into_iter()
			.fold(config_builder, |config_builder, mut config| {
				config.config.set_handshake(Roles::from(&params.role).encode());
				notif_protocols.insert(config.protocol_name, config.handle);

				config_builder.with_notification_protocol(config.config)
			})
			.with_notification_protocol(params.block_announce_config.config);

		// initialize request-response protocols
		let metrics = match &params.metrics_registry {
			Some(registry) => Some(register_without_sources(registry)?),
			None => None,
		};

		// create channels that are used to send request before initializing protocols so the
		// senders can be passed onto all request-response protocols
		//
		// all protocols must have each others' senders so they can send the fallback request in
		// case the main protocol is not supported by the remote peer and user specified a fallback
		let (mut request_response_receivers, request_response_senders): (
			HashMap<_, _>,
			HashMap<_, _>,
		) = request_response_protocols
			.iter()
			.map(|config| {
				let (tx, rx) = tracing_unbounded("outbound-requests", 10_000);
				((config.protocol_name.clone(), rx), (config.protocol_name.clone(), tx))
			})
			.unzip();

		config_builder = request_response_protocols.into_iter().fold(
			config_builder,
			|config_builder, config| {
				let (protocol_config, handle) = RequestResponseConfigBuilder::new(
					Litep2pProtocolName::from(config.protocol_name.clone()),
				)
				.with_max_size(cmp::max(config.max_request_size, config.max_response_size) as usize)
				.with_fallback_names(config.fallback_names.into_iter().map(From::from).collect())
				.with_timeout(config.request_timeout)
				.build();

				let protocol = RequestResponseProtocol::new(
					config.protocol_name.clone(),
					handle,
					Arc::clone(&peer_store_handle),
					config.inbound_queue,
					request_response_receivers
						.remove(&config.protocol_name)
						.expect("receiver exists as it was just added and there are no duplicate protocols; qed"),
					request_response_senders.clone(),
					metrics.clone(),
				);

				executor.run(Box::pin(async move {
					protocol.run().await;
				}));

				config_builder.with_request_response_protocol(protocol_config)
			},
		);

		// collect known addresses
		let known_addresses: HashMap<litep2p::PeerId, Vec<Multiaddr>> =
			known_addresses.into_iter().fold(HashMap::new(), |mut acc, (peer, address)| {
				use sc_network_types::multiaddr::Protocol;

				let address = match address.iter().last() {
					Some(Protocol::Ws(_) | Protocol::Wss(_) | Protocol::Tcp(_)) =>
						address.with(Protocol::P2p(peer.into())),
					Some(Protocol::P2p(_)) => address,
					_ => return acc,
				};

				acc.entry(peer.into()).or_default().push(address.into());
				peer_store_handle.add_known_peer(peer);

				acc
			});

		// enable ipfs ping, identify and kademlia, and potentially mdns if user enabled it
		let listen_addresses = Arc::new(Default::default());
		let (discovery, ping_config, identify_config, kademlia_config, maybe_mdns_config) =
			Discovery::new(
				&network_config,
				params.genesis_hash,
				params.fork_id.as_deref(),
				&params.protocol_id,
				known_addresses.clone(),
				Arc::clone(&listen_addresses),
				Arc::clone(&peer_store_handle),
			);

		config_builder = config_builder
			.with_known_addresses(known_addresses.clone().into_iter())
			.with_libp2p_ping(ping_config)
			.with_libp2p_identify(identify_config)
			.with_libp2p_kademlia(kademlia_config)
			.with_connection_limits(ConnectionLimitsConfig::default().max_incoming_connections(
				Some(crate::MAX_CONNECTIONS_ESTABLISHED_INCOMING as usize),
			))
			.with_executor(executor);

		if let Some(config) = maybe_mdns_config {
			config_builder = config_builder.with_mdns(config);
		}

		if let Some(config) = params.bitswap_config {
			config_builder = config_builder.with_libp2p_bitswap(config);
		}

		let litep2p =
			Litep2p::new(config_builder.build()).map_err(|error| Error::Litep2p(error))?;

		litep2p.listen_addresses().for_each(|address| {
			log::debug!(target: LOG_TARGET, "listening on: {address}");

			listen_addresses.write().insert(address.clone());
		});

		let public_addresses = litep2p.public_addresses();
		for address in network_config.public_addresses.iter() {
			if let Err(err) = public_addresses.add_address(address.clone().into()) {
				log::warn!(
					target: LOG_TARGET,
					"failed to add public address {address:?}: {err:?}",
				);
			}
		}

		let network_service = Arc::new(Litep2pNetworkService::new(
			local_peer_id,
			keypair.clone(),
			cmd_tx,
			Arc::clone(&peer_store_handle),
			notif_protocols.clone(),
			block_announce_protocol.clone(),
			request_response_senders,
			Arc::clone(&listen_addresses),
			public_addresses,
		));

		// register rest of the metrics now that `Litep2p` has been created
		let num_connected = Arc::new(Default::default());
		let bandwidth: Arc<dyn BandwidthSink> =
			Arc::new(Litep2pBandwidthSink { sink: litep2p.bandwidth_sink() });

		if let Some(registry) = &params.metrics_registry {
			MetricSources::register(registry, bandwidth, Arc::clone(&num_connected))?;
		}

		Ok(Self {
			network_service,
			cmd_rx,
			metrics,
			peerset_handles: notif_protocols,
			num_connected,
			discovery,
			pending_put_values: HashMap::new(),
			pending_get_values: HashMap::new(),
			peerstore_handle: peer_store_handle,
			block_announce_protocol,
			event_streams: out_events::OutChannels::new(None)?,
			peers: HashMap::new(),
			litep2p,
		})
	}

	fn network_service(&self) -> Arc<dyn NetworkService> {
		Arc::clone(&self.network_service)
	}

	fn peer_store(
		bootnodes: Vec<sc_network_types::PeerId>,
		metrics_registry: Option<Registry>,
	) -> Self::PeerStore {
		Peerstore::new(bootnodes, metrics_registry)
	}

	fn register_notification_metrics(registry: Option<&Registry>) -> NotificationMetrics {
		NotificationMetrics::new(registry)
	}

	/// Create Bitswap server.
	fn bitswap_server(
		client: Arc<dyn BlockBackend<B> + Send + Sync>,
	) -> (Pin<Box<dyn Future<Output = ()> + Send>>, Self::BitswapConfig) {
		BitswapServer::new(client)
	}

	/// Create notification protocol configuration for `protocol`.
	fn notification_config(
		protocol_name: ProtocolName,
		fallback_names: Vec<ProtocolName>,
		max_notification_size: u64,
		handshake: Option<NotificationHandshake>,
		set_config: SetConfig,
		metrics: NotificationMetrics,
		peerstore_handle: Arc<dyn PeerStoreProvider>,
	) -> (Self::NotificationProtocolConfig, Box<dyn NotificationService>) {
		Self::NotificationProtocolConfig::new(
			protocol_name,
			fallback_names,
			max_notification_size as usize,
			handshake,
			set_config,
			metrics,
			peerstore_handle,
		)
	}

	/// Create request-response protocol configuration.
	fn request_response_config(
		protocol_name: ProtocolName,
		fallback_names: Vec<ProtocolName>,
		max_request_size: u64,
		max_response_size: u64,
		request_timeout: Duration,
		inbound_queue: Option<async_channel::Sender<IncomingRequest>>,
	) -> Self::RequestResponseProtocolConfig {
		Self::RequestResponseProtocolConfig::new(
			protocol_name,
			fallback_names,
			max_request_size,
			max_response_size,
			request_timeout,
			inbound_queue,
		)
	}

	/// Start [`Litep2pNetworkBackend`] event loop.
	async fn run(mut self) {
		log::debug!(target: LOG_TARGET, "starting litep2p network backend");

		loop {
			let num_connected_peers = self
				.peerset_handles
				.get(&self.block_announce_protocol)
				.map_or(0usize, |handle| handle.connected_peers.load(Ordering::Relaxed));
			self.num_connected.store(num_connected_peers, Ordering::Relaxed);

			tokio::select! {
				command = self.cmd_rx.next() => match command {
					None => return,
					Some(command) => match command {
						NetworkServiceCommand::GetValue{ key } => {
							let query_id = self.discovery.get_value(key.clone()).await;
							self.pending_get_values.insert(query_id, (key, Instant::now()));
						}
						NetworkServiceCommand::PutValue { key, value } => {
							let query_id = self.discovery.put_value(key.clone(), value).await;
							self.pending_put_values.insert(query_id, (key, Instant::now()));
						}
						NetworkServiceCommand::PutValueTo { record, peers, update_local_storage} => {
							let kademlia_key = record.key.to_vec().into();
							let query_id = self.discovery.put_value_to_peers(record, peers, update_local_storage).await;
							self.pending_put_values.insert(query_id, (kademlia_key, Instant::now()));
						}

						NetworkServiceCommand::StoreRecord { key, value, publisher, expires } => {
							self.discovery.store_record(key, value, publisher.map(Into::into), expires).await;
						}
						NetworkServiceCommand::EventStream { tx } => {
							self.event_streams.push(tx);
						}
						NetworkServiceCommand::Status { tx } => {
							let _ = tx.send(NetworkStatus {
								num_connected_peers: self
									.peerset_handles
									.get(&self.block_announce_protocol)
									.map_or(0usize, |handle| handle.connected_peers.load(Ordering::Relaxed)),
								total_bytes_inbound: self.litep2p.bandwidth_sink().inbound() as u64,
								total_bytes_outbound: self.litep2p.bandwidth_sink().outbound() as u64,
							});
						}
						NetworkServiceCommand::AddPeersToReservedSet {
							protocol,
							peers,
						} => {
							let peers = self.add_addresses(peers.into_iter().map(Into::into));

							match self.peerset_handles.get(&protocol) {
								Some(handle) => {
									let _ = handle.tx.unbounded_send(PeersetCommand::AddReservedPeers { peers });
								}
								None => log::warn!(target: LOG_TARGET, "protocol {protocol} doens't exist"),
							};
						}
						NetworkServiceCommand::AddKnownAddress { peer, address } => {
							let mut address: Multiaddr = address.into();

							if !address.iter().any(|protocol| std::matches!(protocol, Protocol::P2p(_))) {
								address.push(Protocol::P2p(litep2p::PeerId::from(peer).into()));
							}

							if self.litep2p.add_known_address(peer.into(), iter::once(address.clone())) == 0usize {
								log::warn!(
									target: LOG_TARGET,
									"couldn't add known address ({address}) for {peer:?}, unsupported transport"
								);
							}
						},
						NetworkServiceCommand::SetReservedPeers { protocol, peers } => {
							let peers = self.add_addresses(peers.into_iter().map(Into::into));

							match self.peerset_handles.get(&protocol) {
								Some(handle) => {
									let _ = handle.tx.unbounded_send(PeersetCommand::SetReservedPeers { peers });
								}
								None => log::warn!(target: LOG_TARGET, "protocol {protocol} doens't exist"),
							}

						},
						NetworkServiceCommand::DisconnectPeer {
							protocol,
							peer,
						} => {
							let Some(handle) = self.peerset_handles.get(&protocol) else {
								log::warn!(target: LOG_TARGET, "protocol {protocol} doens't exist");
								continue
							};

							let _ = handle.tx.unbounded_send(PeersetCommand::DisconnectPeer { peer });
						}
						NetworkServiceCommand::SetReservedOnly {
							protocol,
							reserved_only,
						} => {
							let Some(handle) = self.peerset_handles.get(&protocol) else {
								log::warn!(target: LOG_TARGET, "protocol {protocol} doens't exist");
								continue
							};

							let _ = handle.tx.unbounded_send(PeersetCommand::SetReservedOnly { reserved_only });
						}
						NetworkServiceCommand::RemoveReservedPeers {
							protocol,
							peers,
						} => {
							let Some(handle) = self.peerset_handles.get(&protocol) else {
								log::warn!(target: LOG_TARGET, "protocol {protocol} doens't exist");
								continue
							};

							let _ = handle.tx.unbounded_send(PeersetCommand::RemoveReservedPeers { peers });
						}
					}
				},
				event = self.discovery.next() => match event {
					None => return,
					Some(DiscoveryEvent::Discovered { addresses }) => {
						// if at least one address was added for the peer, report the peer to `Peerstore`
						for (peer, addresses) in Litep2pNetworkBackend::parse_addresses(addresses.into_iter()) {
							if self.litep2p.add_known_address(peer.into(), addresses.clone().into_iter()) > 0 {
								self.peerstore_handle.add_known_peer(peer);
							}
						}
					}
					Some(DiscoveryEvent::RoutingTableUpdate { peers }) => {
						for peer in peers {
							self.peerstore_handle.add_known_peer(peer.into());
						}
					}
					Some(DiscoveryEvent::GetRecordSuccess { query_id, records }) => {
						match self.pending_get_values.remove(&query_id) {
							None => log::warn!(
								target: LOG_TARGET,
								"`GET_VALUE` succeeded for a non-existent query",
							),
							Some((key, started)) => {
								log::trace!(
									target: LOG_TARGET,
									"`GET_VALUE` for {:?} ({query_id:?}) succeeded",
									key,
								);
								for record in litep2p_to_libp2p_peer_record(records) {
									self.event_streams.send(
										Event::Dht(
											DhtEvent::ValueFound(
												record
											)
										)
									);
								}

								if let Some(ref metrics) = self.metrics {
									metrics
										.kademlia_query_duration
										.with_label_values(&["value-get"])
										.observe(started.elapsed().as_secs_f64());
								}
							}
						}
					}
					Some(DiscoveryEvent::PutRecordSuccess { query_id }) => {
						match self.pending_put_values.remove(&query_id) {
							None => log::warn!(
								target: LOG_TARGET,
								"`PUT_VALUE` succeeded for a non-existent query",
							),
							Some((key, started)) => {
								log::trace!(
									target: LOG_TARGET,
									"`PUT_VALUE` for {key:?} ({query_id:?}) succeeded",
								);

								self.event_streams.send(Event::Dht(
									DhtEvent::ValuePut(libp2p::kad::RecordKey::new(&key))
								));

								if let Some(ref metrics) = self.metrics {
									metrics
										.kademlia_query_duration
										.with_label_values(&["value-put"])
										.observe(started.elapsed().as_secs_f64());
								}
							}
						}
					}
					Some(DiscoveryEvent::QueryFailed { query_id }) => {
						match self.pending_get_values.remove(&query_id) {
							None => match self.pending_put_values.remove(&query_id) {
								None => log::warn!(
									target: LOG_TARGET,
									"non-existent query failed ({query_id:?})",
								),
								Some((key, started)) => {
									log::debug!(
										target: LOG_TARGET,
										"`PUT_VALUE` ({query_id:?}) failed for key {key:?}",
									);

									self.event_streams.send(Event::Dht(
										DhtEvent::ValuePutFailed(libp2p::kad::RecordKey::new(&key))
									));

									if let Some(ref metrics) = self.metrics {
										metrics
											.kademlia_query_duration
											.with_label_values(&["value-put-failed"])
											.observe(started.elapsed().as_secs_f64());
									}
								}
							}
							Some((key, started)) => {
								log::debug!(
									target: LOG_TARGET,
									"`GET_VALUE` ({query_id:?}) failed for key {key:?}",
								);

								self.event_streams.send(Event::Dht(
									DhtEvent::ValueNotFound(libp2p::kad::RecordKey::new(&key))
								));

								if let Some(ref metrics) = self.metrics {
									metrics
										.kademlia_query_duration
										.with_label_values(&["value-get-failed"])
										.observe(started.elapsed().as_secs_f64());
								}
							}
						}
					}
					Some(DiscoveryEvent::Identified { peer, listen_addresses, supported_protocols, .. }) => {
						self.discovery.add_self_reported_address(peer, supported_protocols, listen_addresses).await;
					}
					Some(DiscoveryEvent::ExternalAddressDiscovered { address }) => {
						match self.litep2p.public_addresses().add_address(address.clone().into()) {
							Ok(inserted) => if inserted {
								log::info!(target: LOG_TARGET, "🔍 Discovered new external address for our node: {address}");
							},
							Err(err) => {
								log::warn!(
									target: LOG_TARGET,
									"🔍 Failed to add discovered external address {address:?}: {err:?}",
								);
							},
						}
					}
					Some(DiscoveryEvent::ExternalAddressExpired{ address }) => {
						let local_peer_id = self.litep2p.local_peer_id();

						// Litep2p requires the peer ID to be present in the address.
						let address = if !std::matches!(address.iter().last(), Some(Protocol::P2p(_))) {
							address.with(Protocol::P2p(*local_peer_id.as_ref()))
						} else {
							address
						};

						if self.litep2p.public_addresses().remove_address(&address) {
							log::info!(target: LOG_TARGET, "🔍 Expired external address for our node: {address}");
						} else {
							log::warn!(
								target: LOG_TARGET,
								"🔍 Failed to remove expired external address {address:?}"
							);
						}
					}
					Some(DiscoveryEvent::Ping { peer, rtt }) => {
						log::trace!(
							target: LOG_TARGET,
							"ping time with {peer:?}: {rtt:?}",
						);
					}
					Some(DiscoveryEvent::IncomingRecord { record: Record { key, value, publisher, expires }} ) => {
						self.event_streams.send(Event::Dht(
							DhtEvent::PutRecordRequest(
								libp2p::kad::RecordKey::new(&key),
								value,
								publisher.map(Into::into),
								expires,
							)
						));
					},

					Some(DiscoveryEvent::RandomKademliaStarted) => {
						if let Some(metrics) = self.metrics.as_ref() {
							metrics.kademlia_random_queries_total.inc();
						}
					}
				},
				event = self.litep2p.next_event() => match event {
					Some(Litep2pEvent::ConnectionEstablished { peer, endpoint }) => {
						let Some(metrics) = &self.metrics else {
							continue;
						};

						let direction = match endpoint {
							Endpoint::Dialer { .. } => "out",
							Endpoint::Listener { .. } => "in",
						};
						metrics.connections_opened_total.with_label_values(&[direction]).inc();

						match self.peers.entry(peer) {
							Entry::Vacant(entry) => {
								entry.insert(ConnectionContext {
									endpoints: HashMap::from_iter([(endpoint.connection_id(), endpoint)]),
									num_connections: 1usize,
								});
								metrics.distinct_peers_connections_opened_total.inc();
							}
							Entry::Occupied(entry) => {
								let entry = entry.into_mut();
								entry.num_connections += 1;
								entry.endpoints.insert(endpoint.connection_id(), endpoint);
							}
						}
					}
					Some(Litep2pEvent::ConnectionClosed { peer, connection_id }) => {
						let Some(metrics) = &self.metrics else {
							continue;
						};

						let Some(context) = self.peers.get_mut(&peer) else {
							log::debug!(target: LOG_TARGET, "unknown peer disconnected: {peer:?} ({connection_id:?})");
							continue
						};

						let direction = match context.endpoints.remove(&connection_id) {
							None => {
								log::debug!(target: LOG_TARGET, "connection {connection_id:?} doesn't exist for {peer:?} ");
								continue
							}
							Some(endpoint) => {
								context.num_connections -= 1;

								match endpoint {
									Endpoint::Dialer { .. } => "out",
									Endpoint::Listener { .. } => "in",
								}
							}
						};

						metrics.connections_closed_total.with_label_values(&[direction, "actively-closed"]).inc();

						if context.num_connections == 0 {
							self.peers.remove(&peer);
							metrics.distinct_peers_connections_closed_total.inc();
						}
					}
					Some(Litep2pEvent::DialFailure { address, error }) => {
						log::debug!(
							target: LOG_TARGET,
							"failed to dial peer at {address:?}: {error:?}",
						);

						if let Some(metrics) = &self.metrics {
							let reason = match error {
								DialError::Timeout => "timeout",
								DialError::AddressError(_) => "invalid-address",
								DialError::DnsError(_) => "cannot-resolve-dns",
								DialError::NegotiationError(error) => match error {
									NegotiationError::Timeout => "timeout",
									NegotiationError::PeerIdMissing => "missing-peer-id",
									NegotiationError::StateMismatch => "state-mismatch",
									NegotiationError::PeerIdMismatch(_,_) => "peer-id-missmatch",
									NegotiationError::MultistreamSelectError(_) => "multistream-select-error",
									NegotiationError::SnowError(_) => "noise-error",
									NegotiationError::ParseError(_) => "parse-error",
									NegotiationError::IoError(_) => "io-error",
									NegotiationError::WebSocket(_) => "webscoket-error",
								}
							};

							metrics.pending_connections_errors_total.with_label_values(&[&reason]).inc();
						}
					}
					Some(Litep2pEvent::ListDialFailures { errors }) => {
						log::debug!(
							target: LOG_TARGET,
							"failed to dial peer on multiple addresses {errors:?}",
						);

						if let Some(metrics) = &self.metrics {
							metrics.pending_connections_errors_total.with_label_values(&["transport-errors"]).inc();
						}
					}
					_ => {}
				},
			}
		}
	}
}

// Glue code to convert from a litep2p records type to a libp2p2 PeerRecord.
fn litep2p_to_libp2p_peer_record(records: RecordsType) -> Vec<PeerRecord> {
	match records {
		litep2p::protocol::libp2p::kademlia::RecordsType::LocalStore(record) => {
			vec![PeerRecord {
				record: P2PRecord {
					key: record.key.to_vec().into(),
					value: record.value,
					publisher: record.publisher.map(|peer_id| {
						let peer_id: sc_network_types::PeerId = peer_id.into();
						peer_id.into()
					}),
					expires: record.expires,
				},
				peer: None,
			}]
		},
		litep2p::protocol::libp2p::kademlia::RecordsType::Network(records) => records
			.into_iter()
			.map(|record| {
				let peer_id: sc_network_types::PeerId = record.peer.into();

				PeerRecord {
					record: P2PRecord {
						key: record.record.key.to_vec().into(),
						value: record.record.value,
						publisher: record.record.publisher.map(|peer_id| {
							let peer_id: sc_network_types::PeerId = peer_id.into();
							peer_id.into()
						}),
						expires: record.record.expires,
					},
					peer: Some(peer_id.into()),
				}
			})
			.collect::<Vec<_>>(),
	}
}
