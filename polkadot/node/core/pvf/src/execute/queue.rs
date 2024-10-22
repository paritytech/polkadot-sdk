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

//! A queue that handles requests for PVF execution.

use super::worker_interface::{Error as WorkerInterfaceError, Response as WorkerInterfaceResponse};
use crate::{
	artifacts::{ArtifactId, ArtifactPathId},
	host::ResultSender,
	metrics::Metrics,
	worker_interface::{IdleWorker, WorkerHandle},
	InvalidCandidate, PossiblyInvalidError, ValidationError, LOG_TARGET,
};
use futures::{
	channel::{mpsc, oneshot},
	future::BoxFuture,
	stream::{FuturesUnordered, StreamExt as _},
	Future, FutureExt,
};
use polkadot_node_core_pvf_common::{
	execute::{JobResponse, WorkerError, WorkerResponse},
	SecurityStatus,
};
use polkadot_node_primitives::PoV;
use polkadot_node_subsystem::messages::PvfExecKind;
use polkadot_primitives::{ExecutorParams, ExecutorParamsHash, PersistedValidationData};
use slotmap::HopSlotMap;
use std::{
	collections::{HashMap, VecDeque},
	fmt,
	path::PathBuf,
	sync::Arc,
	time::{Duration, Instant},
};
use strum::IntoEnumIterator;

/// The amount of time a job for which the queue does not have a compatible worker may wait in the
/// queue. After that time passes, the queue will kill the first worker which becomes idle to
/// re-spawn a new worker to execute the job immediately.
/// To make any sense and not to break things, the value should be greater than minimal execution
/// timeout in use, and less than the block time.
const MAX_KEEP_WAITING: Duration = Duration::from_secs(4);

slotmap::new_key_type! { struct Worker; }

#[derive(Debug)]
pub enum ToQueue {
	Enqueue { artifact: ArtifactPathId, pending_execution_request: PendingExecutionRequest },
}

/// A response from queue.
#[derive(Debug)]
pub enum FromQueue {
	RemoveArtifact { artifact: ArtifactId, reply_to: oneshot::Sender<()> },
}

/// An execution request that should execute the PVF (known in the context) and send the results
/// to the given result sender.
#[derive(Debug)]
pub struct PendingExecutionRequest {
	pub exec_timeout: Duration,
	pub pvd: Arc<PersistedValidationData>,
	pub pov: Arc<PoV>,
	pub executor_params: ExecutorParams,
	pub result_tx: ResultSender,
	pub exec_kind: PvfExecKind,
}

struct ExecuteJob {
	artifact: ArtifactPathId,
	exec_timeout: Duration,
	pvd: Arc<PersistedValidationData>,
	pov: Arc<PoV>,
	executor_params: ExecutorParams,
	result_tx: ResultSender,
	waiting_since: Instant,
}

struct WorkerData {
	idle: Option<IdleWorker>,
	handle: WorkerHandle,
	executor_params_hash: ExecutorParamsHash,
}

impl fmt::Debug for WorkerData {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		write!(f, "WorkerData(pid={})", self.handle.id())
	}
}

struct Workers {
	/// The registry of running workers.
	running: HopSlotMap<Worker, WorkerData>,

	/// The number of spawning but not yet spawned workers.
	spawn_inflight: usize,

	/// The maximum number of workers queue can have at once.
	capacity: usize,
}

impl Workers {
	fn can_afford_one_more(&self) -> bool {
		self.spawn_inflight + self.running.len() < self.capacity
	}

	fn find_available(&self, executor_params_hash: ExecutorParamsHash) -> Option<Worker> {
		self.running.iter().find_map(|d| {
			if d.1.idle.is_some() && d.1.executor_params_hash == executor_params_hash {
				Some(d.0)
			} else {
				None
			}
		})
	}

	fn find_idle(&self) -> Option<Worker> {
		self.running
			.iter()
			.find_map(|d| if d.1.idle.is_some() { Some(d.0) } else { None })
	}

