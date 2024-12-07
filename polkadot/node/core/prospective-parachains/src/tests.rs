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
use polkadot_node_subsystem::{
	errors::RuntimeApiError,
	messages::{
		AllMessages, HypotheticalMembershipRequest, ParentHeadData, ProspectiveParachainsMessage,
		ProspectiveValidationDataRequest,
	},
};
use polkadot_node_subsystem_test_helpers as test_helpers;
use polkadot_primitives::{
	async_backing::{AsyncBackingParams, Constraints, InboundHrmpLimitations},
	vstaging::{
		async_backing::BackingState, CommittedCandidateReceiptV2 as CommittedCandidateReceipt,
		MutateDescriptorV2,
	},
	CoreIndex, HeadData, Header, PersistedValidationData, ScheduledCore, ValidationCodeHash,
};
use polkadot_primitives_test_helpers::make_candidate;
use rstest::rstest;
use std::{
	collections::{BTreeMap, VecDeque},
	sync::Arc,
};
use test_helpers::mock::new_leaf;

const ALLOWED_ANCESTRY_LEN: u32 = 3;
const ASYNC_BACKING_PARAMETERS: AsyncBackingParams =
	AsyncBackingParams { max_candidate_depth: 4, allowed_ancestry_len: ALLOWED_ANCESTRY_LEN };

const ASYNC_BACKING_DISABLED_ERROR: RuntimeApiError =
	RuntimeApiError::NotSupported { runtime_api_name: "test-runtime" };

const MAX_POV_SIZE: u32 = 1_000_000;

type VirtualOverseer =
	polkadot_node_subsystem_test_helpers::TestSubsystemContextHandle<ProspectiveParachainsMessage>;

fn dummy_constraints(
	min_relay_parent_number: BlockNumber,
	valid_watermarks: Vec<BlockNumber>,
	required_parent: HeadData,
	validation_code_hash: ValidationCodeHash,
) -> Constraints {
	Constraints {
		min_relay_parent_number,
		max_pov_size: MAX_POV_SIZE,
		max_code_size: 1_000_000,
		ump_remaining: 10,
		ump_remaining_bytes: 1_000,
		max_ump_num_per_candidate: 10,
		dmp_remaining_messages: vec![],
		hrmp_inbound: InboundHrmpLimitations { valid_watermarks },
		hrmp_channels_out: vec![],
		max_hrmp_num_per_candidate: 0,
		required_parent,
		validation_code_hash,
		upgrade_restriction: None,
		future_validation_code: None,
	}
}

struct TestState {
	claim_queue: BTreeMap<CoreIndex, VecDeque<ParaId>>,
	runtime_api_version: u32,
	validation_code_hash: ValidationCodeHash,
}

impl Default for TestState {
	fn default() -> Self {
		let chain_a = ParaId::from(1);
		let chain_b = ParaId::from(2);

		let mut claim_queue = BTreeMap::new();
		claim_queue.insert(CoreIndex(0), [chain_a].into_iter().collect());
		claim_queue.insert(CoreIndex(1), [chain_b].into_iter().collect());

		let validation_code_hash = Hash::repeat_byte(42).into();

		Self {
			validation_code_hash,
			claim_queue,
			runtime_api_version: RuntimeApiRequest::CLAIM_QUEUE_RUNTIME_REQUIREMENT,
		}
	}
}

impl TestState {
	fn set_runtime_api_version(&mut self, version: u32) {
		self.runtime_api_version = version;
	}
}

fn get_parent_hash(hash: Hash) -> Hash {
	Hash::from_low_u64_be(hash.to_low_u64_be() + 1)
}

fn test_harness<T: Future<Output = VirtualOverseer>>(
	test: impl FnOnce(VirtualOverseer) -> T,
) -> View {
	sp_tracing::init_for_tests();

	let pool = sp_core::testing::TaskExecutor::new();

	let (mut context, virtual_overseer) =
		polkadot_node_subsystem_test_helpers::make_subsystem_context(pool.clone());

	let mut view = View::new();
	let subsystem = async move {
		if let Err(e) = run_iteration(&mut context, &mut view, &Metrics(None)).await {
			panic!("{:?}", e);
		}

		view
	};

	let test_fut = test(virtual_overseer);

	futures::pin_mut!(test_fut);
	futures::pin_mut!(subsystem);
	let (_, view) = futures::executor::block_on(future::join(
		async move {
			let mut virtual_overseer = test_fut.await;
			virtual_overseer.send(FromOrchestra::Signal(OverseerSignal::Conclude)).await;
		},
		subsystem,
	));

	view
}

#[derive(Debug, Clone)]
struct PerParaData {
	min_relay_parent: BlockNumber,
	head_data: HeadData,
	pending_availability: Vec<CandidatePendingAvailability>,
}

impl PerParaData {
	pub fn new(min_relay_parent: BlockNumber, head_data: HeadData) -> Self {
		Self { min_relay_parent, head_data, pending_availability: Vec::new() }
	}

	pub fn new_with_pending(
		min_relay_parent: BlockNumber,
		head_data: HeadData,
		pending: Vec<CandidatePendingAvailability>,
	) -> Self {
		Self { min_relay_parent, head_data, pending_availability: pending }
	}
}

struct TestLeaf {
	number: BlockNumber,
	hash: Hash,
	para_data: Vec<(ParaId, PerParaData)>,
}

impl TestLeaf {
	pub fn para_data(&self, para_id: ParaId) -> &PerParaData {
		self.para_data
			.iter()
			.find_map(|(p_id, data)| if *p_id == para_id { Some(data) } else { None })
			.unwrap()
	}
}

async fn send_block_header(virtual_overseer: &mut VirtualOverseer, hash: Hash, number: u32) {
	let header = Header {
		parent_hash: get_parent_hash(hash),
		number,
		state_root: Hash::zero(),
		extrinsics_root: Hash::zero(),
		digest: Default::default(),
	};

	assert_matches!(
		virtual_overseer.recv().await,
		AllMessages::ChainApi(
			ChainApiMessage::BlockHeader(parent, tx)
		) if parent == hash => {
			tx.send(Ok(Some(header))).unwrap();
		}
	);
}

async fn activate_leaf(
	virtual_overseer: &mut VirtualOverseer,
	leaf: &TestLeaf,
	test_state: &TestState,
) {
	activate_leaf_with_params(virtual_overseer, leaf, test_state, ASYNC_BACKING_PARAMETERS).await;
}

async fn activate_leaf_with_parent_hash_fn(
	virtual_overseer: &mut VirtualOverseer,
	leaf: &TestLeaf,
	test_state: &TestState,
	parent_hash_fn: impl Fn(Hash) -> Hash,
) {
	let TestLeaf { number, hash, .. } = leaf;

	let activated = new_leaf(*hash, *number);

	virtual_overseer
		.send(FromOrchestra::Signal(OverseerSignal::ActiveLeaves(ActiveLeavesUpdate::start_work(
			activated,
		))))
		.await;

	handle_leaf_activation(
		virtual_overseer,
		leaf,
		test_state,
		ASYNC_BACKING_PARAMETERS,
		parent_hash_fn,
	)
	.await;
}

async fn activate_leaf_with_params(
	virtual_overseer: &mut VirtualOverseer,
	leaf: &TestLeaf,
	test_state: &TestState,
	async_backing_params: AsyncBackingParams,
) {
	let TestLeaf { number, hash, .. } = leaf;

	let activated = new_leaf(*hash, *number);

	virtual_overseer
		.send(FromOrchestra::Signal(OverseerSignal::ActiveLeaves(ActiveLeavesUpdate::start_work(
			activated,
		))))
		.await;

	handle_leaf_activation(
		virtual_overseer,
		leaf,
		test_state,
		async_backing_params,
		get_parent_hash,
	)
	.await;
}

async fn handle_leaf_activation(
	virtual_overseer: &mut VirtualOverseer,
	leaf: &TestLeaf,
	test_state: &TestState,
	async_backing_params: AsyncBackingParams,
	parent_hash_fn: impl Fn(Hash) -> Hash,
) {
	let TestLeaf { number, hash, para_data } = leaf;

	assert_matches!(
		virtual_overseer.recv().await,
		AllMessages::RuntimeApi(
			RuntimeApiMessage::Request(parent, RuntimeApiRequest::AsyncBackingParams(tx))
		) if parent == *hash => {
			tx.send(Ok(async_backing_params)).unwrap();
		}
	);

	assert_matches!(
		virtual_overseer.recv().await,
		AllMessages::RuntimeApi(
			RuntimeApiMessage::Request(parent, RuntimeApiRequest::Version(tx))
		) if parent == *hash => {
			tx.send(
				Ok(test_state.runtime_api_version)
			).unwrap();
		}
	);

	if test_state.runtime_api_version < RuntimeApiRequest::CLAIM_QUEUE_RUNTIME_REQUIREMENT {
		assert_matches!(
			virtual_overseer.recv().await,
			AllMessages::RuntimeApi(
				RuntimeApiMessage::Request(parent, RuntimeApiRequest::AvailabilityCores(tx))
			) if parent == *hash => {
				tx.send(Ok(test_state.claim_queue.values().map(|paras| CoreState::Scheduled(
					ScheduledCore {
						para_id: *paras.front().unwrap(),
						collator: None
					}
				)).collect())).unwrap();
			}
		);
	} else {
		assert_matches!(
			virtual_overseer.recv().await,
			AllMessages::RuntimeApi(
				RuntimeApiMessage::Request(parent, RuntimeApiRequest::ClaimQueue(tx))
			) if parent == *hash => {
				tx.send(Ok(test_state.claim_queue.clone())).unwrap();
			}
		);
	}

	send_block_header(virtual_overseer, *hash, *number).await;

	// Check that subsystem job issues a request for ancestors.
	let min_min = para_data.iter().map(|(_, data)| data.min_relay_parent).min().unwrap_or(*number);
	let ancestry_len = number - min_min;
	let ancestry_hashes: Vec<Hash> =
		std::iter::successors(Some(*hash), |h| Some(parent_hash_fn(*h)))
			.skip(1)
			.take(ancestry_len as usize)
			.collect();
	let ancestry_numbers = (min_min..*number).rev();
	let ancestry_iter = ancestry_hashes.clone().into_iter().zip(ancestry_numbers).peekable();
	if ancestry_len > 0 {
		assert_matches!(
			virtual_overseer.recv().await,
			AllMessages::ChainApi(
				ChainApiMessage::Ancestors{hash: block_hash, k, response_channel: tx}
			) if block_hash == *hash && k == ALLOWED_ANCESTRY_LEN as usize => {
				tx.send(Ok(ancestry_hashes.clone())).unwrap();
			}
		);

		assert_matches!(
			virtual_overseer.recv().await,
			AllMessages::RuntimeApi(
				RuntimeApiMessage::Request(parent, RuntimeApiRequest::SessionIndexForChild(tx))
			) if parent == *hash => {
				tx.send(Ok(1)).unwrap();
			}
		);
	}

	let mut used_relay_parents = HashSet::new();
	for (hash, number) in ancestry_iter {
		if !used_relay_parents.contains(&hash) {
			send_block_header(virtual_overseer, hash, number).await;
			assert_matches!(
				virtual_overseer.recv().await,
				AllMessages::RuntimeApi(
					RuntimeApiMessage::Request(parent, RuntimeApiRequest::SessionIndexForChild(tx))
				) if parent == hash => {
					tx.send(Ok(1)).unwrap();
				}
			);
			used_relay_parents.insert(hash);
		}
	}

	let paras: HashSet<_> = test_state.claim_queue.values().flatten().collect();

	for _ in 0..paras.len() {
		let message = virtual_overseer.recv().await;
		// Get the para we are working with since the order is not deterministic.
		let para_id = match &message {
			AllMessages::RuntimeApi(RuntimeApiMessage::Request(
				_,
				RuntimeApiRequest::ParaBackingState(p_id, _),
			)) => *p_id,
			_ => panic!("received unexpected message {:?}", message),
		};

		let PerParaData { min_relay_parent, head_data, pending_availability } =
			leaf.para_data(para_id);
		let constraints = dummy_constraints(
			*min_relay_parent,
			vec![*number],
			head_data.clone(),
			test_state.validation_code_hash,
		);
		let backing_state =
			BackingState { constraints, pending_availability: pending_availability.clone() };

		assert_matches!(
			message,
			AllMessages::RuntimeApi(
				RuntimeApiMessage::Request(parent, RuntimeApiRequest::ParaBackingState(p_id, tx))
			) if parent == *hash && p_id == para_id => {
				tx.send(Ok(Some(backing_state))).unwrap();
			}
		);

		for pending in pending_availability {
			if !used_relay_parents.contains(&pending.descriptor.relay_parent()) {
				send_block_header(
					virtual_overseer,
					pending.descriptor.relay_parent(),
					pending.relay_parent_number,
				)
				.await;

				used_relay_parents.insert(pending.descriptor.relay_parent());
			}
		}
	}

	// Get minimum relay parents.
	let (tx, rx) = oneshot::channel();
	virtual_overseer
		.send(overseer::FromOrchestra::Communication {
			msg: ProspectiveParachainsMessage::GetMinimumRelayParents(*hash, tx),
		})
		.await;
	let mut resp = rx.await.unwrap();
	resp.sort();
	let mrp_response: Vec<(ParaId, BlockNumber)> = para_data
		.iter()
		.map(|(para_id, data)| (*para_id, data.min_relay_parent))
		.collect();
	assert_eq!(resp, mrp_response);
}

