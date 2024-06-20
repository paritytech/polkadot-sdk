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
use polkadot_node_core_approval_voting::{
	time::{Clock, SystemClock},
	Config, RealAssignmentCriteria,
};
use polkadot_node_metrics::metered::{
	self, channel, unbounded, MeteredSender, UnboundedMeteredSender,
};

use polkadot_node_primitives::DISPUTE_WINDOW;
use polkadot_node_subsystem::{
	messages::{ApprovalDistributionMessage, ApprovalVotingMessage, ApprovalVotingParallelMessage},
	overseer, FromOrchestra, SpawnedSubsystem, SubsystemError, SubsystemResult,
};
use polkadot_node_subsystem_util::{
	self,
	database::Database,
	metrics::{self, prometheus},
	runtime::{Config as RuntimeInfoConfig, RuntimeInfo},
};
use polkadot_overseer::{OverseerSignal, SubsystemSender};
use polkadot_primitives::ValidatorIndex;
use rand::SeedableRng;

use sc_keystore::LocalKeystore;
use sp_consensus::SyncOracle;

use futures::{channel::oneshot, prelude::*, StreamExt};
use polkadot_node_core_approval_voting::{
	approval_db::common::Config as DatabaseConfig, ApprovalVotingWorkProvider,
};
use std::{collections::HashMap, fmt::Debug, sync::Arc};
use stream::{select_with_strategy, PollNext};

pub(crate) const LOG_TARGET: &str = "parachain::approval-voting-parallel";

/// The approval voting subsystem.
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
	subsystem_enabled: bool,
}

/// Approval Voting metrics.
#[derive(Default, Clone)]
pub struct Metrics(
	pub polkadot_approval_distribution::metrics::Metrics,
	pub polkadot_node_core_approval_voting::Metrics,
);

impl metrics::Metrics for Metrics {
	fn try_register(
		registry: &prometheus::Registry,
	) -> std::result::Result<Self, prometheus::PrometheusError> {
		Ok(Metrics(
			polkadot_approval_distribution::metrics::Metrics::try_register(registry)?,
			polkadot_node_core_approval_voting::Metrics::try_register(registry)?,
		))
	}
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
		subsystem_enabled: bool,
	) -> Self {
		ApprovalVotingParallelSubsystem::with_config_and_clock(
			config,
			db,
			keystore,
			sync_oracle,
			metrics,
			Arc::new(SystemClock {}),
			spawner,
			subsystem_enabled,
		)
	}

	/// Create a new approval voting subsystem with the given keystore, config, and database.
	pub fn with_config_and_clock(
		config: Config,
		db: Arc<dyn Database>,
		keystore: Arc<LocalKeystore>,
		sync_oracle: Box<dyn SyncOracle + Send>,
		metrics: Metrics,
		clock: Arc<dyn Clock + Send + Sync>,
		spawner: impl overseer::gen::Spawner + 'static,
		subsystem_enabled: bool,
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
			subsystem_enabled,
		}
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

/// The number of workers used for running the approval-distribution logic.
pub const APPROVAL_DISTRIBUTION_WORKER_COUNT: usize = 2;

/// The channel size for the workers.
pub const WORKERS_CHANNEL_SIZE: usize = 64000 / APPROVAL_DISTRIBUTION_WORKER_COUNT;
fn prio_right<'a>(_val: &'a mut ()) -> PollNext {
	PollNext::Right
}

