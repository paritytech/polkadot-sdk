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

//! This module defines worker/worker pool abstractions and also provides a
//! worker pool implementation.
#![allow(unused)]

use async_trait::async_trait;
use bounded_collections::Get;
use futures::{
	future::{join_all, FutureExt},
	stream::{Stream, StreamExt},
};
use primitive_types::H256;
use std::{
	collections::HashMap,
	fmt::Debug,
	future::Future,
	hash::Hash,
	pin::Pin,
	task::{Context, Poll},
};
use tokio::sync::mpsc::{self, Receiver, Sender};

pub(crate) const LOG_TARGET: &str = "parachain::worker-pool";

/// The maximum amount of unprocessed worker messages.
pub const MAX_WORKER_MESSAGES: usize = 16384;
/// The maximum amount of workers that a pool can have.
pub const MAX_WORKERS: usize = 16;

/// The maximum amount of unprocessed `WorkerPoolHandler` messages.
pub const MAX_WORKER_POOL_MESSAGES: usize = MAX_WORKER_MESSAGES;

/// Unique identifier for a worker job.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct JobId(pub H256);

/// A trait to map a work item to a specific `Job``
///
/// Should be implemented by work items, such that a work item always uniquely identifies a single
/// Job.
///
/// The unique identifier is required to route all messages to the same worker. If it is not
/// specified the work item is broadcasted to all workers.
pub trait Job {
	/// Returns the associated context identifier, if any.
	fn id(&self) -> Option<JobId> {
		None
	}
}

// Blanket implementation of `Job`.
impl<T> Job for Option<T> where T: Job {}

/// An abstract worker configuration and spawning interface.
pub trait WorkerConfig: Sized + 'static {
	/// The type used to describe the work to be done.
	type WorkItem: Job + Send + Sync + Clone + Debug;
	/// The type that defines the job initial state
	type JobState: Clone + Debug + Send;
	/// A type implementing the Worker handler.
	type Worker: WorkerHandle + Sync;
	/// A type for channel capacity.
	type ChannelCapacity: Get<u32>;
	/// A type for number of workers.
	type PoolCapacity: Get<u32>;

	/// Spawn a worker and return a `WorkerHandle` to it.
	fn new_worker(&mut self) -> Self::Worker;

	/// Helper for creating a channel from the pool main loop to a worker based on current
	/// configuration.
	// TODO: Priority channel is required to enable workers to be responsive for some messages.
	fn new_worker_channel() -> (Sender<WorkerMessage<Self>>, Receiver<WorkerMessage<Self>>) {
		let max_workers = std::cmp::min(MAX_WORKERS, Self::PoolCapacity::get() as usize);
		let worker_channel_capacity =
			std::cmp::min(MAX_WORKER_MESSAGES / max_workers, Self::ChannelCapacity::get() as usize);

		mpsc::channel::<WorkerMessage<Self>>(worker_channel_capacity)
	}

	/// Helper for creating a channel from worker pool handlers to pool main loop based on current
	/// configuration.
	fn new_pool_channel() -> (Sender<WorkerPoolMessage<Self>>, Receiver<WorkerPoolMessage<Self>>) {
		let pool_channel_capacity = std::cmp::min(
			MAX_WORKER_POOL_MESSAGES,
			(Self::ChannelCapacity::get() * Self::PoolCapacity::get()) as usize,
		);

		mpsc::channel::<WorkerPoolMessage<Self>>(pool_channel_capacity)
	}
}

#[async_trait]
/// An interface to control an abstract worker.
pub trait WorkerHandle: Send + Clone {
	/// The type describing the worker configuration
	type Config: WorkerConfig;

	/// Create a new job with the specified initial `state`.
	async fn new_job(&self, job_id: JobId, state: <Self::Config as WorkerConfig>::JobState) {
		self.send(WorkerMessage::NewJob(job_id, state)).await;
	}

	/// Push some work to the worker.
	async fn queue_work(&self, item: <Self::Config as WorkerConfig>::WorkItem) {
		self.send(WorkerMessage::Queue(item)).await;
	}

	/// Delete jobs across all workers.
	async fn delete_jobs(&self, jobs: &[JobId]) {
		self.send(WorkerMessage::DeleteJobs(jobs.into())).await;
	}

	/// Send a message to the worker.
	async fn send(&self, message: WorkerMessage<Self::Config>);

	/// Returns the worker index.
	fn index(&self) -> u16;
}

/// Messages sent by the pool to individual workers.
#[derive(Debug)]
pub enum WorkerMessage<Config: WorkerConfig> {
	/// Start a new job on the worker initializing it with the given state
	NewJob(JobId, Config::JobState),
	/// New work item.
	Queue(Config::WorkItem),
	/// The above, combined in a batched variant.
	Batch(Vec<Option<Config::JobState>>, Vec<Config::WorkItem>),
	/// Delete a batch of jobs.
	/// The corresponding `WorkerPool::job_per_worker` entries are already removed
	/// when the message is received.
	DeleteJobs(Vec<JobId>),
}

