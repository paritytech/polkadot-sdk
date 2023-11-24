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

use std::{
	collections::{HashMap, HashSet},
	pin::Pin,
	sync::Arc,
	time::{Duration, Instant},
};

use futures::{channel::oneshot, select, Future, FutureExt};
use itertools::Itertools;
use polkadot_approval_distribution::ApprovalDistribution;
use polkadot_node_core_approval_voting::{
	criteria::{
		compute_assignments, compute_relay_vrf_modulo_assignments_v1,
		compute_relay_vrf_modulo_assignments_v2, Config,
	},
	ApprovalVotingSubsystem, Metrics,
};
use polkadot_node_primitives::approval::{
	self,
	v2::{CandidateBitfield, CoreBitfield, IndirectAssignmentCertV2, IndirectSignedApprovalVoteV2},
};

use polkadot_node_network_protocol::{
	grid_topology::{SessionGridTopology, TopologyPeerInfo},
	peer_set::{ProtocolVersion, ValidationVersion},
	vstaging as protocol_vstaging, ObservedRole, Versioned, VersionedValidationProtocol, View,
};

use polkadot_node_subsystem::{
	overseer, AllMessages, FromOrchestra, HeadSupportsParachains, Overseer, OverseerConnector,
	OverseerHandle, SpawnGlue, SpawnedSubsystem, Subsystem,
};
use polkadot_node_subsystem_test_helpers::{
	make_buffered_subsystem_context,
	mock::new_leaf,
	// mock_orchestra::{MockOverseerTest, MockOverseerTestConnector, MockOverseerTestHandle},
	TestSubsystemContext,
	TestSubsystemContextHandle,
};

use polkadot_node_core_approval_voting::{criteria, Config as ApprovalVotingConfig};
use polkadot_node_subsystem_types::{
	messages::{
		network_bridge_event::NewGossipTopology, ApprovalDistributionMessage,
		ApprovalVotingMessage, ChainApiMessage, ChainSelectionMessage, NetworkBridgeEvent,
		NetworkBridgeTxMessage, RuntimeApiMessage, RuntimeApiRequest,
	},
	ActiveLeavesUpdate, OverseerSignal,
};

use rand::{seq::SliceRandom, RngCore, SeedableRng};
use rand_chacha::ChaCha20Rng;

use polkadot_primitives::{
	vstaging::ApprovalVoteMultipleCandidates, ApprovalVote, Block, BlockNumber, CandidateEvent,
	CandidateHash, CandidateIndex, CandidateReceipt, CoreIndex, ExecutorParams, GroupIndex, Hash,
	Header, Id as ParaId, IndexedVec, SessionIndex, SessionInfo, Slot, ValidatorIndex,
	ValidatorPair, ASSIGNMENT_KEY_TYPE_ID,
};
use polkadot_primitives_test_helpers::dummy_candidate_receipt_bad_sig;
use sc_keystore::LocalKeystore;
use sc_network::PeerId;
use sc_service::SpawnTaskHandle;
use sp_consensus::SyncOracle;
use sp_consensus_babe::{
	digests::{CompatibleDigestItem, PreDigest, SecondaryVRFPreDigest},
	AllowedSlots, BabeEpochConfiguration, Epoch, Epoch as BabeEpoch, SlotDuration, VrfOutput,
	VrfProof, VrfSignature, VrfTranscript,
};
use sp_core::{crypto::VrfSecret, hexdisplay::AsBytesRef, Pair};
use sp_keystore::Keystore;
use sp_runtime::{Digest, DigestItem};

pub const LOG_TARGET: &str = "bench::approval";

const DATA_COL: u32 = 0;

pub(crate) const NUM_COLUMNS: u32 = 1;
pub(crate) const SLOT_DURATION_MILLIS: u64 = 6000;
pub(crate) const TEST_CONFIG: ApprovalVotingConfig = ApprovalVotingConfig {
	col_approval_data: DATA_COL,
	slot_duration_millis: SLOT_DURATION_MILLIS,
};

use sp_keyring::sr25519::Keyring as Sr25519Keyring;
use sp_timestamp::Timestamp;

