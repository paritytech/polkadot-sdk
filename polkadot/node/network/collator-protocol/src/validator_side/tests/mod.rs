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

use super::*;
use assert_matches::assert_matches;
use futures::{executor, future, Future};
use sc_network::ProtocolName;
use sp_core::{crypto::Pair, Encode};
use sp_keyring::Sr25519Keyring;
use sp_keystore::Keystore;
use std::{
	collections::{BTreeMap, VecDeque},
	iter,
	sync::Arc,
	time::Duration,
};

use self::prospective_parachains::update_view;
use polkadot_node_network_protocol::{
	peer_set::CollationVersion,
	request_response::{Requests, ResponseSender},
	ObservedRole,
};
use polkadot_node_primitives::{BlockData, PoV};
use polkadot_node_subsystem::messages::{
	AllMessages, ReportPeerMessage, RuntimeApiMessage, RuntimeApiRequest,
};
use polkadot_node_subsystem_test_helpers as test_helpers;
use polkadot_node_subsystem_util::{reputation::add_reputation, TimeoutExt};
use polkadot_primitives::{
	node_features, vstaging::CandidateReceiptV2 as CandidateReceipt, AsyncBackingParams,
	CollatorPair, CoreIndex, GroupRotationInfo, HeadData, NodeFeatures, PersistedValidationData,
	ValidatorId, ValidatorIndex,
};
use polkadot_primitives_test_helpers::{dummy_candidate_receipt_bad_sig, dummy_hash};

mod prospective_parachains;

const ACTIVITY_TIMEOUT: Duration = Duration::from_millis(500);
const DECLARE_TIMEOUT: Duration = Duration::from_millis(25);
const REPUTATION_CHANGE_TEST_INTERVAL: Duration = Duration::from_millis(10);

fn dummy_pvd() -> PersistedValidationData {
	PersistedValidationData {
		parent_head: HeadData(vec![7, 8, 9]),
		relay_parent_number: 5,
		max_pov_size: 1024,
		relay_parent_storage_root: Default::default(),
	}
}

#[derive(Clone)]
struct TestState {
	chain_ids: Vec<ParaId>,
	relay_parent: Hash,
	collators: Vec<CollatorPair>,
	validator_public: Vec<ValidatorId>,
	validator_groups: Vec<Vec<ValidatorIndex>>,
	group_rotation_info: GroupRotationInfo,
	claim_queue: BTreeMap<CoreIndex, VecDeque<ParaId>>,
	async_backing_params: AsyncBackingParams,
	node_features: NodeFeatures,
	session_index: SessionIndex,
	// Used by `update_view` to keep track of latest requested ancestor
	last_known_block: Option<u32>,
}

impl Default for TestState {
	fn default() -> Self {
		let relay_parent = Hash::from_low_u64_be(0x05);
		let collators = iter::repeat(()).map(|_| CollatorPair::generate().0).take(5).collect();

		let validators = vec![
			Sr25519Keyring::Alice,
			Sr25519Keyring::Bob,
			Sr25519Keyring::Charlie,
			Sr25519Keyring::Dave,
			Sr25519Keyring::Eve,
		];

		let validator_public = validators.iter().map(|k| k.public().into()).collect();
		let validator_groups = vec![
			vec![ValidatorIndex(0), ValidatorIndex(1)],
			vec![ValidatorIndex(2), ValidatorIndex(3)],
			vec![ValidatorIndex(4)],
		];

		let group_rotation_info =
			GroupRotationInfo { session_start_block: 0, group_rotation_frequency: 1, now: 0 };

		let mut claim_queue = BTreeMap::new();
		claim_queue.insert(
			CoreIndex(0),
			iter::repeat(ParaId::from(Self::CHAIN_IDS[0]))
				.take(Self::ASYNC_BACKING_PARAMS.allowed_ancestry_len as usize)
				.collect(),
		);
		claim_queue.insert(CoreIndex(1), VecDeque::new());
		claim_queue.insert(
			CoreIndex(2),
			iter::repeat(ParaId::from(Self::CHAIN_IDS[1]))
				.take(Self::ASYNC_BACKING_PARAMS.allowed_ancestry_len as usize)
				.collect(),
		);

		let mut node_features = NodeFeatures::EMPTY;
		node_features.resize(node_features::FeatureIndex::CandidateReceiptV2 as usize + 1, false);
		node_features.set(node_features::FeatureIndex::CandidateReceiptV2 as u8 as usize, true);

		Self {
			chain_ids: Self::CHAIN_IDS.map(|id| ParaId::from(id)).to_vec(),
			relay_parent,
			collators,
			validator_public,
			validator_groups,
			group_rotation_info,
			claim_queue,
			async_backing_params: Self::ASYNC_BACKING_PARAMS,
			node_features,
			session_index: 1,
			last_known_block: None,
		}
	}
}

