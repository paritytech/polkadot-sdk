// Copyright 2017-2019 Parity Technologies (UK) Ltd.
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

use futures::prelude::*;
use network_libp2p::PeerId;
use primitives::storage::StorageKey;
use consensus::{import_queue::IncomingBlock, import_queue::Origin, BlockOrigin};
use runtime_primitives::{generic::BlockId, ConsensusEngineId, Justification};
use runtime_primitives::traits::{As, Block as BlockT, Header as HeaderT, NumberFor, Zero};
use consensus::import_queue::SharedFinalityProofRequestBuilder;
use crate::message::{
	self, BlockRequest as BlockRequestMessage,
	FinalityProofRequest as FinalityProofRequestMessage, Message,
};
use crate::message::generic::{Message as GenericMessage, ConsensusMessage};
use crate::consensus_gossip::{ConsensusGossip, MessageRecipient as GossipMessageRecipient};
use crate::on_demand::OnDemandService;
use crate::specialization::NetworkSpecialization;
use crate::sync::{ChainSync, Context as SyncContext, Status as SyncStatus, SyncState};
use crate::service::{NetworkChan, NetworkMsg, TransactionPool, ExHashT};
use crate::config::{ProtocolConfig, Roles};
use parking_lot::RwLock;
use rustc_hex::ToHex;
use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;
use std::{cmp, num::NonZeroUsize, time};
use log::{trace, debug, warn, error};
use crate::chain::{Client, FinalityProofProvider};
use client::light::fetcher::ChangesProof;
use crate::{error, util::LruHashSet};

const REQUEST_TIMEOUT_SEC: u64 = 40;
/// Interval at which we perform time based maintenance
const TICK_TIMEOUT: time::Duration = time::Duration::from_millis(1100);
/// Interval at which we propagate exstrinsics;
const PROPAGATE_TIMEOUT: time::Duration = time::Duration::from_millis(2900);

/// Current protocol version.
pub(crate) const CURRENT_VERSION: u32 = 3;
/// Lowest version we support
pub(crate) const MIN_VERSION: u32 = 2;

// Maximum allowed entries in `BlockResponse`
const MAX_BLOCK_DATA_RESPONSE: u32 = 128;
/// When light node connects to the full node and the full node is behind light node
/// for at least `LIGHT_MAXIMAL_BLOCKS_DIFFERENCE` blocks, we consider it unuseful
/// and disconnect to free connection slot.
const LIGHT_MAXIMAL_BLOCKS_DIFFERENCE: u64 = 8192;

/// Reputation change when a peer is "clogged", meaning that it's not fast enough to process our
/// messages.
const CLOGGED_PEER_REPUTATION_CHANGE: i32 = -(1 << 12);
/// Reputation change when a peer doesn't respond in time to our messages.
const TIMEOUT_REPUTATION_CHANGE: i32 = -(1 << 10);
/// Reputation change when a peer sends us a status message while we already received one.
const UNEXPECTED_STATUS_REPUTATION_CHANGE: i32 = -(1 << 20);
/// Reputation change when we are a light client and a peer is behind us.
const PEER_BEHIND_US_LIGHT_REPUTATION_CHANGE: i32 = -(1 << 8);
/// Reputation change when a peer sends us an extrinsic that we didn't know about.
const NEW_EXTRINSIC_REPUTATION_CHANGE: i32 = 1 << 7;
/// We sent an RPC query to the given node, but it failed.
const RPC_FAILED_REPUTATION_CHANGE: i32 = -(1 << 12);

// Lock must always be taken in order declared here.
pub struct Protocol<B: BlockT, S: NetworkSpecialization<B>, H: ExHashT> {
	network_chan: NetworkChan<B>,
	/// Interval at which we call `tick`.
	tick_timeout: tokio::timer::Interval,
	/// Interval at which we call `propagate_extrinsics`.
	propagate_timeout: tokio::timer::Interval,
	config: ProtocolConfig,
	on_demand: Option<Arc<OnDemandService<B>>>,
	genesis_hash: B::Hash,
	sync: ChainSync<B>,
	specialization: S,
	consensus_gossip: ConsensusGossip<B>,
	context_data: ContextData<B, H>,
	// Connected peers pending Status message.
	handshaking_peers: HashMap<PeerId, HandshakingPeer>,
	// Connected peers from whom we received a Status message,
	// similar to context_data.peers but shared with the SyncProvider.
	connected_peers: Arc<RwLock<HashMap<PeerId, ConnectedPeer<B>>>>,
	transaction_pool: Arc<TransactionPool<H, B>>,
}

/// A peer from whom we have received a Status message.
#[derive(Clone)]
pub struct ConnectedPeer<B: BlockT> {
	pub peer_info: PeerInfo<B>
}

/// A peer that we are connected to
/// and from whom we have not yet received a Status message.
struct HandshakingPeer {
	timestamp: time::Instant,
}

