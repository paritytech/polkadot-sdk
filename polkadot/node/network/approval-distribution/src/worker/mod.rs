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

//! Approval worker configuration and implementation.
use self::state::MessageSource;
use crate::metrics::Metrics;
use async_trait::async_trait;
use bounded_collections::ConstU32;
use futures::{select, FutureExt};
use polkadot_node_subsystem::{messages::network_bridge_event::NewGossipTopology, overseer};
use polkadot_node_subsystem_util::{
	reputation::REPUTATION_CHANGE_INTERVAL,
	worker_pool::{Job, JobId, WorkerConfig, WorkerHandle, WorkerMessage},
};
use polkadot_primitives::{BlockNumber, CandidateIndex};
use tokio::sync::mpsc;

use polkadot_node_primitives::approval::{
	BlockApprovalMeta, IndirectAssignmentCert, IndirectSignedApprovalVote,
};

use rand::{CryptoRng, Rng, SeedableRng};

use polkadot_node_network_protocol::{
	peer_set::ProtocolVersion, ObservedRole, OurView, PeerId, View,
};

use state::MessageSubject;
pub mod state;

/// Approval work item definition.
#[derive(Clone, Debug)]
pub(crate) enum ApprovalWorkerMessage {
	/// Process an assignment.
	ProcessAssignment(IndirectAssignmentCert, CandidateIndex, MessageSource),
	/// Process an approval
	ProcessApproval(IndirectSignedApprovalVote, MessageSource),
	/// A peer has connected (broadcast)
	PeerConnected(PeerId, ObservedRole, ProtocolVersion),
	/// A peer has disconnected (broadcast)
	PeerDisconnected(PeerId),
	/// A new gossip topology (broadcast)
	NewGossipTopology(NewGossipTopology),
	/// Peer changed view (broadcast)
	PeerViewChange(PeerId, View),
	/// Our view changed (broadcast)
	OurViewChange(OurView),
	/// New blocks imported by approval voting (broadcast)
	NewBlocks(Vec<BlockApprovalMeta>),
	/// Lag update from finality chainn selection.
	ApprovalCheckingLagUpdate(BlockNumber),
	/// Block was finalized
	BlockFinalized(BlockNumber),
}

// Use `MessageSubject` as `JobId`.
impl Job for ApprovalWorkerMessage {
	fn id(&self) -> Option<JobId> {
		match &self {
			ApprovalWorkerMessage::ProcessApproval(vote, _) => Some(
				MessageSubject(vote.block_hash, vote.candidate_index, vote.validator)
					.worker_context(),
			),
			ApprovalWorkerMessage::ProcessAssignment(indirect_cert, candidate_index, _) => Some(
				MessageSubject(indirect_cert.block_hash, *candidate_index, indirect_cert.validator)
					.worker_context(),
			),
			_ => {
				// We don't need a context for messages that are broadcasted to all workers.
				None
			},
		}
	}
}

/// The approval worker configuration.
pub(crate) struct ApprovalWorkerConfig<F>
where
	F: overseer::ApprovalDistributionSenderTrait,
{
	sender: F,
	metrics: Metrics,
	next_worker_id: u16,
}

impl<F> ApprovalWorkerConfig<F>
where
	F: overseer::ApprovalDistributionSenderTrait,
{
	/// Constructor
	pub fn new(sender: F, metrics: Metrics) -> Self {
		Self { sender, metrics, next_worker_id: 0 }
	}

	/// Pop the next worker id.
	pub fn next_id(&mut self) -> u16 {
		let id = self.next_worker_id;
		self.next_worker_id += 1;
		id
	}
}

/// Approval worker handle implementation.
#[derive(Clone)]
pub(crate) struct ApprovalWorkerHandle<F>
where
	F: overseer::ApprovalDistributionSenderTrait,
{
	/// The worker sender
	pub sender: mpsc::Sender<WorkerMessage<ApprovalWorkerConfig<F>>>,
	pub id: u16,
}

#[async_trait]
impl<F> WorkerHandle for ApprovalWorkerHandle<F>
where
	F: overseer::ApprovalDistributionSenderTrait,
{
	type Config = ApprovalWorkerConfig<F>;

	fn index(&self) -> u16 {
		self.id
	}

	async fn send(&self, message: WorkerMessage<Self::Config>) {
		let _ = self.sender.send(message).await;
	}
}

