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

use crate::validator_side_experimental::common::{
	CONNECTED_PEERS_PARA_LIMIT, MAX_STARTUP_ANCESTRY_LOOKBACK,
};

use super::*;
use assert_matches::assert_matches;
use codec::Encode;
use futures::channel::mpsc::{self, UnboundedReceiver};
use polkadot_node_network_protocol::{
	peer_set::{CollationVersion, PeerSet},
	request_response::{Requests, ResponseSender},
};
use polkadot_node_primitives::{BlockData, PoV};
use polkadot_node_subsystem::{
	messages::{
		AllMessages, ChainApiMessage, NetworkBridgeTxMessage, ProspectiveParachainsMessage,
		RuntimeApiMessage, RuntimeApiRequest,
	},
	CollatorProtocolSenderTrait,
};
use polkadot_node_subsystem_test_helpers::{mock::new_leaf, sender_receiver, TestSubsystemSender};
use polkadot_node_subsystem_util::TimeoutExt;
use polkadot_primitives::{
	node_features::FeatureIndex,
	vstaging::{
		ApprovedPeerId, CandidateReceiptV2 as CandidateReceipt,
		CommittedCandidateReceiptV2 as CommittedCandidateReceipt, MutateDescriptorV2, UMPSignal,
		UMP_SEPARATOR,
	},
	BlockNumber, CollatorPair, CoreIndex, GroupRotationInfo, Hash, HeadData, Header, Id as ParaId,
	NodeFeatures, PersistedValidationData, SessionIndex, ValidatorId, ValidatorIndex,
};
use polkadot_primitives_test_helpers::{dummy_committed_candidate_receipt_v2, dummy_hash};
use sc_network_types::multihash::Multihash;
use sp_keyring::Sr25519Keyring;
use sp_keystore::Keystore;
use std::{
	collections::{BTreeMap, HashMap},
	sync::Arc,
	time::Duration,
};

const TIMEOUT: Duration = Duration::from_millis(100);

fn peer_id(i: u8) -> PeerId {
	let data = [i; 32];

	PeerId::from_multihash(Multihash::wrap(0x0, &data).unwrap()).unwrap()
}

#[derive(Clone)]
struct RelayParentInfo {
	hash: Hash,
	number: BlockNumber,
	parent: Hash,
	session_index: SessionIndex,
	claim_queue: BTreeMap<CoreIndex, Vec<ParaId>>,
	assigned_core: CoreIndex,
}

#[derive(Clone)]
struct SessionInfo {
	validators: Vec<ValidatorId>,
	validator_groups: Vec<Vec<ValidatorIndex>>,
	group_rotation_info: GroupRotationInfo,
	v2_receipts: bool,
	scheduling_lookahead: u32,
}

struct ViewUpdate {
	active: Vec<Hash>,
}

struct TestState {
	sender: TestSubsystemSender,
	recv: UnboundedReceiver<AllMessages>,
	rp_info: HashMap<Hash, RelayParentInfo>,
	session_info: HashMap<SessionIndex, SessionInfo>,
	buffered_msg: Option<AllMessages>,
	finalized_block: BlockNumber,
	// The key is the block at which it is included.
	candidates_pending_availability: HashMap<Hash, Vec<CommittedCandidateReceipt>>,
	candidate_nonce: u64,
}

