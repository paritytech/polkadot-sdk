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

use self::test_helpers::mock::new_leaf;
use super::*;
use assert_matches::assert_matches;
use futures::{future, Future};
use polkadot_node_primitives::{BlockData, InvalidCandidate, SignedFullStatement, Statement};
use polkadot_node_subsystem::{
	errors::RuntimeApiError,
	messages::{
		AllMessages, CollatorProtocolMessage, RuntimeApiMessage, RuntimeApiRequest,
		ValidationFailed,
	},
	ActiveLeavesUpdate, FromOrchestra, OverseerSignal, TimeoutExt,
};
use polkadot_node_subsystem_test_helpers as test_helpers;
use polkadot_primitives::{
	node_features, CandidateDescriptor, GroupRotationInfo, HeadData, PersistedValidationData,
	ScheduledCore, SessionIndex, LEGACY_MIN_BACKING_VOTES,
};
use polkadot_primitives_test_helpers::{
	dummy_candidate_receipt_bad_sig, dummy_collator, dummy_collator_signature,
	dummy_committed_candidate_receipt, dummy_hash, validator_pubkeys,
};
use polkadot_statement_table::v2::Misbehavior;
use rstest::rstest;
use sp_application_crypto::AppCrypto;
use sp_keyring::Sr25519Keyring;
use sp_keystore::Keystore;
use sp_tracing as _;
use std::{
	collections::{BTreeMap, HashMap, VecDeque},
	time::Duration,
};

mod prospective_parachains;

const ASYNC_BACKING_DISABLED_ERROR: RuntimeApiError =
	RuntimeApiError::NotSupported { runtime_api_name: "test-runtime" };

fn table_statement_to_primitive(statement: TableStatement) -> Statement {
	match statement {
		TableStatement::Seconded(committed_candidate_receipt) =>
			Statement::Seconded(committed_candidate_receipt),
		TableStatement::Valid(candidate_hash) => Statement::Valid(candidate_hash),
	}
}

fn dummy_pvd() -> PersistedValidationData {
	PersistedValidationData {
		parent_head: HeadData(vec![7, 8, 9]),
		relay_parent_number: 0_u32.into(),
		max_pov_size: 1024,
		relay_parent_storage_root: dummy_hash(),
	}
}

pub(crate) struct TestState {
	chain_ids: Vec<ParaId>,
	keystore: KeystorePtr,
	validators: Vec<Sr25519Keyring>,
	validator_public: Vec<ValidatorId>,
	validation_data: PersistedValidationData,
	validator_groups: (Vec<Vec<ValidatorIndex>>, GroupRotationInfo),
	validator_to_group: IndexedVec<ValidatorIndex, Option<GroupIndex>>,
	availability_cores: Vec<CoreState>,
	claim_queue: BTreeMap<CoreIndex, VecDeque<ParaId>>,
	head_data: HashMap<ParaId, HeadData>,
	signing_context: SigningContext,
	relay_parent: Hash,
	minimum_backing_votes: u32,
	disabled_validators: Vec<ValidatorIndex>,
	node_features: NodeFeatures,
}

impl TestState {
	fn session(&self) -> SessionIndex {
		self.signing_context.session_index
	}
}

impl Default for TestState {
	fn default() -> Self {
		let chain_a = ParaId::from(1);
		let chain_b = ParaId::from(2);

		let chain_ids = vec![chain_a, chain_b];

		let validators = vec![
			Sr25519Keyring::Alice,
			Sr25519Keyring::Bob,
			Sr25519Keyring::Charlie,
			Sr25519Keyring::Dave,
			Sr25519Keyring::Ferdie,
			Sr25519Keyring::One,
		];

		let keystore = Arc::new(sc_keystore::LocalKeystore::in_memory());
		// Make sure `Alice` key is in the keystore, so this mocked node will be a parachain
		// validator.
		Keystore::sr25519_generate_new(&*keystore, ValidatorId::ID, Some(&validators[0].to_seed()))
			.expect("Insert key into keystore");

		let validator_public = validator_pubkeys(&validators);

		let validator_groups = vec![vec![2, 0, 3, 5], vec![1]]
			.into_iter()
			.map(|g| g.into_iter().map(ValidatorIndex).collect())
			.collect();
		let validator_to_group: IndexedVec<_, _> =
			vec![Some(0), Some(1), Some(0), Some(0), None, Some(0)]
				.into_iter()
				.map(|x| x.map(|x| GroupIndex(x)))
				.collect();
		let group_rotation_info =
			GroupRotationInfo { session_start_block: 0, group_rotation_frequency: 100, now: 1 };

		let availability_cores = vec![
			CoreState::Scheduled(ScheduledCore { para_id: chain_a, collator: None }),
			CoreState::Scheduled(ScheduledCore { para_id: chain_b, collator: None }),
		];

		let mut claim_queue = BTreeMap::new();
		claim_queue.insert(CoreIndex(0), [chain_a].into_iter().collect());
		claim_queue.insert(CoreIndex(1), [chain_b].into_iter().collect());

		let mut head_data = HashMap::new();
		head_data.insert(chain_a, HeadData(vec![4, 5, 6]));
		head_data.insert(chain_b, HeadData(vec![5, 6, 7]));

		let relay_parent = Hash::repeat_byte(5);

		let signing_context = SigningContext { session_index: 1, parent_hash: relay_parent };

		let validation_data = PersistedValidationData {
			parent_head: HeadData(vec![7, 8, 9]),
			relay_parent_number: 0_u32.into(),
			max_pov_size: 1024,
			relay_parent_storage_root: dummy_hash(),
		};

		Self {
			chain_ids,
			keystore,
			validators,
			validator_public,
			validator_groups: (validator_groups, group_rotation_info),
			validator_to_group,
			availability_cores,
			claim_queue,
			head_data,
			validation_data,
			signing_context,
			relay_parent,
			minimum_backing_votes: LEGACY_MIN_BACKING_VOTES,
			disabled_validators: Vec::new(),
			node_features: Default::default(),
		}
	}
}

type VirtualOverseer =
	polkadot_node_subsystem_test_helpers::TestSubsystemContextHandle<CandidateBackingMessage>;

fn test_harness<T: Future<Output = VirtualOverseer>>(
	keystore: KeystorePtr,
	test: impl FnOnce(VirtualOverseer) -> T,
) {
	let pool = sp_core::testing::TaskExecutor::new();

	let (context, virtual_overseer) =
		polkadot_node_subsystem_test_helpers::make_subsystem_context(pool.clone());

	let subsystem = async move {
		if let Err(e) = super::run(context, keystore, Metrics(None)).await {
			panic!("{:?}", e);
		}
	};

	let test_fut = test(virtual_overseer);

	futures::pin_mut!(test_fut);
	futures::pin_mut!(subsystem);
	futures::executor::block_on(future::join(
		async move {
			let mut virtual_overseer = test_fut.await;
			virtual_overseer.send(FromOrchestra::Signal(OverseerSignal::Conclude)).await;
		},
		subsystem,
	));
}

fn make_erasure_root(test: &TestState, pov: PoV, validation_data: PersistedValidationData) -> Hash {
	let available_data = AvailableData { validation_data, pov: Arc::new(pov) };

	let chunks =
		polkadot_erasure_coding::obtain_chunks_v1(test.validators.len(), &available_data).unwrap();
	polkadot_erasure_coding::branches(&chunks).root()
}

#[derive(Default, Clone)]
struct TestCandidateBuilder {
	para_id: ParaId,
	head_data: HeadData,
	pov_hash: Hash,
	relay_parent: Hash,
	erasure_root: Hash,
	persisted_validation_data_hash: Hash,
	validation_code: Vec<u8>,
}

impl TestCandidateBuilder {
	fn build(self) -> CommittedCandidateReceipt {
		CommittedCandidateReceipt {
			descriptor: CandidateDescriptor {
				para_id: self.para_id,
				pov_hash: self.pov_hash,
				relay_parent: self.relay_parent,
				erasure_root: self.erasure_root,
				collator: dummy_collator(),
				signature: dummy_collator_signature(),
				para_head: self.head_data.hash(),
				validation_code_hash: ValidationCode(self.validation_code).hash(),
				persisted_validation_data_hash: self.persisted_validation_data_hash,
			},
			commitments: CandidateCommitments {
				head_data: self.head_data,
				upward_messages: Default::default(),
				horizontal_messages: Default::default(),
				new_validation_code: None,
				processed_downward_messages: 0,
				hrmp_watermark: 0_u32,
			},
		}
	}
}

// Tests that the subsystem performs actions that are required on startup.
async fn test_startup(virtual_overseer: &mut VirtualOverseer, test_state: &TestState) {
	// Start work on some new parent.
	virtual_overseer
		.send(FromOrchestra::Signal(OverseerSignal::ActiveLeaves(ActiveLeavesUpdate::start_work(
			new_leaf(test_state.relay_parent, 1),
		))))
		.await;

	assert_matches!(
		virtual_overseer.recv().await,
		AllMessages::RuntimeApi(
			RuntimeApiMessage::Request(parent, RuntimeApiRequest::AsyncBackingParams(tx))
		) if parent == test_state.relay_parent => {
			tx.send(Err(ASYNC_BACKING_DISABLED_ERROR)).unwrap();
		}
	);

	// Check that subsystem job issues a request for the session index for child.
	assert_matches!(
		virtual_overseer.recv().await,
		AllMessages::RuntimeApi(
			RuntimeApiMessage::Request(parent, RuntimeApiRequest::SessionIndexForChild(tx))
		) if parent == test_state.relay_parent => {
			tx.send(Ok(test_state.signing_context.session_index)).unwrap();
		}
	);

	// Check that subsystem job issues a request for a validator set.
	assert_matches!(
		virtual_overseer.recv().await,
		AllMessages::RuntimeApi(
			RuntimeApiMessage::Request(parent, RuntimeApiRequest::Validators(tx))
		) if parent == test_state.relay_parent => {
			tx.send(Ok(test_state.validator_public.clone())).unwrap();
		}
	);

	// Check that subsystem job issues a request for the validator groups.
	assert_matches!(
		virtual_overseer.recv().await,
		AllMessages::RuntimeApi(
			RuntimeApiMessage::Request(parent, RuntimeApiRequest::ValidatorGroups(tx))
		) if parent == test_state.relay_parent => {
			tx.send(Ok(test_state.validator_groups.clone())).unwrap();
		}
	);

	// Check that subsystem job issues a request for the availability cores.
	assert_matches!(
		virtual_overseer.recv().await,
		AllMessages::RuntimeApi(
			RuntimeApiMessage::Request(parent, RuntimeApiRequest::AvailabilityCores(tx))
		) if parent == test_state.relay_parent => {
			tx.send(Ok(test_state.availability_cores.clone())).unwrap();
		}
	);

	// Node features request from runtime: all features are disabled.
	assert_matches!(
		virtual_overseer.recv().await,
		AllMessages::RuntimeApi(
			RuntimeApiMessage::Request(_parent, RuntimeApiRequest::NodeFeatures(_session_index, tx))
		) => {
			tx.send(Ok(test_state.node_features.clone())).unwrap();
		}
	);

	// Check if subsystem job issues a request for the minimum backing votes.
	assert_matches!(
		virtual_overseer.recv().await,
		AllMessages::RuntimeApi(RuntimeApiMessage::Request(
			parent,
			RuntimeApiRequest::MinimumBackingVotes(session_index, tx),
		)) if parent == test_state.relay_parent && session_index == test_state.signing_context.session_index => {
			tx.send(Ok(test_state.minimum_backing_votes)).unwrap();
		}
	);

	// Check that subsystem job issues a request for the runtime version.
	assert_matches!(
		virtual_overseer.recv().await,
		AllMessages::RuntimeApi(
			RuntimeApiMessage::Request(parent, RuntimeApiRequest::Version(tx))
		) if parent == test_state.relay_parent => {
			tx.send(Ok(RuntimeApiRequest::DISABLED_VALIDATORS_RUNTIME_REQUIREMENT)).unwrap();
		}
	);

	// Check that subsystem job issues a request for the disabled validators.
	assert_matches!(
		virtual_overseer.recv().await,
		AllMessages::RuntimeApi(
			RuntimeApiMessage::Request(parent, RuntimeApiRequest::DisabledValidators(tx))
		) if parent == test_state.relay_parent => {
			tx.send(Ok(test_state.disabled_validators.clone())).unwrap();
		}
	);

	assert_matches!(
		virtual_overseer.recv().await,
		AllMessages::RuntimeApi(
			RuntimeApiMessage::Request(parent, RuntimeApiRequest::Version(tx))
		) if parent == test_state.relay_parent => {
			tx.send(Ok(RuntimeApiRequest::CLAIM_QUEUE_RUNTIME_REQUIREMENT)).unwrap();
		}
	);

	assert_matches!(
		virtual_overseer.recv().await,
		AllMessages::RuntimeApi(
			RuntimeApiMessage::Request(parent, RuntimeApiRequest::ClaimQueue(tx))
		) if parent == test_state.relay_parent => {
			tx.send(Ok(
				test_state.claim_queue.clone()
			)).unwrap();
		}
	);
}

