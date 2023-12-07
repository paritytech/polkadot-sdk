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
use super::core::environment::MAX_TIME_OF_FLIGHT;
use crate::{core::mock::ChainApiState, TestEnvironment};
use bitvec::bitvec;
use colored::Colorize;
use itertools::Itertools;
use polkadot_availability_bitfield_distribution::BitfieldDistribution;
use polkadot_node_core_av_store::AvailabilityStoreSubsystem;
use polkadot_node_subsystem::{Overseer, OverseerConnector, SpawnGlue};
use polkadot_node_subsystem_types::{
	messages::{AvailabilityStoreMessage, NetworkBridgeEvent},
	Span,
};
use polkadot_overseer::Handle as OverseerHandle;
use sc_network::{request_responses::ProtocolConfig, PeerId};
use sp_core::H256;
use std::{collections::HashMap, iter::Cycle, ops::Sub, pin::Pin, sync::Arc, time::Instant};

use av_store_helpers::new_av_store;
use futures::{channel::oneshot, stream::FuturesUnordered, Future, StreamExt};
use polkadot_availability_distribution::{
	AvailabilityDistributionSubsystem, IncomingRequestReceivers,
};
use polkadot_node_metrics::metrics::Metrics;
use polkadot_node_subsystem::TimeoutExt;

use polkadot_availability_recovery::AvailabilityRecoverySubsystem;

use crate::GENESIS_HASH;
use futures::FutureExt;
use parity_scale_codec::Encode;
use polkadot_erasure_coding::{branches, obtain_chunks_v1 as obtain_chunks};
use polkadot_node_network_protocol::{
	request_response::{
		v1::ChunkFetchingRequest, IncomingRequest, OutgoingRequest, ReqProtocolNames, Requests,
	},
	BitfieldDistributionMessage, OurView, Versioned, View,
};
use polkadot_node_primitives::{BlockData, PoV, Proof};
use polkadot_node_subsystem::messages::{AllMessages, AvailabilityRecoveryMessage};

use crate::core::{
	environment::TestEnvironmentDependencies,
	mock::{
		av_store,
		network_bridge::{self, MockNetworkBridgeTx, NetworkAvailabilityState},
		runtime_api, MockAvailabilityStore, MockChainApi, MockRuntimeApi,
	},
};

use super::core::{configuration::TestConfiguration, mock::dummy_builder, network::*};

const LOG_TARGET: &str = "subsystem-bench::availability";

use polkadot_node_primitives::{AvailableData, ErasureChunk};

use super::{cli::TestObjective, core::mock::AlwaysSupportsParachains};
use polkadot_node_subsystem_test_helpers::mock::new_block_import_info;
use polkadot_primitives::{
	AvailabilityBitfield, BlockNumber, CandidateHash, CandidateReceipt, GroupIndex, Hash, HeadData,
	Header, PersistedValidationData, Signed, SigningContext, ValidatorIndex,
};
use polkadot_primitives_test_helpers::{dummy_candidate_receipt, dummy_hash};
use sc_service::SpawnTaskHandle;

mod av_store_helpers;
mod cli;
pub use cli::{DataAvailabilityReadOptions, NetworkEmulation};

fn build_overseer_for_availability_read(
	spawn_task_handle: SpawnTaskHandle,
	runtime_api: MockRuntimeApi,
	av_store: MockAvailabilityStore,
	network_bridge: MockNetworkBridgeTx,
	availability_recovery: AvailabilityRecoverySubsystem,
) -> (Overseer<SpawnGlue<SpawnTaskHandle>, AlwaysSupportsParachains>, OverseerHandle) {
	let overseer_connector = OverseerConnector::with_event_capacity(64000);
	let dummy = dummy_builder!(spawn_task_handle);
	let builder = dummy
		.replace_runtime_api(|_| runtime_api)
		.replace_availability_store(|_| av_store)
		.replace_network_bridge_tx(|_| network_bridge)
		.replace_availability_recovery(|_| availability_recovery);

	let (overseer, raw_handle) =
		builder.build_with_connector(overseer_connector).expect("Should not fail");

	(overseer, OverseerHandle::new(raw_handle))
}

