use async_trait::async_trait;
use bounded_collections::Get;
use futures::{
	future::{join_all, FutureExt},
	stream::{Stream, StreamExt},
};
use gum::warn;
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

/// Unique identifier for a worker context.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct ContextCookie(pub H256);

/// A generic opaque worker context identifier.
///
/// Should be implemented by work items, such that a work item always uniquely identifies a single
/// context.
///
/// The unique identifier is required to route all messages to the same worker. If it is not
/// specified the work item is broadcasted to all workers.
pub trait WorkContext {
	fn id(&self) -> Option<ContextCookie> {
		None
	}
}

// Blanket implementation of `WorkContext`.
impl<T> WorkContext for Option<T> where T: WorkContext {}

/// An abstract worker configuration and spawning interface.
pub trait WorkerConfig: Sized + 'static {
	// The type used to describe the work to be done.
	type WorkItem: WorkContext + Send + Sync + Clone + Debug;
	// // The type that defines a context in which WorkItems are processed
	type Context: Clone + Debug + Send;
	// A type implementing the Worker handler.
	type Worker: WorkerHandle + Sync;
	// A type for channel capacity.
	type ChannelCapacity: Get<u32>;
	// A type for number of workers.
	type PoolCapacity: Get<u32>;

	// Spawn a worker and return a `WorkerHandle` to it.
	fn new_worker(&mut self) -> Self::Worker;

	// Helper for creating a channel from the pool main loop to a worker based on current
	// configuration.
	fn new_worker_channel() -> (Sender<WorkerMessage<Self>>, Receiver<WorkerMessage<Self>>) {
		let max_workers = std::cmp::min(MAX_WORKERS, Self::PoolCapacity::get() as usize);
		let worker_channel_capacity =
			std::cmp::min(MAX_WORKER_MESSAGES / max_workers, Self::ChannelCapacity::get() as usize);

		mpsc::channel::<WorkerMessage<Self>>(worker_channel_capacity)
	}

	// Helper for creating a channel from worker pool handlers to pool main loop based on current
	// configuration.
	fn new_pool_channel() -> (Sender<WorkerPoolMessage<Self>>, Receiver<WorkerPoolMessage<Self>>) {
		let pool_channel_capacity = std::cmp::min(
			MAX_WORKER_POOL_MESSAGES,
			(Self::ChannelCapacity::get() * Self::PoolCapacity::get()) as usize,
		);

		mpsc::channel::<WorkerPoolMessage<Self>>(pool_channel_capacity)
	}
}

#[async_trait]
/// An interface to control an abstract contextual worker.
pub trait WorkerHandle: Send + Clone {
	type Config: WorkerConfig;

	// Push a context update to the worker. Usually this is a new context.
	async fn setup_context(&self, context: <Self::Config as WorkerConfig>::Context) {
		self.send(WorkerMessage::SetupContext(context)).await;
	}

	// Push some work to the worker.
	async fn queue_work(&self, item: <Self::Config as WorkerConfig>::WorkItem) {
		self.send(WorkerMessage::Queue(item)).await;
	}

	// Prune all work belonging to the specified `contexts`.
	async fn prune_work(&self, contexts: &[ContextCookie]) {
		self.send(WorkerMessage::PruneWork(contexts.into())).await;
	}

	// Only this is required to be implemented
	async fn send(&self, message: WorkerMessage<Self::Config>);
}

/// Messages sent by the pool to individual workers.
#[derive(Debug)]
pub enum WorkerMessage<Config: WorkerConfig> {
	/// Mandatory: A new `Context` for processing work.
	SetupContext(Config::Context),
	/// Mandatory: New work item.
	Queue(Config::WorkItem),
	/// Mandatory: The above, combined in a batched variant.
	Batch(Vec<Option<Config::Context>>, Vec<Config::WorkItem>),
	/// Mandatory: Prune different `Context` instances on the worker.
	/// The corresponding `WorkerPool::context_per_worker` entries are already removed
	/// when the message is received.
	PruneWork(Vec<ContextCookie>),
}