async fn assert_validation_requests(
	virtual_overseer: &mut VirtualOverseer,
	validation_code: ValidationCode,
) {
	assert_matches!(
		virtual_overseer.recv().await,
		AllMessages::RuntimeApi(
			RuntimeApiMessage::Request(_, RuntimeApiRequest::ValidationCodeByHash(hash, tx))
		) if hash == validation_code.hash() => {
			tx.send(Ok(Some(validation_code))).unwrap();
		}
	);

	assert_matches!(
		virtual_overseer.recv().await,
		AllMessages::RuntimeApi(
			RuntimeApiMessage::Request(_, RuntimeApiRequest::SessionIndexForChild(tx))
		) => {
			tx.send(Ok(1u32.into())).unwrap();
		}
	);

	assert_matches!(
		virtual_overseer.recv().await,
		AllMessages::RuntimeApi(
			RuntimeApiMessage::Request(_, RuntimeApiRequest::SessionExecutorParams(sess_idx, tx))
		) if sess_idx == 1 => {
			tx.send(Ok(Some(ExecutorParams::default()))).unwrap();
		}
	);

	assert_matches!(
		virtual_overseer.recv().await,
		AllMessages::RuntimeApi(
			RuntimeApiMessage::Request(_, RuntimeApiRequest::NodeFeatures(sess_idx, tx))
		) if sess_idx == 1 => {
			tx.send(Ok(NodeFeatures::EMPTY)).unwrap();
		}
	);
}

async fn assert_validate_from_exhaustive(
	virtual_overseer: &mut VirtualOverseer,
	assert_pvd: &PersistedValidationData,
	assert_pov: &PoV,
	assert_validation_code: &ValidationCode,
	assert_candidate: &CommittedCandidateReceipt,
	expected_head_data: &HeadData,
	result_validation_data: PersistedValidationData,
) {
	assert_matches!(
		virtual_overseer.recv().await,
		AllMessages::CandidateValidation(
			CandidateValidationMessage::ValidateFromExhaustive {
				pov,
				validation_data,
				validation_code,
				candidate_receipt,
				exec_kind,
				response_sender,
				..
			},
		) if validation_data == *assert_pvd &&
			validation_code == *assert_validation_code &&
			*pov == *assert_pov && &candidate_receipt.descriptor == assert_candidate.descriptor() &&
			exec_kind == PvfExecKind::BackingSystemParas &&
			candidate_receipt.commitments_hash == assert_candidate.commitments.hash() =>
		{
			response_sender.send(Ok(ValidationResult::Valid(
				CandidateCommitments {
					head_data: expected_head_data.clone(),
					horizontal_messages: Default::default(),
					upward_messages: Default::default(),
					new_validation_code: None,
					processed_downward_messages: 0,
					hrmp_watermark: 0,
				},
				result_validation_data,
			)))
			.unwrap();
		}
	);
}

// Test that a `CandidateBackingMessage::Second` issues validation work
// and in case validation is successful issues a `StatementDistributionMessage`.
#[test]
fn backing_second_works() {
	let test_state = TestState::default();
	test_harness(test_state.keystore.clone(), |mut virtual_overseer| async move {
		test_startup(&mut virtual_overseer, &test_state).await;

		let pov = PoV { block_data: BlockData(vec![42, 43, 44]) };
		let pvd = dummy_pvd();
		let validation_code = ValidationCode(vec![1, 2, 3]);

		let expected_head_data = test_state.head_data.get(&test_state.chain_ids[0]).unwrap();

		let pov_hash = pov.hash();
		let candidate = TestCandidateBuilder {
			para_id: test_state.chain_ids[0],
			relay_parent: test_state.relay_parent,
			pov_hash,
			head_data: expected_head_data.clone(),
			erasure_root: make_erasure_root(&test_state, pov.clone(), pvd.clone()),
			persisted_validation_data_hash: pvd.hash(),
			validation_code: validation_code.0.clone(),
		}
		.build();

		let second = CandidateBackingMessage::Second(
			test_state.relay_parent,
			candidate.to_plain(),
			pvd.clone(),
			pov.clone(),
		);

		virtual_overseer.send(FromOrchestra::Communication { msg: second }).await;

		assert_validation_requests(&mut virtual_overseer, validation_code.clone()).await;

		assert_validate_from_exhaustive(
			&mut virtual_overseer,
			&pvd,
			&pov,
			&validation_code,
			&candidate,
			expected_head_data,
			test_state.validation_data.clone(),
		)
		.await;

		assert_matches!(
			virtual_overseer.recv().await,
			AllMessages::AvailabilityStore(
				AvailabilityStoreMessage::StoreAvailableData { candidate_hash, tx, .. }
			) if candidate_hash == candidate.hash() => {
				tx.send(Ok(())).unwrap();
			}
		);

		assert_matches!(
			virtual_overseer.recv().await,
			AllMessages::StatementDistribution(
				StatementDistributionMessage::Share(
					parent_hash,
					_signed_statement,
				)
			) if parent_hash == test_state.relay_parent => {}
		);

		assert_matches!(
			virtual_overseer.recv().await,
			AllMessages::CollatorProtocol(CollatorProtocolMessage::Seconded(hash, statement)) => {
				assert_eq!(test_state.relay_parent, hash);
				assert_matches!(statement.payload(), Statement::Seconded(_));
			}
		);

		virtual_overseer
			.send(FromOrchestra::Signal(OverseerSignal::ActiveLeaves(
				ActiveLeavesUpdate::stop_work(test_state.relay_parent),
			)))
			.await;
		virtual_overseer
	});
}

// Test that the candidate reaches quorum successfully.
#[rstest]
#[case(true)]
#[case(false)]
fn backing_works(#[case] elastic_scaling_mvp: bool) {
	let mut test_state = TestState::default();
	if elastic_scaling_mvp {
		test_state
			.node_features
			.resize((node_features::FeatureIndex::ElasticScalingMVP as u8 + 1) as usize, false);
		test_state
			.node_features
			.set(node_features::FeatureIndex::ElasticScalingMVP as u8 as usize, true);
	}

	test_harness(test_state.keystore.clone(), |mut virtual_overseer| async move {
		test_startup(&mut virtual_overseer, &test_state).await;

		let pov_ab = PoV { block_data: BlockData(vec![1, 2, 3]) };
		let pvd_ab = dummy_pvd();
		let validation_code_ab = ValidationCode(vec![1, 2, 3]);

		let pov_hash = pov_ab.hash();

		let expected_head_data = test_state.head_data.get(&test_state.chain_ids[0]).unwrap();

		let candidate_a = TestCandidateBuilder {
			para_id: test_state.chain_ids[0],
			relay_parent: test_state.relay_parent,
			pov_hash,
			head_data: expected_head_data.clone(),
			erasure_root: make_erasure_root(&test_state, pov_ab.clone(), pvd_ab.clone()),
			validation_code: validation_code_ab.0.clone(),
			..Default::default()
		}
		.build();

		let candidate_a_hash = candidate_a.hash();
		let candidate_a_commitments_hash = candidate_a.commitments.hash();

		let public1 = Keystore::sr25519_generate_new(
			&*test_state.keystore,
			ValidatorId::ID,
			Some(&test_state.validators[5].to_seed()),
		)
		.expect("Insert key into keystore");
		let public2 = Keystore::sr25519_generate_new(
			&*test_state.keystore,
			ValidatorId::ID,
			Some(&test_state.validators[2].to_seed()),
		)
		.expect("Insert key into keystore");

		let signed_a = SignedFullStatementWithPVD::sign(
			&test_state.keystore,
			StatementWithPVD::Seconded(candidate_a.clone(), pvd_ab.clone()),
			&test_state.signing_context,
			ValidatorIndex(2),
			&public2.into(),
		)
		.ok()
		.flatten()
		.expect("should be signed");

		let signed_b = SignedFullStatementWithPVD::sign(
			&test_state.keystore,
			StatementWithPVD::Valid(candidate_a_hash),
			&test_state.signing_context,
			ValidatorIndex(5),
			&public1.into(),
		)
		.ok()
		.flatten()
		.expect("should be signed");

		let statement =
			CandidateBackingMessage::Statement(test_state.relay_parent, signed_a.clone());

		virtual_overseer.send(FromOrchestra::Communication { msg: statement }).await;

		assert_validation_requests(&mut virtual_overseer, validation_code_ab.clone()).await;

		// Sending a `Statement::Seconded` for our assignment will start
		// validation process. The first thing requested is the PoV.
		assert_matches!(
			virtual_overseer.recv().await,
			AllMessages::AvailabilityDistribution(
				AvailabilityDistributionMessage::FetchPoV {
					relay_parent,
					tx,
					..
				}
			) if relay_parent == test_state.relay_parent => {
				tx.send(pov_ab.clone()).unwrap();
			}
		);

		// The next step is the actual request to Validation subsystem
		// to validate the `Seconded` candidate.
		assert_matches!(
			virtual_overseer.recv().await,
			AllMessages::CandidateValidation(
				CandidateValidationMessage::ValidateFromExhaustive {
					validation_data,
					validation_code,
					candidate_receipt,
					pov,
					exec_kind,
					response_sender,
					..
				},
			) if validation_data == pvd_ab &&
				validation_code == validation_code_ab &&
				*pov == pov_ab && &candidate_receipt.descriptor == candidate_a.descriptor() &&
				exec_kind == PvfExecKind::BackingSystemParas &&
				candidate_receipt.commitments_hash == candidate_a_commitments_hash =>
			{
				response_sender.send(Ok(
					ValidationResult::Valid(CandidateCommitments {
						head_data: expected_head_data.clone(),
						upward_messages: Default::default(),
						horizontal_messages: Default::default(),
						new_validation_code: None,
						processed_downward_messages: 0,
						hrmp_watermark: 0,
					}, test_state.validation_data.clone()),
				)).unwrap();
			}
		);

		assert_matches!(
			virtual_overseer.recv().await,
			AllMessages::AvailabilityStore(
				AvailabilityStoreMessage::StoreAvailableData { candidate_hash, tx, .. }
			) if candidate_hash == candidate_a.hash() => {
				tx.send(Ok(())).unwrap();
			}
		);

		assert_matches!(
			virtual_overseer.recv().await,
			AllMessages::StatementDistribution(
				StatementDistributionMessage::Share(hash, _stmt)
			) => {
				assert_eq!(test_state.relay_parent, hash);
			}
		);

		assert_matches!(
			virtual_overseer.recv().await,
			AllMessages::Provisioner(
				ProvisionerMessage::ProvisionableData(
					_,
					ProvisionableData::BackedCandidate(candidate_receipt)
				)
			) => {
				assert_eq!(candidate_receipt, candidate_a.to_plain());
			}
		);

		let statement =
			CandidateBackingMessage::Statement(test_state.relay_parent, signed_b.clone());

		virtual_overseer.send(FromOrchestra::Communication { msg: statement }).await;

		let (tx, rx) = oneshot::channel();
		let msg = CandidateBackingMessage::GetBackableCandidates(
			std::iter::once((
				test_state.chain_ids[0],
				vec![(candidate_a_hash, test_state.relay_parent)],
			))
			.collect(),
			tx,
		);

		virtual_overseer.send(FromOrchestra::Communication { msg }).await;

		let mut candidates = rx.await.unwrap();
		assert_eq!(1, candidates.len());
		let candidates = candidates.remove(&test_state.chain_ids[0]).unwrap();
		assert_eq!(1, candidates.len());
		assert_eq!(candidates[0].validity_votes().len(), 3);

		let (validator_indices, maybe_core_index) =
			candidates[0].validator_indices_and_core_index(elastic_scaling_mvp);
		if elastic_scaling_mvp {
			assert_eq!(maybe_core_index.unwrap(), CoreIndex(0));
		} else {
			assert!(maybe_core_index.is_none());
		}

		assert_eq!(
			validator_indices,
			bitvec::bitvec![u8, bitvec::order::Lsb0; 1, 1, 0, 1].as_bitslice()
		);

		virtual_overseer
			.send(FromOrchestra::Signal(OverseerSignal::ActiveLeaves(
				ActiveLeavesUpdate::stop_work(test_state.relay_parent),
			)))
			.await;
		virtual_overseer
	});
}

