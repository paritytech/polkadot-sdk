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
	core::{
		configuration::TestConfiguration,
		environment::{BenchmarkUsage, TestEnvironmentDependencies},
		mock::{
			av_store,
			av_store::MockAvailabilityStore,
			chain_api::{ChainApiState, MockChainApi},
			dummy_builder,
			network_bridge::{self, MockNetworkBridgeRx, MockNetworkBridgeTx},
			runtime_api,
			runtime_api::MockRuntimeApi,
			AlwaysSupportsParachains,
		},
		network::new_network,
	},
	TestEnvironment, TestObjective, GENESIS_HASH,
};
use av_store::NetworkAvailabilityState;
use av_store_helpers::new_av_store;
use bitvec::bitvec;
use colored::Colorize;
use futures::{channel::oneshot, stream::FuturesUnordered, StreamExt};
use itertools::Itertools;
use parity_scale_codec::Encode;
use polkadot_availability_bitfield_distribution::BitfieldDistribution;
use polkadot_availability_distribution::{
	AvailabilityDistributionSubsystem, IncomingRequestReceivers,
};
use polkadot_availability_recovery::AvailabilityRecoverySubsystem;
use polkadot_node_core_av_store::AvailabilityStoreSubsystem;
use polkadot_node_metrics::metrics::Metrics;
use polkadot_node_network_protocol::{
	request_response::{v1::ChunkFetchingRequest, IncomingRequest, ReqProtocolNames},
	OurView, Versioned, VersionedValidationProtocol,
};
use polkadot_node_primitives::{AvailableData, BlockData, ErasureChunk, PoV};
use polkadot_node_subsystem::{
	messages::{AllMessages, AvailabilityRecoveryMessage},
	Overseer, OverseerConnector, SpawnGlue,
};
use polkadot_node_subsystem_test_helpers::{
	derive_erasure_chunks_with_proofs_and_root, mock::new_block_import_info,
};
use polkadot_node_subsystem_types::{
	messages::{AvailabilityStoreMessage, NetworkBridgeEvent},
	Span,
};
use polkadot_overseer::{metrics::Metrics as OverseerMetrics, Handle as OverseerHandle};
use polkadot_primitives::{
	AvailabilityBitfield, BlockNumber, CandidateHash, CandidateReceipt, GroupIndex, Hash, HeadData,
	Header, PersistedValidationData, Signed, SigningContext, ValidatorIndex,
};
use polkadot_primitives_test_helpers::{dummy_candidate_receipt, dummy_hash};
use sc_network::{
	request_responses::{IncomingRequest as RawIncomingRequest, ProtocolConfig},
	PeerId,
};
use sc_service::SpawnTaskHandle;
use sp_core::H256;
use std::{collections::HashMap, iter::Cycle, ops::Sub, sync::Arc, time::Instant};

mod av_store_helpers;
pub(crate) mod cli;

const LOG_TARGET: &str = "subsystem-bench::availability";

fn build_overseer_for_availability_read(
	spawn_task_handle: SpawnTaskHandle,
	runtime_api: MockRuntimeApi,
	av_store: MockAvailabilityStore,
	network_bridge: (MockNetworkBridgeTx, MockNetworkBridgeRx),
	availability_recovery: AvailabilityRecoverySubsystem,
	dependencies: &TestEnvironmentDependencies,
) -> (Overseer<SpawnGlue<SpawnTaskHandle>, AlwaysSupportsParachains>, OverseerHandle) {
	let overseer_connector = OverseerConnector::with_event_capacity(64000);
	let overseer_metrics = OverseerMetrics::try_register(&dependencies.registry).unwrap();

	let dummy = dummy_builder!(spawn_task_handle, overseer_metrics);
	let builder = dummy
		.replace_runtime_api(|_| runtime_api)
		.replace_availability_store(|_| av_store)
		.replace_network_bridge_tx(|_| network_bridge.0)
		.replace_network_bridge_rx(|_| network_bridge.1)
		.replace_availability_recovery(|_| availability_recovery);

	let (overseer, raw_handle) =
		builder.build_with_connector(overseer_connector).expect("Should not fail");

	(overseer, OverseerHandle::new(raw_handle))
}

