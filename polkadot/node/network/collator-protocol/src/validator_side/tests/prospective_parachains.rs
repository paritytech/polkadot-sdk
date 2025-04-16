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

//! Tests for the validator side with enabled prospective parachains.

use super::*;

use polkadot_node_subsystem::messages::ChainApiMessage;
use polkadot_primitives::{
	vstaging::{CommittedCandidateReceiptV2 as CommittedCandidateReceipt, MutateDescriptorV2},
	BlockNumber, CandidateCommitments, Header, SigningContext, ValidatorId,
};
use polkadot_primitives_test_helpers::dummy_committed_candidate_receipt_v2;
use rstest::rstest;

fn get_parent_hash(hash: Hash) -> Hash {
	Hash::from_low_u64_be(hash.to_low_u64_be() + 1)
}

async fn assert_construct_per_relay_parent(
	virtual_overseer: &mut VirtualOverseer,
	test_state: &TestState,
	hash: Hash,
	number: BlockNumber,
	next_msg: &mut Option<AllMessages>,
) {
	let msg = match next_msg.take() {
		Some(msg) => msg,
		None => overseer_recv(virtual_overseer).await,
	};
	assert_matches!(
		msg,
		AllMessages::RuntimeApi(
			RuntimeApiMessage::Request(parent, RuntimeApiRequest::Validators(tx))
		) => {
			assert_eq!(parent, hash);
			tx.send(Ok(test_state.validator_public.clone())).unwrap();
		}
	);

	assert_matches!(
		overseer_recv(virtual_overseer).await,
		AllMessages::RuntimeApi(
			RuntimeApiMessage::Request(parent, RuntimeApiRequest::ValidatorGroups(tx))
		) if parent == hash => {
			let validator_groups = test_state.validator_groups.clone();
			let mut group_rotation_info = test_state.group_rotation_info.clone();
			group_rotation_info.now = number;
			tx.send(Ok((validator_groups, group_rotation_info))).unwrap();
		}
	);

	assert_matches!(
		overseer_recv(virtual_overseer).await,
		AllMessages::RuntimeApi(RuntimeApiMessage::Request(
			parent,
			RuntimeApiRequest::ClaimQueue(tx),
		)) if parent == hash => {
			let _ = tx.send(Ok(test_state.claim_queue.clone()));
		}
	);
}

/// Handle a view update.
pub(super) async fn update_view(
	virtual_overseer: &mut VirtualOverseer,
	test_state: &mut TestState,
	new_view: Vec<(Hash, u32)>, // Hash and block number.
) -> Option<AllMessages> {
	let last_block_from_view = new_view.last().map(|t| t.1);
	let new_view: HashMap<Hash, u32> = HashMap::from_iter(new_view);
	let our_view = OurView::new(new_view.keys().map(|hash| *hash), 0);

	overseer_send(
		virtual_overseer,
		CollatorProtocolMessage::NetworkBridgeUpdate(NetworkBridgeEvent::OurViewChange(our_view)),
	)
	.await;

	let mut next_overseer_message = None;
	for _ in 0..new_view.len() {
		let msg = match next_overseer_message.take() {
			Some(msg) => msg,
			None => overseer_recv(virtual_overseer).await,
		};

		let (leaf_hash, leaf_number) = assert_matches!(
			msg,
			AllMessages::RuntimeApi(RuntimeApiMessage::Request(
				parent,
				RuntimeApiRequest::SessionIndexForChild(tx)
			)) => {
				tx.send(Ok(test_state.session_index)).unwrap();
				(parent, new_view.get(&parent).copied().expect("Unknown parent requested"))
			}
		);

		assert_matches!(
			overseer_recv(virtual_overseer).await,
			AllMessages::RuntimeApi(RuntimeApiMessage::Request(
				_,
				RuntimeApiRequest::NodeFeatures(_, tx)
			)) => {
				tx.send(Ok(test_state.node_features.clone())).unwrap();
			}
		);

		assert_construct_per_relay_parent(
			virtual_overseer,
			test_state,
			leaf_hash,
			leaf_number,
			&mut next_overseer_message,
		)
		.await;

		let min_number = leaf_number.saturating_sub(test_state.scheduling_lookahead);

		let ancestry_len = leaf_number + 1 - min_number;
		let ancestry_hashes = std::iter::successors(Some(leaf_hash), |h| Some(get_parent_hash(*h)))
			.take(ancestry_len as usize);
		let ancestry_numbers = (min_number..=leaf_number).rev();
		let ancestry_iter = ancestry_hashes.clone().zip(ancestry_numbers).peekable();

		// How many blocks were actually requested.
		let mut requested_len: usize = 0;
		{
			let mut ancestry_iter = ancestry_iter.clone();
			while let Some((hash, number)) = ancestry_iter.next() {
				if Some(number) == test_state.last_known_block {
					break;
				}

				// May be `None` for the last element.
				let parent_hash =
					ancestry_iter.peek().map(|(h, _)| *h).unwrap_or_else(|| get_parent_hash(hash));

				let msg = match next_overseer_message.take() {
					Some(msg) => msg,
					None => overseer_recv(virtual_overseer).await,
				};

				if !matches!(&msg, AllMessages::ChainApi(ChainApiMessage::BlockHeader(..))) {
					// Ancestry has already been cached for this leaf.
					next_overseer_message.replace(msg);
					break
				}

				assert_matches!(
					msg,
					AllMessages::ChainApi(ChainApiMessage::BlockHeader(.., tx)) => {
						let header = Header {
							parent_hash,
							number,
							state_root: Hash::zero(),
							extrinsics_root: Hash::zero(),
							digest: Default::default(),
						};

						tx.send(Ok(Some(header))).unwrap();
					}
				);

				if requested_len == 0 {
					assert_matches!(
						overseer_recv(virtual_overseer).await,
						AllMessages::ProspectiveParachains(
							ProspectiveParachainsMessage::GetMinimumRelayParents(parent, tx),
						) if parent == leaf_hash => {
							tx.send(test_state.chain_ids.iter().map(|para_id| (*para_id, min_number)).collect()).unwrap();
						}
					);
				}

				requested_len += 1;
			}
		}

		// Skip the leaf.
		for (hash, number) in ancestry_iter.skip(1).take(requested_len.saturating_sub(1)) {
			if Some(number) == test_state.last_known_block {
				break;
			}
			assert_construct_per_relay_parent(
				virtual_overseer,
				test_state,
				hash,
				number,
				&mut next_overseer_message,
			)
			.await;
		}
	}

	test_state.last_known_block = last_block_from_view;

	next_overseer_message
}

async fn send_seconded_statement(
	virtual_overseer: &mut VirtualOverseer,
	keystore: KeystorePtr,
	candidate: &CommittedCandidateReceipt,
) {
	let signing_context = SigningContext { session_index: 0, parent_hash: Hash::zero() };
	let stmt = SignedFullStatement::sign(
		&keystore,
		Statement::Seconded(candidate.clone()),
		&signing_context,
		ValidatorIndex(0),
		&ValidatorId::from(Sr25519Keyring::Alice.public()),
	)
	.ok()
	.flatten()
	.expect("should be signed");

	overseer_send(
		virtual_overseer,
		CollatorProtocolMessage::Seconded(candidate.descriptor.relay_parent(), stmt),
	)
	.await;
}

async fn assert_collation_seconded(
	virtual_overseer: &mut VirtualOverseer,
	relay_parent: Hash,
	peer_id: PeerId,
	version: CollationVersion,
) {
	assert_matches!(
		overseer_recv(virtual_overseer).await,
		AllMessages::NetworkBridgeTx(NetworkBridgeTxMessage::ReportPeer(
			ReportPeerMessage::Single(peer, rep)
		)) => {
			assert_eq!(peer_id, peer);
			assert_eq!(rep.value, BENEFIT_NOTIFY_GOOD.cost_or_benefit());
		}
	);

	match version {
		CollationVersion::V1 => {
			assert_matches!(
				overseer_recv(virtual_overseer).await,
				AllMessages::NetworkBridgeTx(NetworkBridgeTxMessage::SendCollationMessage(
					peers,
					CollationProtocols::V1(protocol_v1::CollationProtocol::CollatorProtocol(
						protocol_v1::CollatorProtocolMessage::CollationSeconded(
							_relay_parent,
							..,
						),
					)),
				)) => {
					assert_eq!(peers, vec![peer_id]);
					assert_eq!(relay_parent, _relay_parent);
				}
			);
		},
		CollationVersion::V2 => {
			assert_matches!(
				overseer_recv(virtual_overseer).await,
				AllMessages::NetworkBridgeTx(NetworkBridgeTxMessage::SendCollationMessage(
					peers,
					CollationProtocols::V2(protocol_v2::CollationProtocol::CollatorProtocol(
						protocol_v2::CollatorProtocolMessage::CollationSeconded(
							_relay_parent,
							..,
						),
					)),
				)) => {
					assert_eq!(peers, vec![peer_id]);
					assert_eq!(relay_parent, _relay_parent);
				}
			);
		},
	}
}