	/// Find the associated data by the worker token and extract it's [`IdleWorker`] token.
	///
	/// Returns `None` if either worker is not recognized or idle token is absent.
	fn claim_idle(&mut self, worker: Worker) -> Option<IdleWorker> {
		self.running.get_mut(worker)?.idle.take()
	}
}

enum QueueEvent {
	Spawn(IdleWorker, WorkerHandle, ExecuteJob),
	FinishWork(
		Worker,
		Result<WorkerInterfaceResponse, WorkerInterfaceError>,
		ArtifactId,
		ResultSender,
	),
}

type Mux = FuturesUnordered<BoxFuture<'static, QueueEvent>>;

struct Queue {
	metrics: Metrics,

	/// The receiver that receives messages to the pool.
	to_queue_rx: mpsc::Receiver<ToQueue>,
	/// The sender to send messages back to validation host.
	from_queue_tx: mpsc::UnboundedSender<FromQueue>,

	// Some variables related to the current session.
	program_path: PathBuf,
	cache_path: PathBuf,
	spawn_timeout: Duration,
	node_version: Option<String>,
	security_status: SecurityStatus,

	/// The queue of jobs that are waiting for a worker to pick up.
	unscheduled: Unscheduled,
	workers: Workers,
	mux: Mux,
}

impl Queue {
	fn new(
		metrics: Metrics,
		program_path: PathBuf,
		cache_path: PathBuf,
		worker_capacity: usize,
		spawn_timeout: Duration,
		node_version: Option<String>,
		security_status: SecurityStatus,
		to_queue_rx: mpsc::Receiver<ToQueue>,
		from_queue_tx: mpsc::UnboundedSender<FromQueue>,
	) -> Self {
		Self {
			metrics,
			program_path,
			cache_path,
			spawn_timeout,
			node_version,
			security_status,
			to_queue_rx,
			from_queue_tx,
			unscheduled: Unscheduled::new(),
			mux: Mux::new(),
			workers: Workers {
				running: HopSlotMap::with_capacity_and_key(10),
				spawn_inflight: 0,
				capacity: worker_capacity,
			},
		}
	}

	async fn run(mut self) {
		loop {
			futures::select! {
				to_queue = self.to_queue_rx.next() => {
					if let Some(to_queue) = to_queue {
						handle_to_queue(&mut self, to_queue);
					} else {
						break;
					}
				}
				ev = self.mux.select_next_some() => handle_mux(&mut self, ev).await,
			}

			purge_dead(&self.metrics, &mut self.workers).await;
		}
	}

	/// Tries to assign a job in the queue to a worker. If an idle worker is provided, it does its
	/// best to find a job with a compatible execution environment unless there are jobs in the
	/// queue waiting too long. In that case, it kills an existing idle worker and spawns a new
	/// one. It may spawn an additional worker if that is affordable.
	/// If all the workers are busy or the queue is empty, it does nothing.
	/// Should be called every time a new job arrives to the queue or a job finishes.
	fn try_assign_next_job(&mut self, finished_worker: Option<Worker>) {
		// We always work at the same priority level
		let priority = self.unscheduled.select_next_priority();
		let Some(queue) = self.unscheduled.get_mut(priority) else { return };

		// New jobs are always pushed to the tail of the queue based on their priority;
		// the one at its head of each queue is always the eldest one.
		let eldest = if let Some(eldest) = queue.get(0) { eldest } else { return };

		// By default, we're going to execute the eldest job on any worker slot available, even if
		// we have to kill and re-spawn a worker
		let mut worker = None;
		let mut job_index = 0;

		// But if we're not pressed for time, we can try to find a better job-worker pair not
		// requiring the expensive kill-spawn operation
		if eldest.waiting_since.elapsed() < MAX_KEEP_WAITING {
			if let Some(finished_worker) = finished_worker {
				if let Some(worker_data) = self.workers.running.get(finished_worker) {
					for (i, job) in queue.iter().enumerate() {
						if worker_data.executor_params_hash == job.executor_params.hash() {
							(worker, job_index) = (Some(finished_worker), i);
							break
						}
					}
				}
			}
		}

		if worker.is_none() {
			// Try to obtain a worker for the job
			worker = self.workers.find_available(queue[job_index].executor_params.hash());
		}

		if worker.is_none() {
			if let Some(idle) = self.workers.find_idle() {
				// No available workers of required type but there are some idle ones of other
				// types, have to kill one and re-spawn with the correct type
				if self.workers.running.remove(idle).is_some() {
					self.metrics.execute_worker().on_retired();
				}
			}
		}

		if worker.is_none() && !self.workers.can_afford_one_more() {
			// Bad luck, no worker slot can be used to execute the job
			return
		}

		let job = queue.remove(job_index).expect("Job is just checked to be in queue; qed");

		if let Some(worker) = worker {
			assign(self, worker, job);
		} else {
			spawn_extra_worker(self, job);
		}
		self.metrics.on_execute_kind(priority);
		self.unscheduled.mark_scheduled(priority);
	}
}