#[test]
fn get_backed_candidate_preserves_order() {
	let mut test_state = TestState::default();
	test_state
		.node_features
		.resize((node_features::FeatureIndex::ElasticScalingMVP as u8 + 1) as usize, false);
	test_state
		.node_features
		.set(node_features::FeatureIndex::ElasticScalingMVP as u8 as usize, true);

	// Set a single validator as the first validator group. It simplifies the test.
	test_state.validator_groups.0[0] = vec![ValidatorIndex(2)];
	// Add another validator group for the third core.
	test_state.validator_groups.0.push(vec![ValidatorIndex(3)]);
	// Assign the second core to the same para as the first one.
	test_state.availability_cores[1] =
		CoreState::Scheduled(ScheduledCore { para_id: test_state.chain_ids[0], collator: None });
	*test_state.claim_queue.get_mut(&CoreIndex(1)).unwrap() =
		[test_state.chain_ids[0]].into_iter().collect();
	// Add another availability core for paraid 2.
	test_state.availability_cores.push(CoreState::Scheduled(ScheduledCore {
		para_id: test_state.chain_ids[1],
		collator: None,
	}));
	test_state
		.claim_queue
		.insert(CoreIndex(2), [test_state.chain_ids[1]].into_iter().collect());

	test_harness(test_state.keystore.clone(), |mut virtual_overseer| async move {
		test_startup(&mut virtual_overseer, &test_state).await;

		let pov_a = PoV { block_data: BlockData(vec![1, 2, 3]) };
		let pov_b = PoV { block_data: BlockData(vec![3, 4, 5]) };
		let pov_c = PoV { block_data: BlockData(vec![5, 6, 7]) };
		let validation_code_ab = ValidationCode(vec![1, 2, 3]);
		let validation_code_c = ValidationCode(vec![4, 5, 6]);

		let parent_head_data_a = test_state.head_data.get(&test_state.chain_ids[0]).unwrap();
		let parent_head_data_b = {
			let mut head = parent_head_data_a.clone();
			head.0[0] = 98;
			head
		};
		let output_head_data_b = {
			let mut head = parent_head_data_a.clone();
			head.0[0] = 99;
			head
		};
		let parent_head_data_c = test_state.head_data.get(&test_state.chain_ids[1]).unwrap();
		let output_head_data_c = {
			let mut head = parent_head_data_c.clone();
			head.0[0] = 97;
			head
		};

		let pvd_a = PersistedValidationData {
			parent_head: parent_head_data_a.clone(),
			relay_parent_number: 0_u32.into(),
			max_pov_size: 1024,
			relay_parent_storage_root: dummy_hash(),
		};
		let pvd_b = PersistedValidationData {
			parent_head: parent_head_data_b.clone(),
			relay_parent_number: 0_u32.into(),
			max_pov_size: 1024,
			relay_parent_storage_root: dummy_hash(),
		};
		let pvd_c = PersistedValidationData {
			parent_head: parent_head_data_c.clone(),
			relay_parent_number: 0_u32.into(),
			max_pov_size: 1024,
			relay_parent_storage_root: dummy_hash(),
		};

		let candidate_a = TestCandidateBuilder {
			para_id: test_state.chain_ids[0],
			relay_parent: test_state.relay_parent,
			pov_hash: pov_a.hash(),
			head_data: parent_head_data_b.clone(),
			erasure_root: make_erasure_root(&test_state, pov_a.clone(), pvd_a.clone()),
			validation_code: validation_code_ab.0.clone(),
			persisted_validation_data_hash: pvd_a.hash(),
		}
		.build();
		let candidate_b = TestCandidateBuilder {
			para_id: test_state.chain_ids[0],
			relay_parent: test_state.relay_parent,
			pov_hash: pov_b.hash(),
			head_data: output_head_data_b.clone(),
			erasure_root: make_erasure_root(&test_state, pov_b.clone(), pvd_b.clone()),
			validation_code: validation_code_ab.0.clone(),
			persisted_validation_data_hash: pvd_b.hash(),
		}
		.build();
		let candidate_c = TestCandidateBuilder {
			para_id: test_state.chain_ids[1],
			relay_parent: test_state.relay_parent,
			pov_hash: pov_c.hash(),
			head_data: output_head_data_c.clone(),
			erasure_root: make_erasure_root(&test_state, pov_b.clone(), pvd_c.clone()),
			validation_code: validation_code_c.0.clone(),
			persisted_validation_data_hash: pvd_c.hash(),
		}
		.build();
		let candidate_a_hash = candidate_a.hash();
		let candidate_b_hash = candidate_b.hash();
		let candidate_c_hash = candidate_c.hash();

		// Back a chain of two candidates for the first paraid. Back one candidate for the second
		// paraid.
		for (candidate, pvd, validator_index) in [
			(candidate_a, pvd_a, ValidatorIndex(2)),
			(candidate_b, pvd_b, ValidatorIndex(1)),
			(candidate_c, pvd_c, ValidatorIndex(3)),
		] {
			let public = Keystore::sr25519_generate_new(
				&*test_state.keystore,
				ValidatorId::ID,
				Some(&test_state.validators[validator_index.0 as usize].to_seed()),
			)
			.expect("Insert key into keystore");

			let signed = SignedFullStatementWithPVD::sign(
				&test_state.keystore,
				StatementWithPVD::Seconded(candidate.clone(), pvd.clone()),
				&test_state.signing_context,
				validator_index,
				&public.into(),
			)
			.ok()
			.flatten()
			.expect("should be signed");

			let statement =
				CandidateBackingMessage::Statement(test_state.relay_parent, signed.clone());

			virtual_overseer.send(FromOrchestra::Communication { msg: statement }).await;

			assert_matches!(
				virtual_overseer.recv().await,
				AllMessages::Provisioner(
					ProvisionerMessage::ProvisionableData(
						_,
						ProvisionableData::BackedCandidate(candidate_receipt)
					)
				) => {
					assert_eq!(candidate_receipt, candidate.to_plain());
				}
			);
		}

		// Happy case, all candidates should be present.
		let (tx, rx) = oneshot::channel();
		let msg = CandidateBackingMessage::GetBackableCandidates(
			[
				(
					test_state.chain_ids[0],
					vec![
						(candidate_a_hash, test_state.relay_parent),
						(candidate_b_hash, test_state.relay_parent),
					],
				),
				(test_state.chain_ids[1], vec![(candidate_c_hash, test_state.relay_parent)]),
			]
			.into_iter()
			.collect(),
			tx,
		);
		virtual_overseer.send(FromOrchestra::Communication { msg }).await;
		let mut candidates = rx.await.unwrap();
		assert_eq!(2, candidates.len());
		assert_eq!(
			candidates
				.remove(&test_state.chain_ids[0])
				.unwrap()
				.iter()
				.map(|c| c.hash())
				.collect::<Vec<_>>(),
			vec![candidate_a_hash, candidate_b_hash]
		);
		assert_eq!(
			candidates
				.remove(&test_state.chain_ids[1])
				.unwrap()
				.iter()
				.map(|c| c.hash())
				.collect::<Vec<_>>(),
			vec![candidate_c_hash]
		);

		// The first candidate of the first para is invalid (we supply the wrong relay parent or a
		// wrong candidate hash). No candidates should be returned for paraid 1. ParaId 2 should be
		// fine.
		for candidates in [
			vec![
				(candidate_a_hash, Hash::repeat_byte(9)),
				(candidate_b_hash, test_state.relay_parent),
			],
			vec![
				(CandidateHash(Hash::repeat_byte(9)), test_state.relay_parent),
				(candidate_b_hash, test_state.relay_parent),
			],
		] {
			let (tx, rx) = oneshot::channel();
			let msg = CandidateBackingMessage::GetBackableCandidates(
				[
					(test_state.chain_ids[0], candidates),
					(test_state.chain_ids[1], vec![(candidate_c_hash, test_state.relay_parent)]),
				]
				.into_iter()
				.collect(),
				tx,
			);
			virtual_overseer.send(FromOrchestra::Communication { msg }).await;
			let mut candidates = rx.await.unwrap();
			assert_eq!(candidates.len(), 1);

			assert!(candidates.remove(&test_state.chain_ids[0]).is_none());
			assert_eq!(
				candidates
					.remove(&test_state.chain_ids[1])
					.unwrap()
					.iter()
					.map(|c| c.hash())
					.collect::<Vec<_>>(),
				vec![candidate_c_hash]
			);
		}

		// The second candidate of the first para is invalid (we supply the wrong relay parent or a
		// wrong candidate hash). The first candidate of the first para should still be present.
		// ParaId 2 is fine.
		for candidates in [
			vec![
				(candidate_a_hash, test_state.relay_parent),
				(candidate_b_hash, Hash::repeat_byte(9)),
			],
			vec![
				(candidate_a_hash, test_state.relay_parent),
				(CandidateHash(Hash::repeat_byte(9)), test_state.relay_parent),
			],
		] {
			let (tx, rx) = oneshot::channel();
			let msg = CandidateBackingMessage::GetBackableCandidates(
				[
					(test_state.chain_ids[0], candidates),
					(test_state.chain_ids[1], vec![(candidate_c_hash, test_state.relay_parent)]),
				]
				.into_iter()
				.collect(),
				tx,
			);
			virtual_overseer.send(FromOrchestra::Communication { msg }).await;
			let mut candidates = rx.await.unwrap();
			assert_eq!(2, candidates.len());
			assert_eq!(
				candidates
					.remove(&test_state.chain_ids[0])
					.unwrap()
					.iter()
					.map(|c| c.hash())
					.collect::<Vec<_>>(),
				vec![candidate_a_hash]
			);
			assert_eq!(
				candidates
					.remove(&test_state.chain_ids[1])
					.unwrap()
					.iter()
					.map(|c| c.hash())
					.collect::<Vec<_>>(),
				vec![candidate_c_hash]
			);
		}

		// Both candidates of para id 1 are invalid (we supply the wrong relay parent or a wrong
		// candidate hash). No candidates should be returned for para id 1. Para Id 2 is fine.
		for candidates in [
			vec![
				(CandidateHash(Hash::repeat_byte(9)), test_state.relay_parent),
				(CandidateHash(Hash::repeat_byte(10)), test_state.relay_parent),
			],
			vec![
				(candidate_a_hash, Hash::repeat_byte(9)),
				(candidate_b_hash, Hash::repeat_byte(10)),
			],
		] {
			let (tx, rx) = oneshot::channel();
			let msg = CandidateBackingMessage::GetBackableCandidates(
				[
					(test_state.chain_ids[0], candidates),
					(test_state.chain_ids[1], vec![(candidate_c_hash, test_state.relay_parent)]),
				]
				.into_iter()
				.collect(),
				tx,
			);
			virtual_overseer.send(FromOrchestra::Communication { msg }).await;
			let mut candidates = rx.await.unwrap();
			assert_eq!(candidates.len(), 1);

			assert!(candidates.remove(&test_state.chain_ids[0]).is_none());
			assert_eq!(
				candidates
					.remove(&test_state.chain_ids[1])
					.unwrap()
					.iter()
					.map(|c| c.hash())
					.collect::<Vec<_>>(),
				vec![candidate_c_hash]
			);
		}

		virtual_overseer
			.send(FromOrchestra::Signal(OverseerSignal::ActiveLeaves(
				ActiveLeavesUpdate::stop_work(test_state.relay_parent),
			)))
			.await;
		virtual_overseer
	});
}

