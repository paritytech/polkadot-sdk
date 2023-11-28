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

use collator_overseer::{CollatorOverseerGenArgs, NewMinimalNode};

use cumulus_relay_chain_interface::{RelayChainError, RelayChainInterface, RelayChainResult};
use cumulus_relay_chain_rpc_interface::{RelayChainRpcClient, RelayChainRpcInterface, Url};
use network::build_collator_network;
use polkadot_network_bridge::{peer_sets_info, IsAuthority};
use polkadot_node_network_protocol::{
	peer_set::{PeerSet, PeerSetProtocolNames},
	request_response::{
		v1, v2, IncomingRequest, IncomingRequestReceiver, Protocol, ReqProtocolNames,
	},
};

use polkadot_node_subsystem_util::metrics::prometheus::Registry;
use polkadot_primitives::CollatorPair;

use sc_authority_discovery::Service as AuthorityDiscoveryService;
use sc_network::{config::FullNetworkConfiguration, Event, NetworkEventStream, NetworkService};
use sc_service::{config::PrometheusConfig, Configuration, TaskManager};
use sp_runtime::{app_crypto::Pair, traits::Block as BlockT};

use futures::{FutureExt, StreamExt};
use std::sync::Arc;

mod blockchain_rpc_client;
mod collator_overseer;
mod network;

pub use blockchain_rpc_client::BlockChainRpcClient;

const LOG_TARGET: &str = "minimal-relaychain-node";

fn build_authority_discovery_service<Block: BlockT>(
	task_manager: &TaskManager,
	client: Arc<BlockChainRpcClient>,
	config: &Configuration,
	network: Arc<NetworkService<Block, <Block as BlockT>::Hash>>,
	prometheus_registry: Option<Registry>,
) -> AuthorityDiscoveryService {
	let auth_disc_publish_non_global_ips = config.network.allow_non_globals_in_dht;
	let authority_discovery_role = sc_authority_discovery::Role::Discover;
	let dht_event_stream = network.event_stream("authority-discovery").filter_map(|e| async move {
		match e {
			Event::Dht(e) => Some(e),
			_ => None,
		}
	});
	let (worker, service) = sc_authority_discovery::new_worker_and_service_with_config(
		sc_authority_discovery::WorkerConfig {
			publish_non_global_ips: auth_disc_publish_non_global_ips,
			// Require that authority discovery records are signed.
			strict_record_validation: true,
			..Default::default()
		},
		client,
		network,
		Box::pin(dht_event_stream),
		authority_discovery_role,
		prometheus_registry,
	);

	task_manager.spawn_handle().spawn(
		"authority-discovery-worker",
		Some("authority-discovery"),
		worker.run(),
	);
	service
}

