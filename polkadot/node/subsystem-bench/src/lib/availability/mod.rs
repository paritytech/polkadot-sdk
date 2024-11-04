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
		network_bridge::{self, MockNetworkBridgeRx, MockNetworkBridgeTx},
		runtime_api::{
			node_features_with_chunk_mapping_enabled, MockRuntimeApi, MockRuntimeApiCoreState,
		},
		AlwaysSupportsParachains,
	},
	network::new_network,
	usage::BenchmarkUsage,
};
use colored::Colorize;
use futures::{channel::oneshot, stream::FuturesUnordered, StreamExt};

use codec::Encode;
use polkadot_availability_bitfield_distribution::BitfieldDistribution;
use polkadot_availability_distribution::{
	AvailabilityDistributionSubsystem, IncomingRequestReceivers,
};
use polkadot_availability_recovery::{AvailabilityRecoverySubsystem, RecoveryStrategyKind};
use polkadot_node_core_av_store::AvailabilityStoreSubsystem;
use polkadot_node_metrics::metrics::Metrics;
use polkadot_node_network_protocol::{
	request_response::{v1, v2, IncomingRequest},
	OurView,
};
use polkadot_node_subsystem::{
	messages::{AllMessages, AvailabilityRecoveryMessage},
	Overseer, OverseerConnector, SpawnGlue,
};
use polkadot_node_subsystem_types::messages::{AvailabilityStoreMessage, NetworkBridgeEvent};
use polkadot_overseer::{metrics::Metrics as OverseerMetrics, Handle as OverseerHandle};
use polkadot_primitives::{Block, CoreIndex, GroupIndex, Hash};
use sc_network::request_responses::{IncomingRequest as RawIncomingRequest, ProtocolConfig};
use std::{ops::Sub, sync::Arc, time::Instant};
use strum::Display;

use sc_service::SpawnTaskHandle;
use serde::{Deserialize, Serialize};
pub use test_state::TestState;

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
	(network_bridge_tx, network_bridge_rx): (MockNetworkBridgeTx, MockNetworkBridgeRx),
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
	(network_bridge_tx, network_bridge_rx): (MockNetworkBridgeTx, MockNetworkBridgeRx),
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

pub fn prepare_test(
	state: &TestState,
	mode: TestDataAvailability,
	with_prometheus_endpoint: bool,
) -> (TestEnvironment, Vec<ProtocolConfig>) {
	let dependencies = TestEnvironmentDependencies::default();

	let availability_state = NetworkAvailabilityState {
		candidate_hashes: state.candidate_hashes.clone(),
		candidate_hash_to_core_index: state.candidate_hash_to_core_index.clone(),
		available_data: state.available_data.clone(),
		chunks: state.chunks.clone(),
		chunk_indices: state.chunk_indices.clone(),
		req_protocol_names: state.req_protocol_names.clone(),
	};

	let mut req_cfgs = Vec::new();

	let (collation_req_receiver, collation_req_cfg) = IncomingRequest::get_config_receiver::<
		Block,
		sc_network::NetworkWorker<Block, Hash>,
	>(&state.req_protocol_names);
	req_cfgs.push(collation_req_cfg);

	let (pov_req_receiver, pov_req_cfg) = IncomingRequest::get_config_receiver::<
		Block,
		sc_network::NetworkWorker<Block, Hash>,
	>(&state.req_protocol_names);
	req_cfgs.push(pov_req_cfg);

	let (chunk_req_v1_receiver, chunk_req_v1_cfg) =
		IncomingRequest::<v1::ChunkFetchingRequest>::get_config_receiver::<
			Block,
			sc_network::NetworkWorker<Block, Hash>,
		>(&state.req_protocol_names);

	// We won't use v1 chunk fetching requests, but we need to keep the inbound queue alive.
	// Otherwise, av-distribution subsystem will terminate.
	std::mem::forget(chunk_req_v1_cfg);

	let (chunk_req_v2_receiver, chunk_req_v2_cfg) =
		IncomingRequest::<v2::ChunkFetchingRequest>::get_config_receiver::<
			Block,
			sc_network::NetworkWorker<Block, Hash>,
		>(&state.req_protocol_names);

	let (network, network_interface, network_receiver) = new_network(
		&state.config,
		&dependencies,
		&state.test_authorities,
		vec![Arc::new(availability_state.clone())],
	);

	let network_bridge_tx = network_bridge::MockNetworkBridgeTx::new(
		network.clone(),
		network_interface.subsystem_sender(),
		state.test_authorities.clone(),
	);
	let network_bridge_rx =
		network_bridge::MockNetworkBridgeRx::new(network_receiver, Some(chunk_req_v2_cfg), false);

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
				node_features: node_features_with_chunk_mapping_enabled(),
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
