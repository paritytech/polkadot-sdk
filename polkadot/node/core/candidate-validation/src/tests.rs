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

use std::{
	collections::BTreeMap,
	sync::atomic::{AtomicUsize, Ordering},
};

use super::*;
use crate::PvfExecKind;
use assert_matches::assert_matches;
use futures::executor;
use polkadot_node_core_pvf::PrepareError;
use polkadot_node_primitives::BlockData;
use polkadot_node_subsystem::messages::AllMessages;
use polkadot_node_subsystem_test_helpers::{make_subsystem_context, TestSubsystemContextHandle};
use polkadot_node_subsystem_util::reexports::SubsystemContext;
use polkadot_overseer::ActivatedLeaf;
use polkadot_primitives::{
	vstaging::{
		CandidateDescriptorV2, CandidateDescriptorVersion, ClaimQueueOffset,
		CommittedCandidateReceiptError, CoreSelector, MutateDescriptorV2, UMPSignal, UMP_SEPARATOR,
	},
	CandidateDescriptor, CoreIndex, GroupIndex, HeadData, Id as ParaId, OccupiedCoreAssumption,
	SessionInfo, UpwardMessage, ValidatorId, DEFAULT_SCHEDULING_LOOKAHEAD,
};
use polkadot_primitives_test_helpers::{
	dummy_collator, dummy_collator_signature, dummy_hash, make_valid_candidate_descriptor,
	make_valid_candidate_descriptor_v2,
};
use rstest::rstest;
use sp_core::{sr25519::Public, testing::TaskExecutor};
use sp_keyring::Sr25519Keyring;
use sp_keystore::{testing::MemoryKeystore, Keystore};

const VALIDATION_CODE_BOMB_LIMIT: u32 = 30 * 1024 * 1024;

#[derive(Debug)]
enum AssumptionCheckOutcome {
	Matches(PersistedValidationData, ValidationCode),
	DoesNotMatch,
	BadRequest,
}

async fn check_assumption_validation_data<Sender>(
	sender: &mut Sender,
	descriptor: &CandidateDescriptor,
	assumption: OccupiedCoreAssumption,
) -> AssumptionCheckOutcome
where
	Sender: SubsystemSender<RuntimeApiMessage>,
{
	let validation_data = {
		let (tx, rx) = oneshot::channel();
		let d = runtime_api_request(
			sender,
			descriptor.relay_parent,
			RuntimeApiRequest::PersistedValidationData(descriptor.para_id, assumption, tx),
			rx,
		)
		.await;

		match d {
			Ok(None) | Err(RuntimeRequestFailed) => return AssumptionCheckOutcome::BadRequest,
			Ok(Some(d)) => d,
		}
	};

	let persisted_validation_data_hash = validation_data.hash();

	if descriptor.persisted_validation_data_hash == persisted_validation_data_hash {
		let (code_tx, code_rx) = oneshot::channel();
		let validation_code = runtime_api_request(
			sender,
			descriptor.relay_parent,
			RuntimeApiRequest::ValidationCode(descriptor.para_id, assumption, code_tx),
			code_rx,
		)
		.await;

		match validation_code {
			Ok(None) | Err(RuntimeRequestFailed) => AssumptionCheckOutcome::BadRequest,
			Ok(Some(v)) => AssumptionCheckOutcome::Matches(validation_data, v),
		}
	} else {
		AssumptionCheckOutcome::DoesNotMatch
	}
}

#[test]
fn correctly_checks_included_assumption() {
	let validation_data: PersistedValidationData = Default::default();
	let validation_code: ValidationCode = vec![1, 2, 3].into();

	let persisted_validation_data_hash = validation_data.hash();
	let relay_parent = [2; 32].into();
	let para_id = ParaId::from(5_u32);

	let descriptor = make_valid_candidate_descriptor(
		para_id,
		relay_parent,
		persisted_validation_data_hash,
		dummy_hash(),
		dummy_hash(),
		dummy_hash(),
		dummy_hash(),
		Sr25519Keyring::Alice,
	)
	.into();

	let pool = TaskExecutor::new();
	let (mut ctx, mut ctx_handle) = make_subsystem_context::<AllMessages, _>(pool.clone());

	let (check_fut, check_result) = check_assumption_validation_data(
		ctx.sender(),
		&descriptor,
		OccupiedCoreAssumption::Included,
	)
	.remote_handle();

	let test_fut = async move {
		assert_matches!(
			ctx_handle.recv().await,
			AllMessages::RuntimeApi(RuntimeApiMessage::Request(
				rp,
				RuntimeApiRequest::PersistedValidationData(
					p,
					OccupiedCoreAssumption::Included,
					tx
				),
			)) => {
				assert_eq!(rp, relay_parent);
				assert_eq!(p, para_id);

				let _ = tx.send(Ok(Some(validation_data.clone())));
			}
		);

		assert_matches!(
			ctx_handle.recv().await,
			AllMessages::RuntimeApi(RuntimeApiMessage::Request(
				rp,
				RuntimeApiRequest::ValidationCode(p, OccupiedCoreAssumption::Included, tx)
			)) => {
				assert_eq!(rp, relay_parent);
				assert_eq!(p, para_id);

				let _ = tx.send(Ok(Some(validation_code.clone())));
			}
		);

		assert_matches!(check_result.await, AssumptionCheckOutcome::Matches(o, v) => {
			assert_eq!(o, validation_data);
			assert_eq!(v, validation_code);
		});
	};

	let test_fut = future::join(test_fut, check_fut);
	executor::block_on(test_fut);
}

#[test]
fn correctly_checks_timed_out_assumption() {
	let validation_data: PersistedValidationData = Default::default();
	let validation_code: ValidationCode = vec![1, 2, 3].into();

	let persisted_validation_data_hash = validation_data.hash();
	let relay_parent = [2; 32].into();
	let para_id = ParaId::from(5_u32);

	let descriptor = make_valid_candidate_descriptor(
		para_id,
		relay_parent,
		persisted_validation_data_hash,
		dummy_hash(),
		dummy_hash(),
		dummy_hash(),
		dummy_hash(),
		Sr25519Keyring::Alice,
	)
	.into();

	let pool = TaskExecutor::new();
	let (mut ctx, mut ctx_handle) = make_subsystem_context::<AllMessages, _>(pool.clone());

	let (check_fut, check_result) = check_assumption_validation_data(
		ctx.sender(),
		&descriptor,
		OccupiedCoreAssumption::TimedOut,
	)
	.remote_handle();

	let test_fut = async move {
		assert_matches!(
			ctx_handle.recv().await,
			AllMessages::RuntimeApi(RuntimeApiMessage::Request(
				rp,
				RuntimeApiRequest::PersistedValidationData(
					p,
					OccupiedCoreAssumption::TimedOut,
					tx
				),
			)) => {
				assert_eq!(rp, relay_parent);
				assert_eq!(p, para_id);

				let _ = tx.send(Ok(Some(validation_data.clone())));
			}
		);

		assert_matches!(
			ctx_handle.recv().await,
			AllMessages::RuntimeApi(RuntimeApiMessage::Request(
				rp,
				RuntimeApiRequest::ValidationCode(p, OccupiedCoreAssumption::TimedOut, tx)
			)) => {
				assert_eq!(rp, relay_parent);
				assert_eq!(p, para_id);

				let _ = tx.send(Ok(Some(validation_code.clone())));
			}
		);

		assert_matches!(check_result.await, AssumptionCheckOutcome::Matches(o, v) => {
			assert_eq!(o, validation_data);
			assert_eq!(v, validation_code);
		});
	};

	let test_fut = future::join(test_fut, check_fut);
	executor::block_on(test_fut);
}

#[test]
fn check_is_bad_request_if_no_validation_data() {
	let validation_data: PersistedValidationData = Default::default();
	let persisted_validation_data_hash = validation_data.hash();
	let relay_parent = [2; 32].into();
	let para_id = ParaId::from(5_u32);

	let descriptor = make_valid_candidate_descriptor(
		para_id,
		relay_parent,
		persisted_validation_data_hash,
		dummy_hash(),
		dummy_hash(),
		dummy_hash(),
		dummy_hash(),
		Sr25519Keyring::Alice,
	)
	.into();

	let pool = TaskExecutor::new();
	let (mut ctx, mut ctx_handle) = make_subsystem_context::<AllMessages, _>(pool.clone());

	let (check_fut, check_result) = check_assumption_validation_data(
		ctx.sender(),
		&descriptor,
		OccupiedCoreAssumption::Included,
	)
	.remote_handle();

	let test_fut = async move {
		assert_matches!(
			ctx_handle.recv().await,
			AllMessages::RuntimeApi(RuntimeApiMessage::Request(
				rp,
				RuntimeApiRequest::PersistedValidationData(
					p,
					OccupiedCoreAssumption::Included,
					tx
				),
			)) => {
				assert_eq!(rp, relay_parent);
				assert_eq!(p, para_id);

				let _ = tx.send(Ok(None));
			}
		);

		assert_matches!(check_result.await, AssumptionCheckOutcome::BadRequest);
	};

	let test_fut = future::join(test_fut, check_fut);
	executor::block_on(test_fut);
}

