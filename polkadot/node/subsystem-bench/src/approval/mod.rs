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
	sync::Arc,
	time::{Duration, Instant},
};

pub const LOG_TARGET: &str = "bench::approval";

use futures::{channel::oneshot, select, FutureExt};
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
	v1::{IndirectAssignmentCert, IndirectSignedApprovalVote},
	v2::{CoreBitfield, IndirectAssignmentCertV2},
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

use polkadot_node_subsystem_types::{
	messages::{
		network_bridge_event::NewGossipTopology, ApprovalDistributionMessage,
		ApprovalVotingMessage, ChainApiMessage, ChainSelectionMessage, NetworkBridgeEvent,
		NetworkBridgeTxMessage, RuntimeApiMessage, RuntimeApiRequest,
	},
	ActiveLeavesUpdate, OverseerSignal,
};
use polkadot_primitives::{
	ApprovalVote, Block, BlockNumber, CandidateEvent, CandidateIndex, CandidateReceipt, CoreIndex,
	ExecutorParams, GroupIndex, Hash, Header, Id as ParaId, IndexedVec, SessionIndex, SessionInfo,
	Slot, ValidatorIndex, ValidatorPair, ASSIGNMENT_KEY_TYPE_ID,
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

pub mod test_constants {
	use polkadot_node_core_approval_voting::Config;
	const DATA_COL: u32 = 0;

	pub(crate) const NUM_COLUMNS: u32 = 1;

	pub(crate) const SLOT_DURATION_MILLIS: u64 = 6000;
	pub(crate) const TEST_CONFIG: Config =
		Config { col_approval_data: DATA_COL, slot_duration_millis: SLOT_DURATION_MILLIS };
}

const NUM_CORES: u32 = 100;
const NUM_CANDIDATES_PER_BLOCK: u32 = 70;
const NUM_HEADS: u8 = 10;
const NUM_VALIDATORS: u32 = 500;
const BUFFER_FOR_GENERATION_MILLIS: u64 = 40_000 * NUM_HEADS as u64;

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
	last_processing: HashMap<(Hash, ValidatorIndex, CandidateIndex), (Slot, u64)>,
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

pub struct ApprovalSubsystemInstance {
	test_state: TestState,
	mock_overseer_handle: OverseerHandle,
	mock_overseer: Overseer<SpawnGlue<SpawnTaskHandle>, AlwaysSupportsParachains>,
	distribution_messages: HashMap<Hash, Vec<ApprovalDistributionMessage>>,
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
		let (approval_voting_context, approval_voting_overseer) =
			make_buffered_subsystem_context::<ApprovalVotingMessage, SpawnTaskHandle>(
				spawn_task_handle.clone(),
				20000,
				"approval-voting-subsystem",
			);

		let db = kvdb_memorydb::create(test_constants::NUM_COLUMNS);
		let db: polkadot_node_subsystem_util::database::kvdb_impl::DbAdapter<
			kvdb_memorydb::InMemory,
		> = polkadot_node_subsystem_util::database::kvdb_impl::DbAdapter::new(db, &[]);

		let keystore = LocalKeystore::in_memory();
		let approval_voting = ApprovalVotingSubsystem::with_config(
			test_constants::TEST_CONFIG,
			Arc::new(db),
			Arc::new(keystore),
			Box::new(TestSyncOracle {}),
			Metrics::default(),
		);

		let db = kvdb_memorydb::create(test_constants::NUM_COLUMNS);
		let db: polkadot_node_subsystem_util::database::kvdb_impl::DbAdapter<
			kvdb_memorydb::InMemory,
		> = polkadot_node_subsystem_util::database::kvdb_impl::DbAdapter::new(db, &[]);
		let keystore = LocalKeystore::in_memory();
		let approval_voting = ApprovalVotingSubsystem::with_config(
			test_constants::TEST_CONFIG,
			Arc::new(db),
			Arc::new(keystore),
			Box::new(TestSyncOracle {}),
			Metrics::default(),
		);

		let spawner_glue = SpawnGlue(spawn_task_handle.clone());

		let approval_distribution = ApprovalDistribution::new(Default::default());
		let overseer_connector = OverseerConnector::with_event_capacity(64000);
		let delta_for_generation = Timestamp::new(BUFFER_FOR_GENERATION_MILLIS);
		let mut current_slot = Slot::from_timestamp(
			Timestamp::current() + delta_for_generation,
			SlotDuration::from_millis(test_constants::SLOT_DURATION_MILLIS),
		);
		let identities = generate_ids();
		let mut last_processing = HashMap::new();

		let mut generated_messages = HashMap::new();
		let mut peer_connected_messages = generate_peer_connected(identities.clone());
		let mut per_slot_heads = Vec::<BlockTestData>::new();
		let babe_epoch = generate_babe_epoch(current_slot, identities.clone());
		let session_info = dummy_session_info2(&identities);

		for i in 1..NUM_HEADS + 1 {
			let block_hash = Hash::repeat_byte(i);
			let parent_hash =
				per_slot_heads.last().map(|val| val.hash).unwrap_or(Hash::repeat_byte(0xde));
			let block_info = BlockTestData {
				slot: current_slot,
				block_number: i as BlockNumber,
				hash: block_hash,
				header: make_header(parent_hash, current_slot, i as u32),
				candidates: make_candidates(
					block_hash,
					i as BlockNumber,
					NUM_CORES,
					NUM_CANDIDATES_PER_BLOCK,
				),
			};

			let mut per_block_messages = peer_connected_messages.drain(..).collect_vec();
			per_block_messages.extend(generate_peer_view_change(block_hash, identities.clone()));
			per_block_messages.extend(generate_new_session_topology(identities.clone()));
			println!(
				"Generating assignments for {:} with candidates {:}",
				i,
				block_info.candidates.len()
			);
			let start = Instant::now();
			let mut assignments = generate_many_assignments(
				&block_info,
				identities.clone(),
				&babe_epoch,
				&session_info,
				true,
			);
			println!(
				"Generating approvals for {:} assignments generated {:} took {:} seconds",
				i,
				assignments.len(),
				start.elapsed().as_secs()
			);

			assignments.extend(issue_approvals(
				&assignments,
				block_hash,
				identities.clone(),
				block_info.candidates.clone(),
			));
			println!(
				"Finished generating messages for {:} took {:} seconds",
				i,
				start.elapsed().as_secs()
			);
			per_block_messages.extend(assignments);
			let last_message = key_from_message(per_block_messages.last().unwrap());
			last_processing.insert(last_message, (current_slot, per_block_messages.len() as u64));

			generated_messages.insert(block_hash, per_block_messages);
			per_slot_heads.push(block_info);
			current_slot = current_slot + 1;
		}
		let state =
			TestState { per_slot_heads, identities, babe_epoch, session_info, last_processing };

		let builder = Overseer::builder()
			.approval_voting(approval_voting)
			.approval_distribution(approval_distribution)
			.availability_recovery(MockAvailabilityRecovery {})
			.candidate_validation(MockCandidateValidation {})
			.chain_api(MockChainApi { state: state.clone() })
			.chain_selection(MockChainSelection {})
			.dispute_coordinator(MockDisputeCoordinator {})
			.runtime_api(MockRuntimeApi { state: state.clone() })
			.network_bridge_tx(MockNetworkBridgeTx { state: state.clone() })
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
				loop {
					sleep(Duration::from_millis(50)).await;
					let mut current_slot = Slot::from_timestamp(
						Timestamp::current(),
						SlotDuration::from_millis(test_constants::SLOT_DURATION_MILLIS),
					);
					if block.slot <= current_slot {
						break
					}
				}
				let block_hash = block.hash;
				let active_leaves = OverseerSignal::ActiveLeaves(ActiveLeavesUpdate::start_work(
					new_leaf(block_hash, 1),
				));
				gum::info!(target: LOG_TARGET, "Sending active leaves ===========");

				self.mock_overseer.broadcast_signal(active_leaves).await;
				gum::info!(target: LOG_TARGET, "Broadcasted active leaves ===========");

				sleep(Duration::from_millis(100)).await;
				gum::info!(target: LOG_TARGET, "Sending messages");

				let distribution_messages =
					self.distribution_messages.get_mut(&block_hash).unwrap().drain(..);

				for msg in distribution_messages {
					self.mock_overseer
						.route_message(AllMessages::ApprovalDistribution(msg), "mock-test")
						.await;
				}

				println!(
					"approval_voting:  ======================    Sent messages==================="
				);
			}

			println!("approval_voting:  ======================    Sleep a bit ===================");
			sleep(Duration::from_secs(30)).await;
			let (tx, rx) = oneshot::channel();
			let target = self.test_state.per_slot_heads.last().unwrap().hash;
			let msg = ApprovalVotingMessage::ApprovedAncestor(target, 0, tx);
			println!("approval_voting:  ======================    Approval request ancestor request  =================== {:?} ", target);

			self.mock_overseer
				.route_message(AllMessages::ApprovalVoting(msg), "mock-test")
				.await;
			let result = rx.await;
			println!("approval_voting:  ======================    Approval get ancestor =================== {:?} {:?}", target, result);

			sleep(Duration::from_secs(100)).await;
			panic!("Exiting hard")
		}
	}
}

