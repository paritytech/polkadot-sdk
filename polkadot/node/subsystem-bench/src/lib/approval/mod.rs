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
	approval::{
		helpers::{
			generate_babe_epoch, generate_new_session_topology, generate_peer_view_change_for,
			make_header, PastSystemClock,
		},
		message_generator::PeerMessagesGenerator,
		mock_chain_selection::MockChainSelection,
		test_message::{MessagesBundle, TestMessageInfo},
	},
	configuration::{TestAuthorities, TestConfiguration},
	dummy_builder,
	environment::{TestEnvironment, TestEnvironmentDependencies, MAX_TIME_OF_FLIGHT},
	mock::{
		chain_api::{ChainApiState, MockChainApi},
		network_bridge::{MockNetworkBridgeRx, MockNetworkBridgeTx},
		runtime_api::MockRuntimeApi,
		AlwaysSupportsParachains, TestSyncOracle,
	},
	network::{
		new_network, HandleNetworkMessage, NetworkEmulatorHandle, NetworkInterface,
		NetworkInterfaceReceiver,
	},
	usage::BenchmarkUsage,
	NODE_UNDER_TEST,
};
use colored::Colorize;
use futures::channel::oneshot;
use itertools::Itertools;
use orchestra::TimeoutExt;
use overseer::{metrics::Metrics as OverseerMetrics, MetricsTrait};
use parity_scale_codec::{Decode, Encode};
use polkadot_approval_distribution::ApprovalDistribution;
use polkadot_node_core_approval_voting::{
	time::{slot_number_to_tick, tick_to_slot_number, Clock, ClockExt, SystemClock},
	ApprovalVotingSubsystem, Config as ApprovalVotingConfig, Metrics as ApprovalVotingMetrics,
};
use polkadot_node_network_protocol::v3 as protocol_v3;
use polkadot_node_primitives::approval::{self, v1::RelayVRFStory};
use polkadot_node_subsystem::{overseer, AllMessages, Overseer, OverseerConnector, SpawnGlue};
use polkadot_node_subsystem_test_helpers::mock::new_block_import_info;
use polkadot_node_subsystem_types::messages::{ApprovalDistributionMessage, ApprovalVotingMessage};
use polkadot_node_subsystem_util::metrics::Metrics;
use polkadot_overseer::Handle as OverseerHandleReal;
use polkadot_primitives::{
	BlockNumber, CandidateEvent, CandidateIndex, CandidateReceipt, Hash, Header, Slot,
	ValidatorIndex,
};
use prometheus::Registry;
use sc_keystore::LocalKeystore;
use sc_service::SpawnTaskHandle;
use serde::{Deserialize, Serialize};
use sp_consensus_babe::Epoch as BabeEpoch;
use sp_core::H256;
use std::{
	cmp::max,
	collections::{HashMap, HashSet},
	fs,
	io::Read,
	ops::Sub,
	sync::{
		atomic::{AtomicBool, AtomicU32, AtomicU64},
		Arc,
	},
	time::{Duration, Instant},
};
use tokio::time::sleep;

mod helpers;
mod message_generator;
mod mock_chain_selection;
mod test_message;

pub(crate) const LOG_TARGET: &str = "subsystem-bench::approval";
pub(crate) const NUM_COLUMNS: u32 = 1;
pub(crate) const SLOT_DURATION_MILLIS: u64 = 6000;
pub(crate) const TEST_CONFIG: ApprovalVotingConfig = ApprovalVotingConfig {
	col_approval_data: DATA_COL,
	slot_duration_millis: SLOT_DURATION_MILLIS,
};

const DATA_COL: u32 = 0;

/// Start generating messages for a slot into the future, so that the
/// generation nevers falls behind the current slot.
const BUFFER_FOR_GENERATION_MILLIS: u64 = 30_000;

