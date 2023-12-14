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

use parity_scale_codec::{Decode, Encode};
use serde::{Deserialize, Serialize};
use std::{
	alloc::System,
	collections::{BTreeMap, HashMap, HashSet},
	fs,
	io::{Read, Write},
	path::Path,
	sync::{
		atomic::{AtomicBool, AtomicU32, AtomicU64},
		Arc,
	},
	time::{Duration, Instant},
};

use colored::Colorize;
use futures::{channel::oneshot, lock::Mutex, Future, FutureExt, StreamExt};
use itertools::Itertools;
use orchestra::TimeoutExt;
use overseer::{messages, metrics::Metrics as OverseerMetrics, MetricsTrait};
use polkadot_approval_distribution::{
	metrics::Metrics as ApprovalDistributionMetrics, ApprovalDistribution,
};
use polkadot_node_core_approval_voting::{
	criteria::{compute_assignments, Config},
	time::{
		slot_number_to_tick, tick_to_slot_number, tranche_to_tick, Clock, ClockExt, SystemClock,
		Tick,
	},
	ApprovalVotingSubsystem, Metrics as ApprovalVotingMetrics,
};
use polkadot_node_primitives::approval::{
	self,
	v1::RelayVRFStory,
	v2::{CoreBitfield, IndirectAssignmentCertV2, IndirectSignedApprovalVoteV2},
};

use sha1::Digest as Sha1Digest;

use polkadot_node_network_protocol::{
	grid_topology::{
		GridNeighbors, RandomRouting, RequiredRouting, SessionGridTopology, TopologyPeerInfo,
	},
	peer_set::{ProtocolVersion, ValidationVersion},
	v3 as protocol_v3, ObservedRole, Versioned, View,
};
use polkadot_node_subsystem::{overseer, AllMessages, Overseer, OverseerConnector, SpawnGlue};
use polkadot_node_subsystem_test_helpers::mock::new_block_import_info;
use polkadot_overseer::Handle as OverseerHandleReal;

use polkadot_node_core_approval_voting::Config as ApprovalVotingConfig;
use polkadot_node_subsystem_types::messages::{
	network_bridge_event::NewGossipTopology, ApprovalDistributionMessage, ApprovalVotingMessage,
	NetworkBridgeEvent,
};

use rand::{seq::SliceRandom, RngCore, SeedableRng};
use rand_chacha::ChaCha20Rng;

use polkadot_primitives::{
	vstaging::ApprovalVoteMultipleCandidates, ApprovalVote, BlockNumber, CandidateEvent,
	CandidateHash, CandidateIndex, CandidateReceipt, CoreIndex, GroupIndex, Hash, Header,
	Id as ParaId, IndexedVec, SessionInfo, Slot, ValidatorIndex, ValidatorPair,
	ASSIGNMENT_KEY_TYPE_ID,
};
use polkadot_primitives_test_helpers::dummy_candidate_receipt_bad_sig;
use sc_keystore::LocalKeystore;
use sc_network::{network_state::Peer, PeerId};
use sc_service::SpawnTaskHandle;
use sp_consensus_babe::{
	digests::{CompatibleDigestItem, PreDigest, SecondaryVRFPreDigest},
	AllowedSlots, BabeEpochConfiguration, Epoch as BabeEpoch, SlotDuration, VrfSignature,
	VrfTranscript,
};
use sp_core::{crypto::VrfSecret, Pair};
use sp_keystore::Keystore;
use sp_runtime::{Digest, DigestItem};
use std::ops::Sub;

use sp_keyring::sr25519::Keyring as Sr25519Keyring;
use sp_timestamp::Timestamp;

use crate::{
	approval::{
		mock_chain_api::MockChainApi, mock_chain_selection::MockChainSelection,
		mock_runtime_api::MockRuntimeApi,
	},
	core::{
		configuration::{TestAuthorities, TestConfiguration, TestObjective},
		environment::{TestEnvironment, TestEnvironmentDependencies, MAX_TIME_OF_FLIGHT},
		keyring::Keyring,
		mock::{
			dummy_builder, AlwaysSupportsParachains, MockNetworkBridgeTx, RespondToRequestsInfo,
			TestSyncOracle,
		},
		network::{NetworkAction, NetworkEmulator},
	},
};

use tokio::time::sleep;

mod message_generator;
mod mock_chain_api;
mod mock_chain_selection;
mod mock_runtime_api;

pub const LOG_TARGET: &str = "bench::approval";
const DATA_COL: u32 = 0;
pub(crate) const NUM_COLUMNS: u32 = 1;
pub(crate) const SLOT_DURATION_MILLIS: u64 = 6000;
pub(crate) const TEST_CONFIG: ApprovalVotingConfig = ApprovalVotingConfig {
	col_approval_data: DATA_COL,
	slot_duration_millis: SLOT_DURATION_MILLIS,
};

const MESSAGES_PATH: &str = "/home/alexggh/messages_tmp/";

pub const NODE_UNDER_TEST: u32 = 0;

/// Start generating messages for a slot into the future, so that the
/// generation nevers falls behind the current slot.
const BUFFER_FOR_GENERATION_MILLIS: u64 = 30_000;

/// Parameters specific to the approvals benchmark
#[derive(Debug, Clone, Serialize, Deserialize, clap::Parser)]
#[clap(rename_all = "kebab-case")]
#[allow(missing_docs)]
pub struct ApprovalsOptions {
	#[clap(short, long, default_value_t = 89)]
	/// The last considered tranche for which we should generate message, this does not
	/// mean the message is sent, because if the block is approved no other message is sent
	/// anymore.
	pub last_considered_tranche: u32,
	#[clap(short, long, default_value_t = 1)]
	/// Min candidates to be signed in a single approval.
	pub min_coalesce: u32,
	#[clap(short, long, default_value_t = 1)]
	/// Max candidate to be signed in a single approval.
	pub max_coalesce: u32,
	/// The maximum tranche diff between approvals coalesced toghther.
	pub coalesce_tranche_diff: u32,
	#[clap(short, long, default_value_t = false)]
	/// Enable assignments v2.
	pub enable_assignments_v2: bool,
	#[clap(short, long, default_value_t = 89)]
	/// Send messages till tranche
	pub send_till_tranche: u32,
	#[clap(short, long, default_value_t = true)]
	/// Sends messages only till block is approved.
	pub stop_when_approved: bool,
}

impl ApprovalsOptions {
	fn gen_key(&self) -> Vec<u8> {
		let mut bytes = Vec::new();
		bytes.extend(self.last_considered_tranche.to_be_bytes());
		bytes.extend(self.min_coalesce.to_be_bytes());
		bytes.extend(self.max_coalesce.to_be_bytes());
		bytes.extend(self.coalesce_tranche_diff.to_be_bytes());
		bytes.extend((self.enable_assignments_v2 as i32).to_be_bytes());
		bytes
	}
}

