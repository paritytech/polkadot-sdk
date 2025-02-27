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

use crate::{
	availability::av_store_helpers::new_av_store,
	dummy_builder,
	environment::{TestEnvironment, TestEnvironmentDependencies},
	mock::{
		av_store::{MockAvailabilityStore, NetworkAvailabilityState},
		chain_api::{ChainApiState, MockChainApi},
		runtime_api::{default_node_features, MockRuntimeApi, MockRuntimeApiCoreState},
		AlwaysSupportsParachains,
	},
	network::new_network,
	network_utils,
	usage::BenchmarkUsage,
	NODE_UNDER_TEST,
};
use codec::{Decode, Encode};
use colored::Colorize;
use futures::{channel::oneshot, stream::FuturesUnordered, StreamExt};
use polkadot_availability_bitfield_distribution::BitfieldDistribution;
use polkadot_availability_distribution::{
	AvailabilityDistributionSubsystem, IncomingRequestReceivers,
};
use polkadot_availability_recovery::{AvailabilityRecoverySubsystem, RecoveryStrategyKind};
use polkadot_network_bridge::WireMessage;
use polkadot_node_core_av_store::AvailabilityStoreSubsystem;
use polkadot_node_metrics::metrics::Metrics;
use polkadot_node_network_protocol::{
	peer_set::PeerSet,
	request_response::{v1, v2, IncomingRequest},
	v3, OurView,
};
use polkadot_node_subsystem::{
	messages::{AllMessages, AvailabilityRecoveryMessage},
	Overseer, OverseerConnector, SpawnGlue,
};
use polkadot_node_subsystem_types::messages::{AvailabilityStoreMessage, NetworkBridgeEvent};
use polkadot_overseer::{metrics::Metrics as OverseerMetrics, Handle as OverseerHandle};
use polkadot_primitives::{CoreIndex, GroupIndex};
use polkadot_service::overseer::{
	NetworkBridgeMetrics, NetworkBridgeRxSubsystem, NetworkBridgeTxSubsystem,
};
use sc_network::{
	config::{MultiaddrWithPeerId, NodeKeyConfig, ProtocolId, Role},
	request_responses::{IncomingRequest as RawIncomingRequest, ProtocolConfig},
	service::traits::{NetworkService, NotificationEvent, ValidationResult},
	NetworkBackend, NotificationMetrics,
};
use sc_service::SpawnTaskHandle;
use serde::{Deserialize, Serialize};
use sp_core::{Bytes, H256};
use sp_runtime::traits::Block as BlockT;
use std::{
	collections::HashMap,
	ops::Sub,
	sync::Arc,
	time::{Duration, Instant},
};
use strum::Display;
pub use test_state::TestState;

use self::network_utils::DummyAuthotiryDiscoveryService;

mod av_store_helpers;
mod test_state;

const LOG_TARGET: &str = "subsystem-bench::availability";

#[derive(clap::ValueEnum, Clone, Copy, Debug, PartialEq, Serialize, Deserialize, Display)]
#[value(rename_all = "kebab-case")]
#[strum(serialize_all = "kebab-case")]
pub enum Strategy {
	/// Regular random chunk recovery. This is also the fallback for the next strategies.
	Chunks,
	/// Recovery from systematic chunks. Much faster than regular chunk recovery becasue it avoid
	/// doing the reed-solomon reconstruction.
	Systematic,
	/// Fetch the full availability datafrom backers first. Saves CPU as we don't need to
	/// re-construct from chunks. Typically this is only faster if nodes have enough bandwidth.
	FullFromBackers,
}

#[derive(Debug, Clone, Serialize, Deserialize, clap::Parser)]
#[clap(rename_all = "kebab-case")]
#[allow(missing_docs)]
pub struct DataAvailabilityReadOptions {
	#[clap(short, long, default_value_t = Strategy::Systematic)]
	pub strategy: Strategy,
}

