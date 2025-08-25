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

use futures::FutureExt;
use std::future::Future;

use polkadot_node_network_protocol::request_response::ReqProtocolNames;
use polkadot_node_primitives::{BlockData, ErasureChunk, PoV};
use polkadot_node_subsystem_util::runtime::RuntimeInfo;
use polkadot_primitives::{
	vstaging::{BackedCandidate, CommittedCandidateReceiptV2, CoreState, MutateDescriptorV2},
	BlockNumber, ChunkIndex, CoreIndex, ExecutorParams, GroupIndex, GroupRotationInfo, Hash,
	Id as ParaId, ScheduledCore, SessionIndex, SessionInfo,
};
use sp_core::{testing::TaskExecutor, traits::SpawnNamed};

use polkadot_node_subsystem::{
	messages::{
		AllMessages, AvailabilityDistributionMessage, AvailabilityStoreMessage,
		CandidateBackingMessage, ChainApiMessage, NetworkBridgeTxMessage,
		ProspectiveParachainsMessage, RuntimeApiMessage, RuntimeApiRequest,
	},
	ActiveLeavesUpdate, SpawnGlue,
};
use polkadot_node_subsystem_test_helpers::{
	make_subsystem_context,
	mock::{make_ferdie_keystore, new_leaf},
	TestSubsystemContext, TestSubsystemContextHandle,
};

use crate::tests::{
	mock::{get_valid_chunk_data, make_session_info, OccupiedCoreBuilder},
	node_features_with_mapping_enabled,
};

use super::Requester;
use polkadot_primitives_test_helpers::dummy_committed_candidate_receipt_v2;

fn get_erasure_chunk() -> ErasureChunk {
	let pov = PoV { block_data: BlockData(vec![45, 46, 47]) };
	get_valid_chunk_data(pov, 10, ChunkIndex(0)).1
}

#[derive(Clone)]
struct TestState {
	// Store prepared backed candidates by their hash to serve CandidateBacking requests
	pub backed_map: std::collections::HashMap<
		polkadot_primitives::CandidateHash,
		polkadot_primitives::vstaging::BackedCandidate,
	>,
	/// Simulated relay chain heads. For each block except genesis
	/// there exists a single corresponding candidate, handled in [`spawn_virtual_overseer`].
	pub relay_chain: Vec<Hash>,
	pub session_info: SessionInfo,
	// Defines a way to compute a session index for the block with
	// a given number. Returns 1 for all blocks by default.
	pub session_index_for_block: fn(BlockNumber) -> SessionIndex,
}

impl TestState {
	fn new() -> Self {
		let relay_chain: Vec<_> = (0u8..10).map(Hash::repeat_byte).collect();
		let session_info = make_session_info();
		let session_index_for_block = |_| 1;
		Self { relay_chain, session_info, session_index_for_block, backed_map: Default::default() }
	}
}