/// Assert that the next message is a persisted validation data request and respond with the
/// supplied PVD.
async fn assert_persisted_validation_data(
	virtual_overseer: &mut VirtualOverseer,
	version: CollationVersion,
	expected_relay_parent: Hash,
	expected_para_id: ParaId,
	expected_parent_head_data_hash: Option<Hash>,
	pvd: Option<PersistedValidationData>,
) {
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
				tx.send(Ok(pvd)).unwrap();
			}
		),
		CollationVersion::V2 => assert_matches!(
			msg,
			AllMessages::ProspectiveParachains(
				ProspectiveParachainsMessage::GetProspectiveValidationData(request, tx),
			) => {
				assert_eq!(expected_relay_parent, request.candidate_relay_parent);
				assert_eq!(expected_para_id, request.para_id);
				if let Some(expected_parent_head_data_hash) = expected_parent_head_data_hash {
					assert_eq!(expected_parent_head_data_hash, request.parent_head_data.hash());
				}
				tx.send(pvd).unwrap();
			}
		),
	}
}

// Combines dummy candidate creation, advertisement and fetching in a single call
async fn submit_second_and_assert(
	virtual_overseer: &mut VirtualOverseer,
	keystore: KeystorePtr,
	para_id: ParaId,
	relay_parent: Hash,
	collator: PeerId,
	candidate_head_data: HeadData,
) {
	let (candidate, commitments) =
		create_dummy_candidate_and_commitments(para_id, candidate_head_data, relay_parent);

	let candidate_hash = candidate.hash();
	let parent_head_data_hash = Hash::zero();

	assert_advertise_collation(
		virtual_overseer,
		collator,
		relay_parent,
		para_id,
		(candidate_hash, parent_head_data_hash),
	)
	.await;

	let response_channel = assert_fetch_collation_request(
		virtual_overseer,
		relay_parent,
		para_id,
		Some(candidate_hash),
	)
	.await;

	let pov = PoV { block_data: BlockData(vec![1]) };

	send_collation_and_assert_processing(
		virtual_overseer,
		keystore,
		relay_parent,
		para_id,
		collator,
		response_channel,
		candidate,
		commitments,
		pov,
	)
	.await;
}

fn create_dummy_candidate_and_commitments(
	para_id: ParaId,
	candidate_head_data: HeadData,
	relay_parent: Hash,
) -> (CandidateReceipt, CandidateCommitments) {
	let mut candidate = dummy_candidate_receipt_bad_sig(relay_parent, Some(Default::default()));
	candidate.descriptor.para_id = para_id;
	candidate.descriptor.persisted_validation_data_hash = dummy_pvd().hash();
	let commitments = CandidateCommitments {
		head_data: candidate_head_data,
		horizontal_messages: Default::default(),
		upward_messages: Default::default(),
		new_validation_code: None,
		processed_downward_messages: 0,
		hrmp_watermark: 0,
	};
	candidate.commitments_hash = commitments.hash();

	(candidate.into(), commitments)
}

async fn assert_advertise_collation(
	virtual_overseer: &mut VirtualOverseer,
	peer: PeerId,
	relay_parent: Hash,
	expected_para_id: ParaId,
	candidate: (CandidateHash, Hash),
) {
	advertise_collation(virtual_overseer, peer, relay_parent, Some(candidate)).await;
	assert_matches!(
		overseer_recv(virtual_overseer).await,
		AllMessages::CandidateBacking(
			CandidateBackingMessage::CanSecond(request, tx),
		) => {
			assert_eq!(request.candidate_hash, candidate.0);
			assert_eq!(request.candidate_para_id, expected_para_id);
			assert_eq!(request.parent_head_data_hash, candidate.1);
			tx.send(true).expect("receiving side should be alive");
		}
	);
}

async fn send_collation_and_assert_processing(
	virtual_overseer: &mut VirtualOverseer,
	keystore: KeystorePtr,
	relay_parent: Hash,
	expected_para_id: ParaId,
	expected_peer_id: PeerId,
	response_channel: ResponseSender,
	candidate: CandidateReceipt,
	commitments: CandidateCommitments,
	pov: PoV,
) {
	response_channel
		.send(Ok((
			request_v2::CollationFetchingResponse::Collation(candidate.clone(), pov.clone())
				.encode(),
			ProtocolName::from(""),
		)))
		.expect("Sending response should succeed");

	assert_candidate_backing_second(
		virtual_overseer,
		relay_parent,
		expected_para_id,
		&pov,
		CollationVersion::V2,
	)
	.await;

	let candidate = CommittedCandidateReceipt { descriptor: candidate.descriptor, commitments };

	send_seconded_statement(virtual_overseer, keystore.clone(), &candidate).await;

	assert_collation_seconded(
		virtual_overseer,
		relay_parent,
		expected_peer_id,
		CollationVersion::V2,
	)
	.await;
}

#[test]
fn v1_advertisement_accepted_and_seconded() {
	let mut test_state = TestState::default();

	test_harness(ReputationAggregator::new(|_| true), |test_harness| async move {
		let TestHarness { mut virtual_overseer, keystore } = test_harness;

		let pair_a = CollatorPair::generate().0;

		let head_b = Hash::from_low_u64_be(128);
		let head_b_num: u32 = 0;

		update_view(&mut virtual_overseer, &mut test_state, vec![(head_b, head_b_num)]).await;

		let peer_a = PeerId::random();

		// Accept both collators from the implicit view.
		connect_and_declare_collator(
			&mut virtual_overseer,
			peer_a,
			pair_a.clone(),
			test_state.chain_ids[0],
			CollationVersion::V1,
		)
		.await;

		advertise_collation(&mut virtual_overseer, peer_a, head_b, None).await;

		let response_channel = assert_fetch_collation_request(
			&mut virtual_overseer,
			head_b,
			test_state.chain_ids[0],
			None,
		)
		.await;

		let mut candidate = dummy_candidate_receipt_bad_sig(head_b, Some(Default::default()));
		candidate.descriptor.para_id = test_state.chain_ids[0];
		candidate.descriptor.persisted_validation_data_hash = dummy_pvd().hash();
		let commitments = CandidateCommitments {
			head_data: HeadData(vec![1u8]),
			horizontal_messages: Default::default(),
			upward_messages: Default::default(),
			new_validation_code: None,
			processed_downward_messages: 0,
			hrmp_watermark: 0,
		};
		candidate.commitments_hash = commitments.hash();
		let candidate: CandidateReceipt = candidate.into();
		let pov = PoV { block_data: BlockData(vec![1]) };

		response_channel
			.send(Ok((
				request_v2::CollationFetchingResponse::Collation(candidate.clone(), pov.clone())
					.encode(),
				ProtocolName::from(""),
			)))
			.expect("Sending response should succeed");

		assert_candidate_backing_second(
			&mut virtual_overseer,
			head_b,
			test_state.chain_ids[0],
			&pov,
			CollationVersion::V1,
		)
		.await;

		let candidate = CommittedCandidateReceipt { descriptor: candidate.descriptor, commitments };

		send_seconded_statement(&mut virtual_overseer, keystore.clone(), &candidate).await;

		assert_collation_seconded(&mut virtual_overseer, head_b, peer_a, CollationVersion::V1)
			.await;

		virtual_overseer
	});
}

#[test]
fn v1_advertisement_rejected_on_non_active_leaf() {
	let mut test_state = TestState::default();

	test_harness(ReputationAggregator::new(|_| true), |test_harness| async move {
		let TestHarness { mut virtual_overseer, .. } = test_harness;

		let pair_a = CollatorPair::generate().0;

		let head_b = Hash::from_low_u64_be(128);
		let head_b_num: u32 = 5;

		update_view(&mut virtual_overseer, &mut test_state, vec![(head_b, head_b_num)]).await;

		let peer_a = PeerId::random();

		// Accept both collators from the implicit view.
		connect_and_declare_collator(
			&mut virtual_overseer,
			peer_a,
			pair_a.clone(),
			test_state.chain_ids[0],
			CollationVersion::V1,
		)
		.await;

		advertise_collation(&mut virtual_overseer, peer_a, get_parent_hash(head_b), None).await;

		assert_matches!(
			overseer_recv(&mut virtual_overseer).await,
			AllMessages::NetworkBridgeTx(
				NetworkBridgeTxMessage::ReportPeer(ReportPeerMessage::Single(peer, rep)),
			) => {
				assert_eq!(peer, peer_a);
				assert_eq!(rep.value, COST_PROTOCOL_MISUSE.cost_or_benefit());
			}
		);

		virtual_overseer
	});
}