pub enum TestDataAvailability {
	Read(DataAvailabilityReadOptions),
	Write,
}

fn build_overseer_for_availability_read(
	spawn_task_handle: SpawnTaskHandle,
	runtime_api: MockRuntimeApi,
	av_store: MockAvailabilityStore,
	(network_bridge_tx, network_bridge_rx): (
		NetworkBridgeTxSubsystem<Arc<dyn NetworkService>, DummyAuthotiryDiscoveryService>,
		NetworkBridgeRxSubsystem<Arc<dyn NetworkService>, DummyAuthotiryDiscoveryService>,
	),
	availability_recovery: AvailabilityRecoverySubsystem,
	dependencies: &TestEnvironmentDependencies,
) -> (Overseer<SpawnGlue<SpawnTaskHandle>, AlwaysSupportsParachains>, OverseerHandle) {
	let overseer_connector = OverseerConnector::with_event_capacity(64000);
	let overseer_metrics = OverseerMetrics::try_register(&dependencies.registry).unwrap();

	let dummy = dummy_builder!(spawn_task_handle, overseer_metrics);
	let builder = dummy
		.replace_runtime_api(|_| runtime_api)
		.replace_availability_store(|_| av_store)
		.replace_network_bridge_tx(|_| network_bridge_tx)
		.replace_network_bridge_rx(|_| network_bridge_rx)
		.replace_availability_recovery(|_| availability_recovery);

	let (overseer, raw_handle) =
		builder.build_with_connector(overseer_connector).expect("Should not fail");

	(overseer, OverseerHandle::new(raw_handle))
}

#[allow(clippy::too_many_arguments)]
fn build_overseer_for_availability_write(
	spawn_task_handle: SpawnTaskHandle,
	runtime_api: MockRuntimeApi,
	(network_bridge_tx, network_bridge_rx): (
		NetworkBridgeTxSubsystem<Arc<dyn NetworkService>, DummyAuthotiryDiscoveryService>,
		NetworkBridgeRxSubsystem<Arc<dyn NetworkService>, DummyAuthotiryDiscoveryService>,
	),
	availability_distribution: AvailabilityDistributionSubsystem,
	chain_api: MockChainApi,
	availability_store: AvailabilityStoreSubsystem,
	bitfield_distribution: BitfieldDistribution,
	dependencies: &TestEnvironmentDependencies,
) -> (Overseer<SpawnGlue<SpawnTaskHandle>, AlwaysSupportsParachains>, OverseerHandle) {
	let overseer_connector = OverseerConnector::with_event_capacity(64000);
	let overseer_metrics = OverseerMetrics::try_register(&dependencies.registry).unwrap();

	let dummy = dummy_builder!(spawn_task_handle, overseer_metrics);
	let builder = dummy
		.replace_runtime_api(|_| runtime_api)
		.replace_availability_store(|_| availability_store)
		.replace_network_bridge_tx(|_| network_bridge_tx)
		.replace_network_bridge_rx(|_| network_bridge_rx)
		.replace_chain_api(|_| chain_api)
		.replace_bitfield_distribution(|_| bitfield_distribution)
		// This is needed to test own chunk recovery for `n_cores`.
		.replace_availability_distribution(|_| availability_distribution);

	let (overseer, raw_handle) =
		builder.build_with_connector(overseer_connector).expect("Should not fail");

	(overseer, OverseerHandle::new(raw_handle))
}