impl TestState {
	const CHAIN_IDS: [u32; 2] = [1, 2];
	const ASYNC_BACKING_PARAMS: AsyncBackingParams =
		AsyncBackingParams { max_candidate_depth: 4, allowed_ancestry_len: 3 };

	fn with_shared_core() -> Self {
		let mut state = Self::default();

		let mut claim_queue = BTreeMap::new();
		claim_queue.insert(
			CoreIndex(0),
			VecDeque::from_iter(
				[
					ParaId::from(Self::CHAIN_IDS[1]),
					ParaId::from(Self::CHAIN_IDS[0]),
					ParaId::from(Self::CHAIN_IDS[0]),
				]
				.into_iter(),
			),
		);
		state.validator_groups.truncate(1);

		assert!(
			claim_queue.get(&CoreIndex(0)).unwrap().len() ==
				Self::ASYNC_BACKING_PARAMS.allowed_ancestry_len as usize
		);

		state.claim_queue = claim_queue;

		state
	}

	fn with_one_scheduled_para() -> Self {
		let mut state = Self::default();

		let validator_groups = vec![vec![ValidatorIndex(0), ValidatorIndex(1)]];

		let mut claim_queue = BTreeMap::new();
		claim_queue.insert(
			CoreIndex(0),
			VecDeque::from_iter(
				[
					ParaId::from(Self::CHAIN_IDS[0]),
					ParaId::from(Self::CHAIN_IDS[0]),
					ParaId::from(Self::CHAIN_IDS[0]),
				]
				.into_iter(),
			),
		);

		assert!(
			claim_queue.get(&CoreIndex(0)).unwrap().len() ==
				Self::ASYNC_BACKING_PARAMS.allowed_ancestry_len as usize
		);

		state.validator_groups = validator_groups;
		state.claim_queue = claim_queue;

		state
	}
}

type VirtualOverseer =
	polkadot_node_subsystem_test_helpers::TestSubsystemContextHandle<CollatorProtocolMessage>;

struct TestHarness {
	virtual_overseer: VirtualOverseer,
	keystore: KeystorePtr,
}

fn test_harness<T: Future<Output = VirtualOverseer>>(
	reputation: ReputationAggregator,
	test: impl FnOnce(TestHarness) -> T,
) {
	sp_tracing::init_for_tests();

	let pool = sp_core::testing::TaskExecutor::new();

	let (context, virtual_overseer) =
		polkadot_node_subsystem_test_helpers::make_subsystem_context(pool.clone());

	let keystore = Arc::new(sc_keystore::LocalKeystore::in_memory());
	Keystore::sr25519_generate_new(
		&*keystore,
		polkadot_primitives::PARACHAIN_KEY_TYPE_ID,
		Some(&Sr25519Keyring::Alice.to_seed()),
	)
	.expect("Insert key into keystore");

	let subsystem = run_inner(
		context,
		keystore.clone(),
		crate::CollatorEvictionPolicy {
			inactive_collator: ACTIVITY_TIMEOUT,
			undeclared: DECLARE_TIMEOUT,
		},
		Metrics::default(),
		reputation,
		REPUTATION_CHANGE_TEST_INTERVAL,
	);

	let test_fut = test(TestHarness { virtual_overseer, keystore });

	futures::pin_mut!(test_fut);
	futures::pin_mut!(subsystem);

	executor::block_on(future::join(
		async move {
			let mut overseer = test_fut.await;
			overseer_signal(&mut overseer, OverseerSignal::Conclude).await;
		},
		subsystem,
	))
	.1
	.unwrap();
}

const TIMEOUT: Duration = Duration::from_millis(200);

async fn overseer_send(overseer: &mut VirtualOverseer, msg: CollatorProtocolMessage) {
	gum::trace!("Sending message:\n{:?}", &msg);
	overseer
		.send(FromOrchestra::Communication { msg })
		.timeout(TIMEOUT)
		.await
		.expect(&format!("{:?} is enough for sending messages.", TIMEOUT));
}

async fn overseer_recv(overseer: &mut VirtualOverseer) -> AllMessages {
	let msg = overseer_recv_with_timeout(overseer, TIMEOUT)
		.await
		.expect(&format!("{:?} is enough to receive messages.", TIMEOUT));

	gum::trace!("Received message:\n{:?}", &msg);

	msg
}

