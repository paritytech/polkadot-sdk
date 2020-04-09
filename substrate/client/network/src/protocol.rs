// Copyright 2017-2020 Parity Technologies (UK) Ltd.
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

use crate::config::ProtocolId;
use crate::utils::interval;
use bytes::{Bytes, BytesMut};
use futures::prelude::*;
use generic_proto::{GenericProto, GenericProtoOut};
use libp2p::{Multiaddr, PeerId};
use libp2p::core::{ConnectedPoint, connection::{ConnectionId, ListenerId}};
use libp2p::swarm::{ProtocolsHandler, IntoProtocolsHandler};
use libp2p::swarm::{NetworkBehaviour, NetworkBehaviourAction, PollParameters};
use sp_core::{
	storage::{StorageKey, ChildInfo},
	hexdisplay::HexDisplay
};
use sp_consensus::{
	BlockOrigin,
	block_validation::BlockAnnounceValidator,
	import_queue::{BlockImportResult, BlockImportError, IncomingBlock, Origin}
};
use codec::{Decode, Encode};
use sp_runtime::{generic::BlockId, ConsensusEngineId, Justification};
use sp_runtime::traits::{
	Block as BlockT, Header as HeaderT, NumberFor, One, Zero, CheckedSub
};
use sp_arithmetic::traits::SaturatedConversion;
use message::{BlockAnnounce, Message};
use message::generic::{Message as GenericMessage, ConsensusMessage, Roles};
use prometheus_endpoint::{Registry, Gauge, GaugeVec, HistogramVec, PrometheusError, Opts, register, U64};
use sync::{ChainSync, SyncState};
use crate::service::{TransactionPool, ExHashT};
use crate::config::BoxFinalityProofRequestBuilder;
use std::borrow::Cow;
use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};
use std::sync::Arc;
use std::fmt::Write;
use std::{cmp, io, num::NonZeroUsize, pin::Pin, task::Poll, time};
use log::{log, Level, trace, debug, warn, error};
use crate::chain::{Client, FinalityProofProvider};
use sc_client_api::{ChangesProof, StorageProof};
use crate::error;
use util::LruHashSet;
use wasm_timer::Instant;

// Include sources generated from protobuf definitions.
pub mod api {
	pub mod v1 {
		include!(concat!(env!("OUT_DIR"), "/api.v1.rs"));
		pub mod light {
			include!(concat!(env!("OUT_DIR"), "/api.v1.light.rs"));
		}
	}
}

mod generic_proto;
mod util;

pub mod block_requests;
pub mod message;
pub mod event;
pub mod light_client_handler;
pub mod sync;

pub use block_requests::BlockRequests;
pub use light_client_handler::LightClientHandler;
pub use generic_proto::LegacyConnectionKillError;

const REQUEST_TIMEOUT_SEC: u64 = 40;
/// Interval at which we perform time based maintenance
const TICK_TIMEOUT: time::Duration = time::Duration::from_millis(1100);
/// Interval at which we propagate extrinsics;
const PROPAGATE_TIMEOUT: time::Duration = time::Duration::from_millis(2900);

/// Maximim number of known block hashes to keep for a peer.
const MAX_KNOWN_BLOCKS: usize = 1024; // ~32kb per peer + LruHashSet overhead
/// Maximim number of known extrinsic hashes to keep for a peer.
const MAX_KNOWN_EXTRINSICS: usize = 4096; // ~128kb per peer + overhead

/// Current protocol version.
pub(crate) const CURRENT_VERSION: u32 = 6;
/// Lowest version we support
pub(crate) const MIN_VERSION: u32 = 3;

// Maximum allowed entries in `BlockResponse`
const MAX_BLOCK_DATA_RESPONSE: u32 = 128;
/// When light node connects to the full node and the full node is behind light node
/// for at least `LIGHT_MAXIMAL_BLOCKS_DIFFERENCE` blocks, we consider it not useful
/// and disconnect to free connection slot.
const LIGHT_MAXIMAL_BLOCKS_DIFFERENCE: u64 = 8192;

mod rep {
	use sc_peerset::ReputationChange as Rep;
	/// Reputation change when a peer is "clogged", meaning that it's not fast enough to process our
	/// messages.
	pub const CLOGGED_PEER: Rep = Rep::new(-(1 << 12), "Clogged message queue");
	/// Reputation change when a peer doesn't respond in time to our messages.
	pub const TIMEOUT: Rep = Rep::new(-(1 << 10), "Request timeout");
	/// Reputation change when a peer sends us a status message while we already received one.
	pub const UNEXPECTED_STATUS: Rep = Rep::new(-(1 << 20), "Unexpected status message");
	/// Reputation change when we are a light client and a peer is behind us.
	pub const PEER_BEHIND_US_LIGHT: Rep = Rep::new(-(1 << 8), "Useless for a light peer");
	/// Reputation change when a peer sends us an extrinsic that we didn't know about.
	pub const GOOD_EXTRINSIC: Rep = Rep::new(1 << 7, "Good extrinsic");
	/// Reputation change when a peer sends us a bad extrinsic.
	pub const BAD_EXTRINSIC: Rep = Rep::new(-(1 << 12), "Bad extrinsic");
	/// We sent an RPC query to the given node, but it failed.
	pub const RPC_FAILED: Rep = Rep::new(-(1 << 12), "Remote call failed");
	/// We received a message that failed to decode.
	pub const BAD_MESSAGE: Rep = Rep::new(-(1 << 12), "Bad message");
	/// We received an unexpected response.
	pub const UNEXPECTED_RESPONSE: Rep = Rep::new_fatal("Unexpected response packet");
	/// We received an unexpected extrinsic packet.
	pub const UNEXPECTED_EXTRINSICS: Rep = Rep::new_fatal("Unexpected extrinsics packet");
	/// We received an unexpected light node request.
	pub const UNEXPECTED_REQUEST: Rep = Rep::new_fatal("Unexpected block request packet");
	/// Peer has different genesis.
	pub const GENESIS_MISMATCH: Rep = Rep::new_fatal("Genesis mismatch");
	/// Peer is on unsupported protocol version.
	pub const BAD_PROTOCOL: Rep = Rep::new_fatal("Unsupported protocol");
	/// Peer role does not match (e.g. light peer connecting to another light peer).
	pub const BAD_ROLE: Rep = Rep::new_fatal("Unsupported role");
	/// Peer response data does not have requested bits.
	pub const BAD_RESPONSE: Rep = Rep::new(-(1 << 12), "Incomplete response");
}

struct Metrics {
	handshaking_peers: Gauge<U64>,
	obsolete_requests: Gauge<U64>,
	peers: Gauge<U64>,
	queued_blocks: Gauge<U64>,
	fork_targets: Gauge<U64>,
	finality_proofs: GaugeVec<U64>,
	justifications: GaugeVec<U64>,
}

impl Metrics {
	fn register(r: &Registry) -> Result<Self, PrometheusError> {
		Ok(Metrics {
			handshaking_peers: {
				let g = Gauge::new("sync_handshaking_peers", "Number of newly connected peers")?;
				register(g, r)?
			},
			obsolete_requests: {
				let g = Gauge::new("sync_obsolete_requests", "Number of obsolete requests")?;
				register(g, r)?
			},
			peers: {
				let g = Gauge::new("sync_peers", "Number of peers we sync with")?;
				register(g, r)?
			},
			queued_blocks: {
				let g = Gauge::new("sync_queued_blocks", "Number of blocks in import queue")?;
				register(g, r)?
			},
			fork_targets: {
				let g = Gauge::new("sync_fork_targets", "Number of fork sync targets")?;
				register(g, r)?
			},
			justifications: {
				let g = GaugeVec::new(
					Opts::new(
						"sync_extra_justifications",
						"Number of extra justifications requests"
					),
					&["status"],
				)?;
				register(g, r)?
			},
			finality_proofs: {
				let g = GaugeVec::new(
					Opts::new(
						"sync_extra_finality_proofs",
						"Number of extra finality proof requests",
					),
					&["status"],
				)?;
				register(g, r)?
			},
		})
	}
}

// Lock must always be taken in order declared here.
pub struct Protocol<B: BlockT, H: ExHashT> {
	/// Interval at which we call `tick`.
	tick_timeout: Pin<Box<dyn Stream<Item = ()> + Send>>,
	/// Interval at which we call `propagate_extrinsics`.
	propagate_timeout: Pin<Box<dyn Stream<Item = ()> + Send>>,
	/// Pending list of messages to return from `poll` as a priority.
	pending_messages: VecDeque<CustomMessageOutcome<B>>,
	config: ProtocolConfig,
	genesis_hash: B::Hash,
	sync: ChainSync<B>,
	context_data: ContextData<B, H>,
	/// List of nodes for which we perform additional logging because they are important for the
	/// user.
	important_peers: HashSet<PeerId>,
	// Connected peers pending Status message.
	handshaking_peers: HashMap<PeerId, HandshakingPeer>,
	/// Used to report reputation changes.
	peerset_handle: sc_peerset::PeersetHandle,
	transaction_pool: Arc<dyn TransactionPool<H, B>>,
	/// When asked for a proof of finality, we use this struct to build one.
	finality_proof_provider: Option<Arc<dyn FinalityProofProvider<B>>>,
	/// Handles opening the unique substream and sending and receiving raw messages.
	behaviour: GenericProto,
	/// For each legacy gossiping engine ID, the corresponding new protocol name.
	protocol_name_by_engine: HashMap<ConsensusEngineId, Cow<'static, [u8]>>,
	/// For each protocol name, the legacy equivalent.
	legacy_equiv_by_name: HashMap<Cow<'static, [u8]>, Fallback>,
	/// Name of the protocol used for transactions.
	transactions_protocol: Cow<'static, [u8]>,
	/// Name of the protocol used for block announces.
	block_announces_protocol: Cow<'static, [u8]>,
	/// Prometheus metrics.
	metrics: Option<Metrics>,
	/// The `PeerId`'s of all boot nodes.
	boot_node_ids: Arc<HashSet<PeerId>>,
}