/// Syncing status and statistics
#[derive(Clone)]
pub struct ProtocolStatus<B: BlockT> {
	/// Sync status.
	pub sync: SyncStatus<B>,
	/// Total number of connected peers
	pub num_peers: usize,
	/// Total number of active peers.
	pub num_active_peers: usize,
}

/// Peer information
#[derive(Debug)]
struct Peer<B: BlockT, H: ExHashT> {
	info: PeerInfo<B>,
	/// Current block request, if any.
	block_request: Option<(time::Instant, message::BlockRequest<B>)>,
	/// Requests we are no longer insterested in.
	obsolete_requests: HashMap<message::RequestId, time::Instant>,
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

/// Context for a network-specific handler.
pub trait Context<B: BlockT> {
	/// Adjusts the reputation of the peer. Use this to point out that a peer has been malign or
	/// irresponsible or appeared lazy.
	fn report_peer(&mut self, who: PeerId, reputation: i32);

	/// Force disconnecting from a peer. Use this when a peer misbehaved.
	fn disconnect_peer(&mut self, who: PeerId);

	/// Send a consensus message to a peer.
	fn send_consensus(&mut self, who: PeerId, consensus: ConsensusMessage);

	/// Send a chain-specific message to a peer.
	fn send_chain_specific(&mut self, who: PeerId, message: Vec<u8>);
}

/// Protocol context.
struct ProtocolContext<'a, B: 'a + BlockT, H: 'a + ExHashT> {
	network_chan: &'a NetworkChan<B>,
	context_data: &'a mut ContextData<B, H>,
}

impl<'a, B: BlockT + 'a, H: 'a + ExHashT> ProtocolContext<'a, B, H> {
	fn new(context_data: &'a mut ContextData<B, H>, network_chan: &'a NetworkChan<B>) -> Self {
		ProtocolContext { network_chan, context_data }
	}
}

impl<'a, B: BlockT + 'a, H: ExHashT + 'a> Context<B> for ProtocolContext<'a, B, H> {
	fn report_peer(&mut self, who: PeerId, reputation: i32) {
		self.network_chan.send(NetworkMsg::ReportPeer(who, reputation))
	}

	fn disconnect_peer(&mut self, who: PeerId) {
		self.network_chan.send(NetworkMsg::DisconnectPeer(who))
	}

	fn send_consensus(&mut self, who: PeerId, consensus: ConsensusMessage) {
		send_message(
			&mut self.context_data.peers,
			&self.network_chan,
			who,
			GenericMessage::Consensus(consensus)
		)
	}

	fn send_chain_specific(&mut self, who: PeerId, message: Vec<u8>) {
		send_message(
			&mut self.context_data.peers,
			&self.network_chan,
			who,
			GenericMessage::ChainSpecific(message)
		)
	}
}

impl<'a, B: BlockT + 'a, H: ExHashT + 'a> SyncContext<B> for ProtocolContext<'a, B, H> {
	fn report_peer(&mut self, who: PeerId, reputation: i32) {
		self.network_chan.send(NetworkMsg::ReportPeer(who, reputation))
	}

	fn disconnect_peer(&mut self, who: PeerId) {
		self.network_chan.send(NetworkMsg::DisconnectPeer(who))
	}

	fn peer_info(&self, who: &PeerId) -> Option<PeerInfo<B>> {
		self.context_data.peers.get(who).map(|p| p.info.clone())
	}

	fn client(&self) -> &Client<B> {
		&*self.context_data.chain
	}

	fn send_finality_proof_request(&mut self, who: PeerId, request: FinalityProofRequestMessage<B::Hash>) {
		send_message(
			&mut self.context_data.peers,
			&self.network_chan,
			who,
			GenericMessage::FinalityProofRequest(request)
		)
	}

	fn send_block_request(&mut self, who: PeerId, request: BlockRequestMessage<B>) {
		send_message(
			&mut self.context_data.peers,
			&self.network_chan,
			who,
			GenericMessage::BlockRequest(request)
		)
	}
}

/// Data necessary to create a context.
struct ContextData<B: BlockT, H: ExHashT> {
	// All connected peers
	peers: HashMap<PeerId, Peer<B, H>>,
	pub chain: Arc<Client<B>>,
	pub finality_proof_provider: Option<Arc<FinalityProofProvider<B>>>,
}