fn build_overseer_for_availability_write(
	spawn_task_handle: SpawnTaskHandle,
	runtime_api: MockRuntimeApi,
	network_bridge: MockNetworkBridgeTx,
	availability_distribution: AvailabilityDistributionSubsystem,
	chain_api: MockChainApi,
	availability_store: AvailabilityStoreSubsystem,
	bitfield_distribution: BitfieldDistribution,
) -> (Overseer<SpawnGlue<SpawnTaskHandle>, AlwaysSupportsParachains>, OverseerHandle) {
	let overseer_connector = OverseerConnector::with_event_capacity(64000);
	let dummy = dummy_builder!(spawn_task_handle);
	let builder = dummy
		.replace_runtime_api(|_| runtime_api)
		.replace_availability_store(|_| availability_store)
		.replace_network_bridge_tx(|_| network_bridge)
		.replace_chain_api(|_| chain_api)
		.replace_bitfield_distribution(|_| bitfield_distribution)
		// This is needed to test own chunk recovery for `n_cores`.
		.replace_availability_distribution(|_| availability_distribution);

	let (overseer, raw_handle) =
		builder.build_with_connector(overseer_connector).expect("Should not fail");

	(overseer, OverseerHandle::new(raw_handle))
}

/// Takes a test configuration and uses it to creates the `TestEnvironment`.
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
	for block_num in 0..config.num_blocks {
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
				.get(0)
				.expect("just inserted above")
				.clone(),
		);
	}

	let runtime_api = runtime_api::MockRuntimeApi::new(
		config.clone(),
		test_authorities.clone(),
		candidate_hashes,
	);

	let availability_state = NetworkAvailabilityState {
		candidate_hashes: state.candidate_hashes.clone(),
		available_data: state.available_data.clone(),
		chunks: state.chunks.clone(),
	};

	let network = NetworkEmulator::new(&config, &dependencies, &test_authorities);

	let network_bridge_tx = network_bridge::MockNetworkBridgeTx::new(
		config.clone(),
		availability_state,
		network.clone(),
	);

	let mut req_cfgs = Vec::new();

	let (overseer, overseer_handle) = match &state.config().objective {
		TestObjective::DataAvailabilityRead(_options) => {
			let use_fast_path = match &state.config().objective {
				TestObjective::DataAvailabilityRead(options) => options.fetch_from_backers,
				_ => panic!("Unexpected objective"),
			};

			let (collation_req_receiver, req_cfg) =
				IncomingRequest::get_config_receiver(&ReqProtocolNames::new(&GENESIS_HASH, None));
			req_cfgs.push(req_cfg);

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
			// TODO: switch to real av-store.
			let av_store = av_store::MockAvailabilityStore::new(
				state.chunks.clone(),
				state.candidate_hashes.clone(),
			);

			build_overseer_for_availability_read(
				dependencies.task_manager.spawn_handle(),
				runtime_api,
				av_store,
				network_bridge_tx,
				subsystem,
			)
		},
		TestObjective::DataAvailabilityWrite => {
			let (pov_req_receiver, pov_req_cfg) =
				IncomingRequest::get_config_receiver(&ReqProtocolNames::new(&GENESIS_HASH, None));
			let (chunk_req_receiver, chunk_req_cfg) =
				IncomingRequest::get_config_receiver(&ReqProtocolNames::new(&GENESIS_HASH, None));
			req_cfgs.push(pov_req_cfg);

			state.set_chunk_request_protocol(chunk_req_cfg);

			let availability_distribution = AvailabilityDistributionSubsystem::new(
				test_authorities.keyring.keystore(),
				IncomingRequestReceivers { pov_req_receiver, chunk_req_receiver },
				Metrics::try_register(&dependencies.registry).unwrap(),
			);

			let block_headers = (0..config.num_blocks)
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
				network_bridge_tx,
				availability_distribution,
				chain_api,
				new_av_store(&dependencies),
				bitfield_distribution,
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
	// Availability distribution
	chunk_request_protocol: Option<ProtocolConfig>,
	// Availability distribution.
	pov_request_protocol: Option<ProtocolConfig>,
	// Per relay chain block - our backed candidate
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

		let pov_sizes = config.pov_sizes().to_vec().into_iter().cycle();
		gum::info!(target: LOG_TARGET, "{}","Created test environment.".bright_blue());

		let mut _self = Self {
			config,
			available_data,
			candidate_receipt_templates,
			chunks,
			pov_size_to_candidate,
			pov_sizes,
			candidate_hashes: HashMap::new(),
			candidates: Vec::new().into_iter().cycle(),
			chunk_request_protocol: None,
			pov_request_protocol: None,
			backed_candidates: Vec::new(),
		};

		_self.generate_candidates();
		_self
	}

	pub fn backed_candidates(&mut self) -> &mut Vec<CandidateReceipt> {
		&mut self.backed_candidates
	}

	pub fn set_chunk_request_protocol(&mut self, config: ProtocolConfig) {
		self.chunk_request_protocol = Some(config);
	}

	pub fn set_pov_request_protocol(&mut self, config: ProtocolConfig) {
		self.pov_request_protocol = Some(config);
	}

	pub fn chunk_request_protocol(&self) -> Option<ProtocolConfig> {
		self.chunk_request_protocol.clone()
	}
}