fn spawn_virtual_overseer(
	pool: TaskExecutor,
	mut test_state: TestState,
	mut ctx_handle: TestSubsystemContextHandle<AvailabilityDistributionMessage>,
) {
	pool.spawn(
		"virtual-overseer",
		None,
		async move {
			loop {
				let msg = ctx_handle.try_recv().await;
				if msg.is_none() {
					break;
				}
				match msg.unwrap() {
					AllMessages::NetworkBridgeTx(NetworkBridgeTxMessage::SendRequests(..)) => {},
					AllMessages::AvailabilityStore(AvailabilityStoreMessage::QueryChunk(
						..,
						tx,
					)) => {
						let chunk = get_erasure_chunk();
						tx.send(Some(chunk)).expect("Receiver is expected to be alive");
					},
					AllMessages::AvailabilityStore(AvailabilityStoreMessage::StoreChunk {
						tx,
						..
					}) => {
						// Silently accept it.
						tx.send(Ok(())).expect("Receiver is expected to be alive");
					},
					AllMessages::RuntimeApi(RuntimeApiMessage::Request(hash, req)) => {
						match req {
							RuntimeApiRequest::SessionIndexForChild(tx) => {
								let chain = &test_state.relay_chain;
								let block_number = chain
									.iter()
									.position(|h| *h == hash)
									.expect("Invalid session index request");
								// Compute session index.
								let session_index_for_block = test_state.session_index_for_block;

								tx.send(Ok(session_index_for_block(block_number as u32 + 1)))
									.expect("Receiver should still be alive");
							},
							RuntimeApiRequest::SessionInfo(_, tx) => {
								tx.send(Ok(Some(test_state.session_info.clone())))
									.expect("Receiver should be alive.");
							},
							RuntimeApiRequest::SessionExecutorParams(_, tx) => {
								tx.send(Ok(Some(ExecutorParams::default())))
									.expect("Receiver should be alive.");
							},
							RuntimeApiRequest::NodeFeatures(_, tx) => {
								tx.send(Ok(node_features_with_mapping_enabled()))
									.expect("Receiver should be alive.");
							},
							RuntimeApiRequest::AvailabilityCores(tx) => {
								let para_id = ParaId::from(1_u32);
								let maybe_block_position =
									test_state.relay_chain.iter().position(|h| *h == hash);
								let cores = match maybe_block_position {
									Some(block_num) => {
										let core = if block_num == 0 {
											CoreState::Scheduled(ScheduledCore {
												para_id,
												collator: None,
											})
										} else {
											CoreState::Occupied(
												OccupiedCoreBuilder {
													group_responsible: GroupIndex(1),
													para_id,
													relay_parent: hash,
													n_validators: 10,
													chunk_index: ChunkIndex(0),
												}
												.build()
												.0,
											)
										};
										vec![core]
									},
									None => Vec::new(),
								};
								tx.send(Ok(cores)).expect("Receiver should be alive.")
							},
							RuntimeApiRequest::ValidatorGroups(tx) => {
								let groups = test_state.session_info.validator_groups.to_vec();
								let group_rotation_info = GroupRotationInfo {
									session_start_block: 1,
									group_rotation_frequency: 12,
									now: 1,
								};
								tx.send(Ok((groups, group_rotation_info)))
									.expect("Receiver should be alive.");
							},
							_ => {
								panic!("Unexpected runtime request: {:?}", req);
							},
						}
					},
					AllMessages::ChainApi(ChainApiMessage::Ancestors {
						hash,
						k,
						response_channel,
					}) => {
						let chain = &test_state.relay_chain;
						let maybe_block_position = chain.iter().position(|h| *h == hash);
						let ancestors = maybe_block_position
							.map(|idx| chain[..idx].iter().rev().take(k).copied().collect())
							.unwrap_or_default();
						response_channel
							.send(Ok(ancestors))
							.expect("Receiver is expected to be alive");
					},
					AllMessages::CandidateBacking(
						CandidateBackingMessage::GetBackableCandidates(hashes, tx),
					) => {
						let mut resp: std::collections::HashMap<ParaId, Vec<BackedCandidate>> =
							Default::default();
						for (para, list) in hashes.into_iter() {
							let mut v = Vec::new();
							for (cand_hash, _relay_parent) in list {
								if let Some(bc) = test_state.backed_map.get(&cand_hash) {
									v.push(bc.clone());
								}
							}
							resp.insert(para, v);
						}
						tx.send(resp).expect("Receiver should be alive");
					},
					AllMessages::ProspectiveParachains(
						ProspectiveParachainsMessage::GetBackableCandidates(
							relay_parent,
							para,
							count,
							_ancestors,
							tx,
						),
					) => {
						// Create `count` dummy candidates for this para and store them for backing
						let mut list = Vec::new();
						for _ in 0..count {
							let mut receipt: CommittedCandidateReceiptV2<Hash> =
								dummy_committed_candidate_receipt_v2(relay_parent);
							receipt.descriptor.set_para_id(para);
							receipt.descriptor.set_core_index(CoreIndex(0));
							let backed = BackedCandidate::new(
								receipt.clone(),
								Vec::new(),
								Default::default(),
								CoreIndex(0),
							);
							let cand_hash = backed.candidate().hash();
							list.push((cand_hash, relay_parent));
							test_state.backed_map.insert(cand_hash, backed);
						}
						tx.send(list).expect("Receiver should be alive");
					},
					msg => panic!("Unexpected overseer message: {:?}", msg),
				}
			}
		}
		.boxed(),
	);
}

fn test_harness<T: Future<Output = ()>>(
	test_state: TestState,
	test_fx: impl FnOnce(
		TestSubsystemContext<AvailabilityDistributionMessage, SpawnGlue<TaskExecutor>>,
	) -> T,
) {
	let pool = TaskExecutor::new();
	let (ctx, ctx_handle) = make_subsystem_context(pool.clone());

	spawn_virtual_overseer(pool, test_state, ctx_handle);

	futures::executor::block_on(test_fx(ctx));
}