async fn deactivate_leaf(virtual_overseer: &mut VirtualOverseer, hash: Hash) {
	virtual_overseer
		.send(FromOrchestra::Signal(OverseerSignal::ActiveLeaves(ActiveLeavesUpdate::stop_work(
			hash,
		))))
		.await;
}

async fn introduce_seconded_candidate(
	virtual_overseer: &mut VirtualOverseer,
	candidate: CommittedCandidateReceipt,
	pvd: PersistedValidationData,
) {
	let req = IntroduceSecondedCandidateRequest {
		candidate_para: candidate.descriptor.para_id(),
		candidate_receipt: candidate,
		persisted_validation_data: pvd,
	};
	let (tx, rx) = oneshot::channel();
	virtual_overseer
		.send(overseer::FromOrchestra::Communication {
			msg: ProspectiveParachainsMessage::IntroduceSecondedCandidate(req, tx),
		})
		.await;
	assert!(rx.await.unwrap());
}

async fn introduce_seconded_candidate_failed(
	virtual_overseer: &mut VirtualOverseer,
	candidate: CommittedCandidateReceipt,
	pvd: PersistedValidationData,
) {
	let req = IntroduceSecondedCandidateRequest {
		candidate_para: candidate.descriptor.para_id(),
		candidate_receipt: candidate,
		persisted_validation_data: pvd,
	};
	let (tx, rx) = oneshot::channel();
	virtual_overseer
		.send(overseer::FromOrchestra::Communication {
			msg: ProspectiveParachainsMessage::IntroduceSecondedCandidate(req, tx),
		})
		.await;
	assert!(!rx.await.unwrap());
}

async fn back_candidate(
	virtual_overseer: &mut VirtualOverseer,
	candidate: &CommittedCandidateReceipt,
	candidate_hash: CandidateHash,
) {
	virtual_overseer
		.send(overseer::FromOrchestra::Communication {
			msg: ProspectiveParachainsMessage::CandidateBacked(
				candidate.descriptor.para_id(),
				candidate_hash,
			),
		})
		.await;
}

async fn get_backable_candidates(
	virtual_overseer: &mut VirtualOverseer,
	leaf: &TestLeaf,
	para_id: ParaId,
	ancestors: Ancestors,
	count: u32,
	expected_result: Vec<(CandidateHash, Hash)>,
) {
	let (tx, rx) = oneshot::channel();
	virtual_overseer
		.send(overseer::FromOrchestra::Communication {
			msg: ProspectiveParachainsMessage::GetBackableCandidates(
				leaf.hash, para_id, count, ancestors, tx,
			),
		})
		.await;
	let resp = rx.await.unwrap();
	assert_eq!(resp, expected_result);
}

async fn get_hypothetical_membership(
	virtual_overseer: &mut VirtualOverseer,
	candidate_hash: CandidateHash,
	receipt: CommittedCandidateReceipt,
	persisted_validation_data: PersistedValidationData,
	expected_membership: Vec<Hash>,
) {
	let hypothetical_candidate = HypotheticalCandidate::Complete {
		candidate_hash,
		receipt: Arc::new(receipt),
		persisted_validation_data,
	};
	let request = HypotheticalMembershipRequest {
		candidates: vec![hypothetical_candidate.clone()],
		fragment_chain_relay_parent: None,
	};
	let (tx, rx) = oneshot::channel();
	virtual_overseer
		.send(overseer::FromOrchestra::Communication {
			msg: ProspectiveParachainsMessage::GetHypotheticalMembership(request, tx),
		})
		.await;
	let mut resp = rx.await.unwrap();
	assert_eq!(resp.len(), 1);
	let (candidate, membership) = resp.remove(0);
	assert_eq!(candidate, hypothetical_candidate);
	assert_eq!(
		membership.into_iter().collect::<HashSet<_>>(),
		expected_membership.into_iter().collect::<HashSet<_>>()
	);
}

async fn get_pvd(
	virtual_overseer: &mut VirtualOverseer,
	para_id: ParaId,
	candidate_relay_parent: Hash,
	parent_head_data: HeadData,
	expected_pvd: Option<PersistedValidationData>,
) {
	let request = ProspectiveValidationDataRequest {
		para_id,
		candidate_relay_parent,
		parent_head_data: ParentHeadData::OnlyHash(parent_head_data.hash()),
	};
	let (tx, rx) = oneshot::channel();
	virtual_overseer
		.send(overseer::FromOrchestra::Communication {
			msg: ProspectiveParachainsMessage::GetProspectiveValidationData(request, tx),
		})
		.await;
	let resp = rx.await.unwrap();
	assert_eq!(resp, expected_pvd);
}

macro_rules! make_and_back_candidate {
	($test_state:ident, $virtual_overseer:ident, $leaf:ident, $parent:expr, $index:expr) => {{
		let (mut candidate, pvd) = make_candidate(
			$leaf.hash,
			$leaf.number,
			1.into(),
			$parent.commitments.head_data.clone(),
			HeadData(vec![$index]),
			$test_state.validation_code_hash,
		);
		// Set a field to make this candidate unique.
		candidate.descriptor.set_para_head(Hash::from_low_u64_le($index));
		let candidate_hash = candidate.hash();
		introduce_seconded_candidate(&mut $virtual_overseer, candidate.clone(), pvd).await;
		back_candidate(&mut $virtual_overseer, &candidate, candidate_hash).await;

		(candidate, candidate_hash)
	}};
}

#[test]
fn should_do_no_work_if_async_backing_disabled_for_leaf() {
	async fn activate_leaf_async_backing_disabled(virtual_overseer: &mut VirtualOverseer) {
		let hash = Hash::from_low_u64_be(130);

		// Start work on some new parent.
		virtual_overseer
			.send(FromOrchestra::Signal(OverseerSignal::ActiveLeaves(
				ActiveLeavesUpdate::start_work(new_leaf(hash, 1)),
			)))
			.await;

		assert_matches!(
			virtual_overseer.recv().await,
			AllMessages::RuntimeApi(
				RuntimeApiMessage::Request(parent, RuntimeApiRequest::AsyncBackingParams(tx))
			) if parent == hash => {
				tx.send(Err(ASYNC_BACKING_DISABLED_ERROR)).unwrap();
			}
		);
	}

	let view = test_harness(|mut virtual_overseer| async move {
		activate_leaf_async_backing_disabled(&mut virtual_overseer).await;

		virtual_overseer
	});

	assert!(view.active_leaves.is_empty());
}