#[test]
fn check_is_bad_request_if_no_validation_code() {
	let validation_data: PersistedValidationData = Default::default();
	let persisted_validation_data_hash = validation_data.hash();
	let relay_parent = [2; 32].into();
	let para_id = ParaId::from(5_u32);

	let descriptor = make_valid_candidate_descriptor(
		para_id,
		relay_parent,
		persisted_validation_data_hash,
		dummy_hash(),
		dummy_hash(),
		dummy_hash(),
		dummy_hash(),
		Sr25519Keyring::Alice,
	)
	.into();

	let pool = TaskExecutor::new();
	let (mut ctx, mut ctx_handle) = make_subsystem_context::<AllMessages, _>(pool.clone());

	let (check_fut, check_result) = check_assumption_validation_data(
		ctx.sender(),
		&descriptor,
		OccupiedCoreAssumption::TimedOut,
	)
	.remote_handle();

	let test_fut = async move {
		assert_matches!(
			ctx_handle.recv().await,
			AllMessages::RuntimeApi(RuntimeApiMessage::Request(
				rp,
				RuntimeApiRequest::PersistedValidationData(
					p,
					OccupiedCoreAssumption::TimedOut,
					tx
				),
			)) => {
				assert_eq!(rp, relay_parent);
				assert_eq!(p, para_id);

				let _ = tx.send(Ok(Some(validation_data.clone())));
			}
		);

		assert_matches!(
			ctx_handle.recv().await,
			AllMessages::RuntimeApi(RuntimeApiMessage::Request(
				rp,
				RuntimeApiRequest::ValidationCode(p, OccupiedCoreAssumption::TimedOut, tx)
			)) => {
				assert_eq!(rp, relay_parent);
				assert_eq!(p, para_id);

				let _ = tx.send(Ok(None));
			}
		);

		assert_matches!(check_result.await, AssumptionCheckOutcome::BadRequest);
	};

	let test_fut = future::join(test_fut, check_fut);
	executor::block_on(test_fut);
}

#[test]
fn check_does_not_match() {
	let validation_data: PersistedValidationData = Default::default();
	let relay_parent = Hash::repeat_byte(0x02);
	let para_id = ParaId::from(5_u32);

	let descriptor = make_valid_candidate_descriptor(
		para_id,
		relay_parent,
		Hash::from([3; 32]),
		dummy_hash(),
		dummy_hash(),
		dummy_hash(),
		dummy_hash(),
		Sr25519Keyring::Alice,
	)
	.into();

	let pool = TaskExecutor::new();
	let (mut ctx, mut ctx_handle) = make_subsystem_context::<AllMessages, _>(pool.clone());

	let (check_fut, check_result) = check_assumption_validation_data(
		ctx.sender(),
		&descriptor,
		OccupiedCoreAssumption::Included,
	)
	.remote_handle();

	let test_fut = async move {
		assert_matches!(
			ctx_handle.recv().await,
			AllMessages::RuntimeApi(RuntimeApiMessage::Request(
				rp,
				RuntimeApiRequest::PersistedValidationData(
					p,
					OccupiedCoreAssumption::Included,
					tx
				),
			)) => {
				assert_eq!(rp, relay_parent);
				assert_eq!(p, para_id);

				let _ = tx.send(Ok(Some(validation_data.clone())));
			}
		);

		assert_matches!(check_result.await, AssumptionCheckOutcome::DoesNotMatch);
	};

	let test_fut = future::join(test_fut, check_fut);
	executor::block_on(test_fut);
}

struct MockValidateCandidateBackend {
	result_list: Vec<Result<WasmValidationResult, ValidationError>>,
	num_times_called: usize,
}

impl MockValidateCandidateBackend {
	fn with_hardcoded_result(result: Result<WasmValidationResult, ValidationError>) -> Self {
		Self { result_list: vec![result], num_times_called: 0 }
	}

	fn with_hardcoded_result_list(
		result_list: Vec<Result<WasmValidationResult, ValidationError>>,
	) -> Self {
		Self { result_list, num_times_called: 0 }
	}
}

#[async_trait]
impl ValidationBackend for MockValidateCandidateBackend {
	async fn validate_candidate(
		&mut self,
		_pvf: PvfPrepData,
		_timeout: Duration,
		_pvd: Arc<PersistedValidationData>,
		_pov: Arc<PoV>,
		_prepare_priority: polkadot_node_core_pvf::Priority,
		_exec_kind: PvfExecKind,
	) -> Result<WasmValidationResult, ValidationError> {
		// This is expected to panic if called more times than expected, indicating an error in the
		// test.
		let result = self.result_list[self.num_times_called].clone();
		self.num_times_called += 1;

		result
	}

	async fn precheck_pvf(&mut self, _pvf: PvfPrepData) -> Result<(), PrepareError> {
		unreachable!()
	}

	async fn heads_up(&mut self, _active_pvfs: Vec<PvfPrepData>) -> Result<(), String> {
		unreachable!()
	}

	async fn update_active_leaves(
		&mut self,
		_update: ActiveLeavesUpdate,
		_ancestors: Vec<Hash>,
	) -> Result<(), String> {
		unreachable!()
	}
}

#[rstest]
#[case(true)]
#[case(false)]
fn candidate_validation_ok_is_ok(#[case] v2_descriptor: bool) {
	let validation_data = PersistedValidationData { max_pov_size: 1024, ..Default::default() };

	let pov = PoV { block_data: BlockData(vec![1; 32]) };
	let head_data = HeadData(vec![1, 1, 1]);
	let validation_code = ValidationCode(vec![2; 16]);

	let descriptor = if v2_descriptor {
		make_valid_candidate_descriptor_v2(
			ParaId::from(1_u32),
			dummy_hash(),
			CoreIndex(1),
			1,
			dummy_hash(),
			pov.hash(),
			validation_code.hash(),
			head_data.hash(),
			dummy_hash(),
		)
	} else {
		make_valid_candidate_descriptor(
			ParaId::from(1_u32),
			dummy_hash(),
			validation_data.hash(),
			pov.hash(),
			validation_code.hash(),
			head_data.hash(),
			dummy_hash(),
			Sr25519Keyring::Alice,
		)
		.into()
	};

	let check = perform_basic_checks(
		&descriptor,
		validation_data.max_pov_size,
		&pov,
		&validation_code.hash(),
	);
	assert!(check.is_ok());

	let mut validation_result = WasmValidationResult {
		head_data,
		new_validation_code: Some(vec![2, 2, 2].into()),
		upward_messages: Default::default(),
		horizontal_messages: Default::default(),
		processed_downward_messages: 0,
		hrmp_watermark: 0,
	};

	if v2_descriptor {
		validation_result.upward_messages.force_push(UMP_SEPARATOR);
		validation_result
			.upward_messages
			.force_push(UMPSignal::SelectCore(CoreSelector(0), ClaimQueueOffset(1)).encode());
		validation_result
			.upward_messages
			.force_push(UMPSignal::ApprovedPeer(vec![1, 2, 3].try_into().unwrap()).encode());
	}

	let commitments = CandidateCommitments {
		head_data: validation_result.head_data.clone(),
		upward_messages: validation_result.upward_messages.clone(),
		horizontal_messages: validation_result.horizontal_messages.clone(),
		new_validation_code: validation_result.new_validation_code.clone(),
		processed_downward_messages: validation_result.processed_downward_messages,
		hrmp_watermark: validation_result.hrmp_watermark,
	};

	let candidate_receipt = CandidateReceipt { descriptor, commitments_hash: commitments.hash() };
	let mut cq = BTreeMap::new();
	let _ = cq.insert(CoreIndex(0), vec![1.into(), 2.into()].into());
	let _ = cq.insert(CoreIndex(1), vec![1.into(), 1.into()].into());

	let v = executor::block_on(validate_candidate_exhaustive(
		1,
		MockValidateCandidateBackend::with_hardcoded_result(Ok(validation_result)),
		validation_data.clone(),
		validation_code,
		candidate_receipt,
		Arc::new(pov),
		ExecutorParams::default(),
		PvfExecKind::Backing(dummy_hash()),
		&Default::default(),
		Some(ClaimQueueSnapshot(cq)),
		VALIDATION_CODE_BOMB_LIMIT,
	))
	.unwrap();

	assert_matches!(v, ValidationResult::Valid(outputs, used_validation_data) => {
		assert_eq!(outputs.head_data, HeadData(vec![1, 1, 1]));
		assert_eq!(outputs.upward_messages, commitments.upward_messages);
		assert_eq!(outputs.horizontal_messages, Vec::new());
		assert_eq!(outputs.new_validation_code, Some(vec![2, 2, 2].into()));
		assert_eq!(outputs.hrmp_watermark, 0);
		assert_eq!(used_validation_data, validation_data);
	});
}