async fn build_interface(
	polkadot_config: Configuration,
	task_manager: &mut TaskManager,
	client: RelayChainRpcClient,
) -> RelayChainResult<(Arc<(dyn RelayChainInterface + 'static)>, Option<CollatorPair>)> {
	let collator_pair = CollatorPair::generate().0;
	let collator_node = new_minimal_relay_chain(
		polkadot_config,
		collator_pair.clone(),
		Arc::new(BlockChainRpcClient::new(client.clone())),
	)
	.await?;
	task_manager.add_child(collator_node.task_manager);
	Ok((
		Arc::new(RelayChainRpcInterface::new(client, collator_node.overseer_handle)),
		Some(collator_pair),
	))
}

pub async fn build_minimal_relay_chain_node_with_rpc(
	polkadot_config: Configuration,
	task_manager: &mut TaskManager,
	relay_chain_url: Vec<Url>,
) -> RelayChainResult<(Arc<(dyn RelayChainInterface + 'static)>, Option<CollatorPair>)> {
	let client = cumulus_relay_chain_rpc_interface::create_client_and_start_worker(
		relay_chain_url,
		task_manager,
	)
	.await?;

	build_interface(polkadot_config, task_manager, client).await
}

pub async fn build_minimal_relay_chain_node_light_client(
	polkadot_config: Configuration,
	task_manager: &mut TaskManager,
) -> RelayChainResult<(Arc<(dyn RelayChainInterface + 'static)>, Option<CollatorPair>)> {
	tracing::info!(
		target: LOG_TARGET,
		chain_name = polkadot_config.chain_spec.name(),
		chain_id = polkadot_config.chain_spec.id(),
		"Initializing embedded light client with chain spec."
	);

	let spec = polkadot_config
		.chain_spec
		.as_json(false)
		.map_err(RelayChainError::GenericError)?;

	let client = cumulus_relay_chain_rpc_interface::create_client_and_start_light_client_worker(
		spec,
		task_manager,
	)
	.await?;

	build_interface(polkadot_config, task_manager, client).await
}
/// Builds a minimal relay chain node. Chain data is fetched
/// via [`BlockChainRpcClient`] and fed into the overseer and its subsystems.
///
/// Instead of spawning all subsystems, this minimal node will only spawn subsystems
/// required to collate:
/// - AvailabilityRecovery
/// - CollationGeneration
/// - CollatorProtocol
/// - NetworkBridgeRx
/// - NetworkBridgeTx
/// - RuntimeApi
#[sc_tracing::logging::prefix_logs_with("Relaychain")]
async fn new_minimal_relay_chain(
	config: Configuration,
	collator_pair: CollatorPair,
	relay_chain_rpc_client: Arc<BlockChainRpcClient>,
) -> Result<NewMinimalNode, RelayChainError> {
	let role = config.role.clone();
	let mut net_config = sc_network::config::FullNetworkConfiguration::new(&config.network);

	let prometheus_registry = config.prometheus_registry();
	let task_manager = TaskManager::new(config.tokio_handle.clone(), prometheus_registry)?;

	if let Some(PrometheusConfig { port, registry }) = config.prometheus_config.clone() {
		task_manager.spawn_handle().spawn(
			"prometheus-endpoint",
			None,
			substrate_prometheus_endpoint::init_prometheus(port, registry).map(drop),
		);
	}

	let genesis_hash = relay_chain_rpc_client.block_get_hash(Some(0)).await?.unwrap_or_default();
	let peer_set_protocol_names =
		PeerSetProtocolNames::new(genesis_hash, config.chain_spec.fork_id());
	let is_authority = if role.is_authority() { IsAuthority::Yes } else { IsAuthority::No };
	let notification_services = peer_sets_info(is_authority, &peer_set_protocol_names)
		.into_iter()
		.map(|(config, (peerset, service))| {
			net_config.add_notification_protocol(config);
			(peerset, service)
		})
		.collect::<std::collections::HashMap<PeerSet, Box<dyn sc_network::NotificationService>>>();

	let request_protocol_names = ReqProtocolNames::new(genesis_hash, config.chain_spec.fork_id());
	let (collation_req_receiver_v1, collation_req_receiver_v2, available_data_req_receiver) =
		build_request_response_protocol_receivers(&request_protocol_names, &mut net_config);

	let best_header = relay_chain_rpc_client
		.chain_get_header(None)
		.await?
		.ok_or_else(|| RelayChainError::RpcCallError("Unable to fetch best header".to_string()))?;
	let (network, network_starter, sync_oracle) = build_collator_network(
		&config,
		net_config,
		task_manager.spawn_handle(),
		genesis_hash,
		best_header,
	)
	.map_err(|e| RelayChainError::Application(Box::new(e) as Box<_>))?;

	let authority_discovery_service = build_authority_discovery_service(
		&task_manager,
		relay_chain_rpc_client.clone(),
		&config,
		network.clone(),
		prometheus_registry.cloned(),
	);

	let overseer_args = CollatorOverseerGenArgs {
		runtime_client: relay_chain_rpc_client.clone(),
		network_service: network,
		sync_oracle,
		authority_discovery_service,
		collation_req_receiver_v1,
		collation_req_receiver_v2,
		available_data_req_receiver,
		registry: prometheus_registry,
		spawner: task_manager.spawn_handle(),
		collator_pair,
		req_protocol_names: request_protocol_names,
		peer_set_protocol_names,
		notification_services,
	};

	let overseer_handle =
		collator_overseer::spawn_overseer(overseer_args, &task_manager, relay_chain_rpc_client)?;

	network_starter.start_network();

	Ok(NewMinimalNode { task_manager, overseer_handle })
}

fn build_request_response_protocol_receivers(
	request_protocol_names: &ReqProtocolNames,
	config: &mut FullNetworkConfiguration,
) -> (
	IncomingRequestReceiver<v1::CollationFetchingRequest>,
	IncomingRequestReceiver<v2::CollationFetchingRequest>,
	IncomingRequestReceiver<v1::AvailableDataFetchingRequest>,
) {
	let (collation_req_receiver_v1, cfg) =
		IncomingRequest::get_config_receiver(request_protocol_names);
	config.add_request_response_protocol(cfg);
	let (collation_req_receiver_v2, cfg) =
		IncomingRequest::get_config_receiver(request_protocol_names);
	config.add_request_response_protocol(cfg);
	let (available_data_req_receiver, cfg) =
		IncomingRequest::get_config_receiver(request_protocol_names);
	config.add_request_response_protocol(cfg);
	let cfg = Protocol::ChunkFetchingV1.get_outbound_only_config(request_protocol_names);
	config.add_request_response_protocol(cfg);
	(collation_req_receiver_v1, collation_req_receiver_v2, available_data_req_receiver)
}
