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

#![allow(clippy::clone_on_copy)]

use super::*;
use crate::*;
use polkadot_node_network_protocol::{
	grid_topology::TopologyPeerInfo,
	request_response::{outgoing::Recipient, ReqProtocolNames},
	v2::{BackedCandidateAcknowledgement, BackedCandidateManifest},
	view, ObservedRole,
};
use polkadot_node_primitives::Statement;
use polkadot_node_subsystem::messages::{
	network_bridge_event::NewGossipTopology, AllMessages, ChainApiMessage, HypotheticalCandidate,
	HypotheticalMembership, NetworkBridgeEvent, ProspectiveParachainsMessage, ReportPeerMessage,
	RuntimeApiMessage, RuntimeApiRequest,
};
use polkadot_node_subsystem_test_helpers as test_helpers;
use polkadot_node_subsystem_util::TimeoutExt;
use polkadot_primitives::{
	vstaging::{CommittedCandidateReceiptV2 as CommittedCandidateReceipt, CoreState},
	AssignmentPair, AsyncBackingParams, Block, BlockNumber, GroupRotationInfo, HeadData, Header,
	IndexedVec, PersistedValidationData, ScheduledCore, SessionIndex, SessionInfo, ValidatorPair,
};
use sc_keystore::LocalKeystore;
use sc_network::ProtocolName;
use sp_application_crypto::Pair as PairT;
use sp_authority_discovery::AuthorityPair as AuthorityDiscoveryPair;
use sp_keyring::Sr25519Keyring;

use assert_matches::assert_matches;
use codec::Encode;
use futures::Future;
use rand::{Rng, SeedableRng};
use test_helpers::mock::new_leaf;

use std::sync::Arc;

mod cluster;
mod grid;
mod requests;

type VirtualOverseer =
	polkadot_node_subsystem_test_helpers::TestSubsystemContextHandle<StatementDistributionMessage>;

const DEFAULT_ASYNC_BACKING_PARAMETERS: AsyncBackingParams =
	AsyncBackingParams { max_candidate_depth: 4, allowed_ancestry_len: 3 };

// Some deterministic genesis hash for req/res protocol names
const GENESIS_HASH: Hash = Hash::repeat_byte(0xff);

#[derive(Debug, Copy, Clone)]
enum LocalRole {
	/// Active validator.
	Validator,
	/// Authority, not in active validator set.
	InactiveValidator,
	/// Not a validator.
	None,
}

struct TestConfig {
	// number of active validators.
	validator_count: usize,
	// how many validators to place in each group.
	group_size: usize,
	// whether the local node should be a validator
	local_validator: LocalRole,
	async_backing_params: Option<AsyncBackingParams>,
}

#[derive(Debug, Clone)]
struct TestLocalValidator {
	validator_index: ValidatorIndex,
	group_index: Option<GroupIndex>,
}

struct TestState {
	config: TestConfig,
	local: Option<TestLocalValidator>,
	validators: Vec<ValidatorPair>,
	session_info: SessionInfo,
	req_sender: async_channel::Sender<sc_network::config::IncomingRequest>,
}

