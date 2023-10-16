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
use async_trait::async_trait;
use bounded_collections::ConstU32;

use polkadot_node_subsystem::{
	messages::{
		network_bridge_event::NewGossipTopology, ApprovalCheckResult, ApprovalDistributionMessage,
		ApprovalVotingMessage, AssignmentCheckResult, NetworkBridgeEvent, NetworkBridgeTxMessage,
	},
	overseer, FromOrchestra, OverseerSignal, SpawnedSubsystem, SubsystemError,
};
use polkadot_node_subsystem_util::worker_pool::{
	ContextCookie, WorkContext, WorkerConfig, WorkerHandle, WorkerMessage, WorkerPool,
};
use polkadot_primitives::{
	BlockNumber, CandidateIndex, Hash, SessionIndex, ValidatorIndex, ValidatorSignature,
};
use tokio::sync::mpsc;

use polkadot_node_primitives::approval::{
	AssignmentCert, BlockApprovalMeta, IndirectAssignmentCert, IndirectSignedApprovalVote,
};

use rand::{CryptoRng, Rng, SeedableRng};
use std::{
	collections::{hash_map, BTreeMap, HashMap, HashSet, VecDeque},
	time::Duration,
};

use polkadot_node_network_protocol::{
	self as net_protocol,
	grid_topology::{RandomRouting, RequiredRouting, SessionGridTopologies, SessionGridTopology},
	peer_set::{ProtocolVersion, ValidationVersion, MAX_NOTIFICATION_SIZE},
	v1 as protocol_v1, v2 as protocol_v2, ObservedRole, OurView, PeerId,
	UnifiedReputationChange as Rep, Versioned, VersionedValidationProtocol, View,
};

use state::{MessageKind, MessageSubject};
pub mod state;

/// Approval work item definition.
#[derive(Clone, Debug)]
pub enum ApprovalWorkerMessage {
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
}

// Use `MessageSubject` as `ContextCookie`.
impl WorkContext for ApprovalWorkerMessage {
	fn id(&self) -> Option<ContextCookie> {
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
struct ApprovalWorkerConfig;

#[derive(Clone)]
struct ApprovalWorkerHandle(mpsc::Sender<WorkerMessage<ApprovalWorkerConfig>>);

#[async_trait]
impl WorkerHandle for ApprovalWorkerHandle {
	type Config = ApprovalWorkerConfig;

	async fn send(&self, message: WorkerMessage<Self::Config>) {
		let _ = self.0.send(message).await;
	}
}

async fn worker_loop<ApprovalWorkerConfig: WorkerConfig>(
	mut from_pool: mpsc::Receiver<WorkerMessage<ApprovalWorkerConfig>>,
) {
	let mut rng = rand::rngs::StdRng::from_entropy();
	let worker_state = state::ApprovalWorkerState::default();

	loop {
		if let Some(worker_message) = from_pool.recv().await {
			match worker_message {
				WorkerMessage::Queue(work_item) => {},
				WorkerMessage::PruneWork(_) => {},
				WorkerMessage::SetupContext(context) => {},
				WorkerMessage::Batch(_, _) => {},
			}
		} else {
			// channel closed, end worker.
			break
		}
	}
}

impl WorkerConfig for ApprovalWorkerConfig {
	type WorkItem = ApprovalWorkerMessage;
	type Worker = ApprovalWorkerHandle;
	type Context = ApprovalContext;
	type ChannelCapacity = ConstU32<4096>;
	type PoolCapacity = ConstU32<4>;

	fn new_worker(&mut self) -> ApprovalWorkerHandle {
		let (to_worker, mut from_pool) = Self::new_worker_channel();

		tokio::spawn(worker_loop(from_pool));

		ApprovalWorkerHandle(to_worker)
	}
}

// A worker context definition.
// It contains per candidate state for book keeping (spam protection) and distributing
// assignments and approvals.
#[derive(Debug, Clone)]
pub struct ApprovalContext {}