/// Parameters specific to the approvals benchmark
#[derive(Debug, Clone, Serialize, Deserialize, clap::Parser)]
#[clap(rename_all = "kebab-case")]
#[allow(missing_docs)]
pub struct ApprovalsOptions {
	#[clap(short, long, default_value_t = 89)]
	/// The last considered tranche for which we send the message.
	pub last_considered_tranche: u32,
	#[clap(short, long, default_value_t = 1.0)]
	/// Min candidates to be signed in a single approval.
	pub coalesce_mean: f32,
	#[clap(short, long, default_value_t = 1.0)]
	/// Max candidate to be signed in a single approval.
	pub coalesce_std_dev: f32,
	/// The maximum tranche diff between approvals coalesced together.
	pub coalesce_tranche_diff: u32,
	#[clap(short, long, default_value_t = false)]
	/// Enable assignments v2.
	pub enable_assignments_v2: bool,
	#[clap(short, long, default_value_t = true)]
	/// Sends messages only till block is approved.
	pub stop_when_approved: bool,
	#[clap(short, long)]
	/// Work directory.
	#[clap(short, long, default_value_t = format!("/tmp"))]
	pub workdir_prefix: String,
	/// The number of no shows per candidate
	#[clap(short, long, default_value_t = 0)]
	pub num_no_shows_per_candidate: u32,
}

impl ApprovalsOptions {
	// Generates a fingerprint use to determine if messages need to be re-generated.
	fn fingerprint(&self) -> Vec<u8> {
		let mut bytes = Vec::new();
		bytes.extend(self.coalesce_mean.to_be_bytes());
		bytes.extend(self.coalesce_std_dev.to_be_bytes());
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
	/// The total number of candidates before this block.
	total_candidates_before: u64,
	/// The votes we sent.
	/// votes[validator_index][candidate_index] tells if validator sent vote for candidate.
	/// We use this to mark the test as successful if GetApprovalSignatures returns all the votes
	/// from here.
	votes: Arc<Vec<Vec<AtomicBool>>>,
}

/// Candidate information used during the test to decide if more messages are needed.
#[derive(Debug)]
struct CandidateTestData {
	/// The configured maximum number of no-shows for this candidate.
	max_no_shows: u32,
	/// The last tranche where we had a no-show.
	last_tranche_with_no_show: u32,
	/// The number of sent assignments.
	sent_assignment: u32,
	/// The number of no-shows.
	num_no_shows: u32,
	/// The maximum tranche were we covered the needed approvals
	max_tranche: u32,
	/// Minimum needed votes to approve candidate.
	needed_approvals: u32,
}

impl CandidateTestData {
	/// If message in this tranche needs to be sent.
	fn should_send_tranche(&self, tranche: u32) -> bool {
		self.sent_assignment <= self.needed_approvals ||
			tranche <= self.max_tranche + self.num_no_shows
	}

	/// Sets max tranche
	fn set_max_tranche(&mut self, tranche: u32) {
		self.max_tranche = max(tranche, self.max_tranche);
	}

	/// Records no-show for candidate.
	fn record_no_show(&mut self, tranche: u32) {
		self.num_no_shows += 1;
		self.last_tranche_with_no_show = max(tranche, self.last_tranche_with_no_show);
	}

	/// Marks an assignment sent.
	fn mark_sent_assignment(&mut self, tranche: u32) {
		if self.sent_assignment < self.needed_approvals {
			self.set_max_tranche(tranche);
		}

		self.sent_assignment += 1;
	}

	/// Tells if a message in this tranche should be a no-show.
	fn should_no_show(&self, tranche: u32) -> bool {
		(self.num_no_shows < self.max_no_shows && self.last_tranche_with_no_show < tranche) ||
			(tranche == 0 && self.num_no_shows == 0 && self.max_no_shows > 0)
	}
}

/// Test state that is pre-generated and loaded from a file that matches the fingerprint
/// of the TestConfiguration.
#[derive(Debug, Clone, Encode, Decode, PartialEq, Eq)]
struct GeneratedState {
	/// All assignments and approvals
	all_messages: Option<Vec<test_message::MessagesBundle>>,
	/// The first slot in the test.
	initial_slot: Slot,
}

/// Approval test state used by all mock subsystems to be able to answer messages emitted
/// by the approval-voting and approval-distribution-subsystems.
///
/// This gets cloned across all mock subsystems, so if there is any information that gets
/// updated between subsystems, they would have to be wrapped in Arc's.
#[derive(Clone)]
pub struct ApprovalTestState {
	/// The main test configuration
	configuration: TestConfiguration,
	/// The specific test configurations passed when starting the benchmark.
	options: ApprovalsOptions,
	/// The list of blocks used for testing.
	blocks: Vec<BlockTestData>,
	/// The babe epoch used during testing.
	babe_epoch: BabeEpoch,
	/// The pre-generated state.
	generated_state: GeneratedState,
	/// The test authorities
	test_authorities: TestAuthorities,
	/// Last approved block number.
	last_approved_block: Arc<AtomicU32>,
	/// Total sent messages from peers to node
	total_sent_messages_to_node: Arc<AtomicU64>,
	/// Total sent messages from test node to other peers
	total_sent_messages_from_node: Arc<AtomicU64>,
	/// Total unique sent messages.
	total_unique_messages: Arc<AtomicU64>,
	/// Approval voting metrics.
	approval_voting_metrics: ApprovalVotingMetrics,
	/// The delta ticks from the tick the messages were generated to the the time we start this
	/// message.
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

