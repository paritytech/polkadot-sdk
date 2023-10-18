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

//! [`ApprovalDistribution`] implementation.
//!
//! <https://w3f.github.io/parachain-implementers-guide/node/approval/approval-distribution.html>

#![warn(missing_docs)]

use futures::{select, FutureExt as _};
// use polkadot_node_jaeger as jaeger;
use polkadot_node_network_protocol::{
	self as net_protocol, v1 as protocol_v1, v2 as protocol_v2, Versioned,
};
use polkadot_node_subsystem_util::worker_pool::{Job, WorkerPool};
use worker::{state::MessageSource, ApprovalContext, ApprovalWorkerConfig, ApprovalWorkerMessage};

use polkadot_node_subsystem::{
	messages::{ApprovalDistributionMessage, NetworkBridgeEvent},
	overseer, FromOrchestra, OverseerSignal, SpawnedSubsystem, SubsystemError,
};

use self::metrics::Metrics;

pub(crate) mod metrics;
mod worker;

#[cfg(test)]
mod tests;

pub(crate) const LOG_TARGET: &str = "parachain::approval-distribution";

/// The Approval Distribution subsystem.
pub struct ApprovalDistribution {
	metrics: Metrics,
}

#[overseer::contextbounds(ApprovalDistribution, prefix = self::overseer)]
impl ApprovalDistribution {
	/// Create a new instance of the [`ApprovalDistribution`] subsystem.
	pub fn new(metrics: Metrics) -> Self {
		Self { metrics }
	}

	async fn run<Context>(self, ctx: Context) {
		// According to the docs of `rand`, this is a ChaCha12 RNG in practice
		// and will always be chosen for strong performance and security properties.
		self.run_inner(ctx).await
	}