async fn overseer_recv_with_timeout(
	overseer: &mut VirtualOverseer,
	timeout: Duration,
) -> Option<AllMessages> {
	gum::trace!("Waiting for message...");
	overseer.recv().timeout(timeout).await
}

async fn overseer_signal(overseer: &mut VirtualOverseer, signal: OverseerSignal) {
	overseer
		.send(FromOrchestra::Signal(signal))
		.timeout(TIMEOUT)
		.await
		.expect(&format!("{:?} is more than enough for sending signals.", TIMEOUT));
}

/// Assert that the next message is a `CandidateBacking(Second())`.
async fn assert_candidate_backing_second(
	virtual_overseer: &mut VirtualOverseer,
	expected_relay_parent: Hash,
	expected_para_id: ParaId,
	expected_pov: &PoV,
	version: CollationVersion,
) -> CandidateReceipt {
	let pvd = dummy_pvd();

	// Depending on relay parent mode pvd will be either requested
	// from the Runtime API or Prospective Parachains.
	let msg = overseer_recv(virtual_overseer).await;
	match version {
		CollationVersion::V1 => assert_matches!(
			msg,
			AllMessages::RuntimeApi(RuntimeApiMessage::Request(
				hash,
				RuntimeApiRequest::PersistedValidationData(para_id, assumption, tx),
			)) => {
				assert_eq!(expected_relay_parent, hash);
				assert_eq!(expected_para_id, para_id);
				assert_eq!(OccupiedCoreAssumption::Free, assumption);
				tx.send(Ok(Some(pvd.clone()))).unwrap();
			}
		),
		CollationVersion::V2 => assert_matches!(
			msg,
			AllMessages::ProspectiveParachains(
				ProspectiveParachainsMessage::GetProspectiveValidationData(request, tx),
			) => {
				assert_eq!(expected_relay_parent, request.candidate_relay_parent);
				assert_eq!(expected_para_id, request.para_id);
				tx.send(Some(pvd.clone())).unwrap();
			}
		),
	}

	assert_matches!(
		overseer_recv(virtual_overseer).await,
		AllMessages::CandidateBacking(CandidateBackingMessage::Second(
			relay_parent,
			candidate_receipt,
			received_pvd,
			incoming_pov,
		)) => {
			assert_eq!(expected_relay_parent, relay_parent);
			assert_eq!(expected_para_id, candidate_receipt.descriptor.para_id());
			assert_eq!(*expected_pov, incoming_pov);
			assert_eq!(pvd, received_pvd);
			candidate_receipt
		}
	)
}

/// Assert that a collator got disconnected.
async fn assert_collator_disconnect(virtual_overseer: &mut VirtualOverseer, expected_peer: PeerId) {
	assert_matches!(
		overseer_recv(virtual_overseer).await,
		AllMessages::NetworkBridgeTx(NetworkBridgeTxMessage::DisconnectPeer(
			peer,
			peer_set,
		)) => {
			assert_eq!(expected_peer, peer);
			assert_eq!(PeerSet::Collation, peer_set);
		}
	);
}

/// Assert that a fetch collation request was send.
async fn assert_fetch_collation_request(
	virtual_overseer: &mut VirtualOverseer,
	relay_parent: Hash,
	para_id: ParaId,
	candidate_hash: Option<CandidateHash>,
) -> ResponseSender {
	assert_matches!(
		overseer_recv(virtual_overseer).await,
		AllMessages::NetworkBridgeTx(NetworkBridgeTxMessage::SendRequests(reqs, IfDisconnected::ImmediateError)
	) => {
		let req = reqs.into_iter().next()
			.expect("There should be exactly one request");
		match candidate_hash {
			None => assert_matches!(
				req,
				Requests::CollationFetchingV1(req) => {
					let payload = req.payload;
					assert_eq!(payload.relay_parent, relay_parent);
					assert_eq!(payload.para_id, para_id);
					req.pending_response
				}
			),
			Some(candidate_hash) => assert_matches!(
				req,
				Requests::CollationFetchingV2(req) => {
					let payload = req.payload;
					assert_eq!(payload.relay_parent, relay_parent);
					assert_eq!(payload.para_id, para_id);
					assert_eq!(payload.candidate_hash, candidate_hash);
					req.pending_response
				}
			),
		}
	})
}