fn derive_erasure_chunks_with_proofs_and_root(
	n_validators: usize,
	available_data: &AvailableData,
	alter_chunk: impl Fn(usize, &mut Vec<u8>),
) -> (Vec<ErasureChunk>, Hash) {
	let mut chunks: Vec<Vec<u8>> = obtain_chunks(n_validators, available_data).unwrap();

	for (i, chunk) in chunks.iter_mut().enumerate() {
		alter_chunk(i, chunk)
	}

	// create proofs for each erasure chunk
	let branches = branches(chunks.as_ref());

	let root = branches.root();
	let erasure_chunks = branches
		.enumerate()
		.map(|(index, (proof, chunk))| ErasureChunk {
			chunk: chunk.to_vec(),
			index: ValidatorIndex(index as _),
			proof: Proof::try_from(proof).unwrap(),
		})
		.collect::<Vec<ErasureChunk>>();

	(erasure_chunks, root)
}

pub async fn benchmark_availability_read(env: &mut TestEnvironment, mut state: TestState) {
	let config = env.config().clone();

	env.import_block(new_block_import_info(Hash::repeat_byte(1), 1)).await;

	let start_marker = Instant::now();
	let mut batch = FuturesUnordered::new();
	let mut availability_bytes = 0u128;

	env.metrics().set_n_validators(config.n_validators);
	env.metrics().set_n_cores(config.n_cores);

	for block_num in 0..env.config().num_blocks {
		gum::info!(target: LOG_TARGET, "Current block {}/{}", block_num + 1, env.config().num_blocks);
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

		gum::info!("{}", format!("{} recoveries pending", batch.len()).bright_black());
		while let Some(completed) = batch.next().await {
			let available_data = completed.unwrap().unwrap();
			env.metrics().on_pov_size(available_data.encoded_size());
			availability_bytes += available_data.encoded_size() as u128;
		}

		let block_time = Instant::now().sub(block_start_ts).as_millis() as u64;
		env.metrics().set_block_time(block_time);
		gum::info!("All work for block completed in {}", format!("{:?}ms", block_time).cyan());
	}

	let duration: u128 = start_marker.elapsed().as_millis();
	let availability_bytes = availability_bytes / 1024;
	gum::info!("All blocks processed in {}", format!("{:?}ms", duration).cyan());
	gum::info!(
		"Throughput: {}",
		format!("{} KiB/block", availability_bytes / env.config().num_blocks as u128).bright_red()
	);
	gum::info!(
		"Block time: {}",
		format!("{} ms", start_marker.elapsed().as_millis() / env.config().num_blocks as u128)
			.red()
	);

	env.display_network_usage();
	env.display_cpu_usage(&["availability-recovery"]);
	env.stop().await;
}