#[test]
// Test v2 receipt validation in the following scenarios:
// - v2 receipt with mismatching session index in descriptor
// - v2 candidate has no assignments but a core selector is present
// - v1 candidate that outputs a UMP signal is invalid.
// - v2 candidate that outputs an approved peer id is valid.
// Also check that the validation of invalid candidates only fail during backing checks.
fn invalid_session_or_ump_signals() {
	let validation_data = PersistedValidationData { max_pov_size: 1024, ..Default::default() };

	let pov: PoV = PoV { block_data: BlockData(vec![1; 32]) };
	let head_data = HeadData(vec![1, 1, 1]);
	let validation_code = ValidationCode(vec![2; 16]);

	let descriptor = make_valid_candidate_descriptor_v2(
		ParaId::from(1_u32),
		dummy_hash(),
		CoreIndex(1),
		100,
		dummy_hash(),
		pov.hash(),
		validation_code.hash(),
		head_data.hash(),
		dummy_hash(),
	);

	let check = perform_basic_checks(
		&descriptor,
		validation_data.max_pov_size,
		&pov,
		&validation_code.hash(),
	);
	assert!(check.is_ok());

	let mut validation_result = WasmValidationResult {
		head_data: head_data.clone(),
		new_validation_code: Some(vec![2, 2, 2].into()),
		upward_messages: Default::default(),
		horizontal_messages: Default::default(),
		processed_downward_messages: 0,
		hrmp_watermark: 0,
	};

	validation_result.upward_messages.force_push(UMP_SEPARATOR);
	validation_result
		.upward_messages
		.force_push(UMPSignal::SelectCore(CoreSelector(1), ClaimQueueOffset(0)).encode());

	let commitments = CandidateCommitments {
		head_data: validation_result.head_data.clone(),
		upward_messages: validation_result.upward_messages.clone(),
		horizontal_messages: validation_result.horizontal_messages.clone(),
		new_validation_code: validation_result.new_validation_code.clone(),
		processed_downward_messages: validation_result.processed_downward_messages,
		hrmp_watermark: validation_result.hrmp_watermark,
	};

	let mut candidate_receipt =
		CandidateReceipt { descriptor, commitments_hash: commitments.hash() };

	// Session index specified in CandidateDescriptor does not match expected session.
	for exec_kind in
		[PvfExecKind::Backing(dummy_hash()), PvfExecKind::BackingSystemParas(dummy_hash())]
	{
		let err = executor::block_on(validate_candidate_exhaustive(
			1,
			MockValidateCandidateBackend::with_hardcoded_result(Ok(validation_result.clone())),
			validation_data.clone(),
			validation_code.clone(),
			candidate_receipt.clone(),
			Arc::new(pov.clone()),
			ExecutorParams::default(),
			exec_kind,
			&Default::default(),
			Default::default(),
			VALIDATION_CODE_BOMB_LIMIT,
		))
		.unwrap();

		assert_matches!(err, ValidationResult::Invalid(InvalidCandidate::InvalidSessionIndex));
	}

	candidate_receipt.descriptor.set_session_index(1);

	// Candidate has no assignments but a core selector.
	for exec_kind in
		[PvfExecKind::Backing(dummy_hash()), PvfExecKind::BackingSystemParas(dummy_hash())]
	{
		let result = executor::block_on(validate_candidate_exhaustive(
			1,
			MockValidateCandidateBackend::with_hardcoded_result(Ok(validation_result.clone())),
			validation_data.clone(),
			validation_code.clone(),
			candidate_receipt.clone(),
			Arc::new(pov.clone()),
			ExecutorParams::default(),
			exec_kind,
			&Default::default(),
			Some(Default::default()),
			VALIDATION_CODE_BOMB_LIMIT,
		))
		.unwrap();
		assert_matches!(
			result,
			ValidationResult::Invalid(InvalidCandidate::InvalidUMPSignals(
				CommittedCandidateReceiptError::NoAssignment
			))
		);
	}

	// Validation doesn't fail for approvals and disputes, core/session index is not checked.
	for exec_kind in [PvfExecKind::Approval, PvfExecKind::Dispute] {
		let v = executor::block_on(validate_candidate_exhaustive(
			1,
			MockValidateCandidateBackend::with_hardcoded_result(Ok(validation_result.clone())),
			validation_data.clone(),
			validation_code.clone(),
			candidate_receipt.clone(),
			Arc::new(pov.clone()),
			ExecutorParams::default(),
			exec_kind,
			&Default::default(),
			Default::default(),
			VALIDATION_CODE_BOMB_LIMIT,
		))
		.unwrap();

		assert_matches!(v, ValidationResult::Valid(outputs, used_validation_data) => {
			assert_eq!(outputs.head_data, HeadData(vec![1, 1, 1]));
			assert_eq!(outputs.upward_messages, commitments.upward_messages);
			assert_eq!(outputs.horizontal_messages, Vec::new());
			assert_eq!(outputs.new_validation_code, Some(vec![2, 2, 2].into()));
			assert_eq!(outputs.hrmp_watermark, 0);
			assert_eq!(used_validation_data, validation_data);
		});
	}

	// Populate claim queue.
	let mut cq = BTreeMap::new();
	let _ = cq.insert(CoreIndex(0), vec![1.into(), 2.into()].into());
	let _ = cq.insert(CoreIndex(1), vec![1.into(), 2.into()].into());

	for exec_kind in
		[PvfExecKind::Backing(dummy_hash()), PvfExecKind::BackingSystemParas(dummy_hash())]
	{
		let v = executor::block_on(validate_candidate_exhaustive(
			1,
			MockValidateCandidateBackend::with_hardcoded_result(Ok(validation_result.clone())),
			validation_data.clone(),
			validation_code.clone(),
			candidate_receipt.clone(),
			Arc::new(pov.clone()),
			ExecutorParams::default(),
			exec_kind,
			&Default::default(),
			Some(ClaimQueueSnapshot(cq.clone())),
			VALIDATION_CODE_BOMB_LIMIT,
		))
		.unwrap();

		assert_matches!(v, ValidationResult::Valid(outputs, used_validation_data) => {
			assert_eq!(outputs.head_data, HeadData(vec![1, 1, 1]));
			assert_eq!(outputs.upward_messages, commitments.upward_messages);
			assert_eq!(outputs.horizontal_messages, Vec::new());
			assert_eq!(outputs.new_validation_code, Some(vec![2, 2, 2].into()));
			assert_eq!(outputs.hrmp_watermark, 0);
			assert_eq!(used_validation_data, validation_data);
		});
	}

	// Test that a v1 candidate that outputs the core selector UMP signal is invalid.
	let descriptor_v1 = make_valid_candidate_descriptor(
		ParaId::from(1_u32),
		dummy_hash(),
		dummy_hash(),
		pov.hash(),
		validation_code.hash(),
		validation_result.head_data.hash(),
		dummy_hash(),
		sp_keyring::Sr25519Keyring::Ferdie,
	);
	let descriptor: CandidateDescriptorV2 = descriptor_v1.into();

	perform_basic_checks(&descriptor, validation_data.max_pov_size, &pov, &validation_code.hash())
		.unwrap();
	assert_eq!(descriptor.version(), CandidateDescriptorVersion::V1);
	let candidate_receipt = CandidateReceipt { descriptor, commitments_hash: commitments.hash() };

	for exec_kind in
		[PvfExecKind::Backing(dummy_hash()), PvfExecKind::BackingSystemParas(dummy_hash())]
	{
		let result = executor::block_on(validate_candidate_exhaustive(
			1,
			MockValidateCandidateBackend::with_hardcoded_result(Ok(validation_result.clone())),
			validation_data.clone(),
			validation_code.clone(),
			candidate_receipt.clone(),
			Arc::new(pov.clone()),
			ExecutorParams::default(),
			exec_kind,
			&Default::default(),
			Some(Default::default()),
			VALIDATION_CODE_BOMB_LIMIT,
		))
		.unwrap();
		assert_matches!(
			result,
			ValidationResult::Invalid(InvalidCandidate::InvalidUMPSignals(
				CommittedCandidateReceiptError::UMPSignalWithV1Decriptor
			))
		);
	}

	// Validation doesn't fail for approvals and disputes, ump signals are not checked.
	for exec_kind in [PvfExecKind::Approval, PvfExecKind::Dispute] {
		let v = executor::block_on(validate_candidate_exhaustive(
			1,
			MockValidateCandidateBackend::with_hardcoded_result(Ok(validation_result.clone())),
			validation_data.clone(),
			validation_code.clone(),
			candidate_receipt.clone(),
			Arc::new(pov.clone()),
			ExecutorParams::default(),
			exec_kind,
			&Default::default(),
			Default::default(),
			VALIDATION_CODE_BOMB_LIMIT,
		))
		.unwrap();

		assert_matches!(v, ValidationResult::Valid(outputs, used_validation_data) => {
			assert_eq!(outputs.head_data, HeadData(vec![1, 1, 1]));
			assert_eq!(outputs.upward_messages, commitments.upward_messages);
			assert_eq!(outputs.horizontal_messages, Vec::new());
			assert_eq!(outputs.new_validation_code, Some(vec![2, 2, 2].into()));
			assert_eq!(outputs.hrmp_watermark, 0);
			assert_eq!(used_validation_data, validation_data);
		});
	}

	// Test that a v2 candidate that outputs an approved peer id is valid.
	let descriptor = make_valid_candidate_descriptor_v2(
		ParaId::from(1_u32),
		dummy_hash(),
		CoreIndex(1),
		1,
		dummy_hash(),
		pov.hash(),
		validation_code.hash(),
		head_data.hash(),
		dummy_hash(),
	);
	let mut validation_result = validation_result.clone();

	validation_result
		.upward_messages
		.force_push(UMPSignal::ApprovedPeer(vec![1, 2, 3].try_into().unwrap()).encode());

	let mut commitments = commitments.clone();
	commitments.upward_messages = validation_result.upward_messages.clone();

	let candidate_receipt = CandidateReceipt { descriptor, commitments_hash: commitments.hash() };
	for exec_kind in
		[PvfExecKind::Backing(dummy_hash()), PvfExecKind::BackingSystemParas(dummy_hash())]
	{
		let v = executor::block_on(validate_candidate_exhaustive(
			1,
			MockValidateCandidateBackend::with_hardcoded_result(Ok(validation_result.clone())),
			validation_data.clone(),
			validation_code.clone(),
			candidate_receipt.clone(),
			Arc::new(pov.clone()),
			ExecutorParams::default(),
			exec_kind,
			&Default::default(),
			Some(ClaimQueueSnapshot(cq.clone())),
			VALIDATION_CODE_BOMB_LIMIT,
		))
		.unwrap();

		assert_matches!(v, ValidationResult::Valid(outputs, used_validation_data) => {
			assert_eq!(outputs.head_data, HeadData(vec![1, 1, 1]));
			assert_eq!(outputs.upward_messages, commitments.upward_messages);
			assert_eq!(outputs.horizontal_messages, Vec::new());
			assert_eq!(outputs.new_validation_code, Some(vec![2, 2, 2].into()));
			assert_eq!(outputs.hrmp_watermark, 0);
			assert_eq!(used_validation_data, validation_data);
		});
	}

	// Validation also doesn't fail for approvals and disputes.
	for exec_kind in [PvfExecKind::Approval, PvfExecKind::Dispute] {
		let v = executor::block_on(validate_candidate_exhaustive(
			1,
			MockValidateCandidateBackend::with_hardcoded_result(Ok(validation_result.clone())),
			validation_data.clone(),
			validation_code.clone(),
			candidate_receipt.clone(),
			Arc::new(pov.clone()),
			ExecutorParams::default(),
			exec_kind,
			&Default::default(),
			Some(ClaimQueueSnapshot(cq.clone())),
			VALIDATION_CODE_BOMB_LIMIT,
		))
		.unwrap();

		assert_matches!(v, ValidationResult::Valid(outputs, used_validation_data) => {
			assert_eq!(outputs.head_data, HeadData(vec![1, 1, 1]));
			assert_eq!(outputs.upward_messages, commitments.upward_messages);
			assert_eq!(outputs.horizontal_messages, Vec::new());
			assert_eq!(outputs.new_validation_code, Some(vec![2, 2, 2].into()));
			assert_eq!(outputs.hrmp_watermark, 0);
			assert_eq!(used_validation_data, validation_data);
		});
	}
}