impl Default for TestState {
	fn default() -> Self {
		let mut rp_info = HashMap::new();
		let cq: BTreeMap<CoreIndex, Vec<ParaId>> = (0..3)
			.map(|i| (CoreIndex::from(i), vec![ParaId::from(100), ParaId::from(200)]))
			.collect();
		rp_info.insert(
			get_hash(10),
			RelayParentInfo {
				hash: get_hash(10),
				number: 10,
				parent: get_parent_hash(10),
				session_index: 1,
				claim_queue: cq.clone(),
				assigned_core: CoreIndex(0),
			},
		);
		rp_info.insert(
			get_hash(9),
			RelayParentInfo {
				hash: get_hash(9),
				number: 9,
				parent: get_parent_hash(9),
				session_index: 1,
				claim_queue: cq.clone(),
				assigned_core: CoreIndex(0),
			},
		);
		rp_info.insert(
			get_hash(8),
			RelayParentInfo {
				hash: get_hash(8),
				number: 8,
				parent: get_parent_hash(8),
				session_index: 1,
				claim_queue: cq.clone(),
				assigned_core: CoreIndex(0),
			},
		);

		let mut session_info = HashMap::new();

		let validators = [
			Sr25519Keyring::Alice,
			Sr25519Keyring::Bob,
			Sr25519Keyring::Charlie,
			Sr25519Keyring::Dave,
			Sr25519Keyring::Eve,
		]
		.iter()
		.map(|k| k.public().into())
		.collect();
		let validator_groups = vec![
			vec![ValidatorIndex(0), ValidatorIndex(1)],
			vec![ValidatorIndex(2), ValidatorIndex(3)],
			vec![ValidatorIndex(4)],
		];

		let group_rotation_info =
			GroupRotationInfo { session_start_block: 0, group_rotation_frequency: 3, now: 0 };
		session_info.insert(
			1,
			SessionInfo {
				validators,
				validator_groups,
				group_rotation_info,
				v2_receipts: true,
				scheduling_lookahead: 3,
			},
		);

		let (sender, recv) = sender_receiver();

		Self {
			session_info,
			rp_info,
			buffered_msg: None,
			sender,
			recv,
			finalized_block: 0,
			candidates_pending_availability: HashMap::new(),
			candidate_nonce: 0,
		}
	}
}

impl TestState {
	fn set_candidates_pending_availability(
		&mut self,
		pending_candidates_info: HashMap<Hash, Vec<(ParaId, PeerId)>>,
	) {
		self.candidates_pending_availability = pending_candidates_info
			.into_iter()
			.map(|(key, info)| {
				(
					key,
					info.into_iter()
						.map(|(para_id, peer_id)| {
							let mut ccr = dummy_committed_candidate_receipt_v2(Hash::zero());
							ccr.descriptor.set_para_id(para_id);
							ccr.descriptor
								.set_pov_hash(Hash::from_low_u64_be(self.candidate_nonce));
							ccr.commitments.upward_messages.force_push(UMP_SEPARATOR);
							ccr.commitments.upward_messages.force_push(
								UMPSignal::ApprovedPeer(
									ApprovedPeerId::try_from(peer_id.to_bytes()).unwrap(),
								)
								.encode(),
							);
							self.candidate_nonce += 1;
							ccr
						})
						.collect(),
				)
			})
			.collect();
	}

	async fn assert_no_messages(&mut self) {
		assert!(self.buffered_msg.is_none());
		assert!(self.recv.next().timeout(TIMEOUT).await.is_none());
	}

	async fn assert_peer_disconnected(&mut self, peer: PeerId) {
		assert_matches!(
			self.timeout_recv().await,
			AllMessages::NetworkBridgeTx(
				NetworkBridgeTxMessage::DisconnectPeers(peers, PeerSet::Collation)
			) if peers == vec![peer]
		);
	}

	async fn timeout_recv(&mut self) -> AllMessages {
		self.recv.next().timeout(TIMEOUT).await.unwrap().unwrap()
	}