/// Information about a block. It is part of test state and it is used by the mock
/// subsystems to be able to answer the calls approval-voting and approval-distribution
/// do into the outside world.
#[derive(Clone, Debug)]
struct BlockTestData {
	/// The slot this block occupies, see implementer's guide to understand what a slot
	/// is in the context of polkadot.
	slot: Slot,
	/// The hash of the block.
	hash: Hash,
	/// The block number.
	block_number: BlockNumber,
	/// The list of candidates included in this block.
	candidates: Vec<CandidateEvent>,
	/// The block header.
	header: Header,
	/// The vrf story for the given block.
	relay_vrf_story: RelayVRFStory,
	/// If the block has been approved by the approval-voting subsystem.
	/// This set on `true` when ChainSelectionMessage::Approved is received inside the chain
	/// selection mock subsystem.
	approved: Arc<AtomicBool>,
	prev_candidates: u64,
	votes: Arc<Vec<Vec<AtomicBool>>>,
}

/// Approval test state used by all mock subsystems to be able to answer messages emitted
/// by the approval-voting and approval-distribution-subystems.
///
/// This gets cloned across all mock subsystems, so if there is any information that gets
/// updated between subsystems, they would have to be wrapped in Arc's.
#[derive(Clone)]
pub struct ApprovalTestState {
	/// The generic test configuration passed when starting the benchmark.
	configuration: TestConfiguration,
	/// The specific test configurations passed when starting the benchmark.
	options: ApprovalsOptions,
	/// The list of blocks used for testing.
	per_slot_heads: Vec<BlockTestData>,
	/// The babe epoch used during testing.
	babe_epoch: BabeEpoch,
	/// The session info used during testing.
	session_info: SessionInfo,
	/// The slot at which this benchamrk begins.
	generated_state: GeneratedState,
	/// The test authorities
	test_authorities: TestAuthorities,
	/// Last approved block number.
	last_approved_block: Arc<AtomicU32>,
	/// Total sent messages.
	total_sent_messages: Arc<AtomicU64>,
	/// Approval voting metrics.
	approval_voting_metrics: ApprovalVotingMetrics,
	delta_tick_from_generated: Arc<AtomicU64>,
}

impl ApprovalTestState {
	/// Build a new `ApprovalTestState` object out of the configurations passed when the benchmark
	/// was tested.
	fn new(
		configuration: &TestConfiguration,
		options: ApprovalsOptions,
		dependencies: &TestEnvironmentDependencies,
	) -> Self {
		let test_authorities = configuration.generate_authorities();
		let start = Instant::now();

		let mut key = options.gen_key();
		let mut exclude_objective = configuration.clone();
		exclude_objective.objective = TestObjective::Unimplemented;
		let configuration_bytes = bincode::serialize(&exclude_objective).unwrap();
		key.extend(configuration_bytes);
		let mut sha1 = sha1::Sha1::new();
		sha1.update(key);
		let result = sha1.finalize();
		let path_name = format!("{}/{}", MESSAGES_PATH, hex::encode(result));
		let delta_to_first_slot_under_test = Timestamp::new(BUFFER_FOR_GENERATION_MILLIS);

		let messages = Path::new(&path_name);
		if !messages.exists() {
			gum::info!("Generate message because filed does not exist");
			let delta_to_first_slot_under_test = Timestamp::new(BUFFER_FOR_GENERATION_MILLIS);
			let initial_slot = Slot::from_timestamp(
				(*Timestamp::current() - *delta_to_first_slot_under_test).into(),
				SlotDuration::from_millis(SLOT_DURATION_MILLIS),
			);

			let babe_epoch = generate_babe_epoch(initial_slot, test_authorities.clone());
			let session_info = session_info_for_peers(configuration, test_authorities.clone());
			let blocks =
				Self::generate_blocks_information(configuration, &babe_epoch, initial_slot);

			let generate = ApprovalTestState::generate_messages(
				initial_slot,
				&test_authorities,
				&blocks,
				&session_info,
				&options,
				&dependencies.task_manager.spawn_handle(),
				&messages,
			);
		}

		let mut file = fs::OpenOptions::new().read(true).open(messages).unwrap();
		let mut content = Vec::<u8>::with_capacity(2000000);

		file.read_to_end(&mut content);
		let mut content = content.as_slice();
		let generated_state: GeneratedState =
			Decode::decode(&mut content).expect("Could not decode messages");

		gum::info!(
			"It took {:?} ms count {:?}",
			start.elapsed().as_millis(),
			generated_state.all_messages.as_ref().map(|val| val.len()).unwrap_or_default()
		);

		let babe_epoch =
			generate_babe_epoch(generated_state.initial_slot, test_authorities.clone());
		let session_info = session_info_for_peers(configuration, test_authorities.clone());
		let blocks = Self::generate_blocks_information(
			configuration,
			&babe_epoch,
			generated_state.initial_slot,
		);

		let mut state = ApprovalTestState {
			per_slot_heads: blocks,
			babe_epoch: babe_epoch.clone(),
			session_info: session_info.clone(),
			configuration: configuration.clone(),
			generated_state,
			test_authorities,
			last_approved_block: Arc::new(AtomicU32::new(0)),
			total_sent_messages: Arc::new(AtomicU64::new(0)),
			options,
			approval_voting_metrics: ApprovalVotingMetrics::try_register(&dependencies.registry)
				.unwrap(),
			delta_tick_from_generated: Arc::new(AtomicU64::new(630720000)),
		};
		gum::info!("Built testing state");

		state
	}

	/// Generates the blocks and the information about the blocks that will be used
	/// to drive this test.
	fn generate_blocks_information(
		configuration: &TestConfiguration,
		babe_epoch: &BabeEpoch,
		initial_slot: Slot,
	) -> Vec<BlockTestData> {
		let mut per_block_heads: Vec<BlockTestData> = Vec::new();
		let mut prev_candidates = 0;
		for block_number in 1..configuration.num_blocks + 1 {
			let block_hash = Hash::repeat_byte(block_number as u8);
			let parent_hash =
				per_block_heads.last().map(|val| val.hash).unwrap_or(Hash::repeat_byte(0xde));
			let slot_for_block = initial_slot + (block_number as u64 - 1);

			let header = make_header(parent_hash, slot_for_block, block_number as u32);

			let unsafe_vrf = approval::v1::babe_unsafe_vrf_info(&header)
				.expect("Can not continue without vrf generator");
			let relay_vrf_story = unsafe_vrf
				.compute_randomness(
					&babe_epoch.authorities,
					&babe_epoch.randomness,
					babe_epoch.epoch_index,
				)
				.expect("Can not continue without vrf story");

			let block_info = BlockTestData {
				slot: slot_for_block,
				block_number: block_number as BlockNumber,
				hash: block_hash,
				header,
				candidates: make_candidates(
					block_hash,
					block_number as BlockNumber,
					configuration.n_cores as u32,
					configuration.n_included_candidates as u32,
				),
				relay_vrf_story,
				approved: Arc::new(AtomicBool::new(false)),
				prev_candidates,
				votes: Arc::new(
					(0..configuration.n_validators)
						.map(|_| {
							(0..configuration.n_cores).map(|_| AtomicBool::new(false)).collect_vec()
						})
						.collect_vec(),
				),
			};
			prev_candidates += block_info.candidates.len() as u64;
			per_block_heads.push(block_info)
		}
		per_block_heads
	}