#[test]
fn extract_core_index_from_statement_works() {
	let test_state = TestState::default();

	let pov_a = PoV { block_data: BlockData(vec![42, 43, 44]) };
	let pvd_a = dummy_pvd();
	let validation_code_a = ValidationCode(vec![1, 2, 3]);

	let pov_hash = pov_a.hash();

	let mut candidate = TestCandidateBuilder {
		para_id: test_state.chain_ids[0],
		relay_parent: test_state.relay_parent,
		pov_hash,
		erasure_root: make_erasure_root(&test_state, pov_a.clone(), pvd_a.clone()),
		persisted_validation_data_hash: pvd_a.hash(),
		validation_code: validation_code_a.0.clone(),
		..Default::default()
	}
	.build();

	let public2 = Keystore::sr25519_generate_new(
		&*test_state.keystore,
		ValidatorId::ID,
		Some(&test_state.validators[2].to_seed()),
	)
	.expect("Insert key into keystore");

	let signed_statement_1 = SignedFullStatementWithPVD::sign(
		&test_state.keystore,
		StatementWithPVD::Seconded(candidate.clone(), pvd_a.clone()),
		&test_state.signing_context,
		ValidatorIndex(2),
		&public2.into(),
	)
	.ok()
	.flatten()
	.expect("should be signed");

	let public1 = Keystore::sr25519_generate_new(
		&*test_state.keystore,
		ValidatorId::ID,
		Some(&test_state.validators[1].to_seed()),
	)
	.expect("Insert key into keystore");

	let signed_statement_2 = SignedFullStatementWithPVD::sign(
		&test_state.keystore,
		StatementWithPVD::Seconded(candidate.clone(), pvd_a.clone()),
		&test_state.signing_context,
		ValidatorIndex(1),
		&public1.into(),
	)
	.ok()
	.flatten()
	.expect("should be signed");

	candidate.descriptor.para_id = test_state.chain_ids[1];

	let signed_statement_3 = SignedFullStatementWithPVD::sign(
		&test_state.keystore,
		StatementWithPVD::Seconded(candidate, pvd_a.clone()),
		&test_state.signing_context,
		ValidatorIndex(1),
		&public1.into(),
	)
	.ok()
	.flatten()
	.expect("should be signed");

	let core_index_1 = core_index_from_statement(
		&test_state.validator_to_group,
		&test_state.validator_groups.1,
		test_state.availability_cores.len() as _,
		&test_state.claim_queue.clone().into(),
		&signed_statement_1,
	)
	.unwrap();

	assert_eq!(core_index_1, CoreIndex(0));

	let core_index_2 = core_index_from_statement(
		&test_state.validator_to_group,
		&test_state.validator_groups.1,
		test_state.availability_cores.len() as _,
		&test_state.claim_queue.clone().into(),
		&signed_statement_2,
	);

	// Must be none, para_id in descriptor is different than para assigned to core
	assert_eq!(core_index_2, None);

	let core_index_3 = core_index_from_statement(
		&test_state.validator_to_group,
		&test_state.validator_groups.1,
		test_state.availability_cores.len() as _,
		&test_state.claim_queue.clone().into(),
		&signed_statement_3,
	)
	.unwrap();

	assert_eq!(core_index_3, CoreIndex(1));
}

#[test]
fn backing_works_while_validation_ongoing() {
	let test_state = TestState::default();
	test_harness(test_state.keystore.clone(), |mut virtual_overseer| async move {
		test_startup(&mut virtual_overseer, &test_state).await;

		let pov_abc = PoV { block_data: BlockData(vec![1, 2, 3]) };
		let pvd_abc = dummy_pvd();
		let validation_code_abc = ValidationCode(vec![1, 2, 3]);

		let pov_hash = pov_abc.hash();

		let expected_head_data = test_state.head_data.get(&test_state.chain_ids[0]).unwrap();

		let candidate_a = TestCandidateBuilder {
			para_id: test_state.chain_ids[0],
			relay_parent: test_state.relay_parent,
			pov_hash,
			head_data: expected_head_data.clone(),
			erasure_root: make_erasure_root(&test_state, pov_abc.clone(), pvd_abc.clone()),
			validation_code: validation_code_abc.0.clone(),
			..Default::default()
		}
		.build();

		let candidate_a_hash = candidate_a.hash();
		let candidate_a_commitments_hash = candidate_a.commitments.hash();

		let public1 = Keystore::sr25519_generate_new(
			&*test_state.keystore,
			ValidatorId::ID,
			Some(&test_state.validators[5].to_seed()),
		)
		.expect("Insert key into keystore");
		let public2 = Keystore::sr25519_generate_new(
			&*test_state.keystore,
			ValidatorId::ID,
			Some(&test_state.validators[2].to_seed()),
		)
		.expect("Insert key into keystore");
		let public3 = Keystore::sr25519_generate_new(
			&*test_state.keystore,
			ValidatorId::ID,
			Some(&test_state.validators[3].to_seed()),
		)
		.expect("Insert key into keystore");

		let signed_a = SignedFullStatementWithPVD::sign(
			&test_state.keystore,
			StatementWithPVD::Seconded(candidate_a.clone(), pvd_abc.clone()),
			&test_state.signing_context,
			ValidatorIndex(2),
			&public2.into(),
		)
		.ok()
		.flatten()
		.expect("should be signed");

		let signed_b = SignedFullStatementWithPVD::sign(
			&test_state.keystore,
			StatementWithPVD::Valid(candidate_a_hash),
			&test_state.signing_context,
			ValidatorIndex(5),
			&public1.into(),
		)
		.ok()
		.flatten()
		.expect("should be signed");

		let signed_c = SignedFullStatementWithPVD::sign(
			&test_state.keystore,
			StatementWithPVD::Valid(candidate_a_hash),
			&test_state.signing_context,
			ValidatorIndex(3),
			&public3.into(),
		)
		.ok()
		.flatten()
		.expect("should be signed");

		let statement =
			CandidateBackingMessage::Statement(test_state.relay_parent, signed_a.clone());
		virtual_overseer.send(FromOrchestra::Communication { msg: statement }).await;

		assert_validation_requests(&mut virtual_overseer, validation_code_abc.clone()).await;

		// Sending a `Statement::Seconded` for our assignment will start
		// validation process. The first thing requested is PoV from the
		// `PoVDistribution`.
		assert_matches!(
			virtual_overseer.recv().await,
			AllMessages::AvailabilityDistribution(
				AvailabilityDistributionMessage::FetchPoV {
					relay_parent,
					tx,
					..
				}
			) if relay_parent == test_state.relay_parent => {
				tx.send(pov_abc.clone()).unwrap();
			}
		);

		// The next step is the actual request to Validation subsystem
		// to validate the `Seconded` candidate.
		assert_matches!(
			virtual_overseer.recv().await,
			AllMessages::CandidateValidation(
				CandidateValidationMessage::ValidateFromExhaustive {
					validation_data,
					validation_code,
					candidate_receipt,
					pov,
					exec_kind,
					response_sender,
					..
				},
			) if validation_data == pvd_abc &&
				validation_code == validation_code_abc &&
				*pov == pov_abc && &candidate_receipt.descriptor == candidate_a.descriptor() &&
				exec_kind == PvfExecKind::BackingSystemParas &&
				candidate_a_commitments_hash == candidate_receipt.commitments_hash =>
			{
				// we never validate the candidate. our local node
				// shouldn't issue any statements.
				std::mem::forget(response_sender);
			}
		);

		let statement =
			CandidateBackingMessage::Statement(test_state.relay_parent, signed_b.clone());

		virtual_overseer.send(FromOrchestra::Communication { msg: statement }).await;

		// Candidate gets backed entirely by other votes.
		assert_matches!(
			virtual_overseer.recv().await,
			AllMessages::Provisioner(
				ProvisionerMessage::ProvisionableData(
					_,
					ProvisionableData::BackedCandidate(CandidateReceipt {
						descriptor,
						..
					})
				)
			) if descriptor == candidate_a.descriptor
		);

		let statement =
			CandidateBackingMessage::Statement(test_state.relay_parent, signed_c.clone());

		virtual_overseer.send(FromOrchestra::Communication { msg: statement }).await;

		let (tx, rx) = oneshot::channel();
		let msg = CandidateBackingMessage::GetBackableCandidates(
			std::iter::once((
				test_state.chain_ids[0],
				vec![(candidate_a.hash(), test_state.relay_parent)],
			))
			.collect(),
			tx,
		);

		virtual_overseer.send(FromOrchestra::Communication { msg }).await;

		let mut candidates = rx.await.unwrap();
		assert_eq!(candidates.len(), 1);
		let candidates = candidates.remove(&test_state.chain_ids[0]).unwrap();
		assert_eq!(1, candidates.len());
		assert_eq!(candidates[0].validity_votes().len(), 3);

		assert!(candidates[0]
			.validity_votes()
			.contains(&ValidityAttestation::Implicit(signed_a.signature().clone())));
		assert!(candidates[0]
			.validity_votes()
			.contains(&ValidityAttestation::Explicit(signed_b.signature().clone())));
		assert!(candidates[0]
			.validity_votes()
			.contains(&ValidityAttestation::Explicit(signed_c.signature().clone())));
		assert_eq!(
			candidates[0].validator_indices_and_core_index(false),
			(bitvec::bitvec![u8, bitvec::order::Lsb0; 1, 0, 1, 1].as_bitslice(), None)
		);

		virtual_overseer
			.send(FromOrchestra::Signal(OverseerSignal::ActiveLeaves(
				ActiveLeavesUpdate::stop_work(test_state.relay_parent),
			)))
			.await;
		virtual_overseer
	});
}