	async fn handle_view_update(&mut self, view_update: ViewUpdate) {
		if view_update.active.is_empty() {
			return
		}

		for active in view_update.active.iter() {
			assert!(self.rp_info.contains_key(active));
		}

		let extra_msg = loop {
			let had_buffered_msg = self.buffered_msg.is_some();
			let msg = match self.buffered_msg.take() {
				Some(msg) => msg,
				None =>
					if let Some(Some(msg)) = self.recv.next().timeout(TIMEOUT).await {
						msg
					} else {
						break None
					},
			};

			match msg {
				AllMessages::ChainApi(ChainApiMessage::BlockHeader(rp, tx)) => {
					tx.send(Ok(Some(
						self.rp_info
							.get(&rp)
							.map(|info| Header {
								parent_hash: info.parent,
								number: info.number,
								state_root: Hash::zero(),
								extrinsics_root: Hash::zero(),
								digest: Default::default(),
							})
							.unwrap(),
					)))
					.unwrap();
				},
				AllMessages::ProspectiveParachains(
					ProspectiveParachainsMessage::GetMinimumRelayParents(rp, tx),
				) => {
					assert!(view_update.active.contains(&rp));
					let rp_info = self.rp_info.get(&rp).unwrap();
					let session_info = self.session_info.get(&rp_info.session_index).unwrap();
					tx.send(
						rp_info
							.claim_queue
							.get(&rp_info.assigned_core)
							.unwrap()
							.iter()
							.map(|para| {
								(
									*para,
									rp_info
										.number
										.saturating_sub(session_info.scheduling_lookahead - 1),
								)
							})
							.collect(),
					)
					.unwrap();
				},
				AllMessages::RuntimeApi(RuntimeApiMessage::Request(
					rp,
					RuntimeApiRequest::SessionIndexForChild(tx),
				)) => {
					tx.send(Ok(self.rp_info.get(&rp).unwrap().session_index)).unwrap();
				},
				AllMessages::RuntimeApi(RuntimeApiMessage::Request(
					rp,
					RuntimeApiRequest::Validators(tx),
				)) => {
					let session_index = self.rp_info.get(&rp).unwrap().session_index;
					let session_info = self.session_info.get(&session_index).unwrap();
					tx.send(Ok(session_info.validators.clone())).unwrap();
				},
				AllMessages::RuntimeApi(RuntimeApiMessage::Request(
					rp,
					RuntimeApiRequest::ValidatorGroups(tx),
				)) => {
					let session_index = self.rp_info.get(&rp).unwrap().session_index;
					let session_info = self.session_info.get(&session_index).unwrap();
					tx.send(Ok((
						session_info.validator_groups.clone(),
						session_info.group_rotation_info.clone(),
					)))
					.unwrap();
				},
				AllMessages::RuntimeApi(RuntimeApiMessage::Request(
					rp,
					RuntimeApiRequest::NodeFeatures(s_index, tx),
				)) => {
					let session_index = self.rp_info.get(&rp).unwrap().session_index;
					assert_eq!(session_index, s_index);
					let session_info = self.session_info.get(&session_index).unwrap();
					let mut node_features = NodeFeatures::EMPTY;
					node_features.resize(FeatureIndex::FirstUnassigned as usize, false);
					node_features
						.set(FeatureIndex::EnableAssignmentsV2 as usize, session_info.v2_receipts);
					tx.send(Ok(node_features)).unwrap();
				},
				AllMessages::RuntimeApi(RuntimeApiMessage::Request(
					rp,
					RuntimeApiRequest::ClaimQueue(tx),
				)) => {
					let rp_info = self.rp_info.get(&rp).unwrap();

					tx.send(Ok(rp_info
						.claim_queue
						.clone()
						.into_iter()
						.map(|(i, cq)| (i, cq.into_iter().collect()))
						.collect()))
						.unwrap();
				},
				other =>
					if had_buffered_msg {
						panic!("Unexpected message: {:?}", other);
					} else {
						break Some(other)
					},
			};
		};

		self.buffered_msg = extra_msg;
	}