use crate::core::{
	keyring::{self, Keyring},
	mock_subsystems::{
		MockAvailabilityDistribution, MockAvailabilityRecovery, MockAvailabilityStore,
		MockBitfieldDistribution, MockBitfieldSigning, MockCandidateBacking,
		MockCandidateValidation, MockCollationGeneration, MockCollatorProtocol,
		MockDisputeCoordinator, MockDisputeDistribution, MockGossipSupport, MockNetworkBridgeRx,
		MockNetworkBridgeTx, MockProspectiveParachains, MockProvisioner, MockPvfChecker,
		MockStatementDistribution,
	},
};

// Test parameters
const NUM_CORES: u32 = 100;
const NUM_CANDIDATES_PER_BLOCK: u32 = 70;
const NUM_HEADS: u8 = 10;
const NUM_VALIDATORS: u32 = 500;
const LAST_CONSIDERED_TRANCHE: u32 = 89;
const MIN_COALESCE: usize = 1;
const MAX_COALESCE: usize = 6;

const BUFFER_FOR_GENERATION_MILLIS: u64 = 40_000 * NUM_HEADS as u64;

use polkadot_node_subsystem::SubsystemError;
use tokio::time::sleep;

#[derive(Clone, Debug)]
struct BlockTestData {
	slot: Slot,
	hash: Hash,
	block_number: BlockNumber,
	candidates: Vec<CandidateEvent>,
	header: Header,
}

#[derive(Clone, Debug)]
pub struct TestState {
	per_slot_heads: Vec<BlockTestData>,
	identities: Vec<(Keyring, PeerId)>,
	babe_epoch: BabeEpoch,
	session_info: SessionInfo,
}

impl TestState {
	fn get_info_by_hash(&self, requested_hash: Hash) -> &BlockTestData {
		self.per_slot_heads
			.iter()
			.filter(|block| block.hash == requested_hash)
			.next()
			.unwrap()
	}

	fn get_info_by_number(&self, requested_number: u32) -> &BlockTestData {
		self.per_slot_heads
			.iter()
			.filter(|block| block.block_number == requested_number)
			.next()
			.unwrap()
	}
}

pub enum MsgPurpose {
	Approval,
	Assignment,
	Setup,
	SampleResponse(&'static str, oneshot::Receiver<Instant>),
}

struct TestMessage {
	msg: ApprovalDistributionMessage,
	purpose: MsgPurpose,
}

pub struct ApprovalSubsystemInstance {
	test_state: TestState,
	mock_overseer_handle: OverseerHandle,
	mock_overseer: Overseer<SpawnGlue<SpawnTaskHandle>, AlwaysSupportsParachains>,
	distribution_messages: HashMap<Hash, Vec<TestMessage>>,
}

struct AlwaysSupportsParachains {}
#[async_trait::async_trait]
impl HeadSupportsParachains for AlwaysSupportsParachains {
	async fn head_supports_parachains(&self, _head: &Hash) -> bool {
		true
	}
}

#[derive(Clone)]
struct TestSyncOracle {}

impl SyncOracle for TestSyncOracle {
	fn is_major_syncing(&self) -> bool {
		false
	}

