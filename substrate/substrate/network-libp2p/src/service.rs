// Copyright 2018 Parity Technologies (UK) Ltd.
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

use bytes::Bytes;
use {Error, ErrorKind, NetworkConfiguration, NetworkProtocolHandler};
use {NonReservedPeerMode, NetworkContext, Severity, NodeIndex, ProtocolId};
use parking_lot::RwLock;
use libp2p;
use libp2p::multiaddr::{AddrComponent, Multiaddr};
use libp2p::kad::{KadSystem, KadConnecConfig, KadSystemConfig};
use libp2p::kad::{KadIncomingRequest, KadConnecController, KadPeer};
use libp2p::kad::{KadConnectionType, KadQueryEvent};
use libp2p::identify::{IdentifyInfo, IdentifyOutput, IdentifySender};
use libp2p::identify::{IdentifyProtocolConfig};
use libp2p::core::{upgrade, Transport, MuxedTransport, ConnectionUpgrade};
use libp2p::core::{Endpoint, PeerId as PeerstorePeerId, PublicKey};
use libp2p::core::{SwarmController, UniqueConnecState};
use libp2p::ping;
use libp2p::transport_timeout::TransportTimeout;
use {PacketId, SessionInfo, TimerToken};
use rand;
use std::io::{Error as IoError, ErrorKind as IoErrorKind};
use std::iter;
use std::net::SocketAddr;
use std::sync::Arc;
use std::sync::mpsc as sync_mpsc;
use std::thread;
use std::time::{Duration, Instant};
use futures::{future, Future, stream, Stream, select_all};
use futures::sync::{mpsc, oneshot};
use tokio::runtime::current_thread;
use tokio_io::{AsyncRead, AsyncWrite};
use tokio_timer::{Interval, Timeout};

use custom_proto::{RegisteredProtocol, RegisteredProtocols};
use custom_proto::RegisteredProtocolOutput;
use network_state::{NetworkState, PeriodicUpdate};
use timeouts;
use transport;

/// IO Service with networking.
pub struct NetworkService {
	shared: Arc<Shared>,

	/// Holds the networking-running background thread alive. The `Option` is
	/// only set to `None` in the destructor.
	/// Sending a message on the channel will trigger the end of the
	/// background thread. We can then wait on the join handle.
	bg_thread: Option<(oneshot::Sender<()>, thread::JoinHandle<()>)>,
}

/// Common struct shared throughout all the components of the service.
struct Shared {
	/// Original configuration of the service.
	config: NetworkConfiguration,

	/// Contains the state of the network.
	network_state: NetworkState,

	/// Kademlia system. Contains the DHT.
	kad_system: KadSystem,

	/// Configuration for the Kademlia upgrade.
	kad_upgrade: KadConnecConfig,

	/// List of protocols available on the network. It is a logic error to
	/// remove protocols from this list, and the code may assume that protocols
	/// stay at the same index forever.
	protocols: RegisteredProtocols<Arc<NetworkProtocolHandler + Send + Sync>>,

	/// Use this channel to send a timeout request to the background thread's
	/// events loop. After the timeout, elapsed, it will call `timeout` on the
	/// `NetworkProtocolHandler`. This can be closed if the background thread
	/// is not running. The sender will be overwritten every time we start
	/// the service.
	timeouts_register_tx: mpsc::UnboundedSender<(Duration, (Arc<NetworkProtocolHandler + Send + Sync>, ProtocolId, TimerToken))>,

	/// Original address from the configuration, after being adjusted by the `Transport`.
	// TODO: because we create the `Shared` before starting to listen, this
	// has to be set later ; sort this out
	original_listened_addr: RwLock<Vec<Multiaddr>>,

	/// Contains the addresses we known about ourselves.
	listened_addrs: RwLock<Vec<Multiaddr>>,
}

impl NetworkService {
	/// Starts the networking service.
	///
	/// Note that we could use an iterator for `protocols`, but having a
	/// generic here is too much and crashes the Rust compiler.
	pub fn new(
		config: NetworkConfiguration,
		protocols: Vec<(Arc<NetworkProtocolHandler + Send + Sync>, ProtocolId, &[(u8, u8)])>
	) -> Result<NetworkService, Error> {
		let network_state = NetworkState::new(&config)?;

		let local_peer_id = network_state.local_public_key().clone()
			.into_peer_id();
		for mut addr in config.listen_addresses.iter().cloned() {
			addr.append(AddrComponent::P2P(local_peer_id.clone().into()));
			info!(target: "sub-libp2p", "Local node address is: {}", addr);
		}

		let kad_system = KadSystem::without_init(KadSystemConfig {
			parallelism: 3,
			local_peer_id: local_peer_id.clone(),
			kbuckets_timeout: Duration::from_secs(600),
			request_timeout: Duration::from_secs(10),
			known_initial_peers: iter::empty(),
		});

		// Channel we use to signal success or failure of the bg thread
		// initialization process.
		let (init_tx, init_rx) = sync_mpsc::channel();
		// Channel the main thread uses to signal the bg thread that it
		// should stop
		let (close_tx, close_rx) = oneshot::channel();
		let (timeouts_register_tx, timeouts_register_rx) = mpsc::unbounded();

		let listened_addrs = config.public_addresses.clone();

		let shared = Arc::new(Shared {
			network_state,
			protocols: RegisteredProtocols(protocols.into_iter()
				.map(|(handler, protocol, versions)|
					RegisteredProtocol::new(handler.clone(), protocol, versions))
				.collect()
			),
			kad_system,
			kad_upgrade: KadConnecConfig::new(),
			config,
			timeouts_register_tx,
			original_listened_addr: RwLock::new(Vec::new()),
			listened_addrs: RwLock::new(listened_addrs),
		});

		// Initialize all the protocols now.
		// TODO: what about failure to initialize? we can't uninitialize a protocol
		// TODO: remove this `initialize` method eventually, as it's only used for timers
		for protocol in shared.protocols.0.iter() {
			protocol.custom_data().initialize(&NetworkContextImpl {
				inner: shared.clone(),
				protocol: protocol.id().clone(),
				current_peer: None,
			});
		}

		let shared_clone = shared.clone();
		let join_handle = thread::spawn(move || {
			// Tokio runtime that is going to run everything in this thread.
			let mut runtime = match current_thread::Runtime::new() {
				Ok(c) => c,
				Err(err) => {
					let _ = init_tx.send(Err(err.into()));
					return
				}
			};

			let fut = match init_thread(shared_clone, timeouts_register_rx, close_rx) {
				Ok(future) => {
					debug!(target: "sub-libp2p", "Successfully started networking service");
					let _ = init_tx.send(Ok(()));
					future
				},
				Err(err) => {
					let _ = init_tx.send(Err(err));
					return
				}
			};

			match runtime.block_on(fut) {
				Ok(()) => debug!(target: "sub-libp2p", "libp2p future finished"),
				Err(err) => error!(target: "sub-libp2p", "error while running libp2p: {:?}", err),
			}
		});

		init_rx.recv().expect("libp2p background thread panicked")?;

		Ok(NetworkService {
			shared,
			bg_thread: Some((close_tx, join_handle)),
		})
	}