pub fn prepare_test<B, N>(
	state: &TestState,
	mode: TestDataAvailability,
	with_prometheus_endpoint: bool,
) -> (TestEnvironment, Vec<ProtocolConfig>)
where
	B: BlockT<Hash = H256> + 'static,
	N: NetworkBackend<B, B::Hash>,
{
	let dependencies = TestEnvironmentDependencies::default();
	let availability_state = NetworkAvailabilityState {
		candidate_hashes: state.candidate_hashes.clone(),
		candidate_hash_to_core_index: state.candidate_hash_to_core_index.clone(),
		available_data: state.available_data.clone(),
		chunks: state.chunks.clone(),
		chunk_indices: state.chunk_indices.clone(),
		req_protocol_names: state.req_protocol_names.clone(),
	};
	let (network, _network_interface, _network_receiver) = new_network(
		&state.config,
		&dependencies,
		&state.test_authorities,
		vec![Arc::new(availability_state.clone())],
	);
	let (overseer, overseer_handle, req_cfgs, peer_id, listen_address) =
		build_overseer::<B, N>(state, &dependencies, mode);

	(0..state.config.n_validators)
		.filter(|i| *i != NODE_UNDER_TEST as usize)
		.for_each(|i| {
			build_peer::<B, N>(state, &dependencies, i as u16, peer_id, listen_address.clone())
		});

	(
		TestEnvironment::new(
			dependencies,
			state.config.clone(),
			network,
			overseer,
			overseer_handle,
			state.test_authorities.clone(),
			with_prometheus_endpoint,
		),
		req_cfgs,
	)
}