	/// Starts the generation of messages(Assignments & Approvals) needed for approving blocks.
	async fn start_message_production(
		&mut self,
		network_emulator: &NetworkEmulator,
		overseer_handle: OverseerHandleReal,
		spawn_task_handle: &SpawnTaskHandle,
	) -> oneshot::Receiver<()> {
		gum::info!(target: LOG_TARGET, "Start assignments/approvals production");

		let topology = generate_topology(&self.test_authorities);

		let topology_node_under_test =
			topology.compute_grid_neighbors_for(ValidatorIndex(NODE_UNDER_TEST)).unwrap();

		let (producer_tx, producer_rx) = oneshot::channel();
		let peer_message_source = PeerMessageProducer {
			network: network_emulator.clone(),
			overseer_handle: overseer_handle.clone(),
			state: self.clone(),
			options: self.options.clone(),
			notify_done: producer_tx,
		};

		peer_message_source.produce_messages(
			&spawn_task_handle,
			self.generated_state.all_messages.take().unwrap(),
		);
		producer_rx
	}

	/// Starts the generation of messages(Assignments & Approvals) needed for approving blocks.
	fn generate_messages(
		initial_slot: Slot,
		test_authorities: &TestAuthorities,
		blocks: &Vec<BlockTestData>,
		session_info: &SessionInfo,
		options: &ApprovalsOptions,
		spawn_task_handle: &SpawnTaskHandle,
		path: &Path,
	) {
		gum::info!(target: LOG_TARGET, "Generate messages");
		let topology = generate_topology(&test_authorities);

		let random_samplings = random_samplings_to_node_patterns(
			ValidatorIndex(NODE_UNDER_TEST),
			test_authorities.keyrings.len(),
			test_authorities.keyrings.len() as usize * 2,
		);

		let topology_node_under_test =
			topology.compute_grid_neighbors_for(ValidatorIndex(NODE_UNDER_TEST)).unwrap();

		let (tx, mut rx) = futures::channel::mpsc::unbounded();
		for current_validator_index in 1..test_authorities.keyrings.len() {
			let peer_message_source = message_generator::PeerMessagesGenerator {
				topology_node_under_test: topology_node_under_test.clone(),
				topology: topology.clone(),
				validator_index: ValidatorIndex(current_validator_index as u32),
				test_authorities: test_authorities.clone(),
				session_info: session_info.clone(),
				blocks: blocks.clone(),
				tx_messages: tx.clone(),
				random_samplings: random_samplings.clone(),
				enable_assignments_v2: options.enable_assignments_v2,
				last_considered_tranche: options.last_considered_tranche,
				min_coalesce: options.min_coalesce,
				max_coalesce: options.max_coalesce,
				coalesce_tranche_diff: options.coalesce_tranche_diff,
			};

			peer_message_source.generate_messages(&spawn_task_handle);
		}

		std::mem::drop(tx);

		let seed = [0x32; 32];
		let mut rand_chacha = ChaCha20Rng::from_seed(seed);
		let mut file = fs::OpenOptions::new()
			.write(true)
			.create(true)
			.truncate(true)
			.open(path)
			.unwrap();

		let mut all_messages: BTreeMap<u64, (Vec<TestMessageInfo>, Vec<TestMessageInfo>)> =
			BTreeMap::new();

		loop {
			match rx.try_next() {
				Ok(Some((block_hash, messages))) =>
					for message in messages {
						let block_info = blocks
							.iter()
							.find(|val| val.hash == block_hash)
							.expect("Should find blocks");
						let tick_to_send =
							tranche_to_tick(SLOT_DURATION_MILLIS, block_info.slot, message.tranche);
						if !all_messages.contains_key(&tick_to_send) {
							all_messages.insert(tick_to_send, (Vec::new(), Vec::new()));
						}

						let to_add = all_messages.get_mut(&tick_to_send).unwrap();
						match message.typ {
							MessageType::Approval => to_add.1.push(message),
							MessageType::Assignment => to_add.0.push(message),
							MessageType::Other => todo!(),
						}
					},
				Ok(None) => break,
				Err(_) => {
					std::thread::sleep(Duration::from_millis(50));
				},
			}
		}
		let all_messages = all_messages
			.into_iter()
			.map(|(_, (mut assignments, mut approvals))| {
				assignments.shuffle(&mut rand_chacha);
				approvals.shuffle(&mut rand_chacha);
				assignments.into_iter().chain(approvals.into_iter())
			})
			.flatten()
			.collect_vec();
		gum::info!("Took something like {:}", all_messages.len());

		let generated_state = GeneratedState { all_messages: Some(all_messages), initial_slot };

		file.write_all(&generated_state.encode());
	}
}
#[derive(Debug, Clone, Encode, Decode, PartialEq, Eq)]
struct GeneratedState {
	all_messages: Option<Vec<TestMessageInfo>>,
	initial_slot: Slot,
}

impl ApprovalTestState {
	/// Returns test data for the given hash
	fn get_info_by_hash(&self, requested_hash: Hash) -> &BlockTestData {
		self.per_slot_heads
			.iter()
			.filter(|block| block.hash == requested_hash)
			.next()
			.expect("Mocks should not use unknown hashes")
	}

	/// Returns test data for the given block number
	fn get_info_by_number(&self, requested_number: u32) -> &BlockTestData {
		self.per_slot_heads
			.iter()
			.filter(|block| block.block_number == requested_number)
			.next()
			.expect("Mocks should not use unknown numbers")
	}

	/// Returns test data for the given slot
	fn get_info_by_slot(&self, slot: Slot) -> Option<&BlockTestData> {
		self.per_slot_heads.iter().filter(|block| block.slot == slot).next()
	}
}

/// Type of generated messages.
#[derive(Debug, Copy, Clone, Encode, Decode, PartialEq, Eq)]

enum MessageType {
	Approval,
	Assignment,
	Other,
}

#[derive(Debug, Clone, Encode, Decode, PartialEq, Eq)]
struct TestMessageInfo {
	/// The actual message
	msg: protocol_v3::ApprovalDistributionMessage,
	/// The list of peers that would sends this message in a real topology.
	/// It includes both the peers that would send the message because of the topology
	/// or because of randomly chosing so.
	sent_by: Vec<ValidatorIndex>,
	/// The tranche at which this message should be sent.
	tranche: u32,
	/// The block hash this message refers to.
	block_hash: Hash,
	/// The type of the message.
	typ: MessageType,
}

impl TestMessageInfo {
	/// Returns the lantency based on the message type.
	fn to_all_messages_from_peer(self, peer: PeerId) -> AllMessages {
		AllMessages::ApprovalDistribution(ApprovalDistributionMessage::NetworkBridgeUpdate(
			NetworkBridgeEvent::PeerMessage(peer, Versioned::V3(self.msg)),
		))
	}

	fn get_latency(&self) -> Option<Duration> {
		match &self.typ {
			// We want assignments to always arrive before approval, so
			// we don't send them with a latency.
			MessageType::Approval => None,
			MessageType::Assignment => None,
			MessageType::Other => None,
		}
	}

	fn record_vote(&self, state: &BlockTestData) {
		if let MessageType::Approval = self.typ {
			match &self.msg {
				protocol_v3::ApprovalDistributionMessage::Assignments(_) => todo!(),
				protocol_v3::ApprovalDistributionMessage::Approvals(approvals) =>
					for approval in approvals {
						for candidate_index in approval.candidate_indices.iter_ones() {
							state
								.votes
								.get(approval.validator.0 as usize)
								.unwrap()
								.get(candidate_index)
								.unwrap()
								.store(true, std::sync::atomic::Ordering::SeqCst);
						}
					},
			}
		}
	}

