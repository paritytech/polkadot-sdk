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

//! The Approval Voting Parallel Subsystem.
//!
//! This subsystem is responsible for orchestrating the work done by
//! approval-voting and approval-distribution subsystem, so they can
//! do their work in parallel, rather than serially, when they are run
//! as independent subsystems.
use itertools::Itertools;
use metrics::{Meters, MetricsWatcher};
use polkadot_node_core_approval_voting::{Config, RealAssignmentCriteria};
use polkadot_node_metrics::metered::{
	self, channel, unbounded, MeteredReceiver, MeteredSender, UnboundedMeteredReceiver,
	UnboundedMeteredSender,
};

use polkadot_node_primitives::{
	approval::time::{Clock, SystemClock},
	DISPUTE_WINDOW,
};
use polkadot_node_subsystem::{
	messages::{ApprovalDistributionMessage, ApprovalVotingMessage, ApprovalVotingParallelMessage},
	overseer, FromOrchestra, SpawnedSubsystem, SubsystemError, SubsystemResult,
};
use polkadot_node_subsystem_util::{
	self,
	database::Database,
	runtime::{Config as RuntimeInfoConfig, RuntimeInfo},
};
use polkadot_overseer::{OverseerSignal, Priority, SubsystemSender, TimeoutExt};
use polkadot_primitives::{CandidateIndex, Hash, ValidatorIndex, ValidatorSignature};
use rand::SeedableRng;

use sc_keystore::LocalKeystore;
use sp_consensus::SyncOracle;

use futures::{channel::oneshot, prelude::*, StreamExt};
pub use metrics::Metrics;
use polkadot_node_core_approval_voting::{
	approval_db::common::Config as DatabaseConfig, ApprovalVotingWorkProvider,
};
use std::{
	collections::{HashMap, HashSet},
	fmt::Debug,
	sync::Arc,
	time::Duration,
};
use stream::{select_with_strategy, PollNext, SelectWithStrategy};
pub mod metrics;

#[cfg(test)]
mod tests;

pub(crate) const LOG_TARGET: &str = "parachain::approval-voting-parallel";
// Value rather arbitrarily: Should not be hit in practice, it exists to more easily diagnose dead
// lock issues for example.
const WAIT_FOR_SIGS_GATHER_TIMEOUT: Duration = Duration::from_millis(2000);

/// The number of workers used for running the approval-distribution logic.
pub const APPROVAL_DISTRIBUTION_WORKER_COUNT: usize = 4;

/// The default channel size for the workers, can be overridden by the user through
/// `overseer_channel_capacity_override`
pub const DEFAULT_WORKERS_CHANNEL_SIZE: usize = 64000 / APPROVAL_DISTRIBUTION_WORKER_COUNT;

fn prio_right<'a>(_val: &'a mut ()) -> PollNext {
	PollNext::Right
}

/// The approval voting parallel subsystem.
pub struct ApprovalVotingParallelSubsystem {
	/// `LocalKeystore` is needed for assignment keys, but not necessarily approval keys.
	///
	/// We do a lot of VRF signing and need the keys to have low latency.
	keystore: Arc<LocalKeystore>,
	db_config: DatabaseConfig,
	slot_duration_millis: u64,
	db: Arc<dyn Database>,
	sync_oracle: Box<dyn SyncOracle + Send>,
	metrics: Metrics,
	spawner: Arc<dyn overseer::gen::Spawner + 'static>,
	clock: Arc<dyn Clock + Send + Sync>,
	overseer_message_channel_capacity_override: Option<usize>,
}

impl ApprovalVotingParallelSubsystem {
	/// Create a new approval voting subsystem with the given keystore, config, and database.
	pub fn with_config(
		config: Config,
		db: Arc<dyn Database>,
		keystore: Arc<LocalKeystore>,
		sync_oracle: Box<dyn SyncOracle + Send>,
		metrics: Metrics,
		spawner: impl overseer::gen::Spawner + 'static + Clone,
		overseer_message_channel_capacity_override: Option<usize>,
	) -> Self {
		ApprovalVotingParallelSubsystem::with_config_and_clock(
			config,
			db,
			keystore,
			sync_oracle,
			metrics,
			Arc::new(SystemClock {}),
			spawner,
			overseer_message_channel_capacity_override,
		)
	}