#[derive(Default)]
struct PacketStats {
	bytes_in: u64,
	bytes_out: u64,
	count_in: u64,
	count_out: u64,
}

/// A peer that we are connected to
/// and from whom we have not yet received a Status message.
struct HandshakingPeer {
	timestamp: Instant,
}

/// Peer information
#[derive(Debug, Clone)]
struct Peer<B: BlockT, H: ExHashT> {
	info: PeerInfo<B>,
	/// Current block request, if any.
	block_request: Option<(Instant, message::BlockRequest<B>)>,
	/// Requests we are no longer interested in.
	obsolete_requests: HashMap<message::RequestId, Instant>,
	/// Holds a set of transactions known to this peer.
	known_extrinsics: LruHashSet<H>,
	/// Holds a set of blocks known to this peer.
	known_blocks: LruHashSet<B::Hash>,
	/// Request counter,
	next_request_id: message::RequestId,
}

/// Info about a peer's known state.
#[derive(Clone, Debug)]
pub struct PeerInfo<B: BlockT> {
	/// Roles
	pub roles: Roles,
	/// Protocol version
	pub protocol_version: u32,
	/// Peer best block hash
	pub best_hash: B::Hash,
	/// Peer best block number
	pub best_number: <B::Header as HeaderT>::Number,
}

/// Data necessary to create a context.
struct ContextData<B: BlockT, H: ExHashT> {
	// All connected peers
	peers: HashMap<PeerId, Peer<B, H>>,
	stats: HashMap<&'static str, PacketStats>,
	pub chain: Arc<dyn Client<B>>,
}

/// Configuration for the Substrate-specific part of the networking layer.
#[derive(Clone)]
pub struct ProtocolConfig {
	/// Assigned roles.
	pub roles: Roles,
	/// Maximum number of peers to ask the same blocks in parallel.
	pub max_parallel_downloads: u32,
}

impl Default for ProtocolConfig {
	fn default() -> ProtocolConfig {
		ProtocolConfig {
			roles: Roles::FULL,
			max_parallel_downloads: 5,
		}
	}
}

/// Fallback mechanism to use to send a notification if no substream is open.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Fallback {
	/// Use a `Message::Consensus` with the given engine ID.
	Consensus(ConsensusEngineId),
	/// The message is the bytes encoding of a `Transactions<E>` (which is itself defined as a `Vec<E>`).
	Transactions,
	/// The message is the bytes encoding of a `BlockAnnounce<H>`.
	BlockAnnounce,
}