// Send some candidates and make sure all are found:
// - Two for the same leaf A (one for parachain 1 and one for parachain 2)
// - One for leaf B on parachain 1
// - One for leaf C on parachain 2
// Also tests a claim queue size larger than 1.
#[test]
fn introduce_candidates_basic() {
	let mut test_state = TestState::default();

	let chain_a = ParaId::from(1);
	let chain_b = ParaId::from(2);
	let mut claim_queue = BTreeMap::new();
	claim_queue.insert(CoreIndex(0), [chain_a, chain_b].into_iter().collect());

	test_state.claim_queue = claim_queue;

	let view = test_harness(|mut virtual_overseer| async move {
		// Leaf A
		let leaf_a = TestLeaf {
			number: 100,
			hash: Hash::from_low_u64_be(130),
			para_data: vec![
				(1.into(), PerParaData::new(97, HeadData(vec![1, 2, 3]))),
				(2.into(), PerParaData::new(100, HeadData(vec![2, 3, 4]))),
			],
		};
		// Leaf B
		let leaf_b = TestLeaf {
			number: 101,
			hash: Hash::from_low_u64_be(131),
			para_data: vec![
				(1.into(), PerParaData::new(99, HeadData(vec![3, 4, 5]))),
				(2.into(), PerParaData::new(101, HeadData(vec![4, 5, 6]))),
			],
		};
		// Leaf C
		let leaf_c = TestLeaf {
			number: 102,
			hash: Hash::from_low_u64_be(132),
			para_data: vec![
				(1.into(), PerParaData::new(102, HeadData(vec![5, 6, 7]))),
				(2.into(), PerParaData::new(98, HeadData(vec![6, 7, 8]))),
			],
		};

		// Activate leaves.
		activate_leaf(&mut virtual_overseer, &leaf_a, &test_state).await;
		activate_leaf(&mut virtual_overseer, &leaf_b, &test_state).await;
		activate_leaf(&mut virtual_overseer, &leaf_c, &test_state).await;

		// Candidate A1
		let (candidate_a1, pvd_a1) = make_candidate(
			leaf_a.hash,
			leaf_a.number,
			1.into(),
			HeadData(vec![1, 2, 3]),
			HeadData(vec![1]),
			test_state.validation_code_hash,
		);
		let candidate_hash_a1 = candidate_a1.hash();
		let response_a1 = vec![(candidate_hash_a1, leaf_a.hash)];

		// Candidate A2
		let (candidate_a2, pvd_a2) = make_candidate(
			leaf_a.hash,
			leaf_a.number,
			2.into(),
			HeadData(vec![2, 3, 4]),
			HeadData(vec![2]),
			test_state.validation_code_hash,
		);
		let candidate_hash_a2 = candidate_a2.hash();
		let response_a2 = vec![(candidate_hash_a2, leaf_a.hash)];

		// Candidate B
		let (candidate_b, pvd_b) = make_candidate(
			leaf_b.hash,
			leaf_b.number,
			1.into(),
			HeadData(vec![3, 4, 5]),
			HeadData(vec![3]),
			test_state.validation_code_hash,
		);
		let candidate_hash_b = candidate_b.hash();
		let response_b = vec![(candidate_hash_b, leaf_b.hash)];

		// Candidate C
		let (candidate_c, pvd_c) = make_candidate(
			leaf_c.hash,
			leaf_c.number,
			2.into(),
			HeadData(vec![6, 7, 8]),
			HeadData(vec![4]),
			test_state.validation_code_hash,
		);
		let candidate_hash_c = candidate_c.hash();
		let response_c = vec![(candidate_hash_c, leaf_c.hash)];

		// Introduce candidates.
		introduce_seconded_candidate(&mut virtual_overseer, candidate_a1.clone(), pvd_a1).await;
		introduce_seconded_candidate(&mut virtual_overseer, candidate_a2.clone(), pvd_a2).await;
		introduce_seconded_candidate(&mut virtual_overseer, candidate_b.clone(), pvd_b).await;
		introduce_seconded_candidate(&mut virtual_overseer, candidate_c.clone(), pvd_c).await;

		// Back candidates. Otherwise, we cannot check membership with GetBackableCandidates.
		back_candidate(&mut virtual_overseer, &candidate_a1, candidate_hash_a1).await;
		back_candidate(&mut virtual_overseer, &candidate_a2, candidate_hash_a2).await;
		back_candidate(&mut virtual_overseer, &candidate_b, candidate_hash_b).await;
		back_candidate(&mut virtual_overseer, &candidate_c, candidate_hash_c).await;

		// Check candidate tree membership.
		get_backable_candidates(
			&mut virtual_overseer,
			&leaf_a,
			1.into(),
			Ancestors::default(),
			5,
			response_a1,
		)
		.await;
		get_backable_candidates(
			&mut virtual_overseer,
			&leaf_a,
			2.into(),
			Ancestors::default(),
			5,
			response_a2,
		)
		.await;
		get_backable_candidates(
			&mut virtual_overseer,
			&leaf_b,
			1.into(),
			Ancestors::default(),
			5,
			response_b,
		)
		.await;
		get_backable_candidates(
			&mut virtual_overseer,
			&leaf_c,
			2.into(),
			Ancestors::default(),
			5,
			response_c,
		)
		.await;

		// Check membership on other leaves.
		get_backable_candidates(
			&mut virtual_overseer,
			&leaf_b,
			2.into(),
			Ancestors::default(),
			5,
			vec![],
		)
		.await;

		get_backable_candidates(
			&mut virtual_overseer,
			&leaf_c,
			1.into(),
			Ancestors::default(),
			5,
			vec![],
		)
		.await;

		virtual_overseer
	});

	assert_eq!(view.active_leaves.len(), 3);
}

#[test]
fn introduce_candidate_multiple_times() {
	let test_state = TestState::default();
	let view = test_harness(|mut virtual_overseer| async move {
		// Leaf A
		let leaf_a = TestLeaf {
			number: 100,
			hash: Hash::from_low_u64_be(130),
			para_data: vec![
				(1.into(), PerParaData::new(97, HeadData(vec![1, 2, 3]))),
				(2.into(), PerParaData::new(100, HeadData(vec![2, 3, 4]))),
			],
		};
		// Activate leaves.
		activate_leaf(&mut virtual_overseer, &leaf_a, &test_state).await;

		// Candidate A.
		let (candidate_a, pvd_a) = make_candidate(
			leaf_a.hash,
			leaf_a.number,
			1.into(),
			HeadData(vec![1, 2, 3]),
			HeadData(vec![1]),
			test_state.validation_code_hash,
		);
		let candidate_hash_a = candidate_a.hash();
		let response_a = vec![(candidate_hash_a, leaf_a.hash)];

		// Introduce candidates.
		introduce_seconded_candidate(&mut virtual_overseer, candidate_a.clone(), pvd_a.clone())
			.await;

		// Back candidates. Otherwise, we cannot check membership with GetBackableCandidates.
		back_candidate(&mut virtual_overseer, &candidate_a, candidate_hash_a).await;

		// Check candidate tree membership.
		get_backable_candidates(
			&mut virtual_overseer,
			&leaf_a,
			1.into(),
			Ancestors::default(),
			5,
			response_a.clone(),
		)
		.await;

		// Introduce the same candidate multiple times. It'll return true but it will only be added
		// once.
		for _ in 0..5 {
			introduce_seconded_candidate(&mut virtual_overseer, candidate_a.clone(), pvd_a.clone())
				.await;
		}

		// Check candidate tree membership.
		get_backable_candidates(
			&mut virtual_overseer,
			&leaf_a,
			1.into(),
			Ancestors::default(),
			5,
			response_a,
		)
		.await;

		virtual_overseer
	});

	assert_eq!(view.active_leaves.len(), 1);
}

#[test]
fn fragment_chain_best_chain_length_is_bounded() {
	let test_state = TestState::default();
	let view = test_harness(|mut virtual_overseer| async move {
		// Leaf A
		let leaf_a = TestLeaf {
			number: 100,
			hash: Hash::from_low_u64_be(130),
			para_data: vec![
				(1.into(), PerParaData::new(97, HeadData(vec![1, 2, 3]))),
				(2.into(), PerParaData::new(100, HeadData(vec![2, 3, 4]))),
			],
		};
		// Activate leaves.
		activate_leaf_with_params(
			&mut virtual_overseer,
			&leaf_a,
			&test_state,
			AsyncBackingParams { max_candidate_depth: 1, allowed_ancestry_len: 3 },
		)
		.await;

		// Candidates A, B and C form a chain.
		let (candidate_a, pvd_a) = make_candidate(
			leaf_a.hash,
			leaf_a.number,
			1.into(),
			HeadData(vec![1, 2, 3]),
			HeadData(vec![1]),
			test_state.validation_code_hash,
		);
		let (candidate_b, pvd_b) = make_candidate(
			leaf_a.hash,
			leaf_a.number,
			1.into(),
			HeadData(vec![1]),
			HeadData(vec![2]),
			test_state.validation_code_hash,
		);
		let (candidate_c, pvd_c) = make_candidate(
			leaf_a.hash,
			leaf_a.number,
			1.into(),
			HeadData(vec![2]),
			HeadData(vec![3]),
			test_state.validation_code_hash,
		);

		// Introduce candidates A and B. Since max depth is 1, only these two will be allowed.
		introduce_seconded_candidate(&mut virtual_overseer, candidate_a.clone(), pvd_a).await;
		introduce_seconded_candidate(&mut virtual_overseer, candidate_b.clone(), pvd_b).await;

		// Back candidates. Otherwise, we cannot check membership with GetBackableCandidates and
		// they won't be part of the best chain.
		back_candidate(&mut virtual_overseer, &candidate_a, candidate_a.hash()).await;
		back_candidate(&mut virtual_overseer, &candidate_b, candidate_b.hash()).await;

		// Check candidate tree membership.
		get_backable_candidates(
			&mut virtual_overseer,
			&leaf_a,
			1.into(),
			Ancestors::default(),
			5,
			vec![(candidate_a.hash(), leaf_a.hash), (candidate_b.hash(), leaf_a.hash)],
		)
		.await;

		// Introducing C will not fail. It will be kept as unconnected storage.
		introduce_seconded_candidate(&mut virtual_overseer, candidate_c.clone(), pvd_c).await;
		// When being backed, candidate C will be dropped.
		back_candidate(&mut virtual_overseer, &candidate_c, candidate_c.hash()).await;

		get_backable_candidates(
			&mut virtual_overseer,
			&leaf_a,
			1.into(),
			Ancestors::default(),
			5,
			vec![(candidate_a.hash(), leaf_a.hash), (candidate_b.hash(), leaf_a.hash)],
		)
		.await;

		virtual_overseer
	});

	assert_eq!(view.active_leaves.len(), 1);
}