	/// Create a new approval voting subsystem with the given keystore, config, clock, and database.
	pub fn with_config_and_clock(
		config: Config,
		db: Arc<dyn Database>,
		keystore: Arc<LocalKeystore>,
		sync_oracle: Box<dyn SyncOracle + Send>,
		metrics: Metrics,
		clock: Arc<dyn Clock + Send + Sync>,
		spawner: impl overseer::gen::Spawner + 'static,
		overseer_message_channel_capacity_override: Option<usize>,
	) -> Self {
		ApprovalVotingParallelSubsystem {
			keystore,
			slot_duration_millis: config.slot_duration_millis,
			db,
			db_config: DatabaseConfig { col_approval_data: config.col_approval_data },
			sync_oracle,
			metrics,
			spawner: Arc::new(spawner),
			clock,
			overseer_message_channel_capacity_override,
		}
	}

	/// The size of the channel used for the workers.
	fn workers_channel_size(&self) -> usize {
		self.overseer_message_channel_capacity_override
			.unwrap_or(DEFAULT_WORKERS_CHANNEL_SIZE)
	}
}

#[overseer::subsystem(ApprovalVotingParallel, error = SubsystemError, prefix = self::overseer)]
impl<Context: Send> ApprovalVotingParallelSubsystem {
	fn start(self, ctx: Context) -> SpawnedSubsystem {
		let future = run::<Context>(ctx, self)
			.map_err(|e| SubsystemError::with_origin("approval-voting-parallel", e))
			.boxed();

		SpawnedSubsystem { name: "approval-voting-parallel-subsystem", future }
	}
}

