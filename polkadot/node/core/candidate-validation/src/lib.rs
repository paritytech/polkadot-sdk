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

//! The Candidate Validation subsystem.
//!
//! This handles incoming requests from other subsystems to validate candidates
//! according to a validation function. This delegates validation to an underlying
//! pool of processes used for execution of the Wasm.

#![deny(unused_crate_dependencies, unused_results)]
#![warn(missing_docs)]

use polkadot_node_core_pvf::{
	InternalValidationError, InvalidCandidate as WasmInvalidCandidate, PossiblyInvalidError,
	PrepareError, PrepareJobKind, PvfPrepData, ValidationError, ValidationHost,
};
use polkadot_node_primitives::{InvalidCandidate, PoV, ValidationResult};
use polkadot_node_subsystem::{
	errors::RuntimeApiError,
	messages::{
		CandidateValidationMessage, PreCheckOutcome, PvfExecKind, RuntimeApiMessage,
		RuntimeApiRequest, ValidationFailed,
	},
	overseer, FromOrchestra, OverseerSignal, SpawnedSubsystem, SubsystemError, SubsystemResult,
	SubsystemSender,
};
use polkadot_node_subsystem_util as util;
use polkadot_overseer::ActiveLeavesUpdate;
use polkadot_parachain_primitives::primitives::ValidationResult as WasmValidationResult;
use polkadot_primitives::{
	executor_params::{
		DEFAULT_APPROVAL_EXECUTION_TIMEOUT, DEFAULT_BACKING_EXECUTION_TIMEOUT,
		DEFAULT_LENIENT_PREPARATION_TIMEOUT, DEFAULT_PRECHECK_PREPARATION_TIMEOUT,
	},
	AuthorityDiscoveryId, CandidateCommitments, CandidateDescriptor, CandidateEvent,
	CandidateReceipt, ExecutorParams, Hash, PersistedValidationData,
	PvfExecKind as RuntimePvfExecKind, PvfPrepKind, SessionIndex, ValidationCode,
	ValidationCodeHash, ValidatorId,
};
use sp_application_crypto::{AppCrypto, ByteArray};
use sp_keystore::KeystorePtr;

use codec::Encode;

use futures::{channel::oneshot, prelude::*, stream::FuturesUnordered};

use std::{
	collections::HashSet,
	path::PathBuf,
	pin::Pin,
	sync::Arc,
	time::{Duration, Instant},
};

use async_trait::async_trait;

mod metrics;
use self::metrics::Metrics;

#[cfg(test)]
mod tests;

const LOG_TARGET: &'static str = "parachain::candidate-validation";

/// The amount of time to wait before retrying after a retry-able approval validation error. We use
/// a higher value for the approval case since we have more time, and if we wait longer it is more
/// likely that transient conditions will resolve.
#[cfg(not(test))]
const PVF_APPROVAL_EXECUTION_RETRY_DELAY: Duration = Duration::from_secs(3);
#[cfg(test)]
const PVF_APPROVAL_EXECUTION_RETRY_DELAY: Duration = Duration::from_millis(200);

// The task queue size is chosen to be somewhat bigger than the PVF host incoming queue size
// to allow exhaustive validation messages to fall through in case the tasks are clogged
const TASK_LIMIT: usize = 30;

/// Configuration for the candidate validation subsystem
#[derive(Clone, Default)]
pub struct Config {
	/// The path where candidate validation can store compiled artifacts for PVFs.
	pub artifacts_cache_path: PathBuf,
	/// The version of the node. `None` can be passed to skip the version check (only for tests).
	pub node_version: Option<String>,
	/// Whether the node is attempting to run as a secure validator.
	pub secure_validator_mode: bool,
	/// Path to the preparation worker binary
	pub prep_worker_path: PathBuf,
	/// Path to the execution worker binary
	pub exec_worker_path: PathBuf,
	/// The maximum number of pvf execution workers.
	pub pvf_execute_workers_max_num: usize,
	/// The maximum number of pvf workers that can be spawned in the pvf prepare pool for tasks
	/// with the priority below critical.
	pub pvf_prepare_workers_soft_max_num: usize,
	/// The absolute number of pvf workers that can be spawned in the pvf prepare pool.
	pub pvf_prepare_workers_hard_max_num: usize,
}