fn build_peer<B, N>(
	state: &TestState,
	dependencies: &TestEnvironmentDependencies,
	index: u16,
	node_peer_id: sc_network::PeerId,
	node_multiaddr: sc_network::Multiaddr,
) where
	B: BlockT<Hash = H256> + 'static,
	N: NetworkBackend<B, B::Hash>,
{
	let _guard = dependencies.runtime.handle().enter();

	let node_key: NodeKeyConfig = state.test_authorities.node_key_configs[index as usize].clone();
	let listen_addr = state.test_authorities.authority_id_to_addr
		[&state.test_authorities.validator_authority_id[index as usize]]
		.clone();
	let role = Role::Authority;
	let protocol_id = ProtocolId::from("sup");
	let notification_metrics = NotificationMetrics::new(None);

	let mut network_config =
		network_utils::build_network_config::<B, N>(node_key, listen_addr, None);

	let (mut collation_req_receiver, collation_req_cfg) =
		IncomingRequest::<v1::AvailableDataFetchingRequest>::get_config_receiver::<B, N>(
			&state.req_protocol_names,
		);
	network_config.add_request_response_protocol(collation_req_cfg);

	let (mut pov_req_receiver, pov_req_cfg) =
		IncomingRequest::<v1::PoVFetchingRequest>::get_config_receiver::<B, N>(
			&state.req_protocol_names,
		);
	network_config.add_request_response_protocol(pov_req_cfg);

	let (mut chunk_req_v1_receiver, chunk_req_v1_cfg) =
		IncomingRequest::<v1::ChunkFetchingRequest>::get_config_receiver::<B, N>(
			&state.req_protocol_names,
		);
	network_config.add_request_response_protocol(chunk_req_v1_cfg);

	let (mut chunk_req_v2_receiver, chunk_req_v2_cfg) =
		IncomingRequest::<v2::ChunkFetchingRequest>::get_config_receiver::<B, N>(
			&state.req_protocol_names,
		);
	network_config.add_request_response_protocol(chunk_req_v2_cfg);

	let (_peer_set_protocol_names, mut peer_set_services) =
		network_utils::build_peer_set_services::<B, N>(&mut network_config, &notification_metrics);
	let mut validation_service = peer_set_services
		.remove(&PeerSet::Validation)
		.expect("validation protocol was enabled so `NotificationService` must exist; qed");
	let mut collation_service = peer_set_services
		.remove(&PeerSet::Collation)
		.expect("collation protocol was enabled so `NotificationService` must exist; qed");
	assert!(peer_set_services.is_empty());

	let (worker, network_service, mut block_announce_service) =
		network_utils::build_network_worker::<B, N>(
			dependencies,
			network_config,
			role,
			protocol_id,
			notification_metrics,
			None,
			"test-environment",
		);

	let spawn_handle = dependencies.task_manager.spawn_handle();
	let peer_worker_name = Box::leak(format!("Peer {} worker", index).into_boxed_str());
	spawn_handle.spawn(peer_worker_name, "test-environment", worker.run());
	let peer_notifications_name =
		Box::leak(format!("Peer {} notifications", index).into_boxed_str());
	spawn_handle.spawn(peer_notifications_name, "test-environment", {
		async move {
			loop {
				tokio::select! {
					event = block_announce_service.next_event() => {
						if let Some(NotificationEvent::ValidateInboundSubstream { result_tx, .. }) = event {
							result_tx.send(ValidationResult::Accept).unwrap();
						}
					},
					event = validation_service.next_event() => {},
					event = collation_service.next_event() => {
						gum::error!(target: LOG_TARGET, "Peer {} received collation {:?}", index, event);
					},
				}
			}
		}
	});
	let peer_requests_name = Box::leak(format!("Peer {} requests", index).into_boxed_str());
	spawn_handle.spawn(peer_requests_name, "test-environment", {
		let state = state.clone();
		async move {
			loop {
				tokio::select! {
					req = collation_req_receiver.recv(Vec::new) => {
						let req = req.unwrap();
						let payload = req.payload;
						gum::debug!(target: LOG_TARGET, "Peer {} received AvailableDataFetchingRequest", index);

						let candidate_hash = payload.candidate_hash;
						let candidate_index = state.candidate_hash_to_core_index
							.get(&candidate_hash)
							.expect("candidate was generated previously; qed");
						gum::debug!(target: LOG_TARGET, ?candidate_hash, ?candidate_index, "Candidate mapped to index");

						let available_data = state.available_data.get(candidate_index.0 as usize).unwrap().clone();
						req
							.pending_response
							.send_response(v1::AvailableDataFetchingResponse::from(Some(available_data)))
							.expect("Response is always sent successfully");

					},
					// req = pov_req_receiver.recv(Vec::new) => {
					// 	let req = req.unwrap();
					// 	let payload = req.payload;
					// 	gum::debug!(target: LOG_TARGET, "Peer {} received PoVFetchingRequest", index);
					// },
					// req = chunk_req_v1_receiver.recv(Vec::new) => {
					// 	let req = req.unwrap();
					// 	let payload = req.payload;
					// 	gum::debug!(target: LOG_TARGET, "Peer {} received ChunkFetchingRequest", index);
					// },
					// req = chunk_req_v2_receiver.recv(Vec::new) => {
					// 	let req = req.unwrap();
					// 	let payload = req.payload;
					// 	gum::debug!(target: LOG_TARGET, "Peer {} received ChunkFetchingRequest", index);
					// },

				}
			}
		}
	});

	network_service
		.add_reserved_peer(MultiaddrWithPeerId { peer_id: node_peer_id, multiaddr: node_multiaddr })
		.unwrap();

	let ready = tokio::spawn({
		let network_service = Arc::clone(&network_service);
		async move {
			while network_service.listen_addresses().is_empty() {
				tokio::time::sleep(Duration::from_millis(10)).await;
			}
		}
	});
	let _ = tokio::task::block_in_place(|| tokio::runtime::Handle::current().block_on(ready));

	let local_peer_id = network_service.local_peer_id();
	let listen_addresses = network_service.listen_addresses();
	gum::info!(target: LOG_TARGET, ?local_peer_id, ?listen_addresses, "Peer {} ready", index);
}