async fn purge_dead(metrics: &Metrics, workers: &mut Workers) {
	let mut to_remove = vec![];
	for (worker, data) in workers.running.iter_mut() {
		if futures::poll!(&mut data.handle).is_ready() {
			// a resolved future means that the worker has terminated. Weed it out.
			to_remove.push(worker);
		}
	}
	for w in to_remove {
		if workers.running.remove(w).is_some() {
			metrics.execute_worker().on_retired();
		}
	}
}

fn handle_to_queue(queue: &mut Queue, to_queue: ToQueue) {
	let ToQueue::Enqueue { artifact, pending_execution_request } = to_queue;
	let PendingExecutionRequest { exec_timeout, pvd, pov, executor_params, result_tx, exec_kind } =
		pending_execution_request;
	gum::debug!(
		target: LOG_TARGET,
		validation_code_hash = ?artifact.id.code_hash,
		"enqueueing an artifact for execution",
	);
	queue.metrics.observe_pov_size(pov.block_data.0.len(), true);
	queue.metrics.execute_enqueued();
	let job = ExecuteJob {
		artifact,
		exec_timeout,
		pvd,
		pov,
		executor_params,
		result_tx,
		waiting_since: Instant::now(),
	};
	queue.unscheduled.add(job, exec_kind);
	queue.try_assign_next_job(None);
}

async fn handle_mux(queue: &mut Queue, event: QueueEvent) {
	match event {
		QueueEvent::Spawn(idle, handle, job) => {
			handle_worker_spawned(queue, idle, handle, job);
		},
		QueueEvent::FinishWork(worker, outcome, artifact_id, result_tx) => {
			handle_job_finish(queue, worker, outcome, artifact_id, result_tx).await;
		},
	}
}

fn handle_worker_spawned(
	queue: &mut Queue,
	idle: IdleWorker,
	handle: WorkerHandle,
	job: ExecuteJob,
) {
	queue.metrics.execute_worker().on_spawned();
	queue.workers.spawn_inflight -= 1;
	let worker = queue.workers.running.insert(WorkerData {
		idle: Some(idle),
		handle,
		executor_params_hash: job.executor_params.hash(),
	});

	gum::debug!(target: LOG_TARGET, ?worker, "execute worker spawned");

	assign(queue, worker, job);
}