	/// Splits a message into multiple messages based on what peers should send this message.
	/// It build a HashMap of messages that should be sent by each peer.
	fn split_by_peer_id(
		self,
		authorities: &TestAuthorities,
	) -> HashMap<(ValidatorIndex, PeerId), Vec<TestMessageInfo>> {
		let mut result: HashMap<(ValidatorIndex, PeerId), Vec<TestMessageInfo>> = HashMap::new();

		for validator_index in &self.sent_by {
			let peer = authorities.peer_ids.get(validator_index.0 as usize).unwrap();
			result.entry((*validator_index, *peer)).or_default().push(TestMessageInfo {
				msg: self.msg.clone(),
				sent_by: Default::default(),
				tranche: self.tranche,
				block_hash: self.block_hash,
				typ: self.typ,
			});
		}
		result
	}
}

/// A generator of messages coming from a given Peer/Validator
struct PeerMessageProducer {
	/// The state state used to know what messages to generate.
	state: ApprovalTestState,
	/// Configuration options, passed at the beginning of the test.
	options: ApprovalsOptions,
	/// A reference to the network emulator
	network: NetworkEmulator,
	/// A handle to the overseer, used for sending messages to the node
	/// under test.
	overseer_handle: OverseerHandleReal,
	notify_done: oneshot::Sender<()>,
}

impl PeerMessageProducer {
	/// Generates messages by spawning a blocking task in the background which begins creating
	/// the assignments/approvals and peer view changes at the begining of each block.
	fn produce_messages(
		mut self,
		spawn_task_handle: &SpawnTaskHandle,
		all_messages: Vec<TestMessageInfo>,
	) {
		spawn_task_handle.spawn_blocking("produce-messages", "produce-messages", async move {
			let mut already_generated = HashSet::new();
			let system_clock =
				FakeSystemClock::new(SystemClock {}, self.state.delta_tick_from_generated.clone());
			let mut all_messages = all_messages.into_iter().peekable();
			while all_messages.peek().is_some() {
				let current_slot =
					tick_to_slot_number(SLOT_DURATION_MILLIS, system_clock.tick_now());
				let block_info =
					self.state.get_info_by_slot(current_slot).map(|block| block.clone());

				if let Some(block_info) = block_info {
					let candidates = self.state.approval_voting_metrics.candidates_imported();
					if candidates >= block_info.prev_candidates + block_info.candidates.len() as u64 &&
						already_generated.insert(block_info.hash)
					{
						gum::info!("Setup {:?}", block_info.hash);
						let (tx, rx) = oneshot::channel();
						self.overseer_handle.wait_for_activation(block_info.hash, tx).await;

						rx.await
							.expect("We should not fail waiting for block to be activated")
							.expect("We should not fail waiting for block to be activated");

						for validator in 1..self.state.test_authorities.keyrings.len() as u32 {
							let peer_id = self
								.state
								.test_authorities
								.peer_ids
								.get(validator as usize)
								.unwrap();
							let validator = ValidatorIndex(validator);

							let view_update =
								generate_peer_view_change_for(block_info.hash, *peer_id, validator);

							self.send_message(view_update, validator, None);
						}
					}
				}

				while all_messages
					.peek()
					.map(|val| {
						let block_info = self.state.get_info_by_hash(val.block_hash);
						let tranche_now =
							system_clock.tranche_now(SLOT_DURATION_MILLIS, block_info.slot);
						let tranche_to_send = match val.typ {
							MessageType::Approval => val.tranche + 1,
							MessageType::Assignment => val.tranche,
							MessageType::Other => val.tranche,
						};
						(tranche_to_send <= tranche_now &&
							current_slot >= block_info.slot &&
							already_generated.contains(&block_info.hash)) ||
							((val.tranche > self.options.send_till_tranche ||
								self.options.stop_when_approved) && block_info
								.approved
								.load(std::sync::atomic::Ordering::SeqCst))
					})
					.unwrap_or_default()
				{
					let message = all_messages.next().unwrap();

					let block_info = self.state.get_info_by_hash(message.block_hash);
					let block_approved =
						block_info.approved.load(std::sync::atomic::Ordering::SeqCst);
					if !block_approved ||
						(!self.options.stop_when_approved &&
							message.tranche <= self.options.send_till_tranche)
					{
						message.record_vote(block_info);
						for (peer, messages) in
							message.split_by_peer_id(&self.state.test_authorities)
						{
							for message in messages {
								let latency = message.get_latency();
								self.state
									.total_sent_messages
									.as_ref()
									.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
								self.send_message(
									message.to_all_messages_from_peer(peer.1),
									peer.0,
									latency,
								)
							}
						}
					}
				}
			}

			gum::info!("All messages sent ");
			let (tx, rx) = oneshot::channel();
			let msg = ApprovalDistributionMessage::GetApprovalSignatures(HashSet::new(), tx);
			self.overseer_handle
				.send_msg(AllMessages::ApprovalDistribution(msg), LOG_TARGET)
				.timeout(MAX_TIME_OF_FLIGHT)
				.await
				.unwrap_or_else(|| {
					panic!("{} ms maximum time of flight breached", MAX_TIME_OF_FLIGHT.as_millis())
				});
			rx.await;
			self.notify_done.send(());
			gum::info!("All messages processed ");
		});
	}

	/// Queues a message to be sent by the peer identified by the `sent_by` value.
	fn send_message(
		&mut self,
		message: AllMessages,
		sent_by: ValidatorIndex,
		latency: Option<Duration>,
	) {
		let peer = self
			.state
			.test_authorities
			.validator_authority_id
			.get(sent_by.0 as usize)
			.expect("We can't handle unknown peers")
			.clone();

		let mut overseer_handle = self.overseer_handle.clone();
		let network_action = NetworkAction::new(
			peer.clone(),
			async move {
				overseer_handle
					.send_msg(message, LOG_TARGET)
					.timeout(MAX_TIME_OF_FLIGHT)
					.await
					.unwrap_or_else(|| {
						panic!(
							"{} ms maximum time of flight breached",
							MAX_TIME_OF_FLIGHT.as_millis()
						)
					});
			}
			.boxed(),
			200,
			latency,
		);
		self.network.submit_peer_action(peer, network_action);
	}
}

/// Helper function to create a a signature for the block header.
fn garbage_vrf_signature() -> VrfSignature {
	let transcript = VrfTranscript::new(b"test-garbage", &[]);
	Sr25519Keyring::Alice.pair().vrf_sign(&transcript.into())
}

/// Helper function to create a block header.
fn make_header(parent_hash: Hash, slot: Slot, number: u32) -> Header {
	let digest =
		{
			let mut digest = Digest::default();
			let vrf_signature = garbage_vrf_signature();
			digest.push(DigestItem::babe_pre_digest(PreDigest::SecondaryVRF(
				SecondaryVRFPreDigest { authority_index: 0, slot, vrf_signature },
			)));
			digest
		};

	Header {
		digest,
		extrinsics_root: Default::default(),
		number,
		state_root: Default::default(),
		parent_hash,
	}
}

/// Helper function to create a candidate receipt.
fn make_candidate(para_id: ParaId, hash: &Hash) -> CandidateReceipt {
	let mut r = dummy_candidate_receipt_bad_sig(*hash, Some(Default::default()));
	r.descriptor.para_id = para_id;
	r
}