/// The candidate validation subsystem.
pub struct CandidateValidationSubsystem {
	keystore: KeystorePtr,
	#[allow(missing_docs)]
	pub metrics: Metrics,
	#[allow(missing_docs)]
	pub pvf_metrics: polkadot_node_core_pvf::Metrics,
	config: Option<Config>,
}

impl CandidateValidationSubsystem {
	/// Create a new `CandidateValidationSubsystem`.
	pub fn with_config(
		config: Option<Config>,
		keystore: KeystorePtr,
		metrics: Metrics,
		pvf_metrics: polkadot_node_core_pvf::Metrics,
	) -> Self {
		CandidateValidationSubsystem { keystore, config, metrics, pvf_metrics }
	}
}

#[overseer::subsystem(CandidateValidation, error=SubsystemError, prefix=self::overseer)]
impl<Context> CandidateValidationSubsystem {
	fn start(self, ctx: Context) -> SpawnedSubsystem {
		if let Some(config) = self.config {
			let future = run(ctx, self.keystore, self.metrics, self.pvf_metrics, config)
				.map_err(|e| SubsystemError::with_origin("candidate-validation", e))
				.boxed();
			SpawnedSubsystem { name: "candidate-validation-subsystem", future }
		} else {
			polkadot_overseer::DummySubsystem.start(ctx)
		}
	}
}

fn handle_validation_message<S>(
	mut sender: S,
	validation_host: ValidationHost,
	metrics: Metrics,
	msg: CandidateValidationMessage,
) -> Pin<Box<dyn Future<Output = ()> + Send>>
where
	S: SubsystemSender<RuntimeApiMessage>,
{
	match msg {
		CandidateValidationMessage::ValidateFromExhaustive {
			validation_data,
			validation_code,
			candidate_receipt,
			pov,
			executor_params,
			exec_kind,
			response_sender,
			..
		} => async move {
			let _timer = metrics.time_validate_from_exhaustive();
			let res = validate_candidate_exhaustive(
				validation_host,
				validation_data,
				validation_code,
				candidate_receipt,
				pov,
				executor_params,
				exec_kind,
				&metrics,
			)
			.await;

			metrics.on_validation_event(&res);
			let _ = response_sender.send(res);
		}
		.boxed(),
		CandidateValidationMessage::PreCheck {
			relay_parent,
			validation_code_hash,
			response_sender,
			..
		} => async move {
			let precheck_result =
				precheck_pvf(&mut sender, validation_host, relay_parent, validation_code_hash)
					.await;

			let _ = response_sender.send(precheck_result);
		}
		.boxed(),
	}
}

#[overseer::contextbounds(CandidateValidation, prefix = self::overseer)]
async fn run<Context>(
	mut ctx: Context,
	keystore: KeystorePtr,
	metrics: Metrics,
	pvf_metrics: polkadot_node_core_pvf::Metrics,
	Config {
		artifacts_cache_path,
		node_version,
		secure_validator_mode,
		prep_worker_path,
		exec_worker_path,
		pvf_execute_workers_max_num,
		pvf_prepare_workers_soft_max_num,
		pvf_prepare_workers_hard_max_num,
	}: Config,
) -> SubsystemResult<()> {
	let (validation_host, task) = polkadot_node_core_pvf::start(
		polkadot_node_core_pvf::Config::new(
			artifacts_cache_path,
			node_version,
			secure_validator_mode,
			prep_worker_path,
			exec_worker_path,
			pvf_execute_workers_max_num,
			pvf_prepare_workers_soft_max_num,
			pvf_prepare_workers_hard_max_num,
		),
		pvf_metrics,
	)
	.await?;
	ctx.spawn_blocking("pvf-validation-host", task.boxed())?;

	let mut tasks = FuturesUnordered::new();
	let mut prepare_state = PrepareValidationState::default();

	loop {
		loop {
			futures::select! {
				comm = ctx.recv().fuse() => {
					match comm {
						Ok(FromOrchestra::Signal(OverseerSignal::ActiveLeaves(update))) => {
							maybe_prepare_validation(ctx.sender(), keystore.clone(), validation_host.clone(), update, &mut prepare_state).await;
						},
						Ok(FromOrchestra::Signal(OverseerSignal::BlockFinalized(..))) => {},
						Ok(FromOrchestra::Signal(OverseerSignal::Conclude)) => return Ok(()),
						Ok(FromOrchestra::Communication { msg }) => {
							let task = handle_validation_message(ctx.sender().clone(), validation_host.clone(), metrics.clone(), msg);
							tasks.push(task);
							if tasks.len() >= TASK_LIMIT {
								break
							}
						},
						Err(e) => return Err(SubsystemError::from(e)),
					}
				},
				_ = tasks.select_next_some() => ()
			}
		}

		gum::debug!(target: LOG_TARGET, "Validation task limit hit");

		loop {
			futures::select! {
				signal = ctx.recv_signal().fuse() => {
					match signal {
						Ok(OverseerSignal::ActiveLeaves(_)) => {},
						Ok(OverseerSignal::BlockFinalized(..)) => {},
						Ok(OverseerSignal::Conclude) => return Ok(()),
						Err(e) => return Err(SubsystemError::from(e)),
					}
				},
				_ = tasks.select_next_some() => {
					if tasks.len() < TASK_LIMIT {
						break
					}
				}
			}
		}
	}
}