/// If there are pending jobs in the queue, schedules the next of them onto the just freed up
/// worker. Otherwise, puts back into the available workers list.
async fn handle_job_finish(
	queue: &mut Queue,
	worker: Worker,
	worker_result: Result<WorkerInterfaceResponse, WorkerInterfaceError>,
	artifact_id: ArtifactId,
	result_tx: ResultSender,
) {
	let (idle_worker, result, duration, sync_channel, pov_size) = match worker_result {
		Ok(WorkerInterfaceResponse {
			worker_response:
				WorkerResponse {
					job_response: JobResponse::Ok { result_descriptor },
					duration,
					pov_size,
				},
			idle_worker,
		}) => {
			// TODO: propagate the soft timeout

			(Some(idle_worker), Ok(result_descriptor), Some(duration), None, Some(pov_size))
		},
		Ok(WorkerInterfaceResponse {
			worker_response: WorkerResponse { job_response: JobResponse::InvalidCandidate(err), .. },
			idle_worker,
		}) => (
			Some(idle_worker),
			Err(ValidationError::Invalid(InvalidCandidate::WorkerReportedInvalid(err))),
			None,
			None,
			None,
		),
		Ok(WorkerInterfaceResponse {
			worker_response:
				WorkerResponse { job_response: JobResponse::PoVDecompressionFailure, .. },
			idle_worker,
		}) => (
			Some(idle_worker),
			Err(ValidationError::Invalid(InvalidCandidate::PoVDecompressionFailure)),
			None,
			None,
			None,
		),
		Ok(WorkerInterfaceResponse {
			worker_response:
				WorkerResponse { job_response: JobResponse::RuntimeConstruction(err), .. },
			idle_worker,
		}) => {
			// The task for artifact removal is executed concurrently with
			// the message to the host on the execution result.
			let (result_tx, result_rx) = oneshot::channel();
			queue
				.from_queue_tx
				.unbounded_send(FromQueue::RemoveArtifact {
					artifact: artifact_id.clone(),
					reply_to: result_tx,
				})
				.expect("from execute queue receiver is listened by the host; qed");
			(
				Some(idle_worker),
				Err(ValidationError::PossiblyInvalid(PossiblyInvalidError::RuntimeConstruction(
					err,
				))),
				None,
				Some(result_rx),
				None,
			)
		},

		Err(WorkerInterfaceError::InternalError(err)) |
		Err(WorkerInterfaceError::WorkerError(WorkerError::InternalError(err))) =>
			(None, Err(ValidationError::Internal(err)), None, None, None),
		// Either the worker or the job timed out. Kill the worker in either case. Treated as
		// definitely-invalid, because if we timed out, there's no time left for a retry.
		Err(WorkerInterfaceError::HardTimeout) |
		Err(WorkerInterfaceError::WorkerError(WorkerError::JobTimedOut)) =>
			(None, Err(ValidationError::Invalid(InvalidCandidate::HardTimeout)), None, None, None),
		// "Maybe invalid" errors (will retry).
		Err(WorkerInterfaceError::CommunicationErr(_err)) => (
			None,
			Err(ValidationError::PossiblyInvalid(PossiblyInvalidError::AmbiguousWorkerDeath)),
			None,
			None,
			None,
		),
		Err(WorkerInterfaceError::WorkerError(WorkerError::JobDied { err, .. })) => (
			None,
			Err(ValidationError::PossiblyInvalid(PossiblyInvalidError::AmbiguousJobDeath(err))),
			None,
			None,
			None,
		),
		Err(WorkerInterfaceError::WorkerError(WorkerError::JobError(err))) => (
			None,
			Err(ValidationError::PossiblyInvalid(PossiblyInvalidError::JobError(err.to_string()))),
			None,
			None,
			None,
		),
	};

	queue.metrics.execute_finished();
	if let Some(pov_size) = pov_size {
		queue.metrics.observe_pov_size(pov_size as usize, false)
	}
	if let Err(ref err) = result {
		gum::warn!(
			target: LOG_TARGET,
			?artifact_id,
			?worker,
			worker_rip = idle_worker.is_none(),
			"execution worker concluded, error occurred: {}",
			err
		);
	} else {
		gum::trace!(
			target: LOG_TARGET,
			?artifact_id,
			?worker,
			worker_rip = idle_worker.is_none(),
			?duration,
			"execute worker concluded successfully",
		);
	}

	if let Some(sync_channel) = sync_channel {
		// err means the sender is dropped (the artifact is already removed from the cache)
		// so that's legitimate to ignore the result
		let _ = sync_channel.await;
	}

	// First we send the result. It may fail due to the other end of the channel being dropped,
	// that's legitimate and we don't treat that as an error.
	let _ = result_tx.send(result);

	// Then, we should deal with the worker:
	//
	// - if the `idle_worker` token was returned we should either schedule the next task or just put
	//   it back so that the next incoming job will be able to claim it
	//
	// - if the `idle_worker` token was consumed, all the metadata pertaining to that worker should
	//   be removed.
	if let Some(idle_worker) = idle_worker {
		if let Some(data) = queue.workers.running.get_mut(worker) {
			data.idle = Some(idle_worker);
			return queue.try_assign_next_job(Some(worker))
		}
	} else {
		// Note it's possible that the worker was purged already by `purge_dead`
		if queue.workers.running.remove(worker).is_some() {
			queue.metrics.execute_worker().on_retired();
		}
	}

	queue.try_assign_next_job(None);
}