	async fn handle_finalized_block(&mut self, finalized: BlockNumber) {
		let old_finalized = self.finalized_block;
		self.finalized_block = finalized;

		let diff = std::cmp::min(
			finalized.checked_sub(old_finalized).unwrap(),
			MAX_STARTUP_ANCESTRY_LOOKBACK,
		);
		if diff == 0 {
			return
		}

		let msg = match self.buffered_msg.take() {
			Some(msg) => msg,
			None => self.timeout_recv().await,
		};

		let ancestors =
			((finalized - diff)..finalized).map(|n| get_hash(n)).rev().collect::<Vec<_>>();

		assert_matches!(
			msg,
			AllMessages::ChainApi(
				ChainApiMessage::Ancestors {
					hash,
					k,
					response_channel
				}
			) => {
				assert_eq!(hash, get_hash(finalized));
				assert_eq!(k as u32, diff);
				assert_eq!(ancestors.len() as u32, diff);
				response_channel.send(Ok(ancestors.clone())).unwrap();
			}
		);

		let extra_msg = loop {
			let had_buffered_msg = self.buffered_msg.is_some();
			let msg = match self.buffered_msg.take() {
				Some(msg) => msg,
				None =>
					if let Some(Some(msg)) = self.recv.next().timeout(TIMEOUT).await {
						msg
					} else {
						break None
					},
			};

			match msg {
				AllMessages::RuntimeApi(RuntimeApiMessage::Request(
					rp,
					RuntimeApiRequest::CandidateEvents(tx),
				)) => {
					assert!(ancestors.contains(&rp) || rp == get_hash(finalized));
					let events = self
						.candidates_pending_availability
						.get(&rp)
						.cloned()
						.unwrap_or_default()
						.iter()
						.map(|ccr| {
							polkadot_primitives::vstaging::CandidateEvent::CandidateIncluded(
								ccr.to_plain(),
								Default::default(),
								Default::default(),
								Default::default(),
							)
						})
						.collect();
					tx.send(Ok(events)).unwrap()
				},
				AllMessages::RuntimeApi(RuntimeApiMessage::Request(
					rp,
					RuntimeApiRequest::CandidatesPendingAvailability(para_id, tx),
				)) => {
					assert!(ancestors.contains(&rp));
					let included_at = (rp.to_low_u64_be() as u32) + 1;
					let candidates = self
						.candidates_pending_availability
						.get(&get_hash(included_at))
						.cloned()
						.unwrap_or_default()
						.into_iter()
						.filter(|ccr| ccr.descriptor.para_id() == para_id)
						.collect();
					tx.send(Ok(candidates)).unwrap();
				},
				other =>
					if had_buffered_msg {
						panic!("Unexpected message: {:?}", other);
					} else {
						break Some(other)
					},
			};
		};

		self.buffered_msg = extra_msg;
	}
}

fn get_parent_hash(number: u32) -> Hash {
	get_hash(number - 1)
}

fn get_hash(number: u32) -> Hash {
	Hash::from_low_u64_be(number as u64)
}

async fn make_state(test_state: &mut TestState, first_view_update: ViewUpdate) -> State<Db> {
	assert_eq!(first_view_update.active.len(), 1);

	let initial_leaf_hash = *(first_view_update.active.first().unwrap());
	let initial_leaf_number = test_state.rp_info.get(&initial_leaf_hash).unwrap().number;

	let keystore = Arc::new(sc_keystore::LocalKeystore::in_memory());
	Keystore::sr25519_generate_new(
		&*keystore,
		polkadot_primitives::PARACHAIN_KEY_TYPE_ID,
		Some(&Sr25519Keyring::Alice.to_seed()),
	)
	.expect("Insert key into keystore");

	let mut sender = test_state.sender.clone();

	let responder = async move {
		test_state.handle_view_update(first_view_update).await;

		let msg = match test_state.buffered_msg.take() {
			Some(msg) => msg,
			None => test_state.timeout_recv().await,
		};

		let finalized_block_number = test_state.finalized_block;
		let finalized_block_hash = get_hash(finalized_block_number);

		assert_matches!(
			msg,
			AllMessages::ChainApi(ChainApiMessage::FinalizedBlockNumber(tx)) => {
				tx.send(Ok(finalized_block_number)).unwrap();
			}
		);

		assert_matches!(
			test_state.timeout_recv().await,
			AllMessages::ChainApi(ChainApiMessage::FinalizedBlockHash(number, tx)) => {
				assert_eq!(number, finalized_block_number);
				tx.send(Ok(Some(finalized_block_hash))).unwrap();
			}
		);

		if finalized_block_number > 0 {
			test_state.handle_finalized_block(finalized_block_number).await;
		}

		// No more messages are expected
		test_state.assert_no_messages().await;
	};

	let initializer = async move {
		let collation_manager = CollationManager::new(
			&mut sender,
			keystore,
			new_leaf(initial_leaf_hash, initial_leaf_number),
		)
		.await
		.unwrap();

		let db = Db::new(MAX_STORED_SCORES_PER_PARA).await;
		let peer_manager = PeerManager::startup(db, &mut sender, collation_manager.assignments())
			.await
			.unwrap();

		State::new(peer_manager, collation_manager, Metrics)
	};

	let (state, ..) = futures::join!(initializer, responder);

	state
}