	fn is_offline(&self) -> bool {
		unimplemented!("should not be used by bridge")
	}
}

impl ApprovalSubsystemInstance {
	pub fn new(spawn_task_handle: SpawnTaskHandle) -> Self {
		// Create approval voting subsystem.
		let db = kvdb_memorydb::create(NUM_COLUMNS);
		let db: polkadot_node_subsystem_util::database::kvdb_impl::DbAdapter<
			kvdb_memorydb::InMemory,
		> = polkadot_node_subsystem_util::database::kvdb_impl::DbAdapter::new(db, &[]);
		let keystore = LocalKeystore::in_memory();
		let approval_voting = ApprovalVotingSubsystem::with_config(
			TEST_CONFIG,
			Arc::new(db),
			Arc::new(keystore),
			Box::new(TestSyncOracle {}),
			Metrics::default(),
		);

		let approval_distribution = ApprovalDistribution::new(Default::default());

		// We pre-generate all messages, so we need to pick some slot into the future, that is not
		// too far in the future but is not close enough to start before we finish generating the
		// messages.
		let delta_to_first_slot_under_test = Timestamp::new(BUFFER_FOR_GENERATION_MILLIS);
		let mut slot_under_test = Slot::from_timestamp(
			Timestamp::current() + delta_to_first_slot_under_test,
			SlotDuration::from_millis(SLOT_DURATION_MILLIS),
		);

		let peer_identities = generate_node_identities();
		let mut generated_messages = HashMap::new();
		let mut peer_connected_messages = generate_peer_connected(peer_identities.clone());
		let mut per_slot_test_data = Vec::<BlockTestData>::new();
		let babe_epoch = generate_babe_epoch(slot_under_test, peer_identities.clone());
		let session_info = test_session_for_peers(&peer_identities);

		for i in 1..NUM_HEADS + 1 {
			let block_hash = Hash::repeat_byte(i);
			let parent_hash =
				per_slot_test_data.last().map(|val| val.hash).unwrap_or(Hash::repeat_byte(0xde));

			let block_info = BlockTestData {
				slot: slot_under_test,
				block_number: i as BlockNumber,
				hash: block_hash,
				header: make_header(parent_hash, slot_under_test, i as u32),
				candidates: make_candidates(
					block_hash,
					i as BlockNumber,
					NUM_CORES,
					NUM_CANDIDATES_PER_BLOCK,
				),
			};

			// Generate messages for block
			let mut per_block_messages = peer_connected_messages.drain(..).collect_vec();
			per_block_messages
				.extend(generate_peer_view_change(block_hash, peer_identities.clone()));
			per_block_messages.extend(generate_new_session_topology(peer_identities.clone()));

			gum::info!(
				"Generating assignments for {:} with num_candidates {:}",
				i,
				block_info.candidates.len()
			);

			let start = Instant::now();

			let mut assignments = generate_assignments(
				&block_info,
				peer_identities.clone(),
				&babe_epoch,
				&session_info,
				true,
			);

			gum::info!(
				"Generating approvals for {:} num_assignments generated {:} took {:} seconds",
				i,
				assignments.len(),
				start.elapsed().as_secs()
			);

			let approvals = issue_approvals(
				&assignments,
				block_hash,
				peer_identities.clone(),
				block_info.candidates.clone(),
			);

			assignments
				.push(generate_snapshot_message(&spawn_task_handle, "assignments-processed"));

			assignments.extend(approvals);
			per_block_messages.extend(assignments);

			per_block_messages
				.push(generate_snapshot_message(&spawn_task_handle, "approvals-processed"));

			gum::info!(
				"Finished generating messages for {:}  num_msg {:} took {:}",
				i,
				per_block_messages.len(),
				start.elapsed().as_secs(),
			);

			generated_messages.insert(block_hash, per_block_messages);
			per_slot_test_data.push(block_info);
			slot_under_test = slot_under_test + 1;
		}

		let state = TestState {
			per_slot_heads: per_slot_test_data,
			identities: peer_identities,
			babe_epoch,
			session_info,
		};

		// Initialize a mock overseer.
		// All subsystem except approval_voting and approval_distribution are mock subsystems.
		let spawner_glue = SpawnGlue(spawn_task_handle.clone());
		let overseer_connector = OverseerConnector::with_event_capacity(64000);
		let builder = Overseer::builder()
			.approval_voting(approval_voting)
			.approval_distribution(approval_distribution)
			.availability_recovery(MockAvailabilityRecovery {})
			.candidate_validation(MockCandidateValidation {})
			.chain_api(MockChainApi { state: state.clone() })
			.chain_selection(MockChainSelection {})
			.dispute_coordinator(MockDisputeCoordinator {})
			.runtime_api(MockRuntimeApi { state: state.clone() })
			.network_bridge_tx(MockNetworkBridgeTx {})
			.availability_distribution(MockAvailabilityDistribution {})
			.availability_store(MockAvailabilityStore {})
			.pvf_checker(MockPvfChecker {})
			.candidate_backing(MockCandidateBacking {})
			.statement_distribution(MockStatementDistribution {})
			.bitfield_signing(MockBitfieldSigning {})
			.bitfield_distribution(MockBitfieldDistribution {})
			.provisioner(MockProvisioner {})
			.network_bridge_rx(MockNetworkBridgeRx {})
			.collation_generation(MockCollationGeneration {})
			.collator_protocol(MockCollatorProtocol {})
			.gossip_support(MockGossipSupport {})
			.dispute_distribution(MockDisputeDistribution {})
			.prospective_parachains(MockProspectiveParachains {})
			.activation_external_listeners(Default::default())
			.span_per_active_leaf(Default::default())
			.active_leaves(Default::default())
			.metrics(Default::default())
			.supports_parachains(AlwaysSupportsParachains {})
			.spawner(spawner_glue);

		let (mock_overseer, mock_overseer_handle) =
			builder.build_with_connector(overseer_connector).expect("Should not fail");

		ApprovalSubsystemInstance {
			test_state: state,
			mock_overseer_handle,
			mock_overseer,
			distribution_messages: generated_messages,
		}
	}