/// Helper function to create a list of candidates that are included in the block
fn make_candidates(
	block_hash: Hash,
	block_number: BlockNumber,
	num_cores: u32,
	num_candidates: u32,
) -> Vec<CandidateEvent> {
	let seed = [block_number as u8; 32];
	let mut rand_chacha = ChaCha20Rng::from_seed(seed);
	let mut candidates = (0..num_cores)
		.map(|core| {
			CandidateEvent::CandidateIncluded(
				make_candidate(ParaId::from(core), &block_hash),
				Vec::new().into(),
				CoreIndex(core),
				GroupIndex(core),
			)
		})
		.collect_vec();
	let (candidates, _) = candidates.partial_shuffle(&mut rand_chacha, num_candidates as usize);
	candidates
		.into_iter()
		.map(|val| val.clone())
		.sorted_by(|a, b| match (a, b) {
			(
				CandidateEvent::CandidateIncluded(_, _, core_a, _),
				CandidateEvent::CandidateIncluded(_, _, core_b, _),
			) => core_a.0.cmp(&core_b.0),
			(_, _) => todo!("Should not happen"),
		})
		.collect_vec()
}

/// Generates a test session info with all passed authorities as consensus validators.
fn session_info_for_peers(
	configuration: &TestConfiguration,
	authorities: TestAuthorities,
) -> SessionInfo {
	let keys = authorities.keyrings.iter().zip(authorities.peer_ids.iter());
	SessionInfo {
		validators: keys.clone().map(|v| v.0.clone().public().into()).collect(),
		discovery_keys: keys.clone().map(|v| v.0.clone().public().into()).collect(),
		assignment_keys: keys.clone().map(|v| v.0.clone().public().into()).collect(),
		validator_groups: IndexedVec::<GroupIndex, Vec<ValidatorIndex>>::from(
			(0..authorities.keyrings.len())
				.map(|index| vec![ValidatorIndex(index as u32)])
				.collect_vec(),
		),
		n_cores: configuration.n_cores as u32,
		needed_approvals: 30,
		zeroth_delay_tranche_width: 0,
		relay_vrf_modulo_samples: 6,
		n_delay_tranches: 89,
		no_show_slots: 3,
		active_validator_indices: (0..authorities.keyrings.len())
			.map(|index| ValidatorIndex(index as u32))
			.collect_vec(),
		dispute_period: 6,
		random_seed: [0u8; 32],
	}
}

/// Helper function to randomly determine how many approvals we coalesce together in a single
/// message.
fn coalesce_approvals_len(
	min_coalesce: u32,
	max_coalesce: u32,
	rand_chacha: &mut ChaCha20Rng,
) -> usize {
	let mut sampling: Vec<usize> = (min_coalesce as usize..max_coalesce as usize + 1).collect_vec();
	*(sampling.partial_shuffle(rand_chacha, 1).0.first().unwrap())
}

/// Helper function to create approvals signatures for all assignments passed as arguments.
/// Returns a list of Approvals messages that need to be sent.
fn issue_approvals(
	assignments: &Vec<TestMessageInfo>,
	block_hash: Hash,
	keyrings: Vec<(Keyring, PeerId)>,
	candidates: Vec<CandidateEvent>,
	min_coalesce: u32,
	max_coalesce: u32,
	coalesce_tranche_diff: u32,
	rand_chacha: &mut ChaCha20Rng,
) -> Vec<TestMessageInfo> {
	let mut to_sign: Vec<TestSignInfo> = Vec::new();
	let mut num_coalesce = coalesce_approvals_len(min_coalesce, max_coalesce, rand_chacha);
	let result = assignments
		.iter()
		.enumerate()
		.map(|(_index, message)| match &message.msg {
			protocol_v3::ApprovalDistributionMessage::Assignments(assignments) => {
				let mut approvals_to_create = Vec::new();

				let current_validator_index =
					to_sign.first().map(|msg| msg.validator_index).unwrap_or(ValidatorIndex(999));

				// Invariant for this benchmark.
				assert_eq!(assignments.len(), 1);

				let assignment = assignments.first().unwrap();

				let earliest_tranche =
					to_sign.first().map(|val| val.tranche).unwrap_or(message.tranche);

				if to_sign.len() >= num_coalesce as usize ||
					(!to_sign.is_empty() && current_validator_index != assignment.0.validator) ||
					message.tranche - earliest_tranche >= coalesce_tranche_diff
				{
					num_coalesce = coalesce_approvals_len(min_coalesce, max_coalesce, rand_chacha);
					approvals_to_create.push(sign_candidates(&mut to_sign, &keyrings, block_hash))
				}

				// If more that one candidate was in the assignment queue all of them.
				for candidate_index in assignment.1.iter_ones() {
					let candidate = candidates.get(candidate_index).unwrap();
					if let CandidateEvent::CandidateIncluded(candidate, _, _, _) = candidate {
						to_sign.push(TestSignInfo {
							candidate_hash: candidate.hash(),
							candidate_index: candidate_index as CandidateIndex,
							validator_index: assignment.0.validator,
							sent_by: message.sent_by.clone().into_iter().collect(),
							tranche: message.tranche,
						});
						let earliest_tranche =
							to_sign.first().map(|val| val.tranche).unwrap_or(message.tranche);

						if to_sign.len() >= num_coalesce ||
							message.tranche - earliest_tranche >= coalesce_tranche_diff
						{
							num_coalesce =
								coalesce_approvals_len(min_coalesce, max_coalesce, rand_chacha);
							approvals_to_create.push(sign_candidates(
								&mut to_sign,
								&keyrings,
								block_hash,
							))
						}
					} else {
						todo!("Other enum variants are not used in this benchmark");
					}
				}
				approvals_to_create
			},
			_ => {
				todo!("Other enum variants are not used in this benchmark");
			},
		})
		.collect_vec();

	let mut result = result.into_iter().flatten().collect_vec();

	if !to_sign.is_empty() {
		result.push(sign_candidates(&mut to_sign, &keyrings, block_hash));
	}
	result
}

/// Helper struct to gather information about more than one candidate an sign it in a single
/// approval message.
struct TestSignInfo {
	candidate_hash: CandidateHash,
	candidate_index: CandidateIndex,
	validator_index: ValidatorIndex,
	sent_by: HashSet<ValidatorIndex>,
	tranche: u32,
}

