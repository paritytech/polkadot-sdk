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
use futures_timer::Delay;
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
	AllMessages, CandidateBackingMessage, IfDisconnected, NetworkBridgeTxMessage,
	ProspectiveParachainsMessage, ReportPeerMessage,
};
use polkadot_node_subsystem_test_helpers as test_helpers;
use polkadot_node_subsystem_util::{reputation::add_reputation, TimeoutExt};
use polkadot_primitives::{
	node_features, CandidateReceiptV2 as CandidateReceipt, CollatorPair, CoreIndex,
	GroupRotationInfo, HeadData, NodeFeatures, PersistedValidationData, ValidatorId,
	ValidatorIndex,
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
	scheduling_lookahead: u32,
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

		let scheduling_lookahead = 3;
		let mut claim_queue = BTreeMap::new();
		claim_queue.insert(
			CoreIndex(0),
			iter::repeat(ParaId::from(Self::CHAIN_IDS[0]))
				.take(scheduling_lookahead as usize)
				.collect(),
		);
		claim_queue.insert(CoreIndex(1), VecDeque::new());
		claim_queue.insert(
			CoreIndex(2),
			iter::repeat(ParaId::from(Self::CHAIN_IDS[1]))
				.take(scheduling_lookahead as usize)
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
			scheduling_lookahead,
			node_features,
			session_index: 1,
			last_known_block: None,
		}
	}
}