/// Messages sent by `WorkerPoolHandler` to the event loop of `WorkerPool`.
#[derive(Clone)]
pub enum WorkerPoolMessage<Config: WorkerConfig> {
	/// Send new work to the pool.
	/// TODO: pass a context closure here.
	Queue(
		<<<Config as WorkerConfig>::Worker as WorkerHandle>::Config as WorkerConfig>::WorkItem,
		Option<
			<<<Config as WorkerConfig>::Worker as WorkerHandle>::Config as WorkerConfig>::Context,
		>,
	),
	/// Prune work items,
	PruneWork(Vec<ContextCookie>),
}

/// A generic worker pool implementation.
/// TODO: multilevel pool. We need second level for the database worker usecase.
/// TODO: Alternatively, support connectors between pools, such that workers can forward or
/// send arbitrary messages to a receiver outside of the pool. This would basically allow
/// chaining of different types of worker pools.
///
/// One easy way to implement might be providing a `WorkerPoolHandle` that can be passed around
/// to different threads that can send work to the pool. The handle would expose much of the
/// `WorkerPool` object, but would actually work over a channel.
///
/// To support this, the `WorkerPool` will need to run it's own event loop to wait for and
/// process messages.
/// Implementing `Future` for the `WorkerPool` to provide this functionality should do it.
///
/// Additionally, a simple interface to expose `capacity` and `blocking` workers is needed to
/// fully support a new way of wrapping core subsystem logic without any side effects.
/// There could be different types of workers, even PVF execution can be wrapped in this framework
/// such that we could build a DAG where each node is a worker pool capable of an arbitray number of
/// threads that process information in paralel. Each pool forward messages to another pool, where
/// the last element in the chain is the final output.
///
/// For `approval-distribution` this means two workerpools (ApprovalPool, DBPool), ApprovalPool for
/// processing messages and DBPool made of just a single `db` worker that commits the output of
/// ApprovalPool to DBPool.
///
/// High level, it should be something like this:
///
/// [network event] -> `approval-main-loop` + DbRead -> ApprovalPool/WorkerN ->
/// DbPool/Worker0/DbWrite
///
///
/// Even more enhancements:
/// - support for batched sending in `WorkerPool`; we define a max buffer size in which we
///   accumulate
/// `WorkItems` and `Contexts` (if needed) for each of them per individual worker. Once a threshold
/// is hit, or a timeout expires, the buffer contents are sent as a batch. Contexts are a concern of
/// the batching mechanism, and should always be available when the batch hits the worker, otherwise
/// the worker won't be able to process it.
pub struct WorkerPool<Config: WorkerConfig> {
	// Per worker context mapping. Values are indices in `worker_handles`.
	context_per_worker: HashMap<ContextCookie, usize>,
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
	pub async fn queue_work(
		&mut self,
		work_item: <<<Config as WorkerConfig>::Worker as WorkerHandle>::Config as WorkerConfig>::WorkItem,
		context: Option<<<<Config as WorkerConfig>::Worker as WorkerHandle>::Config as WorkerConfig>::Context>,
	) {
		if let Err(e) = self.to_pool.send(WorkerPoolMessage::Queue(work_item, context)).await {
			gum::warn!(target: LOG_TARGET, err = ?e, "Unable to send `WorkerPoolMessage::Queue`")
		}
	}

	pub async fn prune_work(&self, contexts: &[ContextCookie]) {
		if let Err(e) = self.to_pool.send(WorkerPoolMessage::PruneWork(Vec::from(contexts))).await {
			gum::warn!(target: LOG_TARGET, err = ?e, "Unable to send `WorkerPoolMessage::PruneWork`")
		}
	}
}

