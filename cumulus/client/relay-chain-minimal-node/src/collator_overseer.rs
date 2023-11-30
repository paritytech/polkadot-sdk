// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Polkadot.

// Polkadot is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Polkadot is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Polkadot.  If not, see <http://www.gnu.org/licenses/>.

use futures::{select, StreamExt};
use parking_lot::Mutex;
use std::{collections::HashMap, sync::Arc};

use polkadot_availability_recovery::AvailabilityRecoverySubsystem;
use polkadot_collator_protocol::{CollatorProtocolSubsystem, ProtocolSide};
use polkadot_network_bridge::{
	Metrics as NetworkBridgeMetrics, NetworkBridgeRx as NetworkBridgeRxSubsystem,
	NetworkBridgeTx as NetworkBridgeTxSubsystem,
};
use polkadot_node_collation_generation::CollationGenerationSubsystem;
use polkadot_node_core_chain_api::ChainApiSubsystem;
use polkadot_node_core_prospective_parachains::ProspectiveParachainsSubsystem;
use polkadot_node_core_runtime_api::RuntimeApiSubsystem;
use polkadot_node_network_protocol::{
	peer_set::{PeerSet, PeerSetProtocolNames},
	request_response::{
		v1::{self, AvailableDataFetchingRequest},
		v2, IncomingRequestReceiver, ReqProtocolNames,
	},
};
use polkadot_node_subsystem_util::metrics::{prometheus::Registry, Metrics};
use polkadot_overseer::{
	BlockInfo, DummySubsystem, Handle, Overseer, OverseerConnector, OverseerHandle, SpawnGlue,
	UnpinHandle,
};
use polkadot_primitives::CollatorPair;

use sc_authority_discovery::Service as AuthorityDiscoveryService;
use sc_network::{NetworkStateInfo, NotificationService};
use sc_service::TaskManager;
use sc_utils::mpsc::tracing_unbounded;

use cumulus_primitives_core::relay_chain::{Block, Hash as PHash};
use cumulus_relay_chain_interface::RelayChainError;

use crate::BlockChainRpcClient;

/// Arguments passed for overseer construction.
pub(crate) struct CollatorOverseerGenArgs<'a> {
	/// Runtime client generic, providing the `ProvieRuntimeApi` trait besides others.
	pub runtime_client: Arc<BlockChainRpcClient>,
	/// Underlying network service implementation.
	pub network_service: Arc<sc_network::NetworkService<Block, PHash>>,
	/// Syncing oracle.
	pub sync_oracle: Box<dyn sp_consensus::SyncOracle + Send>,
	/// Underlying authority discovery service.
	pub authority_discovery_service: AuthorityDiscoveryService,
	/// Receiver for collation request protocol v1.
	pub collation_req_receiver_v1: IncomingRequestReceiver<v1::CollationFetchingRequest>,
	/// Receiver for collation request protocol v2.
	pub collation_req_receiver_v2: IncomingRequestReceiver<v2::CollationFetchingRequest>,
	/// Receiver for availability request protocol
	pub available_data_req_receiver: IncomingRequestReceiver<AvailableDataFetchingRequest>,
	/// Prometheus registry, commonly used for production systems, less so for test.
	pub registry: Option<&'a Registry>,
	/// Task spawner to be used throughout the overseer and the APIs it provides.
	pub spawner: sc_service::SpawnTaskHandle,
	/// Determines the behavior of the collator.
	pub collator_pair: CollatorPair,
	/// Request response protocols
	pub req_protocol_names: ReqProtocolNames,
	/// Peerset protocols name mapping
	pub peer_set_protocol_names: PeerSetProtocolNames,
	/// Notification services for validation/collation protocols.
	pub notification_services: HashMap<PeerSet, Box<dyn NotificationService>>,
}

fn build_overseer(
	connector: OverseerConnector,
	CollatorOverseerGenArgs {
		runtime_client,
		network_service,
		sync_oracle,
		authority_discovery_service,
		collation_req_receiver_v1,
		collation_req_receiver_v2,
		available_data_req_receiver,
		registry,
		spawner,
		collator_pair,
		req_protocol_names,
		peer_set_protocol_names,
		notification_services,
	}: CollatorOverseerGenArgs<'_>,
) -> Result<
	(Overseer<SpawnGlue<sc_service::SpawnTaskHandle>, Arc<BlockChainRpcClient>>, OverseerHandle),
	RelayChainError,