// Send some candidates, check if the candidate won't be found once its relay parent leaves the
// view.
#[test]
fn introduce_candidate_parent_leaving_view() {
	let test_state = TestState::default();
	let view = test_harness(|mut virtual_overseer| async move {
		// Leaf A
		let leaf_a = TestLeaf {
			number: 100,
			hash: Hash::from_low_u64_be(130),
			para_data: vec![
				(1.into(), PerParaData::new(97, HeadData(vec![1, 2, 3]))),
				(2.into(), PerParaData::new(100, HeadData(vec![2, 3, 4]))),
			],
		};
		// Leaf B
		let leaf_b = TestLeaf {
			number: 101,
			hash: Hash::from_low_u64_be(131),
			para_data: vec![
				(1.into(), PerParaData::new(99, HeadData(vec![3, 4, 5]))),
				(2.into(), PerParaData::new(101, HeadData(vec![4, 5, 6]))),
			],
		};
		// Leaf C
		let leaf_c = TestLeaf {
			number: 102,
			hash: Hash::from_low_u64_be(132),
			para_data: vec![
				(1.into(), PerParaData::new(102, HeadData(vec![5, 6, 7]))),
				(2.into(), PerParaData::new(98, HeadData(vec![6, 7, 8]))),
			],
		};

		// Activate leaves.
		activate_leaf(&mut virtual_overseer, &leaf_a, &test_state).await;
		activate_leaf(&mut virtual_overseer, &leaf_b, &test_state).await;
		activate_leaf(&mut virtual_overseer, &leaf_c, &test_state).await;

		// Candidate A1
		let (candidate_a1, pvd_a1) = make_candidate(
			leaf_a.hash,
			leaf_a.number,
			1.into(),
			HeadData(vec![1, 2, 3]),
			HeadData(vec![1]),
			test_state.validation_code_hash,
		);
		let candidate_hash_a1 = candidate_a1.hash();

		// Candidate A2
		let (candidate_a2, pvd_a2) = make_candidate(
			leaf_a.hash,
			leaf_a.number,
			2.into(),
			HeadData(vec![2, 3, 4]),
			HeadData(vec![2]),
			test_state.validation_code_hash,
		);
		let candidate_hash_a2 = candidate_a2.hash();

		// Candidate B
		let (candidate_b, pvd_b) = make_candidate(
			leaf_b.hash,
			leaf_b.number,
			1.into(),
			HeadData(vec![3, 4, 5]),
			HeadData(vec![3]),
			test_state.validation_code_hash,
		);
		let candidate_hash_b = candidate_b.hash();
		let response_b = vec![(candidate_hash_b, leaf_b.hash)];

		// Candidate C
		let (candidate_c, pvd_c) = make_candidate(
			leaf_c.hash,
			leaf_c.number,
			2.into(),
			HeadData(vec![6, 7, 8]),
			HeadData(vec![4]),
			test_state.validation_code_hash,
		);
		let candidate_hash_c = candidate_c.hash();
		let response_c = vec![(candidate_hash_c, leaf_c.hash)];

		// Introduce candidates.
		introduce_seconded_candidate(&mut virtual_overseer, candidate_a1.clone(), pvd_a1).await;
		introduce_seconded_candidate(&mut virtual_overseer, candidate_a2.clone(), pvd_a2).await;
		introduce_seconded_candidate(&mut virtual_overseer, candidate_b.clone(), pvd_b).await;
		introduce_seconded_candidate(&mut virtual_overseer, candidate_c.clone(), pvd_c).await;

		// Back candidates. Otherwise, we cannot check membership with GetBackableCandidates.
		back_candidate(&mut virtual_overseer, &candidate_a1, candidate_hash_a1).await;
		back_candidate(&mut virtual_overseer, &candidate_a2, candidate_hash_a2).await;
		back_candidate(&mut virtual_overseer, &candidate_b, candidate_hash_b).await;
		back_candidate(&mut virtual_overseer, &candidate_c, candidate_hash_c).await;

		// Deactivate leaf A.
		deactivate_leaf(&mut virtual_overseer, leaf_a.hash).await;

		// Candidates A1 and A2 should be gone. Candidates B and C should remain.
		get_backable_candidates(
			&mut virtual_overseer,
			&leaf_a,
			1.into(),
			Ancestors::default(),
			5,
			vec![],
		)
		.await;
		get_backable_candidates(
			&mut virtual_overseer,
			&leaf_a,
			2.into(),
			Ancestors::default(),
			5,
			vec![],
		)
		.await;
		get_backable_candidates(
			&mut virtual_overseer,
			&leaf_b,
			1.into(),
			Ancestors::default(),
			5,
			response_b,
		)
		.await;
		get_backable_candidates(
			&mut virtual_overseer,
			&leaf_c,
			2.into(),
			Ancestors::default(),
			5,
			response_c.clone(),
		)
		.await;

		// Deactivate leaf B.
		deactivate_leaf(&mut virtual_overseer, leaf_b.hash).await;

		// Candidate B should be gone, C should remain.
		get_backable_candidates(
			&mut virtual_overseer,
			&leaf_a,
			1.into(),
			Ancestors::default(),
			5,
			vec![],
		)
		.await;
		get_backable_candidates(
			&mut virtual_overseer,
			&leaf_a,
			2.into(),
			Ancestors::default(),
			5,
			vec![],
		)
		.await;
		get_backable_candidates(
			&mut virtual_overseer,
			&leaf_b,
			1.into(),
			Ancestors::default(),
			5,
			vec![],
		)
		.await;
		get_backable_candidates(
			&mut virtual_overseer,
			&leaf_c,
			2.into(),
			Ancestors::default(),
			5,
			response_c,
		)
		.await;

		// Deactivate leaf C.
		deactivate_leaf(&mut virtual_overseer, leaf_c.hash).await;

		// Candidate C should be gone.
		get_backable_candidates(
			&mut virtual_overseer,
			&leaf_a,
			1.into(),
			Ancestors::default(),
			5,
			vec![],
		)
		.await;
		get_backable_candidates(
			&mut virtual_overseer,
			&leaf_a,
			2.into(),
			Ancestors::default(),
			5,
			vec![],
		)
		.await;
		get_backable_candidates(
			&mut virtual_overseer,
			&leaf_b,
			1.into(),
			Ancestors::default(),
			5,
			vec![],
		)
		.await;
		get_backable_candidates(
			&mut virtual_overseer,
			&leaf_c,
			2.into(),
			Ancestors::default(),
			5,
			vec![],
		)
		.await;

		virtual_overseer
	});

	assert_eq!(view.active_leaves.len(), 0);
}

// Introduce a candidate to multiple forks, see how the membership is returned.
#[test]
fn introduce_candidate_on_multiple_forks() {
	let test_state = TestState::default();
	let view = test_harness(|mut virtual_overseer| async move {
		// Leaf B
		let leaf_b = TestLeaf {
			number: 101,
			hash: Hash::from_low_u64_be(131),
			para_data: vec![
				(1.into(), PerParaData::new(99, HeadData(vec![1, 2, 3]))),
				(2.into(), PerParaData::new(101, HeadData(vec![4, 5, 6]))),
			],
		};
		// Leaf A
		let leaf_a = TestLeaf {
			number: 100,
			hash: get_parent_hash(leaf_b.hash),
			para_data: vec![
				(1.into(), PerParaData::new(97, HeadData(vec![1, 2, 3]))),
				(2.into(), PerParaData::new(100, HeadData(vec![2, 3, 4]))),
			],
		};

		// Activate leaves.
		activate_leaf(&mut virtual_overseer, &leaf_a, &test_state).await;
		activate_leaf(&mut virtual_overseer, &leaf_b, &test_state).await;

		// Candidate built on leaf A.
		let (candidate_a, pvd_a) = make_candidate(
			leaf_a.hash,
			leaf_a.number,
			1.into(),
			HeadData(vec![1, 2, 3]),
			HeadData(vec![1]),
			test_state.validation_code_hash,
		);
		let candidate_hash_a = candidate_a.hash();
		let response_a = vec![(candidate_hash_a, leaf_a.hash)];

		// Introduce candidate. Should be present on leaves B and C.
		introduce_seconded_candidate(&mut virtual_overseer, candidate_a.clone(), pvd_a).await;
		back_candidate(&mut virtual_overseer, &candidate_a, candidate_hash_a).await;

		// Check candidate tree membership.
		get_backable_candidates(
			&mut virtual_overseer,
			&leaf_a,
			1.into(),
			Ancestors::default(),
			5,
			response_a.clone(),
		)
		.await;
		get_backable_candidates(
			&mut virtual_overseer,
			&leaf_b,
			1.into(),
			Ancestors::default(),
			5,
			response_a.clone(),
		)
		.await;

		virtual_overseer
	});

	assert_eq!(view.active_leaves.len(), 2);
}

#[test]
fn unconnected_candidates_become_connected() {
	// This doesn't test all the complicated cases with many unconnected candidates, as it's more
	// extensively tested in the `fragment_chain::tests` module.
	let test_state = TestState::default();
	let view = test_harness(|mut virtual_overseer| async move {
		// Leaf A
		let leaf_a = TestLeaf {
			number: 100,
			hash: Hash::from_low_u64_be(130),
			para_data: vec![
				(1.into(), PerParaData::new(97, HeadData(vec![1, 2, 3]))),
				(2.into(), PerParaData::new(100, HeadData(vec![2, 3, 4]))),
			],
		};
		// Activate leaves.
		activate_leaf(&mut virtual_overseer, &leaf_a, &test_state).await;

		// Candidates A, B, C and D all form a chain, but we'll first introduce A, C and D.
		let (candidate_a, pvd_a) = make_candidate(
			leaf_a.hash,
			leaf_a.number,
			1.into(),
			HeadData(vec![1, 2, 3]),
			HeadData(vec![1]),
			test_state.validation_code_hash,
		);
		let (candidate_b, pvd_b) = make_candidate(
			leaf_a.hash,
			leaf_a.number,
			1.into(),
			HeadData(vec![1]),
			HeadData(vec![2]),
			test_state.validation_code_hash,
		);
		let (candidate_c, pvd_c) = make_candidate(
			leaf_a.hash,
			leaf_a.number,
			1.into(),
			HeadData(vec![2]),
			HeadData(vec![3]),
			test_state.validation_code_hash,
		);
		let (candidate_d, pvd_d) = make_candidate(
			leaf_a.hash,
			leaf_a.number,
			1.into(),
			HeadData(vec![3]),
			HeadData(vec![4]),
			test_state.validation_code_hash,
		);

		// Introduce candidates A, C and D.
		introduce_seconded_candidate(&mut virtual_overseer, candidate_a.clone(), pvd_a.clone())
			.await;
		introduce_seconded_candidate(&mut virtual_overseer, candidate_c.clone(), pvd_c.clone())
			.await;
		introduce_seconded_candidate(&mut virtual_overseer, candidate_d.clone(), pvd_d.clone())
			.await;

		// Back candidates. Otherwise, we cannot check membership with GetBackableCandidates.
		back_candidate(&mut virtual_overseer, &candidate_a, candidate_a.hash()).await;
		back_candidate(&mut virtual_overseer, &candidate_c, candidate_c.hash()).await;
		back_candidate(&mut virtual_overseer, &candidate_d, candidate_d.hash()).await;

		// Check candidate tree membership. Only A should be returned.
		get_backable_candidates(
			&mut virtual_overseer,
			&leaf_a,
			1.into(),
			Ancestors::default(),
			5,
			vec![(candidate_a.hash(), leaf_a.hash)],
		)
		.await;

		// Introduce C and check membership. Full chain should be returned.
		introduce_seconded_candidate(&mut virtual_overseer, candidate_b.clone(), pvd_b.clone())
			.await;
		back_candidate(&mut virtual_overseer, &candidate_b, candidate_b.hash()).await;
		get_backable_candidates(
			&mut virtual_overseer,
			&leaf_a,
			1.into(),
			Ancestors::default(),
			5,
			vec![
				(candidate_a.hash(), leaf_a.hash),
				(candidate_b.hash(), leaf_a.hash),
				(candidate_c.hash(), leaf_a.hash),
				(candidate_d.hash(), leaf_a.hash),
			],
		)
		.await;

		virtual_overseer
	});

	assert_eq!(view.active_leaves.len(), 1);
}

