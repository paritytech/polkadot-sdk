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
	task::{Context as FuturesContext, Poll},
	Future, StreamExt,
};
use polkadot_node_primitives::{BlockData, Collation, CollationResult, MaybeCompressedPoV, PoV};
use polkadot_node_subsystem::{
	messages::{AllMessages, RuntimeApiMessage, RuntimeApiRequest},
	ActivatedLeaf,
};
use polkadot_node_subsystem_test_helpers::TestSubsystemContextHandle;
use polkadot_node_subsystem_util::TimeoutExt;
use polkadot_primitives::{
	node_features,
	vstaging::{
		async_backing::{BackingState, CandidatePendingAvailability},
		CandidateDescriptorVersion,
	},
	AsyncBackingParams, BlockNumber, CollatorPair, HeadData, PersistedValidationData,
	ScheduledCore, ValidationCode,
};
use polkadot_primitives_test_helpers::{
	dummy_candidate_descriptor_v2, dummy_hash, dummy_head_data, make_candidate,
};
use rstest::rstest;
use sp_keyring::sr25519::Keyring as Sr25519Keyring;
use std::pin::Pin;

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
			if let Some(msg) = virtual_overseer.rx.next().timeout(TIMEOUT).await {
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

fn node_features_with_v2_enabled() -> NodeFeatures {
	let mut node_features = NodeFeatures::new();
	node_features.resize(node_features::FeatureIndex::CandidateReceiptV2 as usize + 1, false);
	node_features.set(node_features::FeatureIndex::CandidateReceiptV2 as u8 as usize, true);
	node_features
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

		helpers::handle_runtime_calls_on_submit_collation(
			&mut virtual_overseer,
			relay_parent,
			para_id,
			expected_pvd.clone(),
			AsyncBackingParams { max_candidate_depth: 1, allowed_ancestry_len: 1 },
			NodeFeatures::EMPTY,
			None,
		)
		.await;

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

#[test]
fn distribute_collation_only_for_assigned_para_id() {
	let activated_hash: Hash = [1; 32].into();
	let para_id = ParaId::from(5);

	let cores = (0..=5)
		.into_iter()
		.map(|idx| CoreState::Scheduled(scheduled_core_for(ParaId::from(idx as u32))))
		.collect::<Vec<_>>();

	test_harness(|mut virtual_overseer| async move {
		helpers::initialize_collator(&mut virtual_overseer, para_id).await;
		helpers::activate_new_head(&mut virtual_overseer, activated_hash).await;
		helpers::handle_runtime_calls_on_new_head_activation(
			&mut virtual_overseer,
			activated_hash,
			AsyncBackingParams { max_candidate_depth: 1, allowed_ancestry_len: 1 },
			cores,
			NodeFeatures::EMPTY,
		)
		.await;

		helpers::handle_cores_processing_for_a_leaf(
			&mut virtual_overseer,
			activated_hash,
			para_id,
			OccupiedCoreAssumption::Free,
			vec![5], // Only core 5 is assigned to paraid 5.
			vec![],
		)
		.await;

		virtual_overseer
	});
}

#[rstest]
#[case(true)]
#[case(false)]
fn test_candidate_receipt_versioning(#[case] v2_receipts: bool) {
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
	let node_features =
		if v2_receipts { node_features_with_v2_enabled() } else { NodeFeatures::EMPTY };
	let expected_descriptor_version =
		if v2_receipts { CandidateDescriptorVersion::V2 } else { CandidateDescriptorVersion::V1 };

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

		helpers::handle_runtime_calls_on_submit_collation(
			&mut virtual_overseer,
			relay_parent,
			para_id,
			expected_pvd.clone(),
			AsyncBackingParams { max_candidate_depth: 1, allowed_ancestry_len: 1 },
			node_features,
			Some([(CoreIndex(0), [para_id].into_iter().collect())].into_iter().collect()),
		)
		.await;

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
				// Check that the right version was indeed used.
				assert_eq!(descriptor.version(), expected_descriptor_version);
			}
		);

		virtual_overseer
	});
}

#[test]
fn v2_receipts_failed_core_index_check() {
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

		helpers::handle_runtime_calls_on_submit_collation(
			&mut virtual_overseer,
			relay_parent,
			para_id,
			expected_pvd.clone(),
			AsyncBackingParams { max_candidate_depth: 1, allowed_ancestry_len: 1 },
			node_features_with_v2_enabled(),
			// Core index commitment is on core 0 but don't add any assignment for core 0.
			Some([(CoreIndex(1), [para_id].into_iter().collect())].into_iter().collect()),
		)
		.await;

		// No collation is distributed.

		virtual_overseer
	});
}