impl TestState {
	fn from_config(
		config: TestConfig,
		req_sender: async_channel::Sender<sc_network::config::IncomingRequest>,
		rng: &mut impl Rng,
	) -> Self {
		if config.group_size == 0 {
			panic!("group size cannot be 0");
		}

		let mut validators = Vec::new();
		let mut discovery_keys = Vec::new();
		let mut assignment_keys = Vec::new();
		let mut validator_groups = Vec::new();

		let local_validator_pos = if let LocalRole::Validator = config.local_validator {
			// ensure local validator is always in a full group.
			Some(rng.gen_range(0..config.validator_count).saturating_sub(config.group_size - 1))
		} else {
			None
		};

		for i in 0..config.validator_count {
			let validator_pair = if Some(i) == local_validator_pos {
				// Note: the specific key is used to ensure the keystore holds
				// this key and the subsystem can detect that it is a validator.
				Sr25519Keyring::Ferdie.pair().into()
			} else {
				ValidatorPair::generate().0
			};
			let assignment_id = AssignmentPair::generate().0.public();
			let discovery_id = AuthorityDiscoveryPair::generate().0.public();

			let group_index = i / config.group_size;
			validators.push(validator_pair);
			discovery_keys.push(discovery_id);
			assignment_keys.push(assignment_id);
			if validator_groups.len() == group_index {
				validator_groups.push(vec![ValidatorIndex(i as _)]);
			} else {
				validator_groups.last_mut().unwrap().push(ValidatorIndex(i as _));
			}
		}

		let local = match (config.local_validator, local_validator_pos) {
			(LocalRole::Validator, Some(local_pos)) => Some(TestLocalValidator {
				validator_index: ValidatorIndex(local_pos as _),
				group_index: Some(GroupIndex((local_pos / config.group_size) as _)),
			}),
			(LocalRole::InactiveValidator, None) => {
				discovery_keys.push(AuthorityDiscoveryPair::generate().0.public());
				Some(TestLocalValidator {
					validator_index: ValidatorIndex(config.validator_count as u32),
					group_index: None,
				})
			},
			_ => None,
		};

		let validator_public = validator_pubkeys(&validators);
		let session_info = SessionInfo {
			validators: validator_public,
			discovery_keys,
			validator_groups: IndexedVec::from(validator_groups),
			assignment_keys,
			n_cores: 0,
			zeroth_delay_tranche_width: 0,
			relay_vrf_modulo_samples: 0,
			n_delay_tranches: 0,
			no_show_slots: 0,
			needed_approvals: 0,
			active_validator_indices: vec![],
			dispute_period: 6,
			random_seed: [0u8; 32],
		};

		TestState { config, local, validators, session_info, req_sender }
	}

	fn make_dummy_leaf(&self, relay_parent: Hash) -> TestLeaf {
		self.make_dummy_leaf_with_multiple_cores_per_para(relay_parent, 1)
	}

	fn make_dummy_leaf_with_multiple_cores_per_para(
		&self,
		relay_parent: Hash,
		groups_for_first_para: usize,
	) -> TestLeaf {
		TestLeaf {
			number: 1,
			hash: relay_parent,
			parent_hash: Hash::repeat_byte(0),
			session: 1,
			availability_cores: self.make_availability_cores(|i| {
				let para_id = if i < groups_for_first_para {
					ParaId::from(0u32)
				} else {
					ParaId::from(i as u32)
				};

				CoreState::Scheduled(ScheduledCore { para_id, collator: None })
			}),
			disabled_validators: Default::default(),
			para_data: (0..self.session_info.validator_groups.len())
				.map(|i| {
					let para_id = if i < groups_for_first_para {
						ParaId::from(0u32)
					} else {
						ParaId::from(i as u32)
					};

					(para_id, PerParaData::new(1, vec![1, 2, 3].into()))
				})
				.collect(),
			minimum_backing_votes: 2,
		}
	}

	fn make_dummy_leaf_with_disabled_validators(
		&self,
		relay_parent: Hash,
		disabled_validators: Vec<ValidatorIndex>,
	) -> TestLeaf {
		TestLeaf { disabled_validators, ..self.make_dummy_leaf(relay_parent) }
	}

	fn make_dummy_leaf_with_min_backing_votes(
		&self,
		relay_parent: Hash,
		minimum_backing_votes: u32,
	) -> TestLeaf {
		TestLeaf { minimum_backing_votes, ..self.make_dummy_leaf(relay_parent) }
	}

	fn make_availability_cores(&self, f: impl Fn(usize) -> CoreState) -> Vec<CoreState> {
		(0..self.session_info.validator_groups.len()).map(f).collect()
	}

	fn make_dummy_topology(&self) -> NewGossipTopology {
		let validator_count = self.config.validator_count;
		let is_local_inactive = matches!(self.config.local_validator, LocalRole::InactiveValidator);

		let mut indices: Vec<usize> = (0..validator_count).collect();
		if is_local_inactive {
			indices.push(validator_count);
		}

		NewGossipTopology {
			session: 1,
			topology: SessionGridTopology::new(
				indices.clone(),
				indices
					.into_iter()
					.map(|i| TopologyPeerInfo {
						peer_ids: Vec::new(),
						validator_index: ValidatorIndex(i as u32),
						discovery_id: self.session_info.discovery_keys[i].clone(),
					})
					.collect(),
			),
			local_index: self.local.as_ref().map(|local| local.validator_index),
		}
	}