impl<B: BlockT, H: ExHashT> Protocol<B, H> {
	/// Create a new instance.
	pub fn new(
		config: ProtocolConfig,
		chain: Arc<dyn Client<B>>,
		transaction_pool: Arc<dyn TransactionPool<H, B>>,
		finality_proof_provider: Option<Arc<dyn FinalityProofProvider<B>>>,
		finality_proof_request_builder: Option<BoxFinalityProofRequestBuilder<B>>,
		protocol_id: ProtocolId,
		peerset_config: sc_peerset::PeersetConfig,
		block_announce_validator: Box<dyn BlockAnnounceValidator<B> + Send>,
		metrics_registry: Option<&Registry>,
		boot_node_ids: Arc<HashSet<PeerId>>,
		queue_size_report: Option<HistogramVec>,
	) -> error::Result<(Protocol<B, H>, sc_peerset::PeersetHandle)> {
		let info = chain.info();
		let sync = ChainSync::new(
			config.roles,
			chain.clone(),
			&info,
			finality_proof_request_builder,
			block_announce_validator,
			config.max_parallel_downloads,
		);

		let important_peers = {
			let mut imp_p = HashSet::new();
			for reserved in peerset_config.priority_groups.iter().flat_map(|(_, l)| l.iter()) {
				imp_p.insert(reserved.clone());
			}
			imp_p.shrink_to_fit();
			imp_p
		};

		let (peerset, peerset_handle) = sc_peerset::Peerset::from_config(peerset_config);
		let versions = &((MIN_VERSION as u8)..=(CURRENT_VERSION as u8)).collect::<Vec<u8>>();
		let mut behaviour = GenericProto::new(protocol_id.clone(), versions, peerset, queue_size_report);

		let mut legacy_equiv_by_name = HashMap::new();

		let transactions_protocol: Cow<'static, [u8]> = Cow::from({
			let mut proto = b"/".to_vec();
			proto.extend(protocol_id.as_bytes());
			proto.extend(b"/transactions/1");
			proto
		});
		behaviour.register_notif_protocol(transactions_protocol.clone(), Vec::new());
		legacy_equiv_by_name.insert(transactions_protocol.clone(), Fallback::Transactions);

		let block_announces_protocol: Cow<'static, [u8]> = Cow::from({
			let mut proto = b"/".to_vec();
			proto.extend(protocol_id.as_bytes());
			proto.extend(b"/block-announces/1");
			proto
		});
		behaviour.register_notif_protocol(block_announces_protocol.clone(), Vec::new());
		legacy_equiv_by_name.insert(block_announces_protocol.clone(), Fallback::BlockAnnounce);

		let protocol = Protocol {
			tick_timeout: Box::pin(interval(TICK_TIMEOUT)),
			propagate_timeout: Box::pin(interval(PROPAGATE_TIMEOUT)),
			pending_messages: VecDeque::new(),
			config,
			context_data: ContextData {
				peers: HashMap::new(),
				stats: HashMap::new(),
				chain,
			},
			genesis_hash: info.genesis_hash,
			sync,
			handshaking_peers: HashMap::new(),
			important_peers,
			transaction_pool,
			finality_proof_provider,
			peerset_handle: peerset_handle.clone(),
			behaviour,
			protocol_name_by_engine: HashMap::new(),
			legacy_equiv_by_name,
			transactions_protocol,
			block_announces_protocol,
			metrics: if let Some(r) = metrics_registry {
				Some(Metrics::register(r)?)
			} else {
				None
			},
			boot_node_ids,
		};

		Ok((protocol, peerset_handle))
	}

	/// Returns the list of all the peers we have an open channel to.
	pub fn open_peers(&self) -> impl Iterator<Item = &PeerId> {
		self.behaviour.open_peers()
	}

	/// Returns true if we have a channel open with this node.
	pub fn is_open(&self, peer_id: &PeerId) -> bool {
		self.behaviour.is_open(peer_id)
	}

	/// Returns the list of all the peers that the peerset currently requests us to be connected to.
	pub fn requested_peers(&self) -> impl Iterator<Item = &PeerId> {
		self.behaviour.requested_peers()
	}

	/// Returns the number of discovered nodes that we keep in memory.
	pub fn num_discovered_peers(&self) -> usize {
		self.behaviour.num_discovered_peers()
	}

	/// Disconnects the given peer if we are connected to it.
	pub fn disconnect_peer(&mut self, peer_id: &PeerId) {
		self.behaviour.disconnect_peer(peer_id)
	}

	/// Returns true if we try to open protocols with the given peer.
	pub fn is_enabled(&self, peer_id: &PeerId) -> bool {
		self.behaviour.is_enabled(peer_id)
	}

	/// Returns the state of the peerset manager, for debugging purposes.
	pub fn peerset_debug_info(&mut self) -> serde_json::Value {
		self.behaviour.peerset_debug_info()
	}

	/// Returns the number of peers we're connected to.
	pub fn num_connected_peers(&self) -> usize {
		self.context_data.peers.values().count()
	}

	/// Returns the number of peers we're connected to and that are being queried.
	pub fn num_active_peers(&self) -> usize {
		self.context_data
			.peers
			.values()
			.filter(|p| p.block_request.is_some())
			.count()
	}

	/// Current global sync state.
	pub fn sync_state(&self) -> SyncState {
		self.sync.status().state
	}

	/// Target sync block number.
	pub fn best_seen_block(&self) -> Option<NumberFor<B>> {
		self.sync.status().best_seen_block
	}

	/// Number of peers participating in syncing.
	pub fn num_sync_peers(&self) -> u32 {
		self.sync.status().num_peers
	}

	/// Number of blocks in the import queue.
	pub fn num_queued_blocks(&self) -> u32 {
		self.sync.status().queued_blocks
	}

	/// Number of processed blocks.
	pub fn num_processed_blocks(&self) -> usize {
		self.sync.num_processed_blocks()
	}

	/// Number of active sync requests.
	pub fn num_sync_requests(&self) -> usize {
		self.sync.num_sync_requests()
	}

	fn handle_response(
		&mut self,
		who: PeerId,
		response: &message::BlockResponse<B>
	) -> Option<message::BlockRequest<B>> {
		if let Some(ref mut peer) = self.context_data.peers.get_mut(&who) {
			if let Some(_) = peer.obsolete_requests.remove(&response.id) {
				trace!(target: "sync", "Ignoring obsolete block response packet from {} ({})", who, response.id);
				return None;
			}
			// Clear the request. If the response is invalid peer will be disconnected anyway.
			let request = peer.block_request.take();
			if request.as_ref().map_or(false, |(_, r)| r.id == response.id) {
				return request.map(|(_, r)| r)
			}
			trace!(target: "sync", "Unexpected response packet from {} ({})", who, response.id);
			self.peerset_handle.report_peer(who.clone(), rep::UNEXPECTED_RESPONSE);
			self.behaviour.disconnect_peer(&who);
		}
		None
	}

	fn update_peer_info(&mut self, who: &PeerId) {
		if let Some(info) = self.sync.peer_info(who) {
			if let Some(ref mut peer) = self.context_data.peers.get_mut(who) {
				peer.info.best_hash = info.best_hash;
				peer.info.best_number = info.best_number;
			}
		}
	}

	/// Returns information about all the peers we are connected to after the handshake message.
	pub fn peers_info(&self) -> impl Iterator<Item = (&PeerId, &PeerInfo<B>)> {
		self.context_data.peers.iter().map(|(id, peer)| (id, &peer.info))
	}

	pub fn on_custom_message(
		&mut self,
		who: PeerId,
		data: BytesMut,
	) -> CustomMessageOutcome<B> {

		let message = match <Message<B> as Decode>::decode(&mut &data[..]) {
			Ok(message) => message,
			Err(err) => {
				debug!(target: "sync", "Couldn't decode packet sent by {}: {:?}: {}", who, data, err.what());
				self.peerset_handle.report_peer(who.clone(), rep::BAD_MESSAGE);
				return CustomMessageOutcome::None;
			}
		};

		let mut stats = self.context_data.stats.entry(message.id()).or_default();
		stats.bytes_in += data.len() as u64;
		stats.count_in += 1;

		match message {
			GenericMessage::Status(s) => return self.on_status_message(who, s),
			GenericMessage::BlockRequest(r) => self.on_block_request(who, r),
			GenericMessage::BlockResponse(r) => {
				if let Some(request) = self.handle_response(who.clone(), &r) {
					let outcome = self.on_block_response(who.clone(), request, r);
					self.update_peer_info(&who);
					return outcome
				}
			},
			GenericMessage::BlockAnnounce(announce) => {
				let outcome = self.on_block_announce(who.clone(), announce);
				self.update_peer_info(&who);
				return outcome;
			},
			GenericMessage::Transactions(m) =>
				self.on_extrinsics(who, m),
			GenericMessage::RemoteCallRequest(request) => self.on_remote_call_request(who, request),
			GenericMessage::RemoteCallResponse(_) =>
				warn!(target: "sub-libp2p", "Received unexpected RemoteCallResponse"),
			GenericMessage::RemoteReadRequest(request) =>
				self.on_remote_read_request(who, request),
			GenericMessage::RemoteReadResponse(_) =>
				warn!(target: "sub-libp2p", "Received unexpected RemoteReadResponse"),
			GenericMessage::RemoteHeaderRequest(request) =>
				self.on_remote_header_request(who, request),
			GenericMessage::RemoteHeaderResponse(_) =>
				warn!(target: "sub-libp2p", "Received unexpected RemoteHeaderResponse"),
			GenericMessage::RemoteChangesRequest(request) =>
				self.on_remote_changes_request(who, request),
			GenericMessage::RemoteChangesResponse(_) =>
				warn!(target: "sub-libp2p", "Received unexpected RemoteChangesResponse"),
			GenericMessage::FinalityProofRequest(request) =>
				self.on_finality_proof_request(who, request),
			GenericMessage::FinalityProofResponse(response) =>
				return self.on_finality_proof_response(who, response),
			GenericMessage::RemoteReadChildRequest(request) =>
				self.on_remote_read_child_request(who, request),
			GenericMessage::Consensus(msg) =>
				return if self.protocol_name_by_engine.contains_key(&msg.engine_id) {
					CustomMessageOutcome::NotificationsReceived {
						remote: who.clone(),
						messages: vec![(msg.engine_id, From::from(msg.data))],
					}
				} else {
					warn!(target: "sync", "Received message on non-registered protocol: {:?}", msg.engine_id);
					CustomMessageOutcome::None
				},
			GenericMessage::ConsensusBatch(messages) => {
				let messages = messages
					.into_iter()
					.filter_map(|msg| {
						if self.protocol_name_by_engine.contains_key(&msg.engine_id) {
							Some((msg.engine_id, From::from(msg.data)))
						} else {
							warn!(target: "sync", "Received message on non-registered protocol: {:?}", msg.engine_id);
							None
						}
					})
					.collect::<Vec<_>>();

				return if !messages.is_empty() {
					CustomMessageOutcome::NotificationsReceived {
						remote: who.clone(),
						messages,
					}
				} else {
					CustomMessageOutcome::None
				};
			},
		}

		CustomMessageOutcome::None
	}

	fn send_request(&mut self, who: &PeerId, message: Message<B>) {
		send_request::<B, H>(
			&mut self.behaviour,
			&mut self.context_data.stats,
			&mut self.context_data.peers,
			who,
			message,
		);
	}

	fn send_message(
		&mut self,
		who: &PeerId,
		message: Option<(Cow<'static, [u8]>, Vec<u8>)>,
		legacy: Message<B>,
	) {
		send_message::<B>(
			&mut self.behaviour,
			&mut self.context_data.stats,
			who,
			message,
			legacy,
		);
	}

	/// Called when a new peer is connected
	pub fn on_peer_connected(&mut self, who: PeerId) {
		trace!(target: "sync", "Connecting {}", who);
		self.handshaking_peers.insert(who.clone(), HandshakingPeer { timestamp: Instant::now() });
		self.send_status(who);
	}

	/// Called by peer when it is disconnecting
	pub fn on_peer_disconnected(&mut self, peer: PeerId) -> CustomMessageOutcome<B> {
		if self.important_peers.contains(&peer) {
			warn!(target: "sync", "Reserved peer {} disconnected", peer);
		} else {
			trace!(target: "sync", "{} disconnected", peer);
		}

		// lock all the the peer lists so that add/remove peer events are in order
		let removed = {
			self.handshaking_peers.remove(&peer);
			self.context_data.peers.remove(&peer)
		};
		if let Some(_peer_data) = removed {
			self.sync.peer_disconnected(peer.clone());

			// Notify all the notification protocols as closed.
			CustomMessageOutcome::NotificationStreamClosed {
				remote: peer,
				protocols: self.protocol_name_by_engine.keys().cloned().collect(),
			}
		} else {
			CustomMessageOutcome::None
		}
	}

	/// Called as a back-pressure mechanism if the networking detects that the peer cannot process
	/// our messaging rate fast enough.
	pub fn on_clogged_peer(&self, who: PeerId, _msg: Option<Message<B>>) {
		self.peerset_handle.report_peer(who.clone(), rep::CLOGGED_PEER);

		// Print some diagnostics.
		if let Some(peer) = self.context_data.peers.get(&who) {
			debug!(target: "sync", "Clogged peer {} (protocol_version: {:?}; roles: {:?}; \
				known_extrinsics: {:?}; known_blocks: {:?}; best_hash: {:?}; best_number: {:?})",
				who, peer.info.protocol_version, peer.info.roles, peer.known_extrinsics, peer.known_blocks,
				peer.info.best_hash, peer.info.best_number);
		} else {
			debug!(target: "sync", "Peer clogged before being properly connected");
		}
	}

	fn on_block_request(&mut self, peer: PeerId, request: message::BlockRequest<B>) {
		trace!(target: "sync", "BlockRequest {} from {}: from {:?} to {:?} max {:?} for {:?}",
			request.id,
			peer,
			request.from,
			request.to,
			request.max,
			request.fields,
		);

		// sending block requests to the node that is unable to serve it is considered a bad behavior
		if !self.config.roles.is_full() {
			trace!(target: "sync", "Peer {} is trying to sync from the light node", peer);
			self.behaviour.disconnect_peer(&peer);
			self.peerset_handle.report_peer(peer, rep::UNEXPECTED_REQUEST);
			return;
		}

		let mut blocks = Vec::new();
		let mut id = match request.from {
			message::FromBlock::Hash(h) => BlockId::Hash(h),
			message::FromBlock::Number(n) => BlockId::Number(n),
		};
		let max = cmp::min(request.max.unwrap_or(u32::max_value()), MAX_BLOCK_DATA_RESPONSE) as usize;
		let get_header = request.fields.contains(message::BlockAttributes::HEADER);
		let get_body = request.fields.contains(message::BlockAttributes::BODY);
		let get_justification = request
			.fields
			.contains(message::BlockAttributes::JUSTIFICATION);
		while let Some(header) = self.context_data.chain.header(id).unwrap_or(None) {
			if blocks.len() >= max {
				break;
			}
			let number = header.number().clone();
			let hash = header.hash();
			let parent_hash = header.parent_hash().clone();
			let justification = if get_justification {
				self.context_data.chain.justification(&BlockId::Hash(hash)).unwrap_or(None)
			} else {
				None
			};
			let block_data = message::generic::BlockData {
				hash: hash,
				header: if get_header { Some(header) } else { None },
				body: if get_body {
					self.context_data
						.chain
						.block_body(&BlockId::Hash(hash))
						.unwrap_or(None)
				} else {
					None
				},
				receipt: None,
				message_queue: None,
				justification,
			};
			// Stop if we don't have requested block body
			if get_body && block_data.body.is_none() {
				trace!(target: "sync", "Missing data for block request.");
				break;
			}
			blocks.push(block_data);
			match request.direction {
				message::Direction::Ascending => id = BlockId::Number(number + One::one()),
				message::Direction::Descending => {
					if number.is_zero() {
						break;
					}
					id = BlockId::Hash(parent_hash)
				}
			}
		}
		let response = message::generic::BlockResponse {
			id: request.id,
			blocks: blocks,
		};
		trace!(target: "sync", "Sending BlockResponse with {} blocks", response.blocks.len());
		self.send_message(&peer, None, GenericMessage::BlockResponse(response))
	}

	/// Adjusts the reputation of a node.
	pub fn report_peer(&self, who: PeerId, reputation: sc_peerset::ReputationChange) {
		self.peerset_handle.report_peer(who, reputation)
	}

	fn on_block_response(
		&mut self,
		peer: PeerId,
		request: message::BlockRequest<B>,
		response: message::BlockResponse<B>,
	) -> CustomMessageOutcome<B> {
		let blocks_range = || match (
			response.blocks.first().and_then(|b| b.header.as_ref().map(|h| h.number())),
			response.blocks.last().and_then(|b| b.header.as_ref().map(|h| h.number())),
		) {
			(Some(first), Some(last)) if first != last => format!(" ({}..{})", first, last),
			(Some(first), Some(_)) => format!(" ({})", first),
			_ => Default::default(),
		};
		trace!(target: "sync", "BlockResponse {} from {} with {} blocks {}",
			response.id,
			peer,
			response.blocks.len(),
			blocks_range(),
		);

		if request.fields == message::BlockAttributes::JUSTIFICATION {
			match self.sync.on_block_justification(peer, response) {
				Ok(sync::OnBlockJustification::Nothing) => CustomMessageOutcome::None,
				Ok(sync::OnBlockJustification::Import { peer, hash, number, justification }) =>
					CustomMessageOutcome::JustificationImport(peer, hash, number, justification),
				Err(sync::BadPeer(id, repu)) => {
					self.behaviour.disconnect_peer(&id);
					self.peerset_handle.report_peer(id, repu);
					CustomMessageOutcome::None
				}
			}
		} else {
			// Validate fields against the request.
			if request.fields.contains(message::BlockAttributes::HEADER) && response.blocks.iter().any(|b| b.header.is_none()) {
				self.behaviour.disconnect_peer(&peer);
				self.peerset_handle.report_peer(peer, rep::BAD_RESPONSE);
				trace!(target: "sync", "Missing header for a block");
				return CustomMessageOutcome::None
			}
			if request.fields.contains(message::BlockAttributes::BODY) && response.blocks.iter().any(|b| b.body.is_none()) {
				self.behaviour.disconnect_peer(&peer);
				self.peerset_handle.report_peer(peer, rep::BAD_RESPONSE);
				trace!(target: "sync", "Missing body for a block");
				return CustomMessageOutcome::None
			}

			match self.sync.on_block_data(peer, Some(request), response) {
				Ok(sync::OnBlockData::Import(origin, blocks)) =>
					CustomMessageOutcome::BlockImport(origin, blocks),
				Ok(sync::OnBlockData::Request(peer, req)) => {
					self.send_request(&peer, GenericMessage::BlockRequest(req));
					CustomMessageOutcome::None
				}
				Err(sync::BadPeer(id, repu)) => {
					self.behaviour.disconnect_peer(&id);
					self.peerset_handle.report_peer(id, repu);
					CustomMessageOutcome::None
				}
			}
		}
	}

	/// Perform time based maintenance.
	///
	/// > **Note**: This method normally doesn't have to be called except for testing purposes.
	pub fn tick(&mut self) {
		self.maintain_peers();
		self.report_metrics()
	}

	fn maintain_peers(&mut self) {
		let tick = Instant::now();
		let mut aborting = Vec::new();
		{
			for (who, peer) in self.context_data.peers.iter() {
				if peer.block_request.as_ref().map_or(false, |(t, _)| (tick - *t).as_secs() > REQUEST_TIMEOUT_SEC) {
					log!(
						target: "sync",
						if self.important_peers.contains(who) { Level::Warn } else { Level::Trace },
						"Request timeout {}", who
					);
					aborting.push(who.clone());
				} else if peer.obsolete_requests.values().any(|t| (tick - *t).as_secs() > REQUEST_TIMEOUT_SEC) {
					log!(
						target: "sync",
						if self.important_peers.contains(who) { Level::Warn } else { Level::Trace },
						"Obsolete timeout {}", who
					);
					aborting.push(who.clone());
				}
			}
			for (who, _) in self.handshaking_peers.iter()
				.filter(|(_, handshaking)| (tick - handshaking.timestamp).as_secs() > REQUEST_TIMEOUT_SEC)
			{
				log!(
					target: "sync",
					if self.important_peers.contains(who) { Level::Warn } else { Level::Trace },
					"Handshake timeout {}", who
				);
				aborting.push(who.clone());
			}
		}

		for p in aborting {
			self.behaviour.disconnect_peer(&p);
			self.peerset_handle.report_peer(p, rep::TIMEOUT);
		}
	}

	/// Called by peer to report status
	fn on_status_message(&mut self, who: PeerId, status: message::Status<B>) -> CustomMessageOutcome<B> {
		trace!(target: "sync", "New peer {} {:?}", who, status);
		let _protocol_version = {
			if self.context_data.peers.contains_key(&who) {
				log!(
					target: "sync",
					if self.important_peers.contains(&who) { Level::Warn } else { Level::Debug },
					"Unexpected status packet from {}", who
				);
				self.peerset_handle.report_peer(who, rep::UNEXPECTED_STATUS);
				return CustomMessageOutcome::None;
			}
			if status.genesis_hash != self.genesis_hash {
				log!(
					target: "sync",
					if self.important_peers.contains(&who) { Level::Warn } else { Level::Trace },
					"Peer is on different chain (our genesis: {} theirs: {})",
					self.genesis_hash, status.genesis_hash
				);
				self.peerset_handle.report_peer(who.clone(), rep::GENESIS_MISMATCH);
				self.behaviour.disconnect_peer(&who);

				if self.boot_node_ids.contains(&who) {
					error!(
						target: "sync",
						"Bootnode with peer id `{}` is on a different chain (our genesis: {} theirs: {})",
						who,
						self.genesis_hash,
						status.genesis_hash,
					);
				}

				return CustomMessageOutcome::None;
			}
			if status.version < MIN_VERSION && CURRENT_VERSION < status.min_supported_version {
				log!(
					target: "sync",
					if self.important_peers.contains(&who) { Level::Warn } else { Level::Trace },
					"Peer {:?} using unsupported protocol version {}", who, status.version
				);
				self.peerset_handle.report_peer(who.clone(), rep::BAD_PROTOCOL);
				self.behaviour.disconnect_peer(&who);
				return CustomMessageOutcome::None;
			}

			if self.config.roles.is_light() {
				// we're not interested in light peers
				if status.roles.is_light() {
					debug!(target: "sync", "Peer {} is unable to serve light requests", who);
					self.peerset_handle.report_peer(who.clone(), rep::BAD_ROLE);
					self.behaviour.disconnect_peer(&who);
					return CustomMessageOutcome::None;
				}

				// we don't interested in peers that are far behind us
				let self_best_block = self
					.context_data
					.chain
					.info()
					.best_number;
				let blocks_difference = self_best_block
					.checked_sub(&status.best_number)
					.unwrap_or_else(Zero::zero)
					.saturated_into::<u64>();
				if blocks_difference > LIGHT_MAXIMAL_BLOCKS_DIFFERENCE {
					debug!(target: "sync", "Peer {} is far behind us and will unable to serve light requests", who);
					self.peerset_handle.report_peer(who.clone(), rep::PEER_BEHIND_US_LIGHT);
					self.behaviour.disconnect_peer(&who);
					return CustomMessageOutcome::None;
				}
			}

			let info = match self.handshaking_peers.remove(&who) {
				Some(_handshaking) => {
					PeerInfo {
						protocol_version: status.version,
						roles: status.roles,
						best_hash: status.best_hash,
						best_number: status.best_number
					}
				},
				None => {
					error!(target: "sync", "Received status from previously unconnected node {}", who);
					return CustomMessageOutcome::None;
				},
			};

			let peer = Peer {
				info,
				block_request: None,
				known_extrinsics: LruHashSet::new(NonZeroUsize::new(MAX_KNOWN_EXTRINSICS)
					.expect("Constant is nonzero")),
				known_blocks: LruHashSet::new(NonZeroUsize::new(MAX_KNOWN_BLOCKS)
					.expect("Constant is nonzero")),
				next_request_id: 0,
				obsolete_requests: HashMap::new(),
			};
			self.context_data.peers.insert(who.clone(), peer);

			debug!(target: "sync", "Connected {}", who);
			status.version
		};

		let info = self.context_data.peers.get(&who).expect("We just inserted above; QED").info.clone();
		self.pending_messages.push_back(CustomMessageOutcome::PeerNewBest(who.clone(), status.best_number));
		if info.roles.is_full() {
			match self.sync.new_peer(who.clone(), info.best_hash, info.best_number) {
				Ok(None) => (),
				Ok(Some(req)) => self.send_request(&who, GenericMessage::BlockRequest(req)),
				Err(sync::BadPeer(id, repu)) => {
					self.behaviour.disconnect_peer(&id);
					self.peerset_handle.report_peer(id, repu)
				}
			}
		}

		// Notify all the notification protocols as open.
		CustomMessageOutcome::NotificationStreamOpened {
			remote: who,
			protocols: self.protocol_name_by_engine.keys().cloned().collect(),
			roles: info.roles,
		}
	}

	/// Send a notification to the given peer we're connected to.
	///
	/// Doesn't do anything if we don't have a notifications substream for that protocol with that
	/// peer.
	pub fn write_notification(
		&mut self,
		target: PeerId,
		engine_id: ConsensusEngineId,
		message: impl Into<Vec<u8>>,
	) {
		if let Some(protocol_name) = self.protocol_name_by_engine.get(&engine_id) {
			let message = message.into();
			let fallback = GenericMessage::<(), (), (), ()>::Consensus(ConsensusMessage {
				engine_id,
				data: message.clone(),
			}).encode();
			self.behaviour.write_notification(&target, protocol_name.clone(), message, fallback);
		} else {
			error!(
				target: "sub-libp2p",
				"Sending a notification with a protocol that wasn't registered: {:?}",
				engine_id
			);
		}
	}

	/// Registers a new notifications protocol.
	///
	/// While registering a protocol while we already have open connections is discouraged, we
	/// nonetheless handle it by notifying that we opened channels with everyone. This function
	/// returns a list of substreams to open as a result.
	pub fn register_notifications_protocol<'a>(
		&'a mut self,
		engine_id: ConsensusEngineId,
		protocol_name: impl Into<Cow<'static, [u8]>>,
	) -> impl ExactSizeIterator<Item = (&'a PeerId, Roles)> + 'a {
		let protocol_name = protocol_name.into();
		if self.protocol_name_by_engine.insert(engine_id, protocol_name.clone()).is_some() {
			error!(target: "sub-libp2p", "Notifications protocol already registered: {:?}", protocol_name);
		} else {
			self.behaviour.register_notif_protocol(protocol_name.clone(), Vec::new());
			self.legacy_equiv_by_name.insert(protocol_name, Fallback::Consensus(engine_id));
		}

		self.context_data.peers.iter()
			.map(|(peer_id, peer)| (peer_id, peer.info.roles))
	}

	/// Called when peer sends us new extrinsics
	fn on_extrinsics(
		&mut self,
		who: PeerId,
		extrinsics: message::Transactions<B::Extrinsic>
	) {
		// sending extrinsic to light node is considered a bad behavior
		if !self.config.roles.is_full() {
			trace!(target: "sync", "Peer {} is trying to send extrinsic to the light node", who);
			self.behaviour.disconnect_peer(&who);
			self.peerset_handle.report_peer(who, rep::UNEXPECTED_EXTRINSICS);
			return;
		}

		// Accept extrinsics only when fully synced
		if self.sync.status().state != SyncState::Idle {
			trace!(target: "sync", "{} Ignoring extrinsics while syncing", who);
			return;
		}
		trace!(target: "sync", "Received {} extrinsics from {}", extrinsics.len(), who);
		if let Some(ref mut peer) = self.context_data.peers.get_mut(&who) {
			for t in extrinsics {
				let hash = self.transaction_pool.hash_of(&t);
				peer.known_extrinsics.insert(hash);

				self.transaction_pool.import(
					self.peerset_handle.clone().into(),
					who.clone(),
					rep::GOOD_EXTRINSIC,
					rep::BAD_EXTRINSIC,
					t,
				);
			}
		}
	}

	/// Propagate one extrinsic.
	pub fn propagate_extrinsic(
		&mut self,
		hash: &H,
	) {
		debug!(target: "sync", "Propagating extrinsic [{:?}]", hash);
		// Accept transactions only when fully synced
		if self.sync.status().state != SyncState::Idle {
			return;
		}
		if let Some(extrinsic) = self.transaction_pool.transaction(hash) {
			let propagated_to = self.do_propagate_extrinsics(&[(hash.clone(), extrinsic)]);
			self.transaction_pool.on_broadcasted(propagated_to);
		}
	}

	fn do_propagate_extrinsics(
		&mut self,
		extrinsics: &[(H, B::Extrinsic)],
	) -> HashMap<H, Vec<String>> {
		let mut propagated_to = HashMap::new();
		for (who, peer) in self.context_data.peers.iter_mut() {
			// never send extrinsics to the light node
			if !peer.info.roles.is_full() {
				continue;
			}

			let (hashes, to_send): (Vec<_>, Vec<_>) = extrinsics
				.iter()
				.filter(|&(ref hash, _)| peer.known_extrinsics.insert(hash.clone()))
				.cloned()
				.unzip();

			if !to_send.is_empty() {
				for hash in hashes {
					propagated_to
						.entry(hash)
						.or_insert_with(Vec::new)
						.push(who.to_base58());
				}
				trace!(target: "sync", "Sending {} transactions to {}", to_send.len(), who);
				let encoded = to_send.encode();
				send_message::<B> (
					&mut self.behaviour,
					&mut self.context_data.stats,
					&who,
					Some((self.transactions_protocol.clone(), encoded)),
					GenericMessage::Transactions(to_send)
				)
			}
		}

		propagated_to
	}

	/// Call when we must propagate ready extrinsics to peers.
	pub fn propagate_extrinsics(&mut self) {
		debug!(target: "sync", "Propagating extrinsics");
		// Accept transactions only when fully synced
		if self.sync.status().state != SyncState::Idle {
			return;
		}
		let extrinsics = self.transaction_pool.transactions();
		let propagated_to = self.do_propagate_extrinsics(&extrinsics);
		self.transaction_pool.on_broadcasted(propagated_to);
	}

	/// Make sure an important block is propagated to peers.
	///
	/// In chain-based consensus, we often need to make sure non-best forks are
	/// at least temporarily synced.
	pub fn announce_block(&mut self, hash: B::Hash, data: Vec<u8>) {
		let header = match self.context_data.chain.header(BlockId::Hash(hash)) {
			Ok(Some(header)) => header,
			Ok(None) => {
				warn!("Trying to announce unknown block: {}", hash);
				return;
			}
			Err(e) => {
				warn!("Error reading block header {}: {:?}", hash, e);
				return;
			}
		};

		// don't announce genesis block since it will be ignored
		if header.number().is_zero() {
			return;
		}

		let is_best = self.context_data.chain.info().best_hash == hash;
		debug!(target: "sync", "Reannouncing block {:?}", hash);
		self.send_announcement(&header, data, is_best, true)
	}

	fn send_announcement(&mut self, header: &B::Header, data: Vec<u8>, is_best: bool, force: bool) {
		let hash = header.hash();

		for (who, ref mut peer) in self.context_data.peers.iter_mut() {
			trace!(target: "sync", "Announcing block {:?} to {}", hash, who);
			let inserted = peer.known_blocks.insert(hash);
			if inserted || force {
				let message = message::BlockAnnounce {
					header: header.clone(),
					state: if peer.info.protocol_version >= 4  {
						if is_best {
							Some(message::BlockState::Best)
						} else {
							Some(message::BlockState::Normal)
						}
					} else  {
						None
					},
					data: if peer.info.protocol_version >= 4 {
						Some(data.clone())
					} else {
						None
					},
				};

				let encoded = message.encode();

				send_message::<B> (
					&mut self.behaviour,
					&mut self.context_data.stats,
					&who,
					Some((self.block_announces_protocol.clone(), encoded)),
					Message::<B>::BlockAnnounce(message),
				)
			}
		}
	}

	/// Send Status message
	fn send_status(&mut self, who: PeerId) {
		let info = self.context_data.chain.info();
		let status = message::generic::Status {
			version: CURRENT_VERSION,
			min_supported_version: MIN_VERSION,
			genesis_hash: info.genesis_hash,
			roles: self.config.roles.into(),
			best_number: info.best_number,
			best_hash: info.best_hash,
			chain_status: Vec::new(), // TODO: find a way to make this backwards-compatible
		};

		self.send_message(&who, None, GenericMessage::Status(status))
	}

	fn on_block_announce(
		&mut self,
		who: PeerId,
		announce: BlockAnnounce<B::Header>,
	) -> CustomMessageOutcome<B> {
		let hash = announce.header.hash();
		let number = *announce.header.number();

		if let Some(ref mut peer) = self.context_data.peers.get_mut(&who) {
			peer.known_blocks.insert(hash.clone());
		}

		let is_their_best = match announce.state.unwrap_or(message::BlockState::Best) {
			message::BlockState::Best => true,
			message::BlockState::Normal => false,
		};

		match self.sync.on_block_announce(who.clone(), hash, &announce, is_their_best) {
			sync::OnBlockAnnounce::Nothing => {
				// `on_block_announce` returns `OnBlockAnnounce::ImportHeader`
				// when we have all data required to import the block
				// in the BlockAnnounce message. This is only when:
				// 1) we're on light client;
				// AND
				// 2) parent block is already imported and not pruned.
				if is_their_best {
					return CustomMessageOutcome::PeerNewBest(who, number);
				} else {
					return CustomMessageOutcome::None;
				}
			}
			sync::OnBlockAnnounce::ImportHeader => () // We proceed with the import.
		}

		// to import header from announced block let's construct response to request that normally would have
		// been sent over network (but it is not in our case)
		let blocks_to_import = self.sync.on_block_data(
			who.clone(),
			None,
			message::generic::BlockResponse {
				id: 0,
				blocks: vec![
					message::generic::BlockData {
						hash: hash,
						header: Some(announce.header),
						body: None,
						receipt: None,
						message_queue: None,
						justification: None,
					},
				],
			},
		);
		match blocks_to_import {
			Ok(sync::OnBlockData::Import(origin, blocks)) => {
				if is_their_best {
					self.pending_messages.push_back(CustomMessageOutcome::PeerNewBest(who, number));
				}
				CustomMessageOutcome::BlockImport(origin, blocks)
			},
			Ok(sync::OnBlockData::Request(peer, req)) => {
				self.send_request(&peer, GenericMessage::BlockRequest(req));
				if is_their_best {
					CustomMessageOutcome::PeerNewBest(who, number)
				} else {
					CustomMessageOutcome::None
				}
			}
			Err(sync::BadPeer(id, repu)) => {
				self.behaviour.disconnect_peer(&id);
				self.peerset_handle.report_peer(id, repu);
				if is_their_best {
					CustomMessageOutcome::PeerNewBest(who, number)
				} else {
					CustomMessageOutcome::None
				}
			}
		}
	}

	/// Call this when a block has been imported in the import queue
	pub fn on_block_imported(&mut self, header: &B::Header, is_best: bool) {
		if is_best {
			self.sync.update_chain_info(header);
		}
	}

	/// Call this when a block has been finalized. The sync layer may have some additional
	/// requesting to perform.
	pub fn on_block_finalized(&mut self, hash: B::Hash, header: &B::Header) {
		self.sync.on_block_finalized(&hash, *header.number())
	}

	fn on_remote_call_request(
		&mut self,
		who: PeerId,
		request: message::RemoteCallRequest<B::Hash>,
	) {
		trace!(target: "sync", "Remote call request {} from {} ({} at {})",
			request.id,
			who,
			request.method,
			request.block
		);
		let proof = match self.context_data.chain.execution_proof(
			&BlockId::Hash(request.block),
			&request.method,
			&request.data,
		) {
			Ok((_, proof)) => proof,
			Err(error) => {
				trace!(target: "sync", "Remote call request {} from {} ({} at {}) failed with: {}",
					request.id,
					who,
					request.method,
					request.block,
					error
				);
				self.peerset_handle.report_peer(who.clone(), rep::RPC_FAILED);
				StorageProof::empty()
			}
		};

		self.send_message(
			&who,
			None,
			GenericMessage::RemoteCallResponse(message::RemoteCallResponse {
				id: request.id,
				proof,
			}),
		);
	}

	/// Request a justification for the given block.
	///
	/// Uses `protocol` to queue a new justification request and tries to dispatch all pending
	/// requests.
	pub fn request_justification(&mut self, hash: &B::Hash, number: NumberFor<B>) {
		self.sync.request_justification(&hash, number)
	}

	/// Request syncing for the given block from given set of peers.
	/// Uses `protocol` to queue a new block download request and tries to dispatch all pending
	/// requests.
	pub fn set_sync_fork_request(&mut self, peers: Vec<PeerId>, hash: &B::Hash, number: NumberFor<B>) {
		self.sync.set_sync_fork_request(peers, hash, number)
	}

	/// A batch of blocks have been processed, with or without errors.
	/// Call this when a batch of blocks have been processed by the importqueue, with or without
	/// errors.
	pub fn blocks_processed(
		&mut self,
		imported: usize,
		count: usize,
		results: Vec<(Result<BlockImportResult<NumberFor<B>>, BlockImportError>, B::Hash)>
	) {
		let results = self.sync.on_blocks_processed(
			imported,
			count,
			results,
		);
		for result in results {
			match result {
				Ok((id, req)) => {
					let msg = GenericMessage::BlockRequest(req);
					send_request(
						&mut self.behaviour,
						&mut self.context_data.stats,
						&mut self.context_data.peers,
						&id,
						msg
					)
				}
				Err(sync::BadPeer(id, repu)) => {
					self.behaviour.disconnect_peer(&id);
					self.peerset_handle.report_peer(id, repu)
				}
			}
		}
	}

	/// Call this when a justification has been processed by the import queue, with or without
	/// errors.
	pub fn justification_import_result(&mut self, hash: B::Hash, number: NumberFor<B>, success: bool) {
		self.sync.on_justification_import(hash, number, success)
	}

	/// Request a finality proof for the given block.
	///
	/// Queues a new finality proof request and tries to dispatch all pending requests.
	pub fn request_finality_proof(&mut self, hash: &B::Hash, number: NumberFor<B>) {
		self.sync.request_finality_proof(&hash, number)
	}

	/// Notify the protocol that we have learned about the existence of nodes.
	///
	/// Can be called multiple times with the same `PeerId`s.
	pub fn add_discovered_nodes(&mut self, peer_ids: impl Iterator<Item = PeerId>) {
		self.behaviour.add_discovered_nodes(peer_ids)
	}

	pub fn finality_proof_import_result(
		&mut self,
		request_block: (B::Hash, NumberFor<B>),
		finalization_result: Result<(B::Hash, NumberFor<B>), ()>,
	) {
		self.sync.on_finality_proof_import(request_block, finalization_result)
	}

	fn on_remote_read_request(
		&mut self,
		who: PeerId,
		request: message::RemoteReadRequest<B::Hash>,
	) {
		if request.keys.is_empty() {
			debug!(target: "sync", "Invalid remote read request sent by {}", who);
			self.behaviour.disconnect_peer(&who);
			self.peerset_handle.report_peer(who, rep::BAD_MESSAGE);
			return;
		}

		let keys_str = || match request.keys.len() {
			1 => HexDisplay::from(&request.keys[0]).to_string(),
			_ => format!(
				"{}..{}",
				HexDisplay::from(&request.keys[0]),
				HexDisplay::from(&request.keys[request.keys.len() - 1]),
			),
		};

		trace!(target: "sync", "Remote read request {} from {} ({} at {})",
			request.id, who, keys_str(), request.block);
		let proof = match self.context_data.chain.read_proof(
			&BlockId::Hash(request.block),
			&mut request.keys.iter().map(AsRef::as_ref)
		) {
			Ok(proof) => proof,
			Err(error) => {
				trace!(target: "sync", "Remote read request {} from {} ({} at {}) failed with: {}",
					request.id,
					who,
					keys_str(),
					request.block,
					error
				);
				StorageProof::empty()
			}
		};
		self.send_message(
			&who,
			None,
			GenericMessage::RemoteReadResponse(message::RemoteReadResponse {
				id: request.id,
				proof,
			}),
		);
	}

	fn on_remote_read_child_request(
		&mut self,
		who: PeerId,
		request: message::RemoteReadChildRequest<B::Hash>,
	) {
		if request.keys.is_empty() {
			debug!(target: "sync", "Invalid remote child read request sent by {}", who);
			self.behaviour.disconnect_peer(&who);
			self.peerset_handle.report_peer(who, rep::BAD_MESSAGE);
			return;
		}

		let keys_str = || match request.keys.len() {
			1 => HexDisplay::from(&request.keys[0]).to_string(),
			_ => format!(
				"{}..{}",
				HexDisplay::from(&request.keys[0]),
				HexDisplay::from(&request.keys[request.keys.len() - 1]),
			),
		};

		trace!(target: "sync", "Remote read child request {} from {} ({} {} at {})",
			request.id, who, HexDisplay::from(&request.storage_key), keys_str(), request.block);
		let proof = if let Some(child_info) = ChildInfo::resolve_child_info(request.child_type, &request.child_info[..]) {
			match self.context_data.chain.read_child_proof(
				&BlockId::Hash(request.block),
				&request.storage_key,
				child_info,
				&mut request.keys.iter().map(AsRef::as_ref),
			) {
				Ok(proof) => proof,
				Err(error) => {
					trace!(target: "sync", "Remote read child request {} from {} ({} {} at {}) failed with: {}",
						request.id,
						who,
						HexDisplay::from(&request.storage_key),
						keys_str(),
						request.block,
						error
					);
					StorageProof::empty()
				}
			}
		} else {
			trace!(target: "sync", "Remote read child request {} from {} ({} {} at {}) failed with: {}",
				request.id,
				who,
				HexDisplay::from(&request.storage_key),
				keys_str(),
				request.block,
				"invalid child info and type",
			);

			StorageProof::empty()
		};
		self.send_message(
			&who,
			None,
			GenericMessage::RemoteReadResponse(message::RemoteReadResponse {
				id: request.id,
				proof,
			}),
		);
	}

	fn on_remote_header_request(
		&mut self,
		who: PeerId,
		request: message::RemoteHeaderRequest<NumberFor<B>>,
	) {
		trace!(target: "sync", "Remote header proof request {} from {} ({})",
			request.id, who, request.block);
		let (header, proof) = match self.context_data.chain.header_proof(&BlockId::Number(request.block)) {
			Ok((header, proof)) => (Some(header), proof),
			Err(error) => {
				trace!(target: "sync", "Remote header proof request {} from {} ({}) failed with: {}",
					request.id,
					who,
					request.block,
					error
				);
				(Default::default(), StorageProof::empty())
			}
		};
		self.send_message(
			&who,
			None,
			GenericMessage::RemoteHeaderResponse(message::RemoteHeaderResponse {
				id: request.id,
				header,
				proof,
			}),
		);
	}

	fn on_remote_changes_request(
		&mut self,
		who: PeerId,
		request: message::RemoteChangesRequest<B::Hash>,
	) {
		trace!(target: "sync", "Remote changes proof request {} from {} for key {} ({}..{})",
			request.id,
			who,
			if let Some(sk) = request.storage_key.as_ref() {
				format!("{} : {}", HexDisplay::from(sk), HexDisplay::from(&request.key))
			} else {
				HexDisplay::from(&request.key).to_string()
			},
			request.first,
			request.last
		);
		let storage_key = request.storage_key.map(|sk| StorageKey(sk));
		let key = StorageKey(request.key);
		let proof = match self.context_data.chain.key_changes_proof(
			request.first,
			request.last,
			request.min,
			request.max,
			storage_key.as_ref(),
			&key,
		) {
			Ok(proof) => proof,
			Err(error) => {
				trace!(target: "sync", "Remote changes proof request {} from {} for key {} ({}..{}) failed with: {}",
					request.id,
					who,
					if let Some(sk) = storage_key {
						format!("{} : {}", HexDisplay::from(&sk.0), HexDisplay::from(&key.0))
					} else {
						HexDisplay::from(&key.0).to_string()
					},
					request.first,
					request.last,
					error
				);
				ChangesProof::<B::Header> {
					max_block: Zero::zero(),
					proof: vec![],
					roots: BTreeMap::new(),
					roots_proof: StorageProof::empty(),
				}
			}
		};
		self.send_message(
			&who,
			None,
			GenericMessage::RemoteChangesResponse(message::RemoteChangesResponse {
				id: request.id,
				max: proof.max_block,
				proof: proof.proof,
				roots: proof.roots.into_iter().collect(),
				roots_proof: proof.roots_proof,
			}),
		);
	}

	fn on_finality_proof_request(
		&mut self,
		who: PeerId,
		request: message::FinalityProofRequest<B::Hash>,
	) {
		trace!(target: "sync", "Finality proof request from {} for {}", who, request.block);
		let finality_proof = self.finality_proof_provider.as_ref()
			.ok_or_else(|| String::from("Finality provider is not configured"))
			.and_then(|provider|
				provider.prove_finality(request.block, &request.request).map_err(|e| e.to_string())
			);
		let finality_proof = match finality_proof {
			Ok(finality_proof) => finality_proof,
			Err(error) => {
				trace!(target: "sync", "Finality proof request from {} for {} failed with: {}",
					who,
					request.block,
					error
				);
				None
			},
		};
		self.send_message(
			&who,
			None,
			GenericMessage::FinalityProofResponse(message::FinalityProofResponse {
				id: 0,
				block: request.block,
				proof: finality_proof,
			}),
		);
	}

	fn on_finality_proof_response(
		&mut self,
		who: PeerId,
		response: message::FinalityProofResponse<B::Hash>,
	) -> CustomMessageOutcome<B> {
		trace!(target: "sync", "Finality proof response from {} for {}", who, response.block);
		match self.sync.on_block_finality_proof(who, response) {
			Ok(sync::OnBlockFinalityProof::Nothing) => CustomMessageOutcome::None,
			Ok(sync::OnBlockFinalityProof::Import { peer, hash, number, proof }) =>
				CustomMessageOutcome::FinalityProofImport(peer, hash, number, proof),
			Err(sync::BadPeer(id, repu)) => {
				self.behaviour.disconnect_peer(&id);
				self.peerset_handle.report_peer(id, repu);
				CustomMessageOutcome::None
			}
		}
	}

	fn format_stats(&self) -> String {
		let mut out = String::new();
		for (id, stats) in &self.context_data.stats {
			let _ = writeln!(
				&mut out,
				"{}: In: {} bytes ({}), Out: {} bytes ({})",
				id,
				stats.bytes_in,
				stats.count_in,
				stats.bytes_out,
				stats.count_out,
			);
		}
		out
	}

	fn report_metrics(&self) {
		use std::convert::TryInto;

		if let Some(metrics) = &self.metrics {
			let mut obsolete_requests: u64 = 0;
			for peer in self.context_data.peers.values() {
				let n = peer.obsolete_requests.len().try_into().unwrap_or(std::u64::MAX);
				obsolete_requests = obsolete_requests.saturating_add(n);
			}
			metrics.obsolete_requests.set(obsolete_requests);

			let n = self.handshaking_peers.len().try_into().unwrap_or(std::u64::MAX);
			metrics.handshaking_peers.set(n);

			let n = self.context_data.peers.len().try_into().unwrap_or(std::u64::MAX);
			metrics.peers.set(n);

			let m = self.sync.metrics();

			metrics.fork_targets.set(m.fork_targets.into());
			metrics.queued_blocks.set(m.queued_blocks.into());

			metrics.justifications.with_label_values(&["pending"])
				.set(m.justifications.pending_requests.into());
			metrics.justifications.with_label_values(&["active"])
				.set(m.justifications.active_requests.into());
			metrics.justifications.with_label_values(&["failed"])
				.set(m.justifications.failed_requests.into());
			metrics.justifications.with_label_values(&["importing"])
				.set(m.justifications.importing_requests.into());

			metrics.finality_proofs.with_label_values(&["pending"])
				.set(m.finality_proofs.pending_requests.into());
			metrics.finality_proofs.with_label_values(&["active"])
				.set(m.finality_proofs.active_requests.into());
			metrics.finality_proofs.with_label_values(&["failed"])
				.set(m.finality_proofs.failed_requests.into());
			metrics.finality_proofs.with_label_values(&["importing"])
				.set(m.finality_proofs.importing_requests.into());
		}
	}
}

