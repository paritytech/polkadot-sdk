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

use crate::validator_side_experimental::{
	common::{
		Advertisement, CollationFetchError, Score, CONNECTED_PEERS_PARA_LIMIT, FAILED_FETCH_SLASH,
		MAX_STARTUP_ANCESTRY_LOOKBACK, VALID_INCLUDED_CANDIDATE_BUMP,
	},
	peer_manager::{Backend, ReputationUpdate},
};
use rstest::rstest;

use super::*;
use assert_matches::assert_matches;
use async_trait::async_trait;
use codec::Encode;
use futures::channel::mpsc::UnboundedReceiver;
use polkadot_node_network_protocol::{
	peer_set::{CollationVersion, PeerSet},
	request_response::{
		outgoing::RequestError, v1::CollationFetchingResponse, Recipient, Requests,
	},
	OurView,
};
use polkadot_node_primitives::{
	BlockData, PoV, SignedFullStatement, Statement, UncheckedSignedFullStatement,
};
use polkadot_node_subsystem::messages::{
	AllMessages, CanSecondRequest, CandidateBackingMessage, ChainApiMessage, IfDisconnected,
	NetworkBridgeTxMessage, ParentHeadData, ProspectiveParachainsMessage,
	ProspectiveValidationDataRequest, RuntimeApiMessage, RuntimeApiRequest,
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
	BlockNumber, CoreIndex, GroupRotationInfo, Hash, HeadData, Header, Id as ParaId, NodeFeatures,
	OccupiedCoreAssumption, PersistedValidationData, SessionIndex, SigningContext, ValidatorId,
	ValidatorIndex,
};
use polkadot_primitives_test_helpers::{
	dummy_committed_candidate_receipt, dummy_committed_candidate_receipt_v2,
};
use sc_network::{OutboundFailure, RequestFailure};
use sc_network_types::multihash::Multihash;
use sp_keyring::Sr25519Keyring;
use sp_keystore::Keystore;
use std::{
	collections::{BTreeMap, BTreeSet, HashMap},
	ops::DerefMut,
	sync::{Arc, Mutex},
	time::Duration,
};

const TIMEOUT: Duration = Duration::from_millis(100);

fn peer_id(i: u8) -> PeerId {
	let data = [i; 32];

	PeerId::from_multihash(Multihash::wrap(0x0, &data).unwrap()).unwrap()
}

fn dummy_pvd() -> PersistedValidationData {
	PersistedValidationData {
		parent_head: HeadData(vec![7, 8, 9]),
		relay_parent_number: 5,
		max_pov_size: 1024,
		relay_parent_storage_root: Default::default(),
	}
}

fn dummy_pov() -> PoV {
	PoV { block_data: BlockData(vec![1]) }
}

#[derive(Clone)]
struct RelayParentInfo {
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
	keystore: KeystorePtr,
}