// It starts worker for the approval voting subsystem and the `APPROVAL_DISTRIBUTION_WORKER_COUNT`
// workers for the approval distribution subsystem.
//
// It returns handles that can be used to send messages to the workers.
#[overseer::contextbounds(ApprovalVotingParallel, prefix = self::overseer)]
async fn start_workers<Context>(
	ctx: &mut Context,
	subsystem: ApprovalVotingParallelSubsystem,
	metrics_watcher: &mut MetricsWatcher,
) -> SubsystemResult<(ToWorker<ApprovalVotingMessage>, Vec<ToWorker<ApprovalDistributionMessage>>)>
where
{
	gum::info!(target: LOG_TARGET, "Starting approval distribution workers");

	// Build approval voting handles.
	let (to_approval_voting_worker, approval_voting_work_provider) = build_worker_handles(
		"approval-voting-parallel-db".into(),
		subsystem.workers_channel_size(),
		metrics_watcher,
		prio_right,
	);
	let mut to_approval_distribution_workers = Vec::new();
	let slot_duration_millis = subsystem.slot_duration_millis;

	for i in 0..APPROVAL_DISTRIBUTION_WORKER_COUNT {
		let mut network_sender = ctx.sender().clone();
		let mut runtime_api_sender = ctx.sender().clone();
		let mut approval_distribution_to_approval_voting = to_approval_voting_worker.clone();

		let approval_distr_instance =
			polkadot_approval_distribution::ApprovalDistribution::new_with_clock(
				subsystem.metrics.approval_distribution_metrics(),
				subsystem.slot_duration_millis,
				subsystem.clock.clone(),
				Arc::new(RealAssignmentCriteria {}),
			);
		let task_name = format!("approval-voting-parallel-{}", i);
		let (to_approval_distribution_worker, mut approval_distribution_work_provider) =
			build_worker_handles(
				task_name.clone(),
				subsystem.workers_channel_size(),
				metrics_watcher,
				prio_right,
			);

		metrics_watcher.watch(task_name.clone(), to_approval_distribution_worker.meter());

		subsystem.spawner.spawn_blocking(
			task_name.leak(),
			Some("approval-voting-parallel"),
			Box::pin(async move {
				let mut state =
					polkadot_approval_distribution::State::with_config(slot_duration_millis);
				let mut rng = rand::rngs::StdRng::from_entropy();
				let mut session_info_provider = RuntimeInfo::new_with_config(RuntimeInfoConfig {
					keystore: None,
					session_cache_lru_size: DISPUTE_WINDOW.get(),
				});

				loop {
					let message = match approval_distribution_work_provider.next().await {
						Some(message) => message,
						None => {
							gum::info!(
								target: LOG_TARGET,
								"Approval distribution stream finished, most likely shutting down",
							);
							break;
						},
					};
					if approval_distr_instance
						.handle_from_orchestra(
							message,
							&mut approval_distribution_to_approval_voting,
							&mut network_sender,
							&mut runtime_api_sender,
							&mut state,
							&mut rng,
							&mut session_info_provider,
						)
						.await
					{
						gum::info!(
							target: LOG_TARGET,
							"Approval distribution worker {}, exiting because of shutdown", i
						);
					};
				}
			}),
		);
		to_approval_distribution_workers.push(to_approval_distribution_worker);
	}

	gum::info!(target: LOG_TARGET, "Starting approval voting workers");

	let sender = ctx.sender().clone();
	let to_approval_distribution = ApprovalVotingToApprovalDistribution(sender.clone());
	polkadot_node_core_approval_voting::start_approval_worker(
		approval_voting_work_provider,
		sender.clone(),
		to_approval_distribution,
		polkadot_node_core_approval_voting::Config {
			slot_duration_millis: subsystem.slot_duration_millis,
			col_approval_data: subsystem.db_config.col_approval_data,
		},
		subsystem.db.clone(),
		subsystem.keystore.clone(),
		subsystem.sync_oracle,
		subsystem.metrics.approval_voting_metrics(),
		subsystem.spawner.clone(),
		"approval-voting-parallel-db",
		"approval-voting-parallel",
		subsystem.clock.clone(),
	)
	.await?;

	Ok((to_approval_voting_worker, to_approval_distribution_workers))
}

// The main run function of the approval parallel voting subsystem.
#[overseer::contextbounds(ApprovalVotingParallel, prefix = self::overseer)]
async fn run<Context>(
	mut ctx: Context,
	subsystem: ApprovalVotingParallelSubsystem,
) -> SubsystemResult<()> {
	let mut metrics_watcher = MetricsWatcher::new(subsystem.metrics.clone());
	gum::info!(
		target: LOG_TARGET,
		"Starting workers"
	);

	let (to_approval_voting_worker, to_approval_distribution_workers) =
		start_workers(&mut ctx, subsystem, &mut metrics_watcher).await?;

	gum::info!(
		target: LOG_TARGET,
		"Starting main subsystem loop"
	);

	run_main_loop(ctx, to_approval_voting_worker, to_approval_distribution_workers, metrics_watcher)
		.await
}