	fn group_validators(
		&self,
		group_index: GroupIndex,
		exclude_local: bool,
	) -> Vec<ValidatorIndex> {
		self.session_info
			.validator_groups
			.get(group_index)
			.unwrap()
			.iter()
			.cloned()
			.filter(|&i| {
				self.local.as_ref().map_or(true, |l| !exclude_local || l.validator_index != i)
			})
			.collect()
	}

	fn index_within_group(
		&self,
		group_index: GroupIndex,
		validator_index: ValidatorIndex,
	) -> Option<usize> {
		self.session_info
			.validator_groups
			.get(group_index)
			.unwrap()
			.iter()
			.position(|&i| i == validator_index)
	}

	fn discovery_id(&self, validator_index: ValidatorIndex) -> AuthorityDiscoveryId {
		self.session_info.discovery_keys[validator_index.0 as usize].clone()
	}

	fn sign_statement(
		&self,
		validator_index: ValidatorIndex,
		statement: CompactStatement,
		context: &SigningContext,
	) -> SignedStatement {
		let payload = statement.signing_payload(context);
		let pair = &self.validators[validator_index.0 as usize];
		let signature = pair.sign(&payload[..]);

		SignedStatement::new(statement, validator_index, signature, context, &pair.public())
			.unwrap()
	}

	fn sign_full_statement(
		&self,
		validator_index: ValidatorIndex,
		statement: Statement,
		context: &SigningContext,
		pvd: PersistedValidationData,
	) -> SignedFullStatementWithPVD {
		let payload = statement.to_compact().signing_payload(context);
		let pair = &self.validators[validator_index.0 as usize];
		let signature = pair.sign(&payload[..]);

		SignedFullStatementWithPVD::new(
			statement.supply_pvd(pvd),
			validator_index,
			signature,
			context,
			&pair.public(),
		)
		.unwrap()
	}

	// send a request out, returning a future which expects a response.
	async fn send_request(
		&mut self,
		peer: PeerId,
		request: AttestedCandidateRequest,
	) -> impl Future<Output = Option<sc_network::config::OutgoingResponse>> {
		let (tx, rx) = futures::channel::oneshot::channel();
		let req = sc_network::config::IncomingRequest {
			peer,
			payload: request.encode(),
			pending_response: tx,
		};
		self.req_sender.send(req).await.unwrap();

		rx.map(|r| r.ok())
	}
}

fn test_harness<T: Future<Output = VirtualOverseer>>(
	config: TestConfig,
	test: impl FnOnce(TestState, VirtualOverseer) -> T,
) {
	let pool = sp_core::testing::TaskExecutor::new();
	let keystore = if let LocalRole::Validator = config.local_validator {
		test_helpers::mock::make_ferdie_keystore()
	} else {
		Arc::new(LocalKeystore::in_memory()) as KeystorePtr
	};
	let req_protocol_names = ReqProtocolNames::new(&GENESIS_HASH, None);
	let (statement_req_receiver, _) = IncomingRequest::get_config_receiver::<
		Block,
		sc_network::NetworkWorker<Block, Hash>,
	>(&req_protocol_names);
	let (candidate_req_receiver, req_cfg) = IncomingRequest::get_config_receiver::<
		Block,
		sc_network::NetworkWorker<Block, Hash>,
	>(&req_protocol_names);
	let mut rng = rand_chacha::ChaCha8Rng::seed_from_u64(0);

	let test_state = TestState::from_config(config, req_cfg.inbound_queue.unwrap(), &mut rng);

	let (context, virtual_overseer) =
		polkadot_node_subsystem_test_helpers::make_subsystem_context(pool.clone());
	let subsystem = async move {
		let subsystem = crate::StatementDistributionSubsystem {
			keystore,
			v1_req_receiver: Some(statement_req_receiver),
			req_receiver: Some(candidate_req_receiver),
			metrics: Default::default(),
			rng,
			reputation: ReputationAggregator::new(|_| true),
		};

		if let Err(e) = subsystem.run(context).await {
			panic!("Fatal error: {:?}", e);
		}
	};

	let test_fut = test(test_state, virtual_overseer);

	futures::pin_mut!(test_fut);
	futures::pin_mut!(subsystem);
	futures::executor::block_on(future::join(
		async move {
			let mut virtual_overseer = test_fut.await;
			// Ensure we have handled all responses.
			if let Ok(Some(msg)) = virtual_overseer.rx.try_next() {
				panic!("Did not handle all responses: {:?}", msg);
			}
			// Conclude.
			virtual_overseer.send(FromOrchestra::Signal(OverseerSignal::Conclude)).await;
		},
		subsystem,
	));
}