// Issuing conflicting statements on the same candidate should
// be a misbehavior.
#[test]
fn backing_misbehavior_works() {
	let test_state = TestState::default();
	test_harness(test_state.keystore.clone(), |mut virtual_overseer| async move {
		test_startup(&mut virtual_overseer, &test_state).await;

		let pov_a = PoV { block_data: BlockData(vec![1, 2, 3]) };

		let pov_hash = pov_a.hash();
		let pvd_a = dummy_pvd();
		let validation_code_a = ValidationCode(vec![1, 2, 3]);

		let expected_head_data = test_state.head_data.get(&test_state.chain_ids[0]).unwrap();

		let candidate_a = TestCandidateBuilder {
			para_id: test_state.chain_ids[0],
			relay_parent: test_state.relay_parent,
			pov_hash,
			erasure_root: make_erasure_root(&test_state, pov_a.clone(), pvd_a.clone()),
			head_data: expected_head_data.clone(),
			validation_code: validation_code_a.0.clone(),
			..Default::default()
		}
		.build();

		let candidate_a_hash = candidate_a.hash();
		let candidate_a_commitments_hash = candidate_a.commitments.hash();

		let public2 = Keystore::sr25519_generate_new(
			&*test_state.keystore,
			ValidatorId::ID,
			Some(&test_state.validators[2].to_seed()),
		)
		.expect("Insert key into keystore");
		let seconded_2 = SignedFullStatementWithPVD::sign(
			&test_state.keystore,
			StatementWithPVD::Seconded(candidate_a.clone(), pvd_a.clone()),
			&test_state.signing_context,
			ValidatorIndex(2),
			&public2.into(),
		)
		.ok()
		.flatten()
		.expect("should be signed");

		let valid_2 = SignedFullStatementWithPVD::sign(
			&test_state.keystore,
			StatementWithPVD::Valid(candidate_a_hash),
			&test_state.signing_context,
			ValidatorIndex(2),
			&public2.into(),
		)
		.ok()
		.flatten()
		.expect("should be signed");

		let statement =
			CandidateBackingMessage::Statement(test_state.relay_parent, seconded_2.clone());

		virtual_overseer.send(FromOrchestra::Communication { msg: statement }).await;

		assert_validation_requests(&mut virtual_overseer, validation_code_a.clone()).await;

		assert_matches!(
			virtual_overseer.recv().await,
			AllMessages::AvailabilityDistribution(
				AvailabilityDistributionMessage::FetchPoV {
					relay_parent,
					tx,
					..
				}
			) if relay_parent == test_state.relay_parent => {
				tx.send(pov_a.clone()).unwrap();
			}
		);

		assert_matches!(
			virtual_overseer.recv().await,
			AllMessages::CandidateValidation(
				CandidateValidationMessage::ValidateFromExhaustive {
					validation_data,
					validation_code,
					candidate_receipt,
					pov,
					exec_kind,
					response_sender,
					..
				},
			) if validation_data == pvd_a &&
				validation_code == validation_code_a &&
				*pov == pov_a && &candidate_receipt.descriptor == candidate_a.descriptor() &&
				exec_kind == PvfExecKind::BackingSystemParas &&
				candidate_a_commitments_hash == candidate_receipt.commitments_hash =>
			{
				response_sender.send(Ok(
					ValidationResult::Valid(CandidateCommitments {
						head_data: expected_head_data.clone(),
						upward_messages: Default::default(),
						horizontal_messages: Default::default(),
						new_validation_code: None,
						processed_downward_messages: 0,
						hrmp_watermark: 0,
					}, test_state.validation_data.clone()),
				)).unwrap();
			}
		);

		assert_matches!(
			virtual_overseer.recv().await,
			AllMessages::AvailabilityStore(
				AvailabilityStoreMessage::StoreAvailableData { candidate_hash, tx, .. }
			) if candidate_hash == candidate_a.hash() => {
					tx.send(Ok(())).unwrap();
				}
		);

		assert_matches!(
			virtual_overseer.recv().await,
			AllMessages::StatementDistribution(
				StatementDistributionMessage::Share(
					relay_parent,
					signed_statement,
				)
			) if relay_parent == test_state.relay_parent => {
				assert_eq!(*signed_statement.payload(), StatementWithPVD::Valid(candidate_a_hash));
			}
		);

		assert_matches!(
			virtual_overseer.recv().await,
			AllMessages::Provisioner(
				ProvisionerMessage::ProvisionableData(
					_,
					ProvisionableData::BackedCandidate(CandidateReceipt {
						descriptor,
						..
					})
				)
			) if descriptor == candidate_a.descriptor
		);

		// This `Valid` statement is redundant after the `Seconded` statement already sent.
		let statement =
			CandidateBackingMessage::Statement(test_state.relay_parent, valid_2.clone());

		virtual_overseer.send(FromOrchestra::Communication { msg: statement }).await;

		assert_matches!(
			virtual_overseer.recv().await,
			AllMessages::Provisioner(
				ProvisionerMessage::ProvisionableData(
					_,
					ProvisionableData::MisbehaviorReport(
						relay_parent,
						validator_index,
						Misbehavior::ValidityDoubleVote(vdv),
					)
				)
			) if relay_parent == test_state.relay_parent => {
				let ((t1, s1), (t2, s2)) = vdv.deconstruct::<TableContext>();
				let t1 = table_statement_to_primitive(t1);
				let t2 = table_statement_to_primitive(t2);

				SignedFullStatement::new(
					t1,
					validator_index,
					s1,
					&test_state.signing_context,
					&test_state.validator_public[validator_index.0 as usize],
				).expect("signature must be valid");

				SignedFullStatement::new(
					t2,
					validator_index,
					s2,
					&test_state.signing_context,
					&test_state.validator_public[validator_index.0 as usize],
				).expect("signature must be valid");
			}
		);
		virtual_overseer
	});
}

// Test that if we are asked to second an invalid candidate we
// can still second a valid one afterwards.
#[test]
fn backing_dont_second_invalid() {
	let test_state = TestState::default();
	test_harness(test_state.keystore.clone(), |mut virtual_overseer| async move {
		test_startup(&mut virtual_overseer, &test_state).await;

		let pov_block_a = PoV { block_data: BlockData(vec![42, 43, 44]) };
		let pvd_a = dummy_pvd();
		let validation_code_a = ValidationCode(vec![1, 2, 3]);

		let pov_block_b = PoV { block_data: BlockData(vec![45, 46, 47]) };
		let pvd_b = {
			let mut pvd_b = pvd_a.clone();
			pvd_b.parent_head = HeadData(vec![14, 15, 16]);
			pvd_b.max_pov_size = pvd_a.max_pov_size / 2;
			pvd_b
		};
		let validation_code_b = ValidationCode(vec![4, 5, 6]);

		let pov_hash_a = pov_block_a.hash();
		let pov_hash_b = pov_block_b.hash();

		let expected_head_data = test_state.head_data.get(&test_state.chain_ids[0]).unwrap();

		let candidate_a = TestCandidateBuilder {
			para_id: test_state.chain_ids[0],
			relay_parent: test_state.relay_parent,
			pov_hash: pov_hash_a,
			erasure_root: make_erasure_root(&test_state, pov_block_a.clone(), pvd_a.clone()),
			persisted_validation_data_hash: pvd_a.hash(),
			validation_code: validation_code_a.0.clone(),
			..Default::default()
		}
		.build();

		let candidate_b = TestCandidateBuilder {
			para_id: test_state.chain_ids[0],
			relay_parent: test_state.relay_parent,
			pov_hash: pov_hash_b,
			erasure_root: make_erasure_root(&test_state, pov_block_b.clone(), pvd_b.clone()),
			head_data: expected_head_data.clone(),
			persisted_validation_data_hash: pvd_b.hash(),
			validation_code: validation_code_b.0.clone(),
		}
		.build();

		let second = CandidateBackingMessage::Second(
			test_state.relay_parent,
			candidate_a.to_plain(),
			pvd_a.clone(),
			pov_block_a.clone(),
		);

		virtual_overseer.send(FromOrchestra::Communication { msg: second }).await;

		assert_validation_requests(&mut virtual_overseer, validation_code_a.clone()).await;

		assert_matches!(
			virtual_overseer.recv().await,
			AllMessages::CandidateValidation(
				CandidateValidationMessage::ValidateFromExhaustive {
					validation_data,
					validation_code,
					candidate_receipt,
					pov,
					exec_kind,
					response_sender,
					..
				},
			) if validation_data == pvd_a &&
				validation_code == validation_code_a &&
				*pov == pov_block_a && &candidate_receipt.descriptor == candidate_a.descriptor() &&
				exec_kind == PvfExecKind::BackingSystemParas &&
				candidate_a.commitments.hash() == candidate_receipt.commitments_hash =>
			{
				response_sender.send(Ok(ValidationResult::Invalid(InvalidCandidate::BadReturn))).unwrap();
			}
		);

		assert_matches!(
			virtual_overseer.recv().await,
			AllMessages::CollatorProtocol(
				CollatorProtocolMessage::Invalid(parent_hash, c)
			) if parent_hash == test_state.relay_parent && c == candidate_a.to_plain()
		);

		let second = CandidateBackingMessage::Second(
			test_state.relay_parent,
			candidate_b.to_plain(),
			pvd_b.clone(),
			pov_block_b.clone(),
		);

		virtual_overseer.send(FromOrchestra::Communication { msg: second }).await;

		assert_validation_requests(&mut virtual_overseer, validation_code_b.clone()).await;

		assert_matches!(
			virtual_overseer.recv().await,
			AllMessages::CandidateValidation(
				CandidateValidationMessage::ValidateFromExhaustive {
					validation_data,
					validation_code,
					candidate_receipt,
					pov,
					exec_kind,
					response_sender,
					..
				},
			) if validation_data == pvd_b &&
				validation_code == validation_code_b &&
				*pov == pov_block_b && &candidate_receipt.descriptor == candidate_b.descriptor() &&
				exec_kind == PvfExecKind::BackingSystemParas &&
				candidate_b.commitments.hash() == candidate_receipt.commitments_hash =>
			{
				response_sender.send(Ok(
					ValidationResult::Valid(CandidateCommitments {
						head_data: expected_head_data.clone(),
						upward_messages: Default::default(),
						horizontal_messages: Default::default(),
						new_validation_code: None,
						processed_downward_messages: 0,
						hrmp_watermark: 0,
					}, pvd_b.clone()),
				)).unwrap();
			}
		);

		assert_matches!(
			virtual_overseer.recv().await,
			AllMessages::AvailabilityStore(
				AvailabilityStoreMessage::StoreAvailableData { candidate_hash, tx, .. }
			) if candidate_hash == candidate_b.hash() => {
				tx.send(Ok(())).unwrap();
			}
		);

		assert_matches!(
			virtual_overseer.recv().await,
			AllMessages::StatementDistribution(
				StatementDistributionMessage::Share(
					parent_hash,
					signed_statement,
				)
			) if parent_hash == test_state.relay_parent => {
				assert_eq!(*signed_statement.payload(), StatementWithPVD::Seconded(candidate_b, pvd_b.clone()));
			}
		);

		virtual_overseer
			.send(FromOrchestra::Signal(OverseerSignal::ActiveLeaves(
				ActiveLeavesUpdate::stop_work(test_state.relay_parent),
			)))
			.await;
		virtual_overseer
	});
}

