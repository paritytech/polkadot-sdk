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
use futures::{
	lock::Mutex,
	task::{Context as FuturesContext, Poll},
	Future,
};
use helpers::activate_new_head;
use polkadot_node_primitives::{BlockData, Collation, CollationResult, MaybeCompressedPoV, PoV};
use polkadot_node_subsystem::{
	errors::RuntimeApiError,
	messages::{AllMessages, RuntimeApiMessage, RuntimeApiRequest},
	ActivatedLeaf,
};
use polkadot_node_subsystem_test_helpers::{subsystem_test_harness, TestSubsystemContextHandle};
use polkadot_node_subsystem_util::TimeoutExt;
use polkadot_primitives::{
	vstaging::async_backing::{BackingState, CandidatePendingAvailability},
	AsyncBackingParams, BlockNumber, CollatorPair, HeadData, PersistedValidationData,
	ScheduledCore, ValidationCode,
};
use polkadot_primitives_test_helpers::{
	dummy_candidate_descriptor_v2, dummy_hash, dummy_head_data, dummy_validator, make_candidate,
};
use rstest::rstest;
use sp_keyring::sr25519::Keyring as Sr25519Keyring;
use std::{
	collections::{BTreeMap, VecDeque},
	pin::Pin,
};

type VirtualOverseer = TestSubsystemContextHandle<CollationGenerationMessage>;

const RUNTIME_VERSION: u32 = RuntimeApiRequest::CLAIM_QUEUE_RUNTIME_REQUIREMENT;