#[allow(clippy::too_many_arguments)]
fn build_overseer_for_availability_write(
	spawn_task_handle: SpawnTaskHandle,
	runtime_api: MockRuntimeApi,
	network_bridge: (MockNetworkBridgeTx, MockNetworkBridgeRx),
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
		.replace_network_bridge_tx(|_| network_bridge.0)
		.replace_network_bridge_rx(|_| network_bridge.1)
		.replace_chain_api(|_| chain_api)
		.replace_bitfield_distribution(|_| bitfield_distribution)
		// This is needed to test own chunk recovery for `n_cores`.
		.replace_availability_distribution(|_| availability_distribution);

	let (overseer, raw_handle) =
		builder.build_with_connector(overseer_connector).expect("Should not fail");

	(overseer, OverseerHandle::new(raw_handle))
}

/// Takes a test configuration and uses it to create the `TestEnvironment`.
pub fn prepare_test(
	config: TestConfiguration,
	state: &mut TestState,
) -> (TestEnvironment, Vec<ProtocolConfig>) {
	prepare_test_inner(config, state, TestEnvironmentDependencies::default())
}

fn prepare_test_inner(
	config: TestConfiguration,
	state: &mut TestState,
	dependencies: TestEnvironmentDependencies,
) -> (TestEnvironment, Vec<ProtocolConfig>) {
	// Generate test authorities.
	let test_authorities = config.generate_authorities();

	let mut candidate_hashes: HashMap<H256, Vec<CandidateReceipt>> = HashMap::new();

	// Prepare per block candidates.
	// Genesis block is always finalized, so we start at 1.
	for block_num in 1..=config.num_blocks {
		for _ in 0..config.n_cores {
			candidate_hashes
				.entry(Hash::repeat_byte(block_num as u8))
				.or_default()
				.push(state.next_candidate().expect("Cycle iterator"))
		}

		// First candidate is our backed candidate.
		state.backed_candidates.push(
			candidate_hashes
				.get(&Hash::repeat_byte(block_num as u8))
				.expect("just inserted above")
				.first()
				.expect("just inserted above")
				.clone(),
		);
	}

	let runtime_api = runtime_api::MockRuntimeApi::new(
		config.clone(),
		test_authorities.clone(),
		candidate_hashes,
		Default::default(),
		Default::default(),
		0,
	);

	let availability_state = NetworkAvailabilityState {
		candidate_hashes: state.candidate_hashes.clone(),
		available_data: state.available_data.clone(),
		chunks: state.chunks.clone(),
	};

	let mut req_cfgs = Vec::new();

	let (collation_req_receiver, collation_req_cfg) =
		IncomingRequest::get_config_receiver(&ReqProtocolNames::new(GENESIS_HASH, None));
	req_cfgs.push(collation_req_cfg);

	let (pov_req_receiver, pov_req_cfg) =
		IncomingRequest::get_config_receiver(&ReqProtocolNames::new(GENESIS_HASH, None));

	let (chunk_req_receiver, chunk_req_cfg) =
		IncomingRequest::get_config_receiver(&ReqProtocolNames::new(GENESIS_HASH, None));
	req_cfgs.push(pov_req_cfg);

	let (network, network_interface, network_receiver) =
		new_network(&config, &dependencies, &test_authorities, vec![Arc::new(availability_state)]);

	let network_bridge_tx = network_bridge::MockNetworkBridgeTx::new(
		network.clone(),
		network_interface.subsystem_sender(),
		test_authorities.clone(),
	);

	let network_bridge_rx =
		network_bridge::MockNetworkBridgeRx::new(network_receiver, Some(chunk_req_cfg.clone()));

	let (overseer, overseer_handle) = match &state.config().objective {
		TestObjective::DataAvailabilityRead(options) => {
			let use_fast_path = options.fetch_from_backers;

			let subsystem = if use_fast_path {
				AvailabilityRecoverySubsystem::with_fast_path(
					collation_req_receiver,
					Metrics::try_register(&dependencies.registry).unwrap(),
				)
			} else {
				AvailabilityRecoverySubsystem::with_chunks_only(
					collation_req_receiver,
					Metrics::try_register(&dependencies.registry).unwrap(),
				)
			};

			// Use a mocked av-store.
			let av_store = av_store::MockAvailabilityStore::new(
				state.chunks.clone(),
				state.candidate_hashes.clone(),
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
		TestObjective::DataAvailabilityWrite => {
			let availability_distribution = AvailabilityDistributionSubsystem::new(
				test_authorities.keyring.keystore(),
				IncomingRequestReceivers { pov_req_receiver, chunk_req_receiver },
				Metrics::try_register(&dependencies.registry).unwrap(),
			);

			let block_headers = (1..=config.num_blocks)
				.map(|block_number| {
					(
						Hash::repeat_byte(block_number as u8),
						Header {
							digest: Default::default(),
							number: block_number as BlockNumber,
							parent_hash: Default::default(),
							extrinsics_root: Default::default(),
							state_root: Default::default(),
						},
					)
				})
				.collect::<HashMap<_, _>>();

			let chain_api_state = ChainApiState { block_headers };
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
		_ => {
			unimplemented!("Invalid test objective")
		},
	};

	(
		TestEnvironment::new(
			dependencies,
			config,
			network,
			overseer,
			overseer_handle,
			test_authorities,
		),
		req_cfgs,
	)
}

#[derive(Clone)]
pub struct TestState {
	// Full test configuration
	config: TestConfiguration,
	// A cycle iterator on all PoV sizes used in the test.
	pov_sizes: Cycle<std::vec::IntoIter<usize>>,
	// Generated candidate receipts to be used in the test
	candidates: Cycle<std::vec::IntoIter<CandidateReceipt>>,
	// Map from pov size to candidate index
	pov_size_to_candidate: HashMap<usize, usize>,
	// Map from generated candidate hashes to candidate index in `available_data`
	// and `chunks`.
	candidate_hashes: HashMap<CandidateHash, usize>,
	// Per candidate index receipts.
	candidate_receipt_templates: Vec<CandidateReceipt>,
	// Per candidate index `AvailableData`
	available_data: Vec<AvailableData>,
	// Per candiadte index chunks
	chunks: Vec<Vec<ErasureChunk>>,
	// Per relay chain block - candidate backed by our backing group
	backed_candidates: Vec<CandidateReceipt>,
}

impl TestState {
	fn config(&self) -> &TestConfiguration {
		&self.config
	}

	pub fn next_candidate(&mut self) -> Option<CandidateReceipt> {
		let candidate = self.candidates.next();
		let candidate_hash = candidate.as_ref().unwrap().hash();
		gum::trace!(target: LOG_TARGET, "Next candidate selected {:?}", candidate_hash);
		candidate
	}

	/// Generate candidates to be used in the test.
	fn generate_candidates(&mut self) {
		let count = self.config.n_cores * self.config.num_blocks;
		gum::info!(target: LOG_TARGET,"{}", format!("Pre-generating {} candidates.", count).bright_blue());

		// Generate all candidates
		self.candidates = (0..count)
			.map(|index| {
				let pov_size = self.pov_sizes.next().expect("This is a cycle; qed");
				let candidate_index = *self
					.pov_size_to_candidate
					.get(&pov_size)
					.expect("pov_size always exists; qed");
				let mut candidate_receipt =
					self.candidate_receipt_templates[candidate_index].clone();

				// Make it unique.
				candidate_receipt.descriptor.relay_parent = Hash::from_low_u64_be(index as u64);
				// Store the new candidate in the state
				self.candidate_hashes.insert(candidate_receipt.hash(), candidate_index);

				gum::debug!(target: LOG_TARGET, candidate_hash = ?candidate_receipt.hash(), "new candidate");

				candidate_receipt
			})
			.collect::<Vec<_>>()
			.into_iter()
			.cycle();
	}

	pub fn new(config: &TestConfiguration) -> Self {
		let config = config.clone();

		let mut chunks = Vec::new();
		let mut available_data = Vec::new();
		let mut candidate_receipt_templates = Vec::new();
		let mut pov_size_to_candidate = HashMap::new();

		// we use it for all candidates.
		let persisted_validation_data = PersistedValidationData {
			parent_head: HeadData(vec![7, 8, 9]),
			relay_parent_number: Default::default(),
			max_pov_size: 1024,
			relay_parent_storage_root: Default::default(),
		};

		// For each unique pov we create a candidate receipt.
		for (index, pov_size) in config.pov_sizes().iter().cloned().unique().enumerate() {
			gum::info!(target: LOG_TARGET, index, pov_size, "{}", "Generating template candidate".bright_blue());

			let mut candidate_receipt = dummy_candidate_receipt(dummy_hash());
			let pov = PoV { block_data: BlockData(vec![index as u8; pov_size]) };

			let new_available_data = AvailableData {
				validation_data: persisted_validation_data.clone(),
				pov: Arc::new(pov),
			};

			let (new_chunks, erasure_root) = derive_erasure_chunks_with_proofs_and_root(
				config.n_validators,
				&new_available_data,
				|_, _| {},
			);

			candidate_receipt.descriptor.erasure_root = erasure_root;

			chunks.push(new_chunks);
			available_data.push(new_available_data);
			pov_size_to_candidate.insert(pov_size, index);
			candidate_receipt_templates.push(candidate_receipt);
		}

		gum::info!(target: LOG_TARGET, "{}","Created test environment.".bright_blue());

		let mut _self = Self {
			available_data,
			candidate_receipt_templates,
			chunks,
			pov_size_to_candidate,
			pov_sizes: Vec::from(config.pov_sizes()).into_iter().cycle(),
			candidate_hashes: HashMap::new(),
			candidates: Vec::new().into_iter().cycle(),
			backed_candidates: Vec::new(),
			config,
		};

		_self.generate_candidates();
		_self
	}

	pub fn backed_candidates(&mut self) -> &mut Vec<CandidateReceipt> {
		&mut self.backed_candidates
	}
}

pub async fn benchmark_availability_read(
	benchmark_name: &str,
	env: &mut TestEnvironment,
	mut state: TestState,
) -> BenchmarkUsage {
	let config = env.config().clone();

	env.import_block(new_block_import_info(Hash::repeat_byte(1), 1)).await;

	let test_start = Instant::now();
	let mut batch = FuturesUnordered::new();
	let mut availability_bytes = 0u128;

	env.metrics().set_n_validators(config.n_validators);
	env.metrics().set_n_cores(config.n_cores);

	for block_num in 1..=env.config().num_blocks {
		gum::info!(target: LOG_TARGET, "Current block {}/{}", block_num, env.config().num_blocks);
		env.metrics().set_current_block(block_num);

		let block_start_ts = Instant::now();
		for candidate_num in 0..config.n_cores as u64 {
			let candidate =
				state.next_candidate().expect("We always send up to n_cores*num_blocks; qed");
			let (tx, rx) = oneshot::channel();
			batch.push(rx);

			let message = AllMessages::AvailabilityRecovery(
				AvailabilityRecoveryMessage::RecoverAvailableData(
					candidate.clone(),
					1,
					Some(GroupIndex(
						candidate_num as u32 % (std::cmp::max(5, config.n_cores) / 5) as u32,
					)),
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
	env.collect_resource_usage(benchmark_name, &["availability-recovery"])
}

pub async fn benchmark_availability_write(
	benchmark_name: &str,
	env: &mut TestEnvironment,
	mut state: TestState,
) -> BenchmarkUsage {
	let config = env.config().clone();

	env.metrics().set_n_validators(config.n_validators);
	env.metrics().set_n_cores(config.n_cores);

	gum::info!(target: LOG_TARGET, "Seeding availability store with candidates ...");
	for backed_candidate in state.backed_candidates().clone() {
		let candidate_index = *state.candidate_hashes.get(&backed_candidate.hash()).unwrap();
		let available_data = state.available_data[candidate_index].clone();
		let (tx, rx) = oneshot::channel();
		env.send_message(AllMessages::AvailabilityStore(
			AvailabilityStoreMessage::StoreAvailableData {
				candidate_hash: backed_candidate.hash(),
				n_validators: config.n_validators as u32,
				available_data,
				expected_erasure_root: backed_candidate.descriptor().erasure_root,
				tx,
			},
		))
		.await;

		rx.await
			.unwrap()
			.expect("Test candidates are stored nicely in availability store");
	}

	gum::info!(target: LOG_TARGET, "Done");

	let test_start = Instant::now();

	for block_num in 1..=env.config().num_blocks {
		gum::info!(target: LOG_TARGET, "Current block #{}", block_num);
		env.metrics().set_current_block(block_num);

		let block_start_ts = Instant::now();
		let relay_block_hash = Hash::repeat_byte(block_num as u8);
		env.import_block(new_block_import_info(relay_block_hash, block_num as BlockNumber))
			.await;

		// Inform bitfield distribution about our view of current test block
		let message = polkadot_node_subsystem_types::messages::BitfieldDistributionMessage::NetworkBridgeUpdate(
			NetworkBridgeEvent::OurViewChange(OurView::new(vec![(relay_block_hash, Arc::new(Span::Disabled))], 0))
		);
		env.send_message(AllMessages::BitfieldDistribution(message)).await;

		let chunk_fetch_start_ts = Instant::now();

		// Request chunks of our own backed candidate from all other validators.
		let mut receivers = Vec::new();
		for index in 1..config.n_validators {
			let (pending_response, pending_response_receiver) = oneshot::channel();

			let request = RawIncomingRequest {
				peer: PeerId::random(),
				payload: ChunkFetchingRequest {
					candidate_hash: state.backed_candidates()[block_num - 1].hash(),
					index: ValidatorIndex(index as u32),
				}
				.encode(),
				pending_response,
			};

			let peer = env
				.authorities()
				.validator_authority_id
				.get(index)
				.expect("all validators have keys");

			if env.network().is_peer_connected(peer) &&
				env.network().send_request_from_peer(peer, request).is_ok()
			{
				receivers.push(pending_response_receiver);
			}
		}

		gum::info!(target: LOG_TARGET, "Waiting for all emulated peers to receive their chunk from us ...");
		for receiver in receivers.into_iter() {
			let response = receiver.await.expect("Chunk is always served succesfully");
			// TODO: check if chunk is the one the peer expects to receive.
			assert!(response.result.is_ok());
		}

		let chunk_fetch_duration = Instant::now().sub(chunk_fetch_start_ts).as_millis();

		gum::info!(target: LOG_TARGET, "All chunks received in {}ms", chunk_fetch_duration);

		let signing_context = SigningContext { session_index: 0, parent_hash: relay_block_hash };
		let network = env.network().clone();
		let authorities = env.authorities().clone();
		let n_validators = config.n_validators;

		// Spawn a task that will generate `n_validator` - 1 signed bitfiends and
		// send them from the emulated peers to the subsystem.
		// TODO: Implement topology.
		env.spawn_blocking("send-bitfields", async move {
			for index in 1..n_validators {
				let validator_public =
					authorities.validator_public.get(index).expect("All validator keys are known");

				// Node has all the chunks in the world.
				let payload: AvailabilityBitfield =
					AvailabilityBitfield(bitvec![u8, bitvec::order::Lsb0; 1u8; 32]);
				// TODO(soon): Use pre-signed messages. This is quite intensive on the CPU.
				let signed_bitfield = Signed::<AvailabilityBitfield>::sign(
					&authorities.keyring.keystore(),
					payload,
					&signing_context,
					ValidatorIndex(index as u32),
					validator_public,
				)
				.ok()
				.flatten()
				.expect("should be signed");

				let from_peer = &authorities.validator_authority_id[index];

				let message = peer_bitfield_message_v2(relay_block_hash, signed_bitfield);

				// Send the action from peer only if it is connected to our node.
				if network.is_peer_connected(from_peer) {
					let _ = network.send_message_from_peer(from_peer, message);
				}
			}
		});

		gum::info!(
			"Waiting for {} bitfields to be received and processed",
			config.connected_count()
		);

		// Wait for all bitfields to be processed.
		env.wait_until_metric(
			"polkadot_parachain_received_availabilty_bitfields_total",
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
		benchmark_name,
		&["availability-distribution", "bitfield-distribution", "availability-store"],
	)
}

pub fn peer_bitfield_message_v2(
	relay_hash: H256,
	signed_bitfield: Signed<AvailabilityBitfield>,
) -> VersionedValidationProtocol {
	let bitfield = polkadot_node_network_protocol::v2::BitfieldDistributionMessage::Bitfield(
		relay_hash,
		signed_bitfield.into(),
	);

	Versioned::V2(polkadot_node_network_protocol::v2::ValidationProtocol::BitfieldDistribution(
		bitfield,
	))
}