/// Messages sent by `WorkerPoolHandler` to the event loop of `WorkerPool`.
#[derive(Clone)]
pub enum WorkerPoolMessage<Config: WorkerConfig> {
	/// Create a new job.
	NewJob(
		JobId,
		<<<Config as WorkerConfig>::Worker as WorkerHandle>::Config as WorkerConfig>::JobState,
	),
	/// Send new work to the pool.
	Queue(<<<Config as WorkerConfig>::Worker as WorkerHandle>::Config as WorkerConfig>::WorkItem),
	/// Prune work items,
	DeleteJobs(Vec<JobId>),
}

pub struct WorkerPool<Config: WorkerConfig> {
	// Per worker context mapping. Values are indices in `worker_handles`.
	job_per_worker: HashMap<JobId, usize>,
	// Per worker handles
	worker_handles: Vec<Config::Worker>,
	// Next worker index.
	next_worker: usize,
	// Receive messages from `WorkerPoolHandlers`
	from_handlers: mpsc::Receiver<WorkerPoolMessage<Config>>,
}

impl<Config: WorkerConfig> Unpin for WorkerPool<Config> {}

impl<Config: WorkerConfig> Stream for WorkerPool<Config> {
	type Item = WorkerPoolMessage<Config>;

	fn poll_next(mut self: Pin<&mut Self>, ctx: &mut Context) -> Poll<Option<Self::Item>> {
		match Pin::new(&mut self.from_handlers).poll_recv(ctx) {
			Poll::Ready(maybe_message) => Poll::Ready(maybe_message),
			Poll::Pending => Poll::Pending,
		}
	}
}

impl<Config: WorkerConfig> Future for WorkerPool<Config> {
	type Output = WorkerPoolMessage<Config>;

	fn poll(mut self: Pin<&mut Self>, ctx: &mut Context<'_>) -> Poll<Self::Output> {
		match Pin::new(&mut self.from_handlers).poll_recv(ctx) {
			Poll::Ready(Some(message)) => Poll::Ready(message),
			Poll::Ready(None) => Poll::Pending,
			Poll::Pending => Poll::Pending,
		}
	}
}

/// A `WorkerPool` handle that can be used across threads.
/// Allows us to create processing pipelines as a DAG, where nodes are instances of `WorkerPool` of
/// arbitrary types and capacities.
#[derive(Clone)]
pub struct WorkerPoolHandler<Config: WorkerConfig> {
	to_pool: mpsc::Sender<WorkerPoolMessage<Config>>,
}

impl<Config: WorkerConfig> WorkerPoolHandler<Config> {
	/// Dispatch a `WorkItem` to the appropriate worker.
	pub async fn queue_work(
		&mut self,
		work_item: <<<Config as WorkerConfig>::Worker as WorkerHandle>::Config as WorkerConfig>::WorkItem,
	) {
		if let Err(e) = self.to_pool.send(WorkerPoolMessage::Queue(work_item)).await {
			gum::warn!(target: LOG_TARGET, err = ?e, "Unable to send `WorkerPoolMessage::Queue`")
		}
	}
	/// Setup a new job
	pub async fn new_job(
		&mut self,
		job_id: JobId,
		state: <<<Config as WorkerConfig>::Worker as WorkerHandle>::Config as WorkerConfig>::JobState,
	) {
		if let Err(e) = self.to_pool.send(WorkerPoolMessage::NewJob(job_id, state)).await {
			gum::warn!(target: LOG_TARGET, err = ?e, "Unable to send `WorkerPoolMessage::Queue`")
		}
	}

	/// Notify workers that the specified `Jobs` will not receive any more work and any
	/// state relevant should be pruned.
	pub async fn delete_job(&self, jobs: &[JobId]) {
		if let Err(e) = self.to_pool.send(WorkerPoolMessage::DeleteJobs(Vec::from(jobs))).await {
			gum::warn!(target: LOG_TARGET, err = ?e, "Unable to send `WorkerPoolMessage::DeleteJobs`")
		}
	}
}

impl<Config: WorkerConfig + Sized> WorkerPool<Config> {
	/// Create with specified worker builder.
	pub fn with_config(config: &mut Config) -> (Self, WorkerPoolHandler<Config>) {
		let job_per_worker = HashMap::new();

		let max_workers = std::cmp::min(MAX_WORKERS, Config::PoolCapacity::get() as usize);

		let worker_handles =
			(0..max_workers).into_iter().map(|_| config.new_worker()).collect::<Vec<_>>();

		let (to_pool, from_handlers) = <Config as WorkerConfig>::new_pool_channel();
		(
			WorkerPool { job_per_worker, worker_handles, next_worker: 0, from_handlers },
			WorkerPoolHandler { to_pool },
		)
	}

	/// Returns true if a job already exists.
	pub fn job_exists(&self, job_id: &JobId) -> bool {
		self.job_per_worker.contains_key(job_id)
	}