struct PerParaData {
	min_relay_parent: BlockNumber,
	head_data: HeadData,
}

impl PerParaData {
	pub fn new(min_relay_parent: BlockNumber, head_data: HeadData) -> Self {
		Self { min_relay_parent, head_data }
	}
}

struct TestLeaf {
	number: BlockNumber,
	hash: Hash,
	parent_hash: Hash,
	session: SessionIndex,
	availability_cores: Vec<CoreState>,
	pub disabled_validators: Vec<ValidatorIndex>,
	para_data: Vec<(ParaId, PerParaData)>,
	minimum_backing_votes: u32,
}

impl TestLeaf {
	pub fn para_data(&self, para_id: ParaId) -> &PerParaData {
		self.para_data
			.iter()
			.find_map(|(p_id, data)| if *p_id == para_id { Some(data) } else { None })
			.unwrap()
	}
}

struct TestSetupInfo {
	local_validator: TestLocalValidator,
	local_group: GroupIndex,
	local_para: ParaId,
	other_group: GroupIndex,
	other_para: ParaId,
	relay_parent: Hash,
	test_leaf: TestLeaf,
	peers: Vec<PeerId>,
	validators: Vec<ValidatorIndex>,
}

struct TestPeerToConnect {
	local: bool,
	relay_parent_in_view: bool,
}

// TODO: Generalize, use in more places.
/// Sets up some test info that is common to most tests, and connects the requested peers.
async fn setup_test_and_connect_peers(
	state: &TestState,
	overseer: &mut VirtualOverseer,
	validator_count: usize,
	group_size: usize,
	peers_to_connect: &[TestPeerToConnect],
	send_topology_before_leaf: bool,
) -> TestSetupInfo {
	let local_validator = state.local.clone().unwrap();
	let local_group = local_validator.group_index.unwrap();
	let local_para = ParaId::from(local_group.0);

	let other_group = next_group_index(local_group, validator_count, group_size);
	let other_para = ParaId::from(other_group.0);

	let relay_parent = Hash::repeat_byte(1);
	let test_leaf = state.make_dummy_leaf(relay_parent);

	// Because we are testing grid mod, the "target" group (the one we communicate with) is usually
	// other_group, a non-local group.
	//
	// TODO: change based on `LocalRole`?
	let local_group_validators = state.group_validators(local_group, true);
	let other_group_validators = state.group_validators(other_group, true);

	let mut peers = vec![];
	let mut validators = vec![];
	let mut local_group_idx = 0;
	let mut other_group_idx = 0;
	for peer_to_connect in peers_to_connect {
		let peer = PeerId::random();
		peers.push(peer);

		let v = if peer_to_connect.local {
			let v = local_group_validators[local_group_idx];
			local_group_idx += 1;
			v
		} else {
			let v = other_group_validators[other_group_idx];
			other_group_idx += 1;
			v
		};
		validators.push(v);

		connect_peer(overseer, peer, Some(vec![state.discovery_id(v)].into_iter().collect())).await;

		if peer_to_connect.relay_parent_in_view {
			send_peer_view_change(overseer, peer.clone(), view![relay_parent]).await;
		}
	}

	// Send gossip topology and activate leaf.
	if send_topology_before_leaf {
		send_new_topology(overseer, state.make_dummy_topology()).await;
		// Send cleaning up of a leaf to make sure it does not clear the save topology as well.
		overseer
			.send(FromOrchestra::Signal(OverseerSignal::ActiveLeaves(
				ActiveLeavesUpdate::stop_work(Hash::random()),
			)))
			.await;
		activate_leaf(overseer, &test_leaf, &state, true, vec![]).await;
	} else {
		activate_leaf(overseer, &test_leaf, &state, true, vec![]).await;
		send_new_topology(overseer, state.make_dummy_topology()).await;
	}

	TestSetupInfo {
		local_validator,
		local_group,
		local_para,
		other_group,
		other_para,
		test_leaf,
		relay_parent,
		peers,
		validators,
	}
}