// Main loop of the subsystem, it shouldn't include any logic just dispatching of messages to
// the workers.
//
// It listens for messages from the overseer and dispatches them to the workers.
#[overseer::contextbounds(ApprovalVotingParallel, prefix = self::overseer)]
async fn run_main_loop<Context>(
	mut ctx: Context,
	mut to_approval_voting_worker: ToWorker<ApprovalVotingMessage>,
	mut to_approval_distribution_workers: Vec<ToWorker<ApprovalDistributionMessage>>,
	metrics_watcher: MetricsWatcher,
) -> SubsystemResult<()> {
	loop {
		futures::select! {
			next_msg = ctx.recv().fuse() => {
				let next_msg = match next_msg {
					Ok(msg) => msg,
					Err(err) => {
						gum::info!(target: LOG_TARGET, ?err, "Approval voting parallel subsystem received an error");
						return Err(err);
					}
				};

				match next_msg {
					FromOrchestra::Signal(msg) => {
						if matches!(msg, OverseerSignal::ActiveLeaves(_)) {
							metrics_watcher.collect_metrics();
						}

						for worker in to_approval_distribution_workers.iter_mut() {
							worker
								.send_signal(msg.clone()).await?;
						}

						to_approval_voting_worker.send_signal(msg.clone()).await?;
						if matches!(msg, OverseerSignal::Conclude) {
							break;
						}
					},
					FromOrchestra::Communication { msg } => match msg {
						// The message the approval voting subsystem would've handled.
						ApprovalVotingParallelMessage::ApprovedAncestor(_, _,_) |
						ApprovalVotingParallelMessage::GetApprovalSignaturesForCandidate(_, _)  => {
							to_approval_voting_worker.send_message(
								msg.try_into().expect(
									"Message is one of ApprovedAncestor, GetApprovalSignaturesForCandidate
									 and that can be safely converted to ApprovalVotingMessage; qed"
								)
							).await;
						},
						// Now the message the approval distribution subsystem would've handled and need to
						// be forwarded to the workers.
						ApprovalVotingParallelMessage::NewBlocks(msg) => {
							for worker in to_approval_distribution_workers.iter_mut() {
								worker
									.send_message(
										ApprovalDistributionMessage::NewBlocks(msg.clone()),
									)
									.await;
							}
						},
						ApprovalVotingParallelMessage::DistributeAssignment(assignment, claimed) => {
							let worker = assigned_worker_for_validator(assignment.validator, &mut to_approval_distribution_workers);
							worker
								.send_message(
									ApprovalDistributionMessage::DistributeAssignment(assignment, claimed)
								)
								.await;

						},
						ApprovalVotingParallelMessage::DistributeApproval(vote) => {
							let worker = assigned_worker_for_validator(vote.validator, &mut to_approval_distribution_workers);
							worker
								.send_message(
									ApprovalDistributionMessage::DistributeApproval(vote)
								).await;

						},
						ApprovalVotingParallelMessage::NetworkBridgeUpdate(msg) => {
							if let polkadot_node_subsystem::messages::NetworkBridgeEvent::PeerMessage(
								peer_id,
								msg,
							) = msg
							{
								let (all_msgs_from_same_validator, messages_split_by_validator) = validator_index_for_msg(msg);

								for (validator_index, msg) in all_msgs_from_same_validator.into_iter().chain(messages_split_by_validator.into_iter().flatten()) {
									let worker = assigned_worker_for_validator(validator_index, &mut to_approval_distribution_workers);

									worker
										.send_message(
											ApprovalDistributionMessage::NetworkBridgeUpdate(
												polkadot_node_subsystem::messages::NetworkBridgeEvent::PeerMessage(
													peer_id, msg,
												),
											),
										).await;
								}
							} else {
								for worker in to_approval_distribution_workers.iter_mut() {
									worker
										.send_message_with_priority::<overseer::HighPriority>(
											ApprovalDistributionMessage::NetworkBridgeUpdate(msg.clone()),
										).await;
								}
							}
						},
						ApprovalVotingParallelMessage::GetApprovalSignatures(indices, tx) => {
							handle_get_approval_signatures(&mut ctx, &mut to_approval_distribution_workers, indices, tx).await;
						},
						ApprovalVotingParallelMessage::ApprovalCheckingLagUpdate(lag) => {
							for worker in to_approval_distribution_workers.iter_mut() {
								worker
									.send_message(
										ApprovalDistributionMessage::ApprovalCheckingLagUpdate(lag)
									).await;
							}
						},
					},
				};

			},
		};
	}
	Ok(())
}