// Backs some candidates and tests `GetBackableCandidates` when requesting a single candidate.
#[test]
fn check_backable_query_single_candidate() {
	let test_state = TestState::default();
	let view = test_harness(|mut virtual_overseer| async move {
		// Leaf A
		let leaf_a = TestLeaf {
			number: 100,
			hash: Hash::from_low_u64_be(130),
			para_data: vec![
				(1.into(), PerParaData::new(97, HeadData(vec![1, 2, 3]))),
				(2.into(), PerParaData::new(100, HeadData(vec![2, 3, 4]))),
			],
		};

		// Activate leaves.
		activate_leaf(&mut virtual_overseer, &leaf_a, &test_state).await;

		// Candidate A
		let (candidate_a, pvd_a) = make_candidate(
			leaf_a.hash,
			leaf_a.number,
			1.into(),
			HeadData(vec![1, 2, 3]),
			HeadData(vec![1]),
			test_state.validation_code_hash,
		);
		let candidate_hash_a = candidate_a.hash();

		// Candidate B
		let (mut candidate_b, pvd_b) = make_candidate(
			leaf_a.hash,
			leaf_a.number,
			1.into(),
			HeadData(vec![1]),
			HeadData(vec![2]),
			test_state.validation_code_hash,
		);
		// Set a field to make this candidate unique.
		candidate_b.descriptor.set_para_head(Hash::from_low_u64_le(1000));
		let candidate_hash_b = candidate_b.hash();

		// Introduce candidates.
		introduce_seconded_candidate(&mut virtual_overseer, candidate_a.clone(), pvd_a).await;
		introduce_seconded_candidate(&mut virtual_overseer, candidate_b.clone(), pvd_b).await;

		// Should not get any backable candidates.
		get_backable_candidates(
			&mut virtual_overseer,
			&leaf_a,
			1.into(),
			vec![candidate_hash_a].into_iter().collect(),
			1,
			vec![],
		)
		.await;
		get_backable_candidates(
			&mut virtual_overseer,
			&leaf_a,
			1.into(),
			vec![candidate_hash_a].into_iter().collect(),
			0,
			vec![],
		)
		.await;
		get_backable_candidates(
			&mut virtual_overseer,
			&leaf_a,
			1.into(),
			Ancestors::new(),
			0,
			vec![],
		)
		.await;

		// Back candidates.
		back_candidate(&mut virtual_overseer, &candidate_a, candidate_hash_a).await;
		back_candidate(&mut virtual_overseer, &candidate_b, candidate_hash_b).await;

		// Back an unknown candidate. It doesn't return anything but it's ignored. Will not have any
		// effect on the backable candidates.
		back_candidate(&mut virtual_overseer, &candidate_b, CandidateHash(Hash::random())).await;

		// Should not get any backable candidates for the other para.
		get_backable_candidates(
			&mut virtual_overseer,
			&leaf_a,
			2.into(),
			Ancestors::new(),
			1,
			vec![],
		)
		.await;
		get_backable_candidates(
			&mut virtual_overseer,
			&leaf_a,
			2.into(),
			vec![candidate_hash_a].into_iter().collect(),
			1,
			vec![],
		)
		.await;

		// Get backable candidate.
		get_backable_candidates(
			&mut virtual_overseer,
			&leaf_a,
			1.into(),
			Ancestors::new(),
			1,
			vec![(candidate_hash_a, leaf_a.hash)],
		)
		.await;
		get_backable_candidates(
			&mut virtual_overseer,
			&leaf_a,
			1.into(),
			vec![candidate_hash_a].into_iter().collect(),
			1,
			vec![(candidate_hash_b, leaf_a.hash)],
		)
		.await;

		// Wrong path
		get_backable_candidates(
			&mut virtual_overseer,
			&leaf_a,
			1.into(),
			vec![candidate_hash_b].into_iter().collect(),
			1,
			vec![(candidate_hash_a, leaf_a.hash)],
		)
		.await;

		virtual_overseer
	});

	assert_eq!(view.active_leaves.len(), 1);
}

// Backs some candidates and tests `GetBackableCandidates` when requesting a multiple candidates.
#[test]
fn check_backable_query_multiple_candidates() {
	let test_state = TestState::default();
	let view = test_harness(|mut virtual_overseer| async move {
		// Leaf A
		let leaf_a = TestLeaf {
			number: 100,
			hash: Hash::from_low_u64_be(130),
			para_data: vec![
				(1.into(), PerParaData::new(97, HeadData(vec![1, 2, 3]))),
				(2.into(), PerParaData::new(100, HeadData(vec![2, 3, 4]))),
			],
		};

		// Activate leaves.
		activate_leaf(&mut virtual_overseer, &leaf_a, &test_state).await;

		// Candidate A
		let (candidate_a, pvd_a) = make_candidate(
			leaf_a.hash,
			leaf_a.number,
			1.into(),
			HeadData(vec![1, 2, 3]),
			HeadData(vec![1]),
			test_state.validation_code_hash,
		);
		let candidate_hash_a = candidate_a.hash();
		introduce_seconded_candidate(&mut virtual_overseer, candidate_a.clone(), pvd_a).await;
		back_candidate(&mut virtual_overseer, &candidate_a, candidate_hash_a).await;

		let (candidate_b, candidate_hash_b) =
			make_and_back_candidate!(test_state, virtual_overseer, leaf_a, &candidate_a, 2);
		let (candidate_c, candidate_hash_c) =
			make_and_back_candidate!(test_state, virtual_overseer, leaf_a, &candidate_b, 3);
		let (_candidate_d, candidate_hash_d) =
			make_and_back_candidate!(test_state, virtual_overseer, leaf_a, &candidate_c, 4);

		// Should not get any backable candidates for the other para.
		get_backable_candidates(
			&mut virtual_overseer,
			&leaf_a,
			2.into(),
			Ancestors::new(),
			1,
			vec![],
		)
		.await;
		get_backable_candidates(
			&mut virtual_overseer,
			&leaf_a,
			2.into(),
			Ancestors::new(),
			5,
			vec![],
		)
		.await;
		get_backable_candidates(
			&mut virtual_overseer,
			&leaf_a,
			2.into(),
			vec![candidate_hash_a].into_iter().collect(),
			1,
			vec![],
		)
		.await;

		// Test various scenarios with various counts.

		// empty ancestors
		{
			get_backable_candidates(
				&mut virtual_overseer,
				&leaf_a,
				1.into(),
				Ancestors::new(),
				1,
				vec![(candidate_hash_a, leaf_a.hash)],
			)
			.await;
			for count in 4..10 {
				get_backable_candidates(
					&mut virtual_overseer,
					&leaf_a,
					1.into(),
					Ancestors::new(),
					count,
					vec![
						(candidate_hash_a, leaf_a.hash),
						(candidate_hash_b, leaf_a.hash),
						(candidate_hash_c, leaf_a.hash),
						(candidate_hash_d, leaf_a.hash),
					],
				)
				.await;
			}
		}

		// ancestors of size 1
		{
			get_backable_candidates(
				&mut virtual_overseer,
				&leaf_a,
				1.into(),
				vec![candidate_hash_a].into_iter().collect(),
				1,
				vec![(candidate_hash_b, leaf_a.hash)],
			)
			.await;
			get_backable_candidates(
				&mut virtual_overseer,
				&leaf_a,
				1.into(),
				vec![candidate_hash_a].into_iter().collect(),
				2,
				vec![(candidate_hash_b, leaf_a.hash), (candidate_hash_c, leaf_a.hash)],
			)
			.await;

			// If the requested count exceeds the largest chain, return the longest
			// chain we can get.
			for count in 3..10 {
				get_backable_candidates(
					&mut virtual_overseer,
					&leaf_a,
					1.into(),
					vec![candidate_hash_a].into_iter().collect(),
					count,
					vec![
						(candidate_hash_b, leaf_a.hash),
						(candidate_hash_c, leaf_a.hash),
						(candidate_hash_d, leaf_a.hash),
					],
				)
				.await;
			}
		}

		// ancestor count 2 and higher
		{
			get_backable_candidates(
				&mut virtual_overseer,
				&leaf_a,
				1.into(),
				vec![candidate_hash_a, candidate_hash_b, candidate_hash_c].into_iter().collect(),
				1,
				vec![(candidate_hash_d, leaf_a.hash)],
			)
			.await;

			get_backable_candidates(
				&mut virtual_overseer,
				&leaf_a,
				1.into(),
				vec![candidate_hash_a, candidate_hash_b].into_iter().collect(),
				1,
				vec![(candidate_hash_c, leaf_a.hash)],
			)
			.await;

			// If the requested count exceeds the largest chain, return the longest
			// chain we can get.
			for count in 3..10 {
				get_backable_candidates(
					&mut virtual_overseer,
					&leaf_a,
					1.into(),
					vec![candidate_hash_a, candidate_hash_b].into_iter().collect(),
					count,
					vec![(candidate_hash_c, leaf_a.hash), (candidate_hash_d, leaf_a.hash)],
				)
				.await;
			}
		}

		// No more candidates in the chain.
		for count in 1..4 {
			get_backable_candidates(
				&mut virtual_overseer,
				&leaf_a,
				1.into(),
				vec![candidate_hash_a, candidate_hash_b, candidate_hash_c, candidate_hash_d]
					.into_iter()
					.collect(),
				count,
				vec![],
			)
			.await;
		}

		// Wrong paths.
		get_backable_candidates(
			&mut virtual_overseer,
			&leaf_a,
			1.into(),
			vec![candidate_hash_b].into_iter().collect(),
			1,
			vec![(candidate_hash_a, leaf_a.hash)],
		)
		.await;
		get_backable_candidates(
			&mut virtual_overseer,
			&leaf_a,
			1.into(),
			vec![candidate_hash_b, candidate_hash_c].into_iter().collect(),
			3,
			vec![
				(candidate_hash_a, leaf_a.hash),
				(candidate_hash_b, leaf_a.hash),
				(candidate_hash_c, leaf_a.hash),
			],
		)
		.await;

		get_backable_candidates(
			&mut virtual_overseer,
			&leaf_a,
			1.into(),
			vec![candidate_hash_a, candidate_hash_c, candidate_hash_d].into_iter().collect(),
			2,
			vec![(candidate_hash_b, leaf_a.hash), (candidate_hash_c, leaf_a.hash)],
		)
		.await;

		// Non-existent candidate.
		get_backable_candidates(
			&mut virtual_overseer,
			&leaf_a,
			1.into(),
			vec![candidate_hash_a, CandidateHash(Hash::from_low_u64_be(100))]
				.into_iter()
				.collect(),
			2,
			vec![(candidate_hash_b, leaf_a.hash), (candidate_hash_c, leaf_a.hash)],
		)
		.await;

		// Requested count is zero.
		get_backable_candidates(
			&mut virtual_overseer,
			&leaf_a,
			1.into(),
			Ancestors::new(),
			0,
			vec![],
		)
		.await;
		get_backable_candidates(
			&mut virtual_overseer,
			&leaf_a,
			1.into(),
			vec![candidate_hash_a].into_iter().collect(),
			0,
			vec![],
		)
		.await;
		get_backable_candidates(
			&mut virtual_overseer,
			&leaf_a,
			1.into(),
			vec![candidate_hash_a, candidate_hash_b].into_iter().collect(),
			0,
			vec![],
		)
		.await;

		virtual_overseer
	});

	assert_eq!(view.active_leaves.len(), 1);
}