struct PrepareValidationState {
	session_index: Option<SessionIndex>,
	is_next_session_authority: bool,
	// PVF host won't prepare the same code hash twice, so here we just avoid extra communication
	already_prepared_code_hashes: HashSet<ValidationCodeHash>,
	// How many PVFs per block we take to prepare themselves for the next session validation
	per_block_limit: usize,
}

impl Default for PrepareValidationState {
	fn default() -> Self {
		Self {
			session_index: None,
			is_next_session_authority: false,
			already_prepared_code_hashes: HashSet::new(),
			per_block_limit: 1,
		}
	}
}

async fn maybe_prepare_validation<Sender>(
	sender: &mut Sender,
	keystore: KeystorePtr,
	validation_backend: impl ValidationBackend,
	update: ActiveLeavesUpdate,
	state: &mut PrepareValidationState,
) where
	Sender: SubsystemSender<RuntimeApiMessage>,
{
	let Some(leaf) = update.activated else { return };
	let new_session_index = new_session_index(sender, state.session_index, leaf.hash).await;
	if new_session_index.is_some() {
		state.session_index = new_session_index;
		state.already_prepared_code_hashes.clear();
		state.is_next_session_authority = check_next_session_authority(
			sender,
			keystore,
			leaf.hash,
			state.session_index.expect("qed: just checked above"),
		)
		.await;
	}

	// On every active leaf check candidates and prepare PVFs our node doesn't have yet.
	if state.is_next_session_authority {
		let code_hashes = prepare_pvfs_for_backed_candidates(
			sender,
			validation_backend,
			leaf.hash,
			&state.already_prepared_code_hashes,
			state.per_block_limit,
		)
		.await;
		state.already_prepared_code_hashes.extend(code_hashes.unwrap_or_default());
	}
}

// Returns the new session index if it is greater than the current one.
async fn new_session_index<Sender>(
	sender: &mut Sender,
	session_index: Option<SessionIndex>,
	relay_parent: Hash,
) -> Option<SessionIndex>
where
	Sender: SubsystemSender<RuntimeApiMessage>,
{
	let Ok(Ok(new_session_index)) =
		util::request_session_index_for_child(relay_parent, sender).await.await
	else {
		gum::warn!(
			target: LOG_TARGET,
			?relay_parent,
			"cannot fetch session index from runtime API",
		);
		return None
	};

	session_index.map_or(Some(new_session_index), |index| {
		if new_session_index > index {
			Some(new_session_index)
		} else {
			None
		}
	})
}

// Returns true if the node is an authority in the next session.
async fn check_next_session_authority<Sender>(
	sender: &mut Sender,
	keystore: KeystorePtr,
	relay_parent: Hash,
	session_index: SessionIndex,
) -> bool
where
	Sender: SubsystemSender<RuntimeApiMessage>,
{
	// In spite of function name here we request past, present and future authorities.
	// It's ok to stil prepare PVFs in other cases, but better to request only future ones.
	let Ok(Ok(authorities)) = util::request_authorities(relay_parent, sender).await.await else {
		gum::warn!(
			target: LOG_TARGET,
			?relay_parent,
			"cannot fetch authorities from runtime API",
		);
		return false
	};

	// We need to exclude at least current session authority from the previous request
	let Ok(Ok(Some(session_info))) =
		util::request_session_info(relay_parent, session_index, sender).await.await
	else {
		gum::warn!(
			target: LOG_TARGET,
			?relay_parent,
			"cannot fetch session info from runtime API",
		);
		return false
	};

	let is_past_present_or_future_authority = authorities
		.iter()
		.any(|v| keystore.has_keys(&[(v.to_raw_vec(), AuthorityDiscoveryId::ID)]));

	// We could've checked discovery_keys but on Kusama validators.len() < discovery_keys.len().
	let is_present_validator = session_info
		.validators
		.iter()
		.any(|v| keystore.has_keys(&[(v.to_raw_vec(), ValidatorId::ID)]));

	// There is still a chance to be a previous session authority, but this extra work does not
	// affect the finalization.
	is_past_present_or_future_authority && !is_present_validator
}