		let messages_path = PeerMessagesGenerator::generate_messages_if_needed(
			configuration,
			&test_authorities,
			&options,
			&dependencies.task_manager.spawn_handle(),
		);

		let mut messages_file =
			fs::OpenOptions::new().read(true).open(messages_path.as_path()).unwrap();
		let mut messages_bytes = Vec::<u8>::with_capacity(2000000);

		messages_file
			.read_to_end(&mut messages_bytes)
			.expect("Could not initialize list of messages");
		let generated_state: GeneratedState =
			Decode::decode(&mut messages_bytes.as_slice()).expect("Could not decode messages");

		gum::info!(
			"It took {:?} ms to load {:?} unique messages",
			start.elapsed().as_millis(),
			generated_state.all_messages.as_ref().map(|val| val.len()).unwrap_or_default()
		);

		let babe_epoch =
			generate_babe_epoch(generated_state.initial_slot, test_authorities.clone());
		let blocks = Self::generate_blocks_information(
			configuration,
			&babe_epoch,
			generated_state.initial_slot,
		);

		let state = ApprovalTestState {
			blocks,
			babe_epoch: babe_epoch.clone(),
			generated_state,
			test_authorities,
			last_approved_block: Arc::new(AtomicU32::new(0)),
			total_sent_messages_to_node: Arc::new(AtomicU64::new(0)),
			total_sent_messages_from_node: Arc::new(AtomicU64::new(0)),
			total_unique_messages: Arc::new(AtomicU64::new(0)),
			options,
			approval_voting_metrics: ApprovalVotingMetrics::try_register(&dependencies.registry)
				.unwrap(),
			delta_tick_from_generated: Arc::new(AtomicU64::new(630720000)),
			configuration: configuration.clone(),
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
		for block_number in 1..=configuration.num_blocks {
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
				candidates: helpers::make_candidates(
					block_hash,
					block_number as BlockNumber,
					configuration.n_cores as u32,
					configuration.n_cores as u32,
				),
				relay_vrf_story,
				approved: Arc::new(AtomicBool::new(false)),
				total_candidates_before: prev_candidates,
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
		network_emulator: &NetworkEmulatorHandle,
		overseer_handle: OverseerHandleReal,
		env: &TestEnvironment,
		registry: Registry,
	) -> oneshot::Receiver<()> {
		gum::info!(target: LOG_TARGET, "Start assignments/approvals production");

		let (producer_tx, producer_rx) = oneshot::channel();
		let peer_message_source = PeerMessageProducer {
			network: network_emulator.clone(),
			overseer_handle: overseer_handle.clone(),
			state: self.clone(),
			options: self.options.clone(),
			notify_done: producer_tx,
			registry,
		};

		peer_message_source
			.produce_messages(env, self.generated_state.all_messages.take().unwrap());
		producer_rx
	}

	// Generates a ChainApiState used for driving MockChainApi
	fn build_chain_api_state(&self) -> ChainApiState {
		ChainApiState {
			block_headers: self
				.blocks
				.iter()
				.map(|block| (block.hash, block.header.clone()))
				.collect(),
		}
	}

	// Builds a map  with the list of candidate events per-block.
	fn candidate_events_by_block(&self) -> HashMap<H256, Vec<CandidateEvent>> {
		self.blocks.iter().map(|block| (block.hash, block.candidates.clone())).collect()
	}

	// Builds a map  with the list of candidate hashes per-block.
	fn candidate_hashes_by_block(&self) -> HashMap<H256, Vec<CandidateReceipt>> {
		self.blocks
			.iter()
			.map(|block| {
				(
					block.hash,
					block
						.candidates
						.iter()
						.map(|candidate_event| match candidate_event {
							CandidateEvent::CandidateBacked(_, _, _, _) => todo!(),
							CandidateEvent::CandidateIncluded(receipt, _, _, _) => receipt.clone(),
							CandidateEvent::CandidateTimedOut(_, _, _) => todo!(),
						})
						.collect_vec(),
				)
			})
			.collect()
	}
}

impl ApprovalTestState {
	/// Returns test data for the given hash
	fn get_info_by_hash(&self, requested_hash: Hash) -> &BlockTestData {
		self.blocks
			.iter()
			.find(|block| block.hash == requested_hash)
			.expect("Mocks should not use unknown hashes")
	}

