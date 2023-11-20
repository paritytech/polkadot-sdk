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
	collections::HashMap,
	sync::Arc,
	time::{Duration, Instant},
};

use futures::{select, FutureExt};
use itertools::Itertools;
use polkadot_approval_distribution::ApprovalDistribution;
use polkadot_node_core_approval_voting::{
	criteria::{compute_relay_vrf_modulo_assignments_v1, Config},
	ApprovalVotingSubsystem, Metrics,
};
use polkadot_node_primitives::approval::{
	self,
	v1::{IndirectAssignmentCert, IndirectSignedApprovalVote},
	v2::IndirectAssignmentCertV2,
};

use polkadot_node_network_protocol::{
	grid_topology::{SessionGridTopology, TopologyPeerInfo},
	peer_set::{ProtocolVersion, ValidationVersion},
	vstaging as protocol_vstaging, ObservedRole, Versioned, VersionedValidationProtocol, View,
};

use polkadot_node_subsystem::{FromOrchestra, SpawnGlue, Subsystem};
use polkadot_node_subsystem_test_helpers::{
	make_buffered_subsystem_context,
	mock::new_leaf,
	mock_orchestra::{AllMessages, MockOverseerTest},
	TestSubsystemContext, TestSubsystemContextHandle,
};

use polkadot_node_subsystem_types::{
	messages::{
		network_bridge_event::NewGossipTopology, ApprovalDistributionMessage,
		ApprovalVotingMessage, ChainApiMessage, NetworkBridgeEvent, NetworkBridgeTxMessage,
		RuntimeApiMessage, RuntimeApiRequest,
	},
	ActiveLeavesUpdate, OverseerSignal,
};
use polkadot_primitives::{
	ApprovalVote, CandidateEvent, CandidateReceipt, CoreIndex, ExecutorParams, GroupIndex, Hash,
	Header, Id as ParaId, IndexedVec, SessionIndex, SessionInfo, Slot, ValidatorIndex,
	ValidatorPair,
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
use sp_core::{crypto::VrfSecret, Pair};
use sp_runtime::{Digest, DigestItem};

pub mod test_constants {
	use polkadot_node_core_approval_voting::Config;
	const DATA_COL: u32 = 0;

	pub(crate) const NUM_COLUMNS: u32 = 1;

	pub(crate) const SLOT_DURATION_MILLIS: u64 = 6000;
	pub(crate) const TEST_CONFIG: Config =
		Config { col_approval_data: DATA_COL, slot_duration_millis: SLOT_DURATION_MILLIS };
}

pub struct ApprovalSubsystemInstance {
	approval_voting_overseer: TestSubsystemContextHandle<ApprovalVotingMessage>,
	approval_distribution_overseer: TestSubsystemContextHandle<ApprovalDistributionMessage>,
	slot: Slot,
	distribution_messages: Vec<ApprovalDistributionMessage>,
	identities: Vec<(Keyring, PeerId)>,
	count: u64,
	begin_of_sending: Option<Instant>,
	finish_of_sending: Option<Instant>,
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
		let approval_voting2 = ApprovalVotingSubsystem::with_config(
			test_constants::TEST_CONFIG,
			Arc::new(db),
			Arc::new(keystore),
			Box::new(TestSyncOracle {}),
			Metrics::default(),
		);

		let spawner_glue = SpawnGlue(spawn_task_handle.clone());

		// let mock_overseer = MockOverseerTest::builder()
		// 	.approval_voting(approval_voting2)
		// 	.spawner(spawner_glue);

		let approval_voting = approval_voting.start(approval_voting_context);

		let (approval_distribution_context, approval_distribution_overseer) =
			make_buffered_subsystem_context::<ApprovalDistributionMessage, SpawnTaskHandle>(
				spawn_task_handle.clone(),
				20000,
				"approval-distribution-subsystem",
			);

		let approval_distribution = ApprovalDistribution::new(Default::default());

		let approval_distribution = approval_distribution.start(approval_distribution_context);
		println!("========= CREATED THE TWO SUBSYSTEMS");

		spawn_task_handle.spawn_blocking("approval-voting", "approvals", async move {
			println!("========= Await approval voting");

			approval_voting.future.await;
		});

		spawn_task_handle.spawn_blocking("approval-voting", "approvals", async move {
			println!("========= Await approval distribution");

			approval_distribution.future.await;
		});
		let slot = Slot::from_timestamp(
			Timestamp::current(),
			SlotDuration::from_millis(test_constants::SLOT_DURATION_MILLIS),
		);
		let identities = generate_ids();
		let block_hash = Hash::repeat_byte(1);

		let mut distribution_messages = generate_peer_connected(identities.clone());
		distribution_messages
			.extend(generate_peer_view_change(block_hash, identities.clone()).into_iter());
		distribution_messages.extend(generate_new_session_topology(identities.clone()));
		let mut assignments = generate_many_assignments(slot, identities.clone());
		let approvals = issue_approvals(&assignments, block_hash, identities.clone());
		assignments.extend(approvals.into_iter());

		distribution_messages.extend(assignments);

		// distribution_messages.extend(generate_many_assignments(slot, identities.clone()));

		ApprovalSubsystemInstance {
			approval_voting_overseer,
			approval_distribution_overseer,
			slot,
			distribution_messages,
			identities,
			count: 0,
			begin_of_sending: None,
			finish_of_sending: None,
		}
	}

	pub async fn run_approval_voting(mut self) {
		let block_hash = Hash::repeat_byte(1);
		let send = FromOrchestra::Signal(OverseerSignal::ActiveLeaves(
			ActiveLeavesUpdate::start_work(new_leaf(block_hash, 1)),
		));
		println!("approval_voting: Sending a message");
		let sent = self.approval_voting_overseer.send(send).await;
		println!("approval_voting: Receiving a message");

		loop {
			select! {
				msg = self.approval_voting_overseer.recv().fuse() => {
					println!("approval_voting:  ===========    Received {:?}", msg);

					match msg {
						AllMessages::ChainApi(ChainApiMessage::FinalizedBlockNumber(val)) => {
							val.send(Ok(0));
						},
						AllMessages::ChainApi(ChainApiMessage::BlockHeader(hash, sender)) => {
							sender.send(Ok(Some(make_header(hash, self.slot, 1))));
						},
						AllMessages::ChainApi(ChainApiMessage::FinalizedBlockHash(number, sender)) => {
							sender.send(Ok(Some(block_hash)));
						},
						AllMessages::RuntimeApi(RuntimeApiMessage::Request(
							request,
							RuntimeApiRequest::CandidateEvents(sender),
						)) => {
							let candidate_events = make_candidates(block_hash);
							sender.send(Ok(candidate_events));
						},
						AllMessages::RuntimeApi(RuntimeApiMessage::Request(
							request,
							RuntimeApiRequest::SessionIndexForChild(sender),
						)) => {
							sender.send(Ok(1));
						},
						AllMessages::RuntimeApi(RuntimeApiMessage::Request(
							request,
							RuntimeApiRequest::SessionInfo(session_index, sender),
						)) => {
							sender.send(Ok(Some(dummy_session_info(self.identities.clone()))));
						},
						AllMessages::RuntimeApi(RuntimeApiMessage::Request(
							request,
							RuntimeApiRequest::SessionExecutorParams(session_index, sender),
						)) => {
							sender.send(Ok(Some(ExecutorParams::default())));
						},
						AllMessages::RuntimeApi(RuntimeApiMessage::Request(
							request,
							RuntimeApiRequest::CurrentBabeEpoch(sender),
						)) => {
							sender.send(Ok(generate_babe_epoch(self.slot, self.identities.clone())));
						},
						AllMessages::ApprovalDistribution(approval) => {
							self.approval_distribution_overseer
								.send(FromOrchestra::Communication { msg: approval })
								.await;
							println!("approval_voting:  ======================    Sending assignments ================== {:}", self.distribution_messages.len());
							if !self.distribution_messages.is_empty() {
								self.begin_of_sending = Some(Instant::now());
								self.finish_of_sending = Some(Instant::now());
							}
							for msg in self.distribution_messages.drain(..) {
								self.approval_distribution_overseer.send(FromOrchestra::Communication { msg  }).await;
							}

							println!("approval_voting:  ======================    Sent approvals===================");

						},
						_ => {
							println!("approval_voting:  ======================    Unhandled  {:?} ===================", msg);
						},
					}
				},
				msg = self.approval_distribution_overseer.recv().fuse() => {
					match msg {
						AllMessages::ApprovalVoting(msg) => {
							self.approval_voting_overseer
							.send(FromOrchestra::Communication { msg })
							.await;
						},
						AllMessages::NetworkBridgeTx(NetworkBridgeTxMessage::SendValidationMessage(msg, value)) => {
							println!("approval_distribution:  ======================    TX Bridge {:?} {:?}", msg, value);
							self.count += 1;
							if self.count >= 5800 {
								let spent_from_sending = self.begin_of_sending.map(|instant| instant.elapsed().as_millis()).unwrap_or_default();
								let spent_from_finish = self.finish_of_sending.map(|instant| instant.elapsed().as_millis()).unwrap_or_default();
								println!("approval_distribution:  ======================    Spent {:} finished {:} ms ", spent_from_sending, spent_from_finish);
							}
						},
						_ => {
							println!("approval_distribution:  ======================    Unhandled {:?}", msg);
						},
					}
				},
			}
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
const NUM_CORES: u32 = 100;
const NUM_VALIDATORS: u32 = 500;

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
		zeroth_delay_tranche_width: 5,
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
) -> Vec<ApprovalDistributionMessage> {
	let candidates = make_candidates(block_hash);
	assignments
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
					let candidate =
						candidates.get(assignment.1.iter_ones().next().unwrap()).unwrap();
					if let CandidateEvent::CandidateIncluded(candidate, _, _, _) = candidate {
						let keyring =
							keyrings.get(assignment.0.validator.0 as usize).unwrap().clone();
						let payload = ApprovalVote(candidate.hash()).signing_payload(1);
						let validator_key: ValidatorPair = keyring.0.pair().into();
						let signature = validator_key.sign(&payload[..]);
						let indirect = IndirectSignedApprovalVote {
							block_hash,
							candidate_index: assignment.1.iter_ones().next().unwrap() as u32,
							validator: assignment.0.validator,
							signature,
						};
						let msg = protocol_vstaging::ApprovalDistributionMessage::Approvals(vec![
							indirect,
						]);
						ApprovalDistributionMessage::NetworkBridgeUpdate(
							NetworkBridgeEvent::PeerMessage(keyring.1, Versioned::VStaging(msg)),
						)
					} else {
						panic!("Should not happend");
					}
				} else {
					panic!("Should not happen");
				},
			_ => {
				panic!("Should happen");
			},
		})
		.collect_vec()
}

