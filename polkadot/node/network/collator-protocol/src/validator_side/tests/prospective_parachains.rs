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
	vstaging::CommittedCandidateReceiptV2 as CommittedCandidateReceipt, AsyncBackingParams,
	BlockNumber, CandidateCommitments, Header, SigningContext, ValidatorId,
};
use rstest::rstest;

const ASYNC_BACKING_PARAMETERS: AsyncBackingParams =
	AsyncBackingParams { max_candidate_depth: 4, allowed_ancestry_len: 3 };

fn get_parent_hash(hash: Hash) -> Hash {
	Hash::from_low_u64_be(hash.to_low_u64_be() + 1)
}

async fn assert_assign_incoming(
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
		) if parent == hash => {
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
		AllMessages::RuntimeApi(
			RuntimeApiMessage::Request(parent, RuntimeApiRequest::AvailabilityCores(tx))
		) if parent == hash => {
			tx.send(Ok(test_state.cores.clone())).unwrap();
		}
	);

	assert_matches!(
		overseer_recv(virtual_overseer).await,
		AllMessages::RuntimeApi(RuntimeApiMessage::Request(
			parent,
			RuntimeApiRequest::Version(tx),
		)) if parent == hash => {
			let _ = tx.send(Ok(RuntimeApiRequest::CLAIM_QUEUE_RUNTIME_REQUIREMENT));
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
	test_state: &TestState,
	new_view: Vec<(Hash, u32)>, // Hash and block number.
	activated: u8,              // How many new heads does this update contain?
) -> Option<AllMessages> {
	let new_view: HashMap<Hash, u32> = HashMap::from_iter(new_view);

	let our_view = OurView::new(new_view.keys().map(|hash| *hash), 0);

	overseer_send(
		virtual_overseer,
		CollatorProtocolMessage::NetworkBridgeUpdate(NetworkBridgeEvent::OurViewChange(our_view)),
	)
	.await;

	let mut next_overseer_message = None;
	for _ in 0..activated {
		let (leaf_hash, leaf_number) = assert_matches!(
			overseer_recv(virtual_overseer).await,
			AllMessages::RuntimeApi(RuntimeApiMessage::Request(
				parent,
				RuntimeApiRequest::AsyncBackingParams(tx),
			)) => {
				tx.send(Ok(ASYNC_BACKING_PARAMETERS)).unwrap();
				(parent, new_view.get(&parent).copied().expect("Unknown parent requested"))
			}
		);

		assert_assign_incoming(
			virtual_overseer,
			test_state,
			leaf_hash,
			leaf_number,
			&mut next_overseer_message,
		)
		.await;

		let min_number = leaf_number.saturating_sub(ASYNC_BACKING_PARAMETERS.allowed_ancestry_len);

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
			assert_assign_incoming(
				virtual_overseer,
				test_state,
				hash,
				number,
				&mut next_overseer_message,
			)
			.await;
		}
	}
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
					Versioned::V1(protocol_v1::CollationProtocol::CollatorProtocol(
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
					Versioned::V2(protocol_v2::CollationProtocol::CollatorProtocol(
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

#[test]
fn v1_advertisement_accepted_and_seconded() {
	let test_state = TestState::default();

	test_harness(ReputationAggregator::new(|_| true), |test_harness| async move {
		let TestHarness { mut virtual_overseer, keystore } = test_harness;

		let pair_a = CollatorPair::generate().0;

		let head_b = Hash::from_low_u64_be(128);
		let head_b_num: u32 = 0;

		update_view(&mut virtual_overseer, &test_state, vec![(head_b, head_b_num)], 1).await;

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
			head_data: HeadData(vec![1 as u8]),
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
fn v1_advertisement_rejected_on_non_active_leave() {
	let test_state = TestState::default();

	test_harness(ReputationAggregator::new(|_| true), |test_harness| async move {
		let TestHarness { mut virtual_overseer, .. } = test_harness;

		let pair_a = CollatorPair::generate().0;

		let head_b = Hash::from_low_u64_be(128);
		let head_b_num: u32 = 5;

		update_view(&mut virtual_overseer, &test_state, vec![(head_b, head_b_num)], 1).await;

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
	let test_state = TestState::default();

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
		update_view(&mut virtual_overseer, &test_state, vec![(head_b, head_b_num)], 1).await;

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
	let test_state = TestState::default();

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
		update_view(&mut virtual_overseer, &test_state, vec![(head_b, head_b_num)], 1).await;

		let peer_a = PeerId::random();

		connect_and_declare_collator(
			&mut virtual_overseer,
			peer_a,
			pair.clone(),
			test_state.chain_ids[0],
			CollationVersion::V2,
		)
		.await;

		for i in 0..(ASYNC_BACKING_PARAMETERS.max_candidate_depth + 1) {
			let mut candidate = dummy_candidate_receipt_bad_sig(head_c, Some(Default::default()));
			candidate.descriptor.para_id = test_state.chain_ids[0];
			candidate.descriptor.persisted_validation_data_hash = dummy_pvd().hash();
			let commitments = CandidateCommitments {
				head_data: HeadData(vec![i as u8]),
				horizontal_messages: Default::default(),
				upward_messages: Default::default(),
				new_validation_code: None,
				processed_downward_messages: 0,
				hrmp_watermark: 0,
			};
			candidate.commitments_hash = commitments.hash();
			let candidate: CandidateReceipt = candidate.into();

			let candidate_hash = candidate.hash();
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
					request_v2::CollationFetchingResponse::Collation(
						candidate.clone(),
						pov.clone(),
					)
					.encode(),
					ProtocolName::from(""),
				)))
				.expect("Sending response should succeed");

			assert_candidate_backing_second(
				&mut virtual_overseer,
				head_c,
				test_state.chain_ids[0],
				&pov,
				CollationVersion::V2,
			)
			.await;

			let candidate =
				CommittedCandidateReceipt { descriptor: candidate.descriptor, commitments };

			send_seconded_statement(&mut virtual_overseer, keystore.clone(), &candidate).await;

			assert_collation_seconded(&mut virtual_overseer, head_c, peer_a, CollationVersion::V2)
				.await;
		}

		// No more advertisements can be made for this relay parent.
		let candidate_hash = CandidateHash(Hash::repeat_byte(0xAA));
		advertise_collation(
			&mut virtual_overseer,
			peer_a,
			head_c,
			Some((candidate_hash, Hash::zero())),
		)
		.await;

		// Reported because reached the limit of advertisements per relay parent.
		assert_matches!(
			overseer_recv(&mut virtual_overseer).await,
			AllMessages::NetworkBridgeTx(
				NetworkBridgeTxMessage::ReportPeer(ReportPeerMessage::Single(peer_id, rep)),
			) => {
				assert_eq!(peer_a, peer_id);
				assert_eq!(rep.value, COST_UNEXPECTED_MESSAGE.cost_or_benefit());
			}
		);

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
			head_c,
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
	let test_state = TestState::default();

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
		update_view(&mut virtual_overseer, &test_state, vec![(head_b, head_b_num)], 1).await;

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
	let test_state = TestState::default();

	test_harness(ReputationAggregator::new(|_| true), |test_harness| async move {
		let TestHarness { mut virtual_overseer, .. } = test_harness;

		let pair = CollatorPair::generate().0;

		let head_c = Hash::from_low_u64_be(130);
		let head_c_num = 3;

		update_view(&mut virtual_overseer, &test_state, vec![(head_c, head_c_num)], 1).await;

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
	let test_state = TestState::default();

	test_harness(ReputationAggregator::new(|_| true), |test_harness| async move {
		let TestHarness { mut virtual_overseer, .. } = test_harness;

		let pair_a = CollatorPair::generate().0;

		let head_b = Hash::from_low_u64_be(128);
		let head_b_num: u32 = 2;

		let head_c = get_parent_hash(head_b);

		// Activated leaf is `b`, but the collation will be based on `c`.
		update_view(&mut virtual_overseer, &test_state, vec![(head_b, head_b_num)], 1).await;

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
	let test_state = TestState::default();

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
		update_view(&mut virtual_overseer, &test_state, vec![(head_b, head_b_num)], 1).await;

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
