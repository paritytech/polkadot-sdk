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
use polkadot_node_primitives::{BlockData, Collation, CollationResult, MaybeCompressedPoV, PoV};
use polkadot_node_subsystem::{
	errors::RuntimeApiError,
	messages::{AllMessages, RuntimeApiMessage, RuntimeApiRequest},
	ActivatedLeaf,
};
use polkadot_node_subsystem_test_helpers::{subsystem_test_harness, TestSubsystemContextHandle};
use polkadot_node_subsystem_util::TimeoutExt;
use polkadot_primitives::{
	async_backing::{BackingState, CandidatePendingAvailability},
	AsyncBackingParams, BlockNumber, CollatorPair, HeadData, PersistedValidationData,
	ScheduledCore, ValidationCode,
};
use rstest::rstest;
use sp_keyring::sr25519::Keyring as Sr25519Keyring;
use std::{
	collections::{BTreeMap, VecDeque},
	pin::Pin,
};
use test_helpers::{
	dummy_candidate_descriptor, dummy_hash, dummy_head_data, dummy_validator, make_candidate,
};

type VirtualOverseer = TestSubsystemContextHandle<CollationGenerationMessage>;

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

fn test_collation_compressed() -> Collation {
	let mut collation = test_collation();
	let compressed = collation.proof_of_validity.clone().into_compressed();
	collation.proof_of_validity = MaybeCompressedPoV::Compressed(compressed);
	collation
}

fn test_validation_data() -> PersistedValidationData {
	let mut persisted_validation_data = PersistedValidationData::default();
	persisted_validation_data.max_pov_size = 1024;
	persisted_validation_data
}

// Box<dyn Future<Output = Collation> + Unpin + Send
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

#[rstest]
#[case(RuntimeApiRequest::CLAIM_QUEUE_RUNTIME_REQUIREMENT - 1)]
#[case(RuntimeApiRequest::CLAIM_QUEUE_RUNTIME_REQUIREMENT)]
fn requests_availability_per_relay_parent(#[case] runtime_version: u32) {
	let activated_hashes: Vec<Hash> =
		vec![[1; 32].into(), [4; 32].into(), [9; 32].into(), [16; 32].into()];

	let requested_availability_cores = Arc::new(Mutex::new(Vec::new()));

	let overseer_requested_availability_cores = requested_availability_cores.clone();
	let overseer = |mut handle: TestSubsystemContextHandle<CollationGenerationMessage>| async move {
		loop {
			match handle.try_recv().await {
				None => break,
				Some(AllMessages::RuntimeApi(RuntimeApiMessage::Request(hash, RuntimeApiRequest::AvailabilityCores(tx)))) => {
					overseer_requested_availability_cores.lock().await.push(hash);
					tx.send(Ok(vec![])).unwrap();
				}
				Some(AllMessages::RuntimeApi(RuntimeApiMessage::Request(_hash, RuntimeApiRequest::Validators(tx)))) => {
					tx.send(Ok(vec![dummy_validator(); 3])).unwrap();
				}
				Some(AllMessages::RuntimeApi(RuntimeApiMessage::Request(
					_hash,
					RuntimeApiRequest::AsyncBackingParams(
						tx,
					),
				))) => {
					tx.send(Err(RuntimeApiError::NotSupported { runtime_api_name: "doesnt_matter" })).unwrap();
				},
				Some(AllMessages::RuntimeApi(RuntimeApiMessage::Request(
					_hash,
					RuntimeApiRequest::Version(tx),
				))) => {
					tx.send(Ok(runtime_version)).unwrap();
				},
				Some(AllMessages::RuntimeApi(RuntimeApiMessage::Request(
					_hash,
					RuntimeApiRequest::ClaimQueue(tx),
				))) if runtime_version >= RuntimeApiRequest::CLAIM_QUEUE_RUNTIME_REQUIREMENT => {
					tx.send(Ok(BTreeMap::new())).unwrap();
				},
				Some(AllMessages::RuntimeApi(RuntimeApiMessage::Request(
					_hash,
					RuntimeApiRequest::ParaBackingState(_para_id, tx),
				))) => {
					tx.send(Ok(Some(dummy_backing_state(vec![])))).unwrap();
				},
				Some(msg) => panic!("didn't expect any other overseer requests given no availability cores; got {:?}", msg),
			}
		}
	};

	let subsystem_activated_hashes = activated_hashes.clone();
	subsystem_test_harness(overseer, |mut ctx| async move {
		handle_new_activations(
			Arc::new(test_config(123u32)),
			subsystem_activated_hashes,
			&mut ctx,
			Metrics(None),
		)
		.await
		.unwrap();
	});

	let mut requested_availability_cores = Arc::try_unwrap(requested_availability_cores)
		.expect("overseer should have shut down by now")
		.into_inner();
	requested_availability_cores.sort();

	assert_eq!(requested_availability_cores, activated_hashes);
}