fn build_overseer<B, N>(
	state: &TestState,
	dependencies: &TestEnvironmentDependencies,
	mode: TestDataAvailability,
) -> (
	Overseer<SpawnGlue<SpawnTaskHandle>, AlwaysSupportsParachains>,
	OverseerHandle,
	Vec<ProtocolConfig>,
	sc_network::PeerId,
	sc_network::Multiaddr,
)
where
	B: BlockT<Hash = H256> + 'static,
	N: NetworkBackend<B, B::Hash>,
{
	let _guard = dependencies.runtime.handle().enter();
	let node_key: NodeKeyConfig =
		state.test_authorities.node_key_configs[NODE_UNDER_TEST as usize].clone();
	let listen_addr = state.test_authorities.authority_id_to_addr
		[&state.test_authorities.validator_authority_id[NODE_UNDER_TEST as usize]]
		.clone();
	let role = Role::Authority;
	let protocol_id = ProtocolId::from("sup");
	let notification_metrics = NotificationMetrics::new(Some(&dependencies.registry));

	let mut network_config = network_utils::build_network_config::<B, N>(
		node_key,
		listen_addr,
		Some(dependencies.registry.clone()),
	);

	let (collation_req_receiver, collation_req_cfg) =
		IncomingRequest::get_config_receiver::<B, N>(&state.req_protocol_names);
	network_config.add_request_response_protocol(collation_req_cfg);

	let (pov_req_receiver, pov_req_cfg) =
		IncomingRequest::get_config_receiver::<B, N>(&state.req_protocol_names);
	network_config.add_request_response_protocol(pov_req_cfg);

	let (chunk_req_v1_receiver, chunk_req_v1_cfg) =
		IncomingRequest::<v1::ChunkFetchingRequest>::get_config_receiver::<B, N>(
			&state.req_protocol_names,
		);
	network_config.add_request_response_protocol(chunk_req_v1_cfg);

	let (chunk_req_v2_receiver, chunk_req_v2_cfg) =
		IncomingRequest::<v2::ChunkFetchingRequest>::get_config_receiver::<B, N>(
			&state.req_protocol_names,
		);
	network_config.add_request_response_protocol(chunk_req_v2_cfg);

	let (peer_set_protocol_names, peer_set_services) =
		network_utils::build_peer_set_services::<B, N>(&mut network_config, &notification_metrics);

	let (worker, network_service, mut block_announce_service) =
		network_utils::build_network_worker::<B, N>(
			dependencies,
			network_config,
			role,
			protocol_id,
			notification_metrics,
			Some(dependencies.registry.clone()),
			"networking",
		);

	let network_bridge_metrics =
		NetworkBridgeMetrics::register(Some(&dependencies.registry)).unwrap();

	let authority_discovery_service = network_utils::DummyAuthotiryDiscoveryService::new(
		state.test_authorities.peer_id_to_authority.clone(),
		state.test_authorities.authority_id_to_addr.clone(),
	);
	let notification_sinks = Arc::new(parking_lot::Mutex::new(HashMap::new()));
	let network_bridge_tx = NetworkBridgeTxSubsystem::new(
		Arc::clone(&network_service),
		authority_discovery_service.clone(),
		network_bridge_metrics.clone(),
		state.req_protocol_names.clone(),
		peer_set_protocol_names.clone(),
		Arc::clone(&notification_sinks),
	);
	let dummy_sync_oracle = Box::new(network_utils::DummySyncOracle);
	let network_bridge_rx = NetworkBridgeRxSubsystem::new(
		Arc::clone(&network_service),
		authority_discovery_service,
		dummy_sync_oracle,
		network_bridge_metrics,
		peer_set_protocol_names,
		peer_set_services,
		Arc::clone(&notification_sinks),
		false,
	);

	let runtime_api = MockRuntimeApi::new(
		state.config.clone(),
		state.test_authorities.clone(),
		state.candidate_receipts.clone(),
		Default::default(),
		Default::default(),
		0,
		MockRuntimeApiCoreState::Occupied,
	);

	let (overseer, overseer_handle) = match &mode {
		TestDataAvailability::Read(options) => {
			let subsystem = match options.strategy {
				Strategy::FullFromBackers =>
					AvailabilityRecoverySubsystem::with_recovery_strategy_kind(
						collation_req_receiver,
						&state.req_protocol_names,
						Metrics::try_register(&dependencies.registry).unwrap(),
						RecoveryStrategyKind::BackersFirstAlways,
					),
				Strategy::Chunks => AvailabilityRecoverySubsystem::with_recovery_strategy_kind(
					collation_req_receiver,
					&state.req_protocol_names,
					Metrics::try_register(&dependencies.registry).unwrap(),
					RecoveryStrategyKind::ChunksAlways,
				),
				Strategy::Systematic => AvailabilityRecoverySubsystem::with_recovery_strategy_kind(
					collation_req_receiver,
					&state.req_protocol_names,
					Metrics::try_register(&dependencies.registry).unwrap(),
					RecoveryStrategyKind::SystematicChunks,
				),
			};

			// Use a mocked av-store.
			let av_store = MockAvailabilityStore::new(
				state.chunks.clone(),
				state.chunk_indices.clone(),
				state.candidate_hashes.clone(),
				state.candidate_hash_to_core_index.clone(),
			);

			build_overseer_for_availability_read(
				dependencies.task_manager.spawn_handle(),
				runtime_api,
				av_store,
				(network_bridge_tx, network_bridge_rx),
				subsystem,
				&dependencies,
			)
		},
		TestDataAvailability::Write => {
			let availability_distribution = AvailabilityDistributionSubsystem::new(
				state.test_authorities.keyring.keystore(),
				IncomingRequestReceivers {
					pov_req_receiver,
					chunk_req_v1_receiver,
					chunk_req_v2_receiver,
				},
				state.req_protocol_names.clone(),
				Metrics::try_register(&dependencies.registry).unwrap(),
			);

			let chain_api_state = ChainApiState { block_headers: state.block_headers.clone() };
			let chain_api = MockChainApi::new(chain_api_state);
			let bitfield_distribution =
				BitfieldDistribution::new(Metrics::try_register(&dependencies.registry).unwrap());
			build_overseer_for_availability_write(
				dependencies.task_manager.spawn_handle(),
				runtime_api,
				(network_bridge_tx, network_bridge_rx),
				availability_distribution,
				chain_api,
				new_av_store(&dependencies),
				bitfield_distribution,
				&dependencies,
			)
		},
	};

	let spawn_handle = dependencies.task_manager.spawn_handle();
	spawn_handle.spawn("network-worker", "networking", worker.run());
	let syncing_name = Box::leak(format!("Peer {} syncing", NODE_UNDER_TEST).into_boxed_str());
	// Part of SyncingEngline which is not included to networking tasks
	spawn_handle.spawn(syncing_name, "test-environment", {
		let mut notification_service = block_announce_service.clone().unwrap();
		async move {
			while let Some(event) = notification_service.next_event().await {
				if let NotificationEvent::ValidateInboundSubstream { result_tx, .. } = event {
					result_tx.send(ValidationResult::Accept).unwrap()
				}
			}
		}
	});

	let ready = tokio::spawn({
		let network_service = Arc::clone(&network_service);
		async move {
			while network_service.listen_addresses().is_empty() {
				tokio::time::sleep(Duration::from_millis(10)).await;
			}
		}
	});
	let _ = tokio::task::block_in_place(|| tokio::runtime::Handle::current().block_on(ready));

	let local_peer_id = network_service.local_peer_id();
	let listen_addresses = network_service.listen_addresses();
	gum::info!(target: LOG_TARGET, ?local_peer_id, ?listen_addresses, "Peer {} ready", NODE_UNDER_TEST);

	(
		overseer,
		overseer_handle,
		vec![],
		local_peer_id,
		listen_addresses.first().expect("listen addresses must not be empty").clone(),
	)
}