	/// Returns network configuration.
	pub fn config(&self) -> &NetworkConfiguration {
		&self.shared.config
	}

	pub fn external_url(&self) -> Option<String> {
		// TODO: in the context of libp2p, it is hard to define what an external
		// URL is, as different nodes can have multiple different ways to
		// reach us
		self.shared.original_listened_addr.read().get(0)
			.map(|addr|
				format!("{}/p2p/{}", addr, self.shared.kad_system.local_peer_id().to_base58())
			)
	}

	/// Get a list of all connected peers by id.
	pub fn connected_peers(&self) -> Vec<NodeIndex> {
		self.shared.network_state.connected_peers()
	}

	/// Try to add a reserved peer.
	pub fn add_reserved_peer(&self, peer: &str) -> Result<(), Error> {
		// TODO: try to dial the peer?
		self.shared.network_state.add_reserved_peer(peer)
	}

	/// Try to remove a reserved peer.
	pub fn remove_reserved_peer(&self, peer: &str) -> Result<(), Error> {
		self.shared.network_state.remove_reserved_peer(peer)
	}

	/// Set the non-reserved peer mode.
	pub fn set_non_reserved_mode(&self, mode: NonReservedPeerMode) {
		self.shared.network_state.set_non_reserved_mode(mode)
	}

	/// Executes action in the network context
	pub fn with_context<F>(&self, protocol: ProtocolId, action: F)
		where F: FnOnce(&NetworkContext) {
		self.with_context_eval(protocol, action);
	}

	/// Evaluates function in the network context
	pub fn with_context_eval<F, T>(&self, protocol: ProtocolId, action: F)
		-> Option<T>
		where F: FnOnce(&NetworkContext) -> T {
		if !self.shared.protocols.has_protocol(protocol) {
			return None
		}

		Some(action(&NetworkContextImpl {
			inner: self.shared.clone(),
			protocol: protocol.clone(),
			current_peer: None,
		}))
	}
}

impl Drop for NetworkService {
	fn drop(&mut self) {
		if let Some((close_tx, join)) = self.bg_thread.take() {
			let _ = close_tx.send(());
			if let Err(e) = join.join() {
				warn!(target: "sub-libp2p", "error while waiting on libp2p background thread: {:?}", e);
			}
		}

		debug_assert!(!self.shared.network_state.has_connected_peer());
	}
}

#[derive(Clone)]
struct NetworkContextImpl {
	inner: Arc<Shared>,
	protocol: ProtocolId,
	current_peer: Option<NodeIndex>,
}

impl NetworkContext for NetworkContextImpl {
	fn send(&self, peer: NodeIndex, packet_id: PacketId, data: Vec<u8>) {
		self.send_protocol(self.protocol, peer, packet_id, data)
	}

	fn send_protocol(
		&self,
		protocol: ProtocolId,
		peer: NodeIndex,
		packet_id: PacketId,
		data: Vec<u8>
	) {
		debug_assert!(self.inner.protocols.has_protocol(protocol),
			"invalid protocol id requested in the API of the libp2p networking");
		// TODO: could be "optimized" by building `message` only after checking the validity of
		// 		the peer, but that's probably not worth the effort
		let mut message = Bytes::with_capacity(1 + data.len());
		message.extend_from_slice(&[packet_id]);
		message.extend_from_slice(&data);
		if self.inner.network_state.send(peer, protocol, message).is_err() {
			debug!(target: "sub-libp2p", "Sending to peer {} failed. Dropping.", peer);
			self.inner.network_state.drop_peer(peer);
		}
	}

	fn respond(&self, packet_id: PacketId, data: Vec<u8>) {
		if let Some(peer) = self.current_peer {
			self.send_protocol(self.protocol, peer, packet_id, data)
		} else {
			panic!("respond() called outside of a received message");
		}
	}

	fn report_peer(&self, peer: NodeIndex, reason: Severity) {
		if let Some(info) = self.inner.network_state.peer_info(peer) {
			if let Some(client_version) = info.client_version {
				info!(target: "sub-libp2p",
					"Peer {} ({:?} {}) reported by client: {}",
					peer,
					info.remote_address,
					client_version,
					reason
				);
			} else {
				info!(target: "sub-libp2p", "Peer {} reported by client: {}", peer, reason);
			}
		}
		match reason {
			Severity::Bad(reason) => self.inner.network_state.ban_peer(peer, reason),
			Severity::Useless(_) => self.inner.network_state.drop_peer(peer),
			Severity::Timeout => self.inner.network_state.drop_peer(peer),
		}
	}

	fn is_expired(&self) -> bool {
		if let Some(current_peer) = self.current_peer {
			!self.inner.network_state.is_peer_connected(current_peer)
		} else {
			// TODO: is this correct?
			true
		}
	}

	fn register_timer(&self, token: usize, duration: Duration)
		-> Result<(), Error> {
		let handler = self.inner.protocols
			.find_protocol(self.protocol)
			.ok_or(ErrorKind::BadProtocol)?
			.custom_data()
			.clone();
		self.inner.timeouts_register_tx
			.unbounded_send((duration, (handler, self.protocol, token)))
			.map_err(|err| ErrorKind::Io(IoError::new(IoErrorKind::Other, err)))?;
		Ok(())
	}

	fn peer_client_version(&self, peer: NodeIndex) -> String {
		// Devp2p returns "unknown" on unknown peer ID, so we do the same.
		self.inner.network_state.peer_client_version(peer, self.protocol)
			.unwrap_or_else(|| "unknown".to_string())
	}

	fn session_info(&self, peer: NodeIndex) -> Option<SessionInfo> {
		self.inner.network_state.session_info(peer, self.protocol)
	}

	fn protocol_version(&self, protocol: ProtocolId, peer: NodeIndex) -> Option<u8> {
		self.inner.network_state.protocol_version(peer, protocol)
	}

	fn subprotocol_name(&self) -> ProtocolId {
		self.protocol.clone()
	}
}