// Test that if we have already issued a statement (in this case `Invalid`) about a
// candidate we will not be issuing a `Seconded` statement on it.
#[test]
fn backing_second_after_first_fails_works() {
	let test_state = TestState::default();
	test_harness(test_state.keystore.clone(), |mut virtual_overseer| async move {
		test_startup(&mut virtual_overseer, &test_state).await;

		let pov_a = PoV { block_data: BlockData(vec![42, 43, 44]) };
		let pvd_a = dummy_pvd();
		let validation_code_a = ValidationCode(vec![1, 2, 3]);

		let pov_hash = pov_a.hash();

		let candidate = TestCandidateBuilder {
			para_id: test_state.chain_ids[0],
			relay_parent: test_state.relay_parent,
			pov_hash,
			erasure_root: make_erasure_root(&test_state, pov_a.clone(), pvd_a.clone()),
			persisted_validation_data_hash: pvd_a.hash(),
			validation_code: validation_code_a.0.clone(),
			..Default::default()
		}
		.build();

		let validator2 = Keystore::sr25519_generate_new(
			&*test_state.keystore,
			ValidatorId::ID,
			Some(&test_state.validators[2].to_seed()),
		)
		.expect("Insert key into keystore");

		let signed_a = SignedFullStatementWithPVD::sign(
			&test_state.keystore,
			StatementWithPVD::Seconded(candidate.clone(), pvd_a.clone()),
			&test_state.signing_context,
			ValidatorIndex(2),
			&validator2.into(),
		)
		.ok()
		.flatten()
		.expect("should be signed");

		// Send in a `Statement` with a candidate.
		let statement =
			CandidateBackingMessage::Statement(test_state.relay_parent, signed_a.clone());

		virtual_overseer.send(FromOrchestra::Communication { msg: statement }).await;

		assert_validation_requests(&mut virtual_overseer, validation_code_a.clone()).await;

		// Subsystem requests PoV and requests validation.
		assert_matches!(
			virtual_overseer.recv().await,
			AllMessages::AvailabilityDistribution(
				AvailabilityDistributionMessage::FetchPoV {
					relay_parent,
					tx,
					..
				}
			) if relay_parent == test_state.relay_parent => {
				tx.send(pov_a.clone()).unwrap();
			}
		);

		// Tell subsystem that this candidate is invalid.
		assert_matches!(
			virtual_overseer.recv().await,
			AllMessages::CandidateValidation(
				CandidateValidationMessage::ValidateFromExhaustive {
					validation_data,
					validation_code,
					candidate_receipt,
					pov,
					exec_kind,
					response_sender,
					..
				},
			) if validation_data == pvd_a &&
				validation_code == validation_code_a &&
				*pov == pov_a && &candidate_receipt.descriptor == candidate.descriptor() &&
				exec_kind == PvfExecKind::BackingSystemParas &&
				candidate.commitments.hash() == candidate_receipt.commitments_hash =>
			{
				response_sender.send(Ok(ValidationResult::Invalid(InvalidCandidate::BadReturn))).unwrap();
			}
		);

		// Ask subsystem to `Second` a candidate that already has a statement issued about.
		// This should emit no actions from subsystem.
		let second = CandidateBackingMessage::Second(
			test_state.relay_parent,
			candidate.to_plain(),
			pvd_a.clone(),
			pov_a.clone(),
		);

		virtual_overseer.send(FromOrchestra::Communication { msg: second }).await;

		let pov_to_second = PoV { block_data: BlockData(vec![3, 2, 1]) };
		let pvd_to_second = dummy_pvd();
		let validation_code_to_second = ValidationCode(vec![5, 6, 7]);

		let pov_hash = pov_to_second.hash();

		let candidate_to_second = TestCandidateBuilder {
			para_id: test_state.chain_ids[0],
			relay_parent: test_state.relay_parent,
			pov_hash,
			erasure_root: make_erasure_root(
				&test_state,
				pov_to_second.clone(),
				pvd_to_second.clone(),
			),
			persisted_validation_data_hash: pvd_to_second.hash(),
			validation_code: validation_code_to_second.0.clone(),
			..Default::default()
		}
		.build();

		let second = CandidateBackingMessage::Second(
			test_state.relay_parent,
			candidate_to_second.to_plain(),
			pvd_to_second.clone(),
			pov_to_second.clone(),
		);

		// In order to trigger _some_ actions from subsystem ask it to second another
		// candidate. The only reason to do so is to make sure that no actions were
		// triggered on the prev step.
		virtual_overseer.send(FromOrchestra::Communication { msg: second }).await;

		assert_validation_requests(&mut virtual_overseer, validation_code_to_second.clone()).await;

		assert_matches!(
			virtual_overseer.recv().await,
			AllMessages::CandidateValidation(
				CandidateValidationMessage::ValidateFromExhaustive { pov, .. },
			) => {
				assert_eq!(&*pov, &pov_to_second);
			}
		);
		virtual_overseer
	});
}

// That that if the validation of the candidate has failed this does not stop
// the work of this subsystem and so it is not fatal to the node.
#[test]
fn backing_works_after_failed_validation() {
	let test_state = TestState::default();
	test_harness(test_state.keystore.clone(), |mut virtual_overseer| async move {
		test_startup(&mut virtual_overseer, &test_state).await;

		let pov_a = PoV { block_data: BlockData(vec![42, 43, 44]) };
		let pvd_a = dummy_pvd();
		let validation_code_a = ValidationCode(vec![1, 2, 3]);

		let pov_hash = pov_a.hash();

		let candidate = TestCandidateBuilder {
			para_id: test_state.chain_ids[0],
			relay_parent: test_state.relay_parent,
			pov_hash,
			erasure_root: make_erasure_root(&test_state, pov_a.clone(), pvd_a.clone()),
			validation_code: validation_code_a.0.clone(),
			..Default::default()
		}
		.build();

		let public2 = Keystore::sr25519_generate_new(
			&*test_state.keystore,
			ValidatorId::ID,
			Some(&test_state.validators[2].to_seed()),
		)
		.expect("Insert key into keystore");
		let signed_a = SignedFullStatementWithPVD::sign(
			&test_state.keystore,
			StatementWithPVD::Seconded(candidate.clone(), pvd_a.clone()),
			&test_state.signing_context,
			ValidatorIndex(2),
			&public2.into(),
		)
		.ok()
		.flatten()
		.expect("should be signed");

		// Send in a `Statement` with a candidate.
		let statement =
			CandidateBackingMessage::Statement(test_state.relay_parent, signed_a.clone());

		virtual_overseer.send(FromOrchestra::Communication { msg: statement }).await;

		assert_validation_requests(&mut virtual_overseer, validation_code_a.clone()).await;

		// Subsystem requests PoV and requests validation.
		assert_matches!(
			virtual_overseer.recv().await,
			AllMessages::AvailabilityDistribution(
				AvailabilityDistributionMessage::FetchPoV {
					relay_parent,
					tx,
					..
				}
			) if relay_parent == test_state.relay_parent => {
				tx.send(pov_a.clone()).unwrap();
			}
		);

		// Tell subsystem that this candidate is invalid.
		assert_matches!(
			virtual_overseer.recv().await,
			AllMessages::CandidateValidation(
				CandidateValidationMessage::ValidateFromExhaustive {
					validation_data,
					validation_code,
					candidate_receipt,
					pov,
					exec_kind,
					response_sender,
					..
				},
			) if validation_data == pvd_a &&
				validation_code == validation_code_a &&
				*pov == pov_a && &candidate_receipt.descriptor == candidate.descriptor() &&
				exec_kind == PvfExecKind::BackingSystemParas &&
				candidate.commitments.hash() == candidate_receipt.commitments_hash =>
			{
				response_sender.send(Err(ValidationFailed("Internal test error".into()))).unwrap();
			}
		);

		// Try to get a set of backable candidates to trigger _some_ action in the subsystem
		// and check that it is still alive.
		let (tx, rx) = oneshot::channel();
		let msg = CandidateBackingMessage::GetBackableCandidates(
			std::iter::once((
				test_state.chain_ids[0],
				vec![(candidate.hash(), test_state.relay_parent)],
			))
			.collect(),
			tx,
		);

		virtual_overseer.send(FromOrchestra::Communication { msg }).await;
		assert_eq!(rx.await.unwrap().len(), 0);
		virtual_overseer
	});
}

#[test]
fn candidate_backing_reorders_votes() {
	use sp_core::Encode;

	let core_idx = CoreIndex(10);
	let validators = vec![
		Sr25519Keyring::Alice,
		Sr25519Keyring::Bob,
		Sr25519Keyring::Charlie,
		Sr25519Keyring::Dave,
		Sr25519Keyring::Ferdie,
		Sr25519Keyring::One,
	];

	let validator_public = validator_pubkeys(&validators);
	let validator_groups = {
		let mut validator_groups = HashMap::new();
		validator_groups
			.insert(core_idx, vec![0, 1, 2, 3, 4, 5].into_iter().map(ValidatorIndex).collect());
		validator_groups
	};

	let table_context = TableContext {
		validator: None,
		disabled_validators: Vec::new(),
		groups: validator_groups,
		validators: validator_public.clone(),
	};

	let fake_attestation = |idx: u32| {
		let candidate =
			dummy_candidate_receipt_bad_sig(Default::default(), Some(Default::default()));
		let hash = candidate.hash();
		let mut data = vec![0; 64];
		data[0..32].copy_from_slice(hash.0.as_bytes());
		data[32..36].copy_from_slice(idx.encode().as_slice());

		let sig = ValidatorSignature::try_from(data).unwrap();
		polkadot_statement_table::generic::ValidityAttestation::Implicit(sig)
	};

	let attested = TableAttestedCandidate {
		candidate: dummy_committed_candidate_receipt(dummy_hash()),
		validity_votes: vec![
			(ValidatorIndex(5), fake_attestation(5)),
			(ValidatorIndex(3), fake_attestation(3)),
			(ValidatorIndex(1), fake_attestation(1)),
		],
		group_id: core_idx,
	};

	let backed = table_attested_to_backed(attested, &table_context, false).unwrap();

	let expected_bitvec = {
		let mut validator_indices = BitVec::<u8, bitvec::order::Lsb0>::with_capacity(6);
		validator_indices.resize(6, false);

		validator_indices.set(1, true);
		validator_indices.set(3, true);
		validator_indices.set(5, true);

		validator_indices
	};

	// Should be in bitfield order, which is opposite to the order provided to the function.
	let expected_attestations =
		vec![fake_attestation(1).into(), fake_attestation(3).into(), fake_attestation(5).into()];

	assert_eq!(
		backed.validator_indices_and_core_index(false),
		(expected_bitvec.as_bitslice(), None)
	);
	assert_eq!(backed.validity_votes(), expected_attestations);
}