impl Default for TestState {
	fn default() -> Self {
		sp_tracing::init_for_tests();

		let keystore = Arc::new(sc_keystore::LocalKeystore::in_memory());
		Keystore::sr25519_generate_new(
			&*keystore,
			polkadot_primitives::PARACHAIN_KEY_TYPE_ID,
			Some(&Sr25519Keyring::Alice.to_seed()),
		)
		.expect("Insert key into keystore");

		let mut rp_info = HashMap::new();

		let cq: BTreeMap<CoreIndex, Vec<ParaId>> =
			(1..3).map(|i| (CoreIndex::from(i), vec![600.into(), 600.into()])).collect();

		rp_info.insert(
			get_hash(10),
			RelayParentInfo {
				number: 10,
				parent: get_parent_hash(10),
				session_index: 1,
				claim_queue: {
					let mut cq = cq.clone();
					cq.insert(CoreIndex(0), vec![100.into(), 200.into(), 100.into()]);
					cq
				},
				assigned_core: CoreIndex(0),
			},
		);
		rp_info.insert(
			get_hash(9),
			RelayParentInfo {
				number: 9,
				parent: get_parent_hash(9),
				session_index: 1,
				claim_queue: {
					let mut cq = cq.clone();
					cq.insert(CoreIndex(0), vec![200.into(), 100.into(), 200.into()]);
					cq
				},
				assigned_core: CoreIndex(0),
			},
		);
		rp_info.insert(
			get_hash(8),
			RelayParentInfo {
				number: 8,
				parent: get_parent_hash(8),
				session_index: 1,
				claim_queue: {
					let mut cq = cq.clone();
					cq.insert(CoreIndex(0), vec![100.into(), 200.into(), 100.into()]);
					cq
				},
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
			GroupRotationInfo { session_start_block: 0, group_rotation_frequency: 100, now: 0 };
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
			keystore,
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
		// Use a small timeout here because we expect this to be called after the future we're
		// testing resolved.
		assert!(self.recv.next().timeout(Duration::from_millis(10)).await.is_none());
	}

	async fn assert_peers_disconnected(
		&mut self,
		expected_peers: impl IntoIterator<Item = PeerId>,
	) {
		let msg = match self.buffered_msg.take() {
			Some(msg) => msg,
			None => self.timeout_recv().await,
		};
		assert_matches!(
			msg,
			AllMessages::NetworkBridgeTx(
				NetworkBridgeTxMessage::DisconnectPeers(peers, PeerSet::Collation)
			) if peers.clone().into_iter().collect::<BTreeSet<_>>() == expected_peers.into_iter().collect::<BTreeSet<_>>()
		);
	}

	async fn timeout_recv(&mut self) -> AllMessages {
		self.recv
			.next()
			.timeout(TIMEOUT)
			.await
			.expect("Receiver timed out")
			.expect("Sender dropped")
	}

	async fn handle_view_update(&mut self, active_leaves: Vec<Hash>) {
		if active_leaves.is_empty() {
			return
		}

		for active in active_leaves.iter() {
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
					assert!(active_leaves.contains(&rp));
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
						.set(FeatureIndex::CandidateReceiptV2 as usize, session_info.v2_receipts);
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

	async fn activate_leaf<B: Backend>(&mut self, state: &mut State<B>, height: BlockNumber) {
		let mut sender = self.sender.clone();
		futures::join!(self.handle_view_update(vec![get_hash(height)]), async {
			state
				.handle_our_view_change(&mut sender, OurView::new([get_hash(height)], 0))
				.await
				.unwrap()
		});
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

	async fn handle_advertisement<B: Backend>(&mut self, state: &mut State<B>, adv: Advertisement) {
		let mut sender = self.sender.clone();
		futures::join!(
			state.handle_advertisement(
				&mut sender,
				adv.peer_id,
				adv.relay_parent,
				adv.prospective_candidate
			),
			async move {
				if adv.prospective_candidate.is_some() {
					self.assert_can_second_request(adv, true).await
				}
			}
		);
	}

	async fn assert_collation_request(&mut self, adv: Advertisement) {
		let msg = match self.buffered_msg.take() {
			Some(msg) => msg,
			None => self.timeout_recv().await,
		};

		assert_matches!(
			msg,
			AllMessages::NetworkBridgeTx(NetworkBridgeTxMessage::SendRequests(
				reqs,
				IfDisconnected::ImmediateError
			)) if reqs.len() == 1 => {
				let req = reqs.into_iter().next().unwrap();

				if let Some(ProspectiveCandidate { candidate_hash, .. }) = adv.prospective_candidate {
					assert_matches!(req, Requests::CollationFetchingV2(req) => {
						assert_matches!(req.peer, Recipient::Peer(peer) if peer == adv.peer_id);
						assert!(req.fallback_request.is_none());
						assert_eq!(req.payload.relay_parent, adv.relay_parent);
						assert_eq!(req.payload.para_id, adv.para_id);
						assert_eq!(req.payload.candidate_hash, candidate_hash);
					});
				} else {
					assert_matches!(req, Requests::CollationFetchingV1(req) => {
						assert_matches!(req.peer, Recipient::Peer(peer) if peer == adv.peer_id);
						assert!(req.fallback_request.is_none());
						assert_eq!(req.payload.relay_parent, adv.relay_parent);
						assert_eq!(req.payload.para_id, adv.para_id);
					});
				}
			}
		);
	}

	async fn assert_can_second_request(&mut self, adv: Advertisement, response: bool) {
		let msg = match self.buffered_msg.take() {
			Some(msg) => msg,
			None => self.timeout_recv().await,
		};

		if let Some(prospective_candidate) = adv.prospective_candidate {
			let expected_req = CanSecondRequest {
				candidate_para_id: adv.para_id,
				candidate_relay_parent: adv.relay_parent,
				candidate_hash: prospective_candidate.candidate_hash,
				parent_head_data_hash: prospective_candidate.parent_head_data_hash,
			};

			assert_matches!(
				msg,
				AllMessages::CandidateBacking(CandidateBackingMessage::CanSecond(req, tx)) => {
					assert_eq!(
						expected_req,
						req
					);

					tx.send(response).unwrap();
				}
			);
		} else {
			panic!("Didn't expect to send CanSecond request for protocol v1 {:?}", msg);
		}
	}

	async fn assert_pvd_request(
		&mut self,
		adv: Advertisement,
		pvd: Option<PersistedValidationData>,
	) {
		let msg = match self.buffered_msg.take() {
			Some(msg) => msg,
			None => self.timeout_recv().await,
		};

		if let Some(ProspectiveCandidate { parent_head_data_hash, .. }) = adv.prospective_candidate
		{
			assert_matches!(
				msg,
				AllMessages::ProspectiveParachains(
					ProspectiveParachainsMessage::GetProspectiveValidationData(ProspectiveValidationDataRequest {
						para_id,
						candidate_relay_parent,
						parent_head_data
					}, tx)
				) => {
					assert_eq!(para_id, adv.para_id);
					assert_eq!(candidate_relay_parent, adv.relay_parent);

					assert!(
						matches!(
							parent_head_data,
							ParentHeadData::OnlyHash(head_data_hash) if head_data_hash == parent_head_data_hash
						) ||
						matches!(
							parent_head_data,
							ParentHeadData::WithData {head_data, ..} if head_data == pvd.as_ref().unwrap().parent_head
						)
					);

					tx.send(pvd).unwrap();
				}
			);
		} else {
			assert_matches!(
				msg,
				AllMessages::RuntimeApi(RuntimeApiMessage::Request(
					rp,
					RuntimeApiRequest::PersistedValidationData(
						para_id,
						OccupiedCoreAssumption::Free,
						tx
					)
				)) => {
					assert_eq!(para_id, adv.para_id);
					assert_eq!(rp, adv.relay_parent);
					tx.send(Ok(pvd)).unwrap();
				}
			);
		}
	}

	async fn assert_seconding_kickoff(
		&mut self,
		expected_receipt: CandidateReceipt,
		expected_pvd: PersistedValidationData,
		expected_pov: PoV,
	) {
		let msg = match self.buffered_msg.take() {
			Some(msg) => msg,
			None => self.timeout_recv().await,
		};

		assert_matches!(
			msg,
			AllMessages::CandidateBacking(
				CandidateBackingMessage::Second(rp, receipt, pvd, pov)
			) => {
				assert_eq!(rp, receipt.descriptor.relay_parent());
				assert_eq!(receipt, expected_receipt);
				assert_eq!(pvd, expected_pvd);
				assert_eq!(pov, expected_pov);
			}
		);
	}

	async fn handle_fetched_collation<B: Backend>(
		&mut self,
		state: &mut State<B>,
		adv: Advertisement,
		receipt: CandidateReceipt,
	) {
		let mut sender = self.sender.clone();

		futures::join!(
			state.handle_fetched_collation(
				&mut sender,
				(adv, Ok(CollationFetchingResponse::Collation(receipt.clone(), dummy_pov())))
			),
			self.assert_pvd_request(adv, Some(dummy_pvd()))
		);

		self.assert_seconding_kickoff(receipt, dummy_pvd(), dummy_pov()).await;
	}

	async fn assert_collation_seconded_notification(
		&mut self,
		peer_id: PeerId,
		version: CollationVersion,
		statement: UncheckedSignedFullStatement,
	) {
		let msg = match self.buffered_msg.take() {
			Some(msg) => msg,
			None => self.timeout_recv().await,
		};

		assert_matches!(
			msg,
			AllMessages::NetworkBridgeTx(
				NetworkBridgeTxMessage::SendCollationMessage(peers, msg)
			) => {
				assert_eq!(peers, vec![peer_id]);

				match version {
					CollationVersion::V1 => {
						assert_matches!(
							msg,
							CollationProtocols::V1(protocol_v1::CollationProtocol::CollatorProtocol(
								protocol_v1::CollatorProtocolMessage::CollationSeconded(
									_rp,
									stmt
								),
							)) => {
								assert_eq!(statement, stmt);
							}
						);
					},
					CollationVersion::V2 => {
						assert_matches!(
							msg,
							CollationProtocols::V2(protocol_v2::CollationProtocol::CollatorProtocol(
								protocol_v2::CollatorProtocolMessage::CollationSeconded(
									_rp,
									stmt
								),
							)) => {
								assert_eq!(statement, stmt);
							}
						);
					}
				};
			}
		);
	}

	async fn second_collation<B: Backend>(
		&mut self,
		state: &mut State<B>,
		peer_id: PeerId,
		version: CollationVersion,
		ccr: CommittedCandidateReceipt,
	) {
		let signing_context =
			SigningContext { parent_hash: ccr.descriptor.relay_parent(), session_index: 1 };

		let statement = SignedFullStatement::sign(
			&self.keystore,
			Statement::Seconded(ccr),
			&signing_context,
			ValidatorIndex(0),
			&ValidatorId::from(Sr25519Keyring::Alice.public()),
		)
		.ok()
		.flatten()
		.expect("should be signed");

		state.handle_collation_seconded(&mut self.sender, statement.clone()).await;
		self.assert_collation_seconded_notification(peer_id, version, statement.into())
			.await;
	}
}

fn get_parent_hash(number: u32) -> Hash {
	get_hash(number - 1)
}

fn get_hash(number: u32) -> Hash {
	Hash::from_low_u64_be(number as u64)
}

async fn make_state<B: Backend>(
	db: B,
	test_state: &mut TestState,
	initial_leaf_hash: Hash,
) -> State<B> {
	let initial_leaf_number = test_state.rp_info.get(&initial_leaf_hash).unwrap().number;

	let keystore = test_state.keystore.clone();

	let mut sender = test_state.sender.clone();

	let responder = async move {
		test_state.handle_view_update(vec![initial_leaf_hash]).await;

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

		let peer_manager = PeerManager::startup(db, &mut sender, collation_manager.assignments())
			.await
			.unwrap();

		State::new(peer_manager, collation_manager, Metrics)
	};

	let (state, ..) = futures::join!(initializer, responder);

	state
}

#[derive(Clone, Default)]
struct MockDb {
	finalized: Arc<Mutex<BlockNumber>>,
	// Use BTreeMaps to ensure ordering when asserting.
	witnessed_bumps: Arc<Mutex<BTreeMap<ParaId, BTreeMap<PeerId, Score>>>>,
	witnessed_slash: Arc<Mutex<Option<(PeerId, ParaId, Score)>>>,
}

impl MockDb {
	fn witnessed_bumps(&self) -> BTreeMap<ParaId, BTreeMap<PeerId, Score>> {
		std::mem::take(self.witnessed_bumps.lock().unwrap().deref_mut())
	}

	fn witnessed_slash(&self) -> Option<(PeerId, ParaId, Score)> {
		std::mem::take(self.witnessed_slash.lock().unwrap().deref_mut())
	}
}

#[async_trait]
impl Backend for MockDb {
	async fn processed_finalized_block_number(&self) -> Option<BlockNumber> {
		Some(*(self.finalized.lock().unwrap()))
	}

	async fn query(&self, _peer_id: &PeerId, _para_id: &ParaId) -> Option<Score> {
		None
	}

	async fn slash(&mut self, peer_id: &PeerId, para_id: &ParaId, value: Score) {
		let old_slash = std::mem::replace(
			self.witnessed_slash.lock().unwrap().deref_mut(),
			Some((*peer_id, *para_id, value)),
		);

		assert!(old_slash.is_none());
	}

	async fn prune_paras(&mut self, _registered_paras: BTreeSet<ParaId>) {}

	async fn process_bumps(
		&mut self,
		leaf_number: BlockNumber,
		bumps: BTreeMap<ParaId, HashMap<PeerId, Score>>,
		_decay_value: Option<Score>,
	) -> Vec<ReputationUpdate> {
		let old_bumps = std::mem::replace(
			self.witnessed_bumps.lock().unwrap().deref_mut(),
			bumps.into_iter().map(|(k, v)| (k, v.into_iter().collect())).collect(),
		);

		assert!(old_bumps.is_empty());

		*(self.finalized.lock().unwrap()) = leaf_number;

		vec![]
	}

	async fn max_scores_for_paras(&self, _paras: BTreeSet<ParaId>) -> HashMap<ParaId, Score> {
		HashMap::new()
	}
}

impl Drop for MockDb {
	fn drop(&mut self) {
		assert!(self.witnessed_bumps().is_empty());
		assert!(self.witnessed_slash().is_none());
	}
}

#[tokio::test]
// Test scenarios concerning connects/disconnects and declares.
// More fine grained tests are in the `ConnectedPeers` unit tests.
async fn test_connection_flow() {
	let mut test_state = TestState::default();
	let active_leaf = get_hash(10);
	let db = Db::new(MAX_STORED_SCORES_PER_PARA).await;
	let mut state = make_state(db, &mut test_state, active_leaf).await;
	let mut sender = test_state.sender.clone();

	let first_peer = PeerId::random();
	state.handle_peer_connected(&mut sender, first_peer, CollationVersion::V2).await;
	// If we don't get a disconnect message, it was accepted.
	test_state.assert_no_messages().await;
	assert_eq!(state.connected_peers(), [first_peer].into_iter().collect());

	// Reconnecting is a no-op. We should have first received a disconnect.
	state.handle_peer_connected(&mut sender, first_peer, CollationVersion::V1).await;
	test_state.assert_no_messages().await;
	assert_eq!(state.connected_peers(), [first_peer].into_iter().collect());

	// Disconnect the peer.
	state.handle_peer_disconnected(first_peer).await;
	assert_eq!(state.connected_peers(), Default::default());

	// Fill up the connection slots. For each para (ids 100 and 200) we should have 100 slots.
	let peer_ids = (0..(CONNECTED_PEERS_PARA_LIMIT.get() as u8))
		.map(|i| peer_id(i))
		.collect::<Vec<_>>();

	for id in peer_ids.iter() {
		state.handle_peer_connected(&mut sender, *id, CollationVersion::V2).await;
	}
	test_state.assert_no_messages().await;
	assert_eq!(state.connected_peers(), peer_ids.clone().into_iter().collect());

	// Now all 100 peers were accepted on both paras (since they're not declared).
	// A new connection from a peer with the same score will be rejected.
	let new_peer = PeerId::random();
	state.handle_peer_connected(&mut sender, new_peer, CollationVersion::V2).await;
	test_state.assert_peers_disconnected([new_peer]).await;
	assert_eq!(state.connected_peers(), peer_ids.clone().into_iter().collect());

	// Bump the reputations of all peers except for the first one.
	// The ith peer will have it's reputation bumped i times.
	let mut pending = vec![];
	for (i, peer) in peer_ids.iter().enumerate().skip(1) {
		for _ in 0..i {
			pending.push((ParaId::from(100), *peer));
		}
	}

	test_state.set_candidates_pending_availability(
		[(get_hash(1), pending), (get_hash(2), vec![(ParaId::from(100), new_peer)])]
			.into_iter()
			.collect(),
	);

	// Reputations are bumped on finalized block notifications.
	futures::join!(test_state.handle_finalized_block(2), async {
		state.handle_finalized_block(&mut sender, get_hash(2), 2).await.unwrap()
	});
	test_state.assert_no_messages().await;
	assert_eq!(state.connected_peers(), peer_ids.clone().into_iter().collect());

	state.handle_peer_connected(&mut sender, new_peer, CollationVersion::V2).await;
	// The new peer took the spot of the first one, but that other one remains connected for the
	// other para (200).
	test_state.assert_no_messages().await;
	assert_eq!(state.connected_peers(), peer_ids.clone().into_iter().chain([new_peer]).collect());

	// If that first peer then declares for another para, it will get disconnected.
	state.handle_declare(&mut sender, peer_ids[0], 100.into()).await;
	test_state.assert_peers_disconnected([peer_ids[0]]).await;
	assert_eq!(
		state.connected_peers(),
		peer_ids.clone().into_iter().skip(1).chain([new_peer]).collect()
	);

	// Make all peers declare for para 100.
	state.handle_declare(&mut sender, new_peer, 100.into()).await;
	for peer in peer_ids.iter().skip(1) {
		state.handle_declare(&mut sender, *peer, 100.into()).await;
	}
	test_state.assert_no_messages().await;

	// A subsequent declare is idempotent.
	state.handle_declare(&mut sender, new_peer, 100.into()).await;

	test_state.assert_no_messages().await;

	assert_eq!(
		state.connected_peers(),
		peer_ids.clone().into_iter().skip(1).chain([new_peer]).collect()
	);

	// The first peer can attempt to reconnect and declare for the other para.
	state
		.handle_peer_connected(&mut sender, peer_ids[0], CollationVersion::V2)
		.await;
	state.handle_declare(&mut sender, peer_ids[0], 200.into()).await;
	test_state.assert_no_messages().await;
	assert_eq!(state.connected_peers(), peer_ids.clone().into_iter().chain([new_peer]).collect());
	state.handle_peer_disconnected(peer_ids[0]).await;

	// Will be disconnected if declared to collate for an unscheduled para.
	state
		.handle_peer_connected(&mut sender, peer_ids[0], CollationVersion::V2)
		.await;
	state.handle_declare(&mut sender, peer_ids[0], 600.into()).await;
	test_state.assert_peers_disconnected([peer_ids[0]]).await;

	assert_eq!(
		state.connected_peers(),
		peer_ids.clone().into_iter().skip(1).chain([new_peer]).collect()
	);

	// The new peer will be disconnected if it switches the paraid.
	state.handle_declare(&mut sender, new_peer, 200.into()).await;
	test_state.assert_peers_disconnected([new_peer]).await;
	assert_eq!(state.connected_peers(), peer_ids.clone().into_iter().skip(1).collect());
}

#[tokio::test]
// Test that peer connections are rejected if we have no assignments.
async fn test_no_assignments() {
	let mut test_state = TestState::default();
	let active_leaf = get_hash(10);
	let active_leaf_info = test_state.rp_info.get(&active_leaf).unwrap().clone();
	let assigned_core = active_leaf_info.assigned_core;

	for info in test_state.rp_info.values_mut() {
		info.claim_queue.get_mut(&assigned_core).unwrap().clear();
	}

	let db = Db::new(MAX_STORED_SCORES_PER_PARA).await;
	let mut state = make_state(db, &mut test_state, active_leaf).await;
	let mut sender = test_state.sender.clone();

	let peer = peer_id(1);

	state.handle_peer_connected(&mut sender, peer, CollationVersion::V2).await;
	test_state.assert_peers_disconnected([peer]).await;
	assert!(state.connected_peers().is_empty());
	test_state.assert_no_messages().await;

	// Now add some assignments and connected peers. We want to check what happens to already
	// connected peers when we run out of assignments
	test_state.rp_info.insert(
		get_hash(11),
		RelayParentInfo {
			number: 11,
			parent: get_parent_hash(11),
			session_index: active_leaf_info.session_index,
			claim_queue: [(assigned_core, vec![600.into()])].into_iter().collect(),
			assigned_core,
		},
	);

	test_state.activate_leaf(&mut state, 11).await;

	let first_peer = peer;
	let second_peer = peer_id(2);

	state.handle_peer_connected(&mut sender, first_peer, CollationVersion::V2).await;
	state
		.handle_peer_connected(&mut sender, second_peer, CollationVersion::V2)
		.await;
	state.handle_declare(&mut sender, first_peer, 600.into()).await;
	test_state.assert_no_messages().await;
	assert_eq!(state.connected_peers(), [first_peer, second_peer].into_iter().collect());

	for height in 12..=14 {
		test_state.rp_info.insert(
			get_hash(height),
			RelayParentInfo {
				number: height,
				parent: get_parent_hash(height),
				session_index: active_leaf_info.session_index,
				claim_queue: [(assigned_core, vec![])].into_iter().collect(),
				assigned_core,
			},
		);
	}

	for _ in 12..=13 {
		test_state.activate_leaf(&mut state, 11).await;
		test_state.assert_no_messages().await;
	}
	assert_eq!(state.connected_peers(), [first_peer, second_peer].into_iter().collect());

	// When 14th leaf comes in, we're left with no assignments. Peers will be disconnected.
	test_state.activate_leaf(&mut state, 14).await;
	test_state.assert_peers_disconnected([first_peer, second_peer]).await;
	assert!(state.connected_peers().is_empty());
}

// Test peer connection inheritance across scheduled para change.
#[tokio::test]
async fn test_peer_connections_across_schedule_change() {
	let mut test_state = TestState::default();
	let active_leaf = get_hash(10);

	let db = Db::new(MAX_STORED_SCORES_PER_PARA).await;
	let mut state = make_state(db, &mut test_state, active_leaf).await;
	let mut sender = test_state.sender.clone();

	// Leaf 8 has 100, 200, 100.
	// Leaf 9 has 200, 100, 200.
	// Leaf 10 has 100, 200, 100.

	// First 5 peers will be declared for para 100.
	// Next 5 peers will be declared for para 200.
	// Last 5 peers undeclared.
	let peer_ids = (0..15).map(|i| peer_id(i)).collect::<Vec<_>>();

	for id in peer_ids.iter() {
		state.handle_peer_connected(&mut sender, *id, CollationVersion::V2).await;
	}
	test_state.assert_no_messages().await;
	assert_eq!(state.connected_peers(), peer_ids.clone().into_iter().collect());

	for id in &peer_ids[..5] {
		state.handle_declare(&mut sender, *id, 100.into()).await;
	}
	for id in &peer_ids[5..10] {
		state.handle_declare(&mut sender, *id, 200.into()).await;
	}
	test_state.assert_no_messages().await;
	assert_eq!(state.connected_peers(), peer_ids.clone().into_iter().collect());

	let all_100: Vec<ParaId> = std::iter::repeat(100.into()).take(3).collect();
	let all_600: Vec<ParaId> = std::iter::repeat(600.into()).take(3).collect();

	let prev_leaf_info = test_state.rp_info.get(&active_leaf).unwrap().clone();
	for (height, assignments) in [
		(11, vec![200.into(), 100.into(), 100.into()]),
		(12, all_100.clone()),
		(13, all_100.clone()),
		(14, all_100),
		(15, all_600.clone()),
		(16, all_600.clone()),
		(17, all_600),
	] {
		let mut cq = prev_leaf_info.claim_queue.clone();
		cq.insert(prev_leaf_info.assigned_core, assignments);

		test_state.rp_info.insert(
			get_hash(height),
			RelayParentInfo {
				number: height,
				parent: get_parent_hash(height),
				session_index: prev_leaf_info.session_index,
				claim_queue: cq.clone(),
				assigned_core: prev_leaf_info.assigned_core,
			},
		);
	}

	// Send an active leaf update which preserves one last claim for para 200.
	// Send the same active leaf update twice, should be idempotent.
	for _ in 0..2 {
		test_state.activate_leaf(&mut state, 11).await;
		test_state.assert_no_messages().await;
		assert_eq!(state.connected_peers(), peer_ids.clone().into_iter().collect());
	}

	// Send active leaf updates which drop all assignments for para 200. The declared peers for
	// 200 will be dropped.
	for height in 12..=13 {
		test_state.activate_leaf(&mut state, height).await;
		test_state.assert_no_messages().await;
		assert_eq!(state.connected_peers(), peer_ids.clone().into_iter().collect());
	}
	test_state.activate_leaf(&mut state, 14).await;
	test_state.assert_peers_disconnected((&peer_ids[5..10]).to_vec()).await;
	let expected_connected_peers = (&peer_ids[..5])
		.into_iter()
		.cloned()
		.chain((&peer_ids[10..15]).into_iter().cloned())
		.collect();
	assert_eq!(state.connected_peers(), expected_connected_peers);
	test_state.assert_no_messages().await;

	// Send active leaf updates which drop all assignments for para 100 as well. Only undeclared
	// peers will remain
	for height in 15..=16 {
		test_state.activate_leaf(&mut state, height).await;
		test_state.assert_no_messages().await;
		assert_eq!(state.connected_peers(), expected_connected_peers);
	}

	test_state.activate_leaf(&mut state, 17).await;

	test_state.assert_peers_disconnected((&peer_ids[0..5]).to_vec()).await;
	let expected_connected_peers = (&peer_ids[10..]).into_iter().cloned().collect();
	assert_eq!(state.connected_peers(), expected_connected_peers);
	test_state.assert_no_messages().await;

	// Add a fork which brings back assignment for para 200. Test that assignments are considered
	// across forks.
	let mut cq = prev_leaf_info.claim_queue.clone();
	cq.insert(prev_leaf_info.assigned_core, vec![200.into()]);

	let fork_hash = Hash::random();
	test_state.rp_info.insert(
		fork_hash,
		RelayParentInfo {
			number: 17,
			parent: get_parent_hash(17),
			session_index: prev_leaf_info.session_index,
			claim_queue: cq.clone(),
			assigned_core: prev_leaf_info.assigned_core,
		},
	);
	futures::join!(test_state.handle_view_update(vec![fork_hash]), async {
		state
			.handle_our_view_change(&mut sender, OurView::new([fork_hash], 0))
			.await
			.unwrap()
	});

	assert_eq!(state.connected_peers(), expected_connected_peers);
	test_state.assert_no_messages().await;

	// Declare a peer for para 600 and a peer for para 200. They should both be kept.
	let peer_200 = peer_ids[10];
	let peer_600 = peer_ids[11];
	state.handle_declare(&mut sender, peer_200, 200.into()).await;
	test_state.assert_no_messages().await;

	state.handle_declare(&mut sender, peer_600, 600.into()).await;
	test_state.assert_no_messages().await;
	assert_eq!(state.connected_peers(), expected_connected_peers);
}

// Test peer connection inheritance across group rotations.
#[tokio::test]
async fn test_peer_connections_across_group_rotations() {
	let mut test_state = TestState::default();
	let active_leaf = get_hash(10);
	let active_leaf_info = test_state.rp_info.get(&active_leaf).unwrap().clone();
	let initial_core = active_leaf_info.assigned_core;
	assert_eq!(initial_core, CoreIndex(0));
	let next_core = CoreIndex(1);

	// Set the rotation frequency to 12, so that the core is switched to core 1 on block 11.
	test_state
		.session_info
		.get_mut(&active_leaf_info.session_index)
		.unwrap()
		.group_rotation_info
		.group_rotation_frequency = 12;

	let db = Db::new(MAX_STORED_SCORES_PER_PARA).await;
	let mut state = make_state(db, &mut test_state, active_leaf).await;
	let mut sender = test_state.sender.clone();

	let mut cq = active_leaf_info.claim_queue.clone();
	cq.insert(next_core, std::iter::repeat(600.into()).take(3).collect());

	for rp_info in test_state.rp_info.values_mut() {
		rp_info.claim_queue = cq.clone();
	}

	// Leaf 8 has 100, 200, 100.
	// Leaf 9 has 200, 100, 200.
	// Leaf 10 has 100, 200, 100.
	// Leaves 11-13 switch to 600, 600, 600

	// First 5 peers will be declared for para 100.
	// Last 5 peers undeclared.
	let peer_ids = (0..10).map(|i| peer_id(i)).collect::<Vec<_>>();

	for id in peer_ids.iter() {
		state.handle_peer_connected(&mut sender, *id, CollationVersion::V2).await;
	}
	test_state.assert_no_messages().await;
	assert_eq!(state.connected_peers(), peer_ids.clone().into_iter().collect());

	for id in &peer_ids[..5] {
		state.handle_declare(&mut sender, *id, 100.into()).await;
	}
	test_state.assert_no_messages().await;
	assert_eq!(state.connected_peers(), peer_ids.clone().into_iter().collect());

	for height in 11..=13 {
		test_state.rp_info.insert(
			get_hash(height),
			RelayParentInfo {
				number: height,
				parent: get_parent_hash(height),
				session_index: active_leaf_info.session_index,
				claim_queue: cq.clone(),
				assigned_core: next_core,
			},
		);
	}

	for height in 11..=12 {
		test_state.activate_leaf(&mut state, height).await;
		test_state.assert_no_messages().await;
		assert_eq!(state.connected_peers(), peer_ids.clone().into_iter().collect());
	}

	test_state.activate_leaf(&mut state, 13).await;
	test_state.assert_peers_disconnected((&peer_ids[0..5]).to_vec()).await;
	assert_eq!(state.connected_peers(), (&peer_ids[5..]).into_iter().cloned().collect());

	// Declare the yet undeclared peers for para 600.
	for id in &peer_ids[5..] {
		state.handle_declare(&mut sender, *id, 600.into()).await;
	}
	test_state.assert_no_messages().await;
	assert_eq!(state.connected_peers(), (&peer_ids[5..]).into_iter().cloned().collect());
}

#[tokio::test]
// Test reputation bumps on finalized block notification.
async fn finalized_block_notification() {
	let mut test_state = TestState::default();
	let active_leaf = get_hash(10);

	let db = MockDb::default();
	let mut state = make_state(db.clone(), &mut test_state, active_leaf).await;
	let mut sender = test_state.sender.clone();

	// Add 3 peers and connect them
	let first_peer = peer_id(1);
	let second_peer = peer_id(2);
	let third_peer = peer_id(3);
	let peers = vec![first_peer, second_peer, third_peer];

	for peer in peers.iter() {
		state.handle_peer_connected(&mut sender, *peer, CollationVersion::V2).await;
	}
	test_state.assert_no_messages().await;
	assert_eq!(state.connected_peers(), peers.clone().into_iter().collect());

	// Finalize block 5, no expected bumps, because there are no included candidates.
	futures::join!(test_state.handle_finalized_block(5), async {
		state.handle_finalized_block(&mut sender, get_hash(5), 5).await.unwrap()
	});
	test_state.assert_no_messages().await;

	// Add one included candidate at block 6 for first peer and para 100.
	test_state.set_candidates_pending_availability(
		[(get_hash(6), vec![(ParaId::from(100), first_peer)])].into_iter().collect(),
	);

	let mut expected_bumps = BTreeMap::new();
	expected_bumps.insert(
		ParaId::new(100),
		[(first_peer, Score::new(VALID_INCLUDED_CANDIDATE_BUMP).unwrap())]
			.into_iter()
			.collect(),
	);

	futures::join!(test_state.handle_finalized_block(6), async {
		state.handle_finalized_block(&mut sender, get_hash(6), 6).await.unwrap()
	});
	test_state.assert_no_messages().await;

	assert_eq!(db.witnessed_bumps(), expected_bumps);

	// This peer is not even connected, but we should process its reputation bumps.
	let fourth_peer = peer_id(4);

	test_state.set_candidates_pending_availability(
		[
			// Keep this one to ensure that we don't end up processing it again.
			(get_hash(6), vec![(ParaId::from(100), first_peer)]),
			(
				get_hash(7),
				vec![
					(ParaId::from(200), first_peer),
					(ParaId::from(200), first_peer),
					(ParaId::from(200), second_peer),
				],
			),
			(get_hash(8), vec![(ParaId::from(100), fourth_peer)]),
			(get_hash(10), vec![(ParaId::from(100), first_peer)]),
		]
		.into_iter()
		.collect(),
	);

	let mut expected_bumps = BTreeMap::new();
	expected_bumps.insert(
		ParaId::new(100),
		[
			(first_peer, Score::new(VALID_INCLUDED_CANDIDATE_BUMP).unwrap()),
			(fourth_peer, Score::new(VALID_INCLUDED_CANDIDATE_BUMP).unwrap()),
		]
		.into_iter()
		.collect(),
	);
	expected_bumps.insert(
		ParaId::new(200),
		[
			(first_peer, Score::new(2 * VALID_INCLUDED_CANDIDATE_BUMP).unwrap()),
			(second_peer, Score::new(VALID_INCLUDED_CANDIDATE_BUMP).unwrap()),
		]
		.into_iter()
		.collect(),
	);

	// Add multiple included candidates at different block heights and check that they are processed
	// accordingly.
	futures::join!(test_state.handle_finalized_block(10), async {
		state.handle_finalized_block(&mut sender, get_hash(10), 10).await.unwrap()
	});
	test_state.assert_no_messages().await;
	assert_eq!(state.connected_peers(), peers.clone().into_iter().collect());

	assert_eq!(db.witnessed_bumps(), expected_bumps);
}

#[tokio::test]
// Test various scenarios for rejecting advertisements.
async fn test_advertisement_rejections() {
	let mut test_state = TestState::default();
	let active_leaf = get_hash(10);
	let leaf_info = test_state.rp_info.get(&active_leaf).unwrap().clone();

	let db = Db::new(MAX_STORED_SCORES_PER_PARA).await;
	let mut state = make_state(db, &mut test_state, active_leaf).await;
	let mut sender = test_state.sender.clone();

	let mut ccr = dummy_committed_candidate_receipt_v2(active_leaf);
	ccr.descriptor.set_para_id(100.into());
	ccr.descriptor.set_persisted_validation_data_hash(dummy_pvd().hash());
	ccr.descriptor.set_core_index(leaf_info.assigned_core);
	ccr.descriptor.set_session_index(leaf_info.session_index);

	let receipt = ccr.to_plain();
	let prospective_candidate = Some(ProspectiveCandidate {
		candidate_hash: receipt.hash(),
		parent_head_data_hash: dummy_pvd().parent_head.hash(),
	});

	// Send advertisement from a peer that is not connected. Will be dropped.
	let peer_id = PeerId::random();
	state.handle_advertisement(&mut sender, peer_id, active_leaf, None).await;
	assert!(state.advertisements().is_empty());
	state.try_launch_new_fetch_requests(&mut sender).await;
	test_state.assert_no_messages().await;

	// Send advertisement from a peer that is connected but not declared. Will be dropped.
	state.handle_peer_connected(&mut sender, peer_id, CollationVersion::V2).await;

	state
		.handle_advertisement(&mut sender, peer_id, active_leaf, prospective_candidate)
		.await;
	assert!(state.advertisements().is_empty());
	state.try_launch_new_fetch_requests(&mut sender).await;
	test_state.assert_no_messages().await;

	// Now declare. Still, the old advertisement was dropped.
	state.handle_declare(&mut sender, peer_id, 100.into()).await;
	state.try_launch_new_fetch_requests(&mut sender).await;
	test_state.assert_no_messages().await;
	assert!(state.advertisements().is_empty());

	// Relay parent outside view.
	state
		.handle_advertisement(&mut sender, peer_id, get_hash(11), prospective_candidate)
		.await;
	state.try_launch_new_fetch_requests(&mut sender).await;
	test_state.assert_no_messages().await;
	assert!(state.advertisements().is_empty());

	// Blocked by the backing subsystem. Do this twice, so that we reach the per-rp candidate limit
	// of this peer (which is 2).
	for _ in 0..2 {
		futures::join!(
			state.handle_advertisement(&mut sender, peer_id, active_leaf, prospective_candidate),
			test_state.assert_can_second_request(
				Advertisement {
					peer_id,
					para_id: 100.into(),
					relay_parent: active_leaf,
					prospective_candidate,
				},
				false,
			)
		);
		state.try_launch_new_fetch_requests(&mut sender).await;
		test_state.assert_no_messages().await;
		assert!(state.advertisements().is_empty());
	}

	// Here comes a valid advertisement, will be rejected because we reached the limit.
	state
		.handle_advertisement(&mut sender, peer_id, active_leaf, prospective_candidate)
		.await;

	// Let's add a new peerid then.
	let peer_id = PeerId::random();
	state.handle_peer_connected(&mut sender, peer_id, CollationVersion::V2).await;
	state.handle_declare(&mut sender, peer_id, 100.into()).await;

	// Add a valid advertisement.
	let adv = Advertisement {
		peer_id,
		para_id: 100.into(),
		relay_parent: active_leaf,
		prospective_candidate,
	};
	test_state.handle_advertisement(&mut state, adv).await;
	assert_eq!(state.advertisements(), [adv].into_iter().collect());

	// Duplicate advertisement. Only one fetch request will be launched.
	state
		.handle_advertisement(&mut sender, peer_id, active_leaf, prospective_candidate)
		.await;
	assert_eq!(state.advertisements(), [adv].into_iter().collect());
	state.try_launch_new_fetch_requests(&mut sender).await;
	test_state.assert_collation_request(adv).await;
	assert_eq!(state.advertisements(), [adv].into_iter().collect());
	test_state.assert_no_messages().await;

	// We still detect the duplicate advertisement with a fetching collation.
	state
		.handle_advertisement(&mut sender, peer_id, active_leaf, prospective_candidate)
		.await;
	assert_eq!(state.advertisements(), [adv].into_iter().collect());
	state.try_launch_new_fetch_requests(&mut sender).await;
	test_state.assert_no_messages().await;

	// We still detect the duplicate advertisement with the fetched collation.
	test_state.handle_fetched_collation(&mut state, adv, receipt).await;
	test_state.assert_no_messages().await;
	assert!(state.advertisements().is_empty());
	state
		.handle_advertisement(&mut sender, peer_id, active_leaf, prospective_candidate)
		.await;
	state.try_launch_new_fetch_requests(&mut sender).await;
	test_state.assert_no_messages().await;

	// We still detect the duplicate advertisement with a seconded collation.
	test_state
		.second_collation(&mut state, peer_id, CollationVersion::V2, ccr)
		.await;
	test_state.assert_no_messages().await;
	assert!(state.advertisements().is_empty());
	state
		.handle_advertisement(&mut sender, peer_id, active_leaf, prospective_candidate)
		.await;
	state.try_launch_new_fetch_requests(&mut sender).await;
	assert!(state.advertisements().is_empty());
	test_state.assert_no_messages().await;

	// V1 advertisement rejected on non-active leaf.
	let peer_id = PeerId::random();
	state.handle_peer_connected(&mut sender, peer_id, CollationVersion::V2).await;
	state.handle_declare(&mut sender, peer_id, 100.into()).await;
	state.handle_advertisement(&mut sender, peer_id, get_hash(9), None).await;
	assert!(state.advertisements().is_empty());
	state.try_launch_new_fetch_requests(&mut sender).await;
	test_state.assert_no_messages().await;
}

#[tokio::test]
async fn test_collation_fetch_failure() {
	let mut test_state = TestState::default();
	let active_leaf = get_hash(10);
	let leaf_info = test_state.rp_info.get(&active_leaf).unwrap().clone();

	let db = MockDb::default();
	let mut state = make_state(db.clone(), &mut test_state, active_leaf).await;
	let mut sender = test_state.sender.clone();

	let mut ccr = dummy_committed_candidate_receipt_v2(active_leaf);
	ccr.descriptor.set_para_id(100.into());
	ccr.descriptor.set_persisted_validation_data_hash(dummy_pvd().hash());
	ccr.descriptor.set_core_index(leaf_info.assigned_core);
	ccr.descriptor.set_session_index(leaf_info.session_index);

	let receipt = ccr.to_plain();
	let prospective_candidate = Some(ProspectiveCandidate {
		candidate_hash: receipt.hash(),
		parent_head_data_hash: dummy_pvd().parent_head.hash(),
	});

	// Different network errors.
	for (err, maybe_slash) in [
		// Cancelled by us. No slash.
		(Err(CollationFetchError::Cancelled), None),
		// Network error. No slash as it could be caused by us.
		(
			Err(CollationFetchError::Request(RequestError::NetworkError(
				RequestFailure::NotConnected,
			))),
			None,
		),
		(
			Err(CollationFetchError::Request(RequestError::NetworkError(RequestFailure::Network(
				OutboundFailure::DialFailure,
			)))),
			None,
		),
		// Timeout. Slash.
		(
			Err(CollationFetchError::Request(RequestError::NetworkError(RequestFailure::Network(
				OutboundFailure::Timeout,
			)))),
			Some(FAILED_FETCH_SLASH),
		),
		// Invalid response. Slash.
		(
			Err(CollationFetchError::Request(RequestError::InvalidResponse("Invalid".into()))),
			Some(FAILED_FETCH_SLASH),
		),
	] {
		let peer_id = PeerId::random();

		// We reuse the same advertisement, to test that if a fetch fails, another peer can
		// advertise the same collation.
		let adv = Advertisement {
			peer_id,
			para_id: 100.into(),
			relay_parent: active_leaf,
			prospective_candidate,
		};

		state.handle_peer_connected(&mut sender, peer_id, CollationVersion::V2).await;
		state.handle_declare(&mut sender, peer_id, 100.into()).await;

		test_state.handle_advertisement(&mut state, adv).await;
		state.try_launch_new_fetch_requests(&mut sender).await;
		test_state.assert_collation_request(adv).await;

		state.handle_fetched_collation(&mut sender, (adv, err)).await;
		// Once it failed, we no longer retry it.
		state.try_launch_new_fetch_requests(&mut sender).await;
		assert_eq!(db.witnessed_slash(), maybe_slash.map(|score| (peer_id, adv.para_id, score)));
		test_state.assert_no_messages().await;
	}

	// Received paraid is different than the advertised one. Try for both network protocol
	// versions. This implies a check on the candidate hash as well for v2 advertisements.
	for version in [CollationVersion::V1, CollationVersion::V2] {
		let peer_id = PeerId::random();

		let adv = Advertisement {
			peer_id,
			para_id: 100.into(),
			relay_parent: active_leaf,
			prospective_candidate: if version == CollationVersion::V2 {
				prospective_candidate
			} else {
				None
			},
		};

		state.handle_peer_connected(&mut sender, peer_id, version).await;
		state.handle_declare(&mut sender, peer_id, 100.into()).await;

		test_state.handle_advertisement(&mut state, adv).await;

		state.try_launch_new_fetch_requests(&mut sender).await;
		test_state.assert_collation_request(adv).await;

		let mut receipt = receipt.clone();
		// Modify the paraid.
		receipt.descriptor.set_para_id(200.into());
		let res = Ok(CollationFetchingResponse::Collation(receipt, dummy_pov()));
		state.handle_fetched_collation(&mut sender, (adv, res)).await;
		state.try_launch_new_fetch_requests(&mut sender).await;
		assert_eq!(db.witnessed_slash(), Some((peer_id, adv.para_id, FAILED_FETCH_SLASH)));
		test_state.assert_no_messages().await;
	}

	// Relay parent mismatch.
	{
		let peer_id = PeerId::random();

		let mut adv = Advertisement {
			peer_id,
			para_id: 100.into(),
			relay_parent: active_leaf,
			prospective_candidate,
		};

		state.handle_peer_connected(&mut sender, peer_id, CollationVersion::V2).await;
		state.handle_declare(&mut sender, peer_id, 100.into()).await;

		test_state.handle_advertisement(&mut state, adv).await;

		state.try_launch_new_fetch_requests(&mut sender).await;
		test_state.assert_collation_request(adv).await;

		// Modify the relay parent.
		adv.relay_parent = get_hash(8);
		let res = Ok(CollationFetchingResponse::Collation(receipt.clone(), dummy_pov()));
		state.handle_fetched_collation(&mut sender, (adv, res)).await;
		state.try_launch_new_fetch_requests(&mut sender).await;
		assert_eq!(db.witnessed_slash(), Some((peer_id, adv.para_id, FAILED_FETCH_SLASH)));
		test_state.assert_no_messages().await;
	}

	// Invalid core index on the v2 descriptor.
	{
		let peer_id = PeerId::random();

		let mut receipt = receipt.clone();
		// Set a different core index.
		receipt.descriptor.set_core_index(CoreIndex(5));

		let prospective_candidate = Some(ProspectiveCandidate {
			candidate_hash: receipt.hash(),
			parent_head_data_hash: dummy_pvd().parent_head.hash(),
		});
		let adv = Advertisement {
			peer_id,
			para_id: 100.into(),
			relay_parent: active_leaf,
			prospective_candidate,
		};

		state.handle_peer_connected(&mut sender, peer_id, CollationVersion::V2).await;
		state.handle_declare(&mut sender, peer_id, 100.into()).await;

		test_state.handle_advertisement(&mut state, adv).await;

		state.try_launch_new_fetch_requests(&mut sender).await;
		test_state.assert_collation_request(adv).await;

		let res = Ok(CollationFetchingResponse::Collation(receipt, dummy_pov()));
		state.handle_fetched_collation(&mut sender, (adv, res)).await;
		state.try_launch_new_fetch_requests(&mut sender).await;
		assert_eq!(db.witnessed_slash(), Some((peer_id, adv.para_id, FAILED_FETCH_SLASH)));
		test_state.assert_no_messages().await;
	}

	// Invalid session index on the v2 descriptor.
	{
		let peer_id = PeerId::random();

		let mut receipt = receipt.clone();
		// Set a different session index.
		receipt.descriptor.set_session_index(5);

		let prospective_candidate = Some(ProspectiveCandidate {
			candidate_hash: receipt.hash(),
			parent_head_data_hash: dummy_pvd().parent_head.hash(),
		});
		let adv = Advertisement {
			peer_id,
			para_id: 100.into(),
			relay_parent: active_leaf,
			prospective_candidate,
		};

		state.handle_peer_connected(&mut sender, peer_id, CollationVersion::V2).await;
		state.handle_declare(&mut sender, peer_id, 100.into()).await;

		test_state.handle_advertisement(&mut state, adv).await;

		state.try_launch_new_fetch_requests(&mut sender).await;
		test_state.assert_collation_request(adv).await;

		let res = Ok(CollationFetchingResponse::Collation(receipt, dummy_pov()));
		state.handle_fetched_collation(&mut sender, (adv, res)).await;
		state.try_launch_new_fetch_requests(&mut sender).await;
		assert_eq!(db.witnessed_slash(), Some((peer_id, adv.para_id, FAILED_FETCH_SLASH)));
		test_state.assert_no_messages().await;
	}

	// PVD not found. Only check for v1 advertisement, which will end up querying the runtime.
	// For v2 advertisement, an unknown PVF could just make the candidate end up being blocked from
	// seconding (and we have other tests for it)
	{
		let peer_id = PeerId::random();

		let adv = Advertisement {
			peer_id,
			para_id: 100.into(),
			relay_parent: active_leaf,
			prospective_candidate: None,
		};

		state.handle_peer_connected(&mut sender, peer_id, CollationVersion::V1).await;
		state.handle_declare(&mut sender, peer_id, 100.into()).await;

		test_state.handle_advertisement(&mut state, adv).await;

		state.try_launch_new_fetch_requests(&mut sender).await;
		test_state.assert_collation_request(adv).await;

		let res = Ok(CollationFetchingResponse::Collation(receipt.clone(), dummy_pov()));
		futures::join!(
			state.handle_fetched_collation(&mut sender, (adv, res)),
			test_state.assert_pvd_request(adv, None)
		);
		state.try_launch_new_fetch_requests(&mut sender).await;
		// No slash, as it's not the collator's fault.
		assert_eq!(db.witnessed_slash(), None);
		test_state.assert_no_messages().await;
	}

	// PVD Mismatch.
	{
		let peer_id = PeerId::random();

		let mut receipt = receipt.clone();
		// Modify some random thing in the receipt so that we get a different candidate.
		receipt.commitments_hash = get_hash(10);

		let prospective_candidate = Some(ProspectiveCandidate {
			candidate_hash: receipt.hash(),
			parent_head_data_hash: dummy_pvd().parent_head.hash(),
		});

		let adv = Advertisement {
			peer_id,
			para_id: 100.into(),
			relay_parent: active_leaf,
			prospective_candidate,
		};

		state.handle_peer_connected(&mut sender, peer_id, CollationVersion::V2).await;
		state.handle_declare(&mut sender, peer_id, 100.into()).await;

		test_state.handle_advertisement(&mut state, adv).await;

		state.try_launch_new_fetch_requests(&mut sender).await;
		test_state.assert_collation_request(adv).await;

		// Modify the PVD.
		let mut pvd: PersistedValidationData = dummy_pvd();
		pvd.relay_parent_number = 100;

		let res = Ok(CollationFetchingResponse::Collation(receipt, dummy_pov()));
		futures::join!(
			state.handle_fetched_collation(&mut sender, (adv, res)),
			test_state.assert_pvd_request(adv, Some(pvd))
		);
		state.try_launch_new_fetch_requests(&mut sender).await;
		assert_eq!(db.witnessed_slash(), Some((peer_id, adv.para_id, FAILED_FETCH_SLASH)));
		test_state.assert_no_messages().await;
	}

	// Parent head data mismatch.
	{
		let peer_id = PeerId::random();

		let mut receipt = receipt.clone();
		// Modify some random thing in the receipt so that we get a different candidate.
		receipt.commitments_hash = get_hash(11);

		let prospective_candidate = Some(ProspectiveCandidate {
			candidate_hash: receipt.hash(),
			// Randomly modify the parent head data hash in the advertisement.
			parent_head_data_hash: get_hash(11),
		});

		let adv = Advertisement {
			peer_id,
			para_id: 100.into(),
			relay_parent: active_leaf,
			prospective_candidate,
		};

		state.handle_peer_connected(&mut sender, peer_id, CollationVersion::V2).await;
		state.handle_declare(&mut sender, peer_id, 100.into()).await;

		test_state.handle_advertisement(&mut state, adv).await;

		state.try_launch_new_fetch_requests(&mut sender).await;
		test_state.assert_collation_request(adv).await;

		let res = Ok(CollationFetchingResponse::Collation(receipt, dummy_pov()));
		futures::join!(
			state.handle_fetched_collation(&mut sender, (adv, res)),
			test_state.assert_pvd_request(adv, Some(dummy_pvd()))
		);
		state.try_launch_new_fetch_requests(&mut sender).await;
		assert_eq!(db.witnessed_slash(), Some((peer_id, adv.para_id, FAILED_FETCH_SLASH)));
		test_state.assert_no_messages().await;
	}

	// Parent head data mismatch with full parent head present in response.
	{
		let peer_id = PeerId::random();

		let mut receipt = receipt.clone();
		// Modify some random thing in the receipt so that we get a different candidate.
		receipt.commitments_hash = get_hash(12);

		let prospective_candidate = Some(ProspectiveCandidate {
			candidate_hash: receipt.hash(),
			parent_head_data_hash: dummy_pvd().parent_head.hash(),
		});

		let adv = Advertisement {
			peer_id,
			para_id: 100.into(),
			relay_parent: active_leaf,
			prospective_candidate,
		};

		state.handle_peer_connected(&mut sender, peer_id, CollationVersion::V2).await;
		state.handle_declare(&mut sender, peer_id, 100.into()).await;

		test_state.handle_advertisement(&mut state, adv).await;

		state.try_launch_new_fetch_requests(&mut sender).await;
		test_state.assert_collation_request(adv).await;

		let res = Ok(CollationFetchingResponse::CollationWithParentHeadData {
			receipt,
			pov: dummy_pov(),
			// Add a random head data here.
			parent_head_data: HeadData(vec![1, 2, 3]),
		});
		let mut pvd = dummy_pvd();
		pvd.parent_head = HeadData(vec![1, 2, 3]);

		futures::join!(
			state.handle_fetched_collation(&mut sender, (adv, res)),
			test_state.assert_pvd_request(adv, Some(pvd))
		);
		state.try_launch_new_fetch_requests(&mut sender).await;
		assert_eq!(db.witnessed_slash(), Some((peer_id, adv.para_id, FAILED_FETCH_SLASH)));
		test_state.assert_no_messages().await;
	}
}

#[tokio::test]
// Test that v2 candidates are rejected if the node feature is disabled.
async fn test_v2_descriptor_without_feature_enabled() {
	let mut test_state = TestState::default();
	let active_leaf = get_hash(10);
	let leaf_info = test_state.rp_info.get(&active_leaf).unwrap().clone();
	// Clear the node feature.
	test_state.session_info.get_mut(&leaf_info.session_index).unwrap().v2_receipts = false;

	let db = MockDb::default();
	let mut state = make_state(db.clone(), &mut test_state, active_leaf).await;
	let mut sender = test_state.sender.clone();

	// Build a v2 candidate.
	let mut ccr = dummy_committed_candidate_receipt_v2(active_leaf);
	ccr.descriptor.set_para_id(100.into());
	ccr.descriptor.set_persisted_validation_data_hash(dummy_pvd().hash());
	ccr.descriptor.set_core_index(leaf_info.assigned_core);
	ccr.descriptor.set_session_index(leaf_info.session_index);

	let receipt = ccr.to_plain();
	let prospective_candidate = Some(ProspectiveCandidate {
		candidate_hash: receipt.hash(),
		parent_head_data_hash: dummy_pvd().parent_head.hash(),
	});

	let peer_id = PeerId::random();

	let adv = Advertisement {
		peer_id,
		para_id: 100.into(),
		relay_parent: active_leaf,
		prospective_candidate,
	};

	state.handle_peer_connected(&mut sender, peer_id, CollationVersion::V2).await;
	state.handle_declare(&mut sender, peer_id, 100.into()).await;

	test_state.handle_advertisement(&mut state, adv).await;

	state.try_launch_new_fetch_requests(&mut sender).await;
	test_state.assert_collation_request(adv).await;

	let res = Ok(CollationFetchingResponse::Collation(receipt, dummy_pov()));
	state.handle_fetched_collation(&mut sender, (adv, res)).await;
	state.try_launch_new_fetch_requests(&mut sender).await;
	assert_eq!(db.witnessed_slash(), Some((peer_id, adv.para_id, FAILED_FETCH_SLASH)));
	test_state.assert_no_messages().await;
}

#[rstest]
#[case(true)]
#[case(false)]
#[tokio::test]
// Test that we still accept v1 candidates regardless of whether the v2 descriptor node feature is
// enabled or not
async fn v1_descriptor_compatibility(#[case] v2_receipts: bool) {
	let mut test_state = TestState::default();
	let active_leaf = get_hash(10);
	let leaf_info = test_state.rp_info.get(&active_leaf).unwrap().clone();

	// Set the node feature.
	test_state.session_info.get_mut(&leaf_info.session_index).unwrap().v2_receipts = v2_receipts;

	let db = MockDb::default();
	let mut state = make_state(db.clone(), &mut test_state, active_leaf).await;
	let mut sender = test_state.sender.clone();

	// Build a v1 candidate.
	let mut ccr = dummy_committed_candidate_receipt(active_leaf);
	ccr.descriptor.para_id = 100.into();
	ccr.descriptor.persisted_validation_data_hash = dummy_pvd().hash();

	let receipt = ccr.to_plain();
	let prospective_candidate = Some(ProspectiveCandidate {
		candidate_hash: receipt.hash(),
		parent_head_data_hash: dummy_pvd().parent_head.hash(),
	});

	let peer_id = PeerId::random();

	let adv = Advertisement {
		peer_id,
		para_id: 100.into(),
		relay_parent: active_leaf,
		prospective_candidate,
	};

	state.handle_peer_connected(&mut sender, peer_id, CollationVersion::V2).await;
	state.handle_declare(&mut sender, peer_id, 100.into()).await;

	test_state.handle_advertisement(&mut state, adv).await;

	state.try_launch_new_fetch_requests(&mut sender).await;
	test_state.assert_collation_request(adv).await;

	test_state.handle_fetched_collation(&mut state, adv, receipt.into()).await;
	state.try_launch_new_fetch_requests(&mut sender).await;
	test_state.assert_no_messages().await;
	test_state
		.second_collation(&mut state, peer_id, CollationVersion::V2, ccr.into())
		.await;
	test_state.assert_no_messages().await;
}

// Launching new collations:
// - Verify that we don't try to make requests to a peer that disconnected and that the claims were
//   freed.
// - fetch_one_collation_at_a_time_for_v1_advertisement
// - candidates going out of view
// - multiple candidates per relay parent (including from implicit view)
// - Test fairness according to claim queue and rate limiting according to the claim queue
// - test delay, test prioritisation

// Collation fetch response:
// - fetched but went out of view in the meantime

// Collation seconded response:
// - Valid
// - Invalid

// Unblocking collations (PVD not found for a v2 advertisement), as well as cleanup of blocked
// collations

// Test peer disconnects in various scenarios. With collations being validated
// with collations being fetched. With collations seconded. With collations that are blocking other
// collations.

// View update for collation manager. Test the call to remove_peers and test that collations going
// out of view are pruned.

// LATER:
// - Test subsystem startup: make sure we are properly populating the db.
// - Test a change in the registered paras on finalized block notification.

// Not sure about these:
// - Test a session change and the effect it has on assignments.