/// Builds the main `Future` for the network service.
///
/// - `timeouts_register_rx` should receive newly-registered timeouts.
/// - `close_rx` should be triggered when we want to close the network.
fn init_thread(
	shared: Arc<Shared>,
	timeouts_register_rx: mpsc::UnboundedReceiver<
		(Duration, (Arc<NetworkProtocolHandler + Send + Sync + 'static>, ProtocolId, TimerToken))
	>,
	close_rx: oneshot::Receiver<()>
) -> Result<impl Future<Item = (), Error = IoError>, Error> {
	// Build the transport layer.
	let transport = {
		let base = transport::build_transport(
			shared.network_state.local_private_key().clone()
		);

		let base = base.map_err_dial({
			let shared = shared.clone();
			move |err, addr| {
				trace!(target: "sub-libp2p", "Failed to dial {}: {:?}", addr, err);
				shared.network_state.report_failed_to_connect(&addr);
				err
			}
		});

		let shared = shared.clone();
		Transport::and_then(base, move |(peer_id, stream), endpoint, remote_addr| {
			remote_addr.and_then(move |remote_addr| {
				if &peer_id == shared.kad_system.local_peer_id() {
					// TODO: this happens very frequently for now and floods the logs
					//warn!(target: "sub-libp2p", "Refusing connection from our local peer id");
					return Err(IoErrorKind::ConnectionRefused.into())
				}

				// TODO: there's a possible race condition here if `cleanup_and_prepare_updates` is
				// called between `assign_node_index` and one of `kad_connec`, `unique_connec`,
				// etc. ; in practice though, it is very unlikely to happen
				let node_index = shared.network_state.assign_node_index(&peer_id)?;
				shared.network_state.report_connected(node_index, &remote_addr, endpoint)?;
				let out = TransportOutput {
					socket: stream,
					node_index,
					original_addr: remote_addr.clone(),
				};
				Ok((out, future::ok(remote_addr)))
			})
		})
	};

	// Build the swarm. The swarm is the single entry point where successfully
	// negotiated protocols arrive.
	let (swarm_controller, swarm_events) = {
		let upgraded_transport = transport.clone()
			.and_then({
				let shared = shared.clone();
				move |out, endpoint, client_addr| {
					let node_index = out.node_index;
					let original_addr = out.original_addr;
					let listener_upgrade = upgrade::or(upgrade::or(upgrade::or(
						upgrade::map(shared.kad_upgrade.clone(), move |(c, f)| FinalUpgrade::Kad(node_index, c, f)),
						upgrade::map(IdentifyProtocolConfig, move |id| FinalUpgrade::from((node_index, id, original_addr)))),
						upgrade::map(ping::Ping, move |out| FinalUpgrade::from((node_index, out)))),
						upgrade::map(DelayedProtosList(shared), move |c| FinalUpgrade::Custom(node_index, c)));
					upgrade::apply(out.socket, listener_upgrade, endpoint, client_addr)
				}
			});
		let shared = shared.clone();

		libp2p::core::swarm(
			upgraded_transport,
			move |upgrade, _client_addr|
				listener_handle(shared.clone(), upgrade)
		)
	};

	// Listen on multiaddresses.
	for addr in &shared.config.listen_addresses {
		match swarm_controller.listen_on(addr.clone()) {
			Ok(new_addr) => {
				debug!(target: "sub-libp2p", "Libp2p listening on {}", new_addr);
				shared.original_listened_addr.write().push(new_addr.clone());
			},
			Err(_) => {
				warn!(target: "sub-libp2p", "Can't listen on {}, protocol not supported", addr);
				return Err(ErrorKind::BadProtocol.into())
			},
		}
	}

	// Explicitely connect to _all_ the boostrap nodes as a temporary measure.
	for bootnode in shared.config.boot_nodes.iter() {
		match shared.network_state.add_bootstrap_peer(bootnode) {
			Ok((who, addr)) => {
				trace!(target: "sub-libp2p", "Dialing bootnode {:?} through {}", who, addr);
				for proto in shared.protocols.0.clone().into_iter() {
					open_peer_custom_proto(
						shared.clone(),
						transport.clone(),
						addr.clone(),
						Some(who.clone()),
						proto,
						&swarm_controller
					)
				}
			},
			Err(Error(ErrorKind::AddressParse, _)) => {
				// fallback: trying with IP:Port
				let multi = match bootnode.parse::<SocketAddr>() { 
					Ok(SocketAddr::V4(socket)) =>
						format!("/ip4/{}/tcp/{}", socket.ip(), socket.port()).parse::<Multiaddr>(),
					Ok(SocketAddr::V6(socket)) =>
						format!("/ip6/{}/tcp/{}", socket.ip(), socket.port()).parse::<Multiaddr>(),
					_ => {
						warn!(target: "sub-libp2p", "Not a valid Bootnode Address {:}", bootnode);
						continue;
					}
				};

				if let Ok(addr) = multi {
					trace!(target: "sub-libp2p", "Missing NodeIndex for Bootnode {:}. Querying", bootnode);
					for proto in shared.protocols.0.clone().into_iter() {
						open_peer_custom_proto(
							shared.clone(),
							transport.clone(),
							addr.clone(),
							None,
							proto,
							&swarm_controller
						)
					}
				} else {
					warn!(target: "sub-libp2p", "Not a valid Bootnode Address {:}", bootnode);
					continue;
				}
			},
			Err(err) => warn!(target:"sub-libp2p", "Couldn't parse Bootnode Address: {}", err),
		}
	}

	let outgoing_connections = Interval::new(Instant::now(), Duration::from_secs(5))
		.map_err(|err| IoError::new(IoErrorKind::Other, err))
		.for_each({
			let shared = shared.clone();
			let transport = transport.clone();
			let swarm_controller = swarm_controller.clone();
			move |_| {
				connect_to_nodes(shared.clone(), transport.clone(), &swarm_controller);
				Ok(())
			}
		});

	// Build the timeouts system for the `register_timeout` function.
	// (note: this has nothing to do with socket timeouts)
	let timeouts = timeouts::build_timeouts_stream(timeouts_register_rx)
		.for_each({
			let shared = shared.clone();
			move |(handler, protocol_id, timer_token)| {
				handler.timeout(&NetworkContextImpl {
					inner: shared.clone(),
					protocol: protocol_id,
					current_peer: None,
				}, timer_token);
				Ok(())
			}
		})
		.then(|val| {
			warn!(target: "sub-libp2p", "Timeouts stream closed unexpectedly: {:?}", val);
			val
		});

	// Start the process of periodically discovering nodes to connect to.
	let discovery = start_kademlia_discovery(shared.clone(),
		transport.clone(), swarm_controller.clone());

	// Start the process of pinging the active nodes on the network.
	let periodic = start_periodic_updates(shared.clone(), transport, swarm_controller);

	let futures: Vec<Box<Future<Item = (), Error = IoError>>> = vec![
		Box::new(swarm_events.for_each(|_| Ok(()))),
		Box::new(discovery),
		Box::new(periodic),
		Box::new(outgoing_connections),
		Box::new(timeouts),
		Box::new(close_rx.map_err(|err| IoError::new(IoErrorKind::Other, err))),
	];

	Ok(
		select_all(futures)
		.and_then(move |_| {
			debug!(target: "sub-libp2p", "Networking ended ; disconnecting all peers");
			shared.network_state.disconnect_all();
			Ok(())
		})
		.map_err(|(r, _, _)| r)
	)
}

/// Output of the common transport layer.
struct TransportOutput<S> {
	socket: S,
	node_index: NodeIndex,
	original_addr: Multiaddr,
}

/// Enum of all the possible protocols our service handles.
enum FinalUpgrade<C> {
	Kad(NodeIndex, KadConnecController, Box<Stream<Item = KadIncomingRequest, Error = IoError> + Send>),
	/// The remote identification system, and the multiaddress we see the remote as.
	IdentifyListener(NodeIndex, IdentifySender<C>, Multiaddr),
	/// The remote information about the address they see us as.
	IdentifyDialer(NodeIndex, IdentifyInfo, Multiaddr),
	PingDialer(NodeIndex, ping::Pinger, Box<Future<Item = (), Error = IoError> + Send>),
	PingListener(NodeIndex, Box<Future<Item = (), Error = IoError> + Send>),
	/// `Custom` means anything not in the core libp2p and is handled
	/// by `CustomProtoConnectionUpgrade`.
	Custom(NodeIndex, RegisteredProtocolOutput<Arc<NetworkProtocolHandler + Send + Sync>>),
}

impl<C> From<(NodeIndex, ping::PingOutput)> for FinalUpgrade<C> {
	fn from((node_index, out): (NodeIndex, ping::PingOutput)) -> FinalUpgrade<C> {
		match out {
			ping::PingOutput::Ponger(processing) =>
				FinalUpgrade::PingListener(node_index, processing),
			ping::PingOutput::Pinger { pinger, processing } =>
				FinalUpgrade::PingDialer(node_index, pinger, processing),
		}
	}
}

impl<C> From<(NodeIndex, IdentifyOutput<C>, Multiaddr)> for FinalUpgrade<C> {
	fn from((node_index, out, addr): (NodeIndex, IdentifyOutput<C>, Multiaddr)) -> FinalUpgrade<C> {
		match out {
			IdentifyOutput::RemoteInfo { info, observed_addr } =>
				FinalUpgrade::IdentifyDialer(node_index, info, observed_addr),
			IdentifyOutput::Sender { sender } =>
				FinalUpgrade::IdentifyListener(node_index, sender, addr),
		}
	}
}

/// Called whenever we successfully open a multistream with a remote.
fn listener_handle<'a, C>(
	shared: Arc<Shared>,
	upgrade: FinalUpgrade<C>,
) -> Box<Future<Item = (), Error = IoError> + Send + 'a>
	where C: AsyncRead + AsyncWrite + Send + 'a {
	match upgrade {
		FinalUpgrade::Kad(node_index, controller, kademlia_stream) => {
			trace!(target: "sub-libp2p", "Opened kademlia substream with #{:?}", node_index);
			match handle_kademlia_connection(shared, node_index, controller, kademlia_stream) {
				Ok(fut) => Box::new(fut) as Box<_>,
				Err(err) => Box::new(future::err(err)) as Box<_>,
			}
		},

		FinalUpgrade::IdentifyListener(node_index, sender, original_addr) => {
			trace!(target: "sub-libp2p", "Sending back identification info to #{}", node_index);
			sender.send(
				IdentifyInfo {
					public_key: shared.network_state.local_public_key().clone(),
					protocol_version: concat!("substrate/",
						env!("CARGO_PKG_VERSION")).to_owned(),		// TODO: ?
					agent_version: concat!("substrate/",
						env!("CARGO_PKG_VERSION")).to_owned(),
					listen_addrs: shared.listened_addrs.read().clone(),
					protocols: Vec::new(),		// TODO: protocols_to_report,
				},
				&original_addr
			)
		},

		FinalUpgrade::IdentifyDialer(node_index, info, observed_addr) => {
			process_identify_info(&shared, node_index, &info, &observed_addr);
			Box::new(future::ok(()))
		},

		FinalUpgrade::PingListener(node_index, future) => {
			trace!(target: "sub-libp2p", "Received ping substream from #{}", node_index);
			future
		},

		FinalUpgrade::PingDialer(node_index, pinger, future) => {
			let ping_connec = match shared.network_state.ping_connection(node_index) {
				Some(p) => p,
				None => return Box::new(future::ok(())) as Box<_>
			};
			trace!(target: "sub-libp2p", "Successfully opened ping substream with #{}", node_index);
			let fut = ping_connec.tie_or_passthrough(pinger, future);
			Box::new(fut) as Box<_>
		},

		FinalUpgrade::Custom(node_index, custom_proto_out) => {
			// A "custom" protocol is one that is part of substrate and not part of libp2p.
			let shared = shared.clone();
			let fut = handle_custom_connection(shared, node_index, custom_proto_out);
			Box::new(fut) as Box<_>
		},
	}
}