// It sends a message to all approval workers to get the approval signatures for the requested
// candidates and then merges them all together and sends them back to the requester.
#[overseer::contextbounds(ApprovalVotingParallel, prefix = self::overseer)]
async fn handle_get_approval_signatures<Context>(
	ctx: &mut Context,
	to_approval_distribution_workers: &mut Vec<ToWorker<ApprovalDistributionMessage>>,
	requested_candidates: HashSet<(Hash, CandidateIndex)>,
	result_channel: oneshot::Sender<
		HashMap<ValidatorIndex, (Hash, Vec<CandidateIndex>, ValidatorSignature)>,
	>,
) {
	let mut sigs = HashMap::new();
	let mut signatures_channels = Vec::new();
	for worker in to_approval_distribution_workers.iter_mut() {
		let (tx, rx) = oneshot::channel();
		worker.send_unbounded_message(ApprovalDistributionMessage::GetApprovalSignatures(
			requested_candidates.clone(),
			tx,
		));
		signatures_channels.push(rx);
	}

	let gather_signatures = async move {
		let Some(results) = futures::future::join_all(signatures_channels)
			.timeout(WAIT_FOR_SIGS_GATHER_TIMEOUT)
			.await
		else {
			gum::warn!(
				target: LOG_TARGET,
				"Waiting for approval signatures timed out - dead lock?"
			);
			return;
		};

		for result in results {
			let worker_sigs = match result {
				Ok(sigs) => sigs,
				Err(_) => {
					gum::error!(
						target: LOG_TARGET,
						"Getting approval signatures failed, oneshot got closed"
					);
					continue;
				},
			};
			sigs.extend(worker_sigs);
		}

		if let Err(_) = result_channel.send(sigs) {
			gum::debug!(
					target: LOG_TARGET,
					"Sending back approval signatures failed, oneshot got closed"
			);
		}
	};

	if let Err(err) = ctx.spawn("approval-voting-gather-signatures", Box::pin(gather_signatures)) {
		gum::warn!(target: LOG_TARGET, "Failed to spawn gather signatures task: {:?}", err);
	}
}

// Returns the worker that should receive the message for the given validator.
fn assigned_worker_for_validator(
	validator: ValidatorIndex,
	to_approval_distribution_workers: &mut Vec<ToWorker<ApprovalDistributionMessage>>,
) -> &mut ToWorker<ApprovalDistributionMessage> {
	let worker_index = validator.0 as usize % to_approval_distribution_workers.len();
	to_approval_distribution_workers
		.get_mut(worker_index)
		.expect("Worker index is obtained modulo len; qed")
}

// Returns the validators that initially created this assignments/votes, the validator index
// is later used to decide which approval-distribution worker should receive the message.
//
// Because this is on the hot path and we don't want to be unnecessarily slow, it contains two logic
// paths. The ultra fast path where all messages have the same validator index and we don't do
// any cloning or allocation and the path where we need to split the messages into multiple
// messages, because they have different validator indices, where we do need to clone and allocate.
// In practice most of the message will fall on the ultra fast path.
fn validator_index_for_msg(
	msg: polkadot_node_network_protocol::ApprovalDistributionMessage,
) -> (
	Option<(ValidatorIndex, polkadot_node_network_protocol::ApprovalDistributionMessage)>,
	Option<Vec<(ValidatorIndex, polkadot_node_network_protocol::ApprovalDistributionMessage)>>,
) {
	match msg {
		polkadot_node_network_protocol::ValidationProtocols::V3(ref message) => match message {
			polkadot_node_network_protocol::v3::ApprovalDistributionMessage::Assignments(msgs) =>
				if let Ok(validator) = msgs.iter().map(|(msg, _)| msg.validator).all_equal_value() {
					(Some((validator, msg)), None)
				} else {
					let split = msgs
						.iter()
						.map(|(msg, claimed_candidates)| {
							(
								msg.validator,
								polkadot_node_network_protocol::ValidationProtocols::V3(
									polkadot_node_network_protocol::v3::ApprovalDistributionMessage::Assignments(
										vec![(msg.clone(), claimed_candidates.clone())]
									),
								),
							)
						})
						.collect_vec();
					(None, Some(split))
				},
			polkadot_node_network_protocol::v3::ApprovalDistributionMessage::Approvals(msgs) =>
				if let Ok(validator) = msgs.iter().map(|msg| msg.validator).all_equal_value() {
					(Some((validator, msg)), None)
				} else {
					let split = msgs
						.iter()
						.map(|vote| {
							(
								vote.validator,
								polkadot_node_network_protocol::ValidationProtocols::V3(
									polkadot_node_network_protocol::v3::ApprovalDistributionMessage::Approvals(
										vec![vote.clone()]
									),
								),
							)
						})
						.collect_vec();
					(None, Some(split))
				},
		},
	}
}