#[test]
fn candidate_validation_bad_return_is_invalid() {
	let validation_data = PersistedValidationData { max_pov_size: 1024, ..Default::default() };

	let pov = PoV { block_data: BlockData(vec![1; 32]) };
	let validation_code = ValidationCode(vec![2; 16]);

	let descriptor = make_valid_candidate_descriptor(
		ParaId::from(1_u32),
		dummy_hash(),
		validation_data.hash(),
		pov.hash(),
		validation_code.hash(),
		dummy_hash(),
		dummy_hash(),
		Sr25519Keyring::Alice,
	)
	.into();

	let check = perform_basic_checks(
		&descriptor,
		validation_data.max_pov_size,
		&pov,
		&validation_code.hash(),
	);
	assert!(check.is_ok());

	let candidate_receipt = CandidateReceipt { descriptor, commitments_hash: Hash::zero() };

	let v = executor::block_on(validate_candidate_exhaustive(
		1,
		MockValidateCandidateBackend::with_hardcoded_result(Err(ValidationError::Invalid(
			WasmInvalidCandidate::HardTimeout,
		))),
		validation_data,
		validation_code,
		candidate_receipt,
		Arc::new(pov),
		ExecutorParams::default(),
		PvfExecKind::Backing(dummy_hash()),
		&Default::default(),
		Default::default(),
		VALIDATION_CODE_BOMB_LIMIT,
	))
	.unwrap();

	assert_matches!(v, ValidationResult::Invalid(InvalidCandidate::Timeout));
}

fn perform_basic_checks_on_valid_candidate(
	pov: &PoV,
	validation_code: &ValidationCode,
	validation_data: &PersistedValidationData,
	head_data_hash: Hash,
) -> CandidateDescriptorV2 {
	let descriptor = make_valid_candidate_descriptor(
		ParaId::from(1_u32),
		dummy_hash(),
		validation_data.hash(),
		pov.hash(),
		validation_code.hash(),
		head_data_hash,
		head_data_hash,
		Sr25519Keyring::Alice,
	)
	.into();

	let check = perform_basic_checks(
		&descriptor,
		validation_data.max_pov_size,
		&pov,
		&validation_code.hash(),
	);
	assert!(check.is_ok());
	descriptor
}

// Test that we vote valid if we get `AmbiguousWorkerDeath`, retry, and then succeed.
#[test]
fn candidate_validation_one_ambiguous_error_is_valid() {
	let validation_data = PersistedValidationData { max_pov_size: 1024, ..Default::default() };

	let pov = PoV { block_data: BlockData(vec![1; 32]) };
	let head_data = HeadData(vec![1, 1, 1]);
	let validation_code = ValidationCode(vec![2; 16]);

	let descriptor = perform_basic_checks_on_valid_candidate(
		&pov,
		&validation_code,
		&validation_data,
		head_data.hash(),
	);

	let validation_result = WasmValidationResult {
		head_data,
		new_validation_code: Some(vec![2, 2, 2].into()),
		upward_messages: Default::default(),
		horizontal_messages: Default::default(),
		processed_downward_messages: 0,
		hrmp_watermark: 0,
	};

	let commitments = CandidateCommitments {
		head_data: validation_result.head_data.clone(),
		upward_messages: validation_result.upward_messages.clone(),
		horizontal_messages: validation_result.horizontal_messages.clone(),
		new_validation_code: validation_result.new_validation_code.clone(),
		processed_downward_messages: validation_result.processed_downward_messages,
		hrmp_watermark: validation_result.hrmp_watermark,
	};

	let candidate_receipt = CandidateReceipt { descriptor, commitments_hash: commitments.hash() };

	let v = executor::block_on(validate_candidate_exhaustive(
		1,
		MockValidateCandidateBackend::with_hardcoded_result_list(vec![
			Err(ValidationError::PossiblyInvalid(PossiblyInvalidError::AmbiguousWorkerDeath)),
			Ok(validation_result),
		]),
		validation_data.clone(),
		validation_code,
		candidate_receipt,
		Arc::new(pov),
		ExecutorParams::default(),
		PvfExecKind::Approval,
		&Default::default(),
		Default::default(),
		VALIDATION_CODE_BOMB_LIMIT,
	))
	.unwrap();

	assert_matches!(v, ValidationResult::Valid(outputs, used_validation_data) => {
		assert_eq!(outputs.head_data, HeadData(vec![1, 1, 1]));
		assert_eq!(outputs.upward_messages, Vec::<UpwardMessage>::new());
		assert_eq!(outputs.horizontal_messages, Vec::new());
		assert_eq!(outputs.new_validation_code, Some(vec![2, 2, 2].into()));
		assert_eq!(outputs.hrmp_watermark, 0);
		assert_eq!(used_validation_data, validation_data);
	});
}

#[test]
fn candidate_validation_multiple_ambiguous_errors_is_invalid() {
	let validation_data = PersistedValidationData { max_pov_size: 1024, ..Default::default() };

	let pov = PoV { block_data: BlockData(vec![1; 32]) };
	let validation_code = ValidationCode(vec![2; 16]);

	let descriptor = perform_basic_checks_on_valid_candidate(
		&pov,
		&validation_code,
		&validation_data,
		dummy_hash(),
	);

	let candidate_receipt = CandidateReceipt { descriptor, commitments_hash: Hash::zero() };

	let v = executor::block_on(validate_candidate_exhaustive(
		1,
		MockValidateCandidateBackend::with_hardcoded_result_list(vec![
			Err(ValidationError::PossiblyInvalid(PossiblyInvalidError::AmbiguousWorkerDeath)),
			Err(ValidationError::PossiblyInvalid(PossiblyInvalidError::AmbiguousWorkerDeath)),
		]),
		validation_data,
		validation_code,
		candidate_receipt,
		Arc::new(pov),
		ExecutorParams::default(),
		PvfExecKind::Approval,
		&Default::default(),
		Default::default(),
		VALIDATION_CODE_BOMB_LIMIT,
	))
	.unwrap();

	assert_matches!(v, ValidationResult::Invalid(InvalidCandidate::ExecutionError(_)));
}

// Test that we retry for approval on internal errors.
#[test]
fn candidate_validation_retry_internal_errors() {
	let v = candidate_validation_retry_on_error_helper(
		PvfExecKind::Approval,
		vec![
			Err(InternalValidationError::HostCommunication("foo".into()).into()),
			// Throw an AJD error, we should still retry again.
			Err(ValidationError::PossiblyInvalid(PossiblyInvalidError::AmbiguousJobDeath(
				"baz".into(),
			))),
			// Throw another internal error.
			Err(InternalValidationError::HostCommunication("bar".into()).into()),
		],
	);
	assert_matches!(v, Err(ValidationFailed(s)) if s.contains("bar"));
}

// Test that we don't retry for backing on internal errors.
#[test]
fn candidate_validation_dont_retry_internal_errors() {
	let v = candidate_validation_retry_on_error_helper(
		PvfExecKind::Backing(dummy_hash()),
		vec![
			Err(InternalValidationError::HostCommunication("foo".into()).into()),
			// Throw an AWD error, we should still retry again.
			Err(ValidationError::PossiblyInvalid(PossiblyInvalidError::AmbiguousWorkerDeath)),
			// Throw another internal error.
			Err(InternalValidationError::HostCommunication("bar".into()).into()),
		],
	);

	assert_matches!(v, Err(ValidationFailed(s)) if s.contains("foo"));
}

// Test that we retry for approval on panic errors.
#[test]
fn candidate_validation_retry_panic_errors() {
	let v = candidate_validation_retry_on_error_helper(
		PvfExecKind::Approval,
		vec![
			Err(ValidationError::PossiblyInvalid(PossiblyInvalidError::JobError("foo".into()))),
			// Throw an AWD error, we should still retry again.
			Err(ValidationError::PossiblyInvalid(PossiblyInvalidError::AmbiguousWorkerDeath)),
			// Throw another panic error.
			Err(ValidationError::PossiblyInvalid(PossiblyInvalidError::JobError("bar".into()))),
		],
	);

	assert_matches!(v, Ok(ValidationResult::Invalid(InvalidCandidate::ExecutionError(s))) if s == "bar".to_string());
}

// Test that we don't retry for backing on panic errors.
#[test]
fn candidate_validation_dont_retry_panic_errors() {
	let v = candidate_validation_retry_on_error_helper(
		PvfExecKind::Backing(dummy_hash()),
		vec![
			Err(ValidationError::PossiblyInvalid(PossiblyInvalidError::JobError("foo".into()))),
			// Throw an AWD error, we should still retry again.
			Err(ValidationError::PossiblyInvalid(PossiblyInvalidError::AmbiguousWorkerDeath)),
			// Throw another panic error.
			Err(ValidationError::PossiblyInvalid(PossiblyInvalidError::JobError("bar".into()))),
		],
	);

	assert_matches!(v, Ok(ValidationResult::Invalid(InvalidCandidate::ExecutionError(s))) if s == "foo".to_string());
}