#[test]
fn accept_advertisements_from_implicit_view() {
	let mut test_state = TestState::default();

	test_harness(ReputationAggregator::new(|_| true), |test_harness| async move {
		let TestHarness { mut virtual_overseer, .. } = test_harness;

		let pair_a = CollatorPair::generate().0;
		let pair_b = CollatorPair::generate().0;

		let head_b = Hash::from_low_u64_be(128);
		let head_b_num: u32 = 2;

		let head_c = get_parent_hash(head_b);
		// Grandparent of head `b`.
		// Group rotation frequency is 1 by default, at `d` we're assigned
		// to the first para.
		let head_d = get_parent_hash(head_c);

		// Activated leaf is `b`, but the collation will be based on `c`.
		update_view(&mut virtual_overseer, &mut test_state, vec![(head_b, head_b_num)]).await;

		let peer_a = PeerId::random();
		let peer_b = PeerId::random();

		// Accept both collators from the implicit view.
		connect_and_declare_collator(
			&mut virtual_overseer,
			peer_a,
			pair_a.clone(),
			test_state.chain_ids[0],
			CollationVersion::V2,
		)
		.await;
		connect_and_declare_collator(
			&mut virtual_overseer,
			peer_b,
			pair_b.clone(),
			test_state.chain_ids[1],
			CollationVersion::V2,
		)
		.await;

		let candidate_hash = CandidateHash::default();
		let parent_head_data_hash = Hash::zero();
		advertise_collation(
			&mut virtual_overseer,
			peer_b,
			head_c,
			Some((candidate_hash, parent_head_data_hash)),
		)
		.await;
		assert_matches!(
			overseer_recv(&mut virtual_overseer).await,
			AllMessages::CandidateBacking(
				CandidateBackingMessage::CanSecond(request, tx),
			) => {
				assert_eq!(request.candidate_hash, candidate_hash);
				assert_eq!(request.candidate_para_id, test_state.chain_ids[1]);
				assert_eq!(request.parent_head_data_hash, parent_head_data_hash);
				tx.send(true).expect("receiving side should be alive");
			}
		);

		assert_fetch_collation_request(
			&mut virtual_overseer,
			head_c,
			test_state.chain_ids[1],
			Some(candidate_hash),
		)
		.await;
		// Advertise with different para.
		advertise_collation(
			&mut virtual_overseer,
			peer_a,
			head_d, // Note different relay parent.
			Some((candidate_hash, parent_head_data_hash)),
		)
		.await;
		assert_matches!(
			overseer_recv(&mut virtual_overseer).await,
			AllMessages::CandidateBacking(
				CandidateBackingMessage::CanSecond(request, tx),
			) => {
				assert_eq!(request.candidate_hash, candidate_hash);
				assert_eq!(request.candidate_para_id, test_state.chain_ids[0]);
				assert_eq!(request.parent_head_data_hash, parent_head_data_hash);
				tx.send(true).expect("receiving side should be alive");
			}
		);

		assert_fetch_collation_request(
			&mut virtual_overseer,
			head_d,
			test_state.chain_ids[0],
			Some(candidate_hash),
		)
		.await;

		virtual_overseer
	});
}

#[test]
fn second_multiple_candidates_per_relay_parent() {
	let mut test_state = TestState::with_one_scheduled_para();

	test_harness(ReputationAggregator::new(|_| true), |test_harness| async move {
		let TestHarness { mut virtual_overseer, keystore } = test_harness;

		let pair = CollatorPair::generate().0;

		let head_a = Hash::from_low_u64_be(130);
		let head_a_num: u32 = 0;

		let head_b = Hash::from_low_u64_be(128);
		let head_b_num: u32 = 2;

		// Activated leaf is `a` and `b`.The collation will be based on `b`.
		update_view(
			&mut virtual_overseer,
			&mut test_state,
			vec![(head_a, head_a_num), (head_b, head_b_num)],
		)
		.await;

		let peer_a = PeerId::random();

		connect_and_declare_collator(
			&mut virtual_overseer,
			peer_a,
			pair.clone(),
			test_state.chain_ids[0],
			CollationVersion::V2,
		)
		.await;

		for i in 0..test_state.scheduling_lookahead {
			submit_second_and_assert(
				&mut virtual_overseer,
				keystore.clone(),
				test_state.chain_ids[0],
				head_a,
				peer_a,
				HeadData(vec![i as u8]),
			)
			.await;
		}

		// No more advertisements can be made for this relay parent.
		let candidate_hash = CandidateHash(Hash::repeat_byte(0xAA));
		advertise_collation(
			&mut virtual_overseer,
			peer_a,
			head_a,
			Some((candidate_hash, Hash::zero())),
		)
		.await;

		// Rejected but not reported because reached the limit of advertisements for the para_id
		test_helpers::Yield::new().await;
		assert_matches!(virtual_overseer.recv().now_or_never(), None);

		// By different peer too (not reported).
		let pair_b = CollatorPair::generate().0;
		let peer_b = PeerId::random();

		connect_and_declare_collator(
			&mut virtual_overseer,
			peer_b,
			pair_b.clone(),
			test_state.chain_ids[0],
			CollationVersion::V2,
		)
		.await;

		let candidate_hash = CandidateHash(Hash::repeat_byte(0xFF));
		advertise_collation(
			&mut virtual_overseer,
			peer_b,
			head_a,
			Some((candidate_hash, Hash::zero())),
		)
		.await;

		test_helpers::Yield::new().await;
		assert_matches!(virtual_overseer.recv().now_or_never(), None);

		virtual_overseer
	});
}

#[test]
fn fetched_collation_sanity_check() {
	let mut test_state = TestState::default();

	test_harness(ReputationAggregator::new(|_| true), |test_harness| async move {
		let TestHarness { mut virtual_overseer, .. } = test_harness;

		let pair = CollatorPair::generate().0;

		// Grandparent of head `a`.
		let head_b = Hash::from_low_u64_be(128);
		let head_b_num: u32 = 2;

		// Grandparent of head `b`.
		// Group rotation frequency is 1 by default, at `c` we're assigned
		// to the first para.
		let head_c = Hash::from_low_u64_be(130);

		// Activated leaf is `b`, but the collation will be based on `c`.
		update_view(&mut virtual_overseer, &mut test_state, vec![(head_b, head_b_num)]).await;

		let peer_a = PeerId::random();

		connect_and_declare_collator(
			&mut virtual_overseer,
			peer_a,
			pair.clone(),
			test_state.chain_ids[0],
			CollationVersion::V2,
		)
		.await;

		let mut candidate = dummy_candidate_receipt_bad_sig(head_c, Some(Default::default()));
		candidate.descriptor.para_id = test_state.chain_ids[0];
		let commitments = CandidateCommitments {
			head_data: HeadData(vec![1, 2, 3]),
			horizontal_messages: Default::default(),
			upward_messages: Default::default(),
			new_validation_code: None,
			processed_downward_messages: 0,
			hrmp_watermark: 0,
		};
		candidate.commitments_hash = commitments.hash();
		let candidate: CandidateReceipt = candidate.into();
		let candidate_hash = CandidateHash(Hash::zero());
		let parent_head_data_hash = Hash::zero();

		advertise_collation(
			&mut virtual_overseer,
			peer_a,
			head_c,
			Some((candidate_hash, parent_head_data_hash)),
		)
		.await;
		assert_matches!(
			overseer_recv(&mut virtual_overseer).await,
			AllMessages::CandidateBacking(
				CandidateBackingMessage::CanSecond(request, tx),
			) => {
				assert_eq!(request.candidate_hash, candidate_hash);
				assert_eq!(request.candidate_para_id, test_state.chain_ids[0]);
				assert_eq!(request.parent_head_data_hash, parent_head_data_hash);
				tx.send(true).expect("receiving side should be alive");
			}
		);

		let response_channel = assert_fetch_collation_request(
			&mut virtual_overseer,
			head_c,
			test_state.chain_ids[0],
			Some(candidate_hash),
		)
		.await;

		let pov = PoV { block_data: BlockData(vec![1]) };

		response_channel
			.send(Ok((
				request_v2::CollationFetchingResponse::Collation(candidate.clone(), pov.clone())
					.encode(),
				ProtocolName::from(""),
			)))
			.expect("Sending response should succeed");

		// PVD request.
		assert_matches!(
			overseer_recv(&mut virtual_overseer).await,
			AllMessages::ProspectiveParachains(
				ProspectiveParachainsMessage::GetProspectiveValidationData(request, tx),
			) => {
				assert_eq!(head_c, request.candidate_relay_parent);
				assert_eq!(test_state.chain_ids[0], request.para_id);
				tx.send(Some(dummy_pvd())).unwrap();
			}
		);

		// Reported malicious.
		assert_matches!(
			overseer_recv(&mut virtual_overseer).await,
			AllMessages::NetworkBridgeTx(
				NetworkBridgeTxMessage::ReportPeer(ReportPeerMessage::Single(peer_id, rep)),
			) => {
				assert_eq!(peer_a, peer_id);
				assert_eq!(rep.value, COST_REPORT_BAD.cost_or_benefit());
			}
		);

		virtual_overseer
	});
}