/// A handler object that both type of workers use for receiving work.
///
/// In practive this is just a wrapper over two channels Receiver, that is injected into
/// approval-voting worker and approval-distribution workers.
type WorkProvider<M, Clos, State> = WorkProviderImpl<
	SelectWithStrategy<
		MeteredReceiver<FromOrchestra<M>>,
		UnboundedMeteredReceiver<FromOrchestra<M>>,
		Clos,
		State,
	>,
>;

pub struct WorkProviderImpl<T>(T);

impl<T, M> Stream for WorkProviderImpl<T>
where
	T: Stream<Item = FromOrchestra<M>> + Unpin + Send,
{
	type Item = FromOrchestra<M>;

	fn poll_next(
		mut self: std::pin::Pin<&mut Self>,
		cx: &mut std::task::Context<'_>,
	) -> std::task::Poll<Option<Self::Item>> {
		self.0.poll_next_unpin(cx)
	}
}

#[async_trait::async_trait]
impl<T> ApprovalVotingWorkProvider for WorkProviderImpl<T>
where
	T: Stream<Item = FromOrchestra<ApprovalVotingMessage>> + Unpin + Send,
{
	async fn recv(&mut self) -> SubsystemResult<FromOrchestra<ApprovalVotingMessage>> {
		self.0.next().await.ok_or(SubsystemError::Context(
			"ApprovalVotingWorkProviderImpl: Channel closed".to_string(),
		))
	}
}

impl<M, Clos, State> WorkProvider<M, Clos, State>
where
	M: Send + Sync + 'static,
	Clos: FnMut(&mut State) -> PollNext,
	State: Default,
{
	// Constructs a work providers from the channels handles.
	fn from_rx_worker(rx: RxWorker<M>, prio: Clos) -> Self {
		let prioritised = select_with_strategy(rx.0, rx.1, prio);
		WorkProviderImpl(prioritised)
	}
}

/// Just a wrapper for implementing `overseer::SubsystemSender<ApprovalVotingMessage>` and
/// `overseer::SubsystemSender<ApprovalDistributionMessage>`.
///
/// The instance of this struct can be injected into the workers, so they can talk
/// directly with each other without intermediating in this subsystem loop.
pub struct ToWorker<T: Send + Sync + 'static>(
	MeteredSender<FromOrchestra<T>>,
	UnboundedMeteredSender<FromOrchestra<T>>,
);

impl<T: Send + Sync + 'static> Clone for ToWorker<T> {
	fn clone(&self) -> Self {
		Self(self.0.clone(), self.1.clone())
	}
}

impl<T: Send + Sync + 'static> ToWorker<T> {
	async fn send_signal(&mut self, signal: OverseerSignal) -> Result<(), SubsystemError> {
		self.1
			.unbounded_send(FromOrchestra::Signal(signal))
			.map_err(|err| SubsystemError::QueueError(err.into_send_error()))
	}

	fn meter(&self) -> Meters {
		Meters::new(self.0.meter(), self.1.meter())
	}
}