fn make_candidate(para_id: ParaId, hash: &Hash) -> CandidateReceipt {
	let mut r = dummy_candidate_receipt_bad_sig(*hash, Some(Default::default()));
	r.descriptor.para_id = para_id;
	r
}

fn make_candidates(block_hash: Hash) -> Vec<CandidateEvent> {
	(0..NUM_CORES)
		.map(|core| {
			CandidateEvent::CandidateIncluded(
				make_candidate(ParaId::from(core), &block_hash),
				Vec::new().into(),
				CoreIndex(core),
				GroupIndex(core),
			)
		})
		.collect_vec()
}

fn generate_many_assignments(
	current_slot: Slot,
	keyrings: Vec<(Keyring, PeerId)>,
) -> Vec<ApprovalDistributionMessage> {
	let block_hash = Hash::repeat_byte(1);
	let session_info = dummy_session_info2(&keyrings);

	let config = Config::from(&session_info);
	let leaving_cores = make_candidates(block_hash)
		.into_iter()
		.map(|candidate_event| {
			if let CandidateEvent::CandidateIncluded(candidate, _, core_index, _) = candidate_event
			{
				(candidate.hash(), core_index)
			} else {
				panic!("SHOULD NOT HAPPEN")
			}
		})
		.collect_vec();
	let mut indirect = Vec::new();

	let unsafe_vrf = approval::v1::babe_unsafe_vrf_info(&make_header(block_hash, current_slot, 1))
		.expect("Should be ok");
	let babe_epoch = generate_babe_epoch(current_slot, keyrings.clone());
	let relay_vrf_story = unsafe_vrf
		.compute_randomness(&babe_epoch.authorities, &babe_epoch.randomness, babe_epoch.epoch_index)
		.expect("Should generate vrf_story");
	for i in 0..keyrings.len() as u32 {
		let mut assignments = HashMap::new();
		let leaving_cores = leaving_cores
			.clone()
			.into_iter()
			.filter(|(_, core_index)| core_index.0 != i)
			.collect_vec();
		compute_relay_vrf_modulo_assignments_v1(
			&keyrings[i as usize].0.clone().pair().into(),
			ValidatorIndex(i),
			&config,
			relay_vrf_story.clone(),
			leaving_cores.clone(),
			&mut assignments,
		);

		for (core_index, assignment) in assignments {
			println!(
				"=============== ASSIGNMENTS GENERATED {:?}, cores {:} {:?}",
				&keyrings[i as usize].0.clone().pair().public(),
				config.n_cores,
				relay_vrf_story,
			);

			indirect.push((
				IndirectAssignmentCertV2 {
					block_hash: Hash::repeat_byte(1),
					validator: ValidatorIndex(i),
					cert: assignment.cert().clone(),
				},
				core_index.0.into(),
			));
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
				ProtocolVersion::from(ValidationVersion::V2),
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