#[rstest]
#[case(RuntimeApiRequest::CLAIM_QUEUE_RUNTIME_REQUIREMENT - 1)]
#[case(RuntimeApiRequest::CLAIM_QUEUE_RUNTIME_REQUIREMENT)]
fn requests_validation_data_for_scheduled_matches(#[case] runtime_version: u32) {
	let activated_hashes: Vec<Hash> = vec![
		Hash::repeat_byte(1),
		Hash::repeat_byte(4),
		Hash::repeat_byte(9),
		Hash::repeat_byte(16),
	];

	let requested_validation_data = Arc::new(Mutex::new(Vec::new()));

	let overseer_requested_validation_data = requested_validation_data.clone();
	let overseer = |mut handle: TestSubsystemContextHandle<CollationGenerationMessage>| async move {
		loop {
			match handle.try_recv().await {
				None => break,
				Some(AllMessages::RuntimeApi(RuntimeApiMessage::Request(
					hash,
					RuntimeApiRequest::AvailabilityCores(tx),
				))) => {
					tx.send(Ok(vec![
						CoreState::Free,
						// this is weird, see explanation below
						CoreState::Scheduled(scheduled_core_for(
							(hash.as_fixed_bytes()[0] * 4) as u32,
						)),
						CoreState::Scheduled(scheduled_core_for(
							(hash.as_fixed_bytes()[0] * 5) as u32,
						)),
					]))
					.unwrap();
				},
				Some(AllMessages::RuntimeApi(RuntimeApiMessage::Request(
					hash,
					RuntimeApiRequest::PersistedValidationData(
						_para_id,
						_occupied_core_assumption,
						tx,
					),
				))) => {
					overseer_requested_validation_data.lock().await.push(hash);
					tx.send(Ok(None)).unwrap();
				},
				Some(AllMessages::RuntimeApi(RuntimeApiMessage::Request(
					_hash,
					RuntimeApiRequest::Validators(tx),
				))) => {
					tx.send(Ok(vec![dummy_validator(); 3])).unwrap();
				},
				Some(AllMessages::RuntimeApi(RuntimeApiMessage::Request(
					_hash,
					RuntimeApiRequest::AsyncBackingParams(tx),
				))) => {
					tx.send(Err(RuntimeApiError::NotSupported {
						runtime_api_name: "doesnt_matter",
					}))
					.unwrap();
				},
				Some(AllMessages::RuntimeApi(RuntimeApiMessage::Request(
					_hash,
					RuntimeApiRequest::Version(tx),
				))) => {
					tx.send(Ok(runtime_version)).unwrap();
				},
				Some(AllMessages::RuntimeApi(RuntimeApiMessage::Request(
					_hash,
					RuntimeApiRequest::ClaimQueue(tx),
				))) if runtime_version >= RuntimeApiRequest::CLAIM_QUEUE_RUNTIME_REQUIREMENT => {
					tx.send(Ok(BTreeMap::new())).unwrap();
				},
				Some(AllMessages::RuntimeApi(RuntimeApiMessage::Request(
					_hash,
					RuntimeApiRequest::ParaBackingState(_para_id, tx),
				))) => {
					tx.send(Ok(Some(dummy_backing_state(vec![])))).unwrap();
				},
				Some(msg) => {
					panic!("didn't expect any other overseer requests; got {:?}", msg)
				},
			}
		}
	};

	subsystem_test_harness(overseer, |mut ctx| async move {
		handle_new_activations(
			Arc::new(test_config(16)),
			activated_hashes,
			&mut ctx,
			Metrics(None),
		)
		.await
		.unwrap();
	});

	let requested_validation_data = Arc::try_unwrap(requested_validation_data)
		.expect("overseer should have shut down by now")
		.into_inner();

	// the only activated hash should be from the 4 hash:
	// each activated hash generates two scheduled cores: one with its value * 4, one with its value
	// * 5 given that the test configuration has a `para_id` of 16, there's only one way to get that
	// value: with the 4 hash.
	assert_eq!(requested_validation_data, vec![[4; 32].into()]);
}