/// Connect and declare a collator
async fn connect_and_declare_collator(
	virtual_overseer: &mut VirtualOverseer,
	peer: PeerId,
	collator: CollatorPair,
	para_id: ParaId,
	version: CollationVersion,
) {
	overseer_send(
		virtual_overseer,
		CollatorProtocolMessage::NetworkBridgeUpdate(NetworkBridgeEvent::PeerConnected(
			peer,
			ObservedRole::Full,
			version.into(),
			None,
		)),
	)
	.await;

	let wire_message = match version {
		CollationVersion::V1 => Versioned::V1(protocol_v1::CollatorProtocolMessage::Declare(
			collator.public(),
			para_id,
			collator.sign(&protocol_v1::declare_signature_payload(&peer)),
		)),
		CollationVersion::V2 => Versioned::V2(protocol_v2::CollatorProtocolMessage::Declare(
			collator.public(),
			para_id,
			collator.sign(&protocol_v1::declare_signature_payload(&peer)),
		)),
	};

	overseer_send(
		virtual_overseer,
		CollatorProtocolMessage::NetworkBridgeUpdate(NetworkBridgeEvent::PeerMessage(
			peer,
			wire_message,
		)),
	)
	.await;
}

/// Advertise a collation.
async fn advertise_collation(
	virtual_overseer: &mut VirtualOverseer,
	peer: PeerId,
	relay_parent: Hash,
	candidate: Option<(CandidateHash, Hash)>, // Candidate hash + parent head data hash.
) {
	let wire_message = match candidate {
		Some((candidate_hash, parent_head_data_hash)) =>
			Versioned::V2(protocol_v2::CollatorProtocolMessage::AdvertiseCollation {
				relay_parent,
				candidate_hash,
				parent_head_data_hash,
			}),
		None =>
			Versioned::V1(protocol_v1::CollatorProtocolMessage::AdvertiseCollation(relay_parent)),
	};
	overseer_send(
		virtual_overseer,
		CollatorProtocolMessage::NetworkBridgeUpdate(NetworkBridgeEvent::PeerMessage(
			peer,
			wire_message,
		)),
	)
	.await;
}

// Test that we verify the signatures on `Declare` and `AdvertiseCollation` messages.
#[test]
fn collator_authentication_verification_works() {
	let test_state = TestState::default();

	test_harness(ReputationAggregator::new(|_| true), |test_harness| async move {
		let TestHarness { mut virtual_overseer, .. } = test_harness;

		let peer_b = PeerId::random();

		overseer_send(
			&mut virtual_overseer,
			CollatorProtocolMessage::NetworkBridgeUpdate(NetworkBridgeEvent::PeerConnected(
				peer_b,
				ObservedRole::Full,
				CollationVersion::V1.into(),
				None,
			)),
		)
		.await;

		// the peer sends a declare message but sign the wrong payload
		overseer_send(
			&mut virtual_overseer,
			CollatorProtocolMessage::NetworkBridgeUpdate(NetworkBridgeEvent::PeerMessage(
				peer_b,
				Versioned::V1(protocol_v1::CollatorProtocolMessage::Declare(
					test_state.collators[0].public(),
					test_state.chain_ids[0],
					test_state.collators[0].sign(&[42]),
				)),
			)),
		)
		.await;

		// it should be reported for sending a message with an invalid signature
		assert_matches!(
			overseer_recv(&mut virtual_overseer).await,
			AllMessages::NetworkBridgeTx(
				NetworkBridgeTxMessage::ReportPeer(ReportPeerMessage::Single(peer, rep)),
			) => {
				assert_eq!(peer, peer_b);
				assert_eq!(rep.value, COST_INVALID_SIGNATURE.cost_or_benefit());
			}
		);
		virtual_overseer
	});
}