fn test_harness<T: Future<Output = VirtualOverseer>>(test: impl FnOnce(VirtualOverseer) -> T) {
	let pool = sp_core::testing::TaskExecutor::new();
	let (context, virtual_overseer) =
		polkadot_node_subsystem_test_helpers::make_subsystem_context(pool);
	let subsystem = async move {
		let subsystem = crate::CollationGenerationSubsystem::new(Metrics::default());

		subsystem.run(context).await;
	};

	let test_fut = test(virtual_overseer);

	futures::pin_mut!(test_fut);
	futures::executor::block_on(futures::future::join(
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

fn test_collation() -> Collation {
	Collation {
		upward_messages: Default::default(),
		horizontal_messages: Default::default(),
		new_validation_code: None,
		head_data: dummy_head_data(),
		proof_of_validity: MaybeCompressedPoV::Raw(PoV { block_data: BlockData(Vec::new()) }),
		processed_downward_messages: 0_u32,
		hrmp_watermark: 0_u32.into(),
	}
}

struct TestCollator;

impl Future for TestCollator {
	type Output = Option<CollationResult>;

	fn poll(self: Pin<&mut Self>, _cx: &mut FuturesContext) -> Poll<Self::Output> {
		Poll::Ready(Some(CollationResult { collation: test_collation(), result_sender: None }))
	}
}

impl Unpin for TestCollator {}

const TIMEOUT: std::time::Duration = std::time::Duration::from_millis(2000);

async fn overseer_recv(overseer: &mut VirtualOverseer) -> AllMessages {
	overseer
		.recv()
		.timeout(TIMEOUT)
		.await
		.expect(&format!("{:?} is long enough to receive messages", TIMEOUT))
}

fn test_config<Id: Into<ParaId>>(para_id: Id) -> CollationGenerationConfig {
	CollationGenerationConfig {
		key: CollatorPair::generate().0,
		collator: Some(Box::new(|_: Hash, _vd: &PersistedValidationData| TestCollator.boxed())),
		para_id: para_id.into(),
	}
}

fn test_config_no_collator<Id: Into<ParaId>>(para_id: Id) -> CollationGenerationConfig {
	CollationGenerationConfig {
		key: CollatorPair::generate().0,
		collator: None,
		para_id: para_id.into(),
	}
}

fn scheduled_core_for<Id: Into<ParaId>>(para_id: Id) -> ScheduledCore {
	ScheduledCore { para_id: para_id.into(), collator: None }
}

fn dummy_candidate_pending_availability(
	para_id: ParaId,
	candidate_relay_parent: Hash,
	relay_parent_number: BlockNumber,
) -> CandidatePendingAvailability {
	let (candidate, _pvd) = make_candidate(
		candidate_relay_parent,
		relay_parent_number,
		para_id,
		dummy_head_data(),
		HeadData(vec![1]),
		ValidationCode(vec![1, 2, 3]).hash(),
	);
	let candidate_hash = candidate.hash();

	CandidatePendingAvailability {
		candidate_hash,
		descriptor: candidate.descriptor,
		commitments: candidate.commitments,
		relay_parent_number,
		max_pov_size: 5 * 1024 * 1024,
	}
}

fn dummy_backing_state(pending_availability: Vec<CandidatePendingAvailability>) -> BackingState {
	let constraints = helpers::dummy_constraints(
		0,
		vec![0],
		dummy_head_data(),
		ValidationCodeHash::from(Hash::repeat_byte(42)),
	);

	BackingState { constraints, pending_availability }
}

// #[test]
// fn requests_validation_data_for_scheduled_matches() {
// 	let activated_hashes: Vec<Hash> = vec![
// 		Hash::repeat_byte(1),
// 		Hash::repeat_byte(4),
// 		Hash::repeat_byte(9),
// 		Hash::repeat_byte(16),
// 	];

// 	let requested_validation_data = Arc::new(Mutex::new(Vec::new()));

// 	let overseer_requested_validation_data = requested_validation_data.clone();
// 	let overseer = |mut handle: TestSubsystemContextHandle<CollationGenerationMessage>| async move {
// 		loop {
// 			match handle.try_recv().await {
// 				None => break,
// 				Some(AllMessages::RuntimeApi(RuntimeApiMessage::Request(
// 					hash,
// 					RuntimeApiRequest::AvailabilityCores(tx),
// 				))) => {
// 					tx.send(Ok(vec![
// 						CoreState::Free,
// 						// this is weird, see explanation below
// 						CoreState::Scheduled(scheduled_core_for(
// 							(hash.as_fixed_bytes()[0] * 4) as u32,
// 						)),
// 						CoreState::Scheduled(scheduled_core_for(
// 							(hash.as_fixed_bytes()[0] * 5) as u32,
// 						)),
// 					]))
// 					.unwrap();
// 				},
// 				Some(AllMessages::RuntimeApi(RuntimeApiMessage::Request(
// 					hash,
// 					RuntimeApiRequest::PersistedValidationData(
// 						_para_id,
// 						_occupied_core_assumption,
// 						tx,
// 					),
// 				))) => {
// 					overseer_requested_validation_data.lock().await.push(hash);
// 					tx.send(Ok(None)).unwrap();
// 				},
// 				Some(AllMessages::RuntimeApi(RuntimeApiMessage::Request(
// 					_hash,
// 					RuntimeApiRequest::Validators(tx),
// 				))) => {
// 					tx.send(Ok(vec![dummy_validator(); 3])).unwrap();
// 				},
// 				Some(AllMessages::RuntimeApi(RuntimeApiMessage::Request(
// 					_hash,
// 					RuntimeApiRequest::AsyncBackingParams(tx),
// 				))) => {
// 					tx.send(Err(RuntimeApiError::NotSupported {
// 						runtime_api_name: "doesnt_matter",
// 					}))
// 					.unwrap();
// 				},
// 				Some(AllMessages::RuntimeApi(RuntimeApiMessage::Request(
// 					_hash,
// 					RuntimeApiRequest::Version(tx),
// 				))) => {
// 					tx.send(Ok(RUNTIME_VERSION)).unwrap();
// 				},
// 				Some(AllMessages::RuntimeApi(RuntimeApiMessage::Request(
// 					_hash,
// 					RuntimeApiRequest::ClaimQueue(tx),
// 				))) => {
// 					tx.send(Ok(BTreeMap::new())).unwrap();
// 				},
// 				Some(AllMessages::RuntimeApi(RuntimeApiMessage::Request(
// 					_hash,
// 					RuntimeApiRequest::ParaBackingState(_para_id, tx),
// 				))) => {
// 					tx.send(Ok(Some(dummy_backing_state(vec![])))).unwrap();
// 				},
// 				Some(msg) => {
// 					panic!("didn't expect any other overseer requests; got {:?}", msg)
// 				},
// 			}
// 		}
// 	};

// 	for activated_hash in activated_hashes {
// 		test_harness(|mut virtual_overseer| async move {
// 			helpers::activate_new_head(&mut virtual_overseer, activated_hash).await;

// 			assert_matches!(
// 				overseer_recv(&mut virtual_overseer).await,
// 				AllMessages::RuntimeApi(RuntimeApiMessage::Request(_,
// RuntimeApiRequest::SessionIndexForChild(tx))) => { 					tx.send(Ok(1)).unwrap();
// 				}
// 			);

// 			assert_matches!(
// 				overseer_recv(&mut virtual_overseer).await,
// 				AllMessages::RuntimeApi(RuntimeApiMessage::Request(_,
// RuntimeApiRequest::SessionIndexForChild(tx))) => { 					tx.send(Ok(1)).unwrap();
// 				}
// 			);

// 			virtual_overseer
// 		});
// 	}

// 	let requested_validation_data = Arc::try_unwrap(requested_validation_data)
// 		.expect("overseer should have shut down by now")
// 		.into_inner();

// 	// the only activated hash should be from the 4 hash:
// 	// each activated hash generates two scheduled cores: one with its value * 4, one with its value
// 	// * 5 given that the test configuration has a `para_id` of 16, there's only one way to get that
// 	// value: with the 4 hash.
// 	assert_eq!(requested_validation_data, vec![[4; 32].into()]);
// }

#[test]
fn submit_collation_is_no_op_before_initialization() {
	test_harness(|mut virtual_overseer| async move {
		virtual_overseer
			.send(FromOrchestra::Communication {
				msg: CollationGenerationMessage::SubmitCollation(SubmitCollationParams {
					relay_parent: Hash::repeat_byte(0),
					collation: test_collation(),
					parent_head: vec![1, 2, 3].into(),
					validation_code_hash: Hash::repeat_byte(1).into(),
					result_sender: None,
					core_index: CoreIndex(0),
				}),
			})
			.await;

		virtual_overseer
	});
}

#[test]
fn submit_collation_leads_to_distribution() {
	let relay_parent = Hash::repeat_byte(0);
	let validation_code_hash = ValidationCodeHash::from(Hash::repeat_byte(42));
	let parent_head = dummy_head_data();
	let para_id = ParaId::from(5);
	let expected_pvd = PersistedValidationData {
		parent_head: parent_head.clone(),
		relay_parent_number: 10,
		relay_parent_storage_root: Hash::repeat_byte(1),
		max_pov_size: 1024,
	};

	test_harness(|mut virtual_overseer| async move {
		virtual_overseer
			.send(FromOrchestra::Communication {
				msg: CollationGenerationMessage::Initialize(test_config_no_collator(para_id)),
			})
			.await;

		virtual_overseer
			.send(FromOrchestra::Communication {
				msg: CollationGenerationMessage::SubmitCollation(SubmitCollationParams {
					relay_parent,
					collation: test_collation(),
					parent_head: dummy_head_data(),
					validation_code_hash,
					result_sender: None,
					core_index: CoreIndex(0),
				}),
			})
			.await;

		assert_matches!(
			overseer_recv(&mut virtual_overseer).await,
			AllMessages::RuntimeApi(RuntimeApiMessage::Request(rp, RuntimeApiRequest::PersistedValidationData(id, a, tx))) => {
				assert_eq!(rp, relay_parent);
				assert_eq!(id, para_id);
				assert_eq!(a, OccupiedCoreAssumption::TimedOut);

				// Candidate receipt should be constructed with the real parent head.
				let mut pvd = expected_pvd.clone();
				pvd.parent_head = vec![4, 5, 6].into();
				let _ = tx.send(Ok(Some(pvd)));
			}
		);

		assert_matches!(
			overseer_recv(&mut virtual_overseer).await,
			AllMessages::RuntimeApi(RuntimeApiMessage::Request(rp, RuntimeApiRequest::SessionIndexForChild(tx))) => {
				assert_eq!(rp, relay_parent);
				tx.send(Ok(1)).unwrap();
			}
		);

		assert_matches!(
			overseer_recv(&mut virtual_overseer).await,
			AllMessages::RuntimeApi(RuntimeApiMessage::Request(rp, RuntimeApiRequest::Validators(tx))) => {
				assert_eq!(rp, relay_parent);
				let _ = tx.send(Ok(vec![
					Sr25519Keyring::Alice.public().into(),
					Sr25519Keyring::Bob.public().into(),
					Sr25519Keyring::Charlie.public().into(),
				]));
			}
		);

		assert_matches!(
			overseer_recv(&mut virtual_overseer).await,
			AllMessages::RuntimeApi(RuntimeApiMessage::Request(
								rp,
								RuntimeApiRequest::AsyncBackingParams(
									tx,
								),
							)) => {
				assert_eq!(rp, relay_parent);
				let _ = tx.send(Ok(AsyncBackingParams { max_candidate_depth: 1, allowed_ancestry_len: 1 }));
			}
		);

		assert_matches!(
			overseer_recv(&mut virtual_overseer).await,
			AllMessages::RuntimeApi(RuntimeApiMessage::Request(
								rp,
								RuntimeApiRequest::NodeFeatures(session_index, tx),
							)) => {
				assert_eq!(1, session_index);
				assert_eq!(rp, relay_parent);

				// TODO: add the proper variant here.
				let _ = tx.send(Ok(NodeFeatures::EMPTY));
			}
		);

		assert_matches!(
			overseer_recv(&mut virtual_overseer).await,
			AllMessages::CollatorProtocol(CollatorProtocolMessage::DistributeCollation {
				candidate_receipt,
				parent_head_data_hash,
				..
			}) => {
				let CandidateReceipt { descriptor, .. } = candidate_receipt;
				assert_eq!(parent_head_data_hash, parent_head.hash());
				assert_eq!(descriptor.persisted_validation_data_hash(), expected_pvd.hash());
				assert_eq!(descriptor.para_head(), dummy_head_data().hash());
				assert_eq!(descriptor.validation_code_hash(), validation_code_hash);
			}
		);

		virtual_overseer
	});
}

// There is one core in `Occupied` state and async backing is enabled. On new head activation
// `CollationGeneration` should produce and distribute a new collation.
#[test]
fn distribute_collation_for_occupied_core_with_async_backing_enabled() {
	let activated_hash: Hash = [1; 32].into();
	let para_id = ParaId::from(5);

	// One core, in occupied state. The data in `CoreState` and `ClaimQueue` should match.
	let cores: Vec<CoreState> =
		vec![CoreState::Occupied(polkadot_primitives::vstaging::OccupiedCore {
			next_up_on_available: Some(ScheduledCore { para_id, collator: None }),
			occupied_since: 1,
			time_out_at: 10,
			next_up_on_time_out: Some(ScheduledCore { para_id, collator: None }),
			availability: Default::default(), // doesn't matter
			group_responsible: polkadot_primitives::GroupIndex(0),
			candidate_hash: Default::default(),
			candidate_descriptor: dummy_candidate_descriptor_v2(dummy_hash()),
		})];
	let claim_queue = BTreeMap::from([(CoreIndex::from(0), VecDeque::from([para_id]))]);

	test_harness(|mut virtual_overseer| async move {
		helpers::initialize_collator(&mut virtual_overseer, para_id).await;
		helpers::activate_new_head(&mut virtual_overseer, activated_hash).await;

		let pending_availability =
			vec![dummy_candidate_pending_availability(para_id, activated_hash, 1)];
		helpers::handle_runtime_calls_on_new_head_activation(
			&mut virtual_overseer,
			activated_hash,
			AsyncBackingParams { max_candidate_depth: 1, allowed_ancestry_len: 1 },
			cores,
		)
		.await;
		helpers::handle_cores_processing_for_a_leaf(
			&mut virtual_overseer,
			activated_hash,
			para_id,
			// `CoreState` is `Occupied` => `OccupiedCoreAssumption` is `Included`
			OccupiedCoreAssumption::Included,
			1,
			pending_availability,
		)
		.await;

		virtual_overseer
	});
}

// There are variable number of cores in `Occupied` state and async backing is enabled.
// On new head activation `CollationGeneration` should produce and distribute a new collation
// with proper assumption about the para candidate chain availability at next block.
#[rstest]
#[case(0)]
#[case(1)]
#[case(2)]
fn distribute_collation_for_occupied_cores_with_elastic_scaling(
	#[case] candidates_pending_avail: u32,
) {
	let activated_hash: Hash = [1; 32].into();
	let para_id = ParaId::from(5);

	let cores = (0..3)
		.into_iter()
		.map(|idx| {
			CoreState::Occupied(polkadot_primitives::vstaging::OccupiedCore {
				next_up_on_available: Some(ScheduledCore { para_id, collator: None }),
				occupied_since: 0,
				time_out_at: 10,
				next_up_on_time_out: Some(ScheduledCore { para_id, collator: None }),
				availability: Default::default(), // doesn't matter
				group_responsible: polkadot_primitives::GroupIndex(idx as u32),
				candidate_hash: Default::default(),
				candidate_descriptor: dummy_candidate_descriptor_v2(dummy_hash()),
			})
		})
		.collect::<Vec<_>>();

	let pending_availability = (0..candidates_pending_avail)
		.into_iter()
		.map(|_idx| dummy_candidate_pending_availability(para_id, activated_hash, 0))
		.collect::<Vec<_>>();

	let claim_queue = cores
		.iter()
		.enumerate()
		.map(|(idx, _core)| (CoreIndex::from(idx as u32), VecDeque::from([para_id])))
		.collect::<BTreeMap<_, _>>();
	let total_cores = cores.len();

	test_harness(|mut virtual_overseer| async move {
		helpers::initialize_collator(&mut virtual_overseer, para_id).await;
		helpers::activate_new_head(&mut virtual_overseer, activated_hash).await;
		helpers::handle_runtime_calls_on_new_head_activation(
			&mut virtual_overseer,
			activated_hash,
			AsyncBackingParams { max_candidate_depth: 1, allowed_ancestry_len: 1 },
			cores,
		)
		.await;

		helpers::handle_cores_processing_for_a_leaf(
			&mut virtual_overseer,
			activated_hash,
			para_id,
			// if at least 1 cores is occupied => `OccupiedCoreAssumption` is `Included`
			// else assumption is `Free`.
			if candidates_pending_avail > 0 {
				OccupiedCoreAssumption::Included
			} else {
				OccupiedCoreAssumption::Free
			},
			total_cores,
			pending_availability,
		)
		.await;

		virtual_overseer
	});
}

// There are variable number of cores of cores in `Free` state and async backing is enabled.
// On new head activation `CollationGeneration` should produce and distribute a new collation
// with proper assumption about the para candidate chain availability at next block.
#[rstest]
#[case(0)]
#[case(1)]
#[case(2)]
fn distribute_collation_for_free_cores_with_async_backing_enabled_and_elastic_scaling(
	#[case] total_cores: usize,
) {
	let activated_hash: Hash = [1; 32].into();
	let para_id = ParaId::from(5);

	let cores = (0..total_cores)
		.into_iter()
		.map(|_idx| CoreState::Scheduled(ScheduledCore { para_id, collator: None }))
		.collect::<Vec<_>>();

	let claim_queue = cores
		.iter()
		.enumerate()
		.map(|(idx, _core)| (CoreIndex::from(idx as u32), VecDeque::from([para_id])))
		.collect::<BTreeMap<_, _>>();

	test_harness(|mut virtual_overseer| async move {
		helpers::initialize_collator(&mut virtual_overseer, para_id).await;
		helpers::activate_new_head(&mut virtual_overseer, activated_hash).await;
		helpers::handle_runtime_calls_on_new_head_activation(
			&mut virtual_overseer,
			activated_hash,
			AsyncBackingParams { max_candidate_depth: 1, allowed_ancestry_len: 1 },
			cores,
		)
		.await;

		helpers::handle_cores_processing_for_a_leaf(
			&mut virtual_overseer,
			activated_hash,
			para_id,
			// `CoreState` is `Free` => `OccupiedCoreAssumption` is `Free`
			OccupiedCoreAssumption::Free,
			total_cores,
			vec![],
		)
		.await;

		virtual_overseer
	});
}

// There is one core in `Occupied` state and async backing is disabled. On new head activation
// no new collation should be generated.
#[test]
fn no_collation_is_distributed_for_occupied_core_with_async_backing_disabled() {
	let activated_hash: Hash = [1; 32].into();
	let para_id = ParaId::from(5);

	// One core, in occupied state. The data in `CoreState` and `ClaimQueue` should match.
	let cores: Vec<CoreState> =
		vec![CoreState::Occupied(polkadot_primitives::vstaging::OccupiedCore {
			next_up_on_available: Some(ScheduledCore { para_id, collator: None }),
			occupied_since: 1,
			time_out_at: 10,
			next_up_on_time_out: Some(ScheduledCore { para_id, collator: None }),
			availability: Default::default(), // doesn't matter
			group_responsible: polkadot_primitives::GroupIndex(0),
			candidate_hash: Default::default(),
			candidate_descriptor: dummy_candidate_descriptor_v2(dummy_hash()),
		})];
	let claim_queue = BTreeMap::from([(CoreIndex::from(0), VecDeque::from([para_id]))]);

	test_harness(|mut virtual_overseer| async move {
		helpers::initialize_collator(&mut virtual_overseer, para_id).await;
		helpers::activate_new_head(&mut virtual_overseer, activated_hash).await;

		helpers::handle_runtime_calls_on_new_head_activation(
			&mut virtual_overseer,
			activated_hash,
			AsyncBackingParams { max_candidate_depth: 0, allowed_ancestry_len: 0 },
			cores,
		)
		.await;

		virtual_overseer
	});
}

mod helpers {
	use polkadot_primitives::{
		async_backing::{Constraints, InboundHrmpLimitations},
		BlockNumber,
	};

	use super::*;

	// A set for dummy constraints for `ParaBackingState``
	pub(crate) fn dummy_constraints(
		min_relay_parent_number: BlockNumber,
		valid_watermarks: Vec<BlockNumber>,
		required_parent: HeadData,
		validation_code_hash: ValidationCodeHash,
	) -> Constraints {
		Constraints {
			min_relay_parent_number,
			max_pov_size: 5 * 1024 * 1024,
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

	// Sends `Initialize` with a collator config
	pub async fn initialize_collator(virtual_overseer: &mut VirtualOverseer, para_id: ParaId) {
		virtual_overseer
			.send(FromOrchestra::Communication {
				msg: CollationGenerationMessage::Initialize(test_config(para_id)),
			})
			.await;
	}

	// Sends `ActiveLeaves` for a single leaf with the specified hash. Block number is hardcoded.
	pub async fn activate_new_head(virtual_overseer: &mut VirtualOverseer, activated_hash: Hash) {
		virtual_overseer
			.send(FromOrchestra::Signal(OverseerSignal::ActiveLeaves(ActiveLeavesUpdate {
				activated: Some(ActivatedLeaf {
					hash: activated_hash,
					number: 10,
					unpin_handle: polkadot_node_subsystem_test_helpers::mock::dummy_unpin_handle(
						activated_hash,
					),
					span: Arc::new(overseer::jaeger::Span::Disabled),
				}),
				..Default::default()
			})))
			.await;
	}

	// Handle all runtime calls performed in `handle_new_activations`.
	pub async fn handle_runtime_calls_on_new_head_activation(
		virtual_overseer: &mut VirtualOverseer,
		activated_hash: Hash,
		async_backing_params: AsyncBackingParams,
		cores: Vec<CoreState>,
	) {
		assert_matches!(
			overseer_recv(virtual_overseer).await,
			AllMessages::RuntimeApi(RuntimeApiMessage::Request(hash, RuntimeApiRequest::SessionIndexForChild(tx))) => {
				assert_eq!(hash, activated_hash);
				tx.send(Ok(1)).unwrap();
			}
		);

		assert_matches!(
			overseer_recv(virtual_overseer).await,
			AllMessages::RuntimeApi(RuntimeApiMessage::Request(hash, RuntimeApiRequest::Validators(tx))) => {
				assert_eq!(hash, activated_hash);
				let _ = tx.send(Ok(vec![
					Sr25519Keyring::Alice.public().into(),
					Sr25519Keyring::Bob.public().into(),
					Sr25519Keyring::Charlie.public().into(),
				]));
			}
		);

		assert_matches!(
			overseer_recv(virtual_overseer).await,
			AllMessages::RuntimeApi(RuntimeApiMessage::Request(
								hash,
								RuntimeApiRequest::AsyncBackingParams(
									tx,
								),
							)) => {
				assert_eq!(hash, activated_hash);
				let _ = tx.send(Ok(async_backing_params));
			}
		);

		assert_matches!(
			overseer_recv(virtual_overseer).await,
			AllMessages::RuntimeApi(RuntimeApiMessage::Request(
								hash,
								RuntimeApiRequest::NodeFeatures(session_index, tx),
							)) => {
				assert_eq!(1, session_index);
				assert_eq!(hash, activated_hash);

				// TODO: add the proper variant here.
				let _ = tx.send(Ok(NodeFeatures::EMPTY));
			}
		);

		assert_matches!(
			overseer_recv(virtual_overseer).await,
			AllMessages::RuntimeApi(RuntimeApiMessage::Request(hash, RuntimeApiRequest::AvailabilityCores(tx))) => {
				assert_eq!(hash, activated_hash);
				let _ = tx.send(Ok(cores));
			}
		);
	}

	// Handles all runtime requests performed in `handle_new_activations` for the case when a
	// collation should be prepared for the new leaf
	pub async fn handle_cores_processing_for_a_leaf(
		virtual_overseer: &mut VirtualOverseer,
		activated_hash: Hash,
		para_id: ParaId,
		expected_occupied_core_assumption: OccupiedCoreAssumption,
		cores_assigned: usize,
		pending_availability: Vec<CandidatePendingAvailability>,
	) {
		// Expect no messages if no cores is assigned to the para
		if cores_assigned == 0 {
			assert!(overseer_recv(virtual_overseer).timeout(TIMEOUT / 2).await.is_none());
			return
		}

		// Some hardcoded data - if needed, extract to parameters
		let validation_code_hash = ValidationCodeHash::from(Hash::repeat_byte(42));
		let parent_head = dummy_head_data();
		let pvd = PersistedValidationData {
			parent_head: parent_head.clone(),
			relay_parent_number: 10,
			relay_parent_storage_root: Hash::repeat_byte(1),
			max_pov_size: 1024,
		};

		assert_matches!(
			overseer_recv(virtual_overseer).await,
			AllMessages::RuntimeApi(
				RuntimeApiMessage::Request(parent, RuntimeApiRequest::ParaBackingState(p_id, tx))
			) if parent == activated_hash && p_id == para_id => {
				tx.send(Ok(Some(dummy_backing_state(pending_availability)))).unwrap();
			}
		);

		assert_matches!(
			overseer_recv(virtual_overseer).await,
			AllMessages::RuntimeApi(RuntimeApiMessage::Request(hash, RuntimeApiRequest::PersistedValidationData(id, a, tx))) => {
				assert_eq!(hash, activated_hash);
				assert_eq!(id, para_id);
				assert_eq!(a, expected_occupied_core_assumption);

				let _ = tx.send(Ok(Some(pvd.clone())));
			}
		);

		assert_matches!(
			overseer_recv(virtual_overseer).await,
			AllMessages::RuntimeApi(RuntimeApiMessage::Request(
				hash,
				RuntimeApiRequest::ValidationCodeHash(
					id,
					assumption,
					tx,
				),
			)) => {
				assert_eq!(hash, activated_hash);
				assert_eq!(id, para_id);
				assert_eq!(assumption, expected_occupied_core_assumption);

				let _ = tx.send(Ok(Some(validation_code_hash)));
			}
		);

		for _ in 0..cores_assigned {
			assert_matches!(
				overseer_recv(virtual_overseer).await,
				AllMessages::CollatorProtocol(CollatorProtocolMessage::DistributeCollation{
					candidate_receipt,
					parent_head_data_hash,
					..
				}) => {
					assert_eq!(parent_head_data_hash, parent_head.hash());
					assert_eq!(candidate_receipt.descriptor().persisted_validation_data_hash(), pvd.hash());
					assert_eq!(candidate_receipt.descriptor().para_head(), dummy_head_data().hash());
					assert_eq!(candidate_receipt.descriptor().validation_code_hash(), validation_code_hash);
				}
			);
		}
	}
}