#[test]
fn sanity_check_invalid_parent_head_data() {
	let mut test_state = TestState::default();

	test_harness(ReputationAggregator::new(|_| true), |test_harness| async move {
		let TestHarness { mut virtual_overseer, .. } = test_harness;

		let pair = CollatorPair::generate().0;

		let head_c = Hash::from_low_u64_be(130);
		let head_c_num = 3;

		update_view(&mut virtual_overseer, &mut test_state, vec![(head_c, head_c_num)]).await;

		let peer_a = PeerId::random();

		connect_and_declare_collator(
			&mut virtual_overseer,
			peer_a,
			pair.clone(),
			test_state.chain_ids[0],
			CollationVersion::V2,
		)
		.await;

		let mut candidate = dummy_candidate_receipt_bad_sig(head_c, Some(Default::default()));
		candidate.descriptor.para_id = test_state.chain_ids[0];
		let commitments = CandidateCommitments {
			head_data: HeadData(vec![1, 2, 3]),
			horizontal_messages: Default::default(),
			upward_messages: Default::default(),
			new_validation_code: None,
			processed_downward_messages: 0,
			hrmp_watermark: 0,
		};
		candidate.commitments_hash = commitments.hash();

		let parent_head_data = HeadData(vec![4, 2, 0]);
		let parent_head_data_hash = parent_head_data.hash();
		let wrong_parent_head_data = HeadData(vec![4, 2]);

		let mut pvd = dummy_pvd();
		pvd.parent_head = parent_head_data;

		candidate.descriptor.persisted_validation_data_hash = pvd.hash();
		let candidate: CandidateReceipt = candidate.into();

		let candidate_hash = candidate.hash();

		advertise_collation(
			&mut virtual_overseer,
			peer_a,
			head_c,
			Some((candidate_hash, parent_head_data_hash)),
		)
		.await;
		assert_matches!(
			overseer_recv(&mut virtual_overseer).await,
			AllMessages::CandidateBacking(
				CandidateBackingMessage::CanSecond(request, tx),
			) => {
				assert_eq!(request.candidate_hash, candidate_hash);
				assert_eq!(request.candidate_para_id, test_state.chain_ids[0]);
				assert_eq!(request.parent_head_data_hash, parent_head_data_hash);
				tx.send(true).expect("receiving side should be alive");
			}
		);

		let response_channel = assert_fetch_collation_request(
			&mut virtual_overseer,
			head_c,
			test_state.chain_ids[0],
			Some(candidate_hash),
		)
		.await;

		let pov = PoV { block_data: BlockData(vec![1]) };

		response_channel
			.send(Ok((
				request_v2::CollationFetchingResponse::CollationWithParentHeadData {
					receipt: candidate.clone(),
					pov: pov.clone(),
					parent_head_data: wrong_parent_head_data,
				}
				.encode(),
				ProtocolName::from(""),
			)))
			.expect("Sending response should succeed");

		// PVD request.
		assert_matches!(
			overseer_recv(&mut virtual_overseer).await,
			AllMessages::ProspectiveParachains(
				ProspectiveParachainsMessage::GetProspectiveValidationData(request, tx),
			) => {
				assert_eq!(head_c, request.candidate_relay_parent);
				assert_eq!(test_state.chain_ids[0], request.para_id);
				tx.send(Some(pvd)).unwrap();
			}
		);

		// Reported malicious.
		assert_matches!(
			overseer_recv(&mut virtual_overseer).await,
			AllMessages::NetworkBridgeTx(
				NetworkBridgeTxMessage::ReportPeer(ReportPeerMessage::Single(peer_id, rep)),
			) => {
				assert_eq!(peer_a, peer_id);
				assert_eq!(rep.value, COST_REPORT_BAD.cost_or_benefit());
			}
		);

		test_helpers::Yield::new().await;
		assert_matches!(virtual_overseer.recv().now_or_never(), None);

		virtual_overseer
	});
}

#[test]
fn advertisement_spam_protection() {
	let mut test_state = TestState::default();

	test_harness(ReputationAggregator::new(|_| true), |test_harness| async move {
		let TestHarness { mut virtual_overseer, .. } = test_harness;

		let pair_a = CollatorPair::generate().0;

		let head_b = Hash::from_low_u64_be(128);
		let head_b_num: u32 = 2;

		let head_c = get_parent_hash(head_b);

		// Activated leaf is `b`, but the collation will be based on `c`.
		update_view(&mut virtual_overseer, &mut test_state, vec![(head_b, head_b_num)]).await;

		let peer_a = PeerId::random();
		connect_and_declare_collator(
			&mut virtual_overseer,
			peer_a,
			pair_a.clone(),
			test_state.chain_ids[1],
			CollationVersion::V2,
		)
		.await;

		let candidate_hash = CandidateHash::default();
		let parent_head_data_hash = Hash::zero();
		advertise_collation(
			&mut virtual_overseer,
			peer_a,
			head_c,
			Some((candidate_hash, parent_head_data_hash)),
		)
		.await;
		assert_matches!(
			overseer_recv(&mut virtual_overseer).await,
			AllMessages::CandidateBacking(
				CandidateBackingMessage::CanSecond(request, tx),
			) => {
				assert_eq!(request.candidate_hash, candidate_hash);
				assert_eq!(request.candidate_para_id, test_state.chain_ids[1]);
				assert_eq!(request.parent_head_data_hash, parent_head_data_hash);
				// Reject it.
				tx.send(false).expect("receiving side should be alive");
			}
		);

		// Send the same advertisement again.
		advertise_collation(
			&mut virtual_overseer,
			peer_a,
			head_c,
			Some((candidate_hash, parent_head_data_hash)),
		)
		.await;
		// Reported.
		assert_matches!(
			overseer_recv(&mut virtual_overseer).await,
			AllMessages::NetworkBridgeTx(
				NetworkBridgeTxMessage::ReportPeer(ReportPeerMessage::Single(peer_id, rep)),
			) => {
				assert_eq!(peer_a, peer_id);
				assert_eq!(rep.value, COST_UNEXPECTED_MESSAGE.cost_or_benefit());
			}
		);

		virtual_overseer
	});
}

#[rstest]
#[case(true)]
#[case(false)]
fn child_blocked_from_seconding_by_parent(#[case] valid_parent: bool) {
	let mut test_state = TestState::with_one_scheduled_para();

	test_harness(ReputationAggregator::new(|_| true), |test_harness| async move {
		let TestHarness { mut virtual_overseer, keystore } = test_harness;

		let pair = CollatorPair::generate().0;

		// Grandparent of head `a`.
		let head_b = Hash::from_low_u64_be(128);
		let head_b_num: u32 = 2;

		// Grandparent of head `b`.
		// Group rotation frequency is 1 by default, at `c` we're assigned
		// to the first para.
		let head_c = Hash::from_low_u64_be(130);

		// Activated leaf is `b`, but the collation will be based on `c`.
		update_view(&mut virtual_overseer, &mut test_state, vec![(head_b, head_b_num)]).await;

		let peer_a = PeerId::random();

		connect_and_declare_collator(
			&mut virtual_overseer,
			peer_a,
			pair.clone(),
			test_state.chain_ids[0],
			CollationVersion::V2,
		)
		.await;

		// Candidate A transitions from head data 0 to 1.
		// Candidate B transitions from head data 1 to 2.

		// Candidate B is advertised and fetched before candidate A.

		let mut candidate_b = dummy_candidate_receipt_bad_sig(head_c, Some(Default::default()));
		candidate_b.descriptor.para_id = test_state.chain_ids[0];
		candidate_b.descriptor.para_head = HeadData(vec![2]).hash();
		candidate_b.descriptor.persisted_validation_data_hash =
			PersistedValidationData::<Hash, BlockNumber> {
				parent_head: HeadData(vec![1]),
				relay_parent_number: 5,
				max_pov_size: 1024,
				relay_parent_storage_root: Default::default(),
			}
			.hash();
		let candidate_b_commitments = CandidateCommitments {
			head_data: HeadData(vec![2]),
			horizontal_messages: Default::default(),
			upward_messages: Default::default(),
			new_validation_code: None,
			processed_downward_messages: 0,
			hrmp_watermark: 0,
		};
		let mut candidate_b: CandidateReceipt = candidate_b.into();
		candidate_b.commitments_hash = candidate_b_commitments.hash();

		let candidate_b_hash = candidate_b.hash();

		advertise_collation(
			&mut virtual_overseer,
			peer_a,
			head_c,
			Some((candidate_b_hash, HeadData(vec![1]).hash())),
		)
		.await;
		assert_matches!(
			overseer_recv(&mut virtual_overseer).await,
			AllMessages::CandidateBacking(
				CandidateBackingMessage::CanSecond(request, tx),
			) => {
				assert_eq!(request.candidate_hash, candidate_b_hash);
				assert_eq!(request.candidate_para_id, test_state.chain_ids[0]);
				assert_eq!(request.parent_head_data_hash, HeadData(vec![1]).hash());
				tx.send(true).expect("receiving side should be alive");
			}
		);

		let response_channel = assert_fetch_collation_request(
			&mut virtual_overseer,
			head_c,
			test_state.chain_ids[0],
			Some(candidate_b_hash),
		)
		.await;

		response_channel
			.send(Ok((
				request_v2::CollationFetchingResponse::Collation(
					candidate_b.clone(),
					PoV { block_data: BlockData(vec![1]) },
				)
				.encode(),
				ProtocolName::from(""),
			)))
			.expect("Sending response should succeed");

		// Persisted validation data of candidate B is not found.
		assert_persisted_validation_data(
			&mut virtual_overseer,
			CollationVersion::V2,
			head_c,
			test_state.chain_ids[0],
			Some(HeadData(vec![1]).hash()),
			None,
		)
		.await;

		// Now advertise, fetch and validate candidate A, which is the parent of B.

		let mut candidate_a = dummy_candidate_receipt_bad_sig(head_c, Some(Default::default()));
		candidate_a.descriptor.para_id = test_state.chain_ids[0];
		candidate_a.descriptor.para_head = HeadData(vec![1]).hash();
		candidate_a.descriptor.persisted_validation_data_hash =
			PersistedValidationData::<Hash, BlockNumber> {
				parent_head: HeadData(vec![0]),
				relay_parent_number: 5,
				max_pov_size: 1024,
				relay_parent_storage_root: Default::default(),
			}
			.hash();
		let mut candidate_a: CandidateReceipt = candidate_a.into();
		let candidate_a_commitments = CandidateCommitments {
			head_data: HeadData(vec![1]),
			horizontal_messages: Default::default(),
			upward_messages: Default::default(),
			new_validation_code: None,
			processed_downward_messages: 0,
			hrmp_watermark: 0,
		};
		candidate_a.commitments_hash = candidate_a_commitments.hash();

		let candidate_a: CandidateReceipt = candidate_a.into();
		let candidate_a_hash = candidate_a.hash();

		advertise_collation(
			&mut virtual_overseer,
			peer_a,
			head_c,
			Some((candidate_a_hash, HeadData(vec![0]).hash())),
		)
		.await;
		assert_matches!(
			overseer_recv(&mut virtual_overseer).await,
			AllMessages::CandidateBacking(
				CandidateBackingMessage::CanSecond(request, tx),
			) => {
				assert_eq!(request.candidate_hash, candidate_a_hash);
				assert_eq!(request.candidate_para_id, test_state.chain_ids[0]);
				assert_eq!(request.parent_head_data_hash, HeadData(vec![0]).hash());
				tx.send(true).expect("receiving side should be alive");
			}
		);

		let response_channel = assert_fetch_collation_request(
			&mut virtual_overseer,
			head_c,
			test_state.chain_ids[0],
			Some(candidate_a_hash),
		)
		.await;

		response_channel
			.send(Ok((
				request_v2::CollationFetchingResponse::Collation(
					candidate_a.clone(),
					PoV { block_data: BlockData(vec![2]) },
				)
				.encode(),
				ProtocolName::from(""),
			)))
			.expect("Sending response should succeed");

		assert_persisted_validation_data(
			&mut virtual_overseer,
			CollationVersion::V2,
			head_c,
			test_state.chain_ids[0],
			Some(HeadData(vec![0]).hash()),
			Some(PersistedValidationData::<Hash, BlockNumber> {
				parent_head: HeadData(vec![0]),
				relay_parent_number: 5,
				max_pov_size: 1024,
				relay_parent_storage_root: Default::default(),
			}),
		)
		.await;

		assert_matches!(
			overseer_recv(&mut virtual_overseer).await,
			AllMessages::CandidateBacking(CandidateBackingMessage::Second(
				relay_parent,
				candidate_receipt,
				received_pvd,
				incoming_pov,
			)) => {
				assert_eq!(head_c, relay_parent);
				assert_eq!(test_state.chain_ids[0], candidate_receipt.descriptor.para_id());
				assert_eq!(PoV { block_data: BlockData(vec![2]) }, incoming_pov);
				assert_eq!(PersistedValidationData::<Hash, BlockNumber> {
					parent_head: HeadData(vec![0]),
					relay_parent_number: 5,
					max_pov_size: 1024,
					relay_parent_storage_root: Default::default(),
				}, received_pvd);
				candidate_receipt
			}
		);

		// If candidate A is valid, proceed with seconding B.
		if valid_parent {
			send_seconded_statement(
				&mut virtual_overseer,
				keystore.clone(),
				&CommittedCandidateReceipt {
					descriptor: candidate_a.descriptor,
					commitments: candidate_a_commitments,
				},
			)
			.await;

			assert_collation_seconded(&mut virtual_overseer, head_c, peer_a, CollationVersion::V2)
				.await;

			// Now that candidate A has been seconded, candidate B can be seconded as well.

			assert_persisted_validation_data(
				&mut virtual_overseer,
				CollationVersion::V2,
				head_c,
				test_state.chain_ids[0],
				Some(HeadData(vec![1]).hash()),
				Some(PersistedValidationData::<Hash, BlockNumber> {
					parent_head: HeadData(vec![1]),
					relay_parent_number: 5,
					max_pov_size: 1024,
					relay_parent_storage_root: Default::default(),
				}),
			)
			.await;

			assert_matches!(
				overseer_recv(&mut virtual_overseer).await,
				AllMessages::CandidateBacking(CandidateBackingMessage::Second(
					relay_parent,
					candidate_receipt,
					received_pvd,
					incoming_pov,
				)) => {
					assert_eq!(head_c, relay_parent);
					assert_eq!(test_state.chain_ids[0], candidate_receipt.descriptor.para_id());
					assert_eq!(PoV { block_data: BlockData(vec![1]) }, incoming_pov);
					assert_eq!(PersistedValidationData::<Hash, BlockNumber> {
						parent_head: HeadData(vec![1]),
						relay_parent_number: 5,
						max_pov_size: 1024,
						relay_parent_storage_root: Default::default(),
					}, received_pvd);
					candidate_receipt
				}
			);

			send_seconded_statement(
				&mut virtual_overseer,
				keystore.clone(),
				&CommittedCandidateReceipt {
					descriptor: candidate_b.descriptor,
					commitments: candidate_b_commitments,
				},
			)
			.await;

			assert_collation_seconded(&mut virtual_overseer, head_c, peer_a, CollationVersion::V2)
				.await;
		} else {
			// If candidate A is invalid, B won't be seconded.
			overseer_send(
				&mut virtual_overseer,
				CollatorProtocolMessage::Invalid(head_c, candidate_a),
			)
			.await;

			assert_matches!(
				overseer_recv(&mut virtual_overseer).await,
				AllMessages::NetworkBridgeTx(
					NetworkBridgeTxMessage::ReportPeer(ReportPeerMessage::Single(peer, rep)),
				) => {
					assert_eq!(peer, peer_a);
					assert_eq!(rep.value, COST_REPORT_BAD.cost_or_benefit());
				}
			);
		}

		test_helpers::Yield::new().await;
		assert_matches!(virtual_overseer.recv().now_or_never(), None);

		virtual_overseer
	});
}

