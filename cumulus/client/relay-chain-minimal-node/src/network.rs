// Copyright 2022 Parity Technologies (UK) Ltd.
// This file is part of Cumulus.

// Cumulus is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Cumulus is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Cumulus.  If not, see <http://www.gnu.org/licenses/>.

use polkadot_core_primitives::{Block, Hash};
use polkadot_service::{BlockT, NumberFor};

use polkadot_node_network_protocol::PeerId;
use sc_network::{NetworkService, SyncState};

use sc_client_api::HeaderBackend;
use sc_network_common::{
	config::{
		NonDefaultSetConfig, NonReservedPeerMode, NotificationHandshake, ProtocolId, SetConfig,
	},
	protocol::role::Roles,
	sync::{message::BlockAnnouncesHandshake, Metrics, SyncStatus},
};
use sc_network_light::light_client_requests;
use sc_network_sync::{block_request_handler, state_request_handler};
use sc_service::{error::Error, Configuration, NetworkStarter, SpawnTaskHandle};
use sp_consensus::BlockOrigin;
use sp_runtime::Justifications;

use std::{iter, sync::Arc};

use crate::BlockChainRpcClient;

pub(crate) struct BuildCollatorNetworkParams<'a> {
	/// The service configuration.
	pub config: &'a Configuration,
	/// A shared client returned by `new_full_parts`.
	pub client: Arc<BlockChainRpcClient>,
	/// A handle for spawning tasks.
	pub spawn_handle: SpawnTaskHandle,
	/// Genesis hash
	pub genesis_hash: Hash,
}

/// Build the network service, the network status sinks and an RPC sender.
pub(crate) fn build_collator_network(
	params: BuildCollatorNetworkParams,
) -> Result<(Arc<NetworkService<Block, Hash>>, NetworkStarter), Error> {
	let BuildCollatorNetworkParams { config, client, spawn_handle, genesis_hash } = params;

	let protocol_id = config.protocol_id();

	let block_request_protocol_config =
		block_request_handler::generate_protocol_config(&protocol_id, genesis_hash, None);

	let state_request_protocol_config =
		state_request_handler::generate_protocol_config(&protocol_id, genesis_hash, None);

	let light_client_request_protocol_config =
		light_client_requests::generate_protocol_config(&protocol_id, genesis_hash, None);

	let chain_sync = DummyChainSync;
	let block_announce_config = chain_sync.get_block_announce_proto_config::<Block>(
		protocol_id.clone(),
		&None,
		Roles::from(&config.role),
		client.info().best_number,
		client.info().best_hash,
		genesis_hash,
	);

	let network_params = sc_network::config::Params {
		role: config.role.clone(),
		executor: {
			let spawn_handle = Clone::clone(&spawn_handle);
			Some(Box::new(move |fut| {
				spawn_handle.spawn("libp2p-node", Some("networking"), fut);
			}))
		},
		fork_id: None,
		chain_sync: Box::new(chain_sync),
		network_config: config.network.clone(),
		chain: client.clone(),
		import_queue: Box::new(DummyImportQueue),
		protocol_id,
		metrics_registry: config.prometheus_config.as_ref().map(|config| config.registry.clone()),
		block_announce_config,
		block_request_protocol_config,
		state_request_protocol_config,
		warp_sync_protocol_config: None,
		light_client_request_protocol_config,
		request_response_protocol_configs: Vec::new(),
	};

	let network_worker = sc_network::NetworkWorker::new(network_params)?;
	let network_service = network_worker.service().clone();

	let (network_start_tx, network_start_rx) = futures::channel::oneshot::channel();

	// The network worker is responsible for gathering all network messages and processing
	// them. This is quite a heavy task, and at the time of the writing of this comment it
	// frequently happens that this future takes several seconds or in some situations
	// even more than a minute until it has processed its entire queue. This is clearly an
	// issue, and ideally we would like to fix the network future to take as little time as
	// possible, but we also take the extra harm-prevention measure to execute the networking
	// future using `spawn_blocking`.
	spawn_handle.spawn_blocking("network-worker", Some("networking"), async move {
		if network_start_rx.await.is_err() {
			tracing::warn!(
				"The NetworkStart returned as part of `build_network` has been silently dropped"
			);
			// This `return` might seem unnecessary, but we don't want to make it look like
			// everything is working as normal even though the user is clearly misusing the API.
			return
		}

		network_worker.await
	});

	let network_starter = NetworkStarter::new(network_start_tx);

	Ok((network_service, network_starter))
}