/// Tests that on a V1 Advertisement a validator fetches only one collation at any moment of time
/// per relay parent and ignores other V1 advertisements once a candidate gets seconded.
#[test]
fn fetch_one_collation_at_a_time_for_v1_advertisement() {
	let mut test_state = TestState::default();

	test_harness(ReputationAggregator::new(|_| true), |test_harness| async move {
		let TestHarness { mut virtual_overseer, .. } = test_harness;
		let second = Hash::from_low_u64_be(test_state.relay_parent.to_low_u64_be() - 1);
		let relay_parent = test_state.relay_parent;
		update_view(&mut virtual_overseer, &mut test_state, vec![(relay_parent, 0), (second, 1)])
			.await;

		let peer_b = PeerId::random();
		let peer_c = PeerId::random();

		connect_and_declare_collator(
			&mut virtual_overseer,
			peer_b,
			test_state.collators[0].clone(),
			test_state.chain_ids[0],
			CollationVersion::V1,
		)
		.await;

		connect_and_declare_collator(
			&mut virtual_overseer,
			peer_c,
			test_state.collators[1].clone(),
			test_state.chain_ids[0],
			CollationVersion::V1,
		)
		.await;

		advertise_collation(&mut virtual_overseer, peer_b, relay_parent, None).await;
		advertise_collation(&mut virtual_overseer, peer_c, relay_parent, None).await;

		let response_channel = assert_fetch_collation_request(
			&mut virtual_overseer,
			test_state.relay_parent,
			test_state.chain_ids[0],
			None,
		)
		.await;

		assert!(
			overseer_recv_with_timeout(&mut &mut virtual_overseer, Duration::from_millis(30)).await.is_none(),
			"There should not be sent any other PoV request while the first one wasn't finished or timed out.",
		);

		let pov = PoV { block_data: BlockData(vec![]) };
		let mut candidate_a =
			dummy_candidate_receipt_bad_sig(dummy_hash(), Some(Default::default()));
		candidate_a.descriptor.para_id = test_state.chain_ids[0];
		candidate_a.descriptor.relay_parent = test_state.relay_parent;
		candidate_a.descriptor.persisted_validation_data_hash = dummy_pvd().hash();
		response_channel
			.send(Ok((
				request_v1::CollationFetchingResponse::Collation(
					candidate_a.clone().into(),
					pov.clone(),
				)
				.encode(),
				ProtocolName::from(""),
			)))
			.expect("Sending response should succeed");

		assert_candidate_backing_second(
			&mut virtual_overseer,
			test_state.relay_parent,
			test_state.chain_ids[0],
			&pov,
			CollationVersion::V1,
		)
		.await;

		// Ensure the subsystem is polled.
		test_helpers::Yield::new().await;

		// Second collation is not requested since there's already seconded one.
		assert_matches!(virtual_overseer.recv().now_or_never(), None);

		virtual_overseer
	})
}

/// Tests that a validator starts fetching next queued collations on [`MAX_UNSHARED_DOWNLOAD_TIME`]
/// timeout and in case of an error.
#[test]
fn fetches_next_collation() {
	let mut test_state = TestState::with_one_scheduled_para();

	test_harness(ReputationAggregator::new(|_| true), |test_harness| async move {
		let TestHarness { mut virtual_overseer, .. } = test_harness;

		let first = test_state.relay_parent;
		let second = Hash::random();
		update_view(&mut virtual_overseer, &mut test_state, vec![(first, 0), (second, 1)]).await;

		let peer_b = PeerId::random();
		let peer_c = PeerId::random();
		let peer_d = PeerId::random();

		connect_and_declare_collator(
			&mut virtual_overseer,
			peer_b,
			test_state.collators[2].clone(),
			test_state.chain_ids[0],
			CollationVersion::V1,
		)
		.await;

		connect_and_declare_collator(
			&mut virtual_overseer,
			peer_c,
			test_state.collators[3].clone(),
			test_state.chain_ids[0],
			CollationVersion::V1,
		)
		.await;

		connect_and_declare_collator(
			&mut virtual_overseer,
			peer_d,
			test_state.collators[4].clone(),
			test_state.chain_ids[0],
			CollationVersion::V1,
		)
		.await;

		advertise_collation(&mut virtual_overseer, peer_b, second, None).await;
		advertise_collation(&mut virtual_overseer, peer_c, second, None).await;
		advertise_collation(&mut virtual_overseer, peer_d, second, None).await;

		// Dropping the response channel should lead to fetching the second collation.
		assert_fetch_collation_request(
			&mut virtual_overseer,
			second,
			test_state.chain_ids[0],
			None,
		)
		.await;

		let response_channel_non_exclusive = assert_fetch_collation_request(
			&mut virtual_overseer,
			second,
			test_state.chain_ids[0],
			None,
		)
		.await;

		// Third collator should receive response after that timeout:
		Delay::new(MAX_UNSHARED_DOWNLOAD_TIME + Duration::from_millis(50)).await;

		let response_channel = assert_fetch_collation_request(
			&mut virtual_overseer,
			second,
			test_state.chain_ids[0],
			None,
		)
		.await;

		let pov = PoV { block_data: BlockData(vec![1]) };
		let mut candidate_a =
			dummy_candidate_receipt_bad_sig(dummy_hash(), Some(Default::default()));
		candidate_a.descriptor.para_id = test_state.chain_ids[0];
		candidate_a.descriptor.relay_parent = second;
		candidate_a.descriptor.persisted_validation_data_hash = dummy_pvd().hash();

		// First request finishes now:
		response_channel_non_exclusive
			.send(Ok((
				request_v1::CollationFetchingResponse::Collation(
					candidate_a.clone().into(),
					pov.clone(),
				)
				.encode(),
				ProtocolName::from(""),
			)))
			.expect("Sending response should succeed");

		response_channel
			.send(Ok((
				request_v1::CollationFetchingResponse::Collation(
					candidate_a.clone().into(),
					pov.clone(),
				)
				.encode(),
				ProtocolName::from(""),
			)))
			.expect("Sending response should succeed");

		assert_candidate_backing_second(
			&mut virtual_overseer,
			second,
			test_state.chain_ids[0],
			&pov,
			CollationVersion::V1,
		)
		.await;

		virtual_overseer
	});
}