	pub async fn run_benchmark(mut self) {
		loop {
			for block in &self.test_state.per_slot_heads {
				// Start sending all the messages at the beginning of the slot under test.
				loop {
					sleep(Duration::from_millis(50)).await;
					let mut current_slot = Slot::from_timestamp(
						Timestamp::current(),
						SlotDuration::from_millis(SLOT_DURATION_MILLIS),
					);
					if block.slot <= current_slot {
						break
					}
				}

				let slot_drift_millis =
					Timestamp::current().as_millis() - (*block.slot * SLOT_DURATION_MILLIS);
				let slot_begin = Instant::now();
				let block_hash = block.hash;
				let active_leaves = OverseerSignal::ActiveLeaves(ActiveLeavesUpdate::start_work(
					new_leaf(block_hash, 1),
				));
				gum::info!(
					target: LOG_TARGET,
					?block.hash,
					"Start sending messages for block"
				);

				self.mock_overseer.broadcast_signal(active_leaves).await.unwrap();

				// We have to wait a bit here, for the active_leaves in approval-voting to actually
				// trigger a NewBlocks in approval distribution
				// TODO: We could probably generate the NewBlocks ourselves.
				sleep(Duration::from_millis(100)).await;

				let distribution_messages =
					self.distribution_messages.get_mut(&block_hash).unwrap().drain(..);

				let mut sampled_checkpoints = Vec::new();

				for msg in distribution_messages {
					self.mock_overseer
						.route_message(AllMessages::ApprovalDistribution(msg.msg), "mock-test")
						.await
						.unwrap();

					if let MsgPurpose::SampleResponse(reason, receiver) = msg.purpose {
						sampled_checkpoints.push((reason, receiver))
					}
				}
				let block_hash = block.hash;
				let block_number = block.block_number;
				let future = async move {
					let current = Timestamp::current();

					let mut to_print = String::new();
					for (reason, timestamp) in sampled_checkpoints {
						let timestamp = timestamp.await.unwrap();
						to_print = format!(
							"{} {} in {} ms drift {} ms",
							to_print,
							reason,
							timestamp
								.checked_duration_since(slot_begin)
								.map(|duration| duration.as_millis())
								.unwrap_or(999999) + slot_drift_millis as u128,
							slot_drift_millis,
						);
					}

					gum::info!(
						target: LOG_TARGET,
						"Measured times since begining of block_number {}, block_hash {} {}",
						block_number,
						block_hash,
						to_print
					);
				};

				self.mock_overseer
					.spawner()
					.0
					.spawn_blocking("measure", Some("measure"), future);

				gum::info!(
					target: LOG_TARGET,
					?block.hash,
					"Finished sending messages for block"
				);
			}

			gum::info!(
				target: LOG_TARGET,
				"Wait for things to settle"
			);

			sleep(Duration::from_secs(100)).await;

			gum::info!(
				target: LOG_TARGET,
				"Exiting benchmark"
			);
			break
		}
	}
}

pub(crate) fn garbage_vrf_signature() -> VrfSignature {
	let transcript = VrfTranscript::new(b"test-garbage", &[]);
	Sr25519Keyring::Alice.pair().vrf_sign(&transcript.into())
}

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

fn generate_node_identities() -> Vec<(Keyring, PeerId)> {
	(0..NUM_VALIDATORS)
		.map(|peer_index| {
			(Keyring::new(format!("ApprovalNode{}", peer_index).into()), PeerId::random())
		})
		.collect::<Vec<_>>()
}

fn test_session_for_peers(keys: &[(Keyring, PeerId)]) -> SessionInfo {
	SessionInfo {
		validators: keys.iter().map(|v| v.0.clone().public().into()).collect(),
		discovery_keys: keys.iter().map(|v| v.0.clone().public().into()).collect(),
		assignment_keys: keys.iter().map(|v| v.0.clone().public().into()).collect(),
		validator_groups: IndexedVec::<GroupIndex, Vec<ValidatorIndex>>::from(
			(0..keys.len()).map(|index| vec![ValidatorIndex(index as u32)]).collect_vec(),
		),
		n_cores: NUM_CORES,
		needed_approvals: 30,
		zeroth_delay_tranche_width: 0,
		relay_vrf_modulo_samples: 6,
		n_delay_tranches: 89,
		no_show_slots: 3,
		active_validator_indices: (0..keys.len())
			.map(|index| ValidatorIndex(index as u32))
			.collect_vec(),
		dispute_period: 6,
		random_seed: [0u8; 32],
	}
}

fn coalesce_approvals_len() -> usize {
	let seed = [7u8; 32];
	let mut rand_chacha = ChaCha20Rng::from_seed(seed);
	let mut sampling: Vec<usize> = (MIN_COALESCE..MAX_COALESCE + 1).collect_vec();
	*(sampling.partial_shuffle(&mut rand_chacha, 1).0.first().unwrap())
}

fn issue_approvals(
	assignments: &Vec<TestMessage>,
	block_hash: Hash,
	keyrings: Vec<(Keyring, PeerId)>,
	candidates: Vec<CandidateEvent>,
) -> Vec<TestMessage> {
	let mut to_sign: Vec<TestCandidateInfo> = Vec::new();
	let last_index = assignments.len() - 1;

	let mut result = assignments
		.iter()
		.enumerate()
		.map(|(index, message)| match &message.msg {
			ApprovalDistributionMessage::NetworkBridgeUpdate(NetworkBridgeEvent::PeerMessage(
				_,
				Versioned::VStaging(protocol_vstaging::ApprovalDistributionMessage::Assignments(
					assignments,
				)),
			)) => {
				let mut messages = Vec::new();

				let current_validator_index =
					to_sign.first().map(|val| val.validator_index).unwrap_or(ValidatorIndex(999));

				let assignment = assignments.first().unwrap();

				if to_sign.len() >= coalesce_approvals_len() as usize ||
					(!to_sign.is_empty() && current_validator_index != assignment.0.validator)
				{
					messages.push(sign_candidates(&mut to_sign, &keyrings, block_hash))
				}

				for candidate_index in assignment.1.iter_ones() {
					let candidate = candidates.get(candidate_index).unwrap();
					if let CandidateEvent::CandidateIncluded(candidate, _, _, _) = candidate {
						let keyring =
							keyrings.get(assignment.0.validator.0 as usize).unwrap().clone();

						to_sign.push(TestCandidateInfo {
							candidate_hash: candidate.hash(),
							candidate_index: candidate_index as CandidateIndex,
							validator_index: assignment.0.validator,
						});
					} else {
						panic!("Should not happend see make_candidates");
					}
				}
				messages
			},
			_ => {
				panic!("Should not happen see generate_assignments");
			},
		})
		.collect_vec();
	let mut result = result.into_iter().flatten().collect_vec();

	if !to_sign.is_empty() {
		result.push(sign_candidates(&mut to_sign, &keyrings, block_hash));
	}
	result
}

struct TestCandidateInfo {
	candidate_hash: CandidateHash,
	candidate_index: CandidateIndex,
	validator_index: ValidatorIndex,
}

fn sign_candidates(
	to_sign: &mut Vec<TestCandidateInfo>,
	keyrings: &Vec<(Keyring, PeerId)>,
	block_hash: Hash,
) -> TestMessage {
	let current_validator_index = to_sign.first().map(|val| val.validator_index).unwrap();
	let keyring = keyrings.get(current_validator_index.0 as usize).unwrap().clone();
	let to_sign = to_sign
		.drain(..)
		.sorted_by(|val1, val2| val1.candidate_index.cmp(&val2.candidate_index))
		.collect_vec();
	let hashes = to_sign.iter().map(|val| val.candidate_hash).collect_vec();
	let candidate_indices = to_sign.iter().map(|val| val.candidate_index).collect_vec();

	let payload = ApprovalVoteMultipleCandidates(&hashes).signing_payload(1);

	let validator_key: ValidatorPair = keyring.0.pair().into();
	let signature = validator_key.sign(&payload[..]);
	let indirect = IndirectSignedApprovalVoteV2 {
		block_hash,
		candidate_indices: candidate_indices.try_into().unwrap(),
		validator: current_validator_index,
		signature,
	};
	let msg = protocol_vstaging::ApprovalDistributionMessage::Approvals(vec![indirect]);
	TestMessage {
		msg: ApprovalDistributionMessage::NetworkBridgeUpdate(NetworkBridgeEvent::PeerMessage(
			keyring.1,
			Versioned::VStaging(msg),
		)),
		purpose: MsgPurpose::Approval,
	}
}

// Generates an empty dummy message, that we send to approval distribution and
// we record the timestamp when it answered, this way we can measure how long it takes
// for the approval-distribution to process certain messages.
fn generate_snapshot_message(
	spawn_task_handle: &SpawnTaskHandle,
	label: &'static str,
) -> TestMessage {
	let (tx, rx) = oneshot::channel();
	let msg = ApprovalDistributionMessage::GetApprovalSignatures(HashSet::new(), tx);

	let (sampling_tx, sampling_rx) = oneshot::channel();
	let future = async move {
		rx.await.unwrap();
		sampling_tx.send(Instant::now()).unwrap();
	};

	spawn_task_handle.spawn_blocking(label, label, future);

	TestMessage { msg, purpose: MsgPurpose::SampleResponse(label, sampling_rx) }
}

fn make_candidate(para_id: ParaId, hash: &Hash) -> CandidateReceipt {
	let mut r = dummy_candidate_receipt_bad_sig(*hash, Some(Default::default()));
	r.descriptor.para_id = para_id;
	r
}

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

fn generate_assignments(
	block_info: &BlockTestData,
	keyrings: Vec<(Keyring, PeerId)>,
	babe_epoch: &BabeEpoch,
	session_info: &SessionInfo,
	generate_v1: bool,
) -> Vec<TestMessage> {
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
				panic!("Should not happen see make_candidates")
			}
		})
		.collect_vec();
	let mut indirect = Vec::new();

	let unsafe_vrf = approval::v1::babe_unsafe_vrf_info(&block_info.header).expect("Should be ok");
	let relay_vrf_story = unsafe_vrf
		.compute_randomness(&babe_epoch.authorities, &babe_epoch.randomness, babe_epoch.epoch_index)
		.expect("Should generate vrf_story");

	for i in 0..keyrings.len() as u32 {
		let pair = keyrings[i as usize].0.clone().pair();

		let store = LocalKeystore::in_memory();
		let public = store
			.sr25519_generate_new(
				ASSIGNMENT_KEY_TYPE_ID,
				Some(keyrings[i as usize].0.seed().as_str()),
			)
			.expect("should not fail");

		let leaving_cores = leaving_cores
			.clone()
			.into_iter()
			.filter(|(_, core_index, group_index)| core_index.0 != i)
			.collect_vec();

		let assignments = compute_assignments(
			&store,
			relay_vrf_story.clone(),
			&config,
			leaving_cores.clone(),
			false,
		);

		let mut no_duplicates = HashSet::new();
		for (core_index, assignment) in assignments {
			let assigned_cores = match &assignment.cert().kind {
				approval::v2::AssignmentCertKindV2::RelayVRFModuloCompact { core_bitfield } =>
					core_bitfield.iter_ones().map(|val| CoreIndex::from(val as u32)).collect_vec(),
				approval::v2::AssignmentCertKindV2::RelayVRFDelay { core_index } =>
					vec![*core_index],
				approval::v2::AssignmentCertKindV2::RelayVRFModulo { sample } => vec![core_index],
			};
			if assignment.tranche() > LAST_CONSIDERED_TRANCHE {
				continue
			}
			let bitfiled: CoreBitfield = assigned_cores.clone().try_into().unwrap();
			if no_duplicates.insert(bitfiled) {
				indirect.push((
					IndirectAssignmentCertV2 {
						block_hash: block_info.hash,
						validator: ValidatorIndex(i),
						cert: assignment.cert().clone(),
					},
					block_info
						.candidates
						.iter()
						.enumerate()
						.filter(|(index, candidate)| {
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
				));
			}
		}
	}

	indirect
		.into_iter()
		.map(|indirect| {
			let validator_index = indirect.0.validator.0;
			let msg = protocol_vstaging::ApprovalDistributionMessage::Assignments(vec![indirect]);
			TestMessage {
				msg: ApprovalDistributionMessage::NetworkBridgeUpdate(
					NetworkBridgeEvent::PeerMessage(
						keyrings[validator_index as usize].1,
						Versioned::VStaging(msg),
					),
				),
				purpose: MsgPurpose::Assignment,
			}
		})
		.collect_vec()
}

fn generate_peer_view_change(
	block_hash: Hash,
	keyrings: Vec<(Keyring, PeerId)>,
) -> Vec<TestMessage> {
	keyrings
		.into_iter()
		.map(|(_, peer_id)| {
			let network =
				NetworkBridgeEvent::PeerViewChange(peer_id, View::new([block_hash].into_iter(), 0));
			TestMessage {
				msg: ApprovalDistributionMessage::NetworkBridgeUpdate(network),
				purpose: MsgPurpose::Setup,
			}
		})
		.collect_vec()
}

fn generate_peer_connected(keyrings: Vec<(Keyring, PeerId)>) -> Vec<TestMessage> {
	keyrings
		.into_iter()
		.map(|(_, peer_id)| {
			let network = NetworkBridgeEvent::PeerConnected(
				peer_id,
				ObservedRole::Full,
				ProtocolVersion::from(ValidationVersion::VStaging),
				None,
			);
			TestMessage {
				msg: ApprovalDistributionMessage::NetworkBridgeUpdate(network),
				purpose: MsgPurpose::Setup,
			}
		})
		.collect_vec()
}

fn generate_new_session_topology(keyrings: Vec<(Keyring, PeerId)>) -> Vec<TestMessage> {
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

	let topology = SessionGridTopology::new(shuffled, topology);

	let event = NetworkBridgeEvent::NewGossipTopology(NewGossipTopology {
		session: 1,
		topology,
		local_index: Some(ValidatorIndex(0)), // TODO
	});
	vec![TestMessage {
		msg: ApprovalDistributionMessage::NetworkBridgeUpdate(event),
		purpose: MsgPurpose::Setup,
	}]
}

fn generate_babe_epoch(current_slot: Slot, keyrings: Vec<(Keyring, PeerId)>) -> BabeEpoch {
	let authorities = keyrings
		.into_iter()
		.enumerate()
		.map(|(index, keyring)| (keyring.0.public().into(), index as u64))
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

pub struct MockChainSelection {}
#[overseer::subsystem(ChainSelection, error=SubsystemError, prefix=self::overseer)]
impl<Context> MockChainSelection {
	fn start(self, ctx: Context) -> SpawnedSubsystem {
		let future = self.run(ctx).map(|_| Ok(())).boxed();

		SpawnedSubsystem { name: "mock-chain-subsystem", future }
	}
}

#[overseer::contextbounds(ChainSelection, prefix = self::overseer)]
impl MockChainSelection {
	async fn run<Context>(self, mut ctx: Context) {
		loop {
			let msg = ctx.recv().await.expect("Should not fail");
			match msg {
				orchestra::FromOrchestra::Signal(_) => {},
				orchestra::FromOrchestra::Communication { msg } => match msg {
					ChainSelectionMessage::Approved(hash) => {
						gum::info!(target: LOG_TARGET, ?hash, "Chain selection approved");
					},
					_ => {},
				},
			}
		}
	}
}

pub struct MockChainApi {
	state: TestState,
}
#[overseer::subsystem(ChainApi, error=SubsystemError, prefix=self::overseer)]
impl<Context> MockChainApi {
	fn start(self, ctx: Context) -> SpawnedSubsystem {
		let future = self.run(ctx).map(|_| Ok(())).boxed();

		SpawnedSubsystem { name: "chain-api-subsystem", future }
	}
}

#[overseer::contextbounds(ChainApi, prefix = self::overseer)]
impl MockChainApi {
	async fn run<Context>(self, mut ctx: Context) {
		loop {
			let msg = ctx.recv().await.expect("Should not fail");
			match msg {
				orchestra::FromOrchestra::Signal(_) => {},
				orchestra::FromOrchestra::Communication { msg } => match msg {
					ChainApiMessage::FinalizedBlockNumber(val) => {
						val.send(Ok(0)).unwrap();
					},
					ChainApiMessage::BlockHeader(requested_hash, sender) => {
						let info = self.state.get_info_by_hash(requested_hash);
						sender.send(Ok(Some(info.header.clone()))).unwrap();
					},
					ChainApiMessage::FinalizedBlockHash(requested_number, sender) => {
						let hash = self.state.get_info_by_number(requested_number).hash;
						sender.send(Ok(Some(hash))).unwrap();
					},
					ChainApiMessage::BlockNumber(requested_hash, sender) => {
						sender
							.send(Ok(Some(
								self.state.get_info_by_hash(requested_hash).block_number,
							)))
							.unwrap();
					},
					ChainApiMessage::Ancestors { hash, k, response_channel } => {
						let position = self
							.state
							.per_slot_heads
							.iter()
							.find_position(|block_info| block_info.hash == hash)
							.unwrap();
						let (ancestors, _) = self.state.per_slot_heads.split_at(position.0);

						let ancestors = ancestors.iter().rev().map(|val| val.hash).collect_vec();
						response_channel.send(Ok(ancestors)).unwrap();
					},
					_ => {},
				},
			}
		}
	}
}

pub struct MockRuntimeApi {
	state: TestState,
}
#[overseer::subsystem(RuntimeApi, error=SubsystemError, prefix=self::overseer)]
impl<Context> MockRuntimeApi {
	fn start(self, ctx: Context) -> SpawnedSubsystem {
		let future = self.run(ctx).map(|_| Ok(())).boxed();

		SpawnedSubsystem { name: "runtime-api-subsystem", future }
	}
}

#[overseer::contextbounds(RuntimeApi, prefix = self::overseer)]
impl MockRuntimeApi {
	async fn run<Context>(self, mut ctx: Context) {
		loop {
			let msg = ctx.recv().await.expect("Should not fail");

			match msg {
				orchestra::FromOrchestra::Signal(_) => {},
				orchestra::FromOrchestra::Communication { msg } => match msg {
					RuntimeApiMessage::Request(
						request,
						RuntimeApiRequest::CandidateEvents(sender),
					) => {
						let candidate_events =
							self.state.get_info_by_hash(request).candidates.clone();
						let _ = sender.send(Ok(candidate_events));
					},
					RuntimeApiMessage::Request(
						request,
						RuntimeApiRequest::SessionIndexForChild(sender),
					) => {
						let _ = sender.send(Ok(1));
					},
					RuntimeApiMessage::Request(
						request,
						RuntimeApiRequest::SessionInfo(session_index, sender),
					) => {
						let _ = sender.send(Ok(Some(self.state.session_info.clone())));
					},
					RuntimeApiMessage::Request(
						request,
						RuntimeApiRequest::SessionExecutorParams(session_index, sender),
					) => {
						let _ = sender.send(Ok(Some(ExecutorParams::default())));
					},
					RuntimeApiMessage::Request(
						request,
						RuntimeApiRequest::CurrentBabeEpoch(sender),
					) => {
						let _ = sender.send(Ok(self.state.babe_epoch.clone()));
					},
					_ => {},
				},
			}
		}
	}
}