fn candidate_validation_retry_on_error_helper(
	exec_kind: PvfExecKind,
	mock_errors: Vec<Result<WasmValidationResult, ValidationError>>,
) -> Result<ValidationResult, ValidationFailed> {
	let validation_data = PersistedValidationData { max_pov_size: 1024, ..Default::default() };

	let pov = PoV { block_data: BlockData(vec![1; 32]) };
	let validation_code = ValidationCode(vec![2; 16]);

	let descriptor = make_valid_candidate_descriptor(
		ParaId::from(1_u32),
		dummy_hash(),
		validation_data.hash(),
		pov.hash(),
		validation_code.hash(),
		dummy_hash(),
		dummy_hash(),
		Sr25519Keyring::Alice,
	)
	.into();

	let check = perform_basic_checks(
		&descriptor,
		validation_data.max_pov_size,
		&pov,
		&validation_code.hash(),
	);
	assert!(check.is_ok());

	let candidate_receipt = CandidateReceipt { descriptor, commitments_hash: Hash::zero() };

	return executor::block_on(validate_candidate_exhaustive(
		1,
		MockValidateCandidateBackend::with_hardcoded_result_list(mock_errors),
		validation_data,
		validation_code,
		candidate_receipt,
		Arc::new(pov),
		ExecutorParams::default(),
		exec_kind,
		&Default::default(),
		Default::default(),
		VALIDATION_CODE_BOMB_LIMIT,
	))
}

#[test]
fn candidate_validation_timeout_is_internal_error() {
	let validation_data = PersistedValidationData { max_pov_size: 1024, ..Default::default() };

	let pov = PoV { block_data: BlockData(vec![1; 32]) };
	let validation_code = ValidationCode(vec![2; 16]);

	let descriptor = make_valid_candidate_descriptor(
		ParaId::from(1_u32),
		dummy_hash(),
		validation_data.hash(),
		pov.hash(),
		validation_code.hash(),
		dummy_hash(),
		dummy_hash(),
		Sr25519Keyring::Alice,
	)
	.into();

	let check = perform_basic_checks(
		&descriptor,
		validation_data.max_pov_size,
		&pov,
		&validation_code.hash(),
	);
	assert!(check.is_ok());

	let candidate_receipt = CandidateReceipt { descriptor, commitments_hash: Hash::zero() };

	let v = executor::block_on(validate_candidate_exhaustive(
		1,
		MockValidateCandidateBackend::with_hardcoded_result(Err(ValidationError::Invalid(
			WasmInvalidCandidate::HardTimeout,
		))),
		validation_data,
		validation_code,
		candidate_receipt,
		Arc::new(pov),
		ExecutorParams::default(),
		PvfExecKind::Backing(dummy_hash()),
		&Default::default(),
		Default::default(),
		VALIDATION_CODE_BOMB_LIMIT,
	));

	assert_matches!(v, Ok(ValidationResult::Invalid(InvalidCandidate::Timeout)));
}

#[test]
fn candidate_validation_commitment_hash_mismatch_is_invalid() {
	let validation_data = PersistedValidationData { max_pov_size: 1024, ..Default::default() };
	let pov = PoV { block_data: BlockData(vec![0xff; 32]) };
	let validation_code = ValidationCode(vec![0xff; 16]);
	let head_data = HeadData(vec![1, 1, 1]);

	let candidate_receipt = CandidateReceipt {
		descriptor: make_valid_candidate_descriptor(
			ParaId::from(1_u32),
			validation_data.parent_head.hash(),
			validation_data.hash(),
			pov.hash(),
			validation_code.hash(),
			head_data.hash(),
			dummy_hash(),
			Sr25519Keyring::Alice,
		)
		.into(),
		commitments_hash: Hash::zero(),
	}
	.into();

	// This will result in different commitments for this candidate.
	let validation_result = WasmValidationResult {
		head_data,
		new_validation_code: None,
		upward_messages: Default::default(),
		horizontal_messages: Default::default(),
		processed_downward_messages: 0,
		hrmp_watermark: 12345,
	};

	let result = executor::block_on(validate_candidate_exhaustive(
		1,
		MockValidateCandidateBackend::with_hardcoded_result(Ok(validation_result)),
		validation_data,
		validation_code,
		candidate_receipt,
		Arc::new(pov),
		ExecutorParams::default(),
		PvfExecKind::Backing(dummy_hash()),
		&Default::default(),
		Default::default(),
		VALIDATION_CODE_BOMB_LIMIT,
	))
	.unwrap();

	// Ensure `post validation` check on the commitments hash works as expected.
	assert_matches!(result, ValidationResult::Invalid(InvalidCandidate::CommitmentsHashMismatch));
}

#[test]
fn candidate_validation_code_mismatch_is_invalid() {
	let validation_data = PersistedValidationData { max_pov_size: 1024, ..Default::default() };

	let pov = PoV { block_data: BlockData(vec![1; 32]) };
	let validation_code = ValidationCode(vec![2; 16]);

	let descriptor = make_valid_candidate_descriptor(
		ParaId::from(1_u32),
		dummy_hash(),
		validation_data.hash(),
		pov.hash(),
		ValidationCode(vec![1; 16]).hash(),
		dummy_hash(),
		dummy_hash(),
		Sr25519Keyring::Alice,
	)
	.into();

	let check = perform_basic_checks(
		&descriptor,
		validation_data.max_pov_size,
		&pov,
		&validation_code.hash(),
	);
	assert_matches!(check, Err(InvalidCandidate::CodeHashMismatch));

	let candidate_receipt = CandidateReceipt { descriptor, commitments_hash: Hash::zero() };

	let pool = TaskExecutor::new();
	let (_ctx, _ctx_handle) = make_subsystem_context::<AllMessages, _>(pool.clone());

	let v = executor::block_on(validate_candidate_exhaustive(
		1,
		MockValidateCandidateBackend::with_hardcoded_result(Err(ValidationError::Invalid(
			WasmInvalidCandidate::HardTimeout,
		))),
		validation_data,
		validation_code,
		candidate_receipt,
		Arc::new(pov),
		ExecutorParams::default(),
		PvfExecKind::Backing(dummy_hash()),
		&Default::default(),
		Default::default(),
		VALIDATION_CODE_BOMB_LIMIT,
	))
	.unwrap();

	assert_matches!(v, ValidationResult::Invalid(InvalidCandidate::CodeHashMismatch));
}

#[test]
fn compressed_code_works() {
	let validation_data = PersistedValidationData { max_pov_size: 1024, ..Default::default() };
	let pov = PoV { block_data: BlockData(vec![1; 32]) };
	let head_data = HeadData(vec![1, 1, 1]);

	let raw_code = vec![2u8; 16];
	let validation_code =
		sp_maybe_compressed_blob::compress(&raw_code, VALIDATION_CODE_BOMB_LIMIT as usize)
			.map(ValidationCode)
			.unwrap();

	let descriptor = make_valid_candidate_descriptor(
		ParaId::from(1_u32),
		dummy_hash(),
		validation_data.hash(),
		pov.hash(),
		validation_code.hash(),
		head_data.hash(),
		dummy_hash(),
		Sr25519Keyring::Alice,
	)
	.into();

	let validation_result = WasmValidationResult {
		head_data,
		new_validation_code: None,
		upward_messages: Default::default(),
		horizontal_messages: Default::default(),
		processed_downward_messages: 0,
		hrmp_watermark: 0,
	};

	let commitments = CandidateCommitments {
		head_data: validation_result.head_data.clone(),
		upward_messages: validation_result.upward_messages.clone(),
		horizontal_messages: validation_result.horizontal_messages.clone(),
		new_validation_code: validation_result.new_validation_code.clone(),
		processed_downward_messages: validation_result.processed_downward_messages,
		hrmp_watermark: validation_result.hrmp_watermark,
	};

	let candidate_receipt = CandidateReceipt { descriptor, commitments_hash: commitments.hash() };

	let v = executor::block_on(validate_candidate_exhaustive(
		1,
		MockValidateCandidateBackend::with_hardcoded_result(Ok(validation_result)),
		validation_data,
		validation_code,
		candidate_receipt,
		Arc::new(pov),
		ExecutorParams::default(),
		PvfExecKind::Backing(dummy_hash()),
		&Default::default(),
		Some(Default::default()),
		VALIDATION_CODE_BOMB_LIMIT,
	));

	assert_matches!(v, Ok(ValidationResult::Valid(_, _)));
}

struct MockPreCheckBackend {
	result: Result<(), PrepareError>,
}

impl MockPreCheckBackend {
	fn with_hardcoded_result(result: Result<(), PrepareError>) -> Self {
		Self { result }
	}
}

#[async_trait]
impl ValidationBackend for MockPreCheckBackend {
	async fn validate_candidate(
		&mut self,
		_pvf: PvfPrepData,
		_timeout: Duration,
		_pvd: Arc<PersistedValidationData>,
		_pov: Arc<PoV>,
		_prepare_priority: polkadot_node_core_pvf::Priority,
		_exec_kind: PvfExecKind,
	) -> Result<WasmValidationResult, ValidationError> {
		unreachable!()
	}

	async fn precheck_pvf(&mut self, _pvf: PvfPrepData) -> Result<(), PrepareError> {
		self.result.clone()
	}

	async fn heads_up(&mut self, _active_pvfs: Vec<PvfPrepData>) -> Result<(), String> {
		unreachable!()
	}

	async fn update_active_leaves(
		&mut self,
		_update: ActiveLeavesUpdate,
		_ancestors: Vec<Hash>,
	) -> Result<(), String> {
		unreachable!()
	}
}