impl<Config: WorkerConfig + Sized> WorkerPool<Config> {
	// Create with specified worker builder.
	pub fn with_config(config: &mut Config) -> (Self, WorkerPoolHandler<Config>) {
		let context_per_worker = HashMap::new();

		let max_workers = std::cmp::min(MAX_WORKERS, Config::PoolCapacity::get() as usize);

		let worker_handles =
			(0..max_workers).into_iter().map(|_| config.new_worker()).collect::<Vec<_>>();

		let (to_pool, from_handlers) = <Config as WorkerConfig>::new_pool_channel();
		(
			WorkerPool { context_per_worker, worker_handles, next_worker: 0, from_handlers },
			WorkerPoolHandler { to_pool },
		)
	}

	// Returns an iterator over worker handles.
	pub fn worker_handles(&self) -> &[<Config as WorkerConfig>::Worker] {
		&self.worker_handles
	}

	/// Prune per context cache and notify workers to prune contexts.
	pub async fn prune_work(&mut self, contexts: Vec<ContextCookie>) {
		// We need to split the contexts per worker.
		let mut prunable_per_worker_contexts = vec![Vec::new(); self.worker_handles.len()];
		for context in contexts {
			if let Some(worker_index) = self.context_per_worker.get(&context) {
				prunable_per_worker_contexts
					.get_mut(*worker_index)
					.expect("just created above; qed")
					.push(context);
			}
		}

		for (index, contexts) in prunable_per_worker_contexts.into_iter().enumerate() {
			self.worker_handles[index].prune_work(&contexts).await;
		}
	}

	/// Queue new `WorkItem` to the pool.
	///
	/// `context_fn` is a closure that can provide the `Context` for work not belonging to any
	/// context yet.
	pub async fn queue_work(
		&mut self,
		work_item: <<<Config as WorkerConfig>::Worker as WorkerHandle>::Config as WorkerConfig>::WorkItem,
		context: Option<
			<<<Config as WorkerConfig>::Worker as WorkerHandle>::Config as WorkerConfig>::Context,
		>,
	) {
		let context_id = if let Some(context_id) = work_item.id() {
			context_id
		} else {
			// Work items without context are broadcasted to all workers.
			let broadcast_futures = self
				.worker_handles
				.iter()
				.map(|worker_handle| worker_handle.queue_work(work_item.clone()))
				.collect::<Vec<_>>();
			join_all(broadcast_futures).await;
			return
		};

		if let Some(worker_handle) = self.find_worker_for_context(&context_id) {
			worker_handle.queue_work(work_item).await;
		} else {
			// The work requires a new context and `self.next_worker` should be suitable.
			// TODO: If needed we might want to define more methods to choose a worker if
			// `WorkContext` can provide additional information. TODO: Handle blocking due to queue
			// being full. We want to avoid that, knowing the channel len would provide a better
			// view of the current load of a worker.
			let worker_handle = self.rr_any_worker();
			if let Some(context) = context {
				// Update the context.
				worker_handle.setup_context(context).await;
			} else {
				warn!("No context provided for work item.");
			}

			// Dispatch work item to selected worker.
			worker_handle.queue_work(work_item).await;

			// Map context to worker.
			self.context_per_worker.insert(context_id, self.next_worker);
			self.next_worker = (self.next_worker + 1) % self.worker_handles.len();
		}
	}

	// Returns a worker that is mapped to the specified `context_cookie`.
	fn find_worker_for_context(&self, context_cookie: &ContextCookie) -> Option<&Config::Worker> {
		let worker_handles = self.worker_handles.as_slice();
		self.context_per_worker.get(&context_cookie).map(|worker_index| {
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
						WorkerPoolMessage::Queue(work_item, context) => {
							self.queue_work(work_item, context).await;
						},
						WorkerPoolMessage::PruneWork(contexts) => {
							gum::debug!(target: LOG_TARGET, "WorkerPool received contexts to be pruned, {:?}", &contexts);
							self.prune_work(contexts.into()).await;
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