// Sends PVF with unknown code hashes to the validation host returning the list of code hashes sent.
async fn prepare_pvfs_for_backed_candidates<Sender>(
	sender: &mut Sender,
	mut validation_backend: impl ValidationBackend,
	relay_parent: Hash,
	already_prepared: &HashSet<ValidationCodeHash>,
	per_block_limit: usize,
) -> Option<Vec<ValidationCodeHash>>
where
	Sender: SubsystemSender<RuntimeApiMessage>,
{
	let Ok(Ok(events)) = util::request_candidate_events(relay_parent, sender).await.await else {
		gum::warn!(
			target: LOG_TARGET,
			?relay_parent,
			"cannot fetch candidate events from runtime API",
		);
		return None
	};
	let code_hashes = events
		.into_iter()
		.filter_map(|e| match e {
			CandidateEvent::CandidateBacked(receipt, ..) => {
				let h = receipt.descriptor.validation_code_hash;
				if already_prepared.contains(&h) {
					None
				} else {
					Some(h)
				}
			},
			_ => None,
		})
		.take(per_block_limit)
		.collect::<Vec<_>>();

	let Ok(executor_params) = util::executor_params_at_relay_parent(relay_parent, sender).await
	else {
		gum::warn!(
			target: LOG_TARGET,
			?relay_parent,
			"cannot fetch executor params for the session",
		);
		return None
	};
	let timeout = pvf_prep_timeout(&executor_params, PvfPrepKind::Prepare);

	let mut active_pvfs = vec![];
	let mut processed_code_hashes = vec![];
	for code_hash in code_hashes {
		let Ok(Ok(Some(validation_code))) =
			util::request_validation_code_by_hash(relay_parent, code_hash, sender)
				.await
				.await
		else {
			gum::warn!(
				target: LOG_TARGET,
				?relay_parent,
				?code_hash,
				"cannot fetch validation code hash from runtime API",
			);
			continue;
		};

		let pvf = PvfPrepData::from_code(
			validation_code.0,
			executor_params.clone(),
			timeout,
			PrepareJobKind::Prechecking,
		);

		active_pvfs.push(pvf);
		processed_code_hashes.push(code_hash);
	}

	if active_pvfs.is_empty() {
		return None
	}

	if let Err(err) = validation_backend.heads_up(active_pvfs).await {
		gum::warn!(
			target: LOG_TARGET,
			?relay_parent,
			?err,
			"cannot prepare PVF for the next session",
		);
		return None
	};

	gum::debug!(
		target: LOG_TARGET,
		?relay_parent,
		?processed_code_hashes,
		"Prepared PVF for the next session",
	);

	Some(processed_code_hashes)
}

struct RuntimeRequestFailed;

async fn runtime_api_request<T, Sender>(
	sender: &mut Sender,
	relay_parent: Hash,
	request: RuntimeApiRequest,
	receiver: oneshot::Receiver<Result<T, RuntimeApiError>>,
) -> Result<T, RuntimeRequestFailed>
where
	Sender: SubsystemSender<RuntimeApiMessage>,
{
	sender
		.send_message(RuntimeApiMessage::Request(relay_parent, request).into())
		.await;

	receiver
		.await
		.map_err(|_| {
			gum::debug!(target: LOG_TARGET, ?relay_parent, "Runtime API request dropped");

			RuntimeRequestFailed
		})
		.and_then(|res| {
			res.map_err(|e| {
				gum::debug!(
					target: LOG_TARGET,
					?relay_parent,
					err = ?e,
					"Runtime API request internal error"
				);

				RuntimeRequestFailed
			})
		})
}