pub async fn benchmark_availability_read(
	env: &mut TestEnvironment,
	state: &TestState,
) -> BenchmarkUsage {
	let config = env.config().clone();

	env.metrics().set_n_validators(config.n_validators);
	env.metrics().set_n_cores(config.n_cores);

	let mut batch = FuturesUnordered::new();
	let mut availability_bytes = 0u128;
	let mut candidates = state.candidates.clone();

	tokio::time::sleep(Duration::from_secs(5)).await;
	let test_start = Instant::now();
	for block_info in state.block_infos.iter() {
		let block_num = block_info.number as usize;
		gum::info!(target: LOG_TARGET, "Current block {}/{}", block_num, env.config().num_blocks);
		env.metrics().set_current_block(block_num);

		let block_start_ts = Instant::now();
		env.import_block(block_info.clone()).await;

		for candidate_num in 0..config.n_cores as u64 {
			let candidate =
				candidates.next().expect("We always send up to n_cores*num_blocks; qed");
			let (tx, rx) = oneshot::channel();
			batch.push(rx);

			let message = AllMessages::AvailabilityRecovery(
				AvailabilityRecoveryMessage::RecoverAvailableData(
					candidate.clone(),
					1,
					Some(GroupIndex(
						candidate_num as u32 % (std::cmp::max(5, config.n_cores) / 5) as u32,
					)),
					Some(*state.candidate_hash_to_core_index.get(&candidate.hash()).unwrap()),
					tx,
				),
			);
			env.send_message(message).await;
		}

		gum::info!(target: LOG_TARGET, "{}", format!("{} recoveries pending", batch.len()).bright_black());
		while let Some(completed) = batch.next().await {
			let available_data = completed.unwrap().unwrap();
			env.metrics().on_pov_size(available_data.encoded_size());
			availability_bytes += available_data.encoded_size() as u128;
		}

		let block_time = Instant::now().sub(block_start_ts).as_millis() as u64;
		env.metrics().set_block_time(block_time);
		gum::info!(target: LOG_TARGET, "All work for block completed in {}", format!("{:?}ms", block_time).cyan());
	}

	let duration: u128 = test_start.elapsed().as_millis();
	let availability_bytes = availability_bytes / 1024;
	gum::info!(target: LOG_TARGET, "All blocks processed in {}", format!("{:?}ms", duration).cyan());
	gum::info!(target: LOG_TARGET,
		"Throughput: {}",
		format!("{} KiB/block", availability_bytes / env.config().num_blocks as u128).bright_red()
	);
	gum::info!(target: LOG_TARGET,
		"Avg block time: {}",
		format!("{} ms", test_start.elapsed().as_millis() / env.config().num_blocks as u128).red()
	);

	env.stop().await;
	env.collect_resource_usage(&["availability-recovery"], false)
}