#[test]
fn reject_connection_to_next_group() {
	let mut test_state = TestState::default();

	test_harness(ReputationAggregator::new(|_| true), |test_harness| async move {
		let TestHarness { mut virtual_overseer, .. } = test_harness;

		let relay_parent = test_state.relay_parent;
		update_view(&mut virtual_overseer, &mut test_state, vec![(relay_parent, 0)]).await;

		let peer_b = PeerId::random();

		connect_and_declare_collator(
			&mut virtual_overseer,
			peer_b,
			test_state.collators[0].clone(),
			test_state.chain_ids[1], // next, not current `para_id`
			CollationVersion::V1,
		)
		.await;

		assert_matches!(
			overseer_recv(&mut virtual_overseer).await,
			AllMessages::NetworkBridgeTx(NetworkBridgeTxMessage::ReportPeer(
				ReportPeerMessage::Single(peer, rep),
			)) => {
				assert_eq!(peer, peer_b);
				assert_eq!(rep.value, COST_UNNEEDED_COLLATOR.cost_or_benefit());
			}
		);

		assert_collator_disconnect(&mut virtual_overseer, peer_b).await;

		virtual_overseer
	})
}

// Ensure that we fetch a second collation, after the first checked collation was found to be
// invalid.
#[test]
fn fetch_next_collation_on_invalid_collation() {
	let mut test_state = TestState::with_one_scheduled_para();

	test_harness(ReputationAggregator::new(|_| true), |test_harness| async move {
		let TestHarness { mut virtual_overseer, .. } = test_harness;

		let relay_parent = test_state.relay_parent;
		update_view(&mut virtual_overseer, &mut test_state, vec![(relay_parent, 0)]).await;

		let peer_b = PeerId::random();
		let peer_c = PeerId::random();

		connect_and_declare_collator(
			&mut virtual_overseer,
			peer_b,
			test_state.collators[0].clone(),
			test_state.chain_ids[0],
			CollationVersion::V1,
		)
		.await;

		connect_and_declare_collator(
			&mut virtual_overseer,
			peer_c,
			test_state.collators[1].clone(),
			test_state.chain_ids[0],
			CollationVersion::V1,
		)
		.await;

		advertise_collation(&mut virtual_overseer, peer_b, relay_parent, None).await;
		advertise_collation(&mut virtual_overseer, peer_c, relay_parent, None).await;

		let response_channel = assert_fetch_collation_request(
			&mut virtual_overseer,
			relay_parent,
			test_state.chain_ids[0],
			None,
		)
		.await;

		let pov = PoV { block_data: BlockData(vec![]) };
		let mut candidate_a =
			dummy_candidate_receipt_bad_sig(dummy_hash(), Some(Default::default()));
		candidate_a.descriptor.para_id = test_state.chain_ids[0];
		candidate_a.descriptor.relay_parent = relay_parent;
		candidate_a.descriptor.persisted_validation_data_hash = dummy_pvd().hash();
		response_channel
			.send(Ok((
				request_v1::CollationFetchingResponse::Collation(
					candidate_a.clone().into(),
					pov.clone(),
				)
				.encode(),
				ProtocolName::from(""),
			)))
			.expect("Sending response should succeed");

		let receipt = assert_candidate_backing_second(
			&mut virtual_overseer,
			relay_parent,
			test_state.chain_ids[0],
			&pov,
			CollationVersion::V1,
		)
		.await;

		// Inform that the candidate was invalid.
		overseer_send(
			&mut virtual_overseer,
			CollatorProtocolMessage::Invalid(relay_parent, receipt),
		)
		.await;

		assert_matches!(
			overseer_recv(&mut virtual_overseer).await,
			AllMessages::NetworkBridgeTx(NetworkBridgeTxMessage::ReportPeer(
				ReportPeerMessage::Single(peer, rep),
			)) => {
				assert_eq!(peer, peer_b);
				assert_eq!(rep.value, COST_REPORT_BAD.cost_or_benefit());
			}
		);

		// We should see a request for another collation.
		assert_fetch_collation_request(
			&mut virtual_overseer,
			relay_parent,
			test_state.chain_ids[0],
			None,
		)
		.await;

		virtual_overseer
	});
}