async fn activate_leaf(
	virtual_overseer: &mut VirtualOverseer,
	leaf: &TestLeaf,
	test_state: &TestState,
	is_new_session: bool,
	hypothetical_memberships: Vec<(HypotheticalCandidate, HypotheticalMembership)>,
) {
	let activated = new_leaf(leaf.hash, leaf.number);

	virtual_overseer
		.send(FromOrchestra::Signal(OverseerSignal::ActiveLeaves(ActiveLeavesUpdate::start_work(
			activated,
		))))
		.await;

	handle_leaf_activation(
		virtual_overseer,
		leaf,
		test_state,
		is_new_session,
		hypothetical_memberships,
	)
	.await;
}

async fn handle_leaf_activation(
	virtual_overseer: &mut VirtualOverseer,
	leaf: &TestLeaf,
	test_state: &TestState,
	is_new_session: bool,
	hypothetical_memberships: Vec<(HypotheticalCandidate, HypotheticalMembership)>,
) {
	let TestLeaf {
		number,
		hash,
		parent_hash,
		para_data,
		session,
		availability_cores,
		disabled_validators,
		minimum_backing_votes,
	} = leaf;

	assert_matches!(
		virtual_overseer.recv().await,
		AllMessages::RuntimeApi(
			RuntimeApiMessage::Request(parent, RuntimeApiRequest::AsyncBackingParams(tx))
		) if parent == *hash => {
			tx.send(Ok(test_state.config.async_backing_params.unwrap_or(DEFAULT_ASYNC_BACKING_PARAMETERS))).unwrap();
		}
	);

	let header = Header {
		parent_hash: *parent_hash,
		number: *number,
		state_root: Hash::zero(),
		extrinsics_root: Hash::zero(),
		digest: Default::default(),
	};
	assert_matches!(
		virtual_overseer.recv().await,
		AllMessages::ChainApi(
			ChainApiMessage::BlockHeader(parent, tx)
		) if parent == *hash => {
			tx.send(Ok(Some(header))).unwrap();
		}
	);

	let mrp_response: Vec<(ParaId, BlockNumber)> = para_data
		.iter()
		.map(|(para_id, data)| (*para_id, data.min_relay_parent))
		.collect();
	assert_matches!(
		virtual_overseer.recv().await,
		AllMessages::ProspectiveParachains(
			ProspectiveParachainsMessage::GetMinimumRelayParents(parent, tx)
		) if parent == *hash => {
			tx.send(mrp_response).unwrap();
		}
	);

	loop {
		match virtual_overseer.recv().await {
			AllMessages::RuntimeApi(RuntimeApiMessage::Request(
				_parent,
				RuntimeApiRequest::Version(tx),
			)) => {
				tx.send(Ok(RuntimeApiRequest::DISABLED_VALIDATORS_RUNTIME_REQUIREMENT)).unwrap();
			},
			AllMessages::RuntimeApi(RuntimeApiMessage::Request(
				parent,
				RuntimeApiRequest::DisabledValidators(tx),
			)) if parent == *hash => {
				tx.send(Ok(disabled_validators.clone())).unwrap();
			},
			AllMessages::RuntimeApi(RuntimeApiMessage::Request(
				_parent,
				RuntimeApiRequest::DisabledValidators(tx),
			)) => {
				tx.send(Ok(Vec::new())).unwrap();
			},
			AllMessages::RuntimeApi(RuntimeApiMessage::Request(
				_parent, // assume all active leaves are in the same session
				RuntimeApiRequest::SessionIndexForChild(tx),
			)) => {
				tx.send(Ok(*session)).unwrap();
			},
			AllMessages::RuntimeApi(RuntimeApiMessage::Request(
				parent,
				RuntimeApiRequest::SessionInfo(s, tx),
			)) if parent == *hash && s == *session => {
				assert!(is_new_session, "only expecting this call in a new session");
				tx.send(Ok(Some(test_state.session_info.clone()))).unwrap();
			},
			AllMessages::RuntimeApi(RuntimeApiMessage::Request(
				parent,
				RuntimeApiRequest::MinimumBackingVotes(session_index, tx),
			)) if parent == *hash && session_index == *session => {
				assert!(is_new_session, "only expecting this call in a new session");
				tx.send(Ok(*minimum_backing_votes)).unwrap();
			},
			AllMessages::RuntimeApi(RuntimeApiMessage::Request(
				parent,
				RuntimeApiRequest::AvailabilityCores(tx),
			)) if parent == *hash => {
				tx.send(Ok(availability_cores.clone())).unwrap();
			},
			AllMessages::RuntimeApi(RuntimeApiMessage::Request(
				parent,
				RuntimeApiRequest::ValidatorGroups(tx),
			)) if parent == *hash => {
				let validator_groups = test_state.session_info.validator_groups.to_vec();
				let group_rotation_info = GroupRotationInfo {
					session_start_block: 1,
					group_rotation_frequency: 12,
					now: 1,
				};
				tx.send(Ok((validator_groups, group_rotation_info))).unwrap();
			},
			AllMessages::ProspectiveParachains(
				ProspectiveParachainsMessage::GetHypotheticalMembership(req, tx),
			) => {
				assert_eq!(req.fragment_chain_relay_parent, Some(*hash));
				for (i, (candidate, _)) in hypothetical_memberships.iter().enumerate() {
					assert!(
						req.candidates.iter().any(|c| &c == &candidate),
						"did not receive request for hypothetical candidate {}",
						i,
					);
				}
				tx.send(hypothetical_memberships).unwrap();
				// this is the last expected runtime api call
				break
			},
			msg => panic!("unexpected runtime API call: {msg:?}"),
		}
	}
}