/// Handles a newly-opened Kademlia connection.
fn handle_kademlia_connection(
	shared: Arc<Shared>,
	node_index: NodeIndex,
	controller: KadConnecController,
	kademlia_stream: Box<Stream<Item = KadIncomingRequest, Error = IoError> + Send>
) -> Result<impl Future<Item = (), Error = IoError>, IoError> {
	let kad_connec = match shared.network_state.kad_connection(node_index) {
		Some(kad) => kad,
		None => return Err(IoError::new(IoErrorKind::Other, "node no longer exists")),
	};

	let node_id = match shared.network_state.node_id_from_index(node_index) {
		Some(id) => id,
		None => return Err(IoError::new(IoErrorKind::Other, "node no longer exists")),
	};

	let node_id2 = node_id.clone();
	let future = future::loop_fn(kademlia_stream, move |kademlia_stream| {
		let shared = shared.clone();
		let node_id = node_id.clone();

		let next = kademlia_stream
			.into_future()
			.map_err(|(err, _)| err);

		Timeout::new(next, Duration::from_secs(20))
			.map_err(|err|
				// TODO: improve the error reporting here, but tokio-timer's API is bad
				IoError::new(IoErrorKind::Other, err)
			)
			.and_then(move |(req, rest)| {
				shared.kad_system.update_kbuckets(node_id);
				match req {
					Some(KadIncomingRequest::FindNode { searched, responder }) => {
						let resp = build_kademlia_response(&shared, &searched);
						trace!(target: "sub-libp2p", "Responding to Kad {:?} with {:?}", searched, resp);
						responder.respond(resp)
					},
					Some(KadIncomingRequest::PingPong) => (),
					None => return Ok(future::Loop::Break(()))
				}
				Ok(future::Loop::Continue(rest))
			})
	}).then(move |val| {
		trace!(target: "sub-libp2p", "Closed Kademlia connection with #{} {:?} => {:?}", node_index, node_id2, val);
		val
	});

	Ok(kad_connec.tie_or_passthrough(controller, future))
}