fn spawn_extra_worker(queue: &mut Queue, job: ExecuteJob) {
	queue.metrics.execute_worker().on_begin_spawn();
	gum::debug!(target: LOG_TARGET, "spawning an extra worker");

	queue.mux.push(
		spawn_worker_task(
			queue.program_path.clone(),
			queue.cache_path.clone(),
			job,
			queue.spawn_timeout,
			queue.node_version.clone(),
			queue.security_status.clone(),
		)
		.boxed(),
	);
	queue.workers.spawn_inflight += 1;
}

/// Spawns a new worker to execute a pre-assigned job.
/// A worker is never spawned as idle; a job to be executed by the worker has to be determined
/// beforehand. In such a way, a race condition is avoided: during the worker being spawned,
/// another job in the queue, with an incompatible execution environment, may become stale, and
/// the queue would have to kill a newly started worker and spawn another one.
/// Nevertheless, if the worker finishes executing the job, it becomes idle and may be used to
/// execute other jobs with a compatible execution environment.
async fn spawn_worker_task(
	program_path: PathBuf,
	cache_path: PathBuf,
	job: ExecuteJob,
	spawn_timeout: Duration,
	node_version: Option<String>,
	security_status: SecurityStatus,
) -> QueueEvent {
	use futures_timer::Delay;

	loop {
		match super::worker_interface::spawn(
			&program_path,
			&cache_path,
			job.executor_params.clone(),
			spawn_timeout,
			node_version.as_deref(),
			security_status.clone(),
		)
		.await
		{
			Ok((idle, handle)) => break QueueEvent::Spawn(idle, handle, job),
			Err(err) => {
				gum::warn!(target: LOG_TARGET, "failed to spawn an execute worker: {:?}", err);

				// Assume that the failure is intermittent and retry after a delay.
				Delay::new(Duration::from_secs(3)).await;
			},
		}
	}
}

/// Ask the given worker to perform the given job.
///
/// The worker must be running and idle. The job and the worker must share the same execution
/// environment parameter set.
fn assign(queue: &mut Queue, worker: Worker, job: ExecuteJob) {
	gum::debug!(
		target: LOG_TARGET,
		validation_code_hash = ?job.artifact.id,
		?worker,
		"assigning the execute worker",
	);

	debug_assert_eq!(
		queue
			.workers
			.running
			.get(worker)
			.expect("caller must provide existing worker; qed")
			.executor_params_hash,
		job.executor_params.hash()
	);

	let idle = queue.workers.claim_idle(worker).expect(
		"this caller must supply a worker which is idle and running;
			thus claim_idle cannot return None;
			qed.",
	);
	queue
		.metrics
		.observe_execution_queued_time(job.waiting_since.elapsed().as_millis() as u32);
	let execution_timer = queue.metrics.time_execution();
	queue.mux.push(
		async move {
			let _timer = execution_timer;
			let result = super::worker_interface::start_work(
				idle,
				job.artifact.clone(),
				job.exec_timeout,
				job.pvd,
				job.pov,
			)
			.await;
			QueueEvent::FinishWork(worker, result, job.artifact.id, job.result_tx)
		}
		.boxed(),
	);
}