	/// Used for testing.
	async fn run_inner<Context>(mut self, mut ctx: Context) {
		let sender = ctx.sender().clone();
		let metrics = self.metrics.clone();
		let mut approval_worker_config = worker::ApprovalWorkerConfig::new(sender, metrics.clone());
		let (mut approval_worker_pool, _) = WorkerPool::with_config(&mut approval_worker_config);

		loop {
			select! {
				message = ctx.recv().fuse() => {
					let message = match message {
						Ok(message) => message,
						Err(e) => {
							gum::debug!(target: LOG_TARGET, err = ?e, "Failed to receive a message from Overseer,
			exiting"); 				return
						},
					};
					match message {
						FromOrchestra::Communication { msg } =>
							self.handle_incoming(&mut approval_worker_pool, msg).await,
						FromOrchestra::Signal(OverseerSignal::ActiveLeaves(_update)) => {
							gum::trace!(target: LOG_TARGET, "active leaves signal (ignored)");
							// the relay chain blocks relevant to the approval subsystems
							// are those that are available, but not finalized yet
							// actived and deactivated heads hence are irrelevant to this subsystem, other than
							// for tracing purposes.
							// if let Some(activated) = update.activated {
							// 	let head = activated.hash;
							// 	let approval_distribution_span =
							// 		jaeger::PerLeafSpan::new(activated.span, "approval-distribution");
							// 	state.spans.insert(head, approval_distribution_span);
							// }
						},
						FromOrchestra::Signal(OverseerSignal::BlockFinalized(_hash, number)) => {
							gum::trace!(target: LOG_TARGET, number = %number, "finalized signal");
							approval_worker_pool.queue_work(ApprovalWorkerMessage::BlockFinalized(number)).await;
						},
						FromOrchestra::Signal(OverseerSignal::Conclude) => return,
					}
				},
			}
		}
	}

	async fn handle_network_msg<F>(
		&self,
		worker_pool: &mut WorkerPool<ApprovalWorkerConfig<F>>,
		event: NetworkBridgeEvent<net_protocol::ApprovalDistributionMessage>,
	) where
		F: overseer::ApprovalDistributionSenderTrait,
	{
		match event {
			NetworkBridgeEvent::PeerConnected(peer_id, role, version, _) => {
				worker_pool
					.queue_work(ApprovalWorkerMessage::PeerConnected(peer_id, role, version))
					.await;
			},
			NetworkBridgeEvent::PeerDisconnected(peer_id) => {
				worker_pool.queue_work(ApprovalWorkerMessage::PeerDisconnected(peer_id)).await;
			},
			NetworkBridgeEvent::NewGossipTopology(topology) => {
				worker_pool.queue_work(ApprovalWorkerMessage::NewGossipTopology(topology)).await;
			},
			NetworkBridgeEvent::PeerViewChange(peer_id, view) => {
				worker_pool
					.queue_work(ApprovalWorkerMessage::PeerViewChange(peer_id, view))
					.await;
			},
			NetworkBridgeEvent::OurViewChange(view) => {
				worker_pool.queue_work(ApprovalWorkerMessage::OurViewChange(view)).await;
			},
			NetworkBridgeEvent::PeerMessage(peer_id, msg) => match msg {
				Versioned::V1(protocol_v1::ApprovalDistributionMessage::Assignments(
					assignments,
				)) |
				Versioned::V2(protocol_v2::ApprovalDistributionMessage::Assignments(
					assignments,
				)) =>
					for assignment in assignments {
						let work_item = ApprovalWorkerMessage::ProcessAssignment(
							assignment.0,
							assignment.1,
							MessageSource::Peer(peer_id),
						);
						self.handle_new_job(&work_item, worker_pool).await;
						worker_pool.queue_work(work_item).await;
					},
				Versioned::V1(protocol_v1::ApprovalDistributionMessage::Approvals(approvals)) |
				Versioned::V2(protocol_v2::ApprovalDistributionMessage::Approvals(approvals)) =>
					for approval in approvals.into_iter() {
						let work_item = ApprovalWorkerMessage::ProcessApproval(
							approval,
							MessageSource::Peer(peer_id),
						);
						self.handle_new_job(&work_item, worker_pool).await;
						worker_pool.queue_work(work_item).await;
					},
			},
			NetworkBridgeEvent::UpdatedAuthorityIds { .. } => {
				// The approval-distribution subsystem doesn't deal with `AuthorityDiscoveryId`s.
			},
		}
	}

	async fn handle_new_job<F>(
		&self,
		work_item: &ApprovalWorkerMessage,
		worker_pool: &mut WorkerPool<ApprovalWorkerConfig<F>>,
	) where
		F: overseer::ApprovalDistributionSenderTrait,
	{
		let job_id = if let Some(job_id) = work_item.id() {
			job_id
		} else {
			gum::warn!(
				target: LOG_TARGET,
				?work_item,
				"Expected `JobId`",
			);
			return
		};

		if !worker_pool.job_exists(&job_id) {
			worker_pool.new_job(job_id, ApprovalContext).await;
		}
	}

	async fn handle_incoming<F>(
		&mut self,
		worker_pool: &mut WorkerPool<ApprovalWorkerConfig<F>>,
		msg: ApprovalDistributionMessage,
	) where
		F: overseer::ApprovalDistributionSenderTrait,
	{
		match msg {
			ApprovalDistributionMessage::NetworkBridgeUpdate(event) => {
				self.handle_network_msg(worker_pool, event).await;
			},
			ApprovalDistributionMessage::NewBlocks(metas) => {
				worker_pool.queue_work(ApprovalWorkerMessage::NewBlocks(metas)).await;
			},
			ApprovalDistributionMessage::DistributeAssignment(cert, candidate_index) => {
				gum::debug!(
					target: LOG_TARGET,
					"Distributing our assignment on candidate (block={}, index={})",
					cert.block_hash,
					candidate_index,
				);
				let work_item = ApprovalWorkerMessage::ProcessAssignment(
					cert,
					candidate_index,
					MessageSource::Local,
				);

				self.handle_new_job(&work_item, worker_pool).await;
				worker_pool.queue_work(work_item).await;
			},
			ApprovalDistributionMessage::DistributeApproval(vote) => {
				gum::debug!(
					target: LOG_TARGET,
					"Distributing our approval vote on candidate (block={}, index={})",
					vote.block_hash,
					vote.candidate_index,
				);
				let work_item = ApprovalWorkerMessage::ProcessApproval(vote, MessageSource::Local);
				self.handle_new_job(&work_item, worker_pool).await;
				worker_pool.queue_work(work_item).await;
			},
			ApprovalDistributionMessage::GetApprovalSignatures(_indices, tx) => {
				// let sigs = state.get_approval_signatures(indices);
				// TODO: remove this placeholder and implement aggregation of responses across
				// workers.
				let sigs = Default::default();
				if let Err(_) = tx.send(sigs) {
					gum::debug!(
						target: LOG_TARGET,
						"Sending back approval signatures failed, oneshot got closed"
					);
				}
			},
			ApprovalDistributionMessage::ApprovalCheckingLagUpdate(lag) => {
				gum::debug!(target: LOG_TARGET, lag, "Received `ApprovalCheckingLagUpdate`");
				worker_pool
					.queue_work(ApprovalWorkerMessage::ApprovalCheckingLagUpdate(lag))
					.await;
			},
		}
	}
}

#[overseer::subsystem(ApprovalDistribution, error=SubsystemError, prefix=self::overseer)]
impl<Context> ApprovalDistribution {
	fn start(self, ctx: Context) -> SpawnedSubsystem {
		let future = self.run(ctx).map(|_| Ok(())).boxed();

		SpawnedSubsystem { name: "approval-distribution-subsystem", future }
	}
}