// Test whether we retry on failed PoV fetching.
#[test]
fn retry_works() {
	// sp_tracing::try_init_simple();
	let test_state = TestState::default();
	test_harness(test_state.keystore.clone(), |mut virtual_overseer| async move {
		test_startup(&mut virtual_overseer, &test_state).await;

		let pov_a = PoV { block_data: BlockData(vec![42, 43, 44]) };
		let pvd_a = dummy_pvd();
		let validation_code_a = ValidationCode(vec![1, 2, 3]);

		let pov_hash = pov_a.hash();

		let candidate = TestCandidateBuilder {
			para_id: test_state.chain_ids[0],
			relay_parent: test_state.relay_parent,
			pov_hash,
			erasure_root: make_erasure_root(&test_state, pov_a.clone(), pvd_a.clone()),
			persisted_validation_data_hash: pvd_a.hash(),
			validation_code: validation_code_a.0.clone(),
			..Default::default()
		}
		.build();

		let public2 = Keystore::sr25519_generate_new(
			&*test_state.keystore,
			ValidatorId::ID,
			Some(&test_state.validators[2].to_seed()),
		)
		.expect("Insert key into keystore");
		let public3 = Keystore::sr25519_generate_new(
			&*test_state.keystore,
			ValidatorId::ID,
			Some(&test_state.validators[3].to_seed()),
		)
		.expect("Insert key into keystore");
		let public5 = Keystore::sr25519_generate_new(
			&*test_state.keystore,
			ValidatorId::ID,
			Some(&test_state.validators[5].to_seed()),
		)
		.expect("Insert key into keystore");
		let signed_a = SignedFullStatementWithPVD::sign(
			&test_state.keystore,
			StatementWithPVD::Seconded(candidate.clone(), pvd_a.clone()),
			&test_state.signing_context,
			ValidatorIndex(2),
			&public2.into(),
		)
		.ok()
		.flatten()
		.expect("should be signed");
		let signed_b = SignedFullStatementWithPVD::sign(
			&test_state.keystore,
			StatementWithPVD::Valid(candidate.hash()),
			&test_state.signing_context,
			ValidatorIndex(3),
			&public3.into(),
		)
		.ok()
		.flatten()
		.expect("should be signed");
		let signed_c = SignedFullStatementWithPVD::sign(
			&test_state.keystore,
			StatementWithPVD::Valid(candidate.hash()),
			&test_state.signing_context,
			ValidatorIndex(5),
			&public5.into(),
		)
		.ok()
		.flatten()
		.expect("should be signed");

		// Send in a `Statement` with a candidate.
		let statement =
			CandidateBackingMessage::Statement(test_state.relay_parent, signed_a.clone());
		virtual_overseer.send(FromOrchestra::Communication { msg: statement }).await;

		assert_validation_requests(&mut virtual_overseer, validation_code_a.clone()).await;

		// Subsystem requests PoV and requests validation.
		// We cancel - should mean retry on next backing statement.
		assert_matches!(
			virtual_overseer.recv().await,
			AllMessages::AvailabilityDistribution(
				AvailabilityDistributionMessage::FetchPoV {
					relay_parent,
					tx,
					..
				}
			) if relay_parent == test_state.relay_parent => {
				std::mem::drop(tx);
			}
		);

		let statement =
			CandidateBackingMessage::Statement(test_state.relay_parent, signed_b.clone());
		virtual_overseer.send(FromOrchestra::Communication { msg: statement }).await;

		// Not deterministic which message comes first:
		for _ in 0u32..6 {
			match virtual_overseer.recv().await {
				AllMessages::Provisioner(ProvisionerMessage::ProvisionableData(
					_,
					ProvisionableData::BackedCandidate(CandidateReceipt { descriptor, .. }),
				)) => {
					assert_eq!(descriptor, candidate.descriptor);
				},
				AllMessages::AvailabilityDistribution(
					AvailabilityDistributionMessage::FetchPoV { relay_parent, tx, .. },
				) if relay_parent == test_state.relay_parent => {
					std::mem::drop(tx);
				},
				AllMessages::RuntimeApi(RuntimeApiMessage::Request(
					_,
					RuntimeApiRequest::ValidationCodeByHash(hash, tx),
				)) if hash == validation_code_a.hash() => {
					tx.send(Ok(Some(validation_code_a.clone()))).unwrap();
				},
				AllMessages::RuntimeApi(RuntimeApiMessage::Request(
					_,
					RuntimeApiRequest::SessionIndexForChild(tx),
				)) => {
					tx.send(Ok(1u32.into())).unwrap();
				},
				AllMessages::RuntimeApi(RuntimeApiMessage::Request(
					_,
					RuntimeApiRequest::SessionExecutorParams(1, tx),
				)) => {
					tx.send(Ok(Some(ExecutorParams::default()))).unwrap();
				},
				AllMessages::RuntimeApi(RuntimeApiMessage::Request(
					_,
					RuntimeApiRequest::NodeFeatures(1, tx),
				)) => {
					tx.send(Ok(NodeFeatures::EMPTY)).unwrap();
				},
				msg => {
					assert!(false, "Unexpected message: {:?}", msg);
				},
			}
		}

		let statement =
			CandidateBackingMessage::Statement(test_state.relay_parent, signed_c.clone());
		virtual_overseer.send(FromOrchestra::Communication { msg: statement }).await;

		assert_validation_requests(&mut virtual_overseer, validation_code_a.clone()).await;

		assert_matches!(
			virtual_overseer.recv().await,
			AllMessages::AvailabilityDistribution(
				AvailabilityDistributionMessage::FetchPoV {
					relay_parent,
					tx,
					..
				}
				// Subsystem requests PoV and requests validation.
				// Now we pass.
				) if relay_parent == test_state.relay_parent => {
					tx.send(pov_a.clone()).unwrap();
				}
		);

		assert_matches!(
			virtual_overseer.recv().await,
			AllMessages::CandidateValidation(
				CandidateValidationMessage::ValidateFromExhaustive {
					validation_data,
					validation_code,
					candidate_receipt,
					pov,
					exec_kind,
					..
				},
			) if validation_data == pvd_a &&
				validation_code == validation_code_a &&
				*pov == pov_a && &candidate_receipt.descriptor == candidate.descriptor() &&
				exec_kind == PvfExecKind::BackingSystemParas &&
				candidate.commitments.hash() == candidate_receipt.commitments_hash
		);
		virtual_overseer
	});
}

#[test]
fn observes_backing_even_if_not_validator() {
	let test_state = TestState::default();
	let empty_keystore = Arc::new(sc_keystore::LocalKeystore::in_memory());
	test_harness(empty_keystore, |mut virtual_overseer| async move {
		test_startup(&mut virtual_overseer, &test_state).await;

		let pov = PoV { block_data: BlockData(vec![1, 2, 3]) };
		let pvd = dummy_pvd();
		let validation_code = ValidationCode(vec![1, 2, 3]);

		let pov_hash = pov.hash();

		let expected_head_data = test_state.head_data.get(&test_state.chain_ids[0]).unwrap();

		let candidate_a = TestCandidateBuilder {
			para_id: test_state.chain_ids[0],
			relay_parent: test_state.relay_parent,
			pov_hash,
			head_data: expected_head_data.clone(),
			erasure_root: make_erasure_root(&test_state, pov.clone(), pvd.clone()),
			persisted_validation_data_hash: pvd.hash(),
			validation_code: validation_code.0.clone(),
		}
		.build();

		let candidate_a_hash = candidate_a.hash();
		let public0 = Keystore::sr25519_generate_new(
			&*test_state.keystore,
			ValidatorId::ID,
			Some(&test_state.validators[0].to_seed()),
		)
		.expect("Insert key into keystore");
		let public1 = Keystore::sr25519_generate_new(
			&*test_state.keystore,
			ValidatorId::ID,
			Some(&test_state.validators[5].to_seed()),
		)
		.expect("Insert key into keystore");
		let public2 = Keystore::sr25519_generate_new(
			&*test_state.keystore,
			ValidatorId::ID,
			Some(&test_state.validators[2].to_seed()),
		)
		.expect("Insert key into keystore");

		// Produce a 3-of-5 quorum on the candidate.

		let signed_a = SignedFullStatementWithPVD::sign(
			&test_state.keystore,
			StatementWithPVD::Seconded(candidate_a.clone(), pvd.clone()),
			&test_state.signing_context,
			ValidatorIndex(0),
			&public0.into(),
		)
		.ok()
		.flatten()
		.expect("should be signed");

		let signed_b = SignedFullStatementWithPVD::sign(
			&test_state.keystore,
			StatementWithPVD::Valid(candidate_a_hash),
			&test_state.signing_context,
			ValidatorIndex(5),
			&public1.into(),
		)
		.ok()
		.flatten()
		.expect("should be signed");

		let signed_c = SignedFullStatementWithPVD::sign(
			&test_state.keystore,
			StatementWithPVD::Valid(candidate_a_hash),
			&test_state.signing_context,
			ValidatorIndex(2),
			&public2.into(),
		)
		.ok()
		.flatten()
		.expect("should be signed");

		let statement =
			CandidateBackingMessage::Statement(test_state.relay_parent, signed_a.clone());

		virtual_overseer.send(FromOrchestra::Communication { msg: statement }).await;

		let statement =
			CandidateBackingMessage::Statement(test_state.relay_parent, signed_b.clone());

		virtual_overseer.send(FromOrchestra::Communication { msg: statement }).await;

		assert_matches!(
			virtual_overseer.recv().await,
			AllMessages::Provisioner(
				ProvisionerMessage::ProvisionableData(
					_,
					ProvisionableData::BackedCandidate(candidate_receipt)
				)
			) => {
				assert_eq!(candidate_receipt, candidate_a.to_plain());
			}
		);

		let statement =
			CandidateBackingMessage::Statement(test_state.relay_parent, signed_c.clone());

		virtual_overseer.send(FromOrchestra::Communication { msg: statement }).await;

		virtual_overseer
			.send(FromOrchestra::Signal(OverseerSignal::ActiveLeaves(
				ActiveLeavesUpdate::stop_work(test_state.relay_parent),
			)))
			.await;
		virtual_overseer
	});
}

// Tests that it's impossible to second multiple candidates per relay parent
// without prospective parachains.
#[test]
fn cannot_second_multiple_candidates_per_parent() {
	let test_state = TestState::default();
	test_harness(test_state.keystore.clone(), |mut virtual_overseer| async move {
		test_startup(&mut virtual_overseer, &test_state).await;

		let pov = PoV { block_data: BlockData(vec![42, 43, 44]) };
		let pvd = dummy_pvd();
		let validation_code = ValidationCode(vec![1, 2, 3]);

		let expected_head_data = test_state.head_data.get(&test_state.chain_ids[0]).unwrap();

		let pov_hash = pov.hash();
		let candidate_builder = TestCandidateBuilder {
			para_id: test_state.chain_ids[0],
			relay_parent: test_state.relay_parent,
			pov_hash,
			head_data: expected_head_data.clone(),
			erasure_root: make_erasure_root(&test_state, pov.clone(), pvd.clone()),
			persisted_validation_data_hash: pvd.hash(),
			validation_code: validation_code.0.clone(),
		};
		let candidate = candidate_builder.clone().build();

		let second = CandidateBackingMessage::Second(
			test_state.relay_parent,
			candidate.to_plain(),
			pvd.clone(),
			pov.clone(),
		);

		virtual_overseer.send(FromOrchestra::Communication { msg: second }).await;

		assert_validation_requests(&mut virtual_overseer, validation_code.clone()).await;

		assert_validate_from_exhaustive(
			&mut virtual_overseer,
			&pvd,
			&pov,
			&validation_code,
			&candidate,
			expected_head_data,
			test_state.validation_data.clone(),
		)
		.await;

		assert_matches!(
			virtual_overseer.recv().await,
			AllMessages::AvailabilityStore(
				AvailabilityStoreMessage::StoreAvailableData { candidate_hash, tx, .. }
			) if candidate_hash == candidate.hash() => {
				tx.send(Ok(())).unwrap();
			}
		);

		assert_matches!(
			virtual_overseer.recv().await,
			AllMessages::StatementDistribution(
				StatementDistributionMessage::Share(
					parent_hash,
					_signed_statement,
				)
			) if parent_hash == test_state.relay_parent => {}
		);

		assert_matches!(
			virtual_overseer.recv().await,
			AllMessages::CollatorProtocol(CollatorProtocolMessage::Seconded(hash, statement)) => {
				assert_eq!(test_state.relay_parent, hash);
				assert_matches!(statement.payload(), Statement::Seconded(_));
			}
		);

		// Try to second candidate with the same relay parent again.

		// Make sure the candidate hash is different.
		let validation_code = ValidationCode(vec![4, 5, 6]);
		let mut candidate_builder = candidate_builder;
		candidate_builder.validation_code = validation_code.0.clone();
		let candidate = candidate_builder.build();

		let second = CandidateBackingMessage::Second(
			test_state.relay_parent,
			candidate.to_plain(),
			pvd.clone(),
			pov.clone(),
		);

		virtual_overseer.send(FromOrchestra::Communication { msg: second }).await;

		// The validation is still requested.
		assert_validation_requests(&mut virtual_overseer, validation_code.clone()).await;

		assert_matches!(
			virtual_overseer.recv().await,
			AllMessages::CandidateValidation(
				CandidateValidationMessage::ValidateFromExhaustive { response_sender, .. },
			) => {
				response_sender.send(Ok(ValidationResult::Valid(
					CandidateCommitments {
						head_data: expected_head_data.clone(),
						horizontal_messages: Default::default(),
						upward_messages: Default::default(),
						new_validation_code: None,
						processed_downward_messages: 0,
						hrmp_watermark: 0,
					},
					test_state.validation_data.clone(),
				)))
				.unwrap();
			}
		);

		assert_matches!(
			virtual_overseer.recv().await,
			AllMessages::AvailabilityStore(
				AvailabilityStoreMessage::StoreAvailableData { candidate_hash, tx, .. }
			) if candidate_hash == candidate.hash() => {
				tx.send(Ok(())).unwrap();
			}
		);

		// Validation done, but the candidate is rejected cause of 0-depth being already occupied.

		assert!(virtual_overseer
			.recv()
			.timeout(std::time::Duration::from_millis(50))
			.await
			.is_none());

		virtual_overseer
	});
}