pub fn start(
	metrics: Metrics,
	program_path: PathBuf,
	cache_path: PathBuf,
	worker_capacity: usize,
	spawn_timeout: Duration,
	node_version: Option<String>,
	security_status: SecurityStatus,
) -> (mpsc::Sender<ToQueue>, mpsc::UnboundedReceiver<FromQueue>, impl Future<Output = ()>) {
	let (to_queue_tx, to_queue_rx) = mpsc::channel(20);
	let (from_queue_tx, from_queue_rx) = mpsc::unbounded();

	let run = Queue::new(
		metrics,
		program_path,
		cache_path,
		worker_capacity,
		spawn_timeout,
		node_version,
		security_status,
		to_queue_rx,
		from_queue_tx,
	)
	.run();
	(to_queue_tx, from_queue_rx, run)
}

struct Unscheduled {
	unscheduled: HashMap<PvfExecKind, VecDeque<ExecuteJob>>,
	counter: HashMap<PvfExecKind, usize>,
}

impl Unscheduled {
	/// We keep track of every scheduled job in the `counter`, but reset it if the total number of
	/// counted jobs reaches the threshold. This number is set as the maximum amount of jobs per
	/// relay chain block possible with 4 CPU cores and 2 seconds of execution time. Under normal
	/// conditions, the maximum expected queue size is at least vrf_module_samples(6) + 1 for
	/// backing a parachain candidate. A buffer is added to cover situations where more work
	/// arrives in the queue.
	const SCHEDULING_WINDOW_SIZE: usize = 12;

	/// A threshold in percentages indicates how much time a current priority can "steal" from lower
	/// priorities. Given the `SCHEDULING_WINDOW_SIZE` is 12 and all job priorities are present:
	/// - Disputes consume 70% or 8 jobs in a row.
	/// - The remaining 30% of original 100% is allocated for approval and all backing jobs.
	/// - 80% or 3 jobs of the remaining goes to approvals.
	/// - The remaining 6% of original 100% is allocated for all backing jobs.
	/// - 100% or 1 job of the remaining goes to backing system parachains.
	/// - Nothing is left for backing.
	/// - The counter is restarted and the distribution starts from the beginning.
	///
	/// This system might seem complex, but we operate with the remaining percentages because:
	/// - Not all job types are present in each block. If we used parts of the original 100%,
	///   approvals could not exceed 24%, even if there are no disputes.
	/// - We cannot fully prioritize backing system parachains over backing other parachains based
	///   on the distribution of the original 100%.
	const PRIORITY_ALLOCATION_THRESHOLDS: &'static [(PvfExecKind, usize)] = &[
		(PvfExecKind::Dispute, 70),
		(PvfExecKind::Approval, 80),
		(PvfExecKind::BackingSystemParas, 100),
		(PvfExecKind::Backing, 100),
	];

	fn new() -> Self {
		Self {
			unscheduled: PvfExecKind::iter().map(|priority| (priority, VecDeque::new())).collect(),
			counter: PvfExecKind::iter().map(|priority| (priority, 0)).collect(),
		}
	}

	fn select_next_priority(&self) -> PvfExecKind {
		gum::debug!(
			target: LOG_TARGET,
			unscheduled = ?self.unscheduled.iter().map(|(p, q)| (*p, q.len())).collect::<HashMap<PvfExecKind, usize>>(),
			counter = ?self.counter,
			"Selecting next execution priority...",
		);

		let priority = PvfExecKind::iter()
			.find(|priority| self.has_pending(priority) && !self.has_reached_threshold(priority))
			.unwrap_or_else(|| {
				PvfExecKind::iter()
					.find(|priority| self.has_pending(priority))
					.unwrap_or(PvfExecKind::Backing)
			});

		gum::debug!(
			target: LOG_TARGET,
			?priority,
			"Selected next execution priority",
		);

		priority
	}

	fn get_mut(&mut self, priority: PvfExecKind) -> Option<&mut VecDeque<ExecuteJob>> {
		self.unscheduled.get_mut(&priority)
	}

	fn add(&mut self, job: ExecuteJob, priority: PvfExecKind) {
		self.unscheduled.entry(priority).or_default().push_back(job);
	}

	fn has_pending(&self, priority: &PvfExecKind) -> bool {
		!self.unscheduled.get(priority).unwrap_or(&VecDeque::new()).is_empty()
	}

	fn priority_allocation_threshold(priority: &PvfExecKind) -> Option<usize> {
		Self::PRIORITY_ALLOCATION_THRESHOLDS.iter().find_map(|&(p, value)| {
			if p == *priority {
				Some(value)
			} else {
				None
			}
		})
	}

	/// Checks if a given priority has reached its allocated threshold
	/// The thresholds are defined in `PRIORITY_ALLOCATION_THRESHOLDS`.
	fn has_reached_threshold(&self, priority: &PvfExecKind) -> bool {
		let Some(threshold) = Self::priority_allocation_threshold(priority) else { return false };
		let Some(count) = self.counter.get(&priority) else { return false };
		// Every time we iterate by lower level priorities
		let total_scheduled_at_priority_or_lower: usize = self
			.counter
			.iter()
			.filter_map(|(p, c)| if *p >= *priority { Some(c) } else { None })
			.sum();
		if total_scheduled_at_priority_or_lower == 0 {
			return false
		}

		let has_reached_threshold = count * 100 / total_scheduled_at_priority_or_lower >= threshold;

		gum::debug!(
			target: LOG_TARGET,
			?priority,
			?count,
			?total_scheduled_at_priority_or_lower,
			"Execution priority has {}reached threshold: {}/{}%",
			if has_reached_threshold {""} else {"not "},
			count * 100 / total_scheduled_at_priority_or_lower,
			threshold
		);

		has_reached_threshold
	}

	fn mark_scheduled(&mut self, priority: PvfExecKind) {
		*self.counter.entry(priority).or_default() += 1;

		if self.counter.values().sum::<usize>() >= Self::SCHEDULING_WINDOW_SIZE {
			self.reset_counter();
		}
	}

	fn reset_counter(&mut self) {
		self.counter = PvfExecKind::iter().map(|kind| (kind, 0)).collect();
	}
}