// There is one core in `Occupied` state and async backing is enabled. On new head activation
// `CollationGeneration` should produce and distribute a new collation.
#[test]
fn distribute_collation_for_occupied_core_with_async_backing_enabled() {
	let activated_hash: Hash = [1; 32].into();
	let para_id = ParaId::from(5);

	// One core, in occupied state.
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
			NodeFeatures::EMPTY,
		)
		.await;
		helpers::handle_cores_processing_for_a_leaf(
			&mut virtual_overseer,
			activated_hash,
			para_id,
			// `CoreState` is `Occupied` => `OccupiedCoreAssumption` is `Included`
			OccupiedCoreAssumption::Included,
			vec![0],
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

	test_harness(|mut virtual_overseer| async move {
		helpers::initialize_collator(&mut virtual_overseer, para_id).await;
		helpers::activate_new_head(&mut virtual_overseer, activated_hash).await;
		helpers::handle_runtime_calls_on_new_head_activation(
			&mut virtual_overseer,
			activated_hash,
			AsyncBackingParams { max_candidate_depth: 1, allowed_ancestry_len: 1 },
			cores,
			NodeFeatures::EMPTY,
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
			vec![0, 1, 2],
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

	test_harness(|mut virtual_overseer| async move {
		helpers::initialize_collator(&mut virtual_overseer, para_id).await;
		helpers::activate_new_head(&mut virtual_overseer, activated_hash).await;
		helpers::handle_runtime_calls_on_new_head_activation(
			&mut virtual_overseer,
			activated_hash,
			AsyncBackingParams { max_candidate_depth: 1, allowed_ancestry_len: 1 },
			cores,
			NodeFeatures::EMPTY,
		)
		.await;

		helpers::handle_cores_processing_for_a_leaf(
			&mut virtual_overseer,
			activated_hash,
			para_id,
			// `CoreState` is `Free` => `OccupiedCoreAssumption` is `Free`
			OccupiedCoreAssumption::Free,
			(0..(total_cores as u32)).collect(),
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

	// One core, in occupied state.
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

	test_harness(|mut virtual_overseer| async move {
		helpers::initialize_collator(&mut virtual_overseer, para_id).await;
		helpers::activate_new_head(&mut virtual_overseer, activated_hash).await;

		helpers::handle_runtime_calls_on_new_head_activation(
			&mut virtual_overseer,
			activated_hash,
			AsyncBackingParams { max_candidate_depth: 0, allowed_ancestry_len: 0 },
			cores,
			NodeFeatures::EMPTY,
		)
		.await;

		virtual_overseer
	});
}

mod helpers {
	use std::collections::{BTreeMap, VecDeque};

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

	// Handle all runtime calls performed in `handle_new_activation`.
	pub async fn handle_runtime_calls_on_new_head_activation(
		virtual_overseer: &mut VirtualOverseer,
		activated_hash: Hash,
		async_backing_params: AsyncBackingParams,
		cores: Vec<CoreState>,
		node_features: NodeFeatures,
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
				tx.send(Ok(vec![
					Sr25519Keyring::Alice.public().into(),
					Sr25519Keyring::Bob.public().into(),
					Sr25519Keyring::Charlie.public().into(),
				])).unwrap();
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
				tx.send(Ok(async_backing_params)).unwrap();
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

				tx.send(Ok(node_features)).unwrap();
			}
		);

		assert_matches!(
			overseer_recv(virtual_overseer).await,
			AllMessages::RuntimeApi(RuntimeApiMessage::Request(hash, RuntimeApiRequest::AvailabilityCores(tx))) => {
				assert_eq!(hash, activated_hash);
				tx.send(Ok(cores)).unwrap();
			}
		);
	}

	// Handles all runtime requests performed in `handle_new_activation` for the case when a
	// collation should be prepared for the new leaf
	pub async fn handle_cores_processing_for_a_leaf(
		virtual_overseer: &mut VirtualOverseer,
		activated_hash: Hash,
		para_id: ParaId,
		expected_occupied_core_assumption: OccupiedCoreAssumption,
		cores_assigned: Vec<u32>,
		pending_availability: Vec<CandidatePendingAvailability>,
	) {
		// Expect no messages if no cores is assigned to the para
		if cores_assigned.is_empty() {
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

		for core in cores_assigned {
			assert_matches!(
				overseer_recv(virtual_overseer).await,
				AllMessages::CollatorProtocol(CollatorProtocolMessage::DistributeCollation{
					candidate_receipt,
					parent_head_data_hash,
					core_index,
					..
				}) => {
					assert_eq!(CoreIndex(core), core_index);
					assert_eq!(parent_head_data_hash, parent_head.hash());
					assert_eq!(candidate_receipt.descriptor().persisted_validation_data_hash(), pvd.hash());
					assert_eq!(candidate_receipt.descriptor().para_head(), dummy_head_data().hash());
					assert_eq!(candidate_receipt.descriptor().validation_code_hash(), validation_code_hash);
				}
			);
		}
	}

	// Handles all runtime requests performed in `handle_submit_collation`
	pub async fn handle_runtime_calls_on_submit_collation(
		virtual_overseer: &mut VirtualOverseer,
		relay_parent: Hash,
		para_id: ParaId,
		expected_pvd: PersistedValidationData,
		async_backing_params: AsyncBackingParams,
		node_features: NodeFeatures,
		claim_queue: Option<BTreeMap<CoreIndex, VecDeque<ParaId>>>,
	) {
		assert_matches!(
			overseer_recv(virtual_overseer).await,
			AllMessages::RuntimeApi(RuntimeApiMessage::Request(rp, RuntimeApiRequest::PersistedValidationData(id, a, tx))) => {
				assert_eq!(rp, relay_parent);
				assert_eq!(id, para_id);
				assert_eq!(a, OccupiedCoreAssumption::TimedOut);

				tx.send(Ok(Some(expected_pvd))).unwrap();
			}
		);

		assert_matches!(
			overseer_recv(virtual_overseer).await,
			AllMessages::RuntimeApi(RuntimeApiMessage::Request(rp, RuntimeApiRequest::SessionIndexForChild(tx))) => {
				assert_eq!(rp, relay_parent);
				tx.send(Ok(1)).unwrap();
			}
		);

		assert_matches!(
			overseer_recv(virtual_overseer).await,
			AllMessages::RuntimeApi(RuntimeApiMessage::Request(rp, RuntimeApiRequest::Validators(tx))) => {
				assert_eq!(rp, relay_parent);
				tx.send(Ok(vec![
					Sr25519Keyring::Alice.public().into(),
					Sr25519Keyring::Bob.public().into(),
					Sr25519Keyring::Charlie.public().into(),
				])).unwrap();
			}
		);

		assert_matches!(
			overseer_recv(virtual_overseer).await,
			AllMessages::RuntimeApi(RuntimeApiMessage::Request(
				rp,
				RuntimeApiRequest::AsyncBackingParams(
					tx,
				),
			)) => {
				assert_eq!(rp, relay_parent);
				tx.send(Ok(async_backing_params)).unwrap();
			}
		);

		assert_matches!(
			overseer_recv(virtual_overseer).await,
			AllMessages::RuntimeApi(RuntimeApiMessage::Request(
				rp,
				RuntimeApiRequest::NodeFeatures(session_index, tx),
			)) => {
				assert_eq!(1, session_index);
				assert_eq!(rp, relay_parent);

				tx.send(Ok(node_features.clone())).unwrap();
			}
		);

		if let Some(true) = node_features
			.get(node_features::FeatureIndex::CandidateReceiptV2 as usize)
			.as_deref()
		{
			assert_matches!(
				overseer_recv(virtual_overseer).await,
				AllMessages::RuntimeApi(RuntimeApiMessage::Request(
					rp,
					RuntimeApiRequest::Version(tx),
				)) => {
					assert_eq!(rp, relay_parent);
					tx.send(Ok(RUNTIME_VERSION)).unwrap();
				}
			);
			assert_matches!(
				overseer_recv(virtual_overseer).await,
				AllMessages::RuntimeApi(RuntimeApiMessage::Request(
					rp,
					RuntimeApiRequest::ClaimQueue(tx),
				)) => {
					assert_eq!(rp, relay_parent);
					tx.send(Ok(claim_queue.expect("Claim queue must be passed in for v2 receipts"))).unwrap();
				}
			);
		}
	}
}