// Test hypothetical membership query.
#[test]
fn check_hypothetical_membership_query() {
	let test_state = TestState::default();
	let view = test_harness(|mut virtual_overseer| async move {
		// Leaf B
		let leaf_b = TestLeaf {
			number: 101,
			hash: Hash::from_low_u64_be(131),
			para_data: vec![
				(1.into(), PerParaData::new(97, HeadData(vec![1, 2, 3]))),
				(2.into(), PerParaData::new(100, HeadData(vec![2, 3, 4]))),
			],
		};
		// Leaf A
		let leaf_a = TestLeaf {
			number: 100,
			hash: get_parent_hash(leaf_b.hash),
			para_data: vec![
				(1.into(), PerParaData::new(98, HeadData(vec![1, 2, 3]))),
				(2.into(), PerParaData::new(100, HeadData(vec![2, 3, 4]))),
			],
		};

		// Activate leaves.
		activate_leaf_with_params(
			&mut virtual_overseer,
			&leaf_a,
			&test_state,
			AsyncBackingParams { allowed_ancestry_len: 3, max_candidate_depth: 1 },
		)
		.await;
		activate_leaf_with_params(
			&mut virtual_overseer,
			&leaf_b,
			&test_state,
			AsyncBackingParams { allowed_ancestry_len: 3, max_candidate_depth: 1 },
		)
		.await;

		// Candidates will be valid on both leaves.

		// Candidate A.
		let (candidate_a, pvd_a) = make_candidate(
			leaf_a.hash,
			leaf_a.number,
			1.into(),
			HeadData(vec![1, 2, 3]),
			HeadData(vec![1]),
			test_state.validation_code_hash,
		);

		// Candidate B.
		let (candidate_b, pvd_b) = make_candidate(
			leaf_a.hash,
			leaf_a.number,
			1.into(),
			HeadData(vec![1]),
			HeadData(vec![2]),
			test_state.validation_code_hash,
		);

		// Candidate C.
		let (candidate_c, pvd_c) = make_candidate(
			leaf_a.hash,
			leaf_a.number,
			1.into(),
			HeadData(vec![2]),
			HeadData(vec![3]),
			test_state.validation_code_hash,
		);

		// Get hypothetical membership of candidates before adding candidate A.
		// Candidate A can be added directly, candidates B and C are potential candidates.
		for (candidate, pvd) in [
			(candidate_a.clone(), pvd_a.clone()),
			(candidate_b.clone(), pvd_b.clone()),
			(candidate_c.clone(), pvd_c.clone()),
		] {
			get_hypothetical_membership(
				&mut virtual_overseer,
				candidate.hash(),
				candidate,
				pvd,
				vec![leaf_a.hash, leaf_b.hash],
			)
			.await;
		}

		// Add candidate A.
		introduce_seconded_candidate(&mut virtual_overseer, candidate_a.clone(), pvd_a.clone())
			.await;

		// Get membership of candidates after adding A. They all are still unconnected candidates
		// (not part of the best backable chain).
		for (candidate, pvd) in [
			(candidate_a.clone(), pvd_a.clone()),
			(candidate_b.clone(), pvd_b.clone()),
			(candidate_c.clone(), pvd_c.clone()),
		] {
			get_hypothetical_membership(
				&mut virtual_overseer,
				candidate.hash(),
				candidate,
				pvd,
				vec![leaf_a.hash, leaf_b.hash],
			)
			.await;
		}

		// Back A. Now A is part of the best chain the rest can be added as unconnected.

		back_candidate(&mut virtual_overseer, &candidate_a, candidate_a.hash()).await;

		for (candidate, pvd) in [
			(candidate_a.clone(), pvd_a.clone()),
			(candidate_b.clone(), pvd_b.clone()),
			(candidate_c.clone(), pvd_c.clone()),
		] {
			get_hypothetical_membership(
				&mut virtual_overseer,
				candidate.hash(),
				candidate,
				pvd,
				vec![leaf_a.hash, leaf_b.hash],
			)
			.await;
		}

		// Candidate D has invalid relay parent.
		let (candidate_d, pvd_d) = make_candidate(
			Hash::from_low_u64_be(200),
			leaf_a.number,
			1.into(),
			HeadData(vec![1]),
			HeadData(vec![2]),
			test_state.validation_code_hash,
		);
		introduce_seconded_candidate_failed(&mut virtual_overseer, candidate_d, pvd_d).await;

		// Add candidate B and back it.
		introduce_seconded_candidate(&mut virtual_overseer, candidate_b.clone(), pvd_b.clone())
			.await;
		back_candidate(&mut virtual_overseer, &candidate_b, candidate_b.hash()).await;

		// Get membership of candidates after adding B.
		for (candidate, pvd) in [
			(candidate_a.clone(), pvd_a.clone()),
			(candidate_b.clone(), pvd_b.clone()),
			(candidate_c.clone(), pvd_c.clone()),
		] {
			get_hypothetical_membership(
				&mut virtual_overseer,
				candidate.hash(),
				candidate,
				pvd,
				vec![leaf_a.hash, leaf_b.hash],
			)
			.await;
		}

		virtual_overseer
	});

	assert_eq!(view.active_leaves.len(), 2);
}

#[test]
fn check_pvd_query() {
	let test_state = TestState::default();
	let view = test_harness(|mut virtual_overseer| async move {
		// Leaf A
		let leaf_a = TestLeaf {
			number: 100,
			hash: Hash::from_low_u64_be(130),
			para_data: vec![
				(1.into(), PerParaData::new(97, HeadData(vec![1, 2, 3]))),
				(2.into(), PerParaData::new(100, HeadData(vec![2, 3, 4]))),
			],
		};

		// Activate leaves.
		activate_leaf(&mut virtual_overseer, &leaf_a, &test_state).await;

		// Candidate A.
		let (candidate_a, pvd_a) = make_candidate(
			leaf_a.hash,
			leaf_a.number,
			1.into(),
			HeadData(vec![1, 2, 3]),
			HeadData(vec![1]),
			test_state.validation_code_hash,
		);

		// Candidate B.
		let (candidate_b, pvd_b) = make_candidate(
			leaf_a.hash,
			leaf_a.number,
			1.into(),
			HeadData(vec![1]),
			HeadData(vec![2]),
			test_state.validation_code_hash,
		);

		// Candidate C.
		let (candidate_c, pvd_c) = make_candidate(
			leaf_a.hash,
			leaf_a.number,
			1.into(),
			HeadData(vec![2]),
			HeadData(vec![3]),
			test_state.validation_code_hash,
		);

		// Candidate E.
		let (candidate_e, pvd_e) = make_candidate(
			leaf_a.hash,
			leaf_a.number,
			1.into(),
			HeadData(vec![5]),
			HeadData(vec![6]),
			test_state.validation_code_hash,
		);

		// Get pvd of candidate A before adding it.
		get_pvd(
			&mut virtual_overseer,
			1.into(),
			leaf_a.hash,
			HeadData(vec![1, 2, 3]),
			Some(pvd_a.clone()),
		)
		.await;

		// Add candidate A.
		introduce_seconded_candidate(&mut virtual_overseer, candidate_a.clone(), pvd_a.clone())
			.await;
		back_candidate(&mut virtual_overseer, &candidate_a, candidate_a.hash()).await;

		// Get pvd of candidate A after adding it.
		get_pvd(
			&mut virtual_overseer,
			1.into(),
			leaf_a.hash,
			HeadData(vec![1, 2, 3]),
			Some(pvd_a.clone()),
		)
		.await;

		// Get pvd of candidate B before adding it.
		get_pvd(
			&mut virtual_overseer,
			1.into(),
			leaf_a.hash,
			HeadData(vec![1]),
			Some(pvd_b.clone()),
		)
		.await;

		// Add candidate B.
		introduce_seconded_candidate(&mut virtual_overseer, candidate_b, pvd_b.clone()).await;

		// Get pvd of candidate B after adding it.
		get_pvd(
			&mut virtual_overseer,
			1.into(),
			leaf_a.hash,
			HeadData(vec![1]),
			Some(pvd_b.clone()),
		)
		.await;

		// Get pvd of candidate C before adding it.
		get_pvd(
			&mut virtual_overseer,
			1.into(),
			leaf_a.hash,
			HeadData(vec![2]),
			Some(pvd_c.clone()),
		)
		.await;

		// Add candidate C.
		introduce_seconded_candidate(&mut virtual_overseer, candidate_c, pvd_c.clone()).await;

		// Get pvd of candidate C after adding it.
		get_pvd(&mut virtual_overseer, 1.into(), leaf_a.hash, HeadData(vec![2]), Some(pvd_c)).await;

		// Get pvd of candidate E before adding it. It won't be found, as we don't have its parent.
		get_pvd(&mut virtual_overseer, 1.into(), leaf_a.hash, HeadData(vec![5]), None).await;

		// Add candidate E and check again. Should succeed this time.
		introduce_seconded_candidate(&mut virtual_overseer, candidate_e, pvd_e.clone()).await;

		get_pvd(&mut virtual_overseer, 1.into(), leaf_a.hash, HeadData(vec![5]), Some(pvd_e)).await;

		virtual_overseer
	});

	assert_eq!(view.active_leaves.len(), 1);
}