/// When a remote performs a `FIND_NODE` Kademlia request for `searched`,
/// this function builds the response to send back.
fn build_kademlia_response(
	shared: &Arc<Shared>,
	searched: &PeerstorePeerId
) -> Vec<KadPeer> {
	shared.kad_system
		.known_closest_peers(searched)
		.map(move |who| {
			if who == *shared.kad_system.local_peer_id() {
				KadPeer {
					node_id: who.clone(),
					multiaddrs: shared.listened_addrs.read().clone(),
					connection_ty: KadConnectionType::Connected,
				}
			} else {
				let mut addrs = shared.network_state.addrs_of_peer(&who);
				let connected = addrs.iter().any(|&(_, conn)| conn);
				// The Kademlia protocol of libp2p doesn't allow specifying which address is valid
				// and which is outdated, therefore in order to stay honest towards the network
				// we only report the addresses we're connected to if we're connected to any.
				if connected {
					addrs = addrs.into_iter()
						.filter_map(|(a, c)| if c { Some((a, c)) } else { None })
						.collect();
				}

				KadPeer {
					node_id: who.clone(),
					multiaddrs: addrs.into_iter().map(|(a, _)| a).collect(),
					connection_ty: if connected {
						KadConnectionType::Connected
					} else {
						KadConnectionType::NotConnected
					},
				}
			}
		})
		// TODO: we really want to remove nodes with no multiaddress from
		// the results, but a flaw in the Kad protocol of libp2p makes it
		// impossible to return empty results ; therefore we must at least
		// return ourselves
		.filter(|p| p.node_id == *shared.kad_system.local_peer_id() ||
			!p.multiaddrs.is_empty())
		.take(20)
		.collect::<Vec<_>>()
}

/// Handles a newly-opened connection to a remote with a custom protocol
/// (eg. `/substrate/dot/0`).
/// Returns a future that corresponds to when the handling is finished.
fn handle_custom_connection(
	shared: Arc<Shared>,
	node_index: NodeIndex,
	custom_proto_out: RegisteredProtocolOutput<Arc<NetworkProtocolHandler + Send + Sync>>
) -> Box<Future<Item = (), Error = IoError> + Send> {
	let handler = custom_proto_out.custom_data;
	let protocol_id = custom_proto_out.protocol_id;

	// Determine the ID of this peer, or drop the connection if the peer is disabled,
	// if we reached `max_peers`, or a similar reason.
	// TODO: is there a better way to refuse connections than to drop the
	//		 newly-opened substream? should we refuse the connection
	//		 beforehand?
	let unique_connec = match shared.network_state.custom_proto(
		node_index,
		protocol_id,
	) {
		Some(c) => c,
		None => return Box::new(future::err(IoErrorKind::Other.into())) as Box<_>,
	};

	if let UniqueConnecState::Full = unique_connec.state() {
		debug!(target: "sub-libp2p",
			"Interrupting connection attempt to #{} with {:?} because we're already connected",
			node_index,
			custom_proto_out.protocol_id
		);
		return Box::new(future::ok(())) as Box<_>
	}

	struct ProtoDisconnectGuard {
		inner: Arc<Shared>,
		who: NodeIndex,
		handler: Arc<NetworkProtocolHandler + Send + Sync>,
		protocol: ProtocolId,
		print_log_message: bool,
	}

	impl Drop for ProtoDisconnectGuard {
		fn drop(&mut self) {
			if self.print_log_message {
				info!(target: "sub-libp2p",
					"Node {:?} with peer ID {} through protocol {:?} disconnected",
					self.inner.network_state.node_id_from_index(self.who),
					self.who,
					self.protocol
				);
			}
			self.handler.disconnected(&NetworkContextImpl {
				inner: self.inner.clone(),
				protocol: self.protocol,
				current_peer: Some(self.who),
			}, &self.who);

			// When any custom protocol drops, we drop the peer entirely.
			// TODO: is this correct?
			self.inner.network_state.drop_peer(self.who);
		}
	}

	let mut dc_guard = ProtoDisconnectGuard {
		inner: shared.clone(),
		who: node_index,
		handler: handler.clone(),
		protocol: protocol_id,
		print_log_message: true,
	};

	let fut = custom_proto_out.incoming
		.for_each({
			let shared = shared.clone();
			let handler = handler.clone();
			move |(packet_id, data)| {
				if let Some(id) = shared.network_state.node_id_from_index(node_index) {
					shared.kad_system.update_kbuckets(id);
				}
				handler.read(&NetworkContextImpl {
					inner: shared.clone(),
					protocol: protocol_id,
					current_peer: Some(node_index.clone()),
				}, &node_index, packet_id, &data);
				Ok(())
			}
		});

	let val = (custom_proto_out.outgoing, custom_proto_out.protocol_version);
	let final_fut = unique_connec.tie_or_stop(val, fut)
		.then(move |val| {
			info!(target: "sub-libp2p", "Finishing future for proto {:?} with {:?} => {:?}",
				protocol_id, node_index, val);
			// Makes sure that `dc_guard` is kept alive until here.
			dc_guard.print_log_message = false;
			drop(dc_guard);
			val
		});

	debug!(target: "sub-libp2p",
		"Successfully connected to {:?} (peer id #{}) with protocol {:?} version {}",
		shared.network_state.node_id_from_index(node_index),
		node_index,
		protocol_id,
		custom_proto_out.protocol_version
	);

	handler.connected(&NetworkContextImpl {
		inner: shared.clone(),
		protocol: protocol_id,
		current_peer: Some(node_index),
	}, &node_index);

	Box::new(final_fut) as Box<_>
}

/// Randomly discovers peers to connect to.
/// This works by running a round at a regular interval, and skipping if we
/// reached `min_peers`. When we are over `min_peers`, we stop trying to dial
/// nodes and only accept incoming connections.
fn start_kademlia_discovery<T, To, St, C>(
	shared: Arc<Shared>,
	transport: T,
	swarm_controller: SwarmController<St, Box<Future<Item = (), Error = IoError> + Send>>
) -> Box<Future<Item = (), Error = IoError> + Send>
	where T: MuxedTransport<Output =  TransportOutput<To>> + Clone + Send + 'static,
		T::Dial: Send,
		T::MultiaddrFuture: Send + 'static,
		T::Listener: Send,
		T::ListenerUpgrade: Send,
		T::Incoming: Send,
		T::IncomingUpgrade: Send,
		To: AsyncRead + AsyncWrite + Send + 'static,
		St: MuxedTransport<Output = FinalUpgrade<C>> + Clone + Send + 'static,
		St::Dial: Send,
		St::MultiaddrFuture: Send,
		St::Listener: Send,
		St::ListenerUpgrade: Send,
		St::Incoming: Send,
		St::IncomingUpgrade: Send,
		C: Send + 'static {
	let kad_init = shared.kad_system.perform_initialization({
		let shared = shared.clone();
		let transport = transport.clone();
		let swarm_controller = swarm_controller.clone();
		move |who|
			obtain_kad_connection(
				shared.clone(),
				who.clone(),
				transport.clone(),
				swarm_controller.clone()
			)
	});

	// We perform a random Kademlia query at a regular interval.
	let discovery = Interval::new(Instant::now(), Duration::from_secs(32))
		// TODO: add a timeout to the lookups?
		.map_err(|err| IoError::new(IoErrorKind::Other, err))
		.for_each({
			let shared = shared.clone();
			let transport = transport.clone();
			let swarm_controller = swarm_controller.clone();
			move |_| {
				let _ = shared.network_state.flush_caches_to_disk();
				perform_kademlia_query(shared.clone(), transport.clone(), swarm_controller.clone())
			}
		});

	let final_future = kad_init
		.select(discovery)
		.map_err(|(err, _)| err)
		.and_then(|(_, rest)| rest);

	// Note that we use a Box in order to speed compilation time.
	Box::new(final_future) as Box<Future<Item = _, Error = _> + Send>
}