pub async fn benchmark_availability_write(env: &mut TestEnvironment, mut state: TestState) {
	let config = env.config().clone();
	let start_marker = Instant::now();
	let mut availability_bytes = 0u128;

	env.metrics().set_n_validators(config.n_validators);
	env.metrics().set_n_cores(config.n_cores);

	gum::info!("Seeding availability store with candidates ...");
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

		let _ = rx
			.await
			.unwrap()
			.expect("Test candidates are stored nicely in availability store");
	}

	gum::info!("Done");

	for block_num in 0..env.config().num_blocks {
		gum::info!(target: LOG_TARGET, "Current block {}/{}", block_num + 1, env.config().num_blocks);
		env.metrics().set_current_block(block_num);

		let block_start_ts = Instant::now();
		let relay_block_hash = Hash::repeat_byte(block_num as u8);
		env.import_block(new_block_import_info(relay_block_hash, block_num as BlockNumber))
			.await;

		let mut chunk_request_protocol =
			state.chunk_request_protocol().expect("No chunk fetching protocol configured");

		// Inform bitfield distribution about our view of current test block
		let message = polkadot_node_subsystem_types::messages::BitfieldDistributionMessage::NetworkBridgeUpdate(
			NetworkBridgeEvent::OurViewChange(OurView::new(vec![(relay_block_hash, Arc::new(Span::Disabled))], 0))
		);
		env.send_message(AllMessages::BitfieldDistribution(message)).await;

		// Request chunks of backed candidate from all validators
		let mut receivers = Vec::new();
		for index in 1..config.n_validators {
			let (pending_response, pending_response_receiver) = oneshot::channel();

			// Our backed candidate is first in candidate hashes entry for current block.
			let payload = ChunkFetchingRequest {
				candidate_hash: state.backed_candidates()[block_num].hash(),
				index: ValidatorIndex(index as u32),
			};
			// We don't really care.
			let peer = PeerId::random();

			// They sent it.
			env.network().peer_stats(index).inc_sent(payload.encoded_size());
			// We received it.
			env.network().inc_received(payload.encoded_size());

			// TODO: implement TX rate limiter
			if let Some(sender) = chunk_request_protocol.inbound_queue.clone() {
				receivers.push(pending_response_receiver);
				let _ = sender
					.send(
						IncomingRequest::new(PeerId::random(), payload, pending_response)
							.into_raw(),
					)
					.await;
			}
		}

		gum::info!("Waiting for all emulated peers to receive their chunk from us ...");
		for (index, receiver) in receivers.into_iter().enumerate() {
			let response = receiver.await.expect("Chunk is always served succesfully");
			assert!(response.result.is_ok());
			env.network().peer_stats(index).inc_received(response.result.encoded_size());
			env.network().inc_sent(response.result.encoded_size());
		}

		gum::info!("All chunks sent");

		// This reflects the bitfield sign timer, we expect bitfields to come in from the network
		// after it expires.
		tokio::time::sleep(std::time::Duration::from_millis(1500)).await;
		let signing_context = SigningContext { session_index: 0, parent_hash: relay_block_hash };

		// Generate `n_validator` - 1 messages and inject them to the subsystem via overseer.
		for index in 1..config.n_validators {
			let validator_public = env
				.authorities()
				.validator_public
				.get(index)
				.expect("All validator keys are known");

			// Node has all the chunks in the world.
			let payload: AvailabilityBitfield =
				AvailabilityBitfield(bitvec![u8, bitvec::order::Lsb0; 1u8; 32]);
			let signed_bitfield = Signed::<AvailabilityBitfield>::sign(
				&env.authorities().keyring.keystore(),
				payload,
				&signing_context,
				ValidatorIndex(index as u32),
				&validator_public.clone().into(),
			)
			.ok()
			.flatten()
			.expect("should be signed");

			let overseer_handle = env.overseer_handle();

			let (run, size) =
				send_peer_bitfield(overseer_handle, relay_block_hash, signed_bitfield);
			let network_action = NetworkAction::new(
				env.authorities()
					.validator_authority_id
					.get(index)
					.cloned()
					.expect("All validator keys are known"),
				run,
				size,
				None,
			);
			env.network_mut().submit_peer_action(network_action.peer(), network_action);
		}

		let block_time = Instant::now().sub(block_start_ts).as_millis() as u64;
		env.metrics().set_block_time(block_time);
		gum::info!("All work for block completed in {}", format!("{:?}ms", block_time).cyan());
	}

	let duration: u128 = start_marker.elapsed().as_millis();
	let availability_bytes = availability_bytes / 1024;
	gum::info!("All blocks processed in {}", format!("{:?}ms", duration).cyan());
	gum::info!(
		"Throughput: {}",
		format!("{} KiB/block", availability_bytes / env.config().num_blocks as u128).bright_red()
	);
	gum::info!(
		"Block time: {}",
		format!("{} ms", start_marker.elapsed().as_millis() / env.config().num_blocks as u128)
			.red()
	);

	env.display_network_usage();

	env.display_cpu_usage(&[
		"availability-distribution",
		"bitfield-distribution",
		"availability-store",
	]);

	env.stop().await;
}

pub fn send_peer_bitfield(
	mut overseer_handle: OverseerHandle,
	relay_hash: H256,
	signed_bitfield: Signed<AvailabilityBitfield>,
) -> (Pin<Box<(dyn Future<Output = ()> + std::marker::Send + 'static)>>, usize) {
	let bitfield = polkadot_node_network_protocol::v2::BitfieldDistributionMessage::Bitfield(
		relay_hash,
		signed_bitfield.into(),
	);
	let payload_size = bitfield.encoded_size();

	let message =
		polkadot_node_subsystem_types::messages::BitfieldDistributionMessage::NetworkBridgeUpdate(
			NetworkBridgeEvent::PeerMessage(PeerId::random(), Versioned::V2(bitfield)),
		);

	(
		async move {
			overseer_handle
				.send_msg(AllMessages::BitfieldDistribution(message), LOG_TARGET)
				.timeout(MAX_TIME_OF_FLIGHT)
				.await
				.unwrap_or_else(|| {
					panic!("{}ms maximum time of flight breached", MAX_TIME_OF_FLIGHT.as_millis())
				});
		}
		.boxed(),
		payload_size,
	)
}