#[test]
fn precheck_works() {
	let relay_parent = [3; 32].into();
	let validation_code = ValidationCode(vec![3; 16]);
	let validation_code_hash = validation_code.hash();

	let pool = TaskExecutor::new();
	let (mut ctx, mut ctx_handle) = make_subsystem_context::<AllMessages, _>(pool.clone());

	let (check_fut, check_result) = precheck_pvf(
		ctx.sender(),
		MockPreCheckBackend::with_hardcoded_result(Ok(())),
		relay_parent,
		validation_code_hash,
		VALIDATION_CODE_BOMB_LIMIT,
	)
	.remote_handle();

	let test_fut = async move {
		assert_matches!(
			ctx_handle.recv().await,
			AllMessages::RuntimeApi(RuntimeApiMessage::Request(
				rp,
				RuntimeApiRequest::ValidationCodeByHash(
					vch,
					tx
				),
			)) => {
				assert_eq!(vch, validation_code_hash);
				assert_eq!(rp, relay_parent);

				let _ = tx.send(Ok(Some(validation_code.clone())));
			}
		);
		assert_matches!(
			ctx_handle.recv().await,
			AllMessages::RuntimeApi(
				RuntimeApiMessage::Request(_, RuntimeApiRequest::SessionIndexForChild(tx))
			) => {
				tx.send(Ok(1u32.into())).unwrap();
			}
		);
		assert_matches!(
			ctx_handle.recv().await,
			AllMessages::RuntimeApi(
				RuntimeApiMessage::Request(_, RuntimeApiRequest::SessionExecutorParams(_, tx))
			) => {
				tx.send(Ok(Some(ExecutorParams::default()))).unwrap();
			}
		);
		assert_matches!(check_result.await, PreCheckOutcome::Valid);
	};

	let test_fut = future::join(test_fut, check_fut);
	executor::block_on(test_fut);
}

#[test]
fn precheck_properly_classifies_outcomes() {
	let inner = |prepare_result, precheck_outcome| {
		let relay_parent = [3; 32].into();
		let validation_code = ValidationCode(vec![3; 16]);
		let validation_code_hash = validation_code.hash();

		let pool = TaskExecutor::new();
		let (mut ctx, mut ctx_handle) = make_subsystem_context::<AllMessages, _>(pool.clone());

		let (check_fut, check_result) = precheck_pvf(
			ctx.sender(),
			MockPreCheckBackend::with_hardcoded_result(prepare_result),
			relay_parent,
			validation_code_hash,
			VALIDATION_CODE_BOMB_LIMIT,
		)
		.remote_handle();

		let test_fut = async move {
			assert_matches!(
				ctx_handle.recv().await,
				AllMessages::RuntimeApi(RuntimeApiMessage::Request(
					rp,
					RuntimeApiRequest::ValidationCodeByHash(
						vch,
						tx
					),
				)) => {
					assert_eq!(vch, validation_code_hash);
					assert_eq!(rp, relay_parent);

					let _ = tx.send(Ok(Some(validation_code.clone())));
				}
			);
			assert_matches!(
				ctx_handle.recv().await,
				AllMessages::RuntimeApi(
					RuntimeApiMessage::Request(_, RuntimeApiRequest::SessionIndexForChild(tx))
				) => {
					tx.send(Ok(1u32.into())).unwrap();
				}
			);
			assert_matches!(
				ctx_handle.recv().await,
				AllMessages::RuntimeApi(
					RuntimeApiMessage::Request(_, RuntimeApiRequest::SessionExecutorParams(_, tx))
				) => {
					tx.send(Ok(Some(ExecutorParams::default()))).unwrap();
				}
			);
			assert_eq!(check_result.await, precheck_outcome);
		};

		let test_fut = future::join(test_fut, check_fut);
		executor::block_on(test_fut);
	};

	inner(Err(PrepareError::Prevalidation("foo".to_owned())), PreCheckOutcome::Invalid);
	inner(Err(PrepareError::Preparation("bar".to_owned())), PreCheckOutcome::Invalid);
	inner(Err(PrepareError::JobError("baz".to_owned())), PreCheckOutcome::Invalid);

	inner(Err(PrepareError::TimedOut), PreCheckOutcome::Failed);
	inner(Err(PrepareError::IoErr("fizz".to_owned())), PreCheckOutcome::Failed);
}

#[derive(Default, Clone)]
struct MockHeadsUp {
	heads_up_call_count: Arc<AtomicUsize>,
}

#[async_trait]
impl ValidationBackend for MockHeadsUp {
	async fn validate_candidate(
		&mut self,
		_pvf: PvfPrepData,
		_timeout: Duration,
		_pvd: Arc<PersistedValidationData>,
		_pov: Arc<PoV>,
		_prepare_priority: polkadot_node_core_pvf::Priority,
		_exec_kind: PvfExecKind,
	) -> Result<WasmValidationResult, ValidationError> {
		unreachable!()
	}

	async fn precheck_pvf(&mut self, _pvf: PvfPrepData) -> Result<(), PrepareError> {
		unreachable!()
	}

	async fn heads_up(&mut self, _active_pvfs: Vec<PvfPrepData>) -> Result<(), String> {
		let _ = self.heads_up_call_count.fetch_add(1, Ordering::SeqCst);
		Ok(())
	}

	async fn update_active_leaves(
		&mut self,
		_update: ActiveLeavesUpdate,
		_ancestors: Vec<Hash>,
	) -> Result<(), String> {
		Ok(())
	}
}

fn alice_keystore() -> KeystorePtr {
	let keystore: KeystorePtr = Arc::new(MemoryKeystore::new());
	let _ = Keystore::sr25519_generate_new(
		&*keystore,
		ValidatorId::ID,
		Some(&Sr25519Keyring::Alice.to_seed()),
	)
	.unwrap();
	let _ = Keystore::sr25519_generate_new(
		&*keystore,
		AuthorityDiscoveryId::ID,
		Some(&Sr25519Keyring::Alice.to_seed()),
	)
	.unwrap();

	keystore
}

fn dummy_active_leaves_update(hash: Hash) -> ActiveLeavesUpdate {
	ActiveLeavesUpdate {
		activated: Some(ActivatedLeaf {
			hash,
			number: 10,
			unpin_handle: polkadot_node_subsystem_test_helpers::mock::dummy_unpin_handle(hash),
		}),
		..Default::default()
	}
}

fn dummy_candidate_backed(
	relay_parent: Hash,
	validation_code_hash: ValidationCodeHash,
) -> CandidateEvent {
	let zeros = dummy_hash();
	let descriptor = CandidateDescriptor {
		para_id: ParaId::from(0_u32),
		relay_parent,
		collator: dummy_collator(),
		persisted_validation_data_hash: zeros,
		pov_hash: zeros,
		erasure_root: zeros,
		signature: dummy_collator_signature(),
		para_head: zeros,
		validation_code_hash,
	}
	.into();

	CandidateEvent::CandidateBacked(
		CandidateReceipt { descriptor, commitments_hash: zeros },
		HeadData(Vec::new()),
		CoreIndex(0),
		GroupIndex(0),
	)
}

fn dummy_session_info(keys: Vec<Public>) -> SessionInfo {
	SessionInfo {
		validators: keys.iter().cloned().map(Into::into).collect(),
		discovery_keys: keys.iter().cloned().map(Into::into).collect(),
		assignment_keys: vec![],
		validator_groups: Default::default(),
		n_cores: 4u32,
		zeroth_delay_tranche_width: 0u32,
		relay_vrf_modulo_samples: 0u32,
		n_delay_tranches: 2u32,
		no_show_slots: 0u32,
		needed_approvals: 1u32,
		active_validator_indices: vec![],
		dispute_period: 6,
		random_seed: [0u8; 32],
	}
}

async fn assert_new_active_leaf_messages(
	recv_handle: &mut TestSubsystemContextHandle<AllMessages>,
	expected_session_index: SessionIndex,
) {
	assert_matches!(
		recv_handle.recv().await,
		AllMessages::RuntimeApi(RuntimeApiMessage::Request(_, RuntimeApiRequest::SessionIndexForChild(tx))) => {
			let _ = tx.send(Ok(expected_session_index));
		}
	);

	let lookahead_value = DEFAULT_SCHEDULING_LOOKAHEAD;
	assert_matches!(
		recv_handle.recv().await,
		AllMessages::RuntimeApi(RuntimeApiMessage::Request(_, RuntimeApiRequest::SchedulingLookahead(index, tx))) => {
			assert_eq!(index, expected_session_index);
			let _ = tx.send(Ok(lookahead_value));
		}
	);

	assert_matches!(
		recv_handle.recv().await,
		AllMessages::ChainApi(ChainApiMessage::Ancestors {k, response_channel, ..}) => {
			assert_eq!(k as u32, lookahead_value - 1);
			let _ = response_channel.send(Ok((0..(lookahead_value - 1)).into_iter().map(|i| Hash::from_low_u64_be(i as u64)).collect()));
		}
	);
}