async fn dispatch_work(
	sender: &mut impl overseer::ApprovalDistributionSenderTrait,
	state: &mut state::ApprovalWorkerState,
	metrics: &Metrics,
	rng: &mut (impl CryptoRng + Rng),
	work_item: ApprovalWorkerMessage,
) {
	match work_item {
		ApprovalWorkerMessage::ProcessApproval(vote, source) => {
			if let Some(peer_id) = source.peer_id() {
				// Assingment comes from the network, we have to do some spam filtering.
				state.handle_gossiped_approval(sender, metrics, peer_id, vote).await;
			} else {
				state.import_and_circulate_approval(sender, metrics, source, vote).await;
			}
		},
		ApprovalWorkerMessage::ProcessAssignment(assignment_cert, candidate_index, source) => {
			if let Some(peer_id) = source.peer_id() {
				// Assignment comes from the network, we have to do some spam filtering.
				state
					.handle_gossiped_assignment(
						sender,
						metrics,
						peer_id,
						assignment_cert,
						candidate_index,
						rng,
					)
					.await;
			} else {
				state
					.import_and_circulate_assignment(
						sender,
						metrics,
						source,
						assignment_cert,
						candidate_index,
						rng,
					)
					.await;
			}
		},
		ApprovalWorkerMessage::PeerViewChange(peer_id, view) =>
			state.handle_peer_view_change(sender, metrics, peer_id, view, rng).await,
		ApprovalWorkerMessage::NewBlocks(metas) =>
			state.handle_new_blocks(sender, metrics, metas, rng).await,
		ApprovalWorkerMessage::PeerConnected(peer_id, role, protocol_version) =>
			state.handle_peer_connect(peer_id, role, protocol_version).await,
		ApprovalWorkerMessage::PeerDisconnected(peer_id) =>
			state.handle_peer_disconnect(peer_id).await,
		ApprovalWorkerMessage::NewGossipTopology(topology) =>
			state
				.handle_new_session_topology(
					sender,
					topology.session,
					topology.topology,
					topology.local_index,
				)
				.await,
		ApprovalWorkerMessage::OurViewChange(view) => state.handle_our_view_change(view).await,
		ApprovalWorkerMessage::ApprovalCheckingLagUpdate(lag) => {
			state.update_approval_checking_lag(lag);
		},
		ApprovalWorkerMessage::BlockFinalized(block_number) => {
			state.handle_block_finalized(sender, metrics, block_number).await;
		},
	}
}

async fn worker_loop<
	ApprovalWorkerConfig: WorkerConfig<WorkItem = ApprovalWorkerMessage, JobState = ApprovalContext>,
>(
	mut from_pool: mpsc::Receiver<WorkerMessage<ApprovalWorkerConfig>>,
	mut sender: impl overseer::ApprovalDistributionSenderTrait,
	metrics: Metrics,
) {
	let mut rng = rand::rngs::StdRng::from_entropy();
	let mut worker_state = state::ApprovalWorkerState::default();

	let new_reputation_delay = || futures_timer::Delay::new(REPUTATION_CHANGE_INTERVAL).fuse();
	let mut reputation_delay = new_reputation_delay();

	loop {
		select! {
			_ = reputation_delay => {
				worker_state.reputation().send(&mut sender).await;
				reputation_delay = new_reputation_delay();
			},
			worker_message = from_pool.recv().fuse() => {
				let worker_message = if let Some(worker_message) = worker_message {
					worker_message
				} else {
					// Worker pool exiting.
					return
				};

				match worker_message {
					WorkerMessage::Queue(work_item) => {
						dispatch_work(&mut sender, &mut worker_state, &metrics, &mut rng, work_item)
							.await;
					},
					WorkerMessage::DeleteJobs(jobs) => {
						// This message might not be needed as `ApprovalWorkerMessage` can send the
						// block finalized event. In that case, workers need to notify worker pool that
						// a context has been removed, but this creates a cycle which is likely to cause
						// issues. To avoid the pool loop can periodically ask workers about pruned
						// tasks and delete them accordingly from the hashmap. More thinking required.
						for job_id in jobs {
							worker_state.delete_job(job_id);
						}
					},
					WorkerMessage::NewJob(job_id, state) => {
						worker_state.new_job(job_id, state);
					},
					WorkerMessage::Batch(_, _) => {},
				}
			}
		}
	}
}

impl<F> WorkerConfig for ApprovalWorkerConfig<F>
where
	F: overseer::ApprovalDistributionSenderTrait,
{
	type WorkItem = ApprovalWorkerMessage;
	type Worker = ApprovalWorkerHandle<F>;
	type JobState = ApprovalContext;
	type ChannelCapacity = ConstU32<4096>;
	type PoolCapacity = ConstU32<8>;

	fn new_worker(&mut self) -> ApprovalWorkerHandle<F> {
		let (to_worker, from_pool) = Self::new_worker_channel();

		tokio::spawn(worker_loop(from_pool, self.sender.clone(), self.metrics.clone()));

		ApprovalWorkerHandle { sender: to_worker, id: self.next_id() }
	}
}

// A worker context definition.
// It contains per candidate state for book keeping (spam protection) and distributing
// assignments and approvals.
#[derive(Debug, Clone)]
pub struct ApprovalContext;