impl<B: BlockT, S: NetworkSpecialization<B>, H: ExHashT> Protocol<B, S, H> {
	/// Create a new instance.
	pub fn new(
		connected_peers: Arc<RwLock<HashMap<PeerId, ConnectedPeer<B>>>>,
		network_chan: NetworkChan<B>,
		config: ProtocolConfig,
		chain: Arc<Client<B>>,
		finality_proof_provider: Option<Arc<FinalityProofProvider<B>>>,
		on_demand: Option<Arc<OnDemandService<B>>>,
		transaction_pool: Arc<TransactionPool<H, B>>,
		specialization: S,
	) -> error::Result<Protocol<B, S, H>> {
		let info = chain.info()?;
		let sync = ChainSync::new(config.roles, &info);
		Ok(Protocol {
			network_chan,
			tick_timeout: tokio::timer::Interval::new_interval(TICK_TIMEOUT),
			propagate_timeout: tokio::timer::Interval::new_interval(PROPAGATE_TIMEOUT),
			config: config,
			context_data: ContextData {
				peers: HashMap::new(),
				chain,
				finality_proof_provider,
			},
			on_demand,
			genesis_hash: info.chain.genesis_hash,
			sync,
			specialization: specialization,
			consensus_gossip: ConsensusGossip::new(),
			handshaking_peers: HashMap::new(),
			connected_peers,
			transaction_pool: transaction_pool,
		})
	}

	/// Returns an object representing the status of the protocol.
	pub fn status(&self) -> ProtocolStatus<B> {
		ProtocolStatus {
			sync: self.sync.status(),
			num_peers: self.context_data.peers.values().count(),
			num_active_peers: self
				.context_data
				.peers
				.values()
				.filter(|p| p.block_request.is_some())
				.count(),
		}
	}

	pub fn is_major_syncing(&self) -> bool {
		self.sync.status().is_major_syncing()
	}

	pub fn is_offline(&self) -> bool {
		self.sync.status().is_offline()
	}
}

impl<B: BlockT, S: NetworkSpecialization<B>, H: ExHashT> Future for Protocol<B, S, H> {
	type Item = ();
	type Error = void::Void;

	fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
		while let Ok(Async::Ready(_)) = self.tick_timeout.poll() {
			self.tick();
		}

		while let Ok(Async::Ready(_)) = self.propagate_timeout.poll() {
			self.propagate_extrinsics();
		}

		Ok(Async::NotReady)
	}
}