#[test]
fn maybe_prepare_validation_golden_path() {
	let pool = TaskExecutor::new();
	let (mut ctx, mut ctx_handle) = make_subsystem_context::<AllMessages, _>(pool);

	let keystore = alice_keystore();
	let mut backend = MockHeadsUp::default();
	let activated_hash = Hash::random();
	let update = dummy_active_leaves_update(activated_hash);
	let mut state = PrepareValidationState::default();

	let check_fut =
		handle_active_leaves_update(ctx.sender(), keystore, &mut backend, update, &mut state);

	let test_fut = async move {
		assert_new_active_leaf_messages(&mut ctx_handle, 1).await;

		assert_matches!(
			ctx_handle.recv().await,
			AllMessages::RuntimeApi(RuntimeApiMessage::Request(_, RuntimeApiRequest::Authorities(tx))) => {
				let _ = tx.send(Ok(vec![Sr25519Keyring::Alice.public().into()]));
			}
		);

		assert_matches!(
			ctx_handle.recv().await,
			AllMessages::RuntimeApi(RuntimeApiMessage::Request(_, RuntimeApiRequest::SessionInfo(index, tx))) => {
				assert_eq!(index, 1);
				let _ = tx.send(Ok(Some(dummy_session_info(vec![Sr25519Keyring::Bob.public()]))));
			}
		);

		assert_matches!(
			ctx_handle.recv().await,
			AllMessages::RuntimeApi(RuntimeApiMessage::Request(_, RuntimeApiRequest::CandidateEvents(tx))) => {
				let _ = tx.send(Ok(vec![dummy_candidate_backed(activated_hash, dummy_hash().into())]));
			}
		);

		assert_matches!(
			ctx_handle.recv().await,
			AllMessages::RuntimeApi(RuntimeApiMessage::Request(_, RuntimeApiRequest::SessionIndexForChild(tx))) => {
				let _ = tx.send(Ok(1));
			}
		);

		assert_matches!(
			ctx_handle.recv().await,
			AllMessages::RuntimeApi(RuntimeApiMessage::Request(_, RuntimeApiRequest::SessionExecutorParams(index, tx))) => {
				assert_eq!(index, 1);
				let _ = tx.send(Ok(Some(ExecutorParams::default())));
			}
		);

		assert_matches!(
			ctx_handle.recv().await,
			AllMessages::RuntimeApi(RuntimeApiMessage::Request(_, RuntimeApiRequest::ValidationCodeByHash(hash, tx))) => {
				assert_eq!(hash, dummy_hash().into());
				let _ = tx.send(Ok(Some(ValidationCode(Vec::new()))));
			}
		);

		assert_matches!(
			ctx_handle.recv().await,
			AllMessages::RuntimeApi(RuntimeApiMessage::Request(_, RuntimeApiRequest::SessionIndexForChild(tx))) => {
				let _ = tx.send(Ok(1));
			}
		);

		assert_matches!(
			ctx_handle.recv().await,
			AllMessages::RuntimeApi(RuntimeApiMessage::Request(_, RuntimeApiRequest::ValidationCodeBombLimit(session, tx))) => {
				assert_eq!(session, 1);
				let _ = tx.send(Ok(VALIDATION_CODE_BOMB_LIMIT));
			}
		);
	};

	let test_fut = future::join(test_fut, check_fut);
	executor::block_on(test_fut);

	assert_eq!(backend.heads_up_call_count.load(Ordering::SeqCst), 1);
	assert!(state.session_index.is_some());
	assert!(state.is_next_session_authority);
}

#[test]
fn maybe_prepare_validation_checkes_authority_once_per_session() {
	let pool = TaskExecutor::new();
	let (mut ctx, mut ctx_handle) = make_subsystem_context::<AllMessages, _>(pool);

	let keystore = alice_keystore();
	let mut backend = MockHeadsUp::default();
	let activated_hash = Hash::random();
	let update = dummy_active_leaves_update(activated_hash);
	let mut state = PrepareValidationState {
		session_index: Some(1),
		is_next_session_authority: false,
		..Default::default()
	};

	let check_fut =
		handle_active_leaves_update(ctx.sender(), keystore, &mut backend, update, &mut state);

	let test_fut = assert_new_active_leaf_messages(&mut ctx_handle, 1);

	let test_fut = future::join(test_fut, check_fut);
	executor::block_on(test_fut);

	assert_eq!(backend.heads_up_call_count.load(Ordering::SeqCst), 0);
	assert!(state.session_index.is_some());
	assert!(!state.is_next_session_authority);
}

#[test]
fn maybe_prepare_validation_resets_state_on_a_new_session() {
	let pool = TaskExecutor::new();
	let (mut ctx, mut ctx_handle) = make_subsystem_context::<AllMessages, _>(pool);

	let keystore = alice_keystore();
	let mut backend = MockHeadsUp::default();
	let activated_hash = Hash::random();
	let update = dummy_active_leaves_update(activated_hash);
	let mut state = PrepareValidationState {
		session_index: Some(1),
		is_next_session_authority: true,
		already_prepared_code_hashes: HashSet::from_iter(vec![ValidationCode(vec![0; 16]).hash()]),
		..Default::default()
	};

	let check_fut =
		handle_active_leaves_update(ctx.sender(), keystore, &mut backend, update, &mut state);

	let test_fut = async move {
		assert_new_active_leaf_messages(&mut ctx_handle, 2).await;

		assert_matches!(
			ctx_handle.recv().await,
			AllMessages::RuntimeApi(RuntimeApiMessage::Request(_, RuntimeApiRequest::Authorities(tx))) => {
				let _ = tx.send(Ok(vec![Sr25519Keyring::Bob.public().into()]));
			}
		);

		assert_matches!(
			ctx_handle.recv().await,
			AllMessages::RuntimeApi(RuntimeApiMessage::Request(_, RuntimeApiRequest::SessionInfo(index, tx))) => {
				assert_eq!(index, 2);
				let _ = tx.send(Ok(Some(dummy_session_info(vec![Sr25519Keyring::Bob.public()]))));
			}
		);
	};

	let test_fut = future::join(test_fut, check_fut);
	executor::block_on(test_fut);

	assert_eq!(backend.heads_up_call_count.load(Ordering::SeqCst), 0);
	assert_eq!(state.session_index.unwrap(), 2);
	assert!(!state.is_next_session_authority);
	assert!(state.already_prepared_code_hashes.is_empty());
}

#[test]
fn maybe_prepare_validation_does_not_prepare_pvfs_if_no_new_session_and_not_a_validator() {
	let pool = TaskExecutor::new();
	let (mut ctx, mut ctx_handle) = make_subsystem_context::<AllMessages, _>(pool);

	let keystore = alice_keystore();
	let mut backend = MockHeadsUp::default();
	let activated_hash = Hash::random();
	let update = dummy_active_leaves_update(activated_hash);
	let mut state = PrepareValidationState { session_index: Some(1), ..Default::default() };

	let check_fut =
		handle_active_leaves_update(ctx.sender(), keystore, &mut backend, update, &mut state);

	let test_fut = assert_new_active_leaf_messages(&mut ctx_handle, 1);

	let test_fut = future::join(test_fut, check_fut);
	executor::block_on(test_fut);

	assert_eq!(backend.heads_up_call_count.load(Ordering::SeqCst), 0);
	assert!(state.session_index.is_some());
	assert!(!state.is_next_session_authority);
}

#[test]
fn maybe_prepare_validation_does_not_prepare_pvfs_if_no_new_session_but_a_validator() {
	let pool = TaskExecutor::new();
	let (mut ctx, mut ctx_handle) = make_subsystem_context::<AllMessages, _>(pool);

	let keystore = alice_keystore();
	let mut backend = MockHeadsUp::default();
	let activated_hash = Hash::random();
	let update = dummy_active_leaves_update(activated_hash);
	let mut state = PrepareValidationState {
		session_index: Some(1),
		is_next_session_authority: true,
		..Default::default()
	};

	let check_fut =
		handle_active_leaves_update(ctx.sender(), keystore, &mut backend, update, &mut state);

	let test_fut = async move {
		assert_new_active_leaf_messages(&mut ctx_handle, 1).await;

		assert_matches!(
			ctx_handle.recv().await,
			AllMessages::RuntimeApi(RuntimeApiMessage::Request(_, RuntimeApiRequest::CandidateEvents(tx))) => {
				let _ = tx.send(Ok(vec![dummy_candidate_backed(activated_hash, dummy_hash().into())]));
			}
		);

		assert_matches!(
			ctx_handle.recv().await,
			AllMessages::RuntimeApi(RuntimeApiMessage::Request(_, RuntimeApiRequest::SessionIndexForChild(tx))) => {
				let _ = tx.send(Ok(1));
			}
		);

		assert_matches!(
			ctx_handle.recv().await,
			AllMessages::RuntimeApi(RuntimeApiMessage::Request(_, RuntimeApiRequest::SessionExecutorParams(index, tx))) => {
				assert_eq!(index, 1);
				let _ = tx.send(Ok(Some(ExecutorParams::default())));
			}
		);

		assert_matches!(
			ctx_handle.recv().await,
			AllMessages::RuntimeApi(RuntimeApiMessage::Request(_, RuntimeApiRequest::ValidationCodeByHash(hash, tx))) => {
				assert_eq!(hash, dummy_hash().into());
				let _ = tx.send(Ok(Some(ValidationCode(Vec::new()))));
			}
		);

		assert_matches!(
			ctx_handle.recv().await,
			AllMessages::RuntimeApi(RuntimeApiMessage::Request(_, RuntimeApiRequest::SessionIndexForChild(tx))) => {
				let _ = tx.send(Ok(1));
			}
		);

		assert_matches!(
			ctx_handle.recv().await,
			AllMessages::RuntimeApi(RuntimeApiMessage::Request(_, RuntimeApiRequest::ValidationCodeBombLimit(session, tx))) => {
				assert_eq!(session, 1);
				let _ = tx.send(Ok(VALIDATION_CODE_BOMB_LIMIT));
			}
		);
	};

	let test_fut = future::join(test_fut, check_fut);
	executor::block_on(test_fut);

	assert_eq!(backend.heads_up_call_count.load(Ordering::SeqCst), 1);
	assert!(state.session_index.is_some());
	assert!(state.is_next_session_authority);
}

#[test]
fn maybe_prepare_validation_does_not_prepare_pvfs_if_not_a_validator_in_the_next_session() {
	let pool = TaskExecutor::new();
	let (mut ctx, mut ctx_handle) = make_subsystem_context::<AllMessages, _>(pool);

	let keystore = alice_keystore();
	let mut backend = MockHeadsUp::default();
	let activated_hash = Hash::random();
	let update = dummy_active_leaves_update(activated_hash);
	let mut state = PrepareValidationState::default();

	let check_fut =
		handle_active_leaves_update(ctx.sender(), keystore, &mut backend, update, &mut state);

	let test_fut = async move {
		assert_new_active_leaf_messages(&mut ctx_handle, 1).await;

		assert_matches!(
			ctx_handle.recv().await,
			AllMessages::RuntimeApi(RuntimeApiMessage::Request(_, RuntimeApiRequest::Authorities(tx))) => {
				let _ = tx.send(Ok(vec![Sr25519Keyring::Bob.public().into()]));
			}
		);

		assert_matches!(
			ctx_handle.recv().await,
			AllMessages::RuntimeApi(RuntimeApiMessage::Request(_, RuntimeApiRequest::SessionInfo(index, tx))) => {
				assert_eq!(index, 1);
				let _ = tx.send(Ok(Some(dummy_session_info(vec![Sr25519Keyring::Bob.public()]))));
			}
		);
	};

	let test_fut = future::join(test_fut, check_fut);
	executor::block_on(test_fut);

	assert_eq!(backend.heads_up_call_count.load(Ordering::SeqCst), 0);
	assert!(state.session_index.is_some());
	assert!(!state.is_next_session_authority);
}