#[rstest]
#[case(true)]
#[case(false)]
fn v2_descriptor(#[case] v2_feature_enabled: bool) {
	let mut test_state = TestState::default();

	if !v2_feature_enabled {
		test_state.node_features = NodeFeatures::EMPTY;
	}

	test_harness(ReputationAggregator::new(|_| true), |test_harness| async move {
		let TestHarness { mut virtual_overseer, keystore } = test_harness;

		let pair_a = CollatorPair::generate().0;

		let head_b = Hash::from_low_u64_be(128);
		let head_b_num: u32 = 0;

		update_view(&mut virtual_overseer, &mut test_state, vec![(head_b, head_b_num)]).await;

		let peer_a = PeerId::random();

		connect_and_declare_collator(
			&mut virtual_overseer,
			peer_a,
			pair_a.clone(),
			test_state.chain_ids[0],
			CollationVersion::V2,
		)
		.await;

		let mut committed_candidate = dummy_committed_candidate_receipt_v2(head_b);
		committed_candidate.descriptor.set_para_id(test_state.chain_ids[0]);
		committed_candidate
			.descriptor
			.set_persisted_validation_data_hash(dummy_pvd().hash());
		// First para is assigned to core 0.
		committed_candidate.descriptor.set_core_index(CoreIndex(0));
		committed_candidate.descriptor.set_session_index(test_state.session_index);

		let candidate: CandidateReceipt = committed_candidate.clone().to_plain();
		let pov = PoV { block_data: BlockData(vec![1]) };

		let candidate_hash = candidate.hash();
		let parent_head_data_hash = Hash::zero();

		advertise_collation(
			&mut virtual_overseer,
			peer_a,
			head_b,
			Some((candidate_hash, parent_head_data_hash)),
		)
		.await;

		assert_matches!(
			overseer_recv(&mut virtual_overseer).await,
			AllMessages::CandidateBacking(
				CandidateBackingMessage::CanSecond(request, tx),
			) => {
				assert_eq!(request.candidate_hash, candidate_hash);
				assert_eq!(request.candidate_para_id, test_state.chain_ids[0]);
				assert_eq!(request.parent_head_data_hash, parent_head_data_hash);
				tx.send(true).expect("receiving side should be alive");
			}
		);

		let response_channel = assert_fetch_collation_request(
			&mut virtual_overseer,
			head_b,
			test_state.chain_ids[0],
			Some(candidate_hash),
		)
		.await;

		response_channel
			.send(Ok((
				request_v2::CollationFetchingResponse::Collation(candidate.clone(), pov.clone())
					.encode(),
				ProtocolName::from(""),
			)))
			.expect("Sending response should succeed");

		if v2_feature_enabled {
			assert_candidate_backing_second(
				&mut virtual_overseer,
				head_b,
				test_state.chain_ids[0],
				&pov,
				CollationVersion::V2,
			)
			.await;

			send_seconded_statement(&mut virtual_overseer, keystore.clone(), &committed_candidate)
				.await;

			assert_collation_seconded(&mut virtual_overseer, head_b, peer_a, CollationVersion::V2)
				.await;
		} else {
			// Reported malicious. Used v2 descriptor without the feature being enabled
			assert_matches!(
				overseer_recv(&mut virtual_overseer).await,
				AllMessages::NetworkBridgeTx(
					NetworkBridgeTxMessage::ReportPeer(ReportPeerMessage::Single(peer_id, rep)),
				) => {
					assert_eq!(peer_a, peer_id);
					assert_eq!(rep.value, COST_REPORT_BAD.cost_or_benefit());
				}
			);
		}

		virtual_overseer
	});
}