impl<B: BlockT, S: NetworkSpecialization<B>, H: ExHashT> Protocol<B, S, H> {
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
			self.network_chan.send(NetworkMsg::ReportPeer(who.clone(), i32::min_value()));
			self.network_chan.send(NetworkMsg::DisconnectPeer(who));
		}
		None
	}

	fn update_peer_info(&mut self, who: &PeerId) {
		if let Some(info) = self.sync.peer_info(who) {
			if let Some(ref mut peer) = self.context_data.peers.get_mut(who) {
				peer.info.best_hash = info.best_hash;
				peer.info.best_number = info.best_number;
			}
			let mut peers = self.connected_peers.write();
			if let Some(ref mut peer) = peers.get_mut(who) {
				peer.peer_info.best_hash = info.best_hash;
				peer.peer_info.best_number = info.best_number;
			}
		}
	}

	pub fn on_custom_message(&mut self, who: PeerId, message: Message<B>) -> CustomMessageOutcome<B> {
		match message {
			GenericMessage::Status(s) => self.on_status_message(who, s),
			GenericMessage::BlockRequest(r) => self.on_block_request(who, r),
			GenericMessage::BlockResponse(r) => {
				if let Some(request) = self.handle_response(who.clone(), &r) {
					let outcome = self.on_block_response(who.clone(), request, r);
					self.update_peer_info(&who);
					return outcome
				}
			},
			GenericMessage::BlockAnnounce(announce) => {
				self.on_block_announce(who.clone(), announce);
				self.update_peer_info(&who);
			},
			GenericMessage::Transactions(m) => self.on_extrinsics(who, m),
			GenericMessage::RemoteCallRequest(request) => self.on_remote_call_request(who, request),
			GenericMessage::RemoteCallResponse(response) => self.on_remote_call_response(who, response),
			GenericMessage::RemoteReadRequest(request) => self.on_remote_read_request(who, request),
			GenericMessage::RemoteReadResponse(response) => self.on_remote_read_response(who, response),
			GenericMessage::RemoteHeaderRequest(request) => self.on_remote_header_request(who, request),
			GenericMessage::RemoteHeaderResponse(response) => self.on_remote_header_response(who, response),
			GenericMessage::RemoteChangesRequest(request) => self.on_remote_changes_request(who, request),
			GenericMessage::RemoteChangesResponse(response) => self.on_remote_changes_response(who, response),
			GenericMessage::FinalityProofRequest(request) => self.on_finality_proof_request(who, request),
			GenericMessage::FinalityProofResponse(response) => return self.on_finality_proof_response(who, response),
			GenericMessage::Consensus(msg) => {
				if self.context_data.peers.get(&who).map_or(false, |peer| peer.info.protocol_version > 2) {
					self.consensus_gossip.on_incoming(
						&mut ProtocolContext::new(&mut self.context_data, &self.network_chan),
						who,
						msg,
					);
				}
			}
			other => self.specialization.on_message(
				&mut ProtocolContext::new(&mut self.context_data, &self.network_chan),
				who,
				&mut Some(other),
			),
		}

		CustomMessageOutcome::None
	}

	fn send_message(&mut self, who: PeerId, message: Message<B>) {
		send_message::<B, H>(
			&mut self.context_data.peers,
			&self.network_chan,
			who,
			message,
		);
	}

	/// Locks `self` and returns a context plus the `ConsensusGossip` struct.
	pub fn consensus_gossip_lock<'a>(&'a mut self) -> (impl Context<B> + 'a, &'a mut ConsensusGossip<B>) {
		let context = ProtocolContext::new(&mut self.context_data, &self.network_chan);
		(context, &mut self.consensus_gossip)
	}

	/// Locks `self` and returns a context plus the network specialization.
	pub fn specialization_lock<'a>(&'a mut self) -> (impl Context<B> + 'a, &'a mut S) {
		let context = ProtocolContext::new(&mut self.context_data, &self.network_chan);
		(context, &mut self.specialization)
	}

	/// Gossip a consensus message to the network.
	pub fn gossip_consensus_message(
		&mut self,
		topic: B::Hash,
		engine_id: ConsensusEngineId,
		message: Vec<u8>,
		recipient: GossipMessageRecipient,
	) {
		let mut context = ProtocolContext::new(&mut self.context_data, &self.network_chan);
		let message = ConsensusMessage { data: message, engine_id };
		match recipient {
			GossipMessageRecipient::BroadcastToAll =>
				self.consensus_gossip.multicast(&mut context, topic, message, true),
			GossipMessageRecipient::BroadcastNew =>
				self.consensus_gossip.multicast(&mut context, topic, message, false),
			GossipMessageRecipient::Peer(who) =>
				self.send_message(who, GenericMessage::Consensus(message)),
		}
	}

	/// Called when a new peer is connected
	pub fn on_peer_connected(&mut self, who: PeerId, debug_info: String) {
		trace!(target: "sync", "Connecting {}: {}", who, debug_info);
		self.handshaking_peers.insert(who.clone(), HandshakingPeer { timestamp: time::Instant::now() });
		self.send_status(who);
	}

	/// Called by peer when it is disconnecting
	pub fn on_peer_disconnected(&mut self, peer: PeerId, debug_info: String) {
		trace!(target: "sync", "Disconnecting {}: {}", peer, debug_info);
		// lock all the the peer lists so that add/remove peer events are in order
		let removed = {
			self.handshaking_peers.remove(&peer);
			self.connected_peers.write().remove(&peer);
			self.context_data.peers.remove(&peer)
		};
		if let Some(peer_data) = removed {
			let mut context = ProtocolContext::new(&mut self.context_data, &self.network_chan);
			if peer_data.info.protocol_version > 2 {
				self.consensus_gossip.peer_disconnected(&mut context, peer.clone());
			}
			self.sync.peer_disconnected(&mut context, peer.clone());
			self.specialization.on_disconnect(&mut context, peer.clone());
			self.on_demand.as_ref().map(|s| s.on_disconnect(peer));
		}
	}

	/// Called as a back-pressure mechanism if the networking detects that the peer cannot process
	/// our messaging rate fast enough.
	pub fn on_clogged_peer(&self, who: PeerId, _msg: Option<Message<B>>) {
		self.network_chan.send(NetworkMsg::ReportPeer(who.clone(), CLOGGED_PEER_REPUTATION_CHANGE));

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

	/// Puts the `Synchronized` message on `network_chan`.
	#[cfg(any(test, feature = "test-helpers"))]
	pub fn synchronize(&self) {
		self.network_chan.send(NetworkMsg::Synchronized);
	}

	fn on_block_request(&mut self, peer: PeerId, request: message::BlockRequest<B>) {
		trace!(target: "sync", "BlockRequest {} from {}: from {:?} to {:?} max {:?}",
			request.id,
			peer,
			request.from,
			request.to,
			request.max);

		// sending block requests to the node that is unable to serve it is considered a bad behavior
		if !self.config.roles.is_full() {
			trace!(target: "sync", "Peer {} is trying to sync from the light node", peer);
			self.network_chan.send(NetworkMsg::DisconnectPeer(peer.clone()));
			self.network_chan.send(NetworkMsg::ReportPeer(peer, i32::min_value()));
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
		while let Some(header) = self.context_data.chain.header(&id).unwrap_or(None) {
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
						.body(&BlockId::Hash(hash))
						.unwrap_or(None)
				} else {
					None
				},
				receipt: None,
				message_queue: None,
				justification,
			};
			blocks.push(block_data);
			match request.direction {
				message::Direction::Ascending => id = BlockId::Number(number + As::sa(1)),
				message::Direction::Descending => {
					if number == As::sa(0) {
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
		self.send_message(peer, GenericMessage::BlockResponse(response))
	}

	fn on_block_response(
		&mut self,
		peer: PeerId,
		request: message::BlockRequest<B>,
		response: message::BlockResponse<B>,
	) -> CustomMessageOutcome<B> {
		let blocks_range = match (
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
			blocks_range
		);

		// TODO [andre]: move this logic to the import queue so that
		// justifications are imported asynchronously (#1482)
		if request.fields == message::BlockAttributes::JUSTIFICATION {
			let outcome = self.sync.on_block_justification_data(
				&mut ProtocolContext::new(&mut self.context_data, &self.network_chan),
				peer,
				request,
				response,
			);

			if let Some((origin, hash, nb, just)) = outcome {
				CustomMessageOutcome::JustificationImport(origin, hash, nb, just)
			} else {
				CustomMessageOutcome::None
			}

		} else {
			let outcome = self.sync.on_block_data(
				&mut ProtocolContext::new(&mut self.context_data, &self.network_chan),
				peer,
				request,
				response
			);
			if let Some((origin, blocks)) = outcome {
				CustomMessageOutcome::BlockImport(origin, blocks)
			} else {
				CustomMessageOutcome::None
			}
		}
	}

	/// Perform time based maintenance.
	///
	/// > **Note**: This method normally doesn't have to be called except for testing purposes.
	pub fn tick(&mut self) {
		self.consensus_gossip.tick(&mut ProtocolContext::new(&mut self.context_data, &self.network_chan));
		self.maintain_peers();
		self.sync.tick(&mut ProtocolContext::new(&mut self.context_data, &self.network_chan));
		self.on_demand
			.as_ref()
			.map(|s| s.maintain_peers());
	}

	fn maintain_peers(&mut self) {
		let tick = time::Instant::now();
		let mut aborting = Vec::new();
		{
			for (who, peer) in self.context_data.peers.iter() {
				if peer.block_request.as_ref().map_or(false, |(t, _)| (tick - *t).as_secs() > REQUEST_TIMEOUT_SEC) {
					trace!(target: "sync", "Request timeout {}", who);
					aborting.push(who.clone());
				} else if peer.obsolete_requests.values().any(|t| (tick - *t).as_secs() > REQUEST_TIMEOUT_SEC) {
					trace!(target: "sync", "Obsolete timeout {}", who);
					aborting.push(who.clone());
				}
			}
			for (who, _) in self.handshaking_peers.iter()
				.filter(|(_, handshaking)| (tick - handshaking.timestamp).as_secs() > REQUEST_TIMEOUT_SEC)
			{
				trace!(target: "sync", "Handshake timeout {}", who);
				aborting.push(who.clone());
			}
		}

		self.specialization.maintain_peers(&mut ProtocolContext::new(&mut self.context_data, &self.network_chan));
		for p in aborting {
			let _ = self.network_chan.send(NetworkMsg::DisconnectPeer(p.clone()));
			let _ = self.network_chan.send(NetworkMsg::ReportPeer(p, TIMEOUT_REPUTATION_CHANGE));
		}
	}

	/// Called by peer to report status
	fn on_status_message(&mut self, who: PeerId, status: message::Status<B>) {
		trace!(target: "sync", "New peer {} {:?}", who, status);
		let protocol_version = {
			if self.context_data.peers.contains_key(&who) {
				debug!("Unexpected status packet from {}", who);
				self.network_chan.send(NetworkMsg::ReportPeer(who, UNEXPECTED_STATUS_REPUTATION_CHANGE));
				return;
			}
			if status.genesis_hash != self.genesis_hash {
				trace!(
					target: "protocol",
					"Peer is on different chain (our genesis: {} theirs: {})",
					self.genesis_hash, status.genesis_hash
				);
				self.network_chan.send(NetworkMsg::ReportPeer(who.clone(), i32::min_value()));
				self.network_chan.send(NetworkMsg::DisconnectPeer(who));
				return;
			}
			if status.version < MIN_VERSION && CURRENT_VERSION < status.min_supported_version {
				trace!(target: "protocol", "Peer {:?} using unsupported protocol version {}", who, status.version);
				self.network_chan.send(NetworkMsg::ReportPeer(who.clone(), i32::min_value()));
				self.network_chan.send(NetworkMsg::DisconnectPeer(who));
				return;
			}
			if self.config.roles.is_light() {
				let self_best_block = self
					.context_data
					.chain
					.info()
					.ok()
					.and_then(|info| info.best_queued_number)
					.unwrap_or_else(|| Zero::zero());
				let blocks_difference = self_best_block
					.as_()
					.checked_sub(status.best_number.as_())
					.unwrap_or(0);
				if blocks_difference > LIGHT_MAXIMAL_BLOCKS_DIFFERENCE {
					debug!(target: "sync", "Peer {} is far behind us and will unable to serve light requests", who);
					self.network_chan.send(
						NetworkMsg::ReportPeer(who.clone(), PEER_BEHIND_US_LIGHT_REPUTATION_CHANGE)
					);
					self.network_chan.send(NetworkMsg::DisconnectPeer(who));
					return;
				}
			}

			let cache_limit = NonZeroUsize::new(1_000_000).expect("1_000_000 > 0; qed");

			let info = match self.handshaking_peers.remove(&who) {
				Some(_handshaking) => {
					let peer_info = PeerInfo {
						protocol_version: status.version,
						roles: status.roles,
						best_hash: status.best_hash,
						best_number: status.best_number
					};
					self.connected_peers
						.write()
						.insert(who.clone(), ConnectedPeer { peer_info: peer_info.clone() });
					peer_info
				},
				None => {
					error!(target: "sync", "Received status from previously unconnected node {}", who);
					return;
				},
			};

			let peer = Peer {
				info,
				block_request: None,
				known_extrinsics: LruHashSet::new(cache_limit),
				known_blocks: LruHashSet::new(cache_limit),
				next_request_id: 0,
				obsolete_requests: HashMap::new(),
			};
			self.context_data.peers.insert(who.clone(), peer);

			debug!(target: "sync", "Connected {}", who);
			status.version
		};

		let mut context = ProtocolContext::new(&mut self.context_data, &self.network_chan);
		self.on_demand
			.as_ref()
			.map(|s| s.on_connect(who.clone(), status.roles, status.best_number));
		self.sync.new_peer(&mut context, who.clone());
		if protocol_version > 2 {
			self.consensus_gossip.new_peer(&mut context, who.clone(), status.roles);
		}
		self.specialization.on_connect(&mut context, who, status);
	}

	/// Called when peer sends us new extrinsics
	fn on_extrinsics(&mut self, who: PeerId, extrinsics: message::Transactions<B::Extrinsic>) {
		// Accept extrinsics only when fully synced
		if self.sync.status().state != SyncState::Idle {
			trace!(target: "sync", "{} Ignoring extrinsics while syncing", who);
			return;
		}
		trace!(target: "sync", "Received {} extrinsics from {}", extrinsics.len(), who);
		if let Some(ref mut peer) = self.context_data.peers.get_mut(&who) {
			for t in extrinsics {
				if let Some(hash) = self.transaction_pool.import(&t) {
					self.network_chan.send(NetworkMsg::ReportPeer(who.clone(), NEW_EXTRINSIC_REPUTATION_CHANGE));
					peer.known_extrinsics.insert(hash);
				} else {
					trace!(target: "sync", "Extrinsic rejected");
				}
			}
		}
	}

	/// Call when we must propagate ready extrinsics to peers.
	pub fn propagate_extrinsics(&mut self) {
		debug!(target: "sync", "Propagating extrinsics");

		// Accept transactions only when fully synced
		if self.sync.status().state != SyncState::Idle {
			return;
		}

		let extrinsics = self.transaction_pool.transactions();
		let mut propagated_to = HashMap::new();
		for (who, peer) in self.context_data.peers.iter_mut() {
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
				self.network_chan.send(NetworkMsg::Outgoing(who.clone(), GenericMessage::Transactions(to_send)))
			}
		}
		self.transaction_pool.on_broadcasted(propagated_to);
	}

	/// Make sure an important block is propagated to peers.
	///
	/// In chain-based consensus, we often need to make sure non-best forks are
	/// at least temporarily synced.
	pub fn announce_block(&mut self, hash: B::Hash) {
		let header = match self.context_data.chain.header(&BlockId::Hash(hash)) {
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
		let hash = header.hash();

		let message = GenericMessage::BlockAnnounce(message::BlockAnnounce { header: header.clone() });

		for (who, ref mut peer) in self.context_data.peers.iter_mut() {
			trace!(target: "sync", "Reannouncing block {:?} to {}", hash, who);
			peer.known_blocks.insert(hash);
			self.network_chan.send(NetworkMsg::Outgoing(who.clone(), message.clone()))
		}
	}

	/// Send Status message
	fn send_status(&mut self, who: PeerId) {
		if let Ok(info) = self.context_data.chain.info() {
			let status = message::generic::Status {
				version: CURRENT_VERSION,
				min_supported_version: MIN_VERSION,
				genesis_hash: info.chain.genesis_hash,
				roles: self.config.roles.into(),
				best_number: info.chain.best_number,
				best_hash: info.chain.best_hash,
				chain_status: self.specialization.status(),
			};
			self.send_message(who, GenericMessage::Status(status))
		}
	}

	fn on_block_announce(&mut self, who: PeerId, announce: message::BlockAnnounce<B::Header>) {
		let header = announce.header;
		let hash = header.hash();
		{
			if let Some(ref mut peer) = self.context_data.peers.get_mut(&who) {
				peer.known_blocks.insert(hash.clone());
			}
		}
		self.on_demand
			.as_ref()
			.map(|s| s.on_block_announce(who.clone(), *header.number()));
		self.sync.on_block_announce(
			&mut ProtocolContext::new(&mut self.context_data, &self.network_chan),
			who.clone(),
			hash,
			&header,
		);
	}

	/// Call this when a block has been imported in the import queue and we should announce it on
	/// the network.
	pub fn on_block_imported(&mut self, hash: B::Hash, header: &B::Header) {
		self.sync.update_chain_info(header);
		self.specialization.on_block_imported(
			&mut ProtocolContext::new(&mut self.context_data, &self.network_chan),
			hash.clone(),
			header,
		);

		// blocks are not announced by light clients
		if self.config.roles.is_light() {
			return;
		}

		// send out block announcements

		let message = GenericMessage::BlockAnnounce(message::BlockAnnounce { header: header.clone() });

		for (who, ref mut peer) in self.context_data.peers.iter_mut() {
			if peer.known_blocks.insert(hash.clone()) {
				trace!(target: "sync", "Announcing block {:?} to {}", hash, who);
				self.network_chan.send(NetworkMsg::Outgoing(who.clone(), message.clone()))
			}
		}
	}

	/// Call this when a block has been finalized. The sync layer may have some additional
	/// requesting to perform.
	pub fn on_block_finalized(&mut self, hash: B::Hash, header: &B::Header) {
		self.sync.on_block_finalized(
			&hash,
			*header.number(),
			&mut ProtocolContext::new(&mut self.context_data, &self.network_chan),
		);
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
			&request.block,
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
				self.network_chan.send(NetworkMsg::ReportPeer(who.clone(), RPC_FAILED_REPUTATION_CHANGE));
				Default::default()
			}
		};

		self.send_message(
			who,
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
		let mut context =
			ProtocolContext::new(&mut self.context_data, &self.network_chan);
		self.sync.request_justification(&hash, number, &mut context);
	}

	/// Clears all pending justification requests.
	pub fn clear_justification_requests(&mut self) {
		self.sync.clear_justification_requests()
	}

	/// A batch of blocks have been processed, with or without errors.
	/// Call this when a batch of blocks have been processed by the import queue, with or without
	/// errors.
	pub fn blocks_processed(&mut self, processed_blocks: Vec<B::Hash>, has_error: bool) {
		self.sync.blocks_processed(processed_blocks, has_error);
		let mut context =
			ProtocolContext::new(&mut self.context_data, &self.network_chan);
		self.sync.maintain_sync(&mut context);
	}

	/// Restart the sync process.
	pub fn restart(&mut self) {
		let mut context = ProtocolContext::new(&mut self.context_data, &self.network_chan);
		self.sync.restart(&mut context);
	}

	/// Notify about successful import of the given block.
	pub fn block_imported(&mut self, hash: &B::Hash, number: NumberFor<B>) {
		self.sync.block_imported(hash, number)
	}

	pub fn set_finality_proof_request_builder(&mut self, request_builder: SharedFinalityProofRequestBuilder<B>) {
		self.sync.set_finality_proof_request_builder(request_builder)
	}

	/// Call this when a justification has been processed by the import queue, with or without
	/// errors.
	pub fn justification_import_result(&mut self, hash: B::Hash, number: NumberFor<B>, success: bool) {
		self.sync.justification_import_result(hash, number, success)
	}

	/// Request a finality proof for the given block.
	///
	/// Queues a new finality proof request and tries to dispatch all pending requests.
	pub fn request_finality_proof(&mut self, hash: &B::Hash, number: NumberFor<B>) {
		let mut context = ProtocolContext::new(&mut self.context_data, &self.network_chan);
		self.sync.request_finality_proof(&hash, number, &mut context);
	}

	pub fn finality_proof_import_result(
		&mut self,
		request_block: (B::Hash, NumberFor<B>),
		finalization_result: Result<(B::Hash, NumberFor<B>), ()>,
	) {
		self.sync.finality_proof_import_result(request_block, finalization_result)
	}

	fn on_remote_call_response(&mut self, who: PeerId, response: message::RemoteCallResponse) {
		trace!(target: "sync", "Remote call response {} from {}", response.id, who);
		self.on_demand
			.as_ref()
			.map(|s| s.on_remote_call_response(who, response));
	}

	fn on_remote_read_request(
		&mut self,
		who: PeerId,
		request: message::RemoteReadRequest<B::Hash>,
	) {
		trace!(target: "sync", "Remote read request {} from {} ({} at {})",
			request.id, who, request.key.to_hex::<String>(), request.block);
		let proof = match self.context_data.chain.read_proof(&request.block, &request.key) {
			Ok(proof) => proof,
			Err(error) => {
				trace!(target: "sync", "Remote read request {} from {} ({} at {}) failed with: {}",
					request.id,
					who,
					request.key.to_hex::<String>(),
					request.block,
					error
				);
				Default::default()
			}
		};
		self.send_message(
			who,
			GenericMessage::RemoteReadResponse(message::RemoteReadResponse {
				id: request.id,
				proof,
			}),
		);
	}
	fn on_remote_read_response(&mut self, who: PeerId, response: message::RemoteReadResponse) {
		trace!(target: "sync", "Remote read response {} from {}", response.id, who);
		self.on_demand
			.as_ref()
			.map(|s| s.on_remote_read_response(who, response));
	}

	fn on_remote_header_request(
		&mut self,
		who: PeerId,
		request: message::RemoteHeaderRequest<NumberFor<B>>,
	) {
		trace!(target: "sync", "Remote header proof request {} from {} ({})",
			request.id, who, request.block);
		let (header, proof) = match self.context_data.chain.header_proof(request.block) {
			Ok((header, proof)) => (Some(header), proof),
			Err(error) => {
				trace!(target: "sync", "Remote header proof request {} from {} ({}) failed with: {}",
					request.id,
					who,
					request.block,
					error
				);
				(Default::default(), Default::default())
			}
		};
		self.send_message(
			who,
			GenericMessage::RemoteHeaderResponse(message::RemoteHeaderResponse {
				id: request.id,
				header,
				proof,
			}),
		);
	}

	fn on_remote_header_response(
		&mut self,
		who: PeerId,
		response: message::RemoteHeaderResponse<B::Header>,
	) {
		trace!(target: "sync", "Remote header proof response {} from {}", response.id, who);
		self.on_demand
			.as_ref()
			.map(|s| s.on_remote_header_response(who, response));
	}

	fn on_remote_changes_request(
		&mut self,
		who: PeerId,
		request: message::RemoteChangesRequest<B::Hash>,
	) {
		trace!(target: "sync", "Remote changes proof request {} from {} for key {} ({}..{})",
			request.id,
			who,
			request.key.to_hex::<String>(),
			request.first,
			request.last
		);
		let key = StorageKey(request.key);
		let proof = match self.context_data.chain.key_changes_proof(
			request.first,
			request.last,
			request.min,
			request.max,
			&key
		) {
			Ok(proof) => proof,
			Err(error) => {
				trace!(target: "sync", "Remote changes proof request {} from {} for key {} ({}..{}) failed with: {}",
					request.id,
					who,
					key.0.to_hex::<String>(),
					request.first,
					request.last,
					error
				);
				ChangesProof::<B::Header> {
					max_block: Zero::zero(),
					proof: vec![],
					roots: BTreeMap::new(),
					roots_proof: vec![],
				}
			}
		};
		self.send_message(
			who,
			GenericMessage::RemoteChangesResponse(message::RemoteChangesResponse {
				id: request.id,
				max: proof.max_block,
				proof: proof.proof,
				roots: proof.roots.into_iter().collect(),
				roots_proof: proof.roots_proof,
			}),
		);
	}

	fn on_remote_changes_response(
		&mut self,
		who: PeerId,
		response: message::RemoteChangesResponse<NumberFor<B>, B::Hash>,
	) {
		trace!(target: "sync", "Remote changes proof response {} from {} (max={})",
			response.id,
			who,
			response.max
		);
		self.on_demand
			.as_ref()
			.map(|s| s.on_remote_changes_response(who, response));
	}

	fn on_finality_proof_request(
		&mut self,
		who: PeerId,
		request: message::FinalityProofRequest<B::Hash>,
	) {
		trace!(target: "sync", "Finality proof request from {} for {}", who, request.block);
		let finality_proof = self.context_data.finality_proof_provider.as_ref()
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
			who,
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
		let outcome = self.sync.on_block_finality_proof_data(
			&mut ProtocolContext::new(&mut self.context_data, &self.network_chan),
			who,
			response,
		);

		if let Some((origin, hash, nb, proof)) = outcome {
			CustomMessageOutcome::FinalityProofImport(origin, hash, nb, proof)
		} else {
			CustomMessageOutcome::None
		}
	}
}

/// Outcome of an incoming custom message.
#[derive(Debug)]
pub enum CustomMessageOutcome<B: BlockT> {
	BlockImport(BlockOrigin, Vec<IncomingBlock<B>>),
	JustificationImport(Origin, B::Hash, NumberFor<B>, Justification),
	FinalityProofImport(Origin, B::Hash, NumberFor<B>, Vec<u8>),
	None,
}

fn send_message<B: BlockT, H: ExHashT>(
	peers: &mut HashMap<PeerId, Peer<B, H>>,
	network_chan: &NetworkChan<B>,
	who: PeerId,
	mut message: Message<B>,
) {
	if let GenericMessage::BlockRequest(ref mut r) = message {
		if let Some(ref mut peer) = peers.get_mut(&who) {
			r.id = peer.next_request_id;
			peer.next_request_id = peer.next_request_id + 1;
			if let Some((timestamp, request)) = peer.block_request.take() {
				trace!(target: "sync", "Request {} for {} is now obsolete.", request.id, who);
				peer.obsolete_requests.insert(request.id, timestamp);
			}
			peer.block_request = Some((time::Instant::now(), r.clone()));
		}
	}
	network_chan.send(NetworkMsg::Outgoing(who, message));
}