> {
	let spawner = SpawnGlue(spawner);
	let network_bridge_metrics: NetworkBridgeMetrics = Metrics::register(registry)?;
	let notification_sinks = Arc::new(Mutex::new(HashMap::new()));

	let builder = Overseer::builder()
		.availability_distribution(DummySubsystem)
		.availability_recovery(AvailabilityRecoverySubsystem::for_collator(
			available_data_req_receiver,
			Metrics::register(registry)?,
		))
		.availability_store(DummySubsystem)
		.bitfield_distribution(DummySubsystem)
		.bitfield_signing(DummySubsystem)
		.candidate_backing(DummySubsystem)
		.candidate_validation(DummySubsystem)
		.pvf_checker(DummySubsystem)
		.chain_api(ChainApiSubsystem::new(runtime_client.clone(), Metrics::register(registry)?))
		.collation_generation(CollationGenerationSubsystem::new(Metrics::register(registry)?))
		.collator_protocol({
			let side = ProtocolSide::Collator {
				peer_id: network_service.local_peer_id(),
				collator_pair,
				request_receiver_v1: collation_req_receiver_v1,
				request_receiver_v2: collation_req_receiver_v2,
				metrics: Metrics::register(registry)?,
			};
			CollatorProtocolSubsystem::new(side)
		})
		.network_bridge_rx(NetworkBridgeRxSubsystem::new(
			network_service.clone(),
			authority_discovery_service.clone(),
			sync_oracle,
			network_bridge_metrics.clone(),
			peer_set_protocol_names.clone(),
			notification_services,
			notification_sinks.clone(),
		))
		.network_bridge_tx(NetworkBridgeTxSubsystem::new(
			network_service,
			authority_discovery_service,
			network_bridge_metrics,
			req_protocol_names,
			peer_set_protocol_names,
			notification_sinks,
		))
		.provisioner(DummySubsystem)
		.runtime_api(RuntimeApiSubsystem::new(
			runtime_client.clone(),
			Metrics::register(registry)?,
			spawner.clone(),
		))
		.statement_distribution(DummySubsystem)
		.prospective_parachains(ProspectiveParachainsSubsystem::new(Metrics::register(registry)?))
		.approval_distribution(DummySubsystem)
		.approval_voting(DummySubsystem)
		.gossip_support(DummySubsystem)
		.dispute_coordinator(DummySubsystem)
		.dispute_distribution(DummySubsystem)
		.chain_selection(DummySubsystem)
		.activation_external_listeners(Default::default())
		.span_per_active_leaf(Default::default())
		.active_leaves(Default::default())
		.supports_parachains(runtime_client)
		.metrics(Metrics::register(registry)?)
		.spawner(spawner);

	builder
		.build_with_connector(connector)
		.map_err(|e| RelayChainError::Application(e.into()))
}

pub(crate) fn spawn_overseer(
	overseer_args: CollatorOverseerGenArgs,
	task_manager: &TaskManager,
	relay_chain_rpc_client: Arc<BlockChainRpcClient>,
) -> Result<polkadot_overseer::Handle, RelayChainError> {
	let (overseer, overseer_handle) = build_overseer(OverseerConnector::default(), overseer_args)
		.map_err(|e| {
		tracing::error!("Failed to initialize overseer: {}", e);
		e
	})?;

	let overseer_handle = Handle::new(overseer_handle);
	{
		let handle = overseer_handle.clone();
		task_manager.spawn_essential_handle().spawn_blocking(
			"overseer",
			None,
			Box::pin(async move {
				use futures::{pin_mut, FutureExt};

				let forward = forward_collator_events(relay_chain_rpc_client, handle).fuse();

				let overseer_fut = overseer.run().fuse();

				pin_mut!(overseer_fut);
				pin_mut!(forward);

				select! {
					_ = forward => (),
					_ = overseer_fut => (),
				}
			}),
		);
	}
	Ok(overseer_handle)
}

/// Minimal relay chain node representation
pub struct NewMinimalNode {
	/// Task manager running all tasks for the minimal node
	pub task_manager: TaskManager,
	/// Overseer handle to interact with subsystems
	pub overseer_handle: Handle,
}

/// Glues together the [`Overseer`] and `BlockchainEvents` by forwarding
/// import and finality notifications into the [`OverseerHandle`].
async fn forward_collator_events(
	client: Arc<BlockChainRpcClient>,
	mut handle: Handle,
) -> Result<(), RelayChainError> {
	let mut finality = client.finality_notification_stream().await?.fuse();
	let mut imports = client.import_notification_stream().await?.fuse();
	// Collators do no need to pin any specific blocks
	let (dummy_sink, _) = tracing_unbounded("does-not-matter", 42);
	let dummy_unpin_handle = UnpinHandle::new(Default::default(), dummy_sink);

	loop {
		select! {
			f = finality.next() => {
				match f {
					Some(header) => {
						let hash = header.hash();
						tracing::info!(
							target: "minimal-polkadot-node",
							"Received finalized block via RPC: #{} ({} -> {})",
							header.number,
							header.parent_hash,
							hash,
						);
						let unpin_handle = dummy_unpin_handle.clone();
						let block_info = BlockInfo { hash, parent_hash: header.parent_hash, number: header.number, unpin_handle };
						handle.block_finalized(block_info).await;
					}
					None => return Err(RelayChainError::GenericError("Relay chain finality stream ended.".to_string())),
				}
			},
			i = imports.next() => {
				match i {
					Some(header) => {
						let hash = header.hash();
						tracing::info!(
							target: "minimal-polkadot-node",
							"Received imported block via RPC: #{} ({} -> {})",
							header.number,
							header.parent_hash,
							hash,
						);
						let unpin_handle = dummy_unpin_handle.clone();
						let block_info = BlockInfo { hash, parent_hash: header.parent_hash, number: header.number, unpin_handle };
						handle.block_imported(block_info).await;
					}
					None => return Err(RelayChainError::GenericError("Relay chain import stream ended.".to_string())),
				}
			}
		}
	}
}