#[test]
fn invalid_v2_descriptor() {
	let mut test_state = TestState::default();

	test_harness(ReputationAggregator::new(|_| true), |test_harness| async move {
		let TestHarness { mut virtual_overseer, .. } = test_harness;

		let pair_a = CollatorPair::generate().0;

		let head_b = Hash::from_low_u64_be(128);
		let head_b_num: u32 = 0;

		update_view(&mut virtual_overseer, &mut test_state, vec![(head_b, head_b_num)]).await;

		let peer_a = PeerId::random();

		connect_and_declare_collator(
			&mut virtual_overseer,
			peer_a,
			pair_a.clone(),
			test_state.chain_ids[0],
			CollationVersion::V2,
		)
		.await;

		let mut candidates = vec![];

		let mut committed_candidate = dummy_committed_candidate_receipt_v2(head_b);
		committed_candidate.descriptor.set_para_id(test_state.chain_ids[0]);
		committed_candidate
			.descriptor
			.set_persisted_validation_data_hash(dummy_pvd().hash());
		// First para is assigned to core 0, set an invalid core index.
		committed_candidate.descriptor.set_core_index(CoreIndex(10));
		committed_candidate.descriptor.set_session_index(test_state.session_index);

		candidates.push(committed_candidate.clone());

		// Invalid session index.
		committed_candidate.descriptor.set_core_index(CoreIndex(0));
		committed_candidate.descriptor.set_session_index(10);

		candidates.push(committed_candidate);

		for committed_candidate in candidates {
			let candidate: CandidateReceipt = committed_candidate.clone().to_plain();
			let pov = PoV { block_data: BlockData(vec![1]) };

			let candidate_hash = candidate.hash();
			let parent_head_data_hash = Hash::zero();

			advertise_collation(
				&mut virtual_overseer,
				peer_a,
				head_b,
				Some((candidate_hash, parent_head_data_hash)),
			)
			.await;

			assert_matches!(
				overseer_recv(&mut virtual_overseer).await,
				AllMessages::CandidateBacking(
					CandidateBackingMessage::CanSecond(request, tx),
				) => {
					assert_eq!(request.candidate_hash, candidate_hash);
					assert_eq!(request.candidate_para_id, test_state.chain_ids[0]);
					assert_eq!(request.parent_head_data_hash, parent_head_data_hash);
					tx.send(true).expect("receiving side should be alive");
				}
			);

			let response_channel = assert_fetch_collation_request(
				&mut virtual_overseer,
				head_b,
				test_state.chain_ids[0],
				Some(candidate_hash),
			)
			.await;

			response_channel
				.send(Ok((
					request_v2::CollationFetchingResponse::Collation(
						candidate.clone(),
						pov.clone(),
					)
					.encode(),
					ProtocolName::from(""),
				)))
				.expect("Sending response should succeed");

			// Reported malicious. Invalid core index
			assert_matches!(
				overseer_recv(&mut virtual_overseer).await,
				AllMessages::NetworkBridgeTx(
					NetworkBridgeTxMessage::ReportPeer(ReportPeerMessage::Single(peer_id, rep)),
				) => {
					assert_eq!(peer_a, peer_id);
					assert_eq!(rep.value, COST_REPORT_BAD.cost_or_benefit());
				}
			);
		}

		virtual_overseer
	});
}

#[test]
fn fair_collation_fetches() {
	let mut test_state = TestState::with_shared_core();

	test_harness(ReputationAggregator::new(|_| true), |test_harness| async move {
		let TestHarness { mut virtual_overseer, keystore } = test_harness;

		let head_b = Hash::from_low_u64_be(128);
		let head_b_num: u32 = 2;

		update_view(&mut virtual_overseer, &mut test_state, vec![(head_b, head_b_num)]).await;

		let peer_a = PeerId::random();
		let pair_a = CollatorPair::generate().0;

		connect_and_declare_collator(
			&mut virtual_overseer,
			peer_a,
			pair_a.clone(),
			test_state.chain_ids[0],
			CollationVersion::V2,
		)
		.await;

		let peer_b = PeerId::random();
		let pair_b = CollatorPair::generate().0;

		connect_and_declare_collator(
			&mut virtual_overseer,
			peer_b,
			pair_b.clone(),
			test_state.chain_ids[1],
			CollationVersion::V2,
		)
		.await;

		// `peer_a` sends two advertisements (its claim queue limit)
		for i in 0..2u8 {
			submit_second_and_assert(
				&mut virtual_overseer,
				keystore.clone(),
				ParaId::from(test_state.chain_ids[0]),
				head_b,
				peer_a,
				HeadData(vec![i]),
			)
			.await;
		}

		// `peer_a` sends another advertisement and it is ignored
		let candidate_hash = CandidateHash(Hash::repeat_byte(0xAA));
		advertise_collation(
			&mut virtual_overseer,
			peer_a,
			head_b,
			Some((candidate_hash, Hash::zero())),
		)
		.await;
		test_helpers::Yield::new().await;
		assert_matches!(virtual_overseer.recv().now_or_never(), None);

		// `peer_b` should still be able to advertise its collation
		submit_second_and_assert(
			&mut virtual_overseer,
			keystore.clone(),
			ParaId::from(test_state.chain_ids[1]),
			head_b,
			peer_b,
			HeadData(vec![0u8]),
		)
		.await;

		// And no more advertisements can be made for this relay parent.

		// verify for peer_a
		let candidate_hash = CandidateHash(Hash::repeat_byte(0xBB));
		advertise_collation(
			&mut virtual_overseer,
			peer_a,
			head_b,
			Some((candidate_hash, Hash::zero())),
		)
		.await;
		test_helpers::Yield::new().await;
		assert_matches!(virtual_overseer.recv().now_or_never(), None);

		// verify for peer_b
		let candidate_hash = CandidateHash(Hash::repeat_byte(0xCC));
		advertise_collation(
			&mut virtual_overseer,
			peer_b,
			head_b,
			Some((candidate_hash, Hash::zero())),
		)
		.await;
		test_helpers::Yield::new().await;
		assert_matches!(virtual_overseer.recv().now_or_never(), None);

		virtual_overseer
	});
}

#[test]
fn collation_fetching_prefer_entries_earlier_in_claim_queue() {
	let mut test_state = TestState::with_shared_core();

	test_harness(ReputationAggregator::new(|_| true), |test_harness| async move {
		let TestHarness { mut virtual_overseer, keystore } = test_harness;

		let pair_a = CollatorPair::generate().0;
		let collator_a = PeerId::random();
		let para_id_a = test_state.chain_ids[0];

		let pair_b = CollatorPair::generate().0;
		let collator_b = PeerId::random();
		let para_id_b = test_state.chain_ids[1];

		let head = Hash::from_low_u64_be(128);
		let head_num: u32 = 2;

		update_view(&mut virtual_overseer, &mut test_state, vec![(head, head_num)]).await;

		connect_and_declare_collator(
			&mut virtual_overseer,
			collator_a,
			pair_a.clone(),
			para_id_a,
			CollationVersion::V2,
		)
		.await;

		connect_and_declare_collator(
			&mut virtual_overseer,
			collator_b,
			pair_b.clone(),
			para_id_b,
			CollationVersion::V2,
		)
		.await;

		let (candidate_a1, commitments_a1) =
			create_dummy_candidate_and_commitments(para_id_a, HeadData(vec![0u8]), head);
		let (candidate_b1, commitments_b1) =
			create_dummy_candidate_and_commitments(para_id_b, HeadData(vec![1u8]), head);
		let (candidate_a2, commitments_a2) =
			create_dummy_candidate_and_commitments(para_id_a, HeadData(vec![2u8]), head);
		let (candidate_a3, _) =
			create_dummy_candidate_and_commitments(para_id_a, HeadData(vec![3u8]), head);
		let parent_head_data_a1 = HeadData(vec![0u8]);
		let parent_head_data_b1 = HeadData(vec![1u8]);
		let parent_head_data_a2 = HeadData(vec![2u8]);
		let parent_head_data_a3 = HeadData(vec![3u8]);

		// advertise a collation for `para_id_a` but don't send the collation. This will be a
		// pending fetch.
		assert_advertise_collation(
			&mut virtual_overseer,
			collator_a,
			head,
			para_id_a,
			(candidate_a1.hash(), parent_head_data_a1.hash()),
		)
		.await;

		let response_channel_a1 = assert_fetch_collation_request(
			&mut virtual_overseer,
			head,
			para_id_a,
			Some(candidate_a1.hash()),
		)
		.await;

		// advertise another collation for `para_id_a`. This one should be fetched last.
		assert_advertise_collation(
			&mut virtual_overseer,
			collator_a,
			head,
			para_id_a,
			(candidate_a2.hash(), parent_head_data_a2.hash()),
		)
		.await;

		// There is a pending collation so nothing should be fetched
		test_helpers::Yield::new().await;
		assert_matches!(virtual_overseer.recv().now_or_never(), None);

		// Advertise a collation for `para_id_b`. This should be fetched second
		assert_advertise_collation(
			&mut virtual_overseer,
			collator_b,
			head,
			para_id_b,
			(candidate_b1.hash(), parent_head_data_b1.hash()),
		)
		.await;

		// Again - no fetch because of the pending collation
		test_helpers::Yield::new().await;
		assert_matches!(virtual_overseer.recv().now_or_never(), None);

		//Now send a response for the first fetch and examine the second fetch
		send_collation_and_assert_processing(
			&mut virtual_overseer,
			keystore.clone(),
			head,
			para_id_a,
			collator_a,
			response_channel_a1,
			candidate_a1,
			commitments_a1,
			PoV { block_data: BlockData(vec![1]) },
		)
		.await;

		// The next fetch should be for `para_id_b`
		let response_channel_b = assert_fetch_collation_request(
			&mut virtual_overseer,
			head,
			para_id_b,
			Some(candidate_b1.hash()),
		)
		.await;

		send_collation_and_assert_processing(
			&mut virtual_overseer,
			keystore.clone(),
			head,
			para_id_b,
			collator_b,
			response_channel_b,
			candidate_b1,
			commitments_b1,
			PoV { block_data: BlockData(vec![2]) },
		)
		.await;

		// and the final one for `para_id_a`
		let response_channel_a2 = assert_fetch_collation_request(
			&mut virtual_overseer,
			head,
			para_id_a,
			Some(candidate_a2.hash()),
		)
		.await;

		// Advertise another collation for `para_id_a`. This should be rejected as there is no slot
		// in the claim queue for it. One is fetched and one is pending.
		advertise_collation(
			&mut virtual_overseer,
			collator_a,
			head,
			Some((candidate_a3.hash(), parent_head_data_a3.hash())),
		)
		.await;

		// `CanSecond` shouldn't be sent as the advertisement should be ignored
		test_helpers::Yield::new().await;
		assert_matches!(virtual_overseer.recv().now_or_never(), None);

		// Fetch the pending collation
		send_collation_and_assert_processing(
			&mut virtual_overseer,
			keystore.clone(),
			head,
			para_id_a,
			collator_a,
			response_channel_a2,
			candidate_a2,
			commitments_a2,
			PoV { block_data: BlockData(vec![3]) },
		)
		.await;

		virtual_overseer
	});
}