use sp_keyring::sr25519::Keyring as Sr25519Keyring;
use sp_timestamp::Timestamp;

use crate::core::keyring::{self, Keyring};

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

fn generate_ids() -> Vec<(Keyring, PeerId)> {
	(0..NUM_VALIDATORS)
		.map(|peer_index| {
			(Keyring::new(format!("ApprovalNode{}", peer_index).into()), PeerId::random())
		})
		.collect::<Vec<_>>()
}

fn dummy_session_info(identities: Vec<(Keyring, PeerId)>) -> SessionInfo {
	dummy_session_info2(&identities)
}

fn dummy_session_info2(keys: &[(Keyring, PeerId)]) -> SessionInfo {
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

fn issue_approvals(
	assignments: &Vec<ApprovalDistributionMessage>,
	block_hash: Hash,
	keyrings: Vec<(Keyring, PeerId)>,
	candidates: Vec<CandidateEvent>,
) -> Vec<ApprovalDistributionMessage> {
	let result = assignments
		.iter()
		.map(|message| match message {
			ApprovalDistributionMessage::NetworkBridgeUpdate(NetworkBridgeEvent::PeerMessage(
				_,
				Versioned::VStaging(msg),
			)) =>
				if let protocol_vstaging::ApprovalDistributionMessage::Assignments(assignments) =
					msg
				{
					let assignment = assignments.first().unwrap();
					let mut messages = Vec::new();
					for candidate_index in assignment.1.iter_ones() {
						let candidate = candidates.get(candidate_index).unwrap();
						if let CandidateEvent::CandidateIncluded(candidate, _, _, _) = candidate {
							let keyring =
								keyrings.get(assignment.0.validator.0 as usize).unwrap().clone();
							let payload = ApprovalVote(candidate.hash()).signing_payload(1);
							let validator_key: ValidatorPair = keyring.0.pair().into();
							let signature = validator_key.sign(&payload[..]);
							let indirect = IndirectSignedApprovalVote {
								block_hash,
								candidate_index: candidate_index as u32,
								validator: assignment.0.validator,
								signature,
							};
							let msg =
								protocol_vstaging::ApprovalDistributionMessage::Approvals(vec![
									indirect,
								]);
							messages.push(ApprovalDistributionMessage::NetworkBridgeUpdate(
								NetworkBridgeEvent::PeerMessage(
									keyring.1,
									Versioned::VStaging(msg),
								),
							))
						} else {
							panic!("Should not happend");
						}
					}
					messages
				} else {
					panic!("Should not happen");
				},
			_ => {
				panic!("Should happen");
			},
		})
		.collect_vec();
	result.into_iter().flatten().collect_vec()
}

fn make_candidate(para_id: ParaId, hash: &Hash) -> CandidateReceipt {
	let mut r = dummy_candidate_receipt_bad_sig(*hash, Some(Default::default()));
	r.descriptor.para_id = para_id;
	r
}

use rand::{seq::SliceRandom, SeedableRng};
use rand_chacha::ChaCha20Rng;

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

fn generate_many_assignments(
	block_info: &BlockTestData,
	keyrings: Vec<(Keyring, PeerId)>,
	babe_epoch: &BabeEpoch,
	session_info: &SessionInfo,
	generate_v1: bool,
) -> Vec<ApprovalDistributionMessage> {
	let config = Config::from(session_info);
	let mut count_tranches = vec![0u32; 200];
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
				panic!("SHOULD NOT HAPPEN")
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
		// let mut assignments = HashMap::new();
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

		// if generate_v1 {
		// 	compute_relay_vrf_modulo_assignments_v1(
		// 		&keyrings[i as usize].0.clone().pair().into(),
		// 		ValidatorIndex(i),
		// 		&config,
		// 		relay_vrf_story.clone(),
		// 		leaving_cores.clone(),
		// 		&mut assignments,
		// 	);
		// } else {
		// 	compute_relay_vrf_modulo_assignments_v2(
		// 		&keyrings[i as usize].0.clone().pair().into(),
		// 		ValidatorIndex(i),
		// 		&config,
		// 		relay_vrf_story.clone(),
		// 		leaving_cores.clone(),
		// 		&mut assignments,
		// 	);
		// }
		let mut no_duplicates = HashSet::new();
		for (core_index, assignment) in assignments {
			let assigned_cores = match &assignment.cert().kind {
				approval::v2::AssignmentCertKindV2::RelayVRFModuloCompact { core_bitfield } =>
					core_bitfield.iter_ones().map(|val| CoreIndex::from(val as u32)).collect_vec(),
				approval::v2::AssignmentCertKindV2::RelayVRFDelay { core_index } =>
					vec![*core_index],
				approval::v2::AssignmentCertKindV2::RelayVRFModulo { sample } => vec![core_index],
			};
			count_tranches.get_mut(assignment.tranche() as usize).map(|val| *val = *val + 1);
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
	let mut sum = 0;
	for (tranche, count) in count_tranches.iter().enumerate() {
		sum += count;
		if *count > 0 {
			gum::info!("Generated at tranche {} num_assignments {} ", tranche, sum);
		}
	}
	indirect
		.into_iter()
		.map(|indirect| {
			let validator_index = indirect.0.validator.0;
			let msg = protocol_vstaging::ApprovalDistributionMessage::Assignments(vec![indirect]);
			ApprovalDistributionMessage::NetworkBridgeUpdate(NetworkBridgeEvent::PeerMessage(
				keyrings[validator_index as usize].1,
				Versioned::VStaging(msg),
			))
		})
		.collect_vec()
}

fn generate_peer_view_change(
	block_hash: Hash,
	keyrings: Vec<(Keyring, PeerId)>,
) -> Vec<ApprovalDistributionMessage> {
	keyrings
		.into_iter()
		.map(|(_, peer_id)| {
			let network =
				NetworkBridgeEvent::PeerViewChange(peer_id, View::new([block_hash].into_iter(), 0));
			ApprovalDistributionMessage::NetworkBridgeUpdate(network)
		})
		.collect_vec()
}

fn generate_peer_connected(keyrings: Vec<(Keyring, PeerId)>) -> Vec<ApprovalDistributionMessage> {
	keyrings
		.into_iter()
		.map(|(_, peer_id)| {
			let network = NetworkBridgeEvent::PeerConnected(
				peer_id,
				ObservedRole::Full,
				ProtocolVersion::from(ValidationVersion::VStaging),
				None,
			);
			ApprovalDistributionMessage::NetworkBridgeUpdate(network)
		})
		.collect_vec()
}

fn generate_new_session_topology(
	keyrings: Vec<(Keyring, PeerId)>,
) -> Vec<ApprovalDistributionMessage> {
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
	vec![ApprovalDistributionMessage::NetworkBridgeUpdate(event)]
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

pub struct MockAvailabilityRecovery {}
use polkadot_node_subsystem::SubsystemError;

#[overseer::subsystem(AvailabilityRecovery, error=SubsystemError, prefix=self::overseer)]
impl<Context> MockAvailabilityRecovery {
	fn start(self, ctx: Context) -> SpawnedSubsystem {
		let future = self.run(ctx).map(|_| Ok(())).boxed();

		SpawnedSubsystem { name: "availability-recover-subsystem", future }
	}
}
use tokio::time::sleep;

use self::test_constants::SLOT_DURATION_MILLIS;
#[overseer::contextbounds(AvailabilityRecovery, prefix = self::overseer)]
impl MockAvailabilityRecovery {
	async fn run<Context>(self, mut ctx: Context) {
		loop {
			sleep(Duration::from_secs(3)).await;
		}
	}
}

/// TODO: Make this a macro
pub struct MockCandidateValidation {}
#[overseer::subsystem(CandidateValidation, error=SubsystemError, prefix=self::overseer)]
impl<Context> MockCandidateValidation {
	fn start(self, ctx: Context) -> SpawnedSubsystem {
		let future = self.run(ctx).map(|_| Ok(())).boxed();

		SpawnedSubsystem { name: "candidate-validation-subsystem", future }
	}
}

#[overseer::contextbounds(CandidateValidation, prefix = self::overseer)]
impl MockCandidateValidation {
	async fn run<Context>(self, mut ctx: Context) {
		loop {
			sleep(Duration::from_secs(3)).await;
		}
	}
}

pub struct MockChainSelection {}
#[overseer::subsystem(ChainSelection, error=SubsystemError, prefix=self::overseer)]
impl<Context> MockChainSelection {
	fn start(self, ctx: Context) -> SpawnedSubsystem {
		let future = self.run(ctx).map(|_| Ok(())).boxed();

		SpawnedSubsystem { name: "candidate-validation-subsystem", future }
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

pub struct MockDisputeCoordinator {}
#[overseer::subsystem(DisputeCoordinator, error=SubsystemError, prefix=self::overseer)]
impl<Context> MockDisputeCoordinator {
	fn start(self, ctx: Context) -> SpawnedSubsystem {
		let future = self.run(ctx).map(|_| Ok(())).boxed();

		SpawnedSubsystem { name: "candidate-validation-subsystem", future }
	}
}

#[overseer::contextbounds(DisputeCoordinator, prefix = self::overseer)]
impl MockDisputeCoordinator {
	async fn run<Context>(self, mut ctx: Context) {
		loop {
			sleep(Duration::from_secs(3)).await;
		}
	}
}

pub struct MockNetworkBridgeTx {
	state: TestState,
}
#[overseer::subsystem(NetworkBridgeTx, error=SubsystemError, prefix=self::overseer)]
impl<Context> MockNetworkBridgeTx {
	fn start(self, ctx: Context) -> SpawnedSubsystem {
		let future = self.run(ctx).map(|_| Ok(())).boxed();

		SpawnedSubsystem { name: "candidate-validation-subsystem", future }
	}
}

#[overseer::contextbounds(NetworkBridgeTx, prefix = self::overseer)]
impl MockNetworkBridgeTx {
	async fn run<Context>(self, mut ctx: Context) {
		loop {
			let msg = ctx.recv().await.expect("Should not fail");
			match msg {
				orchestra::FromOrchestra::Signal(_) => {},
				orchestra::FromOrchestra::Communication { msg } => {
					println!(
						" ============== Received message Spent above 5000 time {:?} ms =====================",
						msg,
					);
					let message_key = match msg {
						NetworkBridgeTxMessage::SendValidationMessage(val, msg) => match msg {
							Versioned::VStaging(msg) => match msg {
								protocol_vstaging::ValidationProtocol::ApprovalDistribution(
									protocol_vstaging::ApprovalDistributionMessage::Approvals(
										approvals,
									),
								) => {
									let approval = approvals.last().unwrap();
									(
										approval.block_hash,
										approval.validator,
										approval.candidate_index,
									)
								},
								_ => (Hash::repeat_byte(0xfe), ValidatorIndex(9999), 999),
							},
							_ => (Hash::repeat_byte(0xfe), ValidatorIndex(9999), 999),
						},
						_ => (Hash::repeat_byte(0xfe), ValidatorIndex(9999), 999),
					};

					if let Some((slot, num_messages)) = self.state.last_processing.get(&message_key)
					{
						let slot: u64 = **slot;
						let timestamp_millis = slot * SLOT_DURATION_MILLIS;
						let current = Timestamp::current();
						gum::info!(
							target: LOG_TARGET,
							"Last message processed diff {:?} num {:?} ms {:?}",
							message_key,
							num_messages,
							current.as_millis() - timestamp_millis
						);
					}
				},
			}
			// sleep(Duration::from_secs(3)).await;
		}
	}
}

pub struct MockAvailabilityDistribution {}
#[overseer::subsystem(AvailabilityDistribution, error=SubsystemError, prefix=self::overseer)]
impl<Context> MockAvailabilityDistribution {
	fn start(self, ctx: Context) -> SpawnedSubsystem {
		let future = self.run(ctx).map(|_| Ok(())).boxed();

		SpawnedSubsystem { name: "candidate-validation-subsystem", future }
	}
}

#[overseer::contextbounds(AvailabilityDistribution, prefix = self::overseer)]
impl MockAvailabilityDistribution {
	async fn run<Context>(self, mut ctx: Context) {
		loop {
			sleep(Duration::from_secs(3)).await;
		}
	}
}

pub struct MockAvailabilityStore {}
#[overseer::subsystem(AvailabilityStore, error=SubsystemError, prefix=self::overseer)]
impl<Context> MockAvailabilityStore {
	fn start(self, ctx: Context) -> SpawnedSubsystem {
		let future = self.run(ctx).map(|_| Ok(())).boxed();

		SpawnedSubsystem { name: "candidate-validation-subsystem", future }
	}
}

#[overseer::contextbounds(AvailabilityStore, prefix = self::overseer)]
impl MockAvailabilityStore {
	async fn run<Context>(self, mut ctx: Context) {
		loop {
			sleep(Duration::from_secs(3)).await;
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

		SpawnedSubsystem { name: "candidate-validation-subsystem", future }
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
						val.send(Ok(0));
					},
					ChainApiMessage::BlockHeader(requested_hash, sender) => {
						let info = self.state.get_info_by_hash(requested_hash);
						sender.send(Ok(Some(info.header.clone())));
					},
					ChainApiMessage::FinalizedBlockHash(requested_number, sender) => {
						let hash = self.state.get_info_by_number(requested_number).hash;
						sender.send(Ok(Some(hash)));
					},
					ChainApiMessage::BlockNumber(requested_hash, sender) => {
						sender.send(Ok(Some(
							self.state.get_info_by_hash(requested_hash).block_number,
						)));
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
						response_channel.send(Ok(ancestors));
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

		SpawnedSubsystem { name: "candidate-validation-subsystem", future }
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
						sender.send(Ok(candidate_events));
					},
					RuntimeApiMessage::Request(
						request,
						RuntimeApiRequest::SessionIndexForChild(sender),
					) => {
						sender.send(Ok(1));
					},
					RuntimeApiMessage::Request(
						request,
						RuntimeApiRequest::SessionInfo(session_index, sender),
					) => {
						gum::info!(target: LOG_TARGET, "Request session info");
						sender.send(Ok(Some(self.state.session_info.clone())));
						gum::info!(target: LOG_TARGET, "Send session info");
					},
					RuntimeApiMessage::Request(
						request,
						RuntimeApiRequest::SessionExecutorParams(session_index, sender),
					) => {
						sender.send(Ok(Some(ExecutorParams::default())));
					},
					RuntimeApiMessage::Request(
						request,
						RuntimeApiRequest::CurrentBabeEpoch(sender),
					) => {
						sender.send(Ok(self.state.babe_epoch.clone()));
					},
					_ => {},
				},
			}
		}
	}
}

pub struct MockPvfChecker {}
#[overseer::subsystem(PvfChecker, error=SubsystemError, prefix=self::overseer)]
impl<Context> MockPvfChecker {
	fn start(self, ctx: Context) -> SpawnedSubsystem {
		let future = self.run(ctx).map(|_| Ok(())).boxed();

		SpawnedSubsystem { name: "candidate-validation-subsystem", future }
	}
}

#[overseer::contextbounds(PvfChecker, prefix = self::overseer)]
impl MockPvfChecker {
	async fn run<Context>(self, mut ctx: Context) {
		loop {
			sleep(Duration::from_secs(3)).await;
		}
	}
}

pub struct MockCandidateBacking {}
#[overseer::subsystem(CandidateBacking, error=SubsystemError, prefix=self::overseer)]
impl<Context> MockCandidateBacking {
	fn start(self, ctx: Context) -> SpawnedSubsystem {
		let future = self.run(ctx).map(|_| Ok(())).boxed();

		SpawnedSubsystem { name: "candidate-validation-subsystem", future }
	}
}

#[overseer::contextbounds(CandidateBacking, prefix = self::overseer)]
impl MockCandidateBacking {
	async fn run<Context>(self, mut ctx: Context) {
		loop {
			sleep(Duration::from_secs(3)).await;
		}
	}
}

pub struct MockStatementDistribution {}
#[overseer::subsystem(StatementDistribution, error=SubsystemError, prefix=self::overseer)]
impl<Context> MockStatementDistribution {
	fn start(self, ctx: Context) -> SpawnedSubsystem {
		let future = self.run(ctx).map(|_| Ok(())).boxed();

		SpawnedSubsystem { name: "candidate-validation-subsystem", future }
	}
}

#[overseer::contextbounds(StatementDistribution, prefix = self::overseer)]
impl MockStatementDistribution {
	async fn run<Context>(self, mut ctx: Context) {
		loop {
			sleep(Duration::from_secs(3)).await;
		}
	}
}

pub struct MockBitfieldSigning {}
#[overseer::subsystem(BitfieldSigning, error=SubsystemError, prefix=self::overseer)]
impl<Context> MockBitfieldSigning {
	fn start(self, ctx: Context) -> SpawnedSubsystem {
		let future = self.run(ctx).map(|_| Ok(())).boxed();

		SpawnedSubsystem { name: "candidate-validation-subsystem", future }
	}
}

#[overseer::contextbounds(BitfieldSigning, prefix = self::overseer)]
impl MockBitfieldSigning {
	async fn run<Context>(self, mut ctx: Context) {
		loop {
			sleep(Duration::from_secs(3)).await;
		}
	}
}

pub struct MockBitfieldDistribution {}
#[overseer::subsystem(BitfieldDistribution, error=SubsystemError, prefix=self::overseer)]
impl<Context> MockBitfieldDistribution {
	fn start(self, ctx: Context) -> SpawnedSubsystem {
		let future = self.run(ctx).map(|_| Ok(())).boxed();

		SpawnedSubsystem { name: "candidate-validation-subsystem", future }
	}
}

#[overseer::contextbounds(BitfieldDistribution, prefix = self::overseer)]
impl MockBitfieldDistribution {
	async fn run<Context>(self, mut ctx: Context) {
		loop {
			sleep(Duration::from_secs(3)).await;
		}
	}
}

pub struct MockProvisioner {}
#[overseer::subsystem(Provisioner, error=SubsystemError, prefix=self::overseer)]
impl<Context> MockProvisioner {
	fn start(self, ctx: Context) -> SpawnedSubsystem {
		let future = self.run(ctx).map(|_| Ok(())).boxed();

		SpawnedSubsystem { name: "candidate-validation-subsystem", future }
	}
}

#[overseer::contextbounds(Provisioner, prefix = self::overseer)]
impl MockProvisioner {
	async fn run<Context>(self, mut ctx: Context) {
		loop {
			sleep(Duration::from_secs(3)).await;
		}
	}
}

pub struct MockNetworkBridgeRx {}
#[overseer::subsystem(NetworkBridgeRx, error=SubsystemError, prefix=self::overseer)]
impl<Context> MockNetworkBridgeRx {
	fn start(self, ctx: Context) -> SpawnedSubsystem {
		let future = self.run(ctx).map(|_| Ok(())).boxed();

		SpawnedSubsystem { name: "candidate-validation-subsystem", future }
	}
}

#[overseer::contextbounds(NetworkBridgeRx, prefix = self::overseer)]
impl MockNetworkBridgeRx {
	async fn run<Context>(self, mut ctx: Context) {
		loop {
			sleep(Duration::from_secs(3)).await;
		}
	}
}

pub struct MockCollationGeneration {}
#[overseer::subsystem(CollationGeneration, error=SubsystemError, prefix=self::overseer)]
impl<Context> MockCollationGeneration {
	fn start(self, ctx: Context) -> SpawnedSubsystem {
		let future = self.run(ctx).map(|_| Ok(())).boxed();

		SpawnedSubsystem { name: "candidate-validation-subsystem", future }
	}
}

#[overseer::contextbounds(CollationGeneration, prefix = self::overseer)]
impl MockCollationGeneration {
	async fn run<Context>(self, mut ctx: Context) {
		loop {
			sleep(Duration::from_secs(3)).await;
		}
	}
}

pub struct MockCollatorProtocol {}
#[overseer::subsystem(CollatorProtocol, error=SubsystemError, prefix=self::overseer)]
impl<Context> MockCollatorProtocol {
	fn start(self, ctx: Context) -> SpawnedSubsystem {
		let future = self.run(ctx).map(|_| Ok(())).boxed();

		SpawnedSubsystem { name: "candidate-validation-subsystem", future }
	}
}

#[overseer::contextbounds(CollatorProtocol, prefix = self::overseer)]
impl MockCollatorProtocol {
	async fn run<Context>(self, mut ctx: Context) {
		loop {
			sleep(Duration::from_secs(3)).await;
		}
	}
}

pub struct MockGossipSupport {}
#[overseer::subsystem(GossipSupport, error=SubsystemError, prefix=self::overseer)]
impl<Context> MockGossipSupport {
	fn start(self, ctx: Context) -> SpawnedSubsystem {
		let future = self.run(ctx).map(|_| Ok(())).boxed();

		SpawnedSubsystem { name: "candidate-validation-subsystem", future }
	}
}

#[overseer::contextbounds(GossipSupport, prefix = self::overseer)]
impl MockGossipSupport {
	async fn run<Context>(self, mut ctx: Context) {
		loop {
			sleep(Duration::from_secs(3)).await;
		}
	}
}

pub struct MockDisputeDistribution {}
#[overseer::subsystem(DisputeDistribution, error=SubsystemError, prefix=self::overseer)]
impl<Context> MockDisputeDistribution {
	fn start(self, ctx: Context) -> SpawnedSubsystem {
		let future = self.run(ctx).map(|_| Ok(())).boxed();

		SpawnedSubsystem { name: "candidate-validation-subsystem", future }
	}
}

#[overseer::contextbounds(DisputeDistribution, prefix = self::overseer)]
impl MockDisputeDistribution {
	async fn run<Context>(self, mut ctx: Context) {
		loop {
			sleep(Duration::from_secs(3)).await;
		}
	}
}

pub struct MockProspectiveParachains {}
#[overseer::subsystem(ProspectiveParachains, error=SubsystemError, prefix=self::overseer)]
impl<Context> MockProspectiveParachains {
	fn start(self, ctx: Context) -> SpawnedSubsystem {
		let future = self.run(ctx).map(|_| Ok(())).boxed();

		SpawnedSubsystem { name: "candidate-validation-subsystem", future }
	}
}

#[overseer::contextbounds(ProspectiveParachains, prefix = self::overseer)]
impl MockProspectiveParachains {
	async fn run<Context>(self, mut ctx: Context) {
		loop {
			sleep(Duration::from_secs(3)).await;
		}
	}
}

fn key_from_message(msg: &ApprovalDistributionMessage) -> (Hash, ValidatorIndex, CandidateIndex) {
	match msg {
		ApprovalDistributionMessage::NetworkBridgeUpdate(msg) => match msg {
			NetworkBridgeEvent::PeerMessage(peer, msg) => match msg {
				Versioned::VStaging(protocol_vstaging::ApprovalDistributionMessage::Approvals(
					approvals,
				)) => {
					let approval = approvals.last().unwrap();
					(approval.block_hash, approval.validator, approval.candidate_index)
				},
				_ => panic!("Should not happen"),
			},
			_ => panic!("Should not happen"),
		},
		_ => panic!("Should not happen"),
	}
}