#[test]
fn inactive_disconnected() {
	let mut test_state = TestState::default();

	test_harness(ReputationAggregator::new(|_| true), |test_harness| async move {
		let TestHarness { mut virtual_overseer, .. } = test_harness;

		let pair = CollatorPair::generate().0;

		let relay_parent = test_state.relay_parent;
		update_view(&mut virtual_overseer, &mut test_state, vec![(relay_parent, 0)]).await;

		let peer_b = PeerId::random();

		connect_and_declare_collator(
			&mut virtual_overseer,
			peer_b,
			pair.clone(),
			test_state.chain_ids[0],
			CollationVersion::V1,
		)
		.await;
		advertise_collation(&mut virtual_overseer, peer_b, relay_parent, None).await;

		assert_fetch_collation_request(
			&mut virtual_overseer,
			relay_parent,
			test_state.chain_ids[0],
			None,
		)
		.await;

		Delay::new(ACTIVITY_TIMEOUT * 3).await;

		assert_collator_disconnect(&mut virtual_overseer, peer_b).await;
		virtual_overseer
	});
}

#[test]
fn activity_extends_life() {
	let mut test_state = TestState::with_one_scheduled_para();

	test_harness(ReputationAggregator::new(|_| true), |test_harness| async move {
		let TestHarness { mut virtual_overseer, .. } = test_harness;

		let pair = CollatorPair::generate().0;

		let hash_a = Hash::from_low_u64_be(12);
		let hash_b = Hash::from_low_u64_be(11);
		let hash_c = Hash::from_low_u64_be(10);

		update_view(
			&mut virtual_overseer,
			&mut test_state,
			vec![(hash_a, 0), (hash_b, 1), (hash_c, 2)],
		)
		.await;

		let peer_b = PeerId::random();

		connect_and_declare_collator(
			&mut virtual_overseer,
			peer_b,
			pair.clone(),
			test_state.chain_ids[0],
			CollationVersion::V1,
		)
		.await;

		Delay::new(ACTIVITY_TIMEOUT * 2 / 3).await;

		advertise_collation(&mut virtual_overseer, peer_b, hash_a, None).await;

		assert_fetch_collation_request(
			&mut virtual_overseer,
			hash_a,
			test_state.chain_ids[0],
			None,
		)
		.await;

		Delay::new(ACTIVITY_TIMEOUT * 2 / 3).await;

		advertise_collation(&mut virtual_overseer, peer_b, hash_b, None).await;

		assert_fetch_collation_request(
			&mut virtual_overseer,
			hash_b,
			test_state.chain_ids[0],
			None,
		)
		.await;

		Delay::new(ACTIVITY_TIMEOUT * 2 / 3).await;

		advertise_collation(&mut virtual_overseer, peer_b, hash_c, None).await;

		assert_fetch_collation_request(
			&mut virtual_overseer,
			hash_c,
			test_state.chain_ids[0],
			None,
		)
		.await;

		Delay::new(ACTIVITY_TIMEOUT * 3 / 2).await;

		assert_collator_disconnect(&mut virtual_overseer, peer_b).await;

		virtual_overseer
	});
}

#[test]
fn disconnect_if_no_declare() {
	let mut test_state = TestState::default();

	test_harness(ReputationAggregator::new(|_| true), |test_harness| async move {
		let TestHarness { mut virtual_overseer, .. } = test_harness;

		let relay_parent = test_state.relay_parent;
		update_view(&mut virtual_overseer, &mut test_state, vec![(relay_parent, 0)]).await;

		let peer_b = PeerId::random();

		overseer_send(
			&mut virtual_overseer,
			CollatorProtocolMessage::NetworkBridgeUpdate(NetworkBridgeEvent::PeerConnected(
				peer_b,
				ObservedRole::Full,
				CollationVersion::V1.into(),
				None,
			)),
		)
		.await;

		assert_collator_disconnect(&mut virtual_overseer, peer_b).await;

		virtual_overseer
	})
}