	/// Returns test data for the given slot
	fn get_info_by_slot(&self, slot: Slot) -> Option<&BlockTestData> {
		self.blocks.iter().find(|block| block.slot == slot)
	}
}

impl HandleNetworkMessage for ApprovalTestState {
	fn handle(
		&self,
		_message: crate::network::NetworkMessage,
		_node_sender: &mut futures::channel::mpsc::UnboundedSender<crate::network::NetworkMessage>,
	) -> Option<crate::network::NetworkMessage> {
		self.total_sent_messages_from_node
			.as_ref()
			.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
		None
	}
}

/// A generator of messages coming from a given Peer/Validator
struct PeerMessageProducer {
	/// The state state used to know what messages to generate.
	state: ApprovalTestState,
	/// Configuration options, passed at the beginning of the test.
	options: ApprovalsOptions,
	/// A reference to the network emulator
	network: NetworkEmulatorHandle,
	/// A handle to the overseer, used for sending messages to the node
	/// under test.
	overseer_handle: OverseerHandleReal,
	/// Channel for producer to notify main loop it finished sending
	/// all messages and they have been processed.
	notify_done: oneshot::Sender<()>,
	/// The metrics registry.
	registry: Registry,
}

impl PeerMessageProducer {
	/// Generates messages by spawning a blocking task in the background which begins creating
	/// the assignments/approvals and peer view changes at the beginning of each block.
	fn produce_messages(
		mut self,
		env: &TestEnvironment,
		all_messages: Vec<test_message::MessagesBundle>,
	) {
		env.spawn_blocking("produce-messages", async move {
			let mut initialized_blocks = HashSet::new();
			let mut per_candidate_data: HashMap<(Hash, CandidateIndex), CandidateTestData> =
				self.initialize_candidates_test_data();
			let mut skipped_messages: Vec<test_message::MessagesBundle> = Vec::new();
			let mut re_process_skipped = false;

			let system_clock =
				PastSystemClock::new(SystemClock {}, self.state.delta_tick_from_generated.clone());
			let mut all_messages = all_messages.into_iter().peekable();

			while all_messages.peek().is_some() {
				let current_slot =
					tick_to_slot_number(SLOT_DURATION_MILLIS, system_clock.tick_now());
				let block_to_initialize = self
					.state
					.blocks
					.iter()
					.filter(|block_info| {
						block_info.slot <= current_slot &&
							!initialized_blocks.contains(&block_info.hash)
					})
					.cloned()
					.collect_vec();
				for block_info in block_to_initialize {
					if !TestEnvironment::metric_lower_than(
						&self.registry,
						"polkadot_parachain_imported_candidates_total",
						(block_info.total_candidates_before + block_info.candidates.len() as u64 -
							1) as f64,
					) {
						initialized_blocks.insert(block_info.hash);
						self.initialize_block(&block_info).await;
					}
				}

				let mut maybe_need_skip = if re_process_skipped {
					skipped_messages.clone().into_iter().peekable()
				} else {
					vec![].into_iter().peekable()
				};

				let progressing_iterator = if !re_process_skipped {
					&mut all_messages
				} else {
					re_process_skipped = false;
					skipped_messages.clear();
					&mut maybe_need_skip
				};

				while progressing_iterator
					.peek()
					.map(|bundle| {
						self.time_to_process_message(
							bundle,
							current_slot,
							&initialized_blocks,
							&system_clock,
							&per_candidate_data,
						)
					})
					.unwrap_or_default()
				{
					let bundle = progressing_iterator.next().unwrap();
					re_process_skipped = self.process_message(
						bundle,
						&mut per_candidate_data,
						&mut skipped_messages,
					) || re_process_skipped;
				}
				// Sleep, so that we don't busy wait in this loop when don't have anything to send.
				sleep(Duration::from_millis(50)).await;
			}

			gum::info!(
				"All messages sent max_tranche {:?} last_tranche_with_no_show {:?}",
				per_candidate_data.values().map(|data| data.max_tranche).max(),
				per_candidate_data.values().map(|data| data.last_tranche_with_no_show).max()
			);
			sleep(Duration::from_secs(6)).await;
			// Send an empty GetApprovalSignatures as the last message
			// so when the approval-distribution answered to it, we know it doesn't have anything
			// else to process.
			let (tx, rx) = oneshot::channel();
			let msg = ApprovalDistributionMessage::GetApprovalSignatures(HashSet::new(), tx);
			self.send_overseer_message(
				AllMessages::ApprovalDistribution(msg),
				ValidatorIndex(0),
				None,
			)
			.await;
			rx.await.expect("Failed to get signatures");
			self.notify_done.send(()).expect("Failed to notify main loop");
			gum::info!("All messages processed ");
		});
	}

