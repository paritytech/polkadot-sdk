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

//! `SyncingEngine` is the actor responsible for syncing Substrate chain
//! to tip and keep the blockchain up to date with network updates.

use crate::{
	block_announce_validator::{
		BlockAnnounceValidationResult, BlockAnnounceValidator as BlockAnnounceValidatorStream,
	},
	block_relay_protocol::{BlockDownloader, BlockResponseError},
	block_request_handler::MAX_BLOCKS_IN_RESPONSE,
	pending_responses::{PendingResponses, ResponseEvent},
	schema::v1::{StateRequest, StateResponse},
	service::{
		self,
		syncing_service::{SyncingService, ToServiceCommand},
	},
	strategy::{
		warp::{EncodedProof, WarpProofRequest, WarpSyncParams},
		SyncingAction, SyncingConfig, SyncingStrategy,
	},
	types::{
		BadPeer, ExtendedPeerInfo, OpaqueStateRequest, OpaqueStateResponse, PeerRequest, SyncEvent,
	},
	LOG_TARGET,
};

use codec::{Decode, DecodeAll, Encode};
use futures::{
	channel::oneshot,
	future::{BoxFuture, Fuse},
	FutureExt, StreamExt,
};
use libp2p::{request_response::OutboundFailure, PeerId};
use log::{debug, error, trace};
use prometheus_endpoint::{
	register, Counter, Gauge, MetricSource, Opts, PrometheusError, Registry, SourcedGauge, U64,
};
use prost::Message;
use schnellru::{ByLength, LruMap};
use tokio::time::{Interval, MissedTickBehavior};

use sc_client_api::{BlockBackend, HeaderBackend, ProofProvider};
use sc_consensus::{import_queue::ImportQueueService, IncomingBlock};
use sc_network::{
	config::{
		FullNetworkConfiguration, NonDefaultSetConfig, NonReservedPeerMode, NotificationHandshake,
		ProtocolId, SetConfig,
	},
	peer_store::{PeerStoreHandle, PeerStoreProvider},
	request_responses::{IfDisconnected, RequestFailure},
	service::traits::{Direction, NotificationEvent, ValidationResult},
	types::ProtocolName,
	utils::LruHashSet,
	NotificationService, ReputationChange,
};
use sc_network_common::{
	role::Roles,
	sync::message::{BlockAnnounce, BlockAnnouncesHandshake, BlockRequest, BlockState},
};
use sc_utils::mpsc::{tracing_unbounded, TracingUnboundedReceiver, TracingUnboundedSender};
use sp_blockchain::{Error as ClientError, HeaderMetadata};
use sp_consensus::{block_validation::BlockAnnounceValidator, BlockOrigin};
use sp_runtime::{
	traits::{Block as BlockT, Header, NumberFor, Zero},
	Justifications,
};

use std::{
	collections::{HashMap, HashSet},
	iter,
	num::NonZeroUsize,
	sync::{
		atomic::{AtomicBool, AtomicUsize, Ordering},
		Arc,
	},
	time::{Duration, Instant},
};

/// Interval at which we perform time based maintenance
const TICK_TIMEOUT: std::time::Duration = std::time::Duration::from_millis(1100);

/// Maximum number of known block hashes to keep for a peer.
const MAX_KNOWN_BLOCKS: usize = 1024; // ~32kb per peer + LruHashSet overhead

/// If the block announces stream to peer has been inactive for 30 seconds meaning local node
/// has not sent or received block announcements to/from the peer, report the node for inactivity,
/// disconnect it and attempt to establish connection to some other peer.
const INACTIVITY_EVICT_THRESHOLD: Duration = Duration::from_secs(30);

/// When `SyncingEngine` is started, wait two minutes before actually staring to count peers as
/// evicted.
///
/// Parachain collator may incorrectly get evicted because it's waiting to receive a number of
/// relaychain blocks before it can start creating parachain blocks. During this wait,
/// `SyncingEngine` still counts it as active and as the peer is not sending blocks, it may get
/// evicted if a block is not received within the first 30 secons since the peer connected.
///
/// To prevent this from happening, define a threshold for how long `SyncingEngine` should wait
/// before it starts evicting peers.
const INITIAL_EVICTION_WAIT_PERIOD: Duration = Duration::from_secs(2 * 60);

/// Maximum allowed size for a block announce.
const MAX_BLOCK_ANNOUNCE_SIZE: u64 = 1024 * 1024;

mod rep {
	use sc_network::ReputationChange as Rep;
	/// Peer has different genesis.
	pub const GENESIS_MISMATCH: Rep = Rep::new_fatal("Genesis mismatch");
	/// Peer send us a block announcement that failed at validation.
	pub const BAD_BLOCK_ANNOUNCEMENT: Rep = Rep::new(-(1 << 12), "Bad block announcement");
	/// Block announce substream with the peer has been inactive too long
	pub const INACTIVE_SUBSTREAM: Rep = Rep::new(-(1 << 10), "Inactive block announce substream");
	/// We received a message that failed to decode.
	pub const BAD_MESSAGE: Rep = Rep::new(-(1 << 12), "Bad message");
	/// Peer is on unsupported protocol version.
	pub const BAD_PROTOCOL: Rep = Rep::new_fatal("Unsupported protocol");
	/// Reputation change when a peer refuses a request.
	pub const REFUSED: Rep = Rep::new(-(1 << 10), "Request refused");
	/// Reputation change when a peer doesn't respond in time to our messages.
	pub const TIMEOUT: Rep = Rep::new(-(1 << 10), "Request timeout");
}

struct Metrics {
	peers: Gauge<U64>,
	import_queue_blocks_submitted: Counter<U64>,
	import_queue_justifications_submitted: Counter<U64>,
}

impl Metrics {
	fn register(r: &Registry, major_syncing: Arc<AtomicBool>) -> Result<Self, PrometheusError> {
		let _ = MajorSyncingGauge::register(r, major_syncing)?;
		Ok(Self {
			peers: {
				let g = Gauge::new("substrate_sync_peers", "Number of peers we sync with")?;
				register(g, r)?
			},
			import_queue_blocks_submitted: {
				let c = Counter::new(
					"substrate_sync_import_queue_blocks_submitted",
					"Number of blocks submitted to the import queue.",
				)?;
				register(c, r)?
			},
			import_queue_justifications_submitted: {
				let c = Counter::new(
					"substrate_sync_import_queue_justifications_submitted",
					"Number of justifications submitted to the import queue.",
				)?;
				register(c, r)?
			},
		})
	}
}