async fn request_validation_code_by_hash<Sender>(
	sender: &mut Sender,
	relay_parent: Hash,
	validation_code_hash: ValidationCodeHash,
) -> Result<Option<ValidationCode>, RuntimeRequestFailed>
where
	Sender: SubsystemSender<RuntimeApiMessage>,
{
	let (tx, rx) = oneshot::channel();
	runtime_api_request(
		sender,
		relay_parent,
		RuntimeApiRequest::ValidationCodeByHash(validation_code_hash, tx),
		rx,
	)
	.await
}

async fn precheck_pvf<Sender>(
	sender: &mut Sender,
	mut validation_backend: impl ValidationBackend,
	relay_parent: Hash,
	validation_code_hash: ValidationCodeHash,
) -> PreCheckOutcome
where
	Sender: SubsystemSender<RuntimeApiMessage>,
{
	let validation_code =
		match request_validation_code_by_hash(sender, relay_parent, validation_code_hash).await {
			Ok(Some(code)) => code,
			_ => {
				// The reasoning why this is "failed" and not invalid is because we assume that
				// during pre-checking voting the relay-chain will pin the code. In case the code
				// actually is not there, we issue failed since this looks more like a bug.
				gum::warn!(
					target: LOG_TARGET,
					?relay_parent,
					?validation_code_hash,
					"precheck: requested validation code is not found on-chain!",
				);
				return PreCheckOutcome::Failed
			},
		};

	let executor_params = if let Ok(executor_params) =
		util::executor_params_at_relay_parent(relay_parent, sender).await
	{
		gum::debug!(
			target: LOG_TARGET,
			?relay_parent,
			?validation_code_hash,
			"precheck: acquired executor params for the session: {:?}",
			executor_params,
		);
		executor_params
	} else {
		gum::warn!(
			target: LOG_TARGET,
			?relay_parent,
			?validation_code_hash,
			"precheck: failed to acquire executor params for the session, thus voting against.",
		);
		return PreCheckOutcome::Invalid
	};

	let timeout = pvf_prep_timeout(&executor_params, PvfPrepKind::Precheck);

	let pvf = PvfPrepData::from_code(
		validation_code.0,
		executor_params,
		timeout,
		PrepareJobKind::Prechecking,
	);

	match validation_backend.precheck_pvf(pvf).await {
		Ok(_) => PreCheckOutcome::Valid,
		Err(prepare_err) =>
			if prepare_err.is_deterministic() {
				PreCheckOutcome::Invalid
			} else {
				PreCheckOutcome::Failed
			},
	}
}