/// Intercepts an outgoing request, checks the fields, and sends the response.
async fn handle_sent_request(
	virtual_overseer: &mut VirtualOverseer,
	peer: PeerId,
	candidate_hash: CandidateHash,
	mask: StatementFilter,
	candidate_receipt: CommittedCandidateReceipt,
	persisted_validation_data: PersistedValidationData,
	statements: Vec<UncheckedSignedStatement>,
) {
	assert_matches!(
		virtual_overseer.recv().await,
		AllMessages::NetworkBridgeTx(NetworkBridgeTxMessage::SendRequests(mut requests, IfDisconnected::ImmediateError)) => {
			assert_eq!(requests.len(), 1);
			assert_matches!(
				requests.pop().unwrap(),
				Requests::AttestedCandidateV2(outgoing) => {
					assert_eq!(outgoing.peer, Recipient::Peer(peer));
					assert_eq!(outgoing.payload.candidate_hash, candidate_hash);
					assert_eq!(outgoing.payload.mask, mask);

					let res = AttestedCandidateResponse {
						candidate_receipt,
						persisted_validation_data,
						statements,
					};
					outgoing.pending_response.send(Ok((res.encode(), ProtocolName::from("")))).unwrap();
				}
			);
		}
	);
}

async fn answer_expected_hypothetical_membership_request(
	virtual_overseer: &mut VirtualOverseer,
	responses: Vec<(HypotheticalCandidate, HypotheticalMembership)>,
) {
	assert_matches!(
		virtual_overseer.recv().await,
		AllMessages::ProspectiveParachains(
			ProspectiveParachainsMessage::GetHypotheticalMembership(req, tx)
		) => {
			assert_eq!(req.fragment_chain_relay_parent, None);
			for (i, (candidate, _)) in responses.iter().enumerate() {
				assert!(
					req.candidates.iter().any(|c| &c == &candidate),
					"did not receive request for hypothetical candidate {}",
					i,
				);
			}

			tx.send(responses).unwrap();
		}
	)
}

#[macro_export]
macro_rules! assert_peer_reported {
	($virtual_overseer:expr, $peer_id:expr, $rep_change:expr $(,)*) => {
		assert_matches!(
			$virtual_overseer.recv().await,
			AllMessages::NetworkBridgeTx(NetworkBridgeTxMessage::ReportPeer(ReportPeerMessage::Single(p, r)))
				if p == $peer_id && r == $rep_change.into()
		);
	}
}