/// The "major syncing" metric.
#[derive(Clone)]
pub struct MajorSyncingGauge(Arc<AtomicBool>);

impl MajorSyncingGauge {
	/// Registers the [`MajorSyncGauge`] metric whose value is
	/// obtained from the given `AtomicBool`.
	fn register(registry: &Registry, value: Arc<AtomicBool>) -> Result<(), PrometheusError> {
		prometheus_endpoint::register(
			SourcedGauge::new(
				&Opts::new(
					"substrate_sub_libp2p_is_major_syncing",
					"Whether the node is performing a major sync or not.",
				),
				MajorSyncingGauge(value),
			)?,
			registry,
		)?;

		Ok(())
	}
}

impl MetricSource for MajorSyncingGauge {
	type N = u64;

	fn collect(&self, mut set: impl FnMut(&[&str], Self::N)) {
		set(&[], self.0.load(Ordering::Relaxed) as u64);
	}
}

/// Peer information
#[derive(Debug)]
pub struct Peer<B: BlockT> {
	pub info: ExtendedPeerInfo<B>,
	/// Holds a set of blocks known to this peer.
	pub known_blocks: LruHashSet<B::Hash>,
	/// Is the peer inbound.
	inbound: bool,
}

pub struct SyncingEngine<B: BlockT, Client> {
	/// Syncing strategy.
	strategy: SyncingStrategy<B, Client>,

	/// Syncing configuration for startegies.
	syncing_config: SyncingConfig,

	/// Blockchain client.
	client: Arc<Client>,

	/// Number of peers we're connected to.
	num_connected: Arc<AtomicUsize>,

	/// Are we actively catching up with the chain?
	is_major_syncing: Arc<AtomicBool>,

	/// Network service.
	network_service: service::network::NetworkServiceHandle,

	/// Channel for receiving service commands
	service_rx: TracingUnboundedReceiver<ToServiceCommand<B>>,

	/// Assigned roles.
	roles: Roles,

	/// Genesis hash.
	genesis_hash: B::Hash,

	/// Set of channels for other protocols that have subscribed to syncing events.
	event_streams: Vec<TracingUnboundedSender<SyncEvent>>,

	/// Interval at which we call `tick`.
	tick_timeout: Interval,

	/// All connected peers. Contains both full and light node peers.
	peers: HashMap<PeerId, Peer<B>>,

	/// List of nodes for which we perform additional logging because they are important for the
	/// user.
	important_peers: HashSet<PeerId>,

	/// Actual list of connected no-slot nodes.
	default_peers_set_no_slot_connected_peers: HashSet<PeerId>,

	/// List of nodes that should never occupy peer slots.
	default_peers_set_no_slot_peers: HashSet<PeerId>,

	/// Value that was passed as part of the configuration. Used to cap the number of full
	/// nodes.
	default_peers_set_num_full: usize,

	/// Number of slots to allocate to light nodes.
	default_peers_set_num_light: usize,

	/// Maximum number of inbound peers.
	max_in_peers: usize,

	/// Number of inbound peers accepted so far.
	num_in_peers: usize,

	/// Async processor of block announce validations.
	block_announce_validator: BlockAnnounceValidatorStream<B>,

	/// A cache for the data that was associated to a block announcement.
	block_announce_data_cache: LruMap<B::Hash, Vec<u8>>,

	/// The `PeerId`'s of all boot nodes.
	boot_node_ids: HashSet<PeerId>,

	/// A channel to get target block header if we skip over proofs downloading during warp sync.
	warp_sync_target_block_header_rx_fused:
		Fuse<BoxFuture<'static, Result<B::Header, oneshot::Canceled>>>,

	/// Protocol name used for block announcements
	block_announce_protocol_name: ProtocolName,

	/// Prometheus metrics.
	metrics: Option<Metrics>,

	/// Handle that is used to communicate with `sc_network::Notifications`.
	notification_service: Box<dyn NotificationService>,

	/// When the syncing was started.
	///
	/// Stored as an `Option<Instant>` so once the initial wait has passed, `SyncingEngine`
	/// can reset the peer timers and continue with the normal eviction process.
	syncing_started: Option<Instant>,

	/// Handle to `PeerStore`.
	peer_store_handle: PeerStoreHandle,

	/// Instant when the last notification was sent or received.
	last_notification_io: Instant,

	/// Pending responses
	pending_responses: PendingResponses<B>,

	/// Block downloader
	block_downloader: Arc<dyn BlockDownloader<B>>,

	/// Protocol name used to send out state requests
	state_request_protocol_name: ProtocolName,

	/// Protocol name used to send out warp sync requests
	warp_sync_protocol_name: Option<ProtocolName>,

	/// Handle to import queue.
	import_queue: Box<dyn ImportQueueService<B>>,
}