// Scenarios:
// Test new peer connection: More extensively tested in the peer_manager tests.
// - Test a peer that is already connected. No-op.
// - Test a peer that replaces another peer.
// - Test a peer that is rejected based on its low rep.
// - Test a regular peer that is accepted.

// Peer disconnection:
// - Test a peer that disconnects. Verify that we don't try to make requests to it and that the
//   claims were freed. Check that a reconnect is possible.

// Peer declaration:
// - reject a peer that switches the paraid.
// - reject a peer that declares for a paraid that is not scheduled for the current group.

// Finalized block notification:
// - Test that reputation are bumped/decayed correctly.
// - Later: Test a change in the registered paras.

// Advertisements:
// - Test an advertisement from a peer that is not declared.
// - Test an advertisement that is rejected for some reason (like relay parent out of view or
//   blocked by backing).
// - Test an advertisement that is accepted and results in a request for a collation.

// View updates:

// Launching new collations:

// Collation fetch response:
// - Valid
// - Invalid

// Collation seconded response:
// - Valid
// - Invalid

// Unblocking collations

#[tokio::test]
// Test various scenarios concerning connects/disconnects and declares.
async fn test_connection_flow() {
	// Leaf: 10.
	// Parent: 9.

	let mut test_state = TestState::default();
	let active_leaf = get_hash(10);
	let first_view_update = ViewUpdate { active: vec![active_leaf] };
	let mut state = make_state(&mut test_state, first_view_update).await;
	let mut sender = test_state.sender.clone();

	let first_peer = PeerId::random();
	state.handle_peer_connected(&mut sender, first_peer, CollationVersion::V2).await;
	// If we don't get a disconnect message, it was accepted.
	test_state.assert_no_messages().await;

	// Reconnecting is a no-op. We should have first received a disconnect.
	state.handle_peer_connected(&mut sender, first_peer, CollationVersion::V1).await;
	test_state.assert_no_messages().await;

	// Disconnect the peer.
	state.handle_peer_disconnected(first_peer).await;

	// Fill up the connection slots. For each para (ids 100 and 200) we should have 100 slots.
	let active_leaf_info = test_state.rp_info.get(&active_leaf).unwrap();
	let schedule = active_leaf_info.claim_queue.get(&active_leaf_info.assigned_core).unwrap();
	assert_eq!(schedule.len(), 2);

	let peer_ids = (0..(CONNECTED_PEERS_PARA_LIMIT.get() as u8))
		.map(|i| peer_id(i))
		.collect::<Vec<_>>();

	for id in peer_ids.iter() {
		state.handle_peer_connected(&mut sender, *id, CollationVersion::V2).await;
	}
	test_state.assert_no_messages().await;

	// Now all 100 peers were accepted on both paras (since they're not declared).
	// A new connection from a peer with the same score will be rejected.
	let new_peer = PeerId::random();
	state.handle_peer_connected(&mut sender, new_peer, CollationVersion::V2).await;

	test_state.assert_peer_disconnected(new_peer).await;

	let mut sender = test_state.sender.clone();
	// This bumps the
	let mut pending = HashMap::new();
	pending.insert(get_hash(1), vec![(ParaId::from(100), new_peer)]);
	test_state.set_candidates_pending_availability(pending);
	futures::join!(test_state.handle_finalized_block(2), async {
		state.handle_finalized_block(&mut sender, get_hash(2), 2).await.unwrap()
	});

	test_state.assert_no_messages().await;

	state.handle_peer_connected(&mut sender, new_peer, CollationVersion::V2).await;
	// The new peer took the spot of some other one, but that other one remains connected for the
	// other para (200).
	test_state.assert_no_messages().await;
}