#[cfg(test)]
mod tests {
	use polkadot_node_primitives::BlockData;
	use sp_core::H256;

	use super::*;
	use crate::testing::artifact_id;
	use std::time::Duration;

	fn create_execution_job() -> ExecuteJob {
		let (result_tx, _result_rx) = oneshot::channel();
		let pvd = Arc::new(PersistedValidationData {
			parent_head: Default::default(),
			relay_parent_number: 1u32,
			relay_parent_storage_root: H256::default(),
			max_pov_size: 4096 * 1024,
		});
		let pov = Arc::new(PoV { block_data: BlockData(b"pov".to_vec()) });
		ExecuteJob {
			artifact: ArtifactPathId { id: artifact_id(0), path: PathBuf::new() },
			exec_timeout: Duration::from_secs(10),
			pvd,
			pov,
			executor_params: ExecutorParams::default(),
			result_tx,
			waiting_since: Instant::now(),
		}
	}

	#[test]
	fn test_unscheduled_add() {
		let mut unscheduled = Unscheduled::new();

		PvfExecKind::iter().for_each(|priority| {
			unscheduled.add(create_execution_job(), priority);
		});

		PvfExecKind::iter().for_each(|priority| {
			let queue = unscheduled.unscheduled.get(&priority).unwrap();
			assert_eq!(queue.len(), 1);
		});
	}