impl TestState {
	const CHAIN_IDS: [u32; 2] = [1, 2];

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
			claim_queue.get(&CoreIndex(0)).unwrap().len() == state.scheduling_lookahead as usize
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
			claim_queue.get(&CoreIndex(0)).unwrap().len() == state.scheduling_lookahead as usize
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
	ah_invulnerable_collators: HashSet<PeerId>,
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
		ah_invulnerable_collators,
		HOLD_OFF_DURATION_DEFAULT_VALUE,
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
	expected_scheduling_parent: Hash,
	expected_para_id: ParaId,
	expected_pov: &PoV,
) -> CandidateReceipt {
	let pvd = dummy_pvd();

	let msg = overseer_recv(virtual_overseer).await;
	assert_matches!(
		msg,
		AllMessages::ProspectiveParachains(
			ProspectiveParachainsMessage::GetProspectiveValidationData(request, tx),
		) => {
			assert_eq!(expected_scheduling_parent, request.candidate_relay_parent);
			assert_eq!(expected_para_id, request.para_id);
			tx.send(Some(pvd.clone())).unwrap();
		}
	);

	assert_matches!(
		overseer_recv(virtual_overseer).await,
		AllMessages::CandidateBacking(CandidateBackingMessage::Second {
			scheduling_parent,
			candidate: candidate_receipt,
			pvd: received_pvd,
			pov: incoming_pov,
		}) => {
			assert_eq!(expected_scheduling_parent, scheduling_parent);
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
		AllMessages::NetworkBridgeTx(NetworkBridgeTxMessage::DisconnectPeers(
			peers,
			peer_set,
		)) => {
			assert_eq!(vec![expected_peer], peers);
			assert_eq!(PeerSet::Collation, peer_set);
		}
	);
}

/// Assert that a fetch collation request was send.
async fn assert_fetch_collation_request(
	virtual_overseer: &mut VirtualOverseer,
	scheduling_parent: Hash,
	para_id: ParaId,
	candidate_hash: CandidateHash,
) -> ResponseSender {
	assert_matches!(
		overseer_recv(virtual_overseer).await,
		AllMessages::NetworkBridgeTx(NetworkBridgeTxMessage::SendRequests(reqs, IfDisconnected::ImmediateError)
	) => {
		let req = reqs.into_iter().next()
			.expect("There should be exactly one request");
		assert_matches!(
			req,
			Requests::CollationFetchingV2(req) => {
				let payload = req.payload;
				assert_eq!(payload.scheduling_parent, scheduling_parent);
				assert_eq!(payload.para_id, para_id);
				assert_eq!(payload.candidate_hash, candidate_hash);
				req.pending_response
			}
		)
	})
}

/// After a V2 `AdvertiseCollation`, receive exactly one `CanSecond` for this candidate.
///
/// `can_second` is awaited before requesting the collation, so there must be no
/// `CollationFetchingV2` message for this candidate ahead of this in the test harness queue.
async fn respond_to_can_second(
	virtual_overseer: &mut VirtualOverseer,
	candidate_hash: CandidateHash,
	para_id: ParaId,
	response: bool,
) {
	assert_matches!(
		overseer_recv(virtual_overseer).await,
		AllMessages::CandidateBacking(CandidateBackingMessage::CanSecond(request, tx)) => {
			assert_eq!(request.candidate_hash, candidate_hash);
			assert_eq!(request.candidate_para_id, para_id);
			tx.send(response).expect("receiving side should be alive");
		}
	);
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

	let wire_message = CollationProtocols::V2(protocol_v2::CollatorProtocolMessage::Declare(
		collator.public(),
		para_id,
		collator.sign(&protocol_v2::declare_signature_payload(&peer)),
	));

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
	scheduling_parent: Hash,
	candidate: Option<(CandidateHash, Hash)>, // Candidate hash + parent head data hash.
) {
	let (candidate_hash, parent_head_data_hash) = candidate.expect(
		"V2+ advertisements require candidate hash and parent head data hash; V1 is removed",
	);
	let wire_message =
		CollationProtocols::V2(protocol_v2::CollatorProtocolMessage::AdvertiseCollation {
			scheduling_parent,
			candidate_hash,
			parent_head_data_hash,
		});
	overseer_send(
		virtual_overseer,
		CollatorProtocolMessage::NetworkBridgeUpdate(NetworkBridgeEvent::PeerMessage(
			peer,
			wire_message,
		)),
	)
	.await;
}

/// Advertise a collation using the V3 protocol, which includes the candidate descriptor version.
async fn advertise_collation_v3(
	virtual_overseer: &mut VirtualOverseer,
	peer: PeerId,
	scheduling_parent: Hash,
	candidate_hash: CandidateHash,
	parent_head_data_hash: Hash,
	candidate_descriptor_version: CandidateDescriptorVersion,
) {
	let wire_message =
		CollationProtocols::V3(protocol_v3::CollatorProtocolMessage::AdvertiseCollation {
			scheduling_parent,
			candidate_hash,
			parent_head_data_hash,
			candidate_descriptor_version,
		});
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

	test_harness(ReputationAggregator::new(|_| true), HashSet::new(), |test_harness| async move {
		let TestHarness { mut virtual_overseer, .. } = test_harness;

		let peer_b = PeerId::random();

		overseer_send(
			&mut virtual_overseer,
			CollatorProtocolMessage::NetworkBridgeUpdate(NetworkBridgeEvent::PeerConnected(
				peer_b,
				ObservedRole::Full,
				CollationVersion::V2.into(),
				None,
			)),
		)
		.await;

		// the peer sends a declare message but sign the wrong payload
		overseer_send(
			&mut virtual_overseer,
			CollatorProtocolMessage::NetworkBridgeUpdate(NetworkBridgeEvent::PeerMessage(
				peer_b,
				CollationProtocols::V2(protocol_v2::CollatorProtocolMessage::Declare(
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

/// With V2+ advertisements, only one collation fetch is in flight per relay parent; further ads
/// wait.
#[test]
fn fetch_one_collation_at_a_time_for_v2_advertisement() {
	let mut test_state = TestState::default();

	test_harness(ReputationAggregator::new(|_| true), HashSet::new(), |test_harness| async move {
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
			CollationVersion::V2,
		)
		.await;

		connect_and_declare_collator(
			&mut virtual_overseer,
			peer_c,
			test_state.collators[1].clone(),
			test_state.chain_ids[0],
			CollationVersion::V2,
		)
		.await;

		let pov_b = PoV { block_data: BlockData(vec![10; 1024]) };
		let pov_c = PoV { block_data: BlockData(vec![20; 1024]) };

		let mut cand_b = dummy_candidate_receipt_bad_sig(dummy_hash(), Some(Default::default()));
		cand_b.descriptor.para_id = test_state.chain_ids[0];
		cand_b.descriptor.relay_parent = test_state.relay_parent;
		cand_b.descriptor.persisted_validation_data_hash = dummy_pvd().hash();
		cand_b.commitments_hash = Hash::from_low_u64_be(11);
		let candidate_hash_b = cand_b.hash();

		let mut cand_c = dummy_candidate_receipt_bad_sig(dummy_hash(), Some(Default::default()));
		cand_c.descriptor.para_id = test_state.chain_ids[0];
		cand_c.descriptor.relay_parent = test_state.relay_parent;
		cand_c.descriptor.persisted_validation_data_hash = dummy_pvd().hash();
		cand_c.commitments_hash = Hash::from_low_u64_be(22);
		let candidate_hash_c = cand_c.hash();

		for (peer, cand, candidate_hash, pov) in
			[(peer_b, cand_b, candidate_hash_b, pov_b), (peer_c, cand_c, candidate_hash_c, pov_c)]
		{
			advertise_collation(
				&mut virtual_overseer,
				peer,
				relay_parent,
				Some((candidate_hash, Hash::zero())),
			)
			.await;

			assert_matches!(
				overseer_recv(&mut virtual_overseer).await,
				AllMessages::CandidateBacking(CandidateBackingMessage::CanSecond(request, tx)) => {
					tx.send(true).expect("receiving side should be alive");
					assert_eq!(request.candidate_hash, candidate_hash);
				}
			);

			let response_channel = assert_fetch_collation_request(
				&mut virtual_overseer,
				test_state.relay_parent,
				test_state.chain_ids[0],
				candidate_hash,
			)
			.await;

			assert!(
				overseer_recv_with_timeout(&mut &mut virtual_overseer, Duration::from_millis(30))
					.await
					.is_none(),
				"There should not be sent any other PoV request while the first one wasn't finished or timed out.",
			);

			let candidate: CandidateReceipt = cand.clone().into();
			response_channel
				.send(Ok((
					request_v2::CollationFetchingResponse::Collation(
						candidate.clone().into(),
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
			)
			.await;
		}

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

	test_harness(ReputationAggregator::new(|_| true), HashSet::new(), |test_harness| async move {
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
			CollationVersion::V2,
		)
		.await;

		connect_and_declare_collator(
			&mut virtual_overseer,
			peer_c,
			test_state.collators[3].clone(),
			test_state.chain_ids[0],
			CollationVersion::V2,
		)
		.await;

		connect_and_declare_collator(
			&mut virtual_overseer,
			peer_d,
			test_state.collators[4].clone(),
			test_state.chain_ids[0],
			CollationVersion::V2,
		)
		.await;

		let pov = PoV { block_data: BlockData(vec![1]) };

		let mut candidate_b =
			dummy_candidate_receipt_bad_sig(dummy_hash(), Some(Default::default()));
		candidate_b.descriptor.para_id = test_state.chain_ids[0];
		candidate_b.descriptor.relay_parent = second;
		candidate_b.descriptor.persisted_validation_data_hash = dummy_pvd().hash();
		candidate_b.commitments_hash = Hash::from_low_u64_be(1);
		let candidate_hash_b = candidate_b.hash();

		let mut candidate_c =
			dummy_candidate_receipt_bad_sig(dummy_hash(), Some(Default::default()));
		candidate_c.descriptor.para_id = test_state.chain_ids[0];
		candidate_c.descriptor.relay_parent = second;
		candidate_c.descriptor.persisted_validation_data_hash = dummy_pvd().hash();
		candidate_c.commitments_hash = Hash::from_low_u64_be(2);
		let candidate_hash_c = candidate_c.hash();

		let mut candidate_d =
			dummy_candidate_receipt_bad_sig(dummy_hash(), Some(Default::default()));
		candidate_d.descriptor.para_id = test_state.chain_ids[0];
		candidate_d.descriptor.relay_parent = second;
		candidate_d.descriptor.persisted_validation_data_hash = dummy_pvd().hash();
		candidate_d.commitments_hash = Hash::from_low_u64_be(3);
		let candidate_hash_d = candidate_d.hash();

		// One advertisement at a time: answer `CanSecond`, then take this advert's fetch before
		// advertising the next. A previous collator's download may still be in flight; the next
		// `CollationFetchingV2` is only issued after `MAX_UNSHARED_DOWNLOAD_TIME` when the prior
		// fetch has not completed.
		advertise_collation(
			&mut virtual_overseer,
			peer_b,
			second,
			Some((candidate_hash_b, Hash::zero())),
		)
		.await;
		respond_to_can_second(
			&mut virtual_overseer,
			candidate_hash_b,
			test_state.chain_ids[0],
			true,
		)
		.await;
		let _response_channel_b = assert_fetch_collation_request(
			&mut virtual_overseer,
			second,
			test_state.chain_ids[0],
			candidate_hash_b,
		)
		.await;

		advertise_collation(
			&mut virtual_overseer,
			peer_c,
			second,
			Some((candidate_hash_c, Hash::zero())),
		)
		.await;
		respond_to_can_second(
			&mut virtual_overseer,
			candidate_hash_c,
			test_state.chain_ids[0],
			true,
		)
		.await;
		Delay::new(MAX_UNSHARED_DOWNLOAD_TIME + Duration::from_millis(50)).await;
		let response_channel_non_exclusive = assert_fetch_collation_request(
			&mut virtual_overseer,
			second,
			test_state.chain_ids[0],
			candidate_hash_c,
		)
		.await;

		advertise_collation(
			&mut virtual_overseer,
			peer_d,
			second,
			Some((candidate_hash_d, Hash::zero())),
		)
		.await;
		respond_to_can_second(
			&mut virtual_overseer,
			candidate_hash_d,
			test_state.chain_ids[0],
			true,
		)
		.await;
		Delay::new(MAX_UNSHARED_DOWNLOAD_TIME + Duration::from_millis(50)).await;
		let response_channel = assert_fetch_collation_request(
			&mut virtual_overseer,
			second,
			test_state.chain_ids[0],
			candidate_hash_d,
		)
		.await;

		response_channel_non_exclusive
			.send(Ok((
				request_v2::CollationFetchingResponse::Collation(
					candidate_c.clone().into(),
					pov.clone(),
				)
				.encode(),
				ProtocolName::from(""),
			)))
			.expect("Sending response should succeed");

		response_channel
			.send(Ok((
				request_v2::CollationFetchingResponse::Collation(
					candidate_d.clone().into(),
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
		)
		.await;

		assert_candidate_backing_second(
			&mut virtual_overseer,
			second,
			test_state.chain_ids[0],
			&pov,
		)
		.await;

		virtual_overseer
	});
}

#[test]
fn reject_connection_to_next_group() {
	let mut test_state = TestState::default();

	test_harness(ReputationAggregator::new(|_| true), HashSet::new(), |test_harness| async move {
		let TestHarness { mut virtual_overseer, .. } = test_harness;

		let relay_parent = test_state.relay_parent;
		update_view(&mut virtual_overseer, &mut test_state, vec![(relay_parent, 0)]).await;

		let peer_b = PeerId::random();

		connect_and_declare_collator(
			&mut virtual_overseer,
			peer_b,
			test_state.collators[0].clone(),
			test_state.chain_ids[1], // next, not current `para_id`
			CollationVersion::V2,
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

	test_harness(ReputationAggregator::new(|_| true), HashSet::new(), |test_harness| async move {
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
			CollationVersion::V2,
		)
		.await;

		connect_and_declare_collator(
			&mut virtual_overseer,
			peer_c,
			test_state.collators[1].clone(),
			test_state.chain_ids[0],
			CollationVersion::V2,
		)
		.await;

		let pov = PoV { block_data: BlockData(vec![]) };
		let mut candidate_a =
			dummy_candidate_receipt_bad_sig(dummy_hash(), Some(Default::default()));
		candidate_a.descriptor.para_id = test_state.chain_ids[0];
		candidate_a.descriptor.relay_parent = relay_parent;
		candidate_a.descriptor.persisted_validation_data_hash = dummy_pvd().hash();
		let candidate_hash_b = candidate_a.hash();

		let mut candidate_c =
			dummy_candidate_receipt_bad_sig(dummy_hash(), Some(Default::default()));
		candidate_c.descriptor.para_id = test_state.chain_ids[0];
		candidate_c.descriptor.relay_parent = relay_parent;
		candidate_c.descriptor.persisted_validation_data_hash = dummy_pvd().hash();
		candidate_c.commitments_hash = Hash::from_low_u64_be(99);
		let candidate_hash_c = candidate_c.hash();

		advertise_collation(
			&mut virtual_overseer,
			peer_b,
			relay_parent,
			Some((candidate_hash_b, Hash::zero())),
		)
		.await;
		respond_to_can_second(
			&mut virtual_overseer,
			candidate_hash_b,
			test_state.chain_ids[0],
			true,
		)
		.await;
		let response_channel = assert_fetch_collation_request(
			&mut virtual_overseer,
			relay_parent,
			test_state.chain_ids[0],
			candidate_hash_b,
		)
		.await;

		advertise_collation(
			&mut virtual_overseer,
			peer_c,
			relay_parent,
			Some((candidate_hash_c, Hash::zero())),
		)
		.await;
		respond_to_can_second(
			&mut virtual_overseer,
			candidate_hash_c,
			test_state.chain_ids[0],
			true,
		)
		.await;

		response_channel
			.send(Ok((
				request_v2::CollationFetchingResponse::Collation(
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

		let _ = assert_fetch_collation_request(
			&mut virtual_overseer,
			relay_parent,
			test_state.chain_ids[0],
			candidate_hash_c,
		)
		.await;

		virtual_overseer
	});
}

#[test]
fn inactive_disconnected() {
	let mut test_state = TestState::default();

	test_harness(ReputationAggregator::new(|_| true), HashSet::new(), |test_harness| async move {
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
			CollationVersion::V2,
		)
		.await;

		let candidate_hash = CandidateHash(Hash::from_low_u64_be(7001));
		advertise_collation(
			&mut virtual_overseer,
			peer_b,
			relay_parent,
			Some((candidate_hash, Hash::zero())),
		)
		.await;

		assert_matches!(
			overseer_recv(&mut virtual_overseer).await,
			AllMessages::CandidateBacking(CandidateBackingMessage::CanSecond(request, tx)) => {
				assert_eq!(request.candidate_hash, candidate_hash);
				tx.send(true).expect("receiving side should be alive");
			}
		);

		let _ = assert_fetch_collation_request(
			&mut virtual_overseer,
			relay_parent,
			test_state.chain_ids[0],
			candidate_hash,
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

	test_harness(ReputationAggregator::new(|_| true), HashSet::new(), |test_harness| async move {
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
			CollationVersion::V2,
		)
		.await;

		Delay::new(ACTIVITY_TIMEOUT * 2 / 3).await;

		let h_a = CandidateHash(Hash::from_low_u64_be(8001));
		advertise_collation(&mut virtual_overseer, peer_b, hash_a, Some((h_a, Hash::zero()))).await;
		assert_matches!(
			overseer_recv(&mut virtual_overseer).await,
			AllMessages::CandidateBacking(CandidateBackingMessage::CanSecond(request, tx)) => {
				assert_eq!(request.candidate_hash, h_a);
				tx.send(true).expect("receiving side should be alive");
			}
		);
		let _ = assert_fetch_collation_request(
			&mut virtual_overseer,
			hash_a,
			test_state.chain_ids[0],
			h_a,
		)
		.await;

		Delay::new(ACTIVITY_TIMEOUT * 2 / 3).await;

		let h_b = CandidateHash(Hash::from_low_u64_be(8002));
		advertise_collation(&mut virtual_overseer, peer_b, hash_b, Some((h_b, Hash::zero()))).await;
		assert_matches!(
			overseer_recv(&mut virtual_overseer).await,
			AllMessages::CandidateBacking(CandidateBackingMessage::CanSecond(request, tx)) => {
				assert_eq!(request.candidate_hash, h_b);
				tx.send(true).expect("receiving side should be alive");
			}
		);
		let _ = assert_fetch_collation_request(
			&mut virtual_overseer,
			hash_b,
			test_state.chain_ids[0],
			h_b,
		)
		.await;

		Delay::new(ACTIVITY_TIMEOUT * 2 / 3).await;

		let h_c = CandidateHash(Hash::from_low_u64_be(8003));
		advertise_collation(&mut virtual_overseer, peer_b, hash_c, Some((h_c, Hash::zero()))).await;
		assert_matches!(
			overseer_recv(&mut virtual_overseer).await,
			AllMessages::CandidateBacking(CandidateBackingMessage::CanSecond(request, tx)) => {
				assert_eq!(request.candidate_hash, h_c);
				tx.send(true).expect("receiving side should be alive");
			}
		);
		let _ = assert_fetch_collation_request(
			&mut virtual_overseer,
			hash_c,
			test_state.chain_ids[0],
			h_c,
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

	test_harness(ReputationAggregator::new(|_| true), HashSet::new(), |test_harness| async move {
		let TestHarness { mut virtual_overseer, .. } = test_harness;

		let relay_parent = test_state.relay_parent;
		update_view(&mut virtual_overseer, &mut test_state, vec![(relay_parent, 0)]).await;

		let peer_b = PeerId::random();

		overseer_send(
			&mut virtual_overseer,
			CollatorProtocolMessage::NetworkBridgeUpdate(NetworkBridgeEvent::PeerConnected(
				peer_b,
				ObservedRole::Full,
				CollationVersion::V2.into(),
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

	test_harness(ReputationAggregator::new(|_| true), HashSet::new(), |test_harness| async move {
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
				CollationVersion::V2.into(),
				None,
			)),
		)
		.await;

		overseer_send(
			&mut virtual_overseer,
			CollatorProtocolMessage::NetworkBridgeUpdate(NetworkBridgeEvent::PeerMessage(
				peer_b,
				CollationProtocols::V2(protocol_v2::CollatorProtocolMessage::Declare(
					pair.public(),
					ParaId::from(69),
					pair.sign(&protocol_v2::declare_signature_payload(&peer_b)),
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

	test_harness(ReputationAggregator::new(|_| false), HashSet::new(), |test_harness| async move {
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
				CollationVersion::V2.into(),
				None,
			)),
		)
		.await;

		overseer_send(
			&mut virtual_overseer,
			CollatorProtocolMessage::NetworkBridgeUpdate(NetworkBridgeEvent::PeerMessage(
				peer_b,
				CollationProtocols::V2(protocol_v2::CollatorProtocolMessage::Declare(
					pair.public(),
					ParaId::from(69),
					pair.sign(&protocol_v2::declare_signature_payload(&peer_b)),
				)),
			)),
		)
		.await;

		overseer_send(
			&mut virtual_overseer,
			CollatorProtocolMessage::NetworkBridgeUpdate(NetworkBridgeEvent::PeerMessage(
				peer_b,
				CollationProtocols::V2(protocol_v2::CollatorProtocolMessage::Declare(
					pair.public(),
					ParaId::from(69),
					pair.sign(&protocol_v2::declare_signature_payload(&peer_b)),
				)),
			)),
		)
		.await;

		// Wait enough to fire reputation delay
		futures_timer::Delay::new(REPUTATION_CHANGE_TEST_INTERVAL).await;

		loop {
			match overseer_recv(&mut virtual_overseer).await {
				AllMessages::NetworkBridgeTx(NetworkBridgeTxMessage::DisconnectPeers(_, _)) => {
					gum::trace!("`Disconnecting inactive peer` message skipped");
					continue;
				},
				AllMessages::NetworkBridgeTx(NetworkBridgeTxMessage::ReportPeer(
					ReportPeerMessage::Batch(v),
				)) => {
					let mut expected_change = HashMap::new();
					for rep in vec![COST_UNNEEDED_COLLATOR, COST_UNNEEDED_COLLATOR] {
						add_reputation(&mut expected_change, peer_b, rep);
					}
					assert_eq!(v, expected_change);
					break;
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

	test_harness(ReputationAggregator::new(|_| true), HashSet::new(), |test_harness| async move {
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
			CollationVersion::V2,
		)
		.await;

		test_state.group_rotation_info = test_state.group_rotation_info.bump_rotation();

		update_view(&mut virtual_overseer, &mut test_state, vec![]).await;

		assert_collator_disconnect(&mut virtual_overseer, peer).await;

		virtual_overseer
	})
}

/// Test that when a peer disconnects, their pending collations are removed from the waiting queue.
/// This prevents "NotAdvertised" errors when the peer reconnects with empty advertisement state.
#[test]
fn peer_disconnect_clears_pending_collations_from_waiting_queue() {
	let mut test_state = TestState::default();

	test_harness(ReputationAggregator::new(|_| true), HashSet::new(), |test_harness| async move {
		let TestHarness { mut virtual_overseer, .. } = test_harness;

		let relay_parent = test_state.relay_parent;
		update_view(&mut virtual_overseer, &mut test_state, vec![(relay_parent, 0)]).await;

		// Connect first collator and have them advertise - this will trigger a fetch.
		let peer_a = PeerId::random();
		let collator_a = test_state.collators[0].clone();

		connect_and_declare_collator(
			&mut virtual_overseer,
			peer_a,
			collator_a.clone(),
			test_state.chain_ids[0],
			CollationVersion::V2,
		)
		.await;

		let mut candidate_a_pre =
			dummy_candidate_receipt_bad_sig(dummy_hash(), Some(Default::default()));
		candidate_a_pre.descriptor.para_id = test_state.chain_ids[0];
		candidate_a_pre.descriptor.relay_parent = relay_parent;
		candidate_a_pre.descriptor.persisted_validation_data_hash = dummy_pvd().hash();
		let candidate_hash_a = candidate_a_pre.hash();

		advertise_collation(
			&mut virtual_overseer,
			peer_a,
			relay_parent,
			Some((candidate_hash_a, Hash::zero())),
		)
		.await;

		assert_matches!(
			overseer_recv(&mut virtual_overseer).await,
			AllMessages::CandidateBacking(CandidateBackingMessage::CanSecond(request, tx)) => {
				assert_eq!(request.candidate_hash, candidate_hash_a);
				tx.send(true).expect("receiving side should be alive");
			}
		);

		// First collation fetch is initiated.
		let response_channel_a = assert_fetch_collation_request(
			&mut virtual_overseer,
			relay_parent,
			test_state.chain_ids[0],
			candidate_hash_a,
		)
		.await;

		// Connect second collator and have them advertise.
		// Since we're already fetching, this goes into the waiting queue.
		let peer_b = PeerId::random();
		let collator_b = test_state.collators[1].clone();

		connect_and_declare_collator(
			&mut virtual_overseer,
			peer_b,
			collator_b.clone(),
			test_state.chain_ids[0],
			CollationVersion::V2,
		)
		.await;

		let candidate_hash_b = CandidateHash(Hash::from_low_u64_be(9002));
		advertise_collation(
			&mut virtual_overseer,
			peer_b,
			relay_parent,
			Some((candidate_hash_b, Hash::zero())),
		)
		.await;

		assert_matches!(
			overseer_recv(&mut virtual_overseer).await,
			AllMessages::CandidateBacking(CandidateBackingMessage::CanSecond(request, tx)) => {
				assert_eq!(request.candidate_hash, candidate_hash_b);
				tx.send(true).expect("receiving side should be alive");
			}
		);

		// Now disconnect peer_b. This should clean up their entry from the waiting queue.
		overseer_send(
			&mut virtual_overseer,
			CollatorProtocolMessage::NetworkBridgeUpdate(NetworkBridgeEvent::PeerDisconnected(
				peer_b,
			)),
		)
		.await;

		// Peer_b reconnects and declares again (but does NOT re-advertise yet).
		overseer_send(
			&mut virtual_overseer,
			CollatorProtocolMessage::NetworkBridgeUpdate(NetworkBridgeEvent::PeerConnected(
				peer_b,
				ObservedRole::Full,
				CollationVersion::V2.into(),
				None,
			)),
		)
		.await;

		overseer_send(
			&mut virtual_overseer,
			CollatorProtocolMessage::NetworkBridgeUpdate(NetworkBridgeEvent::PeerMessage(
				peer_b,
				CollationProtocols::V2(protocol_v2::CollatorProtocolMessage::Declare(
					collator_b.public(),
					test_state.chain_ids[0],
					collator_b.sign(&protocol_v2::declare_signature_payload(&peer_b)),
				)),
			)),
		)
		.await;

		// Complete the first fetch from peer_a.
		let pov = PoV { block_data: BlockData(vec![]) };
		let candidate_a: CandidateReceipt = candidate_a_pre.into();

		response_channel_a
			.send(Ok((
				request_v2::CollationFetchingResponse::Collation(
					candidate_a.clone().into(),
					pov.clone(),
				)
				.encode(),
				ProtocolName::from(""),
			)))
			.expect("Sending response should succeed");

		// This triggers candidate backing.
		assert_candidate_backing_second(
			&mut virtual_overseer,
			relay_parent,
			test_state.chain_ids[0],
			&pov,
		)
		.await;

		// Ensure the subsystem is polled.
		test_helpers::Yield::new().await;

		// The key assertion: after completing the first fetch, the subsystem should NOT
		// attempt to fetch from peer_b because their waiting queue entry was cleaned up
		// on disconnect. Without the fix, we would see a fetch request here that would
		// fail with "NotAdvertised" because peer_b's advertisement state was cleared
		// when they disconnected.
		assert!(
			overseer_recv_with_timeout(&mut virtual_overseer, Duration::from_millis(100))
				.await
				.is_none(),
			"There should be no fetch request for peer_b - their entry was cleaned from waiting queue on disconnect"
		);

		virtual_overseer
	})
}