async fn validate_candidate_exhaustive(
	mut validation_backend: impl ValidationBackend + Send,
	persisted_validation_data: PersistedValidationData,
	validation_code: ValidationCode,
	candidate_receipt: CandidateReceipt,
	pov: Arc<PoV>,
	executor_params: ExecutorParams,
	exec_kind: PvfExecKind,
	metrics: &Metrics,
) -> Result<ValidationResult, ValidationFailed> {
	let _timer = metrics.time_validate_candidate_exhaustive();

	let validation_code_hash = validation_code.hash();
	let para_id = candidate_receipt.descriptor.para_id;
	gum::debug!(
		target: LOG_TARGET,
		?validation_code_hash,
		?para_id,
		"About to validate a candidate.",
	);

	if let Err(e) = perform_basic_checks(
		&candidate_receipt.descriptor,
		persisted_validation_data.max_pov_size,
		&pov,
		&validation_code_hash,
	) {
		gum::info!(target: LOG_TARGET, ?para_id, "Invalid candidate (basic checks)");
		return Ok(ValidationResult::Invalid(e))
	}

	let persisted_validation_data = Arc::new(persisted_validation_data);
	let result = match exec_kind {
		// Retry is disabled to reduce the chance of nondeterministic blocks getting backed and
		// honest backers getting slashed.
		PvfExecKind::Backing | PvfExecKind::BackingSystemParas => {
			let prep_timeout = pvf_prep_timeout(&executor_params, PvfPrepKind::Prepare);
			let exec_timeout = pvf_exec_timeout(&executor_params, exec_kind.into());
			let pvf = PvfPrepData::from_code(
				validation_code.0,
				executor_params,
				prep_timeout,
				PrepareJobKind::Compilation,
			);

			validation_backend
				.validate_candidate(
					pvf,
					exec_timeout,
					persisted_validation_data.clone(),
					pov,
					exec_kind.into(),
					exec_kind,
				)
				.await
		},
		PvfExecKind::Approval | PvfExecKind::Dispute =>
			validation_backend
				.validate_candidate_with_retry(
					validation_code.0,
					pvf_exec_timeout(&executor_params, exec_kind.into()),
					persisted_validation_data.clone(),
					pov,
					executor_params,
					PVF_APPROVAL_EXECUTION_RETRY_DELAY,
					exec_kind.into(),
					exec_kind,
				)
				.await,
	};

	if let Err(ref error) = result {
		gum::info!(target: LOG_TARGET, ?para_id, ?error, "Failed to validate candidate");
	}

	match result {
		Err(ValidationError::Internal(e)) => {
			gum::warn!(
				target: LOG_TARGET,
				?para_id,
				?e,
				"An internal error occurred during validation, will abstain from voting",
			);
			Err(ValidationFailed(e.to_string()))
		},
		Err(ValidationError::Invalid(WasmInvalidCandidate::HardTimeout)) =>
			Ok(ValidationResult::Invalid(InvalidCandidate::Timeout)),
		Err(ValidationError::Invalid(WasmInvalidCandidate::WorkerReportedInvalid(e))) =>
			Ok(ValidationResult::Invalid(InvalidCandidate::ExecutionError(e))),
		Err(ValidationError::Invalid(WasmInvalidCandidate::PoVDecompressionFailure)) =>
			Ok(ValidationResult::Invalid(InvalidCandidate::PoVDecompressionFailure)),
		Err(ValidationError::PossiblyInvalid(PossiblyInvalidError::AmbiguousWorkerDeath)) =>
			Ok(ValidationResult::Invalid(InvalidCandidate::ExecutionError(
				"ambiguous worker death".to_string(),
			))),
		Err(ValidationError::PossiblyInvalid(PossiblyInvalidError::JobError(err))) =>
			Ok(ValidationResult::Invalid(InvalidCandidate::ExecutionError(err))),
		Err(ValidationError::PossiblyInvalid(PossiblyInvalidError::RuntimeConstruction(err))) =>
			Ok(ValidationResult::Invalid(InvalidCandidate::ExecutionError(err))),

		Err(ValidationError::PossiblyInvalid(PossiblyInvalidError::AmbiguousJobDeath(err))) =>
			Ok(ValidationResult::Invalid(InvalidCandidate::ExecutionError(format!(
				"ambiguous job death: {err}"
			)))),
		Err(ValidationError::Preparation(e)) => {
			gum::warn!(
				target: LOG_TARGET,
				?para_id,
				?e,
				"Deterministic error occurred during preparation (should have been ruled out by pre-checking phase)",
			);
			Err(ValidationFailed(e.to_string()))
		},
		Ok(res) =>
			if res.head_data.hash() != candidate_receipt.descriptor.para_head {
				gum::info!(target: LOG_TARGET, ?para_id, "Invalid candidate (para_head)");
				Ok(ValidationResult::Invalid(InvalidCandidate::ParaHeadHashMismatch))
			} else {
				let outputs = CandidateCommitments {
					head_data: res.head_data,
					upward_messages: res.upward_messages,
					horizontal_messages: res.horizontal_messages,
					new_validation_code: res.new_validation_code,
					processed_downward_messages: res.processed_downward_messages,
					hrmp_watermark: res.hrmp_watermark,
				};
				if candidate_receipt.commitments_hash != outputs.hash() {
					gum::info!(
						target: LOG_TARGET,
						?para_id,
						"Invalid candidate (commitments hash)"
					);

					// If validation produced a new set of commitments, we treat the candidate as
					// invalid.
					Ok(ValidationResult::Invalid(InvalidCandidate::CommitmentsHashMismatch))
				} else {
					Ok(ValidationResult::Valid(outputs, (*persisted_validation_data).clone()))
				}
			},
	}
}

#[async_trait]
trait ValidationBackend {
	/// Tries executing a PVF a single time (no retries).
	async fn validate_candidate(
		&mut self,
		pvf: PvfPrepData,
		exec_timeout: Duration,
		pvd: Arc<PersistedValidationData>,
		pov: Arc<PoV>,
		// The priority for the preparation job.
		prepare_priority: polkadot_node_core_pvf::Priority,
		// The kind for the execution job.
		exec_kind: PvfExecKind,
	) -> Result<WasmValidationResult, ValidationError>;