#[test]
fn maybe_prepare_validation_does_not_prepare_pvfs_if_a_validator_in_the_current_session() {
	let pool = TaskExecutor::new();
	let (mut ctx, mut ctx_handle) = make_subsystem_context::<AllMessages, _>(pool);

	let keystore = alice_keystore();
	let mut backend = MockHeadsUp::default();
	let activated_hash = Hash::random();
	let update = dummy_active_leaves_update(activated_hash);
	let mut state = PrepareValidationState::default();

	let check_fut =
		handle_active_leaves_update(ctx.sender(), keystore, &mut backend, update, &mut state);

	let test_fut = async move {
		assert_new_active_leaf_messages(&mut ctx_handle, 1).await;

		assert_matches!(
			ctx_handle.recv().await,
			AllMessages::RuntimeApi(RuntimeApiMessage::Request(_, RuntimeApiRequest::Authorities(tx))) => {
				let _ = tx.send(Ok(vec![Sr25519Keyring::Alice.public().into()]));
			}
		);

		assert_matches!(
			ctx_handle.recv().await,
			AllMessages::RuntimeApi(RuntimeApiMessage::Request(_, RuntimeApiRequest::SessionInfo(index, tx))) => {
				assert_eq!(index, 1);
				let _ = tx.send(Ok(Some(dummy_session_info(vec![Sr25519Keyring::Alice.public()]))));
			}
		);
	};

	let test_fut = future::join(test_fut, check_fut);
	executor::block_on(test_fut);

	assert_eq!(backend.heads_up_call_count.load(Ordering::SeqCst), 0);
	assert!(state.session_index.is_some());
	assert!(!state.is_next_session_authority);
}

#[test]
fn maybe_prepare_validation_prepares_a_limited_number_of_pvfs() {
	let pool = TaskExecutor::new();
	let (mut ctx, mut ctx_handle) = make_subsystem_context::<AllMessages, _>(pool);

	let keystore = alice_keystore();
	let mut backend = MockHeadsUp::default();
	let activated_hash = Hash::random();
	let update = dummy_active_leaves_update(activated_hash);
	let mut state = PrepareValidationState { per_block_limit: 2, ..Default::default() };

	let check_fut =
		handle_active_leaves_update(ctx.sender(), keystore, &mut backend, update, &mut state);

	let test_fut = async move {
		assert_new_active_leaf_messages(&mut ctx_handle, 1).await;

		assert_matches!(
			ctx_handle.recv().await,
			AllMessages::RuntimeApi(RuntimeApiMessage::Request(_, RuntimeApiRequest::Authorities(tx))) => {
				let _ = tx.send(Ok(vec![Sr25519Keyring::Alice.public().into()]));
			}
		);

		assert_matches!(
			ctx_handle.recv().await,
			AllMessages::RuntimeApi(RuntimeApiMessage::Request(_, RuntimeApiRequest::SessionInfo(index, tx))) => {
				assert_eq!(index, 1);
				let _ = tx.send(Ok(Some(dummy_session_info(vec![Sr25519Keyring::Bob.public()]))));
			}
		);

		assert_matches!(
			ctx_handle.recv().await,
			AllMessages::RuntimeApi(RuntimeApiMessage::Request(_, RuntimeApiRequest::CandidateEvents(tx))) => {
				let candidates = vec![
					dummy_candidate_backed(activated_hash, ValidationCode(vec![0; 16]).hash()),
					dummy_candidate_backed(activated_hash, ValidationCode(vec![1; 16]).hash()),
					dummy_candidate_backed(activated_hash, ValidationCode(vec![2; 16]).hash()),
				];
				let _ = tx.send(Ok(candidates));
			}
		);

		assert_matches!(
			ctx_handle.recv().await,
			AllMessages::RuntimeApi(RuntimeApiMessage::Request(_, RuntimeApiRequest::SessionIndexForChild(tx))) => {
				let _ = tx.send(Ok(1));
			}
		);

		assert_matches!(
			ctx_handle.recv().await,
			AllMessages::RuntimeApi(RuntimeApiMessage::Request(_, RuntimeApiRequest::SessionExecutorParams(index, tx))) => {
				assert_eq!(index, 1);
				let _ = tx.send(Ok(Some(ExecutorParams::default())));
			}
		);

		for c in 0..2 {
			assert_matches!(
				ctx_handle.recv().await,
				AllMessages::RuntimeApi(RuntimeApiMessage::Request(_, RuntimeApiRequest::ValidationCodeByHash(hash, tx))) => {
					assert_eq!(hash, ValidationCode(vec![c; 16]).hash());
					let _ = tx.send(Ok(Some(ValidationCode(Vec::new()))));
				}
			);

			assert_matches!(
				ctx_handle.recv().await,
				AllMessages::RuntimeApi(RuntimeApiMessage::Request(_, RuntimeApiRequest::SessionIndexForChild(tx))) => {
					let _ = tx.send(Ok(1));
				}
			);

			assert_matches!(
				ctx_handle.recv().await,
				AllMessages::RuntimeApi(RuntimeApiMessage::Request(_, RuntimeApiRequest::ValidationCodeBombLimit(session, tx))) => {
					assert_eq!(session, 1);
					let _ = tx.send(Ok(VALIDATION_CODE_BOMB_LIMIT));
				}
			);
		}
	};

	let test_fut = future::join(test_fut, check_fut);
	executor::block_on(test_fut);

	assert_eq!(backend.heads_up_call_count.load(Ordering::SeqCst), 1);
	assert!(state.session_index.is_some());
	assert!(state.is_next_session_authority);
	assert_eq!(state.already_prepared_code_hashes.len(), 2);
}

#[test]
fn maybe_prepare_validation_does_not_prepare_already_prepared_pvfs() {
	let pool = TaskExecutor::new();
	let (mut ctx, mut ctx_handle) = make_subsystem_context::<AllMessages, _>(pool);

	let keystore = alice_keystore();
	let mut backend = MockHeadsUp::default();
	let activated_hash = Hash::random();
	let update = dummy_active_leaves_update(activated_hash);
	let mut state = PrepareValidationState {
		session_index: Some(1),
		is_next_session_authority: true,
		per_block_limit: 2,
		already_prepared_code_hashes: HashSet::from_iter(vec![
			ValidationCode(vec![0; 16]).hash(),
			ValidationCode(vec![1; 16]).hash(),
		]),
	};

	let check_fut =
		handle_active_leaves_update(ctx.sender(), keystore, &mut backend, update, &mut state);

	let test_fut = async move {
		assert_new_active_leaf_messages(&mut ctx_handle, 1).await;

		assert_matches!(
			ctx_handle.recv().await,
			AllMessages::RuntimeApi(RuntimeApiMessage::Request(_, RuntimeApiRequest::CandidateEvents(tx))) => {
				let candidates = vec![
					dummy_candidate_backed(activated_hash, ValidationCode(vec![0; 16]).hash()),
					dummy_candidate_backed(activated_hash, ValidationCode(vec![1; 16]).hash()),
					dummy_candidate_backed(activated_hash, ValidationCode(vec![2; 16]).hash()),
				];
				let _ = tx.send(Ok(candidates));
			}
		);

		assert_matches!(
			ctx_handle.recv().await,
			AllMessages::RuntimeApi(RuntimeApiMessage::Request(_, RuntimeApiRequest::SessionIndexForChild(tx))) => {
				let _ = tx.send(Ok(1));
			}
		);

		assert_matches!(
			ctx_handle.recv().await,
			AllMessages::RuntimeApi(RuntimeApiMessage::Request(_, RuntimeApiRequest::SessionExecutorParams(index, tx))) => {
				assert_eq!(index, 1);
				let _ = tx.send(Ok(Some(ExecutorParams::default())));
			}
		);

		assert_matches!(
			ctx_handle.recv().await,
			AllMessages::RuntimeApi(RuntimeApiMessage::Request(_, RuntimeApiRequest::ValidationCodeByHash(hash, tx))) => {
				assert_eq!(hash, ValidationCode(vec![2; 16]).hash());
				let _ = tx.send(Ok(Some(ValidationCode(Vec::new()))));
			}
		);

		assert_matches!(
			ctx_handle.recv().await,
			AllMessages::RuntimeApi(RuntimeApiMessage::Request(_, RuntimeApiRequest::SessionIndexForChild(tx))) => {
				let _ = tx.send(Ok(1));
			}
		);

		assert_matches!(
			ctx_handle.recv().await,
			AllMessages::RuntimeApi(RuntimeApiMessage::Request(_, RuntimeApiRequest::ValidationCodeBombLimit(session, tx))) => {
				assert_eq!(session, 1);
				let _ = tx.send(Ok(VALIDATION_CODE_BOMB_LIMIT));
			}
		);
	};

	let test_fut = future::join(test_fut, check_fut);
	executor::block_on(test_fut);

	assert_eq!(backend.heads_up_call_count.load(Ordering::SeqCst), 1);
	assert!(state.session_index.is_some());
	assert!(state.is_next_session_authority);
	assert_eq!(state.already_prepared_code_hashes.len(), 3);
}