/// Empty ChainSync shell. Syncing code is not necessary for
/// the minimal node, but network currently requires it. So
/// we provide a noop implementation.
struct DummyChainSync;

impl DummyChainSync {
	pub fn get_block_announce_proto_config<B: BlockT>(
		&self,
		protocol_id: ProtocolId,
		fork_id: &Option<String>,
		roles: Roles,
		best_number: NumberFor<B>,
		best_hash: B::Hash,
		genesis_hash: B::Hash,
	) -> NonDefaultSetConfig {
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

		NonDefaultSetConfig {
			notifications_protocol: block_announces_protocol.into(),
			fallback_names: iter::once(
				format!("/{}/block-announces/1", protocol_id.as_ref()).into(),
			)
			.collect(),
			max_notification_size: 1024 * 1024,
			handshake: Some(NotificationHandshake::new(BlockAnnouncesHandshake::<B>::build(
				roles,
				best_number,
				best_hash,
				genesis_hash,
			))),
			// NOTE: `set_config` will be ignored by `protocol.rs` as the block announcement
			// protocol is still hardcoded into the peerset.
			set_config: SetConfig {
				in_peers: 0,
				out_peers: 0,
				reserved_nodes: Vec::new(),
				non_reserved_mode: NonReservedPeerMode::Deny,
			},
		}
	}
}

impl<B: BlockT> sc_network_common::sync::ChainSync<B> for DummyChainSync {
	fn peer_info(&self, _who: &PeerId) -> Option<sc_network_common::sync::PeerInfo<B>> {
		None
	}

	fn status(&self) -> sc_network_common::sync::SyncStatus<B> {
		SyncStatus {
			state: SyncState::Idle,
			best_seen_block: None,
			num_peers: 0,
			queued_blocks: 0,
			state_sync: None,
			warp_sync: None,
		}
	}

	fn num_sync_requests(&self) -> usize {
		0
	}

	fn num_downloaded_blocks(&self) -> usize {
		0
	}

	fn num_peers(&self) -> usize {
		0
	}

	fn new_peer(
		&mut self,
		_who: PeerId,
		_best_hash: <B as BlockT>::Hash,
		_best_number: polkadot_service::NumberFor<B>,
	) -> Result<
		Option<sc_network_common::sync::message::BlockRequest<B>>,
		sc_network_common::sync::BadPeer,
	> {
		Ok(None)
	}

	fn update_chain_info(
		&mut self,
		_best_hash: &<B as BlockT>::Hash,
		_best_number: polkadot_service::NumberFor<B>,
	) {
	}

	fn request_justification(
		&mut self,
		_hash: &<B as BlockT>::Hash,
		_number: polkadot_service::NumberFor<B>,
	) {
	}

	fn clear_justification_requests(&mut self) {}

	fn set_sync_fork_request(
		&mut self,
		_peers: Vec<PeerId>,
		_hash: &<B as BlockT>::Hash,
		_number: polkadot_service::NumberFor<B>,
	) {
	}

	fn justification_requests(
		&mut self,
	) -> Box<dyn Iterator<Item = (PeerId, sc_network_common::sync::message::BlockRequest<B>)> + '_>
	{
		Box::new(std::iter::empty())
	}

	fn block_requests(
		&mut self,
	) -> Box<dyn Iterator<Item = (PeerId, sc_network_common::sync::message::BlockRequest<B>)> + '_>
	{
		Box::new(std::iter::empty())
	}

	fn state_request(&mut self) -> Option<(PeerId, sc_network_common::sync::OpaqueStateRequest)> {
		None
	}

	fn warp_sync_request(
		&mut self,
	) -> Option<(PeerId, sc_network_common::sync::warp::WarpProofRequest<B>)> {
		None
	}

	fn on_block_data(
		&mut self,
		_who: &PeerId,
		_request: Option<sc_network_common::sync::message::BlockRequest<B>>,
		_response: sc_network_common::sync::message::BlockResponse<B>,
	) -> Result<sc_network_common::sync::OnBlockData<B>, sc_network_common::sync::BadPeer> {
		unimplemented!("Not supported on the RPC collator")
	}

	fn on_state_data(
		&mut self,
		_who: &PeerId,
		_response: sc_network_common::sync::OpaqueStateResponse,
	) -> Result<sc_network_common::sync::OnStateData<B>, sc_network_common::sync::BadPeer> {
		unimplemented!("Not supported on the RPC collator")
	}