	// Processes a single message bundle and queue the messages to be sent by the peers that would
	// send the message in our simulation.
	pub fn process_message(
		&mut self,
		bundle: test_message::MessagesBundle,
		per_candidate_data: &mut HashMap<(Hash, CandidateIndex), CandidateTestData>,
		skipped_messages: &mut Vec<test_message::MessagesBundle>,
	) -> bool {
		let mut reprocess_skipped = false;
		let block_info = self
			.state
			.get_info_by_hash(bundle.assignments.first().unwrap().block_hash)
			.clone();

		if bundle.should_send(per_candidate_data, &self.options) {
			bundle.record_sent_assignment(per_candidate_data);

			let assignments = bundle.assignments.clone();

			for message in bundle.assignments.into_iter().chain(bundle.approvals.into_iter()) {
				if message.no_show_if_required(&assignments, per_candidate_data) {
					reprocess_skipped = true;
					continue;
				} else {
					message.record_vote(&block_info);
				}
				self.state
					.total_unique_messages
					.as_ref()
					.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
				for (peer, messages) in
					message.clone().split_by_peer_id(&self.state.test_authorities)
				{
					for message in messages {
						self.state
							.total_sent_messages_to_node
							.as_ref()
							.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
						self.queue_message_from_peer(message, peer.0)
					}
				}
			}
		} else if !block_info.approved.load(std::sync::atomic::Ordering::SeqCst) &&
			self.options.num_no_shows_per_candidate > 0
		{
			skipped_messages.push(bundle);
		}
		reprocess_skipped
	}

	// Tells if it is the time to process a message.
	pub fn time_to_process_message(
		&self,
		bundle: &MessagesBundle,
		current_slot: Slot,
		initialized_blocks: &HashSet<Hash>,
		system_clock: &PastSystemClock,
		per_candidate_data: &HashMap<(Hash, CandidateIndex), CandidateTestData>,
	) -> bool {
		let block_info =
			self.state.get_info_by_hash(bundle.assignments.first().unwrap().block_hash);
		let tranche_now = system_clock.tranche_now(SLOT_DURATION_MILLIS, block_info.slot);

		Self::is_past_tranche(
			bundle,
			tranche_now,
			current_slot,
			block_info,
			initialized_blocks.contains(&block_info.hash),
		) || !bundle.should_send(per_candidate_data, &self.options)
	}

	// Tells if the tranche where the bundle should be sent has passed.
	pub fn is_past_tranche(
		bundle: &MessagesBundle,
		tranche_now: u32,
		current_slot: Slot,
		block_info: &BlockTestData,
		block_initialized: bool,
	) -> bool {
		bundle.tranche_to_send() <= tranche_now &&
			current_slot >= block_info.slot &&
			block_initialized
	}