/// Outcome of an incoming custom message.
#[derive(Debug)]
pub enum CustomMessageOutcome<B: BlockT> {
	BlockImport(BlockOrigin, Vec<IncomingBlock<B>>),
	JustificationImport(Origin, B::Hash, NumberFor<B>, Justification),
	FinalityProofImport(Origin, B::Hash, NumberFor<B>, Vec<u8>),
	/// Notification protocols have been opened with a remote.
	NotificationStreamOpened { remote: PeerId, protocols: Vec<ConsensusEngineId>, roles: Roles },
	/// Notification protocols have been closed with a remote.
	NotificationStreamClosed { remote: PeerId, protocols: Vec<ConsensusEngineId> },
	/// Messages have been received on one or more notifications protocols.
	NotificationsReceived { remote: PeerId, messages: Vec<(ConsensusEngineId, Bytes)> },
	/// Peer has a reported a new head of chain.
	PeerNewBest(PeerId, NumberFor<B>),
	None,
}

fn send_request<B: BlockT, H: ExHashT>(
	behaviour: &mut GenericProto,
	stats: &mut HashMap<&'static str, PacketStats>,
	peers: &mut HashMap<PeerId, Peer<B, H>>,
	who: &PeerId,
	mut message: Message<B>,
) {
	if let GenericMessage::BlockRequest(ref mut r) = message {
		if let Some(ref mut peer) = peers.get_mut(who) {
			r.id = peer.next_request_id;
			peer.next_request_id = peer.next_request_id + 1;
			if let Some((timestamp, request)) = peer.block_request.take() {
				trace!(target: "sync", "Request {} for {} is now obsolete.", request.id, who);
				peer.obsolete_requests.insert(request.id, timestamp);
			}
			peer.block_request = Some((Instant::now(), r.clone()));
		}
	}
	send_message::<B>(behaviour, stats, who, None, message)
}