/// Helper function to create a signture for all candidates in `to_sign` parameter.
/// Returns a TestMessage
fn sign_candidates(
	to_sign: &mut Vec<TestSignInfo>,
	keyrings: &Vec<(Keyring, PeerId)>,
	block_hash: Hash,
) -> TestMessageInfo {
	let current_validator_index = to_sign.first().map(|val| val.validator_index).unwrap();
	let tranche_trigger_timestamp = to_sign.iter().map(|val| val.tranche).max().unwrap();
	let keyring = keyrings.get(current_validator_index.0 as usize).unwrap().clone();

	let to_sign = to_sign
		.drain(..)
		.sorted_by(|val1, val2| val1.candidate_index.cmp(&val2.candidate_index))
		.collect_vec();

	let hashes = to_sign.iter().map(|val| val.candidate_hash).collect_vec();
	let candidate_indices = to_sign.iter().map(|val| val.candidate_index).collect_vec();
	let sent_by = to_sign
		.iter()
		.map(|val| val.sent_by.iter())
		.flatten()
		.map(|peer| *peer)
		.collect::<HashSet<ValidatorIndex>>();

	let payload = ApprovalVoteMultipleCandidates(&hashes).signing_payload(1);

	let validator_key: ValidatorPair = keyring.0.pair().into();
	let signature = validator_key.sign(&payload[..]);
	let indirect = IndirectSignedApprovalVoteV2 {
		block_hash,
		candidate_indices: candidate_indices.try_into().unwrap(),
		validator: current_validator_index,
		signature,
	};
	let msg = protocol_v3::ApprovalDistributionMessage::Approvals(vec![indirect]);
	TestMessageInfo {
		msg,
		sent_by: sent_by.into_iter().collect_vec(),
		tranche: tranche_trigger_timestamp,
		block_hash,
		typ: MessageType::Approval,
	}
}

fn neighbours_that_would_sent_message(
	keyrings: &Vec<(Keyring, PeerId)>,
	current_validator_index: u32,
	topology_node_under_test: &GridNeighbors,
	topology: &SessionGridTopology,
) -> Vec<(ValidatorIndex, PeerId)> {
	let topology_originator = topology
		.compute_grid_neighbors_for(ValidatorIndex(current_validator_index as u32))
		.unwrap();

	let originator_y = topology_originator
		.validator_indices_y
		.iter()
		.filter(|validator| {
			topology_node_under_test.required_routing_by_index(**validator, false) ==
				RequiredRouting::GridY
		})
		.next();
	let originator_x = topology_originator
		.validator_indices_x
		.iter()
		.filter(|validator| {
			topology_node_under_test.required_routing_by_index(**validator, false) ==
				RequiredRouting::GridX
		})
		.next();

	let is_neighbour = topology_originator
		.validator_indices_x
		.contains(&ValidatorIndex(NODE_UNDER_TEST)) ||
		topology_originator
			.validator_indices_y
			.contains(&ValidatorIndex(NODE_UNDER_TEST));

	let mut to_be_sent_by = originator_y
		.into_iter()
		.chain(originator_x)
		.map(|val| (*val, keyrings[val.0 as usize].1))
		.collect_vec();

	if is_neighbour {
		to_be_sent_by.push((ValidatorIndex(NODE_UNDER_TEST), keyrings[0].1));
	}
	to_be_sent_by
}

/// Generates assignments for the given `current_validator_index`
/// Returns a list of assignments to be sent sorted by tranche.
fn generate_assignments(
	block_info: &BlockTestData,
	keyrings: Vec<(Keyring, PeerId)>,
	session_info: &SessionInfo,
	generate_v2_assignments: bool,
	random_samplings: &Vec<Vec<ValidatorIndex>>,
	current_validator_index: u32,
	relay_vrf_story: &RelayVRFStory,
	topology_node_under_test: &GridNeighbors,
	topology: &SessionGridTopology,
	last_considered_tranche: u32,
) -> Vec<TestMessageInfo> {
	let config = Config::from(session_info);

	let leaving_cores = block_info
		.candidates
		.clone()
		.into_iter()
		.map(|candidate_event| {
			if let CandidateEvent::CandidateIncluded(candidate, _, core_index, group_index) =
				candidate_event
			{
				(candidate.hash(), core_index, group_index)
			} else {
				todo!("Variant is never created in this benchmark")
			}
		})
		.collect_vec();

	let mut assignments_by_tranche = BTreeMap::new();

	let bytes = current_validator_index.to_be_bytes();
	let seed = [
		bytes[0], bytes[1], bytes[2], bytes[3], 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
		0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
	];
	let mut rand_chacha = ChaCha20Rng::from_seed(seed);

	let to_be_sent_by = neighbours_that_would_sent_message(
		&keyrings,
		current_validator_index,
		topology_node_under_test,
		topology,
	);

	let store = LocalKeystore::in_memory();
	let _public = store
		.sr25519_generate_new(
			ASSIGNMENT_KEY_TYPE_ID,
			Some(keyrings[current_validator_index as usize].0.seed().as_str()),
		)
		.expect("should not fail");

	let leaving_cores = leaving_cores
		.clone()
		.into_iter()
		.filter(|(_, core_index, _group_index)| core_index.0 != current_validator_index)
		.collect_vec();

	let assignments = compute_assignments(
		&store,
		relay_vrf_story.clone(),
		&config,
		leaving_cores.clone(),
		generate_v2_assignments,
	);

	let random_sending_nodes = random_samplings
		.get(rand_chacha.next_u32() as usize % random_samplings.len())
		.unwrap();
	let random_sending_peer_ids = random_sending_nodes
		.into_iter()
		.map(|validator| (*validator, keyrings[validator.0 as usize].1))
		.collect_vec();

	let mut no_duplicates = HashSet::new();
	for (core_index, assignment) in assignments {
		let assigned_cores = match &assignment.cert().kind {
			approval::v2::AssignmentCertKindV2::RelayVRFModuloCompact { core_bitfield } =>
				core_bitfield.iter_ones().map(|val| CoreIndex::from(val as u32)).collect_vec(),
			approval::v2::AssignmentCertKindV2::RelayVRFDelay { core_index } => vec![*core_index],
			approval::v2::AssignmentCertKindV2::RelayVRFModulo { sample: _ } => vec![core_index],
		};
		if assignment.tranche() > last_considered_tranche {
			continue
		}

		let bitfiled: CoreBitfield = assigned_cores.clone().try_into().unwrap();

		// For the cases where tranch0 assignments are in a single certificate we need to make
		// sure we create a single message.
		if no_duplicates.insert(bitfiled) {
			if !assignments_by_tranche.contains_key(&assignment.tranche()) {
				assignments_by_tranche.insert(assignment.tranche(), Vec::new());
			}
			let this_tranche_assignments =
				assignments_by_tranche.get_mut(&assignment.tranche()).unwrap();
			this_tranche_assignments.push((
				IndirectAssignmentCertV2 {
					block_hash: block_info.hash,
					validator: ValidatorIndex(current_validator_index),
					cert: assignment.cert().clone(),
				},
				block_info
					.candidates
					.iter()
					.enumerate()
					.filter(|(_index, candidate)| {
						if let CandidateEvent::CandidateIncluded(_, _, core, _) = candidate {
							assigned_cores.contains(core)
						} else {
							panic!("Should not happen");
						}
					})
					.map(|(index, _)| index as u32)
					.collect_vec()
					.try_into()
					.unwrap(),
				to_be_sent_by
					.iter()
					.chain(random_sending_peer_ids.iter())
					.map(|peer| *peer)
					.collect::<HashSet<(ValidatorIndex, PeerId)>>(),
				assignment.tranche(),
			));
		}
	}

	let res = assignments_by_tranche
		.into_values()
		.map(|assignments| assignments.into_iter())
		.flatten()
		.map(|indirect| {
			let validator_index = indirect.0.validator.0;
			let msg = protocol_v3::ApprovalDistributionMessage::Assignments(vec![(
				indirect.0, indirect.1,
			)]);
			TestMessageInfo {
				msg,
				sent_by: indirect
					.2
					.into_iter()
					.map(|(validator_index, peer_id)| validator_index)
					.collect_vec(),
				tranche: indirect.3,
				block_hash: block_info.hash,
				typ: MessageType::Assignment,
			}
		})
		.collect_vec();

	res
}