#[test]
fn disconnect_if_wrong_declare() {
	let mut test_state = TestState::default();

	test_harness(ReputationAggregator::new(|_| true), |test_harness| async move {
		let TestHarness { mut virtual_overseer, .. } = test_harness;
		let pair = CollatorPair::generate().0;
		let peer_b = PeerId::random();

		let relay_parent = test_state.relay_parent;
		update_view(&mut virtual_overseer, &mut test_state, vec![(relay_parent, 0)]).await;

		overseer_send(
			&mut virtual_overseer,
			CollatorProtocolMessage::NetworkBridgeUpdate(NetworkBridgeEvent::PeerConnected(
				peer_b,
				ObservedRole::Full,
				CollationVersion::V1.into(),
				None,
			)),
		)
		.await;

		overseer_send(
			&mut virtual_overseer,
			CollatorProtocolMessage::NetworkBridgeUpdate(NetworkBridgeEvent::PeerMessage(
				peer_b,
				Versioned::V1(protocol_v1::CollatorProtocolMessage::Declare(
					pair.public(),
					ParaId::from(69),
					pair.sign(&protocol_v1::declare_signature_payload(&peer_b)),
				)),
			)),
		)
		.await;

		assert_matches!(
			overseer_recv(&mut virtual_overseer).await,
			AllMessages::NetworkBridgeTx(NetworkBridgeTxMessage::ReportPeer(
				ReportPeerMessage::Single(peer, rep),
			)) => {
				assert_eq!(peer, peer_b);
				assert_eq!(rep.value, COST_UNNEEDED_COLLATOR.cost_or_benefit());
			}
		);

		assert_collator_disconnect(&mut virtual_overseer, peer_b).await;

		virtual_overseer
	})
}

#[test]
fn delay_reputation_change() {
	let mut test_state = TestState::default();

	test_harness(ReputationAggregator::new(|_| false), |test_harness| async move {
		let TestHarness { mut virtual_overseer, .. } = test_harness;
		let pair = CollatorPair::generate().0;
		let peer_b = PeerId::random();

		let relay_parent = test_state.relay_parent;
		update_view(&mut virtual_overseer, &mut test_state, vec![(relay_parent, 0)]).await;

		overseer_send(
			&mut virtual_overseer,
			CollatorProtocolMessage::NetworkBridgeUpdate(NetworkBridgeEvent::PeerConnected(
				peer_b,
				ObservedRole::Full,
				CollationVersion::V1.into(),
				None,
			)),
		)
		.await;

		overseer_send(
			&mut virtual_overseer,
			CollatorProtocolMessage::NetworkBridgeUpdate(NetworkBridgeEvent::PeerMessage(
				peer_b,
				Versioned::V1(protocol_v1::CollatorProtocolMessage::Declare(
					pair.public(),
					ParaId::from(69),
					pair.sign(&protocol_v1::declare_signature_payload(&peer_b)),
				)),
			)),
		)
		.await;

		overseer_send(
			&mut virtual_overseer,
			CollatorProtocolMessage::NetworkBridgeUpdate(NetworkBridgeEvent::PeerMessage(
				peer_b,
				Versioned::V1(protocol_v1::CollatorProtocolMessage::Declare(
					pair.public(),
					ParaId::from(69),
					pair.sign(&protocol_v1::declare_signature_payload(&peer_b)),
				)),
			)),
		)
		.await;

		// Wait enough to fire reputation delay
		futures_timer::Delay::new(REPUTATION_CHANGE_TEST_INTERVAL).await;

		loop {
			match overseer_recv(&mut virtual_overseer).await {
				AllMessages::NetworkBridgeTx(NetworkBridgeTxMessage::DisconnectPeer(_, _)) => {
					gum::trace!("`Disconnecting inactive peer` message skipped");
					continue
				},
				AllMessages::NetworkBridgeTx(NetworkBridgeTxMessage::ReportPeer(
					ReportPeerMessage::Batch(v),
				)) => {
					let mut expected_change = HashMap::new();
					for rep in vec![COST_UNNEEDED_COLLATOR, COST_UNNEEDED_COLLATOR] {
						add_reputation(&mut expected_change, peer_b, rep);
					}
					assert_eq!(v, expected_change);
					break
				},
				_ => panic!("Message should be either `DisconnectPeer` or `ReportPeer`"),
			}
		}

		virtual_overseer
	})
}

#[test]
fn view_change_clears_old_collators() {
	let mut test_state = TestState::default();

	test_harness(ReputationAggregator::new(|_| true), |test_harness| async move {
		let TestHarness { mut virtual_overseer, .. } = test_harness;

		let pair = CollatorPair::generate().0;

		let peer = PeerId::random();
		let relay_parent = test_state.relay_parent;
		update_view(&mut virtual_overseer, &mut test_state, vec![(relay_parent, 0)]).await;

		connect_and_declare_collator(
			&mut virtual_overseer,
			peer,
			pair.clone(),
			test_state.chain_ids[0],
			CollationVersion::V1,
		)
		.await;

		test_state.group_rotation_info = test_state.group_rotation_info.bump_rotation();

		update_view(&mut virtual_overseer, &mut test_state, vec![]).await;

		assert_collator_disconnect(&mut virtual_overseer, peer).await;

		virtual_overseer
	})
}