#[rstest]
#[case(RuntimeApiRequest::CLAIM_QUEUE_RUNTIME_REQUIREMENT - 1)]
#[case(RuntimeApiRequest::CLAIM_QUEUE_RUNTIME_REQUIREMENT)]
fn sends_distribute_collation_message(#[case] runtime_version: u32) {
	let activated_hashes: Vec<Hash> = vec![
		Hash::repeat_byte(1),
		Hash::repeat_byte(4),
		Hash::repeat_byte(9),
		Hash::repeat_byte(16),
	];

	// empty vec doesn't allocate on the heap, so it's ok we throw it away
	let to_collator_protocol = Arc::new(Mutex::new(Vec::new()));
	let inner_to_collator_protocol = to_collator_protocol.clone();

	let overseer = |mut handle: TestSubsystemContextHandle<CollationGenerationMessage>| async move {
		loop {
			match handle.try_recv().await {
				None => break,
				Some(AllMessages::RuntimeApi(RuntimeApiMessage::Request(
					hash,
					RuntimeApiRequest::AvailabilityCores(tx),
				))) => {
					tx.send(Ok(vec![
						CoreState::Free,
						// this is weird, see explanation below
						CoreState::Scheduled(scheduled_core_for(
							(hash.as_fixed_bytes()[0] * 4) as u32,
						)),
						CoreState::Scheduled(scheduled_core_for(
							(hash.as_fixed_bytes()[0] * 5) as u32,
						)),
					]))
					.unwrap();
				},
				Some(AllMessages::RuntimeApi(RuntimeApiMessage::Request(
					_hash,
					RuntimeApiRequest::PersistedValidationData(
						_para_id,
						_occupied_core_assumption,
						tx,
					),
				))) => {
					tx.send(Ok(Some(test_validation_data()))).unwrap();
				},
				Some(AllMessages::RuntimeApi(RuntimeApiMessage::Request(
					_hash,
					RuntimeApiRequest::Validators(tx),
				))) => {
					tx.send(Ok(vec![dummy_validator(); 3])).unwrap();
				},
				Some(AllMessages::RuntimeApi(RuntimeApiMessage::Request(
					_hash,
					RuntimeApiRequest::ValidationCodeHash(
						_para_id,
						OccupiedCoreAssumption::Free,
						tx,
					),
				))) => {
					tx.send(Ok(Some(ValidationCode(vec![1, 2, 3]).hash()))).unwrap();
				},
				Some(AllMessages::RuntimeApi(RuntimeApiMessage::Request(
					_hash,
					RuntimeApiRequest::AsyncBackingParams(tx),
				))) => {
					tx.send(Err(RuntimeApiError::NotSupported {
						runtime_api_name: "doesnt_matter",
					}))
					.unwrap();
				},
				Some(AllMessages::RuntimeApi(RuntimeApiMessage::Request(
					_hash,
					RuntimeApiRequest::Version(tx),
				))) => {
					tx.send(Ok(runtime_version)).unwrap();
				},
				Some(AllMessages::RuntimeApi(RuntimeApiMessage::Request(
					_hash,
					RuntimeApiRequest::ClaimQueue(tx),
				))) if runtime_version >= RuntimeApiRequest::CLAIM_QUEUE_RUNTIME_REQUIREMENT => {
					tx.send(Ok(BTreeMap::new())).unwrap();
				},
				Some(AllMessages::RuntimeApi(RuntimeApiMessage::Request(
					_hash,
					RuntimeApiRequest::ParaBackingState(_para_id, tx),
				))) => {
					tx.send(Ok(Some(dummy_backing_state(vec![])))).unwrap();
				},
				Some(msg @ AllMessages::CollatorProtocol(_)) => {
					inner_to_collator_protocol.lock().await.push(msg);
				},
				Some(msg) => {
					panic!("didn't expect any other overseer requests; got {:?}", msg)
				},
			}
		}
	};

	let config = Arc::new(test_config(16));
	let subsystem_config = config.clone();

	subsystem_test_harness(overseer, |mut ctx| async move {
		handle_new_activations(subsystem_config, activated_hashes, &mut ctx, Metrics(None))
			.await
			.unwrap();
	});

	let mut to_collator_protocol = Arc::try_unwrap(to_collator_protocol)
		.expect("subsystem should have shut down by now")
		.into_inner();

	// we expect a single message to be sent, containing a candidate receipt.
	// we don't care too much about the `commitments_hash` right now, but let's ensure that we've
	// calculated the correct descriptor
	let expect_pov_hash = test_collation_compressed().proof_of_validity.into_compressed().hash();
	let expect_validation_data_hash = test_validation_data().hash();
	let expect_relay_parent = Hash::repeat_byte(4);
	let expect_validation_code_hash = ValidationCode(vec![1, 2, 3]).hash();
	let expect_payload = collator_signature_payload(
		&expect_relay_parent,
		&config.para_id,
		&expect_validation_data_hash,
		&expect_pov_hash,
		&expect_validation_code_hash,
	);
	let expect_descriptor = CandidateDescriptor {
		signature: config.key.sign(&expect_payload),
		para_id: config.para_id,
		relay_parent: expect_relay_parent,
		collator: config.key.public(),
		persisted_validation_data_hash: expect_validation_data_hash,
		pov_hash: expect_pov_hash,
		erasure_root: dummy_hash(), // this isn't something we're checking right now
		para_head: test_collation().head_data.hash(),
		validation_code_hash: expect_validation_code_hash,
	};

	assert_eq!(to_collator_protocol.len(), 1);
	match AllMessages::from(to_collator_protocol.pop().unwrap()) {
		AllMessages::CollatorProtocol(CollatorProtocolMessage::DistributeCollation {
			candidate_receipt,
			..
		}) => {
			let CandidateReceipt { descriptor, .. } = candidate_receipt;
			// signature generation is non-deterministic, so we can't just assert that the
			// expected descriptor is correct. What we can do is validate that the produced
			// descriptor has a valid signature, then just copy in the generated signature
			// and check the rest of the fields for equality.
			assert!(CollatorPair::verify(
				&descriptor.signature,
				&collator_signature_payload(
					&descriptor.relay_parent,
					&descriptor.para_id,
					&descriptor.persisted_validation_data_hash,
					&descriptor.pov_hash,
					&descriptor.validation_code_hash,
				)
				.as_ref(),
				&descriptor.collator,
			));
			let expect_descriptor = {
				let mut expect_descriptor = expect_descriptor;
				expect_descriptor.signature = descriptor.signature.clone();
				expect_descriptor.erasure_root = descriptor.erasure_root;
				expect_descriptor
			};
			assert_eq!(descriptor, expect_descriptor);
		},
		_ => panic!("received wrong message type"),
	}
}