// Test simultaneously activating and deactivating leaves, and simultaneously deactivating
// multiple leaves.
// This test is parametrised with the runtime api version. For versions that don't support the claim
// queue API, we check that av-cores are used.
#[rstest]
#[case(RuntimeApiRequest::CLAIM_QUEUE_RUNTIME_REQUIREMENT)]
#[case(8)]
fn correctly_updates_leaves(#[case] runtime_api_version: u32) {
	let mut test_state = TestState::default();
	test_state.set_runtime_api_version(runtime_api_version);

	let view = test_harness(|mut virtual_overseer| async move {
		// Leaf A
		let leaf_a = TestLeaf {
			number: 100,
			hash: Hash::from_low_u64_be(130),
			para_data: vec![
				(1.into(), PerParaData::new(97, HeadData(vec![1, 2, 3]))),
				(2.into(), PerParaData::new(100, HeadData(vec![2, 3, 4]))),
			],
		};
		// Leaf B
		let leaf_b = TestLeaf {
			number: 101,
			hash: Hash::from_low_u64_be(131),
			para_data: vec![
				(1.into(), PerParaData::new(99, HeadData(vec![3, 4, 5]))),
				(2.into(), PerParaData::new(101, HeadData(vec![4, 5, 6]))),
			],
		};
		// Leaf C
		let leaf_c = TestLeaf {
			number: 102,
			hash: Hash::from_low_u64_be(132),
			para_data: vec![
				(1.into(), PerParaData::new(102, HeadData(vec![5, 6, 7]))),
				(2.into(), PerParaData::new(98, HeadData(vec![6, 7, 8]))),
			],
		};

		// Activate leaves.
		activate_leaf(&mut virtual_overseer, &leaf_a, &test_state).await;
		activate_leaf(&mut virtual_overseer, &leaf_b, &test_state).await;

		// Try activating a duplicate leaf.
		activate_leaf(&mut virtual_overseer, &leaf_b, &test_state).await;

		// Pass in an empty update.
		let update = ActiveLeavesUpdate::default();
		virtual_overseer
			.send(FromOrchestra::Signal(OverseerSignal::ActiveLeaves(update)))
			.await;

		// Activate a leaf and remove one at the same time.
		let activated = new_leaf(leaf_c.hash, leaf_c.number);
		let update = ActiveLeavesUpdate {
			activated: Some(activated),
			deactivated: [leaf_b.hash][..].into(),
		};
		virtual_overseer
			.send(FromOrchestra::Signal(OverseerSignal::ActiveLeaves(update)))
			.await;
		handle_leaf_activation(
			&mut virtual_overseer,
			&leaf_c,
			&test_state,
			ASYNC_BACKING_PARAMETERS,
			get_parent_hash,
		)
		.await;

		// Remove all remaining leaves.
		let update = ActiveLeavesUpdate {
			deactivated: [leaf_a.hash, leaf_c.hash][..].into(),
			..Default::default()
		};
		virtual_overseer
			.send(FromOrchestra::Signal(OverseerSignal::ActiveLeaves(update)))
			.await;

		// Activate and deactivate the same leaf.
		let activated = new_leaf(leaf_a.hash, leaf_a.number);
		let update = ActiveLeavesUpdate {
			activated: Some(activated),
			deactivated: [leaf_a.hash][..].into(),
		};
		virtual_overseer
			.send(FromOrchestra::Signal(OverseerSignal::ActiveLeaves(update)))
			.await;

		// Remove the leaf again. Send some unnecessary hashes.
		let update = ActiveLeavesUpdate {
			deactivated: [leaf_a.hash, leaf_b.hash, leaf_c.hash][..].into(),
			..Default::default()
		};
		virtual_overseer
			.send(FromOrchestra::Signal(OverseerSignal::ActiveLeaves(update)))
			.await;

		virtual_overseer
	});

	assert_eq!(view.active_leaves.len(), 0);
}

#[test]
fn handle_active_leaves_update_gets_candidates_from_parent() {
	let para_id = ParaId::from(1);
	let mut test_state = TestState::default();
	test_state.claim_queue = test_state
		.claim_queue
		.into_iter()
		.filter(|(_, paras)| matches!(paras.front(), Some(para) if para == &para_id))
		.collect();
	assert_eq!(test_state.claim_queue.len(), 1);
	let view = test_harness(|mut virtual_overseer| async move {
		// Leaf A
		let leaf_a = TestLeaf {
			number: 100,
			hash: Hash::from_low_u64_be(130),
			para_data: vec![(para_id, PerParaData::new(97, HeadData(vec![1, 2, 3])))],
		};
		// Activate leaf A.
		activate_leaf(&mut virtual_overseer, &leaf_a, &test_state).await;

		// Candidates A, B, C and D all form a chain
		let (candidate_a, pvd_a) = make_candidate(
			leaf_a.hash,
			leaf_a.number,
			para_id,
			HeadData(vec![1, 2, 3]),
			HeadData(vec![1]),
			test_state.validation_code_hash,
		);
		let candidate_hash_a = candidate_a.hash();
		introduce_seconded_candidate(&mut virtual_overseer, candidate_a.clone(), pvd_a).await;
		back_candidate(&mut virtual_overseer, &candidate_a, candidate_hash_a).await;

		let (candidate_b, candidate_hash_b) =
			make_and_back_candidate!(test_state, virtual_overseer, leaf_a, &candidate_a, 2);
		let (candidate_c, candidate_hash_c) =
			make_and_back_candidate!(test_state, virtual_overseer, leaf_a, &candidate_b, 3);
		let (candidate_d, candidate_hash_d) =
			make_and_back_candidate!(test_state, virtual_overseer, leaf_a, &candidate_c, 4);

		let mut all_candidates_resp = vec![
			(candidate_hash_a, leaf_a.hash),
			(candidate_hash_b, leaf_a.hash),
			(candidate_hash_c, leaf_a.hash),
			(candidate_hash_d, leaf_a.hash),
		];

		// Check candidate tree membership.
		get_backable_candidates(
			&mut virtual_overseer,
			&leaf_a,
			para_id,
			Ancestors::default(),
			5,
			all_candidates_resp.clone(),
		)
		.await;

		// Activate leaf B, which makes candidates A and B pending availability.
		// Leaf B
		let leaf_b = TestLeaf {
			number: 101,
			hash: Hash::from_low_u64_be(129),
			para_data: vec![(
				para_id,
				PerParaData::new_with_pending(
					98,
					HeadData(vec![1, 2, 3]),
					vec![
						CandidatePendingAvailability {
							candidate_hash: candidate_a.hash(),
							descriptor: candidate_a.descriptor.clone(),
							commitments: candidate_a.commitments.clone(),
							relay_parent_number: leaf_a.number,
							max_pov_size: MAX_POV_SIZE,
						},
						CandidatePendingAvailability {
							candidate_hash: candidate_b.hash(),
							descriptor: candidate_b.descriptor.clone(),
							commitments: candidate_b.commitments.clone(),
							relay_parent_number: leaf_a.number,
							max_pov_size: MAX_POV_SIZE,
						},
					],
				),
			)],
		};
		// Activate leaf B.
		activate_leaf(&mut virtual_overseer, &leaf_b, &test_state).await;
		get_backable_candidates(
			&mut virtual_overseer,
			&leaf_b,
			para_id,
			Ancestors::default(),
			5,
			vec![],
		)
		.await;

		get_backable_candidates(
			&mut virtual_overseer,
			&leaf_b,
			para_id,
			[candidate_a.hash(), candidate_b.hash()].into_iter().collect(),
			5,
			vec![(candidate_c.hash(), leaf_a.hash), (candidate_d.hash(), leaf_a.hash)],
		)
		.await;

		get_backable_candidates(
			&mut virtual_overseer,
			&leaf_b,
			para_id,
			Ancestors::default(),
			5,
			vec![],
		)
		.await;

		get_backable_candidates(
			&mut virtual_overseer,
			&leaf_a,
			para_id,
			Ancestors::default(),
			5,
			all_candidates_resp.clone(),
		)
		.await;

		// Now deactivate leaf A.
		deactivate_leaf(&mut virtual_overseer, leaf_a.hash).await;

		get_backable_candidates(
			&mut virtual_overseer,
			&leaf_b,
			para_id,
			Ancestors::default(),
			5,
			vec![],
		)
		.await;

		get_backable_candidates(
			&mut virtual_overseer,
			&leaf_b,
			para_id,
			[candidate_a.hash(), candidate_b.hash()].into_iter().collect(),
			5,
			vec![(candidate_c.hash(), leaf_a.hash), (candidate_d.hash(), leaf_a.hash)],
		)
		.await;

		// Now add leaf C, which will be a sibling (fork) of leaf B. It should also inherit the
		// candidates of leaf A (their common parent).
		let leaf_c = TestLeaf {
			number: 101,
			hash: Hash::from_low_u64_be(12),
			para_data: vec![(
				para_id,
				PerParaData::new_with_pending(98, HeadData(vec![1, 2, 3]), vec![]),
			)],
		};

		activate_leaf_with_parent_hash_fn(&mut virtual_overseer, &leaf_c, &test_state, |hash| {
			if hash == leaf_c.hash {
				leaf_a.hash
			} else {
				get_parent_hash(hash)
			}
		})
		.await;

		get_backable_candidates(
			&mut virtual_overseer,
			&leaf_b,
			para_id,
			[candidate_a.hash(), candidate_b.hash()].into_iter().collect(),
			5,
			vec![(candidate_c.hash(), leaf_a.hash), (candidate_d.hash(), leaf_a.hash)],
		)
		.await;

		get_backable_candidates(
			&mut virtual_overseer,
			&leaf_c,
			para_id,
			Ancestors::new(),
			5,
			all_candidates_resp.clone(),
		)
		.await;

		// Deactivate C and add another candidate that will be present on the deactivated parent A.
		// When activating C again it should also get the new candidate. Deactivated leaves are
		// still updated with new candidates.
		deactivate_leaf(&mut virtual_overseer, leaf_c.hash).await;

		let (candidate_e, _) =
			make_and_back_candidate!(test_state, virtual_overseer, leaf_a, &candidate_d, 5);
		activate_leaf_with_parent_hash_fn(&mut virtual_overseer, &leaf_c, &test_state, |hash| {
			if hash == leaf_c.hash {
				leaf_a.hash
			} else {
				get_parent_hash(hash)
			}
		})
		.await;

		get_backable_candidates(
			&mut virtual_overseer,
			&leaf_b,
			para_id,
			[candidate_a.hash(), candidate_b.hash()].into_iter().collect(),
			5,
			vec![
				(candidate_c.hash(), leaf_a.hash),
				(candidate_d.hash(), leaf_a.hash),
				(candidate_e.hash(), leaf_a.hash),
			],
		)
		.await;

		all_candidates_resp.push((candidate_e.hash(), leaf_a.hash));
		get_backable_candidates(
			&mut virtual_overseer,
			&leaf_c,
			para_id,
			Ancestors::new(),
			5,
			all_candidates_resp,
		)
		.await;

		// Querying the backable candidates for deactivated leaf won't work.
		get_backable_candidates(
			&mut virtual_overseer,
			&leaf_a,
			para_id,
			Ancestors::new(),
			5,
			vec![],
		)
		.await;

		virtual_overseer
	});

	assert_eq!(view.active_leaves.len(), 2);
	assert_eq!(view.per_relay_parent.len(), 3);
}