fn send_message<B: BlockT>(
	behaviour: &mut GenericProto,
	stats: &mut HashMap<&'static str, PacketStats>,
	who: &PeerId,
	message: Option<(Cow<'static, [u8]>, Vec<u8>)>,
	legacy_message: Message<B>,
) {
	let encoded = legacy_message.encode();
	let mut stats = stats.entry(legacy_message.id()).or_default();
	stats.bytes_out += encoded.len() as u64;
	stats.count_out += 1;
	if let Some((proto, msg)) = message {
		behaviour.write_notification(who, proto, msg, encoded);
	} else {
		behaviour.send_packet(who, encoded);
	}
}

impl<B: BlockT, H: ExHashT> NetworkBehaviour for Protocol<B, H> {
	type ProtocolsHandler = <GenericProto as NetworkBehaviour>::ProtocolsHandler;
	type OutEvent = CustomMessageOutcome<B>;

	fn new_handler(&mut self) -> Self::ProtocolsHandler {
		self.behaviour.new_handler()
	}

	fn addresses_of_peer(&mut self, peer_id: &PeerId) -> Vec<Multiaddr> {
		self.behaviour.addresses_of_peer(peer_id)
	}

	fn inject_connection_established(&mut self, peer_id: &PeerId, conn: &ConnectionId, endpoint: &ConnectedPoint) {
		self.behaviour.inject_connection_established(peer_id, conn, endpoint)
	}

	fn inject_connection_closed(&mut self, peer_id: &PeerId, conn: &ConnectionId, endpoint: &ConnectedPoint) {
		self.behaviour.inject_connection_closed(peer_id, conn, endpoint)
	}

	fn inject_connected(&mut self, peer_id: &PeerId) {
		self.behaviour.inject_connected(peer_id)
	}

	fn inject_disconnected(&mut self, peer_id: &PeerId) {
		self.behaviour.inject_disconnected(peer_id)
	}

	fn inject_event(
		&mut self,
		peer_id: PeerId,
		connection: ConnectionId,
		event: <<Self::ProtocolsHandler as IntoProtocolsHandler>::Handler as ProtocolsHandler>::OutEvent,
	) {
		self.behaviour.inject_event(peer_id, connection, event)
	}

	fn poll(
		&mut self,
		cx: &mut std::task::Context,
		params: &mut impl PollParameters,
	) -> Poll<
		NetworkBehaviourAction<
			<<Self::ProtocolsHandler as IntoProtocolsHandler>::Handler as ProtocolsHandler>::InEvent,
			Self::OutEvent
		>
	> {
		if let Some(message) = self.pending_messages.pop_front() {
			return Poll::Ready(NetworkBehaviourAction::GenerateEvent(message));
		}

		while let Poll::Ready(Some(())) = self.tick_timeout.poll_next_unpin(cx) {
			self.tick();
		}

		while let Poll::Ready(Some(())) = self.propagate_timeout.poll_next_unpin(cx) {
			self.propagate_extrinsics();
		}

		for (id, r) in self.sync.block_requests() {
			send_request(
				&mut self.behaviour,
				&mut self.context_data.stats,
				&mut self.context_data.peers,
				&id,
				GenericMessage::BlockRequest(r)
			)
		}
		for (id, r) in self.sync.justification_requests() {
			send_request(
				&mut self.behaviour,
				&mut self.context_data.stats,
				&mut self.context_data.peers,
				&id,
				GenericMessage::BlockRequest(r)
			)
		}
		for (id, r) in self.sync.finality_proof_requests() {
			send_request(
				&mut self.behaviour,
				&mut self.context_data.stats,
				&mut self.context_data.peers,
				&id,
				GenericMessage::FinalityProofRequest(r))
		}

		let event = match self.behaviour.poll(cx, params) {
			Poll::Pending => return Poll::Pending,
			Poll::Ready(NetworkBehaviourAction::GenerateEvent(ev)) => ev,
			Poll::Ready(NetworkBehaviourAction::DialAddress { address }) =>
				return Poll::Ready(NetworkBehaviourAction::DialAddress { address }),
			Poll::Ready(NetworkBehaviourAction::DialPeer { peer_id, condition }) =>
				return Poll::Ready(NetworkBehaviourAction::DialPeer { peer_id, condition }),
			Poll::Ready(NetworkBehaviourAction::NotifyHandler { peer_id, handler, event }) =>
				return Poll::Ready(NetworkBehaviourAction::NotifyHandler { peer_id, handler, event }),
			Poll::Ready(NetworkBehaviourAction::ReportObservedAddr { address }) =>
				return Poll::Ready(NetworkBehaviourAction::ReportObservedAddr { address }),
		};

		let outcome = match event {
			GenericProtoOut::CustomProtocolOpen { peer_id, .. } => {
				self.on_peer_connected(peer_id.clone());
				CustomMessageOutcome::None
			}
			GenericProtoOut::CustomProtocolClosed { peer_id, .. } => {
				self.on_peer_disconnected(peer_id.clone())
			},
			GenericProtoOut::LegacyMessage { peer_id, message } =>
				self.on_custom_message(peer_id, message),
			GenericProtoOut::Notification { peer_id, protocol_name, message } =>
				match self.legacy_equiv_by_name.get(&protocol_name) {
					Some(Fallback::Consensus(engine_id)) => {
						CustomMessageOutcome::NotificationsReceived {
							remote: peer_id,
							messages: vec![(*engine_id, message.freeze())],
						}
					}
					Some(Fallback::Transactions) => {
						if let Ok(m) = message::Transactions::decode(&mut message.as_ref()) {
							self.on_extrinsics(peer_id, m);
						} else {
							warn!(target: "sub-libp2p", "Failed to decode transactions list");
						}
						CustomMessageOutcome::None
					}
					Some(Fallback::BlockAnnounce) => {
						if let Ok(announce) = message::BlockAnnounce::decode(&mut message.as_ref()) {
							let outcome = self.on_block_announce(peer_id.clone(), announce);
							self.update_peer_info(&peer_id);
							outcome
						} else {
							warn!(target: "sub-libp2p", "Failed to decode block announce");
							CustomMessageOutcome::None
						}
					}
					None => {
						error!(target: "sub-libp2p", "Received notification from unknown protocol {:?}", protocol_name);
						CustomMessageOutcome::None
					}
				}
			GenericProtoOut::Clogged { peer_id, messages } => {
				debug!(target: "sync", "{} clogging messages:", messages.len());
				for msg in messages.into_iter().take(5) {
					let message: Option<Message<B>> = Decode::decode(&mut &msg[..]).ok();
					debug!(target: "sync", "{:?}", message);
					self.on_clogged_peer(peer_id.clone(), message);
				}
				CustomMessageOutcome::None
			}
		};

		if let CustomMessageOutcome::None = outcome {
			Poll::Pending
		} else {
			Poll::Ready(NetworkBehaviourAction::GenerateEvent(outcome))
		}
	}

	fn inject_addr_reach_failure(
		&mut self,
		peer_id: Option<&PeerId>,
		addr: &Multiaddr,
		error: &dyn std::error::Error
	) {
		self.behaviour.inject_addr_reach_failure(peer_id, addr, error)
	}

	fn inject_dial_failure(&mut self, peer_id: &PeerId) {
		self.behaviour.inject_dial_failure(peer_id)
	}

	fn inject_new_listen_addr(&mut self, addr: &Multiaddr) {
		self.behaviour.inject_new_listen_addr(addr)
	}

	fn inject_expired_listen_addr(&mut self, addr: &Multiaddr) {
		self.behaviour.inject_expired_listen_addr(addr)
	}

	fn inject_new_external_addr(&mut self, addr: &Multiaddr) {
		self.behaviour.inject_new_external_addr(addr)
	}

	fn inject_listener_error(&mut self, id: ListenerId, err: &(dyn std::error::Error + 'static)) {
		self.behaviour.inject_listener_error(id, err);
	}

	fn inject_listener_closed(&mut self, id: ListenerId, reason: Result<(), &io::Error>) {
		self.behaviour.inject_listener_closed(id, reason);
	}
}

impl<B: BlockT, H: ExHashT> Drop for Protocol<B, H> {
	fn drop(&mut self) {
		debug!(target: "sync", "Network stats:\n{}", self.format_stats());
	}
}

#[cfg(test)]
mod tests {
	use crate::PeerId;
	use crate::config::EmptyTransactionPool;
	use super::{CustomMessageOutcome, Protocol, ProtocolConfig};

	use sp_consensus::block_validation::DefaultBlockAnnounceValidator;
	use std::sync::Arc;
	use substrate_test_runtime_client::{TestClientBuilder, TestClientBuilderExt};
	use substrate_test_runtime_client::runtime::{Block, Hash};

	#[test]
	fn no_handshake_no_notif_closed() {
		let client = Arc::new(TestClientBuilder::with_default_backend().build_with_longest_chain().0);

		let (mut protocol, _) = Protocol::<Block, Hash>::new(
			ProtocolConfig::default(),
			client.clone(),
			Arc::new(EmptyTransactionPool),
			None,
			None,
			From::from(&b"test"[..]),
			sc_peerset::PeersetConfig {
				in_peers: 10,
				out_peers: 10,
				bootnodes: Vec::new(),
				reserved_only: false,
				priority_groups: Vec::new(),
			},
			Box::new(DefaultBlockAnnounceValidator::new(client.clone())),
			None,
			Default::default(),
			None,
		).unwrap();

		let dummy_peer_id = PeerId::random();
		let _ = protocol.on_peer_connected(dummy_peer_id.clone());
		match protocol.on_peer_disconnected(dummy_peer_id) {
			CustomMessageOutcome::None => {},
			_ => panic!()
		};
	}
}