#[test]
fn new_leaf_view_doesnt_clobber_old() {
	let mut test_state = TestState::default();
	let relay_parent_2 = Hash::repeat_byte(1);
	assert_ne!(test_state.relay_parent, relay_parent_2);
	test_harness(test_state.keystore.clone(), |mut virtual_overseer| async move {
		test_startup(&mut virtual_overseer, &test_state).await;

		// New leaf that doesn't clobber old.
		{
			let old_relay_parent = test_state.relay_parent;
			test_state.relay_parent = relay_parent_2;
			test_startup(&mut virtual_overseer, &test_state).await;
			test_state.relay_parent = old_relay_parent;
		}

		let pov = PoV { block_data: BlockData(vec![42, 43, 44]) };
		let pvd = dummy_pvd();
		let validation_code = ValidationCode(vec![1, 2, 3]);

		let expected_head_data = test_state.head_data.get(&test_state.chain_ids[0]).unwrap();

		let pov_hash = pov.hash();
		let candidate = TestCandidateBuilder {
			para_id: test_state.chain_ids[0],
			relay_parent: test_state.relay_parent,
			pov_hash,
			head_data: expected_head_data.clone(),
			erasure_root: make_erasure_root(&test_state, pov.clone(), pvd.clone()),
			persisted_validation_data_hash: pvd.hash(),
			validation_code: validation_code.0.clone(),
		}
		.build();

		let second = CandidateBackingMessage::Second(
			test_state.relay_parent,
			candidate.to_plain(),
			pvd.clone(),
			pov.clone(),
		);

		virtual_overseer.send(FromOrchestra::Communication { msg: second }).await;

		// If the old leaf was clobbered by the first, the seconded candidate
		// would be ignored.
		assert!(
			virtual_overseer
				.recv()
				.timeout(std::time::Duration::from_millis(500))
				.await
				.is_some(),
			"first leaf appears to be inactive"
		);

		virtual_overseer
	});
}

// Test that a disabled local validator doesn't do any work on `CandidateBackingMessage::Second`
#[test]
fn disabled_validator_doesnt_distribute_statement_on_receiving_second() {
	let mut test_state = TestState::default();
	test_state.disabled_validators.push(ValidatorIndex(0));

	test_harness(test_state.keystore.clone(), |mut virtual_overseer| async move {
		test_startup(&mut virtual_overseer, &test_state).await;

		let pov = PoV { block_data: BlockData(vec![42, 43, 44]) };
		let pvd = dummy_pvd();
		let validation_code = ValidationCode(vec![1, 2, 3]);

		let expected_head_data = test_state.head_data.get(&test_state.chain_ids[0]).unwrap();

		let pov_hash = pov.hash();
		let candidate = TestCandidateBuilder {
			para_id: test_state.chain_ids[0],
			relay_parent: test_state.relay_parent,
			pov_hash,
			head_data: expected_head_data.clone(),
			erasure_root: make_erasure_root(&test_state, pov.clone(), pvd.clone()),
			persisted_validation_data_hash: pvd.hash(),
			validation_code: validation_code.0.clone(),
		}
		.build();

		let second = CandidateBackingMessage::Second(
			test_state.relay_parent,
			candidate.to_plain(),
			pvd.clone(),
			pov.clone(),
		);

		virtual_overseer.send(FromOrchestra::Communication { msg: second }).await;

		// Ensure backing subsystem is not doing any work
		assert_matches!(virtual_overseer.recv().timeout(Duration::from_secs(1)).await, None);

		virtual_overseer
			.send(FromOrchestra::Signal(OverseerSignal::ActiveLeaves(
				ActiveLeavesUpdate::stop_work(test_state.relay_parent),
			)))
			.await;
		virtual_overseer
	});
}

// Test that a disabled local validator doesn't do any work on `CandidateBackingMessage::Statement`
#[test]
fn disabled_validator_doesnt_distribute_statement_on_receiving_statement() {
	let mut test_state = TestState::default();
	test_state.disabled_validators.push(ValidatorIndex(0));

	test_harness(test_state.keystore.clone(), |mut virtual_overseer| async move {
		test_startup(&mut virtual_overseer, &test_state).await;

		let pov = PoV { block_data: BlockData(vec![42, 43, 44]) };
		let pvd = dummy_pvd();
		let validation_code = ValidationCode(vec![1, 2, 3]);

		let expected_head_data = test_state.head_data.get(&test_state.chain_ids[0]).unwrap();

		let pov_hash = pov.hash();
		let candidate = TestCandidateBuilder {
			para_id: test_state.chain_ids[0],
			relay_parent: test_state.relay_parent,
			pov_hash,
			head_data: expected_head_data.clone(),
			erasure_root: make_erasure_root(&test_state, pov.clone(), pvd.clone()),
			persisted_validation_data_hash: pvd.hash(),
			validation_code: validation_code.0.clone(),
		}
		.build();

		let public2 = Keystore::sr25519_generate_new(
			&*test_state.keystore,
			ValidatorId::ID,
			Some(&test_state.validators[2].to_seed()),
		)
		.expect("Insert key into keystore");

		let signed = SignedFullStatementWithPVD::sign(
			&test_state.keystore,
			StatementWithPVD::Seconded(candidate.clone(), pvd.clone()),
			&test_state.signing_context,
			ValidatorIndex(2),
			&public2.into(),
		)
		.ok()
		.flatten()
		.expect("should be signed");

		let statement = CandidateBackingMessage::Statement(test_state.relay_parent, signed.clone());

		virtual_overseer.send(FromOrchestra::Communication { msg: statement }).await;

		// Ensure backing subsystem is not doing any work
		assert_matches!(virtual_overseer.recv().timeout(Duration::from_secs(1)).await, None);

		virtual_overseer
			.send(FromOrchestra::Signal(OverseerSignal::ActiveLeaves(
				ActiveLeavesUpdate::stop_work(test_state.relay_parent),
			)))
			.await;
		virtual_overseer
	});
}

// Test that a validator doesn't do any work on receiving a `CandidateBackingMessage::Statement`
// from a disabled validator
#[test]
fn validator_ignores_statements_from_disabled_validators() {
	let mut test_state = TestState::default();
	test_state.disabled_validators.push(ValidatorIndex(2));

	test_harness(test_state.keystore.clone(), |mut virtual_overseer| async move {
		test_startup(&mut virtual_overseer, &test_state).await;

		let pov = PoV { block_data: BlockData(vec![42, 43, 44]) };
		let pvd = dummy_pvd();
		let validation_code = ValidationCode(vec![1, 2, 3]);

		let expected_head_data = test_state.head_data.get(&test_state.chain_ids[0]).unwrap();

		let pov_hash = pov.hash();
		let candidate = TestCandidateBuilder {
			para_id: test_state.chain_ids[0],
			relay_parent: test_state.relay_parent,
			pov_hash,
			head_data: expected_head_data.clone(),
			erasure_root: make_erasure_root(&test_state, pov.clone(), pvd.clone()),
			persisted_validation_data_hash: pvd.hash(),
			validation_code: validation_code.0.clone(),
		}
		.build();
		let candidate_commitments_hash = candidate.commitments.hash();

		let public2 = Keystore::sr25519_generate_new(
			&*test_state.keystore,
			ValidatorId::ID,
			Some(&test_state.validators[2].to_seed()),
		)
		.expect("Insert key into keystore");

		let signed_2 = SignedFullStatementWithPVD::sign(
			&test_state.keystore,
			StatementWithPVD::Seconded(candidate.clone(), pvd.clone()),
			&test_state.signing_context,
			ValidatorIndex(2),
			&public2.into(),
		)
		.ok()
		.flatten()
		.expect("should be signed");

		let statement_2 =
			CandidateBackingMessage::Statement(test_state.relay_parent, signed_2.clone());

		virtual_overseer.send(FromOrchestra::Communication { msg: statement_2 }).await;

		// Ensure backing subsystem is not doing any work
		assert_matches!(virtual_overseer.recv().timeout(Duration::from_secs(1)).await, None);

		// Now send a statement from a honest validator and make sure it gets processed
		let public3 = Keystore::sr25519_generate_new(
			&*test_state.keystore,
			ValidatorId::ID,
			Some(&test_state.validators[3].to_seed()),
		)
		.expect("Insert key into keystore");

		let signed_3 = SignedFullStatementWithPVD::sign(
			&test_state.keystore,
			StatementWithPVD::Seconded(candidate.clone(), pvd.clone()),
			&test_state.signing_context,
			ValidatorIndex(3),
			&public3.into(),
		)
		.ok()
		.flatten()
		.expect("should be signed");

		let statement_3 =
			CandidateBackingMessage::Statement(test_state.relay_parent, signed_3.clone());

		virtual_overseer.send(FromOrchestra::Communication { msg: statement_3 }).await;

		assert_validation_requests(&mut virtual_overseer, validation_code.clone()).await;

		// Sending a `Statement::Seconded` for our assignment will start
		// validation process. The first thing requested is the PoV.
		assert_matches!(
			virtual_overseer.recv().await,
			AllMessages::AvailabilityDistribution(
				AvailabilityDistributionMessage::FetchPoV {
					relay_parent,
					tx,
					..
				}
			) if relay_parent == test_state.relay_parent => {
				tx.send(pov.clone()).unwrap();
			}
		);

		// The next step is the actual request to Validation subsystem
		// to validate the `Seconded` candidate.
		let expected_pov = pov;
		let expected_validation_code = validation_code;
		assert_matches!(
			virtual_overseer.recv().await,
			AllMessages::CandidateValidation(
				CandidateValidationMessage::ValidateFromExhaustive {
					validation_data,
					validation_code,
					candidate_receipt,
					pov,
					executor_params: _,
					exec_kind,
					response_sender,
				}
			) if validation_data == pvd &&
				validation_code == expected_validation_code &&
				*pov == expected_pov && &candidate_receipt.descriptor == candidate.descriptor() &&
				exec_kind == PvfExecKind::BackingSystemParas &&
				candidate_commitments_hash == candidate_receipt.commitments_hash =>
			{
				response_sender.send(Ok(
					ValidationResult::Valid(CandidateCommitments {
						head_data: expected_head_data.clone(),
						upward_messages: Default::default(),
						horizontal_messages: Default::default(),
						new_validation_code: None,
						processed_downward_messages: 0,
						hrmp_watermark: 0,
					}, test_state.validation_data.clone()),
				)).unwrap();
			}
		);

		assert_matches!(
			virtual_overseer.recv().await,
			AllMessages::AvailabilityStore(
				AvailabilityStoreMessage::StoreAvailableData { candidate_hash, tx, .. }
			) if candidate_hash == candidate.hash() => {
				tx.send(Ok(())).unwrap();
			}
		);

		assert_matches!(
			virtual_overseer.recv().await,
			AllMessages::StatementDistribution(
				StatementDistributionMessage::Share(hash, _stmt)
			) => {
				assert_eq!(test_state.relay_parent, hash);
			}
		);

		assert_matches!(
			virtual_overseer.recv().await,
			AllMessages::Provisioner(
				ProvisionerMessage::ProvisionableData(
					_,
					ProvisionableData::BackedCandidate(candidate_receipt)
				)
			) => {
				assert_eq!(candidate_receipt, candidate.to_plain());
			}
		);

		virtual_overseer
			.send(FromOrchestra::Signal(OverseerSignal::ActiveLeaves(
				ActiveLeavesUpdate::stop_work(test_state.relay_parent),
			)))
			.await;
		virtual_overseer
	});
}