#[rstest]
#[case(RuntimeApiRequest::CLAIM_QUEUE_RUNTIME_REQUIREMENT - 1)]
#[case(RuntimeApiRequest::CLAIM_QUEUE_RUNTIME_REQUIREMENT)]
fn fallback_when_no_validation_code_hash_api(#[case] runtime_version: u32) {
	// This is a variant of the above test, but with the validation code hash API disabled.

	let activated_hashes: Vec<Hash> = vec![
		Hash::repeat_byte(1),
		Hash::repeat_byte(4),
		Hash::repeat_byte(9),
		Hash::repeat_byte(16),
	];

	// empty vec doesn't allocate on the heap, so it's ok we throw it away
	let to_collator_protocol = Arc::new(Mutex::new(Vec::new()));
	let inner_to_collator_protocol = to_collator_protocol.clone();

	let overseer = |mut handle: TestSubsystemContextHandle<CollationGenerationMessage>| async move {
		loop {
			match handle.try_recv().await {
				None => break,
				Some(AllMessages::RuntimeApi(RuntimeApiMessage::Request(
					hash,
					RuntimeApiRequest::AvailabilityCores(tx),
				))) => {
					tx.send(Ok(vec![
						CoreState::Free,
						CoreState::Scheduled(scheduled_core_for(
							(hash.as_fixed_bytes()[0] * 4) as u32,
						)),
						CoreState::Scheduled(scheduled_core_for(
							(hash.as_fixed_bytes()[0] * 5) as u32,
						)),
					]))
					.unwrap();
				},
				Some(AllMessages::RuntimeApi(RuntimeApiMessage::Request(
					_hash,
					RuntimeApiRequest::PersistedValidationData(
						_para_id,
						_occupied_core_assumption,
						tx,
					),
				))) => {
					tx.send(Ok(Some(test_validation_data()))).unwrap();
				},
				Some(AllMessages::RuntimeApi(RuntimeApiMessage::Request(
					_hash,
					RuntimeApiRequest::Validators(tx),
				))) => {
					tx.send(Ok(vec![dummy_validator(); 3])).unwrap();
				},
				Some(AllMessages::RuntimeApi(RuntimeApiMessage::Request(
					_hash,
					RuntimeApiRequest::ValidationCodeHash(
						_para_id,
						OccupiedCoreAssumption::Free,
						tx,
					),
				))) => {
					tx.send(Err(RuntimeApiError::NotSupported {
						runtime_api_name: "validation_code_hash",
					}))
					.unwrap();
				},
				Some(AllMessages::RuntimeApi(RuntimeApiMessage::Request(
					_hash,
					RuntimeApiRequest::ValidationCode(_para_id, OccupiedCoreAssumption::Free, tx),
				))) => {
					tx.send(Ok(Some(ValidationCode(vec![1, 2, 3])))).unwrap();
				},
				Some(AllMessages::RuntimeApi(RuntimeApiMessage::Request(
					_hash,
					RuntimeApiRequest::AsyncBackingParams(tx),
				))) => {
					tx.send(Err(RuntimeApiError::NotSupported {
						runtime_api_name: "doesnt_matter",
					}))
					.unwrap();
				},
				Some(AllMessages::RuntimeApi(RuntimeApiMessage::Request(
					_hash,
					RuntimeApiRequest::Version(tx),
				))) => {
					tx.send(Ok(runtime_version)).unwrap();
				},
				Some(msg @ AllMessages::CollatorProtocol(_)) => {
					inner_to_collator_protocol.lock().await.push(msg);
				},
				Some(AllMessages::RuntimeApi(RuntimeApiMessage::Request(
					_hash,
					RuntimeApiRequest::ClaimQueue(tx),
				))) if runtime_version >= RuntimeApiRequest::CLAIM_QUEUE_RUNTIME_REQUIREMENT => {
					tx.send(Ok(Default::default())).unwrap();
				},
				Some(AllMessages::RuntimeApi(RuntimeApiMessage::Request(
					_hash,
					RuntimeApiRequest::ParaBackingState(_para_id, tx),
				))) => {
					tx.send(Ok(Some(dummy_backing_state(vec![])))).unwrap();
				},
				Some(msg) => {
					panic!("didn't expect any other overseer requests; got {:?}", msg)
				},
			}
		}
	};

	let config = Arc::new(test_config(16u32));
	let subsystem_config = config.clone();

	// empty vec doesn't allocate on the heap, so it's ok we throw it away
	subsystem_test_harness(overseer, |mut ctx| async move {
		handle_new_activations(subsystem_config, activated_hashes, &mut ctx, Metrics(None))
			.await
			.unwrap();
	});

	let to_collator_protocol = Arc::try_unwrap(to_collator_protocol)
		.expect("subsystem should have shut down by now")
		.into_inner();

	let expect_validation_code_hash = ValidationCode(vec![1, 2, 3]).hash();

	assert_eq!(to_collator_protocol.len(), 1);
	match &to_collator_protocol[0] {
		AllMessages::CollatorProtocol(CollatorProtocolMessage::DistributeCollation {
			candidate_receipt,
			..
		}) => {
			let CandidateReceipt { descriptor, .. } = candidate_receipt;
			assert_eq!(expect_validation_code_hash, descriptor.validation_code_hash);
		},
		_ => panic!("received wrong message type"),
	}
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
			AllMessages::CollatorProtocol(CollatorProtocolMessage::DistributeCollation {
				candidate_receipt,
				parent_head_data_hash,
				..
			}) => {
				let CandidateReceipt { descriptor, .. } = candidate_receipt;
				assert_eq!(parent_head_data_hash, parent_head.hash());
				assert_eq!(descriptor.persisted_validation_data_hash, expected_pvd.hash());
				assert_eq!(descriptor.para_head, dummy_head_data().hash());
				assert_eq!(descriptor.validation_code_hash, validation_code_hash);
			}
		);

		virtual_overseer
	});
}