#[test]
fn handle_active_leaves_update_bounded_implicit_view() {
	let para_id = ParaId::from(1);
	let mut test_state = TestState::default();
	test_state.claim_queue = test_state
		.claim_queue
		.into_iter()
		.filter(|(_, paras)| matches!(paras.front(), Some(para) if para == &para_id))
		.collect();
	assert_eq!(test_state.claim_queue.len(), 1);

	let mut leaves = vec![TestLeaf {
		number: 100,
		hash: Hash::from_low_u64_be(130),
		para_data: vec![(
			para_id,
			PerParaData::new(100 - ALLOWED_ANCESTRY_LEN, HeadData(vec![1, 2, 3])),
		)],
	}];

	for index in 1..10 {
		let prev_leaf = &leaves[index - 1];
		leaves.push(TestLeaf {
			number: prev_leaf.number - 1,
			hash: get_parent_hash(prev_leaf.hash),
			para_data: vec![(
				para_id,
				PerParaData::new(
					prev_leaf.number - 1 - ALLOWED_ANCESTRY_LEN,
					HeadData(vec![1, 2, 3]),
				),
			)],
		});
	}
	leaves.reverse();

	let view = test_harness(|mut virtual_overseer| async {
		// Activate first 10 leaves.
		for leaf in &leaves[0..10] {
			activate_leaf(&mut virtual_overseer, leaf, &test_state).await;
		}

		// Now deactivate first 9 leaves.
		for leaf in &leaves[0..9] {
			deactivate_leaf(&mut virtual_overseer, leaf.hash).await;
		}

		virtual_overseer
	});

	// Only latest leaf is active.
	assert_eq!(view.active_leaves.len(), 1);
	// We keep allowed_ancestry_len implicit leaves. The latest leaf is also present here.
	assert_eq!(
		view.per_relay_parent.len() as u32,
		ASYNC_BACKING_PARAMETERS.allowed_ancestry_len + 1
	);

	assert_eq!(view.active_leaves, [leaves[9].hash].into_iter().collect());
	assert_eq!(
		view.per_relay_parent.into_keys().collect::<HashSet<_>>(),
		leaves[6..].into_iter().map(|l| l.hash).collect::<HashSet<_>>()
	);
}

#[test]
fn persists_pending_availability_candidate() {
	let mut test_state = TestState::default();
	let para_id = ParaId::from(1);
	test_state.claim_queue = test_state
		.claim_queue
		.into_iter()
		.filter(|(_, paras)| matches!(paras.front(), Some(para) if para == &para_id))
		.collect();
	assert_eq!(test_state.claim_queue.len(), 1);

	test_harness(|mut virtual_overseer| async move {
		let para_head = HeadData(vec![1, 2, 3]);

		// Min allowed relay parent for leaf `a` which goes out of scope in the test.
		let candidate_relay_parent = Hash::from_low_u64_be(5);
		let candidate_relay_parent_number = 97;

		let leaf_a = TestLeaf {
			number: candidate_relay_parent_number + ALLOWED_ANCESTRY_LEN,
			hash: Hash::from_low_u64_be(2),
			para_data: vec![(
				para_id,
				PerParaData::new(candidate_relay_parent_number, para_head.clone()),
			)],
		};

		let leaf_b_hash = Hash::from_low_u64_be(1);
		let leaf_b_number = leaf_a.number + 1;

		// Activate leaf.
		activate_leaf(&mut virtual_overseer, &leaf_a, &test_state).await;

		// Candidate A
		let (candidate_a, pvd_a) = make_candidate(
			candidate_relay_parent,
			candidate_relay_parent_number,
			para_id,
			para_head.clone(),
			HeadData(vec![1]),
			test_state.validation_code_hash,
		);
		let candidate_hash_a = candidate_a.hash();

		// Candidate B, built on top of the candidate which is out of scope but pending
		// availability.
		let (candidate_b, pvd_b) = make_candidate(
			leaf_b_hash,
			leaf_b_number,
			para_id,
			HeadData(vec![1]),
			HeadData(vec![2]),
			test_state.validation_code_hash,
		);
		let candidate_hash_b = candidate_b.hash();

		introduce_seconded_candidate(&mut virtual_overseer, candidate_a.clone(), pvd_a.clone())
			.await;
		back_candidate(&mut virtual_overseer, &candidate_a, candidate_hash_a).await;

		let candidate_a_pending_av = CandidatePendingAvailability {
			candidate_hash: candidate_hash_a,
			descriptor: candidate_a.descriptor.clone(),
			commitments: candidate_a.commitments.clone(),
			relay_parent_number: candidate_relay_parent_number,
			max_pov_size: MAX_POV_SIZE,
		};
		let leaf_b = TestLeaf {
			number: leaf_b_number,
			hash: leaf_b_hash,
			para_data: vec![(
				1.into(),
				PerParaData::new_with_pending(
					candidate_relay_parent_number + 1,
					para_head.clone(),
					vec![candidate_a_pending_av],
				),
			)],
		};
		activate_leaf(&mut virtual_overseer, &leaf_b, &test_state).await;

		get_hypothetical_membership(
			&mut virtual_overseer,
			candidate_hash_a,
			candidate_a,
			pvd_a,
			vec![leaf_a.hash, leaf_b.hash],
		)
		.await;

		introduce_seconded_candidate(&mut virtual_overseer, candidate_b.clone(), pvd_b).await;
		back_candidate(&mut virtual_overseer, &candidate_b, candidate_hash_b).await;

		get_backable_candidates(
			&mut virtual_overseer,
			&leaf_b,
			para_id,
			vec![candidate_hash_a].into_iter().collect(),
			1,
			vec![(candidate_hash_b, leaf_b_hash)],
		)
		.await;

		virtual_overseer
	});
}

#[test]
fn backwards_compatible_with_non_async_backing_params() {
	let mut test_state = TestState::default();
	let para_id = ParaId::from(1);
	test_state.claim_queue = test_state
		.claim_queue
		.into_iter()
		.filter(|(_, paras)| matches!(paras.front(), Some(para) if para == &para_id))
		.collect();
	assert_eq!(test_state.claim_queue.len(), 1);

	test_harness(|mut virtual_overseer| async move {
		let para_head = HeadData(vec![1, 2, 3]);

		let leaf_b_hash = Hash::repeat_byte(15);
		let candidate_relay_parent = get_parent_hash(leaf_b_hash);
		let candidate_relay_parent_number = 100;

		let leaf_a = TestLeaf {
			number: candidate_relay_parent_number,
			hash: candidate_relay_parent,
			para_data: vec![(
				para_id,
				PerParaData::new(candidate_relay_parent_number, para_head.clone()),
			)],
		};

		// Activate leaf.
		activate_leaf_with_params(
			&mut virtual_overseer,
			&leaf_a,
			&test_state,
			AsyncBackingParams { allowed_ancestry_len: 0, max_candidate_depth: 0 },
		)
		.await;

		// Candidate A
		let (candidate_a, pvd_a) = make_candidate(
			candidate_relay_parent,
			candidate_relay_parent_number,
			para_id,
			para_head.clone(),
			HeadData(vec![1]),
			test_state.validation_code_hash,
		);
		let candidate_hash_a = candidate_a.hash();

		introduce_seconded_candidate(&mut virtual_overseer, candidate_a.clone(), pvd_a).await;
		back_candidate(&mut virtual_overseer, &candidate_a, candidate_hash_a).await;

		get_backable_candidates(
			&mut virtual_overseer,
			&leaf_a,
			para_id,
			Ancestors::new(),
			1,
			vec![(candidate_hash_a, candidate_relay_parent)],
		)
		.await;

		let leaf_b = TestLeaf {
			number: candidate_relay_parent_number + 1,
			hash: leaf_b_hash,
			para_data: vec![(
				para_id,
				PerParaData::new(candidate_relay_parent_number + 1, para_head.clone()),
			)],
		};
		activate_leaf_with_params(
			&mut virtual_overseer,
			&leaf_b,
			&test_state,
			AsyncBackingParams { allowed_ancestry_len: 0, max_candidate_depth: 0 },
		)
		.await;

		get_backable_candidates(
			&mut virtual_overseer,
			&leaf_b,
			para_id,
			Ancestors::new(),
			1,
			vec![],
		)
		.await;

		virtual_overseer
	});
}

#[test]
fn uses_ancestry_only_within_session() {
	test_harness(|mut virtual_overseer| async move {
		let number = 5;
		let hash = Hash::repeat_byte(5);
		let ancestry_len = 3;
		let session = 2;

		let ancestry_hashes =
			vec![Hash::repeat_byte(4), Hash::repeat_byte(3), Hash::repeat_byte(2)];
		let session_change_hash = Hash::repeat_byte(3);

		let activated = new_leaf(hash, number);

		virtual_overseer
			.send(FromOrchestra::Signal(OverseerSignal::ActiveLeaves(
				ActiveLeavesUpdate::start_work(activated),
			)))
			.await;

		assert_matches!(
		virtual_overseer.recv().await,
		AllMessages::RuntimeApi(
			RuntimeApiMessage::Request(
				parent,
				RuntimeApiRequest::AsyncBackingParams(tx)
			)) if parent == hash => {
				tx.send(Ok(AsyncBackingParams { max_candidate_depth: 0, allowed_ancestry_len: ancestry_len})).unwrap();
		});

		assert_matches!(
			virtual_overseer.recv().await,
			AllMessages::RuntimeApi(
				RuntimeApiMessage::Request(parent, RuntimeApiRequest::Version(tx))
			) if parent == hash => {
				tx.send(Ok(RuntimeApiRequest::CLAIM_QUEUE_RUNTIME_REQUIREMENT)).unwrap();
			}
		);

		assert_matches!(
			virtual_overseer.recv().await,
			AllMessages::RuntimeApi(
				RuntimeApiMessage::Request(parent, RuntimeApiRequest::ClaimQueue(tx))
			) if parent == hash => {
				tx.send(Ok(BTreeMap::new())).unwrap();
			}
		);

		send_block_header(&mut virtual_overseer, hash, number).await;

		assert_matches!(
			virtual_overseer.recv().await,
			AllMessages::ChainApi(
				ChainApiMessage::Ancestors{hash: block_hash, k, response_channel: tx}
			) if block_hash == hash && k == ancestry_len as usize => {
				tx.send(Ok(ancestry_hashes.clone())).unwrap();
			}
		);

		assert_matches!(
			virtual_overseer.recv().await,
			AllMessages::RuntimeApi(
				RuntimeApiMessage::Request(parent, RuntimeApiRequest::SessionIndexForChild(tx))
			) if parent == hash => {
				tx.send(Ok(session)).unwrap();
			}
		);

		for (i, hash) in ancestry_hashes.into_iter().enumerate() {
			let number = number - (i + 1) as BlockNumber;
			send_block_header(&mut virtual_overseer, hash, number).await;
			assert_matches!(
				virtual_overseer.recv().await,
				AllMessages::RuntimeApi(
					RuntimeApiMessage::Request(parent, RuntimeApiRequest::SessionIndexForChild(tx))
				) if parent == hash => {
					if hash == session_change_hash {
						tx.send(Ok(session - 1)).unwrap();
						break
					} else {
						tx.send(Ok(session)).unwrap();
					}
				}
			);
		}

		virtual_overseer
	});
}