	#[test]
	fn test_unscheduled_priority_distribution() {
		use PvfExecKind::*;

		let mut priorities = vec![];

		let mut unscheduled = Unscheduled::new();
		for _ in 0..Unscheduled::SCHEDULING_WINDOW_SIZE {
			unscheduled.add(create_execution_job(), Dispute);
			unscheduled.add(create_execution_job(), Approval);
			unscheduled.add(create_execution_job(), BackingSystemParas);
			unscheduled.add(create_execution_job(), Backing);
		}

		for _ in 0..Unscheduled::SCHEDULING_WINDOW_SIZE {
			let priority = unscheduled.select_next_priority();
			priorities.push(priority);
			unscheduled.mark_scheduled(priority);
		}

		assert_eq!(priorities.iter().filter(|v| **v == Dispute).count(), 8);
		assert_eq!(priorities.iter().filter(|v| **v == Approval).count(), 3);
		assert_eq!(priorities.iter().filter(|v| **v == BackingSystemParas).count(), 1);
	}

	#[test]
	fn test_unscheduled_priority_distribution_without_backing_system_paras() {
		use PvfExecKind::*;

		let mut priorities = vec![];

		let mut unscheduled = Unscheduled::new();
		for _ in 0..Unscheduled::SCHEDULING_WINDOW_SIZE {
			unscheduled.add(create_execution_job(), Dispute);
			unscheduled.add(create_execution_job(), Approval);
			unscheduled.add(create_execution_job(), Backing);
		}

		for _ in 0..Unscheduled::SCHEDULING_WINDOW_SIZE {
			let priority = unscheduled.select_next_priority();
			priorities.push(priority);
			unscheduled.mark_scheduled(priority);
		}

		assert_eq!(priorities.iter().filter(|v| **v == Dispute).count(), 8);
		assert_eq!(priorities.iter().filter(|v| **v == Approval).count(), 3);
		assert_eq!(priorities.iter().filter(|v| **v == Backing).count(), 1);
	}

	#[test]
	fn test_unscheduled_priority_distribution_without_disputes() {
		use PvfExecKind::*;

		let mut priorities = vec![];

		let mut unscheduled = Unscheduled::new();
		for _ in 0..Unscheduled::SCHEDULING_WINDOW_SIZE {
			unscheduled.add(create_execution_job(), Approval);
			unscheduled.add(create_execution_job(), BackingSystemParas);
			unscheduled.add(create_execution_job(), Backing);
		}

		for _ in 0..Unscheduled::SCHEDULING_WINDOW_SIZE {
			let priority = unscheduled.select_next_priority();
			priorities.push(priority);
			unscheduled.mark_scheduled(priority);
		}

		assert_eq!(priorities.iter().filter(|v| **v == Approval).count(), 9);
		assert_eq!(priorities.iter().filter(|v| **v == BackingSystemParas).count(), 2);
		assert_eq!(priorities.iter().filter(|v| **v == Backing).count(), 1);
	}

	#[test]
	fn test_unscheduled_priority_distribution_without_disputes_and_only_one_backing() {
		use PvfExecKind::*;

		let mut priorities = vec![];

		let mut unscheduled = Unscheduled::new();
		for _ in 0..Unscheduled::SCHEDULING_WINDOW_SIZE {
			unscheduled.add(create_execution_job(), Approval);
		}
		unscheduled.add(create_execution_job(), Backing);

		for _ in 0..Unscheduled::SCHEDULING_WINDOW_SIZE {
			let priority = unscheduled.select_next_priority();
			priorities.push(priority);
			unscheduled.mark_scheduled(priority);
		}

		assert_eq!(priorities.iter().filter(|v| **v == Approval).count(), 11);
		assert_eq!(priorities.iter().filter(|v| **v == Backing).count(), 1);
	}

	#[test]
	fn test_unscheduled_does_not_postpone_backing() {
		use PvfExecKind::*;

		let mut priorities = vec![];

		let mut unscheduled = Unscheduled::new();
		for _ in 0..Unscheduled::SCHEDULING_WINDOW_SIZE {
			unscheduled.add(create_execution_job(), Approval);
		}
		unscheduled.add(create_execution_job(), Backing);

		for _ in 0..Unscheduled::SCHEDULING_WINDOW_SIZE {
			let priority = unscheduled.select_next_priority();
			priorities.push(priority);
			unscheduled.mark_scheduled(priority);
		}

		assert_eq!(&priorities[..4], &[Approval, Backing, Approval, Approval]);
	}
}