/// Performs a kademlia request to a random node.
/// Note that we don't actually care about the results, so the future
/// produces `()`.
fn perform_kademlia_query<T, To, St, C>(
	shared: Arc<Shared>,
	transport: T,
	swarm_controller: SwarmController<St, Box<Future<Item = (), Error = IoError> + Send>>
) -> Box<Future<Item = (), Error = IoError> + Send>
	where T: MuxedTransport<Output = TransportOutput<To>> + Clone + Send + 'static,
		T::MultiaddrFuture: Send + 'static,
		T::Dial: Send,
		T::Listener: Send,
		T::ListenerUpgrade: Send,
		T::Incoming: Send,
		T::IncomingUpgrade: Send,
		To: AsyncRead + AsyncWrite + Send + 'static,
		St: MuxedTransport<Output = FinalUpgrade<C>> + Send + Clone + 'static,
		St::Dial: Send,
		St::MultiaddrFuture: Send,
		St::Listener: Send,
		St::ListenerUpgrade: Send,
		St::Incoming: Send,
		St::IncomingUpgrade: Send,
		C: Send + 'static {
	// Query the node IDs that are closest to a random ID.
	// Note that the randomness doesn't have to be secure, as this only
	// influences which nodes we end up being connected to.
	let random_key = PublicKey::Ed25519((0 .. 32)
		.map(|_| -> u8 { rand::random() }).collect());
	let random_peer_id = random_key.into_peer_id();
	trace!(target: "sub-libp2p", "Start kademlia discovery for {:?}", random_peer_id);

	let future = shared.clone()
		.kad_system
		.find_node(random_peer_id, {
			let shared = shared.clone();
			let transport = transport.clone();
			let swarm_controller = swarm_controller.clone();
			move |who| obtain_kad_connection(shared.clone(), who.clone(),
				transport.clone(), swarm_controller.clone())
		})
		.filter_map(move |event|
			match event {
				KadQueryEvent::PeersReported(peers) => {
					for peer in peers {
						let connected = match peer.connection_ty {
							KadConnectionType::NotConnected => false,
							KadConnectionType::Connected => true,
							KadConnectionType::CanConnect => true,
							KadConnectionType::CannotConnect => continue,
						};

						for addr in peer.multiaddrs {
							shared.network_state.add_kad_discovered_addr(
								&peer.node_id,
								addr,
								connected
							);
						}
					}
					None
				},
				KadQueryEvent::Finished(_) => Some(()),
			}
		)
		.into_future()
		.map_err(|(err, _)| err)
		.map(|_| ());

	// Note that we use a `Box` in order to speed up compilation.
	Box::new(future) as Box<Future<Item = _, Error = _> + Send>
}

/// Connects to additional nodes, if necessary.
fn connect_to_nodes<T, To, St, C>(
	shared: Arc<Shared>,
	base_transport: T,
	swarm_controller: &SwarmController<St, Box<Future<Item = (), Error = IoError> + Send>>
)
	where T: MuxedTransport<Output = TransportOutput<To>> + Clone + Send + 'static,
		T::MultiaddrFuture: Send + 'static,
		T::Dial: Send,
		T::Listener: Send,
		T::ListenerUpgrade: Send,
		T::Incoming: Send,
		T::IncomingUpgrade: Send,
		To: AsyncRead + AsyncWrite + Send + 'static,
		St: MuxedTransport<Output = FinalUpgrade<C>> + Clone + Send + 'static,
		St::Dial: Send,
		St::MultiaddrFuture: Send,
		St::Listener: Send,
		St::ListenerUpgrade: Send,
		St::Incoming: Send,
		St::IncomingUpgrade: Send,
		C: Send + 'static {
	let (addrs, _will_change) = shared.network_state.outgoing_connections_to_attempt();

	for (peer, addr) in addrs.into_iter() {
		// Try to dial that node for each registered protocol. Since dialing
		// upgrades the connection to use multiplexing, dialing multiple times
		// should automatically open multiple substreams.
		for proto in shared.protocols.0.clone().into_iter() {
			open_peer_custom_proto(
				shared.clone(),
				base_transport.clone(),
				addr.clone(),
				Some(peer.clone()),
				proto,
				swarm_controller
			)
		}
	}
}

/// Dials the given address for the given protocol and using the given `swarm_controller`.
///
/// This function *always* performs a dial, and doesn't check whether we already have an existing
/// connection to the remote. This is expected to be checked by the caller.
///
/// The dialing will fail if the obtained peer ID doesn't match the expected ID. This is an
/// opinionated decision, as we could just let the new connection through. But we decide not to.
/// If `None` is passed for the expected peer ID, we always accept the connection.
fn open_peer_custom_proto<T, To, St, C>(
	shared: Arc<Shared>,
	base_transport: T,
	addr: Multiaddr,
	expected_peer_id: Option<PeerstorePeerId>,
	proto: RegisteredProtocol<Arc<NetworkProtocolHandler + Send + Sync>>,
	swarm_controller: &SwarmController<St, Box<Future<Item = (), Error = IoError> + Send>>
)
	where T: MuxedTransport<Output = TransportOutput<To>> + Clone + Send + 'static,
		T::MultiaddrFuture: Send + 'static,
		T::Dial: Send,
		T::Listener: Send,
		T::ListenerUpgrade: Send,
		T::Incoming: Send,
		T::IncomingUpgrade: Send,
		To: AsyncRead + AsyncWrite + Send + 'static,
		St: MuxedTransport<Output = FinalUpgrade<C>> + Clone + Send + 'static,
		St::Dial: Send,
		St::MultiaddrFuture: Send,
		St::Listener: Send,
		St::ListenerUpgrade: Send,
		St::Incoming: Send,
		St::IncomingUpgrade: Send,
		C: Send + 'static,
{
	let proto_id = proto.id();

	let with_proto = base_transport
		.and_then(move |out, endpoint, client_addr| {
			let node_index = out.node_index;
			upgrade::apply(out.socket, proto, endpoint, client_addr)
				.map(move |(custom, client_addr)|
					((node_index, FinalUpgrade::Custom(node_index, custom)), client_addr))
		});

	let with_timeout = TransportTimeout::new(with_proto, Duration::from_secs(20));

	if let Some(expected_peer_id) = expected_peer_id {
		let expected_node_index = match shared.network_state.assign_node_index(&expected_peer_id) {
			Ok(i) => i,
			Err(_) => return,
		};

		let unique_connec = match shared.network_state.custom_proto(expected_node_index, proto_id) {
			Some(uc) => uc,
			None => return,
		};

		let with_peer_check = with_timeout
			.and_then(move |(node_index, custom), _, client_addr| {
				if node_index == expected_node_index {
					future::ok((custom, client_addr))
				} else {
					future::err(IoError::new(IoErrorKind::ConnectionRefused, "Peer id mismatch"))
				}
			});

		trace!(target: "sub-libp2p",
			"Opening connection to {:?} through {} with proto {:?}",
			expected_peer_id,
			addr,
			proto_id
		);

		let _ = unique_connec.dial(swarm_controller, &addr, with_peer_check);

	} else {
		let trans = with_timeout.map(|(_, out), _| out);
		if let Err(addr) = swarm_controller.dial(addr, trans) {
			debug!(target: "sub-libp2p", "Failed to dial {:?}", addr);
		}
	}
}