async fn send_share_message(
	virtual_overseer: &mut VirtualOverseer,
	relay_parent: Hash,
	statement: SignedFullStatementWithPVD,
) {
	virtual_overseer
		.send(FromOrchestra::Communication {
			msg: StatementDistributionMessage::Share(relay_parent, statement),
		})
		.await;
}

async fn send_backed_message(
	virtual_overseer: &mut VirtualOverseer,
	candidate_hash: CandidateHash,
) {
	virtual_overseer
		.send(FromOrchestra::Communication {
			msg: StatementDistributionMessage::Backed(candidate_hash),
		})
		.await;
}

async fn send_manifest_from_peer(
	virtual_overseer: &mut VirtualOverseer,
	peer_id: PeerId,
	manifest: BackedCandidateManifest,
) {
	send_peer_message(
		virtual_overseer,
		peer_id,
		protocol_v2::StatementDistributionMessage::BackedCandidateManifest(manifest),
	)
	.await;
}

async fn send_ack_from_peer(
	virtual_overseer: &mut VirtualOverseer,
	peer_id: PeerId,
	ack: BackedCandidateAcknowledgement,
) {
	send_peer_message(
		virtual_overseer,
		peer_id,
		protocol_v2::StatementDistributionMessage::BackedCandidateKnown(ack),
	)
	.await;
}

fn validator_pubkeys(val_ids: &[ValidatorPair]) -> IndexedVec<ValidatorIndex, ValidatorId> {
	val_ids.iter().map(|v| v.public().into()).collect()
}

async fn connect_peer(
	virtual_overseer: &mut VirtualOverseer,
	peer: PeerId,
	authority_ids: Option<HashSet<AuthorityDiscoveryId>>,
) {
	virtual_overseer
		.send(FromOrchestra::Communication {
			msg: StatementDistributionMessage::NetworkBridgeUpdate(
				NetworkBridgeEvent::PeerConnected(
					peer,
					ObservedRole::Authority,
					ValidationVersion::V2.into(),
					authority_ids,
				),
			),
		})
		.await;
}

// TODO: Add some tests using this?
#[allow(dead_code)]
async fn disconnect_peer(virtual_overseer: &mut VirtualOverseer, peer: PeerId) {
	virtual_overseer
		.send(FromOrchestra::Communication {
			msg: StatementDistributionMessage::NetworkBridgeUpdate(
				NetworkBridgeEvent::PeerDisconnected(peer),
			),
		})
		.await;
}

async fn send_peer_view_change(virtual_overseer: &mut VirtualOverseer, peer: PeerId, view: View) {
	virtual_overseer
		.send(FromOrchestra::Communication {
			msg: StatementDistributionMessage::NetworkBridgeUpdate(
				NetworkBridgeEvent::PeerViewChange(peer, view),
			),
		})
		.await;
}

async fn send_peer_message(
	virtual_overseer: &mut VirtualOverseer,
	peer: PeerId,
	message: protocol_v2::StatementDistributionMessage,
) {
	virtual_overseer
		.send(FromOrchestra::Communication {
			msg: StatementDistributionMessage::NetworkBridgeUpdate(
				NetworkBridgeEvent::PeerMessage(peer, Versioned::V2(message)),
			),
		})
		.await;
}

async fn send_new_topology(virtual_overseer: &mut VirtualOverseer, topology: NewGossipTopology) {
	virtual_overseer
		.send(FromOrchestra::Communication {
			msg: StatementDistributionMessage::NetworkBridgeUpdate(
				NetworkBridgeEvent::NewGossipTopology(topology),
			),
		})
		.await;
}

async fn overseer_recv_with_timeout(
	overseer: &mut VirtualOverseer,
	timeout: Duration,
) -> Option<AllMessages> {
	gum::trace!("waiting for message...");
	overseer.recv().timeout(timeout).await
}

fn next_group_index(
	group_index: GroupIndex,
	validator_count: usize,
	group_size: usize,
) -> GroupIndex {
	let next_group = group_index.0 + 1;
	let num_groups =
		validator_count / group_size + if validator_count % group_size > 0 { 1 } else { 0 };
	GroupIndex::from(next_group % num_groups as u32)
}