#[test]
fn check_ancestry_lookup_in_same_session() {
	let test_state = TestState::new();
	let mut requester =
		Requester::new(ReqProtocolNames::new(&Hash::repeat_byte(0xff), None), Default::default());
	let keystore = make_ferdie_keystore();
	let mut runtime = RuntimeInfo::new(Some(keystore));

	test_harness(test_state.clone(), |mut ctx| async move {
		let chain = &test_state.relay_chain;
		let block_number = 1;
		let update = ActiveLeavesUpdate {
			activated: Some(new_leaf(chain[block_number], block_number as u32)),
			deactivated: Vec::new().into(),
		};

		requester
			.update_fetching_heads(&mut ctx, &mut runtime, update)
			.await
			.expect("Leaf processing failed");
		let fetch_tasks = &requester.fetches;
		assert_eq!(fetch_tasks.len(), 1);
		let block_1_candidate =
			*fetch_tasks.keys().next().expect("A task is checked to be present; qed");

		let block_number = 2;
		let update = ActiveLeavesUpdate {
			activated: Some(new_leaf(chain[block_number], block_number as u32)),
			deactivated: Vec::new().into(),
		};

		requester
			.update_fetching_heads(&mut ctx, &mut runtime, update)
			.await
			.expect("Leaf processing failed");
		let fetch_tasks = &requester.fetches;
		assert_eq!(fetch_tasks.len(), 2);
		let task = fetch_tasks.get(&block_1_candidate).expect("Leaf hasn't been deactivated yet");
		// The task should be live in both blocks 1 and 2.
		assert_eq!(task.live_in.len(), 2);
		let block_2_candidate = *fetch_tasks
			.keys()
			.find(|hash| **hash != block_1_candidate)
			.expect("Two tasks are present, the first one corresponds to block 1 candidate; qed");

		// Deactivate both blocks but keep the second task as a
		// part of ancestry.
		let block_number = 2 + Requester::LEAF_ANCESTRY_LEN_WITHIN_SESSION;
		let update = ActiveLeavesUpdate {
			activated: Some(new_leaf(chain[block_number], block_number as u32)),
			deactivated: vec![chain[1], chain[2]].into(),
		};
		requester
			.update_fetching_heads(&mut ctx, &mut runtime, update)
			.await
			.expect("Leaf processing failed");
		let fetch_tasks = &requester.fetches;
		// The leaf + K its ancestors.
		assert_eq!(fetch_tasks.len(), Requester::LEAF_ANCESTRY_LEN_WITHIN_SESSION + 1);

		let block_2_task = fetch_tasks
			.get(&block_2_candidate)
			.expect("Expected to be live as a part of ancestry");
		assert_eq!(block_2_task.live_in.len(), 1);
	});
}

#[test]
fn check_ancestry_lookup_in_different_sessions() {
	let mut test_state = TestState::new();
	let mut requester =
		Requester::new(ReqProtocolNames::new(&Hash::repeat_byte(0xff), None), Default::default());
	let keystore = make_ferdie_keystore();
	let mut runtime = RuntimeInfo::new(Some(keystore));

	test_state.session_index_for_block = |block_number| match block_number {
		0..=3 => 1,
		_ => 2,
	};

	test_harness(test_state.clone(), |mut ctx| async move {
		let chain = &test_state.relay_chain;
		let block_number = 3;
		let update = ActiveLeavesUpdate {
			activated: Some(new_leaf(chain[block_number], block_number as u32)),
			deactivated: Vec::new().into(),
		};

		requester
			.update_fetching_heads(&mut ctx, &mut runtime, update)
			.await
			.expect("Leaf processing failed");
		let fetch_tasks = &requester.fetches;
		assert_eq!(fetch_tasks.len(), 3.min(Requester::LEAF_ANCESTRY_LEN_WITHIN_SESSION + 1));

		let block_number = 4;
		let update = ActiveLeavesUpdate {
			activated: Some(new_leaf(chain[block_number], block_number as u32)),
			deactivated: vec![chain[1], chain[2], chain[3]].into(),
		};

		requester
			.update_fetching_heads(&mut ctx, &mut runtime, update)
			.await
			.expect("Leaf processing failed");
		let fetch_tasks = &requester.fetches;
		assert_eq!(fetch_tasks.len(), 1);

		let block_number = 5;
		let update = ActiveLeavesUpdate {
			activated: Some(new_leaf(chain[block_number], block_number as u32)),
			deactivated: vec![chain[4]].into(),
		};

		requester
			.update_fetching_heads(&mut ctx, &mut runtime, update)
			.await
			.expect("Leaf processing failed");
		let fetch_tasks = &requester.fetches;
		assert_eq!(fetch_tasks.len(), 2.min(Requester::LEAF_ANCESTRY_LEN_WITHIN_SESSION + 1));
	});
}

#[test]
fn schedule_chunk_prefetch_creates_fetch_task() {
	let test_state = TestState::new();
	let mut requester =
		Requester::new(ReqProtocolNames::new(&Hash::repeat_byte(0xfe), None), Default::default());
	let keystore = make_ferdie_keystore();
	let mut runtime = RuntimeInfo::new(Some(keystore));

	test_harness(test_state.clone(), |mut ctx| async move {
		let chain = &test_state.relay_chain;
		let block_number = 0; // ensures AvailabilityCores returns Scheduled core at index 0
		let update = ActiveLeavesUpdate {
			activated: Some(new_leaf(chain[block_number], block_number as u32)),
			deactivated: Vec::new().into(),
		};

		requester
			.update_fetching_heads(&mut ctx, &mut runtime, update)
			.await
			.expect("Leaf processing failed");

		let fetch_tasks = &requester.fetches;
		assert_eq!(fetch_tasks.len(), 1, "expected exactly one prefetch fetch task");
	});
}