/// Obtain a Kademlia connection to the given peer.
fn obtain_kad_connection<T, To, St, C>(
	shared: Arc<Shared>,
	who: PeerstorePeerId,
	transport: T,
	swarm_controller: SwarmController<St, Box<Future<Item = (), Error = IoError> + Send>>
) -> Box<Future<Item = KadConnecController, Error = IoError> + Send>
	where T: MuxedTransport<Output =  TransportOutput<To>> + Clone + Send + 'static,
		T::MultiaddrFuture: Send + 'static,
		T::Dial: Send,
		T::Listener: Send,
		T::ListenerUpgrade: Send,
		T::Incoming: Send,
		T::IncomingUpgrade: Send,
		To: AsyncRead + AsyncWrite + Send + 'static,
		St: MuxedTransport<Output = FinalUpgrade<C>> + Clone + Send + 'static,
		St::Dial: Send,
		St::MultiaddrFuture: Send,
		St::Listener: Send,
		St::ListenerUpgrade: Send,
		St::Incoming: Send,
		St::IncomingUpgrade: Send,
		C: Send + 'static {
	let kad_upgrade = shared.kad_upgrade.clone();
	let transport = transport
		.and_then(move |out, endpoint, client_addr| {
			let node_index = out.node_index;
			upgrade::apply(out.socket, kad_upgrade.clone(), endpoint, client_addr)
				.map(move |((ctrl, fut), addr)| (FinalUpgrade::Kad(node_index, ctrl, fut), addr))
		});

	// This function consists in trying all the addresses we know one by one until we find
	// one that works.
	//
	// This `future` returns a Kad controller, or an error if all dialing attempts failed.
	let future = stream::iter_ok(shared.network_state.addrs_of_peer(&who))
		.and_then(move |addr| {
			let node_index = shared.network_state.assign_node_index(&who)?;
			let kad = match shared.network_state.kad_connection(node_index) {
				Some(kad) => kad,
				None => return Err(IoError::new(IoErrorKind::Other, "node no longer exists")),
			};
			Ok((kad, addr))
		})
		.and_then(move |(unique_connec, addr)| {
			unique_connec.dial(&swarm_controller, &addr.0, transport.clone())
		})
		.then(|result| -> Result<_, ()> { Ok(result.ok()) })
		.filter_map(|result| result)
		.into_future()
		.map_err(|_| -> IoError { unreachable!("all items always succeed") })
		.and_then(|(kad, _)| kad.ok_or_else(|| IoErrorKind::ConnectionRefused.into()));

	// Note that we use a Box in order to speed up compilation.
	Box::new(future) as Box<Future<Item = _, Error = _> + Send>
}

/// Processes the identification information that we received about a node.
fn process_identify_info(
	shared: &Shared,
	node_index: NodeIndex,
	info: &IdentifyInfo,
	observed_addr: &Multiaddr,
) {
	trace!(target: "sub-libp2p", "Received identification info from #{}", node_index);

	shared.network_state.set_node_info(node_index, info.agent_version.clone());

	for original_listened_addr in &*shared.original_listened_addr.read() {
		// TODO: we're using a hack here ; ideally we would call `nat_traversal` on our
		// `Transport` ; but that's complicated to pass around ; we could put it in a `Box` in
		// `Shared`, but since our transport doesn't implement `Send` (libp2p doesn't implement
		// `Send` on modifiers), we can't. Instead let's just recreate a transport locally every
		// time.
		let transport = libp2p::tcp::TcpConfig::new();
		if let Some(mut ext_addr) = transport.nat_traversal(original_listened_addr, &observed_addr) {
			let mut listened_addrs = shared.listened_addrs.write();
			if !listened_addrs.iter().any(|a| a == &ext_addr) {
				trace!(target: "sub-libp2p",
					"NAT traversal: remote observes us as {}; registering {} as one of our own addresses",
					observed_addr,
					ext_addr
				);
				listened_addrs.push(ext_addr.clone());
				ext_addr.append(AddrComponent::P2P(shared.kad_system
					.local_peer_id().clone().into()));
				info!(target: "sub-libp2p", "New external node address: {}", ext_addr);
			}
		}
	}

	for addr in info.listen_addrs.iter() {
		if let Some(node_id) = shared.network_state.node_id_from_index(node_index) {
			shared.network_state.add_kad_discovered_addr(&node_id, addr.clone(), true);
		}
	}
}

/// Returns a future that regularly pings every peer we're connected to.
/// If a peer doesn't respond after a while, we disconnect it.
fn start_periodic_updates<T, To, St, C>(
	shared: Arc<Shared>,
	transport: T,
	swarm_controller: SwarmController<St, Box<Future<Item = (), Error = IoError> + Send>>
) -> Box<Future<Item = (), Error = IoError> + Send>
	where T: MuxedTransport<Output = TransportOutput<To>> + Clone + Send + 'static,
		T::MultiaddrFuture: Send + 'static,
		T::Dial: Send,
		T::Listener: Send,
		T::ListenerUpgrade: Send,
		T::Incoming: Send,
		T::IncomingUpgrade: Send,
		To: AsyncRead + AsyncWrite + Send + 'static,
		St: MuxedTransport<Output = FinalUpgrade<C>> + Clone + Send + 'static,
		St::Dial: Send,
		St::MultiaddrFuture: Send,
		St::Listener: Send,
		St::ListenerUpgrade: Send,
		St::Incoming: Send,
		St::IncomingUpgrade: Send,
		C: Send + 'static {
	let ping_transport = transport.clone()
		.and_then(move |out, endpoint, client_addr| {
			let node_index = out.node_index;
			upgrade::apply(out.socket, ping::Ping, endpoint, client_addr)
				.map(move |(stream, addr)| (FinalUpgrade::from((node_index, stream)), addr))
		});
	
	let identify_transport = transport
		.and_then(move |out, endpoint, client_addr| {
			let node_index = out.node_index;
			upgrade::apply(out.socket, IdentifyProtocolConfig, endpoint, client_addr)
				.map(move |(id, addr)| {
					let fin = match id {
						IdentifyOutput::RemoteInfo { info, observed_addr } =>
							FinalUpgrade::IdentifyDialer(node_index, info, observed_addr),
						IdentifyOutput::Sender { .. } => unreachable!("can't reach that on the dialer side"),
					};
					(fin, addr)
				})
		});

	let fut = Interval::new(Instant::now() + Duration::from_secs(5), Duration::from_secs(30))
		.map_err(|err| IoError::new(IoErrorKind::Other, err))
		.for_each(move |_| periodic_updates(
			shared.clone(),
			ping_transport.clone(),
			identify_transport.clone(),
			&swarm_controller
		))
		.then(|val| {
			warn!(target: "sub-libp2p", "Periodic updates stream has stopped: {:?}", val);
			val
		});

	// Note that we use a Box in order to speed compilation time.
	Box::new(fut) as Box<Future<Item = _, Error = _> + Send>
}