	/// Tries executing a PVF. Will retry once if an error is encountered that may have
	/// been transient.
	///
	/// The `prepare_priority` is relevant in the context of the caller. Currently we expect
	/// that `approval` context has priority over `backing` context.
	///
	/// NOTE: Should retry only on errors that are a result of execution itself, and not of
	/// preparation.
	async fn validate_candidate_with_retry(
		&mut self,
		code: Vec<u8>,
		exec_timeout: Duration,
		pvd: Arc<PersistedValidationData>,
		pov: Arc<PoV>,
		executor_params: ExecutorParams,
		retry_delay: Duration,
		// The priority for the preparation job.
		prepare_priority: polkadot_node_core_pvf::Priority,
		// The kind for the execution job.
		exec_kind: PvfExecKind,
	) -> Result<WasmValidationResult, ValidationError> {
		let prep_timeout = pvf_prep_timeout(&executor_params, PvfPrepKind::Prepare);
		// Construct the PVF a single time, since it is an expensive operation. Cloning it is cheap.
		let pvf = PvfPrepData::from_code(
			code,
			executor_params,
			prep_timeout,
			PrepareJobKind::Compilation,
		);
		// We keep track of the total time that has passed and stop retrying if we are taking too
		// long.
		let total_time_start = Instant::now();

		// Use `Priority::Critical` as finality trumps parachain liveliness.
		let mut validation_result = self
			.validate_candidate(
				pvf.clone(),
				exec_timeout,
				pvd.clone(),
				pov.clone(),
				prepare_priority,
				exec_kind,
			)
			.await;
		if validation_result.is_ok() {
			return validation_result
		}

		macro_rules! break_if_no_retries_left {
			($counter:ident) => {
				if $counter > 0 {
					$counter -= 1;
				} else {
					break
				}
			};
		}

		// Allow limited retries for each kind of error.
		let mut num_death_retries_left = 1;
		let mut num_job_error_retries_left = 1;
		let mut num_internal_retries_left = 1;
		let mut num_runtime_construction_retries_left = 1;
		loop {
			// Stop retrying if we exceeded the timeout.
			if total_time_start.elapsed() + retry_delay > exec_timeout {
				break
			}
			let mut retry_immediately = false;
			match validation_result {
				Err(ValidationError::PossiblyInvalid(
					PossiblyInvalidError::AmbiguousWorkerDeath |
					PossiblyInvalidError::AmbiguousJobDeath(_),
				)) => break_if_no_retries_left!(num_death_retries_left),

				Err(ValidationError::PossiblyInvalid(PossiblyInvalidError::JobError(_))) =>
					break_if_no_retries_left!(num_job_error_retries_left),

				Err(ValidationError::Internal(_)) =>
					break_if_no_retries_left!(num_internal_retries_left),

				Err(ValidationError::PossiblyInvalid(
					PossiblyInvalidError::RuntimeConstruction(_),
				)) => {
					break_if_no_retries_left!(num_runtime_construction_retries_left);
					self.precheck_pvf(pvf.clone()).await?;
					// In this case the error is deterministic
					// And a retry forces the ValidationBackend
					// to re-prepare the artifact so
					// there is no need to wait before the retry
					retry_immediately = true;
				},

				Ok(_) | Err(ValidationError::Invalid(_) | ValidationError::Preparation(_)) => break,
			}

			// If we got a possibly transient error, retry once after a brief delay, on the
			// assumption that the conditions that caused this error may have resolved on their own.
			{
				// In case of many transient errors it is necessary to wait a little bit
				// for the error to be probably resolved
				if !retry_immediately {
					futures_timer::Delay::new(retry_delay).await;
				}

				let new_timeout = exec_timeout.saturating_sub(total_time_start.elapsed());

				gum::warn!(
					target: LOG_TARGET,
					?pvf,
					?new_timeout,
					"Re-trying failed candidate validation due to possible transient error: {:?}",
					validation_result
				);

				validation_result = self
					.validate_candidate(
						pvf.clone(),
						new_timeout,
						pvd.clone(),
						pov.clone(),
						prepare_priority,
						exec_kind,
					)
					.await;
			}
		}

		validation_result
	}

	async fn precheck_pvf(&mut self, pvf: PvfPrepData) -> Result<(), PrepareError>;

	async fn heads_up(&mut self, active_pvfs: Vec<PvfPrepData>) -> Result<(), String>;
}