	/// Returns an iterator over worker handles.
	pub fn worker_handles(&self) -> &[<Config as WorkerConfig>::Worker] {
		&self.worker_handles
	}

	/// Prune specified jobs and notify workers.
	pub async fn delete_jobs(&mut self, jobs: Vec<JobId>) {
		// We need to split the contexts per worker.
		let mut prunable_per_worker_jobs = vec![Vec::new(); self.worker_handles.len()];
		let num_deleted = jobs.len();
		for job in jobs {
			if let Some(worker_index) = self.job_per_worker.get(&job) {
				prunable_per_worker_jobs
					.get_mut(*worker_index)
					.expect("just created above; qed")
					.push(job);
			}
		}

		for (index, jobs) in prunable_per_worker_jobs.into_iter().enumerate() {
			self.worker_handles[index].delete_jobs(&jobs).await;
		}

		gum::debug!(target: LOG_TARGET, num_total_jobs = self.job_per_worker.len(), num_deleted, "worker-pool: delete_jobs");
	}

	/// Removes completed jobs
	pub async fn complete_jobs(&mut self, jobs: &[JobId]) {
		for job in jobs {
			self.job_per_worker.remove(&job);
		}
		gum::debug!(target: LOG_TARGET, num_total_jobs = self.job_per_worker.len(), num_deleted = ?jobs.len(), "worker-pool: complete_jobs");
	}

	/// Create or update a job with the given state.
	pub async fn new_job(
		&mut self,
		job_id: JobId,
		state: <<<Config as WorkerConfig>::Worker as WorkerHandle>::Config as WorkerConfig>::JobState,
	) {
		if let Some(worker_handle) = self.find_worker_for_job(&job_id) {
			worker_handle.new_job(job_id, state).await;
		} else {
			// The work requires a new `Job`` and `self.next_worker` should be suitable.
			//
			// TODO: If needed we might want to define more methods to choose a worker if
			// `Job` can provide additional information. TODO: Handle blocking due to queue
			// being full. We want to avoid that, knowing the channel len would provide a better
			// view of the current load of a worker.
			let worker_handle = self.rr_any_worker();
			gum::trace!(target: LOG_TARGET, ?job_id, worker_idx = ?worker_handle.index(), "Creating new job on worker");

			// Dispatch work item to selected worker.
			worker_handle.new_job(job_id.clone(), state).await;

			// Map context to worker.
			self.job_per_worker.insert(job_id, self.next_worker);
			self.next_worker = (self.next_worker + 1) % self.worker_handles.len();
		}
	}
	/// Queue new `WorkItem` to the pool
	///
	/// `work_item` is sent to all workers if it doesn't belong to any job ( `work_item.id()` is
	/// None).
	pub async fn queue_work(
		&mut self,
		work_item: <<<Config as WorkerConfig>::Worker as WorkerHandle>::Config as WorkerConfig>::WorkItem,
	) {
		let job_id = if let Some(job_id) = work_item.id() {
			job_id
		} else {
			// Work items not associated top a specific `Job`` are broadcasted to all workers.
			let broadcast_futures = self
				.worker_handles
				.iter()
				.map(|worker_handle| worker_handle.queue_work(work_item.clone()))
				.collect::<Vec<_>>();
			join_all(broadcast_futures).await;
			return
		};

		if let Some(worker_handle) = self.find_worker_for_job(&job_id) {
			worker_handle.queue_work(work_item).await;
		} else {
			gum::error!(target: LOG_TARGET, ?job_id, "`work_item` associated to job, but job doesn't exist. Ensure `new_job()` is called first.");
		}
	}

	// Returns a worker that is mapped to the specified `job_id`.
	fn find_worker_for_job(&self, job_id: &JobId) -> Option<&Config::Worker> {
		let worker_handles = self.worker_handles.as_slice();
		self.job_per_worker.get(&job_id).map(|worker_index| {
			worker_handles.get(*worker_index).expect("worker_index is always valid in here")
		})
	}

	// Round robin worker choosing.
	fn rr_any_worker(&self) -> &Config::Worker {
		&self.worker_handles[self.next_worker]
	}

	// Default main loop implementation
	fn run_main_loop(mut self) {
		let worker_loop = async move {
			loop {
				if let Some(worker_message) = self.next().await {
					match worker_message {
						WorkerPoolMessage::NewJob(job_id, state) => {
							self.new_job(job_id, state).await;
						},
						WorkerPoolMessage::Queue(work_item) => {
							self.queue_work(work_item).await;
						},
						WorkerPoolMessage::DeleteJobs(jobs) => {
							self.delete_jobs(jobs.into()).await;
						},
					}
				} else {
					// channel closed, end worker.
					break
				}
			}
		}
		.boxed();

		tokio::spawn(worker_loop);
	}
}

mod test {
	// Test mockups to cover the thin layer of logic in generic code.
	// TODO:
	// A worker that counts work items and doesnt do any work.
	// TODO:
	#[test]
	fn test_construction() {}
}