#[test]
fn collation_fetching_considers_advertisements_from_the_whole_view() {
	let mut test_state = TestState::with_shared_core();

	test_harness(ReputationAggregator::new(|_| true), |test_harness| async move {
		let TestHarness { mut virtual_overseer, keystore } = test_harness;

		let pair_a = CollatorPair::generate().0;
		let collator_a = PeerId::random();
		let para_id_a = test_state.chain_ids[0];

		let pair_b = CollatorPair::generate().0;
		let collator_b = PeerId::random();
		let para_id_b = test_state.chain_ids[1];

		let relay_parent_2 = Hash::from_low_u64_be(test_state.relay_parent.to_low_u64_be() - 1);

		assert_eq!(
			*test_state.claim_queue.get(&CoreIndex(0)).unwrap(),
			VecDeque::from([para_id_b, para_id_a, para_id_a])
		);

		update_view(&mut virtual_overseer, &mut test_state, vec![(relay_parent_2, 2)]).await;

		connect_and_declare_collator(
			&mut virtual_overseer,
			collator_a,
			pair_a.clone(),
			para_id_a,
			CollationVersion::V2,
		)
		.await;

		connect_and_declare_collator(
			&mut virtual_overseer,
			collator_b,
			pair_b.clone(),
			para_id_b,
			CollationVersion::V2,
		)
		.await;

		submit_second_and_assert(
			&mut virtual_overseer,
			keystore.clone(),
			para_id_a,
			relay_parent_2,
			collator_a,
			HeadData(vec![0u8]),
		)
		.await;

		submit_second_and_assert(
			&mut virtual_overseer,
			keystore.clone(),
			para_id_b,
			relay_parent_2,
			collator_b,
			HeadData(vec![1u8]),
		)
		.await;

		let relay_parent_3 = Hash::from_low_u64_be(relay_parent_2.to_low_u64_be() - 1);
		*test_state.claim_queue.get_mut(&CoreIndex(0)).unwrap() =
			VecDeque::from([para_id_a, para_id_a, para_id_b]);
		update_view(&mut virtual_overseer, &mut test_state, vec![(relay_parent_3, 3)]).await;

		submit_second_and_assert(
			&mut virtual_overseer,
			keystore.clone(),
			para_id_b,
			relay_parent_3,
			collator_b,
			HeadData(vec![3u8]),
		)
		.await;
		submit_second_and_assert(
			&mut virtual_overseer,
			keystore.clone(),
			para_id_a,
			relay_parent_3,
			collator_a,
			HeadData(vec![3u8]),
		)
		.await;

		// At this point the claim queue is satisfied and any advertisement at `relay_parent_4`
		// must be ignored

		let (candidate_a, _) =
			create_dummy_candidate_and_commitments(para_id_a, HeadData(vec![5u8]), relay_parent_3);
		let parent_head_data_a = HeadData(vec![5u8]);

		advertise_collation(
			&mut virtual_overseer,
			collator_a,
			relay_parent_3,
			Some((candidate_a.hash(), parent_head_data_a.hash())),
		)
		.await;

		test_helpers::Yield::new().await;
		assert_matches!(virtual_overseer.recv().now_or_never(), None);

		let (candidate_b, _) =
			create_dummy_candidate_and_commitments(para_id_b, HeadData(vec![6u8]), relay_parent_3);
		let parent_head_data_b = HeadData(vec![6u8]);

		advertise_collation(
			&mut virtual_overseer,
			collator_b,
			relay_parent_3,
			Some((candidate_b.hash(), parent_head_data_b.hash())),
		)
		.await;

		// `CanSecond` shouldn't be sent as the advertisement should be ignored
		test_helpers::Yield::new().await;
		assert_matches!(virtual_overseer.recv().now_or_never(), None);

		// At `relay_parent_6` the advertisement for `para_id_b` falls out of the view so a new one
		// can be accepted
		let relay_parent_6 = Hash::from_low_u64_be(relay_parent_3.to_low_u64_be() - 2);
		update_view(&mut virtual_overseer, &mut test_state, vec![(relay_parent_6, 6)]).await;

		submit_second_and_assert(
			&mut virtual_overseer,
			keystore.clone(),
			para_id_a,
			relay_parent_6,
			collator_a,
			HeadData(vec![3u8]),
		)
		.await;

		virtual_overseer
	});
}

#[test]
fn collation_fetching_fairness_handles_old_claims() {
	let mut test_state = TestState::with_shared_core();

	test_harness(ReputationAggregator::new(|_| true), |test_harness| async move {
		let TestHarness { mut virtual_overseer, keystore } = test_harness;

		let pair_a = CollatorPair::generate().0;
		let collator_a = PeerId::random();
		let para_id_a = test_state.chain_ids[0];

		let pair_b = CollatorPair::generate().0;
		let collator_b = PeerId::random();
		let para_id_b = test_state.chain_ids[1];

		let relay_parent_2 = Hash::from_low_u64_be(test_state.relay_parent.to_low_u64_be() - 1);

		*test_state.claim_queue.get_mut(&CoreIndex(0)).unwrap() =
			VecDeque::from([para_id_a, para_id_b, para_id_a]);

		update_view(&mut virtual_overseer, &mut test_state, vec![(relay_parent_2, 2)]).await;

		connect_and_declare_collator(
			&mut virtual_overseer,
			collator_a,
			pair_a.clone(),
			para_id_a,
			CollationVersion::V2,
		)
		.await;

		connect_and_declare_collator(
			&mut virtual_overseer,
			collator_b,
			pair_b.clone(),
			para_id_b,
			CollationVersion::V2,
		)
		.await;

		submit_second_and_assert(
			&mut virtual_overseer,
			keystore.clone(),
			para_id_a,
			relay_parent_2,
			collator_a,
			HeadData(vec![0u8]),
		)
		.await;

		submit_second_and_assert(
			&mut virtual_overseer,
			keystore.clone(),
			para_id_b,
			relay_parent_2,
			collator_b,
			HeadData(vec![1u8]),
		)
		.await;

		submit_second_and_assert(
			&mut virtual_overseer,
			keystore.clone(),
			para_id_a,
			relay_parent_2,
			collator_a,
			HeadData(vec![2u8]),
		)
		.await;

		let relay_parent_3 = Hash::from_low_u64_be(relay_parent_2.to_low_u64_be() - 1);

		*test_state.claim_queue.get_mut(&CoreIndex(0)).unwrap() =
			VecDeque::from([para_id_b, para_id_a, para_id_b]);
		update_view(&mut virtual_overseer, &mut test_state, vec![(relay_parent_3, 3)]).await;

		// nothing is advertised here
		test_helpers::Yield::new().await;
		assert_matches!(virtual_overseer.recv().now_or_never(), None);

		let relay_parent_4 = Hash::from_low_u64_be(relay_parent_3.to_low_u64_be() - 1);

		*test_state.claim_queue.get_mut(&CoreIndex(0)).unwrap() =
			VecDeque::from([para_id_a, para_id_b, para_id_a]);
		update_view(&mut virtual_overseer, &mut test_state, vec![(relay_parent_4, 4)]).await;

		submit_second_and_assert(
			&mut virtual_overseer,
			keystore.clone(),
			para_id_b,
			relay_parent_4,
			collator_b,
			HeadData(vec![3u8]),
		)
		.await;

		submit_second_and_assert(
			&mut virtual_overseer,
			keystore.clone(),
			para_id_a,
			relay_parent_4,
			collator_a,
			HeadData(vec![4u8]),
		)
		.await;

		// At this point the claim queue is satisfied and any advertisement at `relay_parent_4`
		// must be ignored

		// Advertisement for `para_id_a` at `relay_parent_4` which must be ignored
		let (candidate_a, _) =
			create_dummy_candidate_and_commitments(para_id_a, HeadData(vec![5u8]), relay_parent_4);
		let parent_head_data_a = HeadData(vec![5u8]);

		advertise_collation(
			&mut virtual_overseer,
			collator_a,
			relay_parent_4,
			Some((candidate_a.hash(), parent_head_data_a.hash())),
		)
		.await;

		test_helpers::Yield::new().await;
		assert_matches!(virtual_overseer.recv().now_or_never(), None);

		// Advertisement for `para_id_b` at `relay_parent_4` which must be ignored
		let (candidate_b, _) =
			create_dummy_candidate_and_commitments(para_id_b, HeadData(vec![6u8]), relay_parent_4);
		let parent_head_data_b = HeadData(vec![6u8]);

		advertise_collation(
			&mut virtual_overseer,
			collator_b,
			relay_parent_4,
			Some((candidate_b.hash(), parent_head_data_b.hash())),
		)
		.await;

		// `CanSecond` shouldn't be sent as the advertisement should be ignored
		test_helpers::Yield::new().await;
		assert_matches!(virtual_overseer.recv().now_or_never(), None);

		virtual_overseer
	});
}