// There is one core in `Occupied` state and async backing is enabled. On new head activation
// `CollationGeneration` should produce and distribute a new collation.
#[rstest]
#[case(RuntimeApiRequest::CLAIM_QUEUE_RUNTIME_REQUIREMENT - 1)]
#[case(RuntimeApiRequest::CLAIM_QUEUE_RUNTIME_REQUIREMENT)]
fn distribute_collation_for_occupied_core_with_async_backing_enabled(#[case] runtime_version: u32) {
	let activated_hash: Hash = [1; 32].into();
	let para_id = ParaId::from(5);

	// One core, in occupied state. The data in `CoreState` and `ClaimQueue` should match.
	let cores: Vec<CoreState> = vec![CoreState::Occupied(polkadot_primitives::OccupiedCore {
		next_up_on_available: Some(ScheduledCore { para_id, collator: None }),
		occupied_since: 1,
		time_out_at: 10,
		next_up_on_time_out: Some(ScheduledCore { para_id, collator: None }),
		availability: Default::default(), // doesn't matter
		group_responsible: polkadot_primitives::GroupIndex(0),
		candidate_hash: Default::default(),
		candidate_descriptor: dummy_candidate_descriptor(dummy_hash()),
	})];
	let claim_queue = BTreeMap::from([(CoreIndex::from(0), VecDeque::from([para_id]))]).into();

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
			runtime_version,
			claim_queue,
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

// There are variable number of cores of cores in `Occupied` state and async backing is enabled.
// On new head activation `CollationGeneration` should produce and distribute a new collation
// with proper assumption about the para candidate chain availability at next block.
#[rstest]
#[case(0)]
#[case(1)]
#[case(2)]
fn distribute_collation_for_occupied_cores_with_async_backing_enabled_and_elastic_scaling(
	#[case] candidates_pending_avail: u32,
) {
	let activated_hash: Hash = [1; 32].into();
	let para_id = ParaId::from(5);

	let cores = (0..3)
		.into_iter()
		.map(|idx| {
			CoreState::Occupied(polkadot_primitives::OccupiedCore {
				next_up_on_available: Some(ScheduledCore { para_id, collator: None }),
				occupied_since: 0,
				time_out_at: 10,
				next_up_on_time_out: Some(ScheduledCore { para_id, collator: None }),
				availability: Default::default(), // doesn't matter
				group_responsible: polkadot_primitives::GroupIndex(idx as u32),
				candidate_hash: Default::default(),
				candidate_descriptor: dummy_candidate_descriptor(dummy_hash()),
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
			// Using latest runtime with the fancy claim queue exposed.
			RuntimeApiRequest::CLAIM_QUEUE_RUNTIME_REQUIREMENT,
			claim_queue,
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
			// Using latest runtime with the fancy claim queue exposed.
			RuntimeApiRequest::CLAIM_QUEUE_RUNTIME_REQUIREMENT,
			claim_queue,
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
#[rstest]
#[case(RuntimeApiRequest::CLAIM_QUEUE_RUNTIME_REQUIREMENT - 1)]
#[case(RuntimeApiRequest::CLAIM_QUEUE_RUNTIME_REQUIREMENT)]
fn no_collation_is_distributed_for_occupied_core_with_async_backing_disabled(
	#[case] runtime_version: u32,
) {
	let activated_hash: Hash = [1; 32].into();
	let para_id = ParaId::from(5);

	// One core, in occupied state. The data in `CoreState` and `ClaimQueue` should match.
	let cores: Vec<CoreState> = vec![CoreState::Occupied(polkadot_primitives::OccupiedCore {
		next_up_on_available: Some(ScheduledCore { para_id, collator: None }),
		occupied_since: 1,
		time_out_at: 10,
		next_up_on_time_out: Some(ScheduledCore { para_id, collator: None }),
		availability: Default::default(), // doesn't matter
		group_responsible: polkadot_primitives::GroupIndex(0),
		candidate_hash: Default::default(),
		candidate_descriptor: dummy_candidate_descriptor(dummy_hash()),
	})];
	let claim_queue = BTreeMap::from([(CoreIndex::from(0), VecDeque::from([para_id]))]).into();

	test_harness(|mut virtual_overseer| async move {
		helpers::initialize_collator(&mut virtual_overseer, para_id).await;
		helpers::activate_new_head(&mut virtual_overseer, activated_hash).await;

		helpers::handle_runtime_calls_on_new_head_activation(
			&mut virtual_overseer,
			activated_hash,
			AsyncBackingParams { max_candidate_depth: 0, allowed_ancestry_len: 0 },
			cores,
			runtime_version,
			claim_queue,
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

	// Handle all runtime calls performed in `handle_new_activations`. Conditionally expects a
	// `CLAIM_QUEUE_RUNTIME_REQUIREMENT` call if the passed `runtime_version` is greater or equal to
	// `CLAIM_QUEUE_RUNTIME_REQUIREMENT`
	pub async fn handle_runtime_calls_on_new_head_activation(
		virtual_overseer: &mut VirtualOverseer,
		activated_hash: Hash,
		async_backing_params: AsyncBackingParams,
		cores: Vec<CoreState>,
		runtime_version: u32,
		claim_queue: BTreeMap<CoreIndex, VecDeque<ParaId>>,
	) {
		assert_matches!(
			overseer_recv(virtual_overseer).await,
			AllMessages::RuntimeApi(RuntimeApiMessage::Request(hash, RuntimeApiRequest::AvailabilityCores(tx))) => {
				assert_eq!(hash, activated_hash);
				let _ = tx.send(Ok(cores));
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
								RuntimeApiRequest::Version(tx),
							)) => {
				assert_eq!(hash, activated_hash);
				let _ = tx.send(Ok(runtime_version));
			}
		);

		if runtime_version == RuntimeApiRequest::CLAIM_QUEUE_RUNTIME_REQUIREMENT {
			assert_matches!(
				overseer_recv(virtual_overseer).await,
				AllMessages::RuntimeApi(RuntimeApiMessage::Request(
									hash,
									RuntimeApiRequest::ClaimQueue(tx),
								)) => {
					assert_eq!(hash, activated_hash);
					let _ = tx.send(Ok(claim_queue.into()));
				}
			);
		}
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
					assert_eq!(candidate_receipt.descriptor().persisted_validation_data_hash, pvd.hash());
					assert_eq!(candidate_receipt.descriptor().para_head, dummy_head_data().hash());
					assert_eq!(candidate_receipt.descriptor().validation_code_hash, validation_code_hash);
				}
			);
		}
	}
}