	fn on_warp_sync_data(
		&mut self,
		_who: &PeerId,
		_response: sc_network_common::sync::warp::EncodedProof,
	) -> Result<(), sc_network_common::sync::BadPeer> {
		unimplemented!("Not supported on the RPC collator")
	}

	fn on_block_justification(
		&mut self,
		_who: PeerId,
		_response: sc_network_common::sync::message::BlockResponse<B>,
	) -> Result<sc_network_common::sync::OnBlockJustification<B>, sc_network_common::sync::BadPeer>
	{
		unimplemented!("Not supported on the RPC collator")
	}

	fn on_blocks_processed(
		&mut self,
		_imported: usize,
		_count: usize,
		_results: Vec<(
			Result<
				sc_consensus::BlockImportStatus<polkadot_service::NumberFor<B>>,
				sc_consensus::BlockImportError,
			>,
			<B as BlockT>::Hash,
		)>,
	) -> Box<
		dyn Iterator<
			Item = Result<
				(PeerId, sc_network_common::sync::message::BlockRequest<B>),
				sc_network_common::sync::BadPeer,
			>,
		>,
	> {
		Box::new(std::iter::empty())
	}

	fn on_justification_import(
		&mut self,
		_hash: <B as BlockT>::Hash,
		_number: polkadot_service::NumberFor<B>,
		_success: bool,
	) {
	}

	fn on_block_finalized(
		&mut self,
		_hash: &<B as BlockT>::Hash,
		_number: polkadot_service::NumberFor<B>,
	) {
	}

	fn push_block_announce_validation(
		&mut self,
		_who: PeerId,
		_hash: <B as BlockT>::Hash,
		_announce: sc_network_common::sync::message::BlockAnnounce<<B as BlockT>::Header>,
		_is_best: bool,
	) {
	}

	fn poll_block_announce_validation(
		&mut self,
		_cx: &mut std::task::Context,
	) -> std::task::Poll<sc_network_common::sync::PollBlockAnnounceValidation<<B as BlockT>::Header>>
	{
		std::task::Poll::Pending
	}

	fn peer_disconnected(
		&mut self,
		_who: &PeerId,
	) -> Option<sc_network_common::sync::OnBlockData<B>> {
		None
	}

	fn metrics(&self) -> sc_network_common::sync::Metrics {
		Metrics {
			queued_blocks: 0,
			fork_targets: 0,
			justifications: sc_network_common::sync::metrics::Metrics {
				pending_requests: 0,
				active_requests: 0,
				importing_requests: 0,
				failed_requests: 0,
			},
		}
	}

	fn create_opaque_block_request(
		&self,
		_request: &sc_network_common::sync::message::BlockRequest<B>,
	) -> sc_network_common::sync::OpaqueBlockRequest {
		unimplemented!("Not supported on the RPC collator")
	}

	fn encode_block_request(
		&self,
		_request: &sc_network_common::sync::OpaqueBlockRequest,
	) -> Result<Vec<u8>, String> {
		unimplemented!("Not supported on the RPC collator")
	}

	fn decode_block_response(
		&self,
		_response: &[u8],
	) -> Result<sc_network_common::sync::OpaqueBlockResponse, String> {
		unimplemented!("Not supported on the RPC collator")
	}

	fn block_response_into_blocks(
		&self,
		_request: &sc_network_common::sync::message::BlockRequest<B>,
		_response: sc_network_common::sync::OpaqueBlockResponse,
	) -> Result<Vec<sc_network_common::sync::message::BlockData<B>>, String> {
		unimplemented!("Not supported on the RPC collator")
	}

	fn encode_state_request(
		&self,
		_request: &sc_network_common::sync::OpaqueStateRequest,
	) -> Result<Vec<u8>, String> {
		unimplemented!("Not supported on the RPC collator")
	}

	fn decode_state_response(
		&self,
		_response: &[u8],
	) -> Result<sc_network_common::sync::OpaqueStateResponse, String> {
		unimplemented!("Not supported on the RPC collator")
	}
}

struct DummyImportQueue;

impl sc_service::ImportQueue<Block> for DummyImportQueue {
	fn import_blocks(
		&mut self,
		_origin: BlockOrigin,
		_blocks: Vec<sc_consensus::IncomingBlock<Block>>,
	) {
	}

	fn import_justifications(
		&mut self,
		_who: PeerId,
		_hash: Hash,
		_number: NumberFor<Block>,
		_justifications: Justifications,
	) {
	}

	fn poll_actions(
		&mut self,
		_cx: &mut futures::task::Context,
		_link: &mut dyn sc_consensus::import_queue::Link<Block>,
	) {
	}
}