/// A list of random samplings that we use to determine which nodes should send a given message to
/// the node under test.
/// We can not sample every time for all the messages because that would be too expensive to
/// perform, so pre-generate a list of samples for a given network size.
fn random_samplings_to_node_patterns(
	node_under_test: ValidatorIndex,
	num_validators: usize,
	num_patterns: usize,
) -> Vec<Vec<ValidatorIndex>> {
	let seed = [7u8; 32];
	let mut rand_chacha = ChaCha20Rng::from_seed(seed);

	(0..num_patterns)
		.map(|_| {
			(0..num_validators)
				.map(|sending_validator_index| {
					let mut validators = (0..num_validators).map(|val| val).collect_vec();
					validators.shuffle(&mut rand_chacha);

					let mut random_routing = RandomRouting::default();
					validators
						.into_iter()
						.flat_map(|validator_to_send| {
							if random_routing.sample(num_validators, &mut rand_chacha) {
								random_routing.inc_sent();
								if validator_to_send == node_under_test.0 as usize {
									Some(ValidatorIndex(sending_validator_index as u32))
								} else {
									None
								}
							} else {
								None
							}
						})
						.collect_vec()
				})
				.flatten()
				.collect_vec()
		})
		.collect_vec()
}

/// Generates a peer view change for the passed `block_hash`
fn generate_peer_view_change_for(
	block_hash: Hash,
	peer_id: PeerId,
	validator_index: ValidatorIndex,
) -> AllMessages {
	let network =
		NetworkBridgeEvent::PeerViewChange(peer_id, View::new([block_hash].into_iter(), 0));

	AllMessages::ApprovalDistribution(ApprovalDistributionMessage::NetworkBridgeUpdate(network))
}

/// Generates peer_connected messages for all peers in `test_authorities`
fn generate_peer_connected(
	test_authorities: &TestAuthorities,
	block_hash: Hash,
) -> Vec<AllMessages> {
	let keyrings = test_authorities
		.keyrings
		.clone()
		.into_iter()
		.zip(test_authorities.peer_ids.clone().into_iter())
		.collect_vec();
	keyrings
		.into_iter()
		.map(|(_, peer_id)| {
			let network = NetworkBridgeEvent::PeerConnected(
				peer_id,
				ObservedRole::Full,
				ProtocolVersion::from(ValidationVersion::V3),
				None,
			);
			AllMessages::ApprovalDistribution(ApprovalDistributionMessage::NetworkBridgeUpdate(
				network,
			))
		})
		.collect_vec()
}

/// Generates a topology to be used for this benchmark.
fn generate_topology(test_authorities: &TestAuthorities) -> SessionGridTopology {
	let keyrings = test_authorities
		.keyrings
		.clone()
		.into_iter()
		.zip(test_authorities.peer_ids.clone().into_iter())
		.collect_vec();

	let topology = keyrings
		.clone()
		.into_iter()
		.enumerate()
		.map(|(index, (keyring, peer_id))| TopologyPeerInfo {
			peer_ids: vec![peer_id],
			validator_index: ValidatorIndex(index as u32),
			discovery_id: keyring.public().into(),
		})
		.collect_vec();
	let shuffled = (0..keyrings.len()).collect_vec();

	SessionGridTopology::new(shuffled, topology)
}

/// Generates new session topology message.
fn generate_new_session_topology(
	test_authorities: &TestAuthorities,
	block_hash: Hash,
) -> Vec<AllMessages> {
	let topology = generate_topology(test_authorities);

	let event = NetworkBridgeEvent::NewGossipTopology(NewGossipTopology {
		session: 1,
		topology,
		local_index: Some(ValidatorIndex(NODE_UNDER_TEST)), // TODO
	});
	vec![AllMessages::ApprovalDistribution(ApprovalDistributionMessage::NetworkBridgeUpdate(event))]
}

/// Helper function to generate a  babe epoch for this benchmark.
/// It does not change for the duration of the test.
fn generate_babe_epoch(current_slot: Slot, authorities: TestAuthorities) -> BabeEpoch {
	let authorities = authorities
		.keyrings
		.into_iter()
		.enumerate()
		.map(|(index, keyring)| (keyring.public().into(), index as u64))
		.collect_vec();
	BabeEpoch {
		epoch_index: 1,
		start_slot: current_slot.saturating_sub(1u64),
		duration: 200,
		authorities,
		randomness: [0xde; 32],
		config: BabeEpochConfiguration { c: (1, 4), allowed_slots: AllowedSlots::PrimarySlots },
	}
}

/// Helper function to build an overseer with the real implementation for `ApprovalDistribution` and
/// `ApprovalVoting` subystems and mock subsytems for all others.
fn build_overseer(
	state: &ApprovalTestState,
	network: &NetworkEmulator,
	config: &TestConfiguration,
	dependencies: &TestEnvironmentDependencies,
) -> (Overseer<SpawnGlue<SpawnTaskHandle>, AlwaysSupportsParachains>, OverseerHandleReal) {
	let overseer_connector = OverseerConnector::with_event_capacity(6400000);

	let spawn_task_handle = dependencies.task_manager.spawn_handle();

	let db = kvdb_memorydb::create(NUM_COLUMNS);
	let db: polkadot_node_subsystem_util::database::kvdb_impl::DbAdapter<kvdb_memorydb::InMemory> =
		polkadot_node_subsystem_util::database::kvdb_impl::DbAdapter::new(db, &[]);
	let keystore = LocalKeystore::in_memory();
	let real_system_clock = SystemClock {};
	let system_clock =
		FakeSystemClock::new(SystemClock {}, state.delta_tick_from_generated.clone());
	let approval_voting = ApprovalVotingSubsystem::with_config(
		TEST_CONFIG,
		Arc::new(db),
		Arc::new(keystore),
		Box::new(TestSyncOracle {}),
		state.approval_voting_metrics.clone(),
		Box::new(system_clock.clone()),
	);

	let approval_distribution = ApprovalDistribution::new(
		ApprovalDistributionMetrics::try_register(&dependencies.registry).unwrap(),
	);
	let mock_chain_api = MockChainApi { state: state.clone() };
	let mock_chain_selection = MockChainSelection { state: state.clone(), clock: system_clock };
	let mock_runtime_api = MockRuntimeApi { state: state.clone() };
	let mock_tx_bridge = MockNetworkBridgeTx::new(config.clone(), state.clone(), network.clone());
	let overseer_metrics = OverseerMetrics::try_register(&dependencies.registry).unwrap();
	let dummy = dummy_builder!(spawn_task_handle, overseer_metrics)
		.replace_approval_distribution(|_| approval_distribution)
		.replace_approval_voting(|_| approval_voting)
		.replace_chain_api(|_| mock_chain_api)
		.replace_chain_selection(|_| mock_chain_selection)
		.replace_runtime_api(|_| mock_runtime_api)
		.replace_network_bridge_tx(|_| mock_tx_bridge);

	let (overseer, raw_handle) =
		dummy.build_with_connector(overseer_connector).expect("Should not fail");

	let overseer_handle = OverseerHandleReal::new(raw_handle);
	(overseer, overseer_handle)
}