/// Pings all the nodes we're connected to and disconnects any node that
/// doesn't respond. Identifies nodes that need to be identified. Returns
/// a `Future` when all the pings have either suceeded or timed out.
fn periodic_updates<Tp, Tid, St, C>(
	shared: Arc<Shared>,
	ping_transport: Tp,
	identify_transport: Tid,
	swarm_controller: &SwarmController<St, Box<Future<Item = (), Error = IoError> + Send>>
) -> Box<Future<Item = (), Error = IoError> + Send>
	where Tp: MuxedTransport<Output = FinalUpgrade<C>> + Clone + Send + 'static,
		Tp::MultiaddrFuture: Send + 'static,
		Tp::Dial: Send,
		Tp::MultiaddrFuture: Send,
		Tp::Listener: Send,
		Tp::ListenerUpgrade: Send,
		Tp::Incoming: Send,
		Tp::IncomingUpgrade: Send,
		Tid: MuxedTransport<Output = FinalUpgrade<C>> + Clone + Send + 'static,
		Tid::MultiaddrFuture: Send + 'static,
		Tid::Dial: Send,
		Tid::MultiaddrFuture: Send,
		Tid::Listener: Send,
		Tid::ListenerUpgrade: Send,
		Tid::Incoming: Send,
		Tid::IncomingUpgrade: Send,
		St: MuxedTransport<Output = FinalUpgrade<C>> + Clone + Send + 'static,
		St::Dial: Send,
		St::MultiaddrFuture: Send,
		St::Listener: Send,
		St::ListenerUpgrade: Send,
		St::Incoming: Send,
		St::IncomingUpgrade: Send,
		C: Send + 'static {
	trace!(target: "sub-libp2p", "Periodic update cycle");

	let mut ping_futures = Vec::new();

	for PeriodicUpdate { node_index, peer_id, address, pinger, identify } in
		shared.network_state.cleanup_and_prepare_updates() {
		let shared = shared.clone();

		let fut = pinger
			.dial(&swarm_controller, &address, ping_transport.clone())
			.and_then(move |mut p| {
				trace!(target: "sub-libp2p", "Pinging peer #{} aka. {:?}", node_index, peer_id);
				p.ping()
					.map(move |()| peer_id)
					.map_err(|err| IoError::new(IoErrorKind::Other, err))
			});
		let ping_start_time = Instant::now();
		let fut = Timeout::new_at(fut, ping_start_time + Duration::from_secs(30))
			.then(move |val|
				match val {
					Err(err) => {
						trace!(target: "sub-libp2p", "Error while pinging #{:?} => {:?}", node_index, err);
						shared.network_state.report_ping_failed(node_index);
						// Return Ok, otherwise we would close the ping service
						Ok(())
					},
					Ok(who) => {
						let elapsed = ping_start_time.elapsed();
						trace!(target: "sub-libp2p", "Pong from #{:?} in {:?}", who, elapsed);
						shared.network_state.report_ping_duration(node_index, elapsed);
						shared.kad_system.update_kbuckets(who);
						Ok(())
					}
				}
			);
		ping_futures.push(fut);

		if identify {
			// Ignore dialing errors, as identifying is only about diagnostics.
			trace!(target: "sub-libp2p", "Attempting to identify #{}", node_index);
			let _ = swarm_controller.dial(address, identify_transport.clone());
		}
	}

	let future = future::loop_fn(ping_futures, |ping_futures| {
		if ping_futures.is_empty() {
			let fut = future::ok(future::Loop::Break(()));
			return future::Either::A(fut)
		}

		let fut = future::select_all(ping_futures)
			.map(|((), _, rest)| future::Loop::Continue(rest))
			.map_err(|(err, _, _)| err);
		future::Either::B(fut)
	});

	// Note that we use a Box in order to speed up compilation.
	Box::new(future) as Box<Future<Item = _, Error = _> + Send>
}

/// Since new protocols are added after the networking starts, we have to load the protocols list
/// in a lazy way. This is what this wrapper does.
#[derive(Clone)]
struct DelayedProtosList(Arc<Shared>);
// `Maf` is short for `MultiaddressFuture`
impl<C, Maf> ConnectionUpgrade<C, Maf> for DelayedProtosList
where C: AsyncRead + AsyncWrite + Send + 'static,		// TODO: 'static :-/
	Maf: Future<Item = Multiaddr, Error = IoError> + Send + 'static,		// TODO: 'static :(
{
	type NamesIter = <RegisteredProtocols<Arc<NetworkProtocolHandler + Send + Sync>> as ConnectionUpgrade<C, Maf>>::NamesIter;
	type UpgradeIdentifier = <RegisteredProtocols<Arc<NetworkProtocolHandler + Send + Sync>> as ConnectionUpgrade<C, Maf>>::UpgradeIdentifier;

	fn protocol_names(&self) -> Self::NamesIter {
		ConnectionUpgrade::<C, Maf>::protocol_names(&self.0.protocols)
	}

	type Output = <RegisteredProtocols<Arc<NetworkProtocolHandler + Send + Sync>> as ConnectionUpgrade<C, Maf>>::Output;
	type MultiaddrFuture = <RegisteredProtocols<Arc<NetworkProtocolHandler + Send + Sync>> as ConnectionUpgrade<C, Maf>>::MultiaddrFuture;
	type Future = <RegisteredProtocols<Arc<NetworkProtocolHandler + Send + Sync>> as ConnectionUpgrade<C, Maf>>::Future;

	#[inline]
	fn upgrade(self, socket: C, id: Self::UpgradeIdentifier, endpoint: Endpoint,
		remote_addr: Maf) -> Self::Future
	{
		self.0.protocols
			.clone()
			.upgrade(socket, id, endpoint, remote_addr)
	}
}

#[cfg(test)]
mod tests {
	use super::NetworkService;

	#[test]
	fn builds_and_finishes_in_finite_time() {
		// Checks that merely starting the network doesn't end up in an infinite loop.
		let _service = NetworkService::new(Default::default(), vec![]).unwrap();
	}
}