	// Queue message to be sent by validator `sent_by`
	fn queue_message_from_peer(&mut self, message: TestMessageInfo, sent_by: ValidatorIndex) {
		let peer_authority_id = self
			.state
			.test_authorities
			.validator_authority_id
			.get(sent_by.0 as usize)
			.expect("We can't handle unknown peers")
			.clone();

		self.network
			.send_message_from_peer(
				&peer_authority_id,
				protocol_v3::ValidationProtocol::ApprovalDistribution(message.msg).into(),
			)
			.unwrap_or_else(|_| panic!("Network should be up and running {:?}", sent_by));
	}

	// Queues a message to be sent by the peer identified by the `sent_by` value.
	async fn send_overseer_message(
		&mut self,
		message: AllMessages,
		_sent_by: ValidatorIndex,
		_latency: Option<Duration>,
	) {
		self.overseer_handle
			.send_msg(message, LOG_TARGET)
			.timeout(MAX_TIME_OF_FLIGHT)
			.await
			.unwrap_or_else(|| {
				panic!("{} ms maximum time of flight breached", MAX_TIME_OF_FLIGHT.as_millis())
			});
	}

	// Sends the messages needed by approval-distribution and approval-voting for processing a
	// message. E.g: PeerViewChange.
	async fn initialize_block(&mut self, block_info: &BlockTestData) {
		gum::info!("Initialize block {:?}", block_info.hash);
		let (tx, rx) = oneshot::channel();
		self.overseer_handle.wait_for_activation(block_info.hash, tx).await;

		rx.await
			.expect("We should not fail waiting for block to be activated")
			.expect("We should not fail waiting for block to be activated");

		for validator in 1..self.state.test_authorities.validator_authority_id.len() as u32 {
			let peer_id = self.state.test_authorities.peer_ids.get(validator as usize).unwrap();
			let validator = ValidatorIndex(validator);
			let view_update = generate_peer_view_change_for(block_info.hash, *peer_id);

			self.send_overseer_message(view_update, validator, None).await;
		}
	}