#[async_trait]
impl ValidationBackend for ValidationHost {
	/// Tries executing a PVF a single time (no retries).
	async fn validate_candidate(
		&mut self,
		pvf: PvfPrepData,
		exec_timeout: Duration,
		pvd: Arc<PersistedValidationData>,
		pov: Arc<PoV>,
		// The priority for the preparation job.
		prepare_priority: polkadot_node_core_pvf::Priority,
		// The kind for the execution job.
		exec_kind: PvfExecKind,
	) -> Result<WasmValidationResult, ValidationError> {
		let (tx, rx) = oneshot::channel();
		if let Err(err) = self
			.execute_pvf(pvf, exec_timeout, pvd, pov, prepare_priority, exec_kind, tx)
			.await
		{
			return Err(InternalValidationError::HostCommunication(format!(
				"cannot send pvf to the validation host, it might have shut down: {:?}",
				err
			))
			.into())
		}

		rx.await.map_err(|_| {
			ValidationError::from(InternalValidationError::HostCommunication(
				"validation was cancelled".into(),
			))
		})?
	}

	async fn precheck_pvf(&mut self, pvf: PvfPrepData) -> Result<(), PrepareError> {
		let (tx, rx) = oneshot::channel();
		if let Err(err) = self.precheck_pvf(pvf, tx).await {
			// Return an IO error if there was an error communicating with the host.
			return Err(PrepareError::IoErr(err))
		}

		let precheck_result = rx.await.map_err(|err| PrepareError::IoErr(err.to_string()))?;

		precheck_result
	}

	async fn heads_up(&mut self, active_pvfs: Vec<PvfPrepData>) -> Result<(), String> {
		self.heads_up(active_pvfs).await
	}
}

/// Does basic checks of a candidate. Provide the encoded PoV-block. Returns `Ok` if basic checks
/// are passed, `Err` otherwise.
fn perform_basic_checks(
	candidate: &CandidateDescriptor,
	max_pov_size: u32,
	pov: &PoV,
	validation_code_hash: &ValidationCodeHash,
) -> Result<(), InvalidCandidate> {
	let pov_hash = pov.hash();

	let encoded_pov_size = pov.encoded_size();
	if encoded_pov_size > max_pov_size as usize {
		return Err(InvalidCandidate::ParamsTooLarge(encoded_pov_size as u64))
	}

	if pov_hash != candidate.pov_hash {
		return Err(InvalidCandidate::PoVHashMismatch)
	}

	if *validation_code_hash != candidate.validation_code_hash {
		return Err(InvalidCandidate::CodeHashMismatch)
	}

	if let Err(()) = candidate.check_collator_signature() {
		return Err(InvalidCandidate::BadSignature)
	}

	Ok(())
}

/// To determine the amount of timeout time for the pvf execution.
///
/// Precheck
///	The time period after which the preparation worker is considered
/// unresponsive and will be killed.
///
/// Prepare
///The time period after which the preparation worker is considered
/// unresponsive and will be killed.
fn pvf_prep_timeout(executor_params: &ExecutorParams, kind: PvfPrepKind) -> Duration {
	if let Some(timeout) = executor_params.pvf_prep_timeout(kind) {
		return timeout
	}
	match kind {
		PvfPrepKind::Precheck => DEFAULT_PRECHECK_PREPARATION_TIMEOUT,
		PvfPrepKind::Prepare => DEFAULT_LENIENT_PREPARATION_TIMEOUT,
	}
}

/// To determine the amount of timeout time for the pvf execution.
///
/// Backing subsystem
/// The amount of time to spend on execution during backing.
///
/// Approval subsystem
/// The amount of time to spend on execution during approval or disputes.
/// This should be much longer than the backing execution timeout to ensure that in the
/// absence of extremely large disparities between hardware, blocks that pass backing are
/// considered executable by approval checkers or dispute participants.
fn pvf_exec_timeout(executor_params: &ExecutorParams, kind: RuntimePvfExecKind) -> Duration {
	if let Some(timeout) = executor_params.pvf_exec_timeout(kind) {
		return timeout
	}
	match kind {
		RuntimePvfExecKind::Backing => DEFAULT_BACKING_EXECUTION_TIMEOUT,
		RuntimePvfExecKind::Approval => DEFAULT_APPROVAL_EXECUTION_TIMEOUT,
	}
}