/// Takes a test configuration and uses it to creates the `TestEnvironment`.
pub fn prepare_test(
	config: TestConfiguration,
	options: ApprovalsOptions,
) -> (TestEnvironment, ApprovalTestState) {
	prepare_test_inner(config, TestEnvironmentDependencies::default(), options)
}

/// Build the test environment for an Approval benchmark.
fn prepare_test_inner(
	config: TestConfiguration,
	dependencies: TestEnvironmentDependencies,
	options: ApprovalsOptions,
) -> (TestEnvironment, ApprovalTestState) {
	gum::info!("Prepare test state");
	let mut state = ApprovalTestState::new(&config, options, &dependencies);

	gum::info!("Build network emulator");

	let network = NetworkEmulator::new(&config, &dependencies, &state.test_authorities);

	gum::info!("Build overseer");

	let (overseer, overseer_handle) = build_overseer(&state, &network, &config, &dependencies);

	(TestEnvironment::new(dependencies, config, network, overseer, overseer_handle), state)
}

pub async fn bench_approvals(env: &mut TestEnvironment, mut state: ApprovalTestState) {
	let producer_rx = state
		.start_message_production(env.network(), env.overseer_handle().clone(), &env.spawn_handle())
		.await;
	bench_approvals_run(env, state, producer_rx).await
}

/// Runs the approval benchmark.
pub async fn bench_approvals_run(
	env: &mut TestEnvironment,
	state: ApprovalTestState,
	producer_rx: oneshot::Receiver<()>,
) {
	let config = env.config().clone();

	env.metrics().set_n_validators(config.n_validators);
	env.metrics().set_n_cores(config.n_cores);

	// First create the initialization messages that make sure that then node under
	// tests receives notifications about the topology used and the connected peers.
	let mut initialization_messages = generate_peer_connected(
		&state.test_authorities,
		state.per_slot_heads.first().unwrap().hash,
	);
	initialization_messages.extend(generate_new_session_topology(
		&state.test_authorities,
		state.per_slot_heads.first().unwrap().hash,
	));
	for message in initialization_messages {
		env.send_message(message).await;
	}

	let start_marker = Instant::now();
	let real_clock = SystemClock {};
	state.delta_tick_from_generated.store(
		real_clock.tick_now() -
			slot_number_to_tick(SLOT_DURATION_MILLIS, state.generated_state.initial_slot),
		std::sync::atomic::Ordering::SeqCst,
	);
	let system_clock = FakeSystemClock::new(real_clock, state.delta_tick_from_generated.clone());

	for block_num in 0..env.config().num_blocks {
		let mut current_slot = tick_to_slot_number(SLOT_DURATION_MILLIS, system_clock.tick_now());

		// Wait untill the time arrieves at the first slot under test.
		while current_slot < state.generated_state.initial_slot {
			sleep(Duration::from_millis(5)).await;
			current_slot = tick_to_slot_number(SLOT_DURATION_MILLIS, system_clock.tick_now());
		}

		gum::info!(target: LOG_TARGET, "Current block {}/{}", block_num + 1, env.config().num_blocks);
		env.metrics().set_current_block(block_num);
		let block_start_ts = Instant::now();

		if let Some(block_info) = state.get_info_by_slot(current_slot) {
			env.import_block(new_block_import_info(block_info.hash, block_info.block_number))
				.await;
		}

		// let block_time_delta = Duration::from_millis(
		// 	(*current_slot + 1) * SLOT_DURATION_MILLIS - Timestamp::current().as_millis(),
		// );
		let block_time = Instant::now().sub(block_start_ts).as_millis() as u64;
		env.metrics().set_block_time(block_time);
		gum::info!("Block time {}", format!("{:?}ms", block_time).cyan());
		// gum::info!(target: LOG_TARGET,"{}", format!("Sleeping till end of block ({}ms)",
		// block_time_delta.as_millis()).bright_black()); tokio::time::sleep(block_time_delta).
		// await;
		system_clock
			.wait(slot_number_to_tick(SLOT_DURATION_MILLIS, current_slot + 1))
			.await;
	}

	// Wait for all blocks to be approved before exiting.
	// This is an invariant of the benchmark, if this does not happen something went teribbly wrong.
	while state.last_approved_block.load(std::sync::atomic::Ordering::SeqCst) <
		env.config().num_blocks as u32
	{
		gum::info!(
			"Waiting for all blocks to be approved current approved {:} num_sent {:}",
			state.last_approved_block.load(std::sync::atomic::Ordering::SeqCst),
			state.total_sent_messages.load(std::sync::atomic::Ordering::SeqCst)
		);
		tokio::time::sleep(Duration::from_secs(6)).await;
	}

	gum::info!("Awaiting producer to signal done");

	producer_rx.await;
	gum::info!("Requesting approval votes ms");

	for info in &state.per_slot_heads {
		for (index, candidates) in info.candidates.iter().enumerate() {
			match candidates {
				CandidateEvent::CandidateBacked(_, _, _, _) => todo!(),
				CandidateEvent::CandidateIncluded(receipt_fetch, head, _, _) => {
					let (tx, rx) = oneshot::channel();

					let msg = ApprovalVotingMessage::GetApprovalSignaturesForCandidate(
						receipt_fetch.hash(),
						tx,
					);
					env.send_message(AllMessages::ApprovalVoting(msg)).await;

					let start: Instant = Instant::now();

					let result = rx.await.unwrap();

					for (validator, votes) in result.iter() {
						let result = info
							.votes
							.get(validator.0 as usize)
							.unwrap()
							.get(index)
							.unwrap()
							.store(false, std::sync::atomic::Ordering::SeqCst);
					}
				},

				CandidateEvent::CandidateTimedOut(_, _, _) => todo!(),
			};
		}
	}

	for state in &state.per_slot_heads {
		for (validator, votes) in state.votes.as_ref().iter().enumerate() {
			for (index, candidate) in votes.iter().enumerate() {
				assert_eq!(
					(validator, index, candidate.load(std::sync::atomic::Ordering::SeqCst)),
					(validator, index, false)
				);
			}
		}
	}

	env.stop().await;

	let duration: u128 = start_marker.elapsed().as_millis();
	gum::info!(
		"All blocks processed in {} num_messages {}",
		format!("{:?}ms", duration).cyan(),
		state.total_sent_messages.load(std::sync::atomic::Ordering::SeqCst)
	);

	gum::info!("{}", &env);
}

#[derive(Clone)]

struct FakeSystemClock {
	real_system_clock: SystemClock,
	delta_ticks: Arc<AtomicU64>,
}

impl FakeSystemClock {
	fn new(real_system_clock: SystemClock, delta_ticks: Arc<AtomicU64>) -> Self {
		FakeSystemClock { real_system_clock, delta_ticks }
	}
}

impl Clock for FakeSystemClock {
	fn tick_now(&self) -> Tick {
		self.real_system_clock.tick_now() -
			self.delta_ticks.load(std::sync::atomic::Ordering::SeqCst)
	}

	fn wait(
		&self,
		tick: Tick,
	) -> std::pin::Pin<Box<dyn futures::prelude::Future<Output = ()> + Send + 'static>> {
		self.real_system_clock
			.wait(tick + self.delta_ticks.load(std::sync::atomic::Ordering::SeqCst))
	}
}