impl<T: Send + Sync + 'static + Debug> overseer::SubsystemSender<T> for ToWorker<T> {
	fn send_message<'life0, 'async_trait>(
		&'life0 mut self,
		msg: T,
	) -> ::core::pin::Pin<
		Box<dyn ::core::future::Future<Output = ()> + ::core::marker::Send + 'async_trait>,
	>
	where
		'life0: 'async_trait,
		Self: 'async_trait,
	{
		async {
			if let Err(err) =
				self.0.send(polkadot_overseer::FromOrchestra::Communication { msg }).await
			{
				gum::error!(
					target: LOG_TARGET,
					"Failed to send message to approval voting worker: {:?}, subsystem is probably shutting down.",
					err
				);
			}
		}
		.boxed()
	}

	fn try_send_message(&mut self, msg: T) -> Result<(), metered::TrySendError<T>> {
		self.0
			.try_send(polkadot_overseer::FromOrchestra::Communication { msg })
			.map_err(|result| {
				let is_full = result.is_full();
				let msg = match result.into_inner() {
					polkadot_overseer::FromOrchestra::Signal(_) =>
						panic!("Cannot happen variant is never built"),
					polkadot_overseer::FromOrchestra::Communication { msg } => msg,
				};
				if is_full {
					metered::TrySendError::Full(msg)
				} else {
					metered::TrySendError::Closed(msg)
				}
			})
	}

	fn send_messages<'life0, 'async_trait, I>(
		&'life0 mut self,
		msgs: I,
	) -> ::core::pin::Pin<
		Box<dyn ::core::future::Future<Output = ()> + ::core::marker::Send + 'async_trait>,
	>
	where
		I: IntoIterator<Item = T> + Send,
		I::IntoIter: Send,
		I: 'async_trait,
		'life0: 'async_trait,
		Self: 'async_trait,
	{
		async {
			for msg in msgs {
				self.send_message(msg).await;
			}
		}
		.boxed()
	}

	fn send_unbounded_message(&mut self, msg: T) {
		if let Err(err) =
			self.1.unbounded_send(polkadot_overseer::FromOrchestra::Communication { msg })
		{
			gum::error!(
				target: LOG_TARGET,
				"Failed to send unbounded message to approval voting worker: {:?}, subsystem is probably shutting down.",
				err
			);
		}
	}

	fn send_message_with_priority<'life0, 'async_trait, P>(
		&'life0 mut self,
		msg: T,
	) -> ::core::pin::Pin<
		Box<dyn ::core::future::Future<Output = ()> + ::core::marker::Send + 'async_trait>,
	>
	where
		P: 'async_trait + Priority,
		'life0: 'async_trait,
		Self: 'async_trait,
	{
		match P::priority() {
			polkadot_overseer::PriorityLevel::Normal => self.send_message(msg),
			polkadot_overseer::PriorityLevel::High =>
				async { self.send_unbounded_message(msg) }.boxed(),
		}
	}

	fn try_send_message_with_priority<P: Priority>(
		&mut self,
		msg: T,
	) -> Result<(), metered::TrySendError<T>> {
		match P::priority() {
			polkadot_overseer::PriorityLevel::Normal => self.try_send_message(msg),
			polkadot_overseer::PriorityLevel::High => Ok(self.send_unbounded_message(msg)),
		}
	}
}

/// Handles that are used by an worker to receive work.
pub struct RxWorker<T: Send + Sync + 'static>(
	MeteredReceiver<FromOrchestra<T>>,
	UnboundedMeteredReceiver<FromOrchestra<T>>,
);

// Build all the necessary channels for sending messages to an worker
// and for the worker to receive them.
fn build_channels<T: Send + Sync + 'static>(
	channel_name: String,
	channel_size: usize,
	metrics_watcher: &mut MetricsWatcher,
) -> (ToWorker<T>, RxWorker<T>) {
	let (tx_work, rx_work) = channel::<FromOrchestra<T>>(channel_size);
	let (tx_work_unbounded, rx_work_unbounded) = unbounded::<FromOrchestra<T>>();
	let to_worker = ToWorker(tx_work, tx_work_unbounded);

	metrics_watcher.watch(channel_name, to_worker.meter());

	(to_worker, RxWorker(rx_work, rx_work_unbounded))
}