#[overseer::contextbounds(ApprovalVotingParallel, prefix = self::overseer)]
async fn run<Context>(
	mut ctx: Context,
	subsystem: ApprovalVotingParallelSubsystem,
) -> SubsystemResult<()>
where
{
	// Build approval voting handles.
	let (tx_approval_voting_work, rx_approval_voting_work) =
		channel::<FromOrchestra<ApprovalVotingMessage>>(WORKERS_CHANNEL_SIZE);
	let (tx_approval_voting_work_unbounded, rx_approval_voting_work_unbounded) =
		unbounded::<FromOrchestra<ApprovalVotingMessage>>();

	let mut to_approval_voting_worker =
		ToWorker(tx_approval_voting_work, tx_approval_voting_work_unbounded);

	let prioritised = select_with_strategy(
		rx_approval_voting_work,
		rx_approval_voting_work_unbounded,
		prio_right,
	);
	let approval_voting_work_provider = ApprovalVotingWorkProviderImpl(prioritised);

	gum::info!(target: LOG_TARGET, "Starting approval distribution workers");

	let mut approval_distribution_channels = Vec::new();
	let slot_duration_millis = subsystem.slot_duration_millis;

	for i in 0..APPROVAL_DISTRIBUTION_WORKER_COUNT {
		let approval_distro_orig =
			polkadot_approval_distribution::ApprovalDistribution::new_with_clock(
				subsystem.metrics.0.clone(),
				subsystem.slot_duration_millis,
				subsystem.clock.clone(),
				false,
			);

		let (tx_approval_distribution_work, rx_approval_distribution_work) =
			channel::<FromOrchestra<ApprovalDistributionMessage>>(WORKERS_CHANNEL_SIZE);
		let (tx_approval_distribution_work_unbounded, rx_approval_distribution_unbounded) =
			unbounded::<FromOrchestra<ApprovalDistributionMessage>>();

		let to_approval_distribution_worker =
			ToWorker(tx_approval_distribution_work, tx_approval_distribution_work_unbounded);

		let task_name = format!("approval-voting-parallel-{}", i);

		let mut network_sender = ctx.sender().clone();
		let mut runtime_api_sender = ctx.sender().clone();
		let mut approval_distribution_to_approval_voting = to_approval_voting_worker.clone();

		subsystem.spawner.spawn_blocking(
			task_name.leak(),
			Some("approval-voting-parallel-subsystem"),
			Box::pin(async move {
				let mut state =
					polkadot_approval_distribution::State::with_config(slot_duration_millis);
				let mut rng = rand::rngs::StdRng::from_entropy();
				let assignment_criteria = RealAssignmentCriteria {};
				let mut session_info_provider = RuntimeInfo::new_with_config(RuntimeInfoConfig {
					keystore: None,
					session_cache_lru_size: DISPUTE_WINDOW.get(),
				});

				let mut work_channels = select_with_strategy(
					rx_approval_distribution_work,
					rx_approval_distribution_unbounded,
					prio_right,
				);

				loop {
					let message = match work_channels.next().await {
						Some(message) => message,
						None => {
							gum::info!(
								target: LOG_TARGET,
								"Approval distribution stream finished, most likely shutting down",
							);
							break;
						},
					};
					approval_distro_orig
						.handle_from_orchestra(
							message,
							&mut approval_distribution_to_approval_voting,
							&mut network_sender,
							&mut runtime_api_sender,
							&mut state,
							&mut rng,
							&assignment_criteria,
							&mut session_info_provider,
						)
						.await;
				}
			}),
		);
		approval_distribution_channels.push(to_approval_distribution_worker);
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
		subsystem.metrics.1.clone(),
		subsystem.spawner.clone(),
		subsystem.clock.clone(),
	)
	.await?;

	gum::info!(
		target: LOG_TARGET,
		subsystem_disabled = ?subsystem.subsystem_enabled,
		"Starting main subsystem loop"
	);

	// Main loop of the subsystem, it shouldn't include any logic just dispatching of messages to
	// the workers.
	loop {
		futures::select! {
			next_msg = ctx.recv().fuse() => {
				let next_msg = match next_msg {
					Ok(msg) => msg,
					Err(err) => {
						gum::info!(target: LOG_TARGET, ?err, "Approval voting parallel subsystem received an error");
						break;
					}
				};
				if !subsystem.subsystem_enabled {
					gum::trace!(target: LOG_TARGET, ?next_msg, "Parallel processing is not enabled skipping message");
					continue;
				}
				gum::trace!(target: LOG_TARGET, ?next_msg, "Parallel processing is not enabled skipping message");

				match next_msg {
					FromOrchestra::Signal(msg) => {
						for worker in approval_distribution_channels.iter_mut() {
							worker
								.send_signal(msg.clone()).await?;
						}

						to_approval_voting_worker.send_signal(msg).await?;
					},
					FromOrchestra::Communication { msg } => match msg {
						// The message the approval voting subsystem would've handled.
						ApprovalVotingParallelMessage::CheckAndImportAssignment(_,_, _) |
						ApprovalVotingParallelMessage::CheckAndImportApproval(_)|
						ApprovalVotingParallelMessage::ApprovedAncestor(_, _,_) |
						ApprovalVotingParallelMessage::GetApprovalSignaturesForCandidate(_, _)  => {
							// Safe to unwrap because we know the message is of the right type.
							to_approval_voting_worker.send_message(msg.try_into().unwrap()).await;
						},
						// Now the message the approval distribution subsystem would've handled and need to
						// be forwarded to the workers.
						ApprovalVotingParallelMessage::NewBlocks(msg) => {
							for worker in approval_distribution_channels.iter_mut() {
								worker
									.send_message(
										ApprovalDistributionMessage::NewBlocks(msg.clone()),
									)
									.await;
							}
						},
						ApprovalVotingParallelMessage::DistributeAssignment(assignment, claimed) => {
							let worker_index = assignment.validator.0 as usize % approval_distribution_channels.len();
							let worker = approval_distribution_channels.get_mut(worker_index).expect("Worker index is obtained modulo len; qed");
							worker
								.send_message(
									ApprovalDistributionMessage::DistributeAssignment(assignment, claimed)
								)
								.await;

						},
						ApprovalVotingParallelMessage::DistributeApproval(vote) => {
							let worker_index = vote.validator.0 as usize % approval_distribution_channels.len();
							let worker = approval_distribution_channels.get_mut(worker_index).expect("Worker index is obtained modulo len; qed");
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
									let worker_index = validator_index.0 as usize % approval_distribution_channels.len();
									let worker = approval_distribution_channels.get_mut(worker_index).expect("Worker index is obtained modulo len; qed");

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
								for worker in approval_distribution_channels.iter_mut() {
									worker
										.send_message(
											ApprovalDistributionMessage::NetworkBridgeUpdate(msg.clone()),
										).await;
								}
							}
						},
						ApprovalVotingParallelMessage::GetApprovalSignatures(indices, tx) => {
							let mut sigs = HashMap::new();
							let mut signatures_channels = Vec::new();
							for worker in approval_distribution_channels.iter_mut() {
								let (tx, rx) = oneshot::channel();
								worker
									.send_message(
										ApprovalDistributionMessage::GetApprovalSignatures(indices.clone(), tx)
									).await;
								signatures_channels.push(rx);
							}
							let results = futures::future::join_all(signatures_channels).await;

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

							if let Err(_) = tx.send(sigs) {
								gum::debug!(
										target: LOG_TARGET,
										"Sending back approval signatures failed, oneshot got closed"
								);
							}
						},
						ApprovalVotingParallelMessage::ApprovalCheckingLagUpdate(lag) => {
							for worker in approval_distribution_channels.iter_mut() {
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

// Returns the validators that initially created this assignments/votes, the validator index
// is later used to decide which approval-distribution worker should receive the message.
//
// Because this is on the hot path and we don't want to be unnecessarily slow, it contains two logic
// paths. The ultra fast path where all messages have the same validator index and we don't don't do
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
		polkadot_node_network_protocol::Versioned::V1(ref message) => match message {
			polkadot_node_network_protocol::v1::ApprovalDistributionMessage::Assignments(msgs) =>
				if let Ok(validator) = msgs.iter().map(|(msg, _)| msg.validator).all_equal_value() {
					(Some((validator, msg)), None)
				} else {
					let split = msgs
						.iter()
						.map(|(msg, claimed_candidates)| {
							(
								msg.validator,
								polkadot_node_network_protocol::Versioned::V1(
									polkadot_node_network_protocol::v1::ApprovalDistributionMessage::Assignments(
										vec![(msg.clone(), *claimed_candidates)]
									),
								),
							)
						})
						.collect_vec();
					(None, Some(split))
				},
			polkadot_node_network_protocol::v1::ApprovalDistributionMessage::Approvals(msgs) =>
				if let Ok(validator) = msgs.iter().map(|msg| msg.validator).all_equal_value() {
					(Some((validator, msg)), None)
				} else {
					let split = msgs
						.iter()
						.map(|vote| {
							(
								vote.validator,
								polkadot_node_network_protocol::Versioned::V1(
									polkadot_node_network_protocol::v1::ApprovalDistributionMessage::Approvals(
										vec![vote.clone()]
									),
								),
							)
						})
						.collect_vec();
					(None, Some(split))
				},
		},
		polkadot_node_network_protocol::Versioned::V2(ref message) => match message {
			polkadot_node_network_protocol::v2::ApprovalDistributionMessage::Assignments(msgs) =>
				if let Ok(validator) = msgs.iter().map(|(msg, _)| msg.validator).all_equal_value() {
					(Some((validator, msg)), None)
				} else {
					let split = msgs
						.iter()
						.map(|(msg, claimed_candidates)| {
							(
								msg.validator,
								polkadot_node_network_protocol::Versioned::V2(
									polkadot_node_network_protocol::v2::ApprovalDistributionMessage::Assignments(
										vec![(msg.clone(), *claimed_candidates)]
									),
								),
							)
						})
						.collect_vec();
					(None, Some(split))
				},

			polkadot_node_network_protocol::v2::ApprovalDistributionMessage::Approvals(msgs) =>
				if let Ok(validator) = msgs.iter().map(|msg| msg.validator).all_equal_value() {
					(Some((validator, msg)), None)
				} else {
					let split = msgs
						.iter()
						.map(|vote| {
							(
								vote.validator,
								polkadot_node_network_protocol::Versioned::V2(
									polkadot_node_network_protocol::v2::ApprovalDistributionMessage::Approvals(
										vec![vote.clone()]
									),
								),
							)
						})
						.collect_vec();
					(None, Some(split))
				},
		},
		polkadot_node_network_protocol::Versioned::V3(ref message) => match message {
			polkadot_node_network_protocol::v3::ApprovalDistributionMessage::Assignments(msgs) =>
				if let Ok(validator) = msgs.iter().map(|(msg, _)| msg.validator).all_equal_value() {
					(Some((validator, msg)), None)
				} else {
					let split = msgs
						.iter()
						.map(|(msg, claimed_candidates)| {
							(
								msg.validator,
								polkadot_node_network_protocol::Versioned::V3(
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
								polkadot_node_network_protocol::Versioned::V3(
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

/// Just a wrapper over a channel Receiver, that is injected into approval-voting worker for
/// providing the messages to be processed.
pub struct ApprovalVotingWorkProviderImpl<T>(T);

#[async_trait::async_trait]
impl<T> ApprovalVotingWorkProvider for ApprovalVotingWorkProviderImpl<T>
where
	T: Stream<Item = FromOrchestra<ApprovalVotingMessage>> + Unpin + Send,
{
	async fn recv(&mut self) -> SubsystemResult<FromOrchestra<ApprovalVotingMessage>> {
		self.0.next().await.ok_or(SubsystemError::Context(
			"ApprovalVotingWorkProviderImpl: Channel closed".to_string(),
		))
	}
}

/// Just a wrapper for implementing overseer::SubsystemSender<ApprovalVotingMessage> and
/// overseer::SubsystemSender<ApprovalDistributionMessage>, so that we can inject into the
/// workers, so they can talke directly with each other without intermediating in this subsystem
/// loop.
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
}

/// Just a wrapper for implementing overseer::SubsystemSender<ApprovalDistributionMessage>, so that
/// we can inject into the approval voting subsystem.
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
}