	// Initializes the candidates test data. This is used for bookkeeping if more assignments and
	// approvals would be needed.
	fn initialize_candidates_test_data(
		&self,
	) -> HashMap<(Hash, CandidateIndex), CandidateTestData> {
		let mut per_candidate_data: HashMap<(Hash, CandidateIndex), CandidateTestData> =
			HashMap::new();
		for block_info in self.state.blocks.iter() {
			for (candidate_index, _) in block_info.candidates.iter().enumerate() {
				per_candidate_data.insert(
					(block_info.hash, candidate_index as CandidateIndex),
					CandidateTestData {
						max_no_shows: self.options.num_no_shows_per_candidate,
						last_tranche_with_no_show: 0,
						sent_assignment: 0,
						num_no_shows: 0,
						max_tranche: 0,
						needed_approvals: self.state.configuration.needed_approvals as u32,
					},
				);
			}
		}
		per_candidate_data
	}
}

/// Helper function to build an overseer with the real implementation for `ApprovalDistribution` and
/// `ApprovalVoting` subsystems and mock subsystems for all others.
fn build_overseer(
	state: &ApprovalTestState,
	network: &NetworkEmulatorHandle,
	config: &TestConfiguration,
	dependencies: &TestEnvironmentDependencies,
	network_interface: &NetworkInterface,
	network_receiver: NetworkInterfaceReceiver,
) -> (Overseer<SpawnGlue<SpawnTaskHandle>, AlwaysSupportsParachains>, OverseerHandleReal) {
	let overseer_connector = OverseerConnector::with_event_capacity(6400000);

	let spawn_task_handle = dependencies.task_manager.spawn_handle();

	let db = kvdb_memorydb::create(NUM_COLUMNS);
	let db: polkadot_node_subsystem_util::database::kvdb_impl::DbAdapter<kvdb_memorydb::InMemory> =
		polkadot_node_subsystem_util::database::kvdb_impl::DbAdapter::new(db, &[]);
	let keystore = LocalKeystore::in_memory();

	let system_clock =
		PastSystemClock::new(SystemClock {}, state.delta_tick_from_generated.clone());
	let approval_voting = ApprovalVotingSubsystem::with_config_and_clock(
		TEST_CONFIG,
		Arc::new(db),
		Arc::new(keystore),
		Box::new(TestSyncOracle {}),
		state.approval_voting_metrics.clone(),
		Box::new(system_clock.clone()),
	);

	let approval_distribution =
		ApprovalDistribution::new(Metrics::register(Some(&dependencies.registry)).unwrap());
	let mock_chain_api = MockChainApi::new(state.build_chain_api_state());
	let mock_chain_selection = MockChainSelection { state: state.clone(), clock: system_clock };
	let mock_runtime_api = MockRuntimeApi::new(
		config.clone(),
		state.test_authorities.clone(),
		state.candidate_hashes_by_block(),
		state.candidate_events_by_block(),
		Some(state.babe_epoch.clone()),
		1,
	);
	let mock_tx_bridge = MockNetworkBridgeTx::new(
		network.clone(),
		network_interface.subsystem_sender(),
		state.test_authorities.clone(),
	);
	let mock_rx_bridge = MockNetworkBridgeRx::new(network_receiver, None);
	let overseer_metrics = OverseerMetrics::try_register(&dependencies.registry).unwrap();
	let dummy = dummy_builder!(spawn_task_handle, overseer_metrics)
		.replace_approval_distribution(|_| approval_distribution)
		.replace_approval_voting(|_| approval_voting)
		.replace_chain_api(|_| mock_chain_api)
		.replace_chain_selection(|_| mock_chain_selection)
		.replace_runtime_api(|_| mock_runtime_api)
		.replace_network_bridge_tx(|_| mock_tx_bridge)
		.replace_network_bridge_rx(|_| mock_rx_bridge);

	let (overseer, raw_handle) =
		dummy.build_with_connector(overseer_connector).expect("Should not fail");

	let overseer_handle = OverseerHandleReal::new(raw_handle);
	(overseer, overseer_handle)
}

/// Takes a test configuration and uses it to creates the `TestEnvironment`.
pub fn prepare_test(
	config: TestConfiguration,
	options: ApprovalsOptions,
	with_prometheus_endpoint: bool,
) -> (TestEnvironment, ApprovalTestState) {
	prepare_test_inner(
		config,
		TestEnvironmentDependencies::default(),
		options,
		with_prometheus_endpoint,
	)
}

/// Build the test environment for an Approval benchmark.
fn prepare_test_inner(
	config: TestConfiguration,
	dependencies: TestEnvironmentDependencies,
	options: ApprovalsOptions,
	with_prometheus_endpoint: bool,
) -> (TestEnvironment, ApprovalTestState) {
	gum::info!("Prepare test state");
	let state = ApprovalTestState::new(&config, options, &dependencies);

	gum::info!("Build network emulator");

	let (network, network_interface, network_receiver) =
		new_network(&config, &dependencies, &state.test_authorities, vec![Arc::new(state.clone())]);

	gum::info!("Build overseer");

	let (overseer, overseer_handle) = build_overseer(
		&state,
		&network,
		&config,
		&dependencies,
		&network_interface,
		network_receiver,
	);

	(
		TestEnvironment::new(
			dependencies,
			config,
			network,
			overseer,
			overseer_handle,
			state.test_authorities.clone(),
			with_prometheus_endpoint,
		),
		state,
	)
}

pub async fn bench_approvals(
	benchmark_name: &str,
	env: &mut TestEnvironment,
	mut state: ApprovalTestState,
) -> BenchmarkUsage {
	let producer_rx = state
		.start_message_production(
			env.network(),
			env.overseer_handle().clone(),
			env,
			env.registry().clone(),
		)
		.await;
	bench_approvals_run(benchmark_name, env, state, producer_rx).await
}

/// Runs the approval benchmark.
pub async fn bench_approvals_run(
	benchmark_name: &str,
	env: &mut TestEnvironment,
	state: ApprovalTestState,
	producer_rx: oneshot::Receiver<()>,
) -> BenchmarkUsage {
	let config = env.config().clone();

	env.metrics().set_n_validators(config.n_validators);
	env.metrics().set_n_cores(config.n_cores);

	// First create the initialization messages that make sure that then node under
	// tests receives notifications about the topology used and the connected peers.
	let mut initialization_messages = env.network().generate_peer_connected();
	initialization_messages.extend(generate_new_session_topology(
		&state.test_authorities,
		ValidatorIndex(NODE_UNDER_TEST),
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
	let system_clock = PastSystemClock::new(real_clock, state.delta_tick_from_generated.clone());

	for block_num in 0..env.config().num_blocks {
		let mut current_slot = tick_to_slot_number(SLOT_DURATION_MILLIS, system_clock.tick_now());

		// Wait until the time arrives at the first slot under test.
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

		let block_time = Instant::now().sub(block_start_ts).as_millis() as u64;
		env.metrics().set_block_time(block_time);
		gum::info!("Block time {}", format!("{:?}ms", block_time).cyan());

		system_clock
			.wait(slot_number_to_tick(SLOT_DURATION_MILLIS, current_slot + 1))
			.await;
	}

	// Wait for all blocks to be approved before exiting.
	// This is an invariant of the benchmark, if this does not happen something went terribly wrong.
	while state.last_approved_block.load(std::sync::atomic::Ordering::SeqCst) <
		env.config().num_blocks as u32
	{
		gum::info!(
			"Waiting for all blocks to be approved current approved {:} num_sent {:} num_unique {:}",
			state.last_approved_block.load(std::sync::atomic::Ordering::SeqCst),
			state.total_sent_messages_to_node.load(std::sync::atomic::Ordering::SeqCst),
			state.total_unique_messages.load(std::sync::atomic::Ordering::SeqCst)
		);
		tokio::time::sleep(Duration::from_secs(6)).await;
	}

	gum::info!("Awaiting producer to signal done");

	producer_rx.await.expect("Failed to receive done from message producer");

	gum::info!("Awaiting polkadot_parachain_subsystem_bounded_received to tells us the messages have been processed");
	let at_least_messages =
		state.total_sent_messages_to_node.load(std::sync::atomic::Ordering::SeqCst) as usize;
	env.wait_until_metric(
		"polkadot_parachain_subsystem_bounded_received",
		Some(("subsystem_name", "approval-distribution-subsystem")),
		|value| {
			gum::info!(target: LOG_TARGET, ?value, ?at_least_messages, "Waiting metric");
			value >= at_least_messages as f64
		},
	)
	.await;
	gum::info!("Requesting approval votes ms");

	for info in &state.blocks {
		for (index, candidates) in info.candidates.iter().enumerate() {
			match candidates {
				CandidateEvent::CandidateBacked(_, _, _, _) => todo!(),
				CandidateEvent::CandidateIncluded(receipt_fetch, _head, _, _) => {
					let (tx, rx) = oneshot::channel();

					let msg = ApprovalVotingMessage::GetApprovalSignaturesForCandidate(
						receipt_fetch.hash(),
						tx,
					);
					env.send_message(AllMessages::ApprovalVoting(msg)).await;

					let result = rx.await.unwrap();

					for (validator, _) in result.iter() {
						info.votes
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

	gum::info!("Awaiting polkadot_parachain_subsystem_bounded_received to tells us the messages have been processed");
	let at_least_messages =
		state.total_sent_messages_to_node.load(std::sync::atomic::Ordering::SeqCst) as usize;
	env.wait_until_metric(
		"polkadot_parachain_subsystem_bounded_received",
		Some(("subsystem_name", "approval-distribution-subsystem")),
		|value| {
			gum::info!(target: LOG_TARGET, ?value, ?at_least_messages, "Waiting metric");
			value >= at_least_messages as f64
		},
	)
	.await;

	for state in &state.blocks {
		for (validator, votes) in state
			.votes
			.as_ref()
			.iter()
			.enumerate()
			.filter(|(validator, _)| *validator != NODE_UNDER_TEST as usize)
		{
			for (index, candidate) in votes.iter().enumerate() {
				assert_eq!(
					(
						validator,
						index,
						candidate.load(std::sync::atomic::Ordering::SeqCst),
						state.hash
					),
					(validator, index, false, state.hash)
				);
			}
		}
	}

	env.stop().await;

	let duration: u128 = start_marker.elapsed().as_millis();
	gum::info!(
		"All blocks processed in {} total_sent_messages_to_node {} total_sent_messages_from_node {} num_unique_messages {}",
		format!("{:?}ms", duration).cyan(),
		state.total_sent_messages_to_node.load(std::sync::atomic::Ordering::SeqCst),
		state.total_sent_messages_from_node.load(std::sync::atomic::Ordering::SeqCst),
		state.total_unique_messages.load(std::sync::atomic::Ordering::SeqCst)
	);

	env.collect_resource_usage(benchmark_name, &["approval-distribution", "approval-voting"])
}