#[test]
fn claims_below_are_counted_correctly() {
	let mut test_state = TestState::with_one_scheduled_para();

	// Shorten the claim queue to make the test smaller
	let mut claim_queue = BTreeMap::new();
	claim_queue.insert(
		CoreIndex(0),
		VecDeque::from_iter(
			[ParaId::from(test_state.chain_ids[0]), ParaId::from(test_state.chain_ids[0])]
				.into_iter(),
		),
	);
	test_state.claim_queue = claim_queue;
	test_state.scheduling_lookahead = 2;

	test_harness(ReputationAggregator::new(|_| true), |test_harness| async move {
		let TestHarness { mut virtual_overseer, keystore } = test_harness;

		let hash_a = Hash::from_low_u64_be(test_state.relay_parent.to_low_u64_be() - 1);
		let hash_b = Hash::from_low_u64_be(hash_a.to_low_u64_be() - 1);
		let hash_c = Hash::from_low_u64_be(hash_b.to_low_u64_be() - 1);

		let pair_a = CollatorPair::generate().0;
		let collator_a = PeerId::random();
		let para_id_a = test_state.chain_ids[0];

		update_view(&mut virtual_overseer, &mut test_state, vec![(hash_c, 2)]).await;

		connect_and_declare_collator(
			&mut virtual_overseer,
			collator_a,
			pair_a.clone(),
			para_id_a,
			CollationVersion::V2,
		)
		.await;

		// A collation at hash_a claims the spot at hash_a
		submit_second_and_assert(
			&mut virtual_overseer,
			keystore.clone(),
			ParaId::from(test_state.chain_ids[0]),
			hash_a,
			collator_a,
			HeadData(vec![0u8]),
		)
		.await;

		// Another collation at hash_a claims the spot at hash_b
		submit_second_and_assert(
			&mut virtual_overseer,
			keystore.clone(),
			ParaId::from(test_state.chain_ids[0]),
			hash_a,
			collator_a,
			HeadData(vec![1u8]),
		)
		.await;

		// Collation at hash_c claims its own spot
		submit_second_and_assert(
			&mut virtual_overseer,
			keystore.clone(),
			ParaId::from(test_state.chain_ids[0]),
			hash_c,
			collator_a,
			HeadData(vec![2u8]),
		)
		.await;

		// Collation at hash_b should be ignored because the claim queue is satisfied
		let (ignored_candidate, _) =
			create_dummy_candidate_and_commitments(para_id_a, HeadData(vec![3u8]), hash_b);

		advertise_collation(
			&mut virtual_overseer,
			collator_a,
			hash_b,
			Some((ignored_candidate.hash(), Hash::random())),
		)
		.await;

		test_helpers::Yield::new().await;
		assert_matches!(virtual_overseer.recv().now_or_never(), None);

		virtual_overseer
	});
}

#[test]
fn claims_above_are_counted_correctly() {
	let mut test_state = TestState::with_one_scheduled_para();

	// Shorten the claim queue to make the test smaller
	let mut claim_queue = BTreeMap::new();
	claim_queue.insert(
		CoreIndex(0),
		VecDeque::from_iter(
			[ParaId::from(test_state.chain_ids[0]), ParaId::from(test_state.chain_ids[0])]
				.into_iter(),
		),
	);
	test_state.claim_queue = claim_queue;
	test_state.scheduling_lookahead = 2;

	test_harness(ReputationAggregator::new(|_| true), |test_harness| async move {
		let TestHarness { mut virtual_overseer, keystore } = test_harness;

		let hash_a = Hash::from_low_u64_be(test_state.relay_parent.to_low_u64_be() - 1); // block 0
		let hash_b = Hash::from_low_u64_be(hash_a.to_low_u64_be() - 1); // block 1
		let hash_c = Hash::from_low_u64_be(hash_b.to_low_u64_be() - 1); // block 2

		let pair_a = CollatorPair::generate().0;
		let collator_a = PeerId::random();
		let para_id_a = test_state.chain_ids[0];

		update_view(&mut virtual_overseer, &mut test_state, vec![(hash_c, 2)]).await;

		connect_and_declare_collator(
			&mut virtual_overseer,
			collator_a,
			pair_a.clone(),
			para_id_a,
			CollationVersion::V2,
		)
		.await;

		// A collation at hash_b claims the spot at hash_b
		submit_second_and_assert(
			&mut virtual_overseer,
			keystore.clone(),
			ParaId::from(test_state.chain_ids[0]),
			hash_b,
			collator_a,
			HeadData(vec![0u8]),
		)
		.await;

		// Another collation at hash_b claims the spot at hash_c
		submit_second_and_assert(
			&mut virtual_overseer,
			keystore.clone(),
			ParaId::from(test_state.chain_ids[0]),
			hash_b,
			collator_a,
			HeadData(vec![1u8]),
		)
		.await;

		// Collation at hash_a claims its own spot
		submit_second_and_assert(
			&mut virtual_overseer,
			keystore.clone(),
			ParaId::from(test_state.chain_ids[0]),
			hash_a,
			collator_a,
			HeadData(vec![0u8]),
		)
		.await;

		// Another Collation at hash_a should be ignored because the claim queue is satisfied
		let (ignored_candidate, _) =
			create_dummy_candidate_and_commitments(para_id_a, HeadData(vec![2u8]), hash_a);

		advertise_collation(
			&mut virtual_overseer,
			collator_a,
			hash_a,
			Some((ignored_candidate.hash(), Hash::random())),
		)
		.await;

		test_helpers::Yield::new().await;
		assert_matches!(virtual_overseer.recv().now_or_never(), None);

		// Same for hash_b
		let (ignored_candidate, _) =
			create_dummy_candidate_and_commitments(para_id_a, HeadData(vec![3u8]), hash_b);

		advertise_collation(
			&mut virtual_overseer,
			collator_a,
			hash_b,
			Some((ignored_candidate.hash(), Hash::random())),
		)
		.await;

		test_helpers::Yield::new().await;
		assert_matches!(virtual_overseer.recv().now_or_never(), None);

		virtual_overseer
	});
}

#[test]
fn claim_fills_last_free_slot() {
	let mut test_state = TestState::with_one_scheduled_para();

	// Shorten the claim queue to make the test smaller
	let mut claim_queue = BTreeMap::new();
	claim_queue.insert(
		CoreIndex(0),
		VecDeque::from_iter(
			[ParaId::from(test_state.chain_ids[0]), ParaId::from(test_state.chain_ids[0])]
				.into_iter(),
		),
	);
	test_state.claim_queue = claim_queue;
	test_state.scheduling_lookahead = 2;

	test_harness(ReputationAggregator::new(|_| true), |test_harness| async move {
		let TestHarness { mut virtual_overseer, keystore } = test_harness;

		let hash_a = Hash::from_low_u64_be(test_state.relay_parent.to_low_u64_be() - 1); // block 0
		let hash_b = Hash::from_low_u64_be(hash_a.to_low_u64_be() - 1); // block 1
		let hash_c = Hash::from_low_u64_be(hash_b.to_low_u64_be() - 1); // block 2

		let pair_a = CollatorPair::generate().0;
		let collator_a = PeerId::random();
		let para_id_a = test_state.chain_ids[0];

		update_view(&mut virtual_overseer, &mut test_state, vec![(hash_c, 2)]).await;

		connect_and_declare_collator(
			&mut virtual_overseer,
			collator_a,
			pair_a.clone(),
			para_id_a,
			CollationVersion::V2,
		)
		.await;

		// A collation at hash_a claims its spot
		submit_second_and_assert(
			&mut virtual_overseer,
			keystore.clone(),
			ParaId::from(test_state.chain_ids[0]),
			hash_a,
			collator_a,
			HeadData(vec![0u8]),
		)
		.await;

		// Collation at hash_b claims its own spot
		submit_second_and_assert(
			&mut virtual_overseer,
			keystore.clone(),
			ParaId::from(test_state.chain_ids[0]),
			hash_b,
			collator_a,
			HeadData(vec![3u8]),
		)
		.await;

		// Collation at hash_c claims its own spot
		submit_second_and_assert(
			&mut virtual_overseer,
			keystore.clone(),
			ParaId::from(test_state.chain_ids[0]),
			hash_c,
			collator_a,
			HeadData(vec![2u8]),
		)
		.await;

		// Another Collation at hash_a should be ignored because the claim queue is satisfied
		let (ignored_candidate, _) =
			create_dummy_candidate_and_commitments(para_id_a, HeadData(vec![3u8]), hash_a);

		advertise_collation(
			&mut virtual_overseer,
			collator_a,
			hash_a,
			Some((ignored_candidate.hash(), Hash::random())),
		)
		.await;

		test_helpers::Yield::new().await;
		assert_matches!(virtual_overseer.recv().now_or_never(), None);

		// Same for hash_b
		let (ignored_candidate, _) =
			create_dummy_candidate_and_commitments(para_id_a, HeadData(vec![4u8]), hash_b);

		advertise_collation(
			&mut virtual_overseer,
			collator_a,
			hash_b,
			Some((ignored_candidate.hash(), Hash::random())),
		)
		.await;

		test_helpers::Yield::new().await;
		assert_matches!(virtual_overseer.recv().now_or_never(), None);

		virtual_overseer
	});
}