pub async fn benchmark_availability_write(
	env: &mut TestEnvironment,
	state: &TestState,
) -> BenchmarkUsage {
	let config = env.config().clone();

	env.metrics().set_n_validators(config.n_validators);
	env.metrics().set_n_cores(config.n_cores);

	gum::info!(target: LOG_TARGET, "Seeding availability store with candidates ...");
	for (core_index, backed_candidate) in state.backed_candidates.clone().into_iter().enumerate() {
		let candidate_index = *state.candidate_hashes.get(&backed_candidate.hash()).unwrap();
		let available_data = state.available_data[candidate_index].clone();
		let (tx, rx) = oneshot::channel();
		env.send_message(AllMessages::AvailabilityStore(
			AvailabilityStoreMessage::StoreAvailableData {
				candidate_hash: backed_candidate.hash(),
				n_validators: config.n_validators as u32,
				available_data,
				expected_erasure_root: backed_candidate.descriptor().erasure_root(),
				tx,
				core_index: CoreIndex(core_index as u32),
				node_features: default_node_features(),
			},
		))
		.await;

		rx.await
			.unwrap()
			.expect("Test candidates are stored nicely in availability store");
	}

	gum::info!(target: LOG_TARGET, "Done");

	let test_start = Instant::now();
	for block_info in state.block_infos.iter() {
		let block_num = block_info.number as usize;
		gum::info!(target: LOG_TARGET, "Current block #{}", block_num);
		env.metrics().set_current_block(block_num);

		let block_start_ts = Instant::now();
		let relay_block_hash = block_info.hash;
		env.import_block(block_info.clone()).await;

		// Inform bitfield distribution about our view of current test block
		let message = polkadot_node_subsystem_types::messages::BitfieldDistributionMessage::NetworkBridgeUpdate(
			NetworkBridgeEvent::OurViewChange(OurView::new(vec![relay_block_hash], 0))
		);
		env.send_message(AllMessages::BitfieldDistribution(message)).await;

		let chunk_fetch_start_ts = Instant::now();

		// Request chunks of our own backed candidate from all other validators.
		let payloads = state.chunk_fetching_requests.get(block_num - 1).expect("pregenerated");
		let receivers = (1..config.n_validators).filter_map(|index| {
			let (pending_response, pending_response_receiver) = oneshot::channel();

			let peer_id = *env.authorities().peer_ids.get(index).expect("all validators have ids");
			let payload = payloads.get(index).expect("pregenerated").clone();
			let request = RawIncomingRequest { peer: peer_id, payload, pending_response };
			let peer = env
				.authorities()
				.validator_authority_id
				.get(index)
				.expect("all validators have keys");

			if env.network().is_peer_connected(peer) &&
				env.network().send_request_from_peer(peer, request).is_ok()
			{
				Some(pending_response_receiver)
			} else {
				None
			}
		});

		gum::info!(target: LOG_TARGET, "Waiting for all emulated peers to receive their chunk from us ...");

		let responses = futures::future::try_join_all(receivers)
			.await
			.expect("Chunk is always served successfully");
		// TODO: check if chunk is the one the peer expects to receive.
		assert!(responses.iter().all(|v| v.result.is_ok()));

		let chunk_fetch_duration = Instant::now().sub(chunk_fetch_start_ts).as_millis();
		gum::info!(target: LOG_TARGET, "All chunks received in {}ms", chunk_fetch_duration);

		let network = env.network().clone();
		let authorities = env.authorities().clone();

		// Spawn a task that will generate `n_validator` - 1 signed bitfields and
		// send them from the emulated peers to the subsystem.
		// TODO: Implement topology.
		let messages = state.signed_bitfields.get(&relay_block_hash).expect("pregenerated").clone();
		for index in 1..config.n_validators {
			let from_peer = &authorities.validator_authority_id[index];
			let message = messages.get(index).expect("pregenerated").clone();

			// Send the action from peer only if it is connected to our node.
			if network.is_peer_connected(from_peer) {
				let _ = network.send_message_from_peer(from_peer, message);
			}
		}

		gum::info!(
			"Waiting for {} bitfields to be received and processed",
			config.connected_count()
		);

		// Wait for all bitfields to be processed.
		env.wait_until_metric(
			"polkadot_parachain_received_availability_bitfields_total",
			None,
			|value| value == (config.connected_count() * block_num) as f64,
		)
		.await;

		gum::info!(target: LOG_TARGET, "All bitfields processed");

		let block_time = Instant::now().sub(block_start_ts).as_millis() as u64;
		env.metrics().set_block_time(block_time);
		gum::info!(target: LOG_TARGET, "All work for block completed in {}", format!("{:?}ms", block_time).cyan());
	}

	let duration: u128 = test_start.elapsed().as_millis();
	gum::info!(target: LOG_TARGET, "All blocks processed in {}", format!("{:?}ms", duration).cyan());
	gum::info!(target: LOG_TARGET,
		"Avg block time: {}",
		format!("{} ms", test_start.elapsed().as_millis() / env.config().num_blocks as u128).red()
	);

	env.stop().await;
	env.collect_resource_usage(
		&["availability-distribution", "bitfield-distribution", "availability-store"],
		false,
	)
}