/// Build the worker handles used for interacting with the workers.
///
/// `ToWorker` is used for sending messages to the workers.
/// `WorkProvider` is used by the workers for receiving the messages.
fn build_worker_handles<M, Clos, State>(
	channel_name: String,
	channel_size: usize,
	metrics_watcher: &mut MetricsWatcher,
	prio_right: Clos,
) -> (ToWorker<M>, WorkProvider<M, Clos, State>)
where
	M: Send + Sync + 'static,
	Clos: FnMut(&mut State) -> PollNext,
	State: Default,
{
	let (to_worker, rx_worker) = build_channels(channel_name, channel_size, metrics_watcher);
	(to_worker, WorkProviderImpl::from_rx_worker(rx_worker, prio_right))
}

/// Just a wrapper for implementing `overseer::SubsystemSender<ApprovalDistributionMessage>`, so
/// that we can inject into the approval voting subsystem.
#[derive(Clone)]
pub struct ApprovalVotingToApprovalDistribution<S: SubsystemSender<ApprovalVotingParallelMessage>>(
	S,
);

impl<S: SubsystemSender<ApprovalVotingParallelMessage>>
	overseer::SubsystemSender<ApprovalDistributionMessage>
	for ApprovalVotingToApprovalDistribution<S>
{
	#[allow(clippy::type_complexity, clippy::type_repetition_in_bounds)]
	fn send_message<'life0, 'async_trait>(
		&'life0 mut self,
		msg: ApprovalDistributionMessage,
	) -> ::core::pin::Pin<
		Box<dyn ::core::future::Future<Output = ()> + ::core::marker::Send + 'async_trait>,
	>
	where
		'life0: 'async_trait,
		Self: 'async_trait,
	{
		self.0.send_message(msg.into())
	}

	fn try_send_message(
		&mut self,
		msg: ApprovalDistributionMessage,
	) -> Result<(), metered::TrySendError<ApprovalDistributionMessage>> {
		self.0.try_send_message(msg.into()).map_err(|err| match err {
			// Safe to unwrap because it was built from the same type.
			metered::TrySendError::Closed(msg) =>
				metered::TrySendError::Closed(msg.try_into().unwrap()),
			metered::TrySendError::Full(msg) =>
				metered::TrySendError::Full(msg.try_into().unwrap()),
		})
	}

	#[allow(clippy::type_complexity, clippy::type_repetition_in_bounds)]
	fn send_messages<'life0, 'async_trait, I>(
		&'life0 mut self,
		msgs: I,
	) -> ::core::pin::Pin<
		Box<dyn ::core::future::Future<Output = ()> + ::core::marker::Send + 'async_trait>,
	>
	where
		I: IntoIterator<Item = ApprovalDistributionMessage> + Send,
		I::IntoIter: Send,
		I: 'async_trait,
		'life0: 'async_trait,
		Self: 'async_trait,
	{
		self.0.send_messages(msgs.into_iter().map(|msg| msg.into()))
	}

	fn send_unbounded_message(&mut self, msg: ApprovalDistributionMessage) {
		self.0.send_unbounded_message(msg.into())
	}

	fn send_message_with_priority<'life0, 'async_trait, P>(
		&'life0 mut self,
		msg: ApprovalDistributionMessage,
	) -> ::core::pin::Pin<
		Box<dyn ::core::future::Future<Output = ()> + ::core::marker::Send + 'async_trait>,
	>
	where
		P: 'async_trait + Priority,
		'life0: 'async_trait,
		Self: 'async_trait,
	{
		self.0.send_message_with_priority::<P>(msg.into())
	}

	fn try_send_message_with_priority<P: Priority>(
		&mut self,
		msg: ApprovalDistributionMessage,
	) -> Result<(), metered::TrySendError<ApprovalDistributionMessage>> {
		self.0.try_send_message_with_priority::<P>(msg.into()).map_err(|err| match err {
			// Safe to unwrap because it was built from the same type.
			metered::TrySendError::Closed(msg) =>
				metered::TrySendError::Closed(msg.try_into().unwrap()),
			metered::TrySendError::Full(msg) =>
				metered::TrySendError::Full(msg.try_into().unwrap()),
		})
	}
}
