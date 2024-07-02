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
use polkadot_node_core_approval_voting::{
	time::{Clock, SystemClock},
	Config,
};
use polkadot_node_metrics::metered;

use polkadot_node_subsystem::{
	messages::{ApprovalDistributionMessage, ApprovalVotingMessage, ApprovalVotingParallelMessage},
	overseer, FromOrchestra, SpawnedSubsystem, SubsystemError, SubsystemResult,
};

use polkadot_node_subsystem_util::{
	self,
	database::Database,
	metrics::{self, prometheus},
};
use polkadot_overseer::SubsystemSender;
use polkadot_primitives::ValidatorIndex;
use rand::SeedableRng;

use sc_keystore::LocalKeystore;
use sp_consensus::SyncOracle;

use futures::{channel::oneshot, prelude::*, StreamExt};
use polkadot_node_core_approval_voting::approval_db::common::Config as DatabaseConfig;
use std::{collections::HashMap, sync::Arc};

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
	mode: polkadot_node_core_approval_voting::Mode,
	metrics: Metrics,
	spawner: Arc<dyn overseer::gen::Spawner + 'static>,
	clock: Arc<dyn Clock + Send + Sync>,
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
	) -> Self {
		ApprovalVotingParallelSubsystem::with_config_and_clock(
			config,
			db,
			keystore,
			sync_oracle,
			metrics,
			Arc::new(SystemClock {}),
			spawner,
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
	) -> Self {
		ApprovalVotingParallelSubsystem {
			keystore,
			slot_duration_millis: config.slot_duration_millis,
			db,
			db_config: DatabaseConfig { col_approval_data: config.col_approval_data },
			mode: polkadot_node_core_approval_voting::Mode::Syncing(sync_oracle),
			metrics,
			spawner: Arc::new(spawner),
			clock,
		}
	}
}

#[overseer::subsystem(ApprovalVotingRewrite, error = SubsystemError, prefix = self::overseer)]
impl<Context: Send> ApprovalVotingParallelSubsystem {
	fn start(self, ctx: Context) -> SpawnedSubsystem {
		let future = run::<Context>(ctx, self)
			.map_err(|e| SubsystemError::with_origin("approval-voting-parallel", e))
			.boxed();

		SpawnedSubsystem { name: "approval-voting-parallel-subsystem", future }
	}
}

/// The number of workers used for running the approval-distribution logic.
pub const APPROVAL_DISTRIBUTION_WORKER_COUNT: usize = 8;

/// The channel size for the workers.
pub const WORKERS_CHANNEL_SIZE: usize = 64000 / APPROVAL_DISTRIBUTION_WORKER_COUNT;