impl<B: BlockT, Client> SyncingEngine<B, Client>
where
	B: BlockT,
	Client: HeaderBackend<B>
		+ BlockBackend<B>
		+ HeaderMetadata<B, Error = sp_blockchain::Error>
		+ ProofProvider<B>
		+ Send
		+ Sync
		+ 'static,
{
	pub fn new(
		roles: Roles,
		client: Arc<Client>,
		metrics_registry: Option<&Registry>,
		net_config: &FullNetworkConfiguration,
		protocol_id: ProtocolId,
		fork_id: &Option<String>,
		block_announce_validator: Box<dyn BlockAnnounceValidator<B> + Send>,
		warp_sync_params: Option<WarpSyncParams<B>>,
		network_service: service::network::NetworkServiceHandle,
		import_queue: Box<dyn ImportQueueService<B>>,
		block_downloader: Arc<dyn BlockDownloader<B>>,
		state_request_protocol_name: ProtocolName,
		warp_sync_protocol_name: Option<ProtocolName>,
		peer_store_handle: PeerStoreHandle,
	) -> Result<(Self, SyncingService<B>, NonDefaultSetConfig), ClientError> {
		let mode = net_config.network_config.sync_mode;
		let max_parallel_downloads = net_config.network_config.max_parallel_downloads;
		let max_blocks_per_request =
			if net_config.network_config.max_blocks_per_request > MAX_BLOCKS_IN_RESPONSE as u32 {
				log::info!(
					target: LOG_TARGET,
					"clamping maximum blocks per request to {}",
					MAX_BLOCKS_IN_RESPONSE,
				);
				MAX_BLOCKS_IN_RESPONSE as u32
			} else {
				net_config.network_config.max_blocks_per_request
			};
		let syncing_config = SyncingConfig {
			mode,
			max_parallel_downloads,
			max_blocks_per_request,
			metrics_registry: metrics_registry.cloned(),
		};
		let cache_capacity = (net_config.network_config.default_peers_set.in_peers +
			net_config.network_config.default_peers_set.out_peers)
			.max(1);
		let important_peers = {
			let mut imp_p = HashSet::new();
			for reserved in &net_config.network_config.default_peers_set.reserved_nodes {
				imp_p.insert(reserved.peer_id);
			}
			for config in net_config.notification_protocols() {
				let peer_ids = config
					.set_config()
					.reserved_nodes
					.iter()
					.map(|info| info.peer_id)
					.collect::<Vec<PeerId>>();
				imp_p.extend(peer_ids);
			}

			imp_p.shrink_to_fit();
			imp_p
		};
		let boot_node_ids = {
			let mut list = HashSet::new();
			for node in &net_config.network_config.boot_nodes {
				list.insert(node.peer_id);
			}
			list.shrink_to_fit();
			list
		};
		let default_peers_set_no_slot_peers = {
			let mut no_slot_p: HashSet<PeerId> = net_config
				.network_config
				.default_peers_set
				.reserved_nodes
				.iter()
				.map(|reserved| reserved.peer_id)
				.collect();
			no_slot_p.shrink_to_fit();
			no_slot_p
		};
		let default_peers_set_num_full =
			net_config.network_config.default_peers_set_num_full as usize;
		let default_peers_set_num_light = {
			let total = net_config.network_config.default_peers_set.out_peers +
				net_config.network_config.default_peers_set.in_peers;
			total.saturating_sub(net_config.network_config.default_peers_set_num_full) as usize
		};

		let (block_announce_config, notification_service) = Self::get_block_announce_proto_config(
			protocol_id,
			fork_id,
			roles,
			client.info().best_number,
			client.info().best_hash,
			client
				.block_hash(Zero::zero())
				.ok()
				.flatten()
				.expect("Genesis block exists; qed"),
		);

		// Split warp sync params into warp sync config and a channel to retreive target block
		// header.
		let (warp_sync_config, warp_sync_target_block_header_rx) =
			warp_sync_params.map_or((None, None), |params| {
				let (config, target_block_rx) = params.split();
				(Some(config), target_block_rx)
			});

		// Make sure polling of the target block channel is a no-op if there is no block to
		// retrieve.
		let warp_sync_target_block_header_rx_fused = warp_sync_target_block_header_rx
			.map_or(futures::future::pending().boxed().fuse(), |rx| rx.boxed().fuse());

		// Initialize syncing strategy.
		let strategy =
			SyncingStrategy::new(syncing_config.clone(), client.clone(), warp_sync_config)?;

		let block_announce_protocol_name = block_announce_config.protocol_name().clone();
		let (tx, service_rx) = tracing_unbounded("mpsc_chain_sync", 100_000);
		let num_connected = Arc::new(AtomicUsize::new(0));
		let is_major_syncing = Arc::new(AtomicBool::new(false));
		let genesis_hash = client
			.block_hash(0u32.into())
			.ok()
			.flatten()
			.expect("Genesis block exists; qed");

		// `default_peers_set.in_peers` contains an unspecified amount of light peers so the number
		// of full inbound peers must be calculated from the total full peer count
		let max_full_peers = net_config.network_config.default_peers_set_num_full;
		let max_out_peers = net_config.network_config.default_peers_set.out_peers;
		let max_in_peers = (max_full_peers - max_out_peers) as usize;

		let tick_timeout = {
			let mut interval = tokio::time::interval(TICK_TIMEOUT);
			interval.set_missed_tick_behavior(MissedTickBehavior::Delay);
			interval
		};

		Ok((
			Self {
				roles,
				client,
				strategy,
				syncing_config,
				network_service,
				peers: HashMap::new(),
				block_announce_data_cache: LruMap::new(ByLength::new(cache_capacity)),
				block_announce_protocol_name,
				block_announce_validator: BlockAnnounceValidatorStream::new(
					block_announce_validator,
				),
				num_connected: num_connected.clone(),
				is_major_syncing: is_major_syncing.clone(),
				service_rx,
				genesis_hash,
				important_peers,
				default_peers_set_no_slot_connected_peers: HashSet::new(),
				warp_sync_target_block_header_rx_fused,
				boot_node_ids,
				default_peers_set_no_slot_peers,
				default_peers_set_num_full,
				default_peers_set_num_light,
				num_in_peers: 0usize,
				max_in_peers,
				event_streams: Vec::new(),
				notification_service,
				tick_timeout,
				syncing_started: None,
				peer_store_handle,
				last_notification_io: Instant::now(),
				metrics: if let Some(r) = metrics_registry {
					match Metrics::register(r, is_major_syncing.clone()) {
						Ok(metrics) => Some(metrics),
						Err(err) => {
							log::error!(target: LOG_TARGET, "Failed to register metrics {err:?}");
							None
						},
					}
				} else {
					None
				},
				pending_responses: PendingResponses::new(),
				block_downloader,
				state_request_protocol_name,
				warp_sync_protocol_name,
				import_queue,
			},
			SyncingService::new(tx, num_connected, is_major_syncing),
			block_announce_config,
		))
	}

	/// Report Prometheus metrics.
	pub fn report_metrics(&self) {
		if let Some(metrics) = &self.metrics {
			let n = u64::try_from(self.peers.len()).unwrap_or(std::u64::MAX);
			metrics.peers.set(n);
		}
		self.strategy.report_metrics();
	}

	fn update_peer_info(
		&mut self,
		peer_id: &PeerId,
		best_hash: B::Hash,
		best_number: NumberFor<B>,
	) {
		if let Some(ref mut peer) = self.peers.get_mut(peer_id) {
			peer.info.best_hash = best_hash;
			peer.info.best_number = best_number;
		}
	}

	/// Process the result of the block announce validation.
	fn process_block_announce_validation_result(
		&mut self,
		validation_result: BlockAnnounceValidationResult<B::Header>,
	) {
		match validation_result {
			BlockAnnounceValidationResult::Skip { peer_id: _ } => {},
			BlockAnnounceValidationResult::Process { is_new_best, peer_id, announce } => {
				if let Some((best_hash, best_number)) =
					self.strategy.on_validated_block_announce(is_new_best, peer_id, &announce)
				{
					self.update_peer_info(&peer_id, best_hash, best_number);
				}

				if let Some(data) = announce.data {
					if !data.is_empty() {
						self.block_announce_data_cache.insert(announce.header.hash(), data);
					}
				}
			},
			BlockAnnounceValidationResult::Failure { peer_id, disconnect } => {
				if disconnect {
					self.network_service
						.disconnect_peer(peer_id, self.block_announce_protocol_name.clone());
				}

				self.network_service.report_peer(peer_id, rep::BAD_BLOCK_ANNOUNCEMENT);
			},
		}
	}

	/// Push a block announce validation.
	pub fn push_block_announce_validation(
		&mut self,
		peer_id: PeerId,
		announce: BlockAnnounce<B::Header>,
	) {
		let hash = announce.header.hash();

		let peer = match self.peers.get_mut(&peer_id) {
			Some(p) => p,
			None => {
				log::error!(
					target: LOG_TARGET,
					"Received block announce from disconnected peer {peer_id}",
				);
				debug_assert!(false);
				return
			},
		};
		peer.known_blocks.insert(hash);

		if peer.info.roles.is_full() {
			let is_best = match announce.state.unwrap_or(BlockState::Best) {
				BlockState::Best => true,
				BlockState::Normal => false,
			};

			self.block_announce_validator
				.push_block_announce_validation(peer_id, hash, announce, is_best);
		}
	}

	/// Make sure an important block is propagated to peers.
	///
	/// In chain-based consensus, we often need to make sure non-best forks are
	/// at least temporarily synced.
	pub fn announce_block(&mut self, hash: B::Hash, data: Option<Vec<u8>>) {
		let header = match self.client.header(hash) {
			Ok(Some(header)) => header,
			Ok(None) => {
				log::warn!(target: LOG_TARGET, "Trying to announce unknown block: {hash}");
				return
			},
			Err(e) => {
				log::warn!(target: LOG_TARGET, "Error reading block header {hash}: {e}");
				return
			},
		};

		// don't announce genesis block since it will be ignored
		if header.number().is_zero() {
			return
		}

		let is_best = self.client.info().best_hash == hash;
		log::debug!(target: LOG_TARGET, "Reannouncing block {hash:?} is_best: {is_best}");

		let data = data
			.or_else(|| self.block_announce_data_cache.get(&hash).cloned())
			.unwrap_or_default();

		for (peer_id, ref mut peer) in self.peers.iter_mut() {
			let inserted = peer.known_blocks.insert(hash);
			if inserted {
				log::trace!(target: LOG_TARGET, "Announcing block {hash:?} to {peer_id}");
				let message = BlockAnnounce {
					header: header.clone(),
					state: if is_best { Some(BlockState::Best) } else { Some(BlockState::Normal) },
					data: Some(data.clone()),
				};

				self.last_notification_io = Instant::now();
				let _ = self.notification_service.send_sync_notification(peer_id, message.encode());
			}
		}
	}

	pub async fn run(mut self) {
		self.syncing_started = Some(Instant::now());

		loop {
			tokio::select! {
				_ = self.tick_timeout.tick() => self.perform_periodic_actions(),
				command = self.service_rx.select_next_some() =>
					self.process_service_command(command),
				notification_event = self.notification_service.next_event() => match notification_event {
					Some(event) => self.process_notification_event(event),
					None => return,
				},
				warp_target_block_header = &mut self.warp_sync_target_block_header_rx_fused =>
					self.pass_warp_sync_target_block_header(warp_target_block_header),
				response_event = self.pending_responses.select_next_some() =>
					self.process_response_event(response_event),
				validation_result = self.block_announce_validator.select_next_some() =>
					self.process_block_announce_validation_result(validation_result),
			}

			// Update atomic variables
			self.num_connected.store(self.peers.len(), Ordering::Relaxed);
			self.is_major_syncing.store(self.strategy.is_major_syncing(), Ordering::Relaxed);

			// Process actions requested by a syncing strategy.
			if let Err(e) = self.process_strategy_actions() {
				error!("Terminating `SyncingEngine` due to fatal error: {e:?}");
				return
			}
		}
	}

	fn process_strategy_actions(&mut self) -> Result<(), ClientError> {
		for action in self.strategy.actions() {
			match action {
				SyncingAction::SendBlockRequest { peer_id, request } => {
					// Sending block request implies dropping obsolete pending response as we are
					// not interested in it anymore (see [`SyncingAction::SendBlockRequest`]).
					// Furthermore, only one request at a time is allowed to any peer.
					let removed = self.pending_responses.remove(&peer_id);
					self.send_block_request(peer_id, request.clone());

					trace!(
						target: LOG_TARGET,
						"Processed `ChainSyncAction::SendBlockRequest` to {} with {:?}, stale response removed: {}.",
						peer_id,
						request,
						removed,
					)
				},
				SyncingAction::CancelBlockRequest { peer_id } => {
					let removed = self.pending_responses.remove(&peer_id);

					trace!(
						target: LOG_TARGET,
						"Processed {action:?}, response removed: {removed}.",
					);
				},
				SyncingAction::SendStateRequest { peer_id, request } => {
					self.send_state_request(peer_id, request);

					trace!(
						target: LOG_TARGET,
						"Processed `ChainSyncAction::SendBlockRequest` to {peer_id}.",
					);
				},
				SyncingAction::SendWarpProofRequest { peer_id, request } => {
					self.send_warp_proof_request(peer_id, request.clone());

					trace!(
						target: LOG_TARGET,
						"Processed `ChainSyncAction::SendWarpProofRequest` to {}, request: {:?}.",
						peer_id,
						request,
					);
				},
				SyncingAction::DropPeer(BadPeer(peer_id, rep)) => {
					self.pending_responses.remove(&peer_id);
					self.network_service
						.disconnect_peer(peer_id, self.block_announce_protocol_name.clone());
					self.network_service.report_peer(peer_id, rep);

					trace!(target: LOG_TARGET, "{peer_id:?} dropped: {rep:?}.");
				},
				SyncingAction::ImportBlocks { origin, blocks } => {
					let count = blocks.len();
					self.import_blocks(origin, blocks);

					trace!(
						target: LOG_TARGET,
						"Processed `ChainSyncAction::ImportBlocks` with {count} blocks.",
					);
				},
				SyncingAction::ImportJustifications { peer_id, hash, number, justifications } => {
					self.import_justifications(peer_id, hash, number, justifications);

					trace!(
						target: LOG_TARGET,
						"Processed `ChainSyncAction::ImportJustifications` from peer {} for block {} ({}).",
						peer_id,
						hash,
						number,
					)
				},
				SyncingAction::Finished => {
					let connected_peers = self.peers.iter().filter_map(|(peer_id, peer)| {
						peer.info.roles.is_full().then_some((
							*peer_id,
							peer.info.best_hash,
							peer.info.best_number,
						))
					});
					self.strategy.switch_to_next(
						self.syncing_config.clone(),
						self.client.clone(),
						connected_peers,
					)?;
				},
			}
		}

		Ok(())
	}

	fn perform_periodic_actions(&mut self) {
		self.report_metrics();

		// if `SyncingEngine` has just started, don't evict seemingly inactive peers right away
		// as they may not have produced blocks not because they've disconnected but because
		// they're still waiting to receive enough relaychain blocks to start producing blocks.
		if let Some(started) = self.syncing_started {
			if started.elapsed() < INITIAL_EVICTION_WAIT_PERIOD {
				return
			}

			self.syncing_started = None;
			self.last_notification_io = Instant::now();
		}

		// if syncing hasn't sent or received any blocks within `INACTIVITY_EVICT_THRESHOLD`,
		// it means the local node has stalled and is connected to peers who either don't
		// consider it connected or are also all stalled. In order to unstall the node,
		// disconnect all peers and allow `ProtocolController` to establish new connections.
		if self.last_notification_io.elapsed() > INACTIVITY_EVICT_THRESHOLD {
			log::debug!(
				target: LOG_TARGET,
				"syncing has halted due to inactivity, evicting all peers",
			);

			for peer in self.peers.keys() {
				self.network_service.report_peer(*peer, rep::INACTIVE_SUBSTREAM);
				self.network_service
					.disconnect_peer(*peer, self.block_announce_protocol_name.clone());
			}

			// after all the peers have been evicted, start timer again to prevent evicting
			// new peers that join after the old peer have been evicted
			self.last_notification_io = Instant::now();
		}
	}

	fn process_service_command(&mut self, command: ToServiceCommand<B>) {
		match command {
			ToServiceCommand::SetSyncForkRequest(peers, hash, number) => {
				self.strategy.set_sync_fork_request(peers, &hash, number);
			},
			ToServiceCommand::EventStream(tx) => self.event_streams.push(tx),
			ToServiceCommand::RequestJustification(hash, number) =>
				self.strategy.request_justification(&hash, number),
			ToServiceCommand::ClearJustificationRequests =>
				self.strategy.clear_justification_requests(),
			ToServiceCommand::BlocksProcessed(imported, count, results) => {
				self.strategy.on_blocks_processed(imported, count, results);
			},
			ToServiceCommand::JustificationImported(peer_id, hash, number, success) => {
				self.strategy.on_justification_import(hash, number, success);
				if !success {
					log::info!(
						target: LOG_TARGET,
						"💔 Invalid justification provided by {peer_id} for #{hash}",
					);
					self.network_service
						.disconnect_peer(peer_id, self.block_announce_protocol_name.clone());
					self.network_service
						.report_peer(peer_id, ReputationChange::new_fatal("Invalid justification"));
				}
			},
			ToServiceCommand::AnnounceBlock(hash, data) => self.announce_block(hash, data),
			ToServiceCommand::NewBestBlockImported(hash, number) => {
				log::debug!(target: LOG_TARGET, "New best block imported {:?}/#{}", hash, number);

				self.strategy.update_chain_info(&hash, number);
				let _ = self.notification_service.try_set_handshake(
					BlockAnnouncesHandshake::<B>::build(
						self.roles,
						number,
						hash,
						self.genesis_hash,
					)
					.encode(),
				);
			},
			ToServiceCommand::Status(tx) => {
				let mut status = self.strategy.status();
				status.num_connected_peers = self.peers.len() as u32;
				let _ = tx.send(status);
			},
			ToServiceCommand::NumActivePeers(tx) => {
				let _ = tx.send(self.num_active_peers());
			},
			ToServiceCommand::SyncState(tx) => {
				let _ = tx.send(self.strategy.status());
			},
			ToServiceCommand::BestSeenBlock(tx) => {
				let _ = tx.send(self.strategy.status().best_seen_block);
			},
			ToServiceCommand::NumSyncPeers(tx) => {
				let _ = tx.send(self.strategy.status().num_peers);
			},
			ToServiceCommand::NumQueuedBlocks(tx) => {
				let _ = tx.send(self.strategy.status().queued_blocks);
			},
			ToServiceCommand::NumDownloadedBlocks(tx) => {
				let _ = tx.send(self.strategy.num_downloaded_blocks());
			},
			ToServiceCommand::NumSyncRequests(tx) => {
				let _ = tx.send(self.strategy.num_sync_requests());
			},
			ToServiceCommand::PeersInfo(tx) => {
				let peers_info = self
					.peers
					.iter()
					.map(|(peer_id, peer)| (*peer_id, peer.info.clone()))
					.collect();
				let _ = tx.send(peers_info);
			},
			ToServiceCommand::OnBlockFinalized(hash, header) =>
				self.strategy.on_block_finalized(&hash, *header.number()),
		}
	}

	fn process_notification_event(&mut self, event: NotificationEvent) {
		match event {
			NotificationEvent::ValidateInboundSubstream { peer, handshake, result_tx } => {
				let validation_result = self
					.validate_connection(&peer, handshake, Direction::Inbound)
					.map_or(ValidationResult::Reject, |_| ValidationResult::Accept);

				let _ = result_tx.send(validation_result);
			},
			NotificationEvent::NotificationStreamOpened { peer, handshake, direction, .. } => {
				log::debug!(
					target: LOG_TARGET,
					"Substream opened for {peer}, handshake {handshake:?}"
				);

				match self.validate_connection(&peer, handshake, direction) {
					Ok(handshake) => {
						if self.on_sync_peer_connected(peer, &handshake, direction).is_err() {
							log::debug!(target: LOG_TARGET, "Failed to register peer {peer}");
							self.network_service
								.disconnect_peer(peer, self.block_announce_protocol_name.clone());
						}
					},
					Err(wrong_genesis) => {
						log::debug!(target: LOG_TARGET, "`SyncingEngine` rejected {peer}");

						if wrong_genesis {
							self.peer_store_handle.report_peer(peer, rep::GENESIS_MISMATCH);
						}

						self.network_service
							.disconnect_peer(peer, self.block_announce_protocol_name.clone());
					},
				}
			},
			NotificationEvent::NotificationStreamClosed { peer } => {
				self.on_sync_peer_disconnected(peer);
			},
			NotificationEvent::NotificationReceived { peer, notification } => {
				if !self.peers.contains_key(&peer) {
					log::error!(
						target: LOG_TARGET,
						"received notification from {peer} who had been earlier refused by `SyncingEngine`",
					);
					return
				}

				let Ok(announce) = BlockAnnounce::decode(&mut notification.as_ref()) else {
					log::warn!(target: LOG_TARGET, "failed to decode block announce");
					return
				};

				self.last_notification_io = Instant::now();
				self.push_block_announce_validation(peer, announce);
			},
		}
	}

	fn pass_warp_sync_target_block_header(&mut self, header: Result<B::Header, oneshot::Canceled>) {
		match header {
			Ok(header) =>
				if let SyncingStrategy::WarpSyncStrategy(warp_sync) = &mut self.strategy {
					warp_sync.set_target_block(header);
				} else {
					error!(
						target: LOG_TARGET,
						"Cannot set warp sync target block: no warp sync strategy is active."
					);
					debug_assert!(false);
				},
			Err(err) => {
				error!(
					target: LOG_TARGET,
					"Failed to get target block for warp sync. Error: {err:?}",
				);
			},
		}
	}

	/// Called by peer when it is disconnecting.
	///
	/// Returns a result if the handshake of this peer was indeed accepted.
	fn on_sync_peer_disconnected(&mut self, peer_id: PeerId) {
		let Some(info) = self.peers.remove(&peer_id) else {
			log::debug!(target: LOG_TARGET, "{peer_id} does not exist in `SyncingEngine`");
			return
		};

		if self.important_peers.contains(&peer_id) {
			log::warn!(target: LOG_TARGET, "Reserved peer {peer_id} disconnected");
		} else {
			log::debug!(target: LOG_TARGET, "{peer_id} disconnected");
		}

		if !self.default_peers_set_no_slot_connected_peers.remove(&peer_id) &&
			info.inbound && info.info.roles.is_full()
		{
			match self.num_in_peers.checked_sub(1) {
				Some(value) => {
					self.num_in_peers = value;
				},
				None => {
					log::error!(
						target: LOG_TARGET,
						"trying to disconnect an inbound node which is not counted as inbound"
					);
					debug_assert!(false);
				},
			}
		}

		self.strategy.remove_peer(&peer_id);
		self.pending_responses.remove(&peer_id);
		self.event_streams
			.retain(|stream| stream.unbounded_send(SyncEvent::PeerDisconnected(peer_id)).is_ok());
	}

	/// Validate received handshake.
	fn validate_handshake(
		&mut self,
		peer_id: &PeerId,
		handshake: Vec<u8>,
	) -> Result<BlockAnnouncesHandshake<B>, bool> {
		log::trace!(target: LOG_TARGET, "Validate handshake for {peer_id}");

		let handshake = <BlockAnnouncesHandshake<B> as DecodeAll>::decode_all(&mut &handshake[..])
			.map_err(|error| {
				log::debug!(target: LOG_TARGET, "Failed to decode handshake for {peer_id}: {error:?}");
				false
			})?;

		if handshake.genesis_hash != self.genesis_hash {
			if self.important_peers.contains(&peer_id) {
				log::error!(
					target: LOG_TARGET,
					"Reserved peer id `{peer_id}` is on a different chain (our genesis: {} theirs: {})",
					self.genesis_hash,
					handshake.genesis_hash,
				);
			} else if self.boot_node_ids.contains(&peer_id) {
				log::error!(
					target: LOG_TARGET,
					"Bootnode with peer id `{peer_id}` is on a different chain (our genesis: {} theirs: {})",
					self.genesis_hash,
					handshake.genesis_hash,
				);
			} else {
				log::debug!(
					target: LOG_TARGET,
					"Peer is on different chain (our genesis: {} theirs: {})",
					self.genesis_hash,
					handshake.genesis_hash
				);
			}

			return Err(true)
		}

		Ok(handshake)
	}

	/// Validate connection.
	// NOTE Returning `Err(bool)` is a really ugly hack to work around the issue
	// that `ProtocolController` thinks the peer is connected when in fact it can
	// still be under validation. If the peer has different genesis than the
	// local node the validation fails but the peer cannot be reported in
	// `validate_connection()` as that is also called by
	// `ValiateInboundSubstream` which means that the peer is still being
	// validated and banning the peer when handling that event would
	// result in peer getting dropped twice.
	//
	// The proper way to fix this is to integrate `ProtocolController` more
	// tightly with `NotificationService` or add an additional API call for
	// banning pre-accepted peers (which is not desirable)
	fn validate_connection(
		&mut self,
		peer_id: &PeerId,
		handshake: Vec<u8>,
		direction: Direction,
	) -> Result<BlockAnnouncesHandshake<B>, bool> {
		log::trace!(target: LOG_TARGET, "New peer {peer_id} {handshake:?}");

		let handshake = self.validate_handshake(peer_id, handshake)?;

		if self.peers.contains_key(&peer_id) {
			log::error!(
				target: LOG_TARGET,
				"Called `validate_connection()` with already connected peer {peer_id}",
			);
			debug_assert!(false);
			return Err(false)
		}

		let no_slot_peer = self.default_peers_set_no_slot_peers.contains(&peer_id);
		let this_peer_reserved_slot: usize = if no_slot_peer { 1 } else { 0 };

		if handshake.roles.is_full() &&
			self.strategy.num_peers() >=
				self.default_peers_set_num_full +
					self.default_peers_set_no_slot_connected_peers.len() +
					this_peer_reserved_slot
		{
			log::debug!(target: LOG_TARGET, "Too many full nodes, rejecting {peer_id}");
			return Err(false)
		}

		// make sure to accept no more than `--in-peers` many full nodes
		if !no_slot_peer &&
			handshake.roles.is_full() &&
			direction.is_inbound() &&
			self.num_in_peers == self.max_in_peers
		{
			log::debug!(target: LOG_TARGET, "All inbound slots have been consumed, rejecting {peer_id}");
			return Err(false)
		}

		// make sure that all slots are not occupied by light peers
		//
		// `ChainSync` only accepts full peers whereas `SyncingEngine` accepts both full and light
		// peers. Verify that there is a slot in `SyncingEngine` for the inbound light peer
		if handshake.roles.is_light() &&
			(self.peers.len() - self.strategy.num_peers()) >= self.default_peers_set_num_light
		{
			log::debug!(target: LOG_TARGET, "Too many light nodes, rejecting {peer_id}");
			return Err(false)
		}

		Ok(handshake)
	}

	/// Called on the first connection between two peers on the default set, after their exchange
	/// of handshake.
	///
	/// Returns `Ok` if the handshake is accepted and the peer added to the list of peers we sync
	/// from.
	fn on_sync_peer_connected(
		&mut self,
		peer_id: PeerId,
		status: &BlockAnnouncesHandshake<B>,
		direction: Direction,
	) -> Result<(), ()> {
		log::trace!(target: LOG_TARGET, "New peer {peer_id} {status:?}");

		let peer = Peer {
			info: ExtendedPeerInfo {
				roles: status.roles,
				best_hash: status.best_hash,
				best_number: status.best_number,
			},
			known_blocks: LruHashSet::new(
				NonZeroUsize::new(MAX_KNOWN_BLOCKS).expect("Constant is nonzero"),
			),
			inbound: direction.is_inbound(),
		};

		// Only forward full peers to syncing strategy.
		if status.roles.is_full() {
			self.strategy.add_peer(peer_id, peer.info.best_hash, peer.info.best_number);
		}

		log::debug!(target: LOG_TARGET, "Connected {peer_id}");

		self.peers.insert(peer_id, peer);
		self.peer_store_handle.set_peer_role(&peer_id, status.roles.into());

		if self.default_peers_set_no_slot_peers.contains(&peer_id) {
			self.default_peers_set_no_slot_connected_peers.insert(peer_id);
		} else if direction.is_inbound() && status.roles.is_full() {
			self.num_in_peers += 1;
		}

		self.event_streams
			.retain(|stream| stream.unbounded_send(SyncEvent::PeerConnected(peer_id)).is_ok());

		Ok(())
	}

	fn send_block_request(&mut self, peer_id: PeerId, request: BlockRequest<B>) {
		if !self.peers.contains_key(&peer_id) {
			trace!(target: LOG_TARGET, "Cannot send block request to unknown peer {peer_id}");
			debug_assert!(false);
			return
		}

		let downloader = self.block_downloader.clone();

		self.pending_responses.insert(
			peer_id,
			PeerRequest::Block(request.clone()),
			async move { downloader.download_blocks(peer_id, request).await }.boxed(),
		);
	}

	fn send_state_request(&mut self, peer_id: PeerId, request: OpaqueStateRequest) {
		if !self.peers.contains_key(&peer_id) {
			trace!(target: LOG_TARGET, "Cannot send state request to unknown peer {peer_id}");
			debug_assert!(false);
			return
		}

		let (tx, rx) = oneshot::channel();

		self.pending_responses.insert(peer_id, PeerRequest::State, rx.boxed());

		match Self::encode_state_request(&request) {
			Ok(data) => {
				self.network_service.start_request(
					peer_id,
					self.state_request_protocol_name.clone(),
					data,
					tx,
					IfDisconnected::ImmediateError,
				);
			},
			Err(err) => {
				log::warn!(
					target: LOG_TARGET,
					"Failed to encode state request {request:?}: {err:?}",
				);
			},
		}
	}

	fn send_warp_proof_request(&mut self, peer_id: PeerId, request: WarpProofRequest<B>) {
		if !self.peers.contains_key(&peer_id) {
			trace!(target: LOG_TARGET, "Cannot send warp proof request to unknown peer {peer_id}");
			debug_assert!(false);
			return
		}

		let (tx, rx) = oneshot::channel();

		self.pending_responses.insert(peer_id, PeerRequest::WarpProof, rx.boxed());

		match &self.warp_sync_protocol_name {
			Some(name) => self.network_service.start_request(
				peer_id,
				name.clone(),
				request.encode(),
				tx,
				IfDisconnected::ImmediateError,
			),
			None => {
				log::warn!(
					target: LOG_TARGET,
					"Trying to send warp sync request when no protocol is configured {request:?}",
				);
			},
		}
	}

	fn encode_state_request(request: &OpaqueStateRequest) -> Result<Vec<u8>, String> {
		let request: &StateRequest = request.0.downcast_ref().ok_or_else(|| {
			"Failed to downcast opaque state response during encoding, this is an \
				implementation bug."
				.to_string()
		})?;

		Ok(request.encode_to_vec())
	}

	fn decode_state_response(response: &[u8]) -> Result<OpaqueStateResponse, String> {
		let response = StateResponse::decode(response)
			.map_err(|error| format!("Failed to decode state response: {error}"))?;

		Ok(OpaqueStateResponse(Box::new(response)))
	}

	fn process_response_event(&mut self, response_event: ResponseEvent<B>) {
		let ResponseEvent { peer_id, request, response } = response_event;

		match response {
			Ok(Ok((resp, _))) => match request {
				PeerRequest::Block(req) => {
					match self.block_downloader.block_response_into_blocks(&req, resp) {
						Ok(blocks) => {
							self.strategy.on_block_response(peer_id, req, blocks);
						},
						Err(BlockResponseError::DecodeFailed(e)) => {
							debug!(
								target: LOG_TARGET,
								"Failed to decode block response from peer {:?}: {:?}.",
								peer_id,
								e
							);
							self.network_service.report_peer(peer_id, rep::BAD_MESSAGE);
							self.network_service.disconnect_peer(
								peer_id,
								self.block_announce_protocol_name.clone(),
							);
							return
						},
						Err(BlockResponseError::ExtractionFailed(e)) => {
							debug!(
								target: LOG_TARGET,
								"Failed to extract blocks from peer response {:?}: {:?}.",
								peer_id,
								e
							);
							self.network_service.report_peer(peer_id, rep::BAD_MESSAGE);
							return
						},
					}
				},
				PeerRequest::State => {
					let response = match Self::decode_state_response(&resp[..]) {
						Ok(proto) => proto,
						Err(e) => {
							debug!(
								target: LOG_TARGET,
								"Failed to decode state response from peer {peer_id:?}: {e:?}.",
							);
							self.network_service.report_peer(peer_id, rep::BAD_MESSAGE);
							self.network_service.disconnect_peer(
								peer_id,
								self.block_announce_protocol_name.clone(),
							);
							return
						},
					};

					self.strategy.on_state_response(peer_id, response);
				},
				PeerRequest::WarpProof => {
					self.strategy.on_warp_proof_response(&peer_id, EncodedProof(resp));
				},
			},
			Ok(Err(e)) => {
				debug!(target: LOG_TARGET, "Request to peer {peer_id:?} failed: {e:?}.");

				match e {
					RequestFailure::Network(OutboundFailure::Timeout) => {
						self.network_service.report_peer(peer_id, rep::TIMEOUT);
						self.network_service
							.disconnect_peer(peer_id, self.block_announce_protocol_name.clone());
					},
					RequestFailure::Network(OutboundFailure::UnsupportedProtocols) => {
						self.network_service.report_peer(peer_id, rep::BAD_PROTOCOL);
						self.network_service
							.disconnect_peer(peer_id, self.block_announce_protocol_name.clone());
					},
					RequestFailure::Network(OutboundFailure::DialFailure) => {
						self.network_service
							.disconnect_peer(peer_id, self.block_announce_protocol_name.clone());
					},
					RequestFailure::Refused => {
						self.network_service.report_peer(peer_id, rep::REFUSED);
						self.network_service
							.disconnect_peer(peer_id, self.block_announce_protocol_name.clone());
					},
					RequestFailure::Network(OutboundFailure::ConnectionClosed) |
					RequestFailure::NotConnected => {
						self.network_service
							.disconnect_peer(peer_id, self.block_announce_protocol_name.clone());
					},
					RequestFailure::UnknownProtocol => {
						debug_assert!(false, "Block request protocol should always be known.");
					},
					RequestFailure::Obsolete => {
						debug_assert!(
							false,
							"Can not receive `RequestFailure::Obsolete` after dropping the \
								response receiver.",
						);
					},
				}
			},
			Err(oneshot::Canceled) => {
				trace!(
					target: LOG_TARGET,
					"Request to peer {peer_id:?} failed due to oneshot being canceled.",
				);
				self.network_service
					.disconnect_peer(peer_id, self.block_announce_protocol_name.clone());
			},
		}
	}

	/// Returns the number of peers we're connected to and that are being queried.
	fn num_active_peers(&self) -> usize {
		self.pending_responses.len()
	}

	/// Get config for the block announcement protocol
	fn get_block_announce_proto_config(
		protocol_id: ProtocolId,
		fork_id: &Option<String>,
		roles: Roles,
		best_number: NumberFor<B>,
		best_hash: B::Hash,
		genesis_hash: B::Hash,
	) -> (NonDefaultSetConfig, Box<dyn NotificationService>) {
		let block_announces_protocol = {
			let genesis_hash = genesis_hash.as_ref();
			if let Some(ref fork_id) = fork_id {
				format!(
					"/{}/{}/block-announces/1",
					array_bytes::bytes2hex("", genesis_hash),
					fork_id
				)
			} else {
				format!("/{}/block-announces/1", array_bytes::bytes2hex("", genesis_hash))
			}
		};

		NonDefaultSetConfig::new(
			block_announces_protocol.into(),
			iter::once(format!("/{}/block-announces/1", protocol_id.as_ref()).into()).collect(),
			MAX_BLOCK_ANNOUNCE_SIZE,
			Some(NotificationHandshake::new(BlockAnnouncesHandshake::<B>::build(
				roles,
				best_number,
				best_hash,
				genesis_hash,
			))),
			// NOTE: `set_config` will be ignored by `protocol.rs` as the block announcement
			// protocol is still hardcoded into the peerset.
			SetConfig {
				in_peers: 0,
				out_peers: 0,
				reserved_nodes: Vec::new(),
				non_reserved_mode: NonReservedPeerMode::Deny,
			},
		)
	}

	/// Import blocks.
	fn import_blocks(&mut self, origin: BlockOrigin, blocks: Vec<IncomingBlock<B>>) {
		if let Some(metrics) = &self.metrics {
			metrics.import_queue_blocks_submitted.inc();
		}

		self.import_queue.import_blocks(origin, blocks);
	}

	/// Import justifications.
	fn import_justifications(
		&mut self,
		peer_id: PeerId,
		hash: B::Hash,
		number: NumberFor<B>,
		justifications: Justifications,
	) {
		if let Some(metrics) = &self.metrics {
			metrics.import_queue_justifications_submitted.inc();
		}

		self.import_queue.import_justifications(peer_id, hash, number, justifications);
	}
}