#[overseer::contextbounds(ApprovalVotingRewrite, prefix = self::overseer)]
async fn run<Context>(
	mut ctx: Context,
	subsystem: ApprovalVotingParallelSubsystem,
) -> SubsystemResult<()>
where
{
	let mut approval_distribution_channels = Vec::new();
	let (mut tx_approval_voting_work, rx_approval_voting_work) = futures::channel::mpsc::channel::<
		FromOrchestra<ApprovalVotingMessage>,
	>(WORKERS_CHANNEL_SIZE);

	let slot_duration_millis = subsystem.slot_duration_millis;

	gum::info!(target: LOG_TARGET, "Starting approval distribution workers");

	for i in 0..APPROVAL_DISTRIBUTION_WORKER_COUNT {
		let approval_distro_orig = polkadot_approval_distribution::ApprovalDistribution::new(
			subsystem.metrics.0.clone(),
			subsystem.slot_duration_millis,
		);

		let (tx_approval_distribution_work, mut rx_approval_distribution_work) =
			futures::channel::mpsc::channel::<FromOrchestra<ApprovalDistributionMessage>>(
				WORKERS_CHANNEL_SIZE,
			);

		let task_name = format!("approval-voting-parallel-{}", i);
		let mut approval_distribution_to_approval_voting =
			ApprovalDistributionToApprovalWorker(tx_approval_voting_work.clone());
		let mut network_sender = ctx.sender().clone();
		let clock = subsystem.clock.clone();

		subsystem.spawner.spawn_blocking(
			task_name.leak(),
			Some("approval-voting-parallel-subsystem"),
			Box::pin(async move {
				let mut state =
					polkadot_approval_distribution::State::with_config(slot_duration_millis, clock);
				let mut rng = rand::rngs::StdRng::from_entropy();

				loop {
					let message = rx_approval_distribution_work.next().await.unwrap();
					approval_distro_orig
						.handle_from_orchestra(
							message,
							&mut approval_distribution_to_approval_voting,
							&mut network_sender,
							&mut state,
							&mut rng,
						)
						.await;
				}
			}),
		);
		approval_distribution_channels.push(tx_approval_distribution_work);
	}
	gum::info!(target: LOG_TARGET, "Starting approval voting workers");

	let sender = ctx.sender().clone();

	let approval_voting_to_subsystem = ApprovalVotingToApprovalDistribution(sender.clone());

	polkadot_node_core_approval_voting::start_approval_worker(
		rx_approval_voting_work,
		sender.clone(),
		approval_voting_to_subsystem,
		polkadot_node_core_approval_voting::Config {
			slot_duration_millis: subsystem.slot_duration_millis,
			col_approval_data: subsystem.db_config.col_approval_data,
		},
		subsystem.db.clone(),
		subsystem.keystore.clone(),
		subsystem.mode,
		subsystem.metrics.1.clone(),
		subsystem.spawner.clone(),
		subsystem.clock.clone(),
	)
	.await
	.unwrap();

	gum::info!(target: LOG_TARGET, "Starting main subsystem loop");

	// Main loop of the subsystem, it shouldn't include any logic just dispatching of messages to
	// the workers.
	loop {
		futures::select! {
			next_msg = ctx.recv().fuse() => {
				match next_msg.unwrap() {
					FromOrchestra::Signal(msg) => {
						for worker in approval_distribution_channels.iter_mut() {
							worker
								.send(FromOrchestra::Signal(msg.clone())).await?;
						}

						tx_approval_voting_work.send(FromOrchestra::Signal(msg)).await?;
					},
					FromOrchestra::Communication { msg } => match msg {
						// The message the approval voting subsystem would've handled.
						ApprovalVotingParallelMessage::CheckAndImportAssignment(_,_, _) |
						ApprovalVotingParallelMessage::CheckAndImportApproval(_)|
						ApprovalVotingParallelMessage::ApprovedAncestor(_, _,_) |
						ApprovalVotingParallelMessage::GetApprovalSignaturesForCandidate(_, _)  => {
							// Safe to unwrap because we know the message is the right type.
							tx_approval_voting_work.send(FromOrchestra::Communication{msg: msg.try_into().unwrap()}).await?;
						},
						// Not the message the approval distribution subsystem would've handled.
						ApprovalVotingParallelMessage::NewBlocks(msg) => {
							for worker in approval_distribution_channels.iter_mut() {
								worker
									.send(FromOrchestra::Communication {
										msg: ApprovalDistributionMessage::NewBlocks(msg.clone()),
									})
									.await?;
							}
						},
						ApprovalVotingParallelMessage::DistributeAssignment(assignment, claimed) => {
							let worker_index = assignment.validator.0 as usize % approval_distribution_channels.len();
							let worker = approval_distribution_channels.get_mut(worker_index).unwrap();
							worker
								.send(FromOrchestra::Communication {
									msg: ApprovalDistributionMessage::DistributeAssignment(assignment, claimed),
								})
								.await?;

						},
						ApprovalVotingParallelMessage::DistributeApproval(vote) => {
							let worker_index = vote.validator.0 as usize % approval_distribution_channels.len();
							let worker = approval_distribution_channels.get_mut(worker_index).unwrap();
							worker
								.send(FromOrchestra::Communication {
									msg: ApprovalDistributionMessage::DistributeApproval(vote),
								}).await?;

						},
						ApprovalVotingParallelMessage::NetworkBridgeUpdate(msg) => {
							if let polkadot_node_subsystem::messages::NetworkBridgeEvent::PeerMessage(
								peer_id,
								msg,
							) = msg
							{
								let validator_index = validator_index_for_msg(&msg);
								let worker_index = validator_index.0 as usize % approval_distribution_channels.len();
								let worker = approval_distribution_channels.get_mut(worker_index).unwrap();

								worker
									.send(FromOrchestra::Communication {
										msg: ApprovalDistributionMessage::NetworkBridgeUpdate(
											polkadot_node_subsystem::messages::NetworkBridgeEvent::PeerMessage(
												peer_id, msg,
											),
										),
									})
									.await?;
							} else {
								for worker in approval_distribution_channels.iter_mut() {
									worker
										.send(FromOrchestra::Communication {
											msg: ApprovalDistributionMessage::NetworkBridgeUpdate(msg.clone()),
										}).await?;
								}
							}
						},
						ApprovalVotingParallelMessage::GetApprovalSignatures(indices, tx) => {
							let mut sigs = HashMap::new();
							let mut signatures_channels = Vec::new();
							for worker in approval_distribution_channels.iter_mut() {
								let (tx, rx) = oneshot::channel();
								worker
									.send(FromOrchestra::Communication {
										msg: ApprovalDistributionMessage::GetApprovalSignatures(indices.clone(), tx),
									}).await?;
								signatures_channels.push(rx);
							}
							let results = futures::future::join_all(signatures_channels).await;

							for result in results {
								let worker_sigs = result.unwrap();
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
									.send(FromOrchestra::Communication {
										msg: ApprovalDistributionMessage::ApprovalCheckingLagUpdate(lag),
									}).await?;
							}
						},
					},
				};

			},
		};
	}
}

// Returns the validators that initially created this Assignment or Vote.
fn validator_index_for_msg(
	msg: &polkadot_node_network_protocol::ApprovalDistributionMessage,
) -> ValidatorIndex {
	match msg {
		polkadot_node_network_protocol::Versioned::V1(ref msg) => match msg {
			polkadot_node_network_protocol::v1::ApprovalDistributionMessage::Assignments(msgs) =>
				msgs.first().unwrap().0.validator,
			polkadot_node_network_protocol::v1::ApprovalDistributionMessage::Approvals(msgs) =>
				msgs.first().unwrap().validator,
		},
		polkadot_node_network_protocol::Versioned::V2(ref msg) => match msg {
			polkadot_node_network_protocol::v2::ApprovalDistributionMessage::Assignments(msgs) =>
				msgs.first().unwrap().0.validator,
			polkadot_node_network_protocol::v2::ApprovalDistributionMessage::Approvals(msgs) =>
				msgs.first().unwrap().validator,
		},
		polkadot_node_network_protocol::Versioned::V3(ref msg) => match msg {
			polkadot_node_network_protocol::v3::ApprovalDistributionMessage::Assignments(msgs) =>
				msgs.first().unwrap().0.validator,
			polkadot_node_network_protocol::v3::ApprovalDistributionMessage::Approvals(msgs) =>
				msgs.first().unwrap().validator,
		},
	}
}

/// Just a wrapper for implementing overseer::SubsystemSender<ApprovalVotingMessage>, so that
/// we can inject into the approval-distribution subsystem.
#[derive(Clone)]
pub struct ApprovalDistributionToApprovalWorker(
	futures::channel::mpsc::Sender<FromOrchestra<ApprovalVotingMessage>>,
);

impl overseer::SubsystemSender<ApprovalVotingMessage> for ApprovalDistributionToApprovalWorker {
	fn send_message<'life0, 'async_trait>(
		&'life0 mut self,
		msg: ApprovalVotingMessage,
	) -> ::core::pin::Pin<
		Box<dyn ::core::future::Future<Output = ()> + ::core::marker::Send + 'async_trait>,
	>
	where
		'life0: 'async_trait,
		Self: 'async_trait,
	{
		async {
			self.0
				.send(polkadot_overseer::FromOrchestra::Communication { msg })
				.await
				.unwrap()
		}
		.boxed()
	}

	fn try_send_message(
		&mut self,
		_msg: ApprovalVotingMessage,
	) -> Result<(), metered::TrySendError<ApprovalVotingMessage>> {
		todo!("Unused  for now")
	}

	fn send_messages<'life0, 'async_trait, I>(
		&'life0 mut self,
		_msgs: I,
	) -> ::core::pin::Pin<
		Box<dyn ::core::future::Future<Output = ()> + ::core::marker::Send + 'async_trait>,
	>
	where
		I: IntoIterator<Item = ApprovalVotingMessage> + Send,
		I::IntoIter: Send,
		I: 'async_trait,
		'life0: 'async_trait,
		Self: 'async_trait,
	{
		todo!("Unused  for now")
	}

	fn send_unbounded_message(&mut self, _msg: ApprovalVotingMessage) {
		todo!("Unused  for now")
	}
}

/// Just a wrapper for implementing overseer::SubsystemSender<ApprovalDistributionMessage>, so that
/// we can inject into the approval voting subsystem.
#[derive(Clone)]
pub struct ApprovalVotingToApprovalDistribution<S: SubsystemSender<ApprovalVotingParallelMessage>>(
	S,
);

impl<S: SubsystemSender<ApprovalVotingParallelMessage>>
	overseer::SubsystemSender<ApprovalDistributionMessage> for ApprovalVotingToApprovalDistribution<S>
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
