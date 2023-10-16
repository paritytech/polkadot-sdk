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
use polkadot_node_jaeger as jaeger;
use polkadot_node_network_protocol::{
	self as net_protocol,
	grid_topology::{RandomRouting, RequiredRouting, SessionGridTopologies, SessionGridTopology},
	peer_set::{ValidationVersion, MAX_NOTIFICATION_SIZE},
	v1 as protocol_v1, v2 as protocol_v2, PeerId, UnifiedReputationChange as Rep, Versioned,
	VersionedValidationProtocol, View,
};
use polkadot_node_primitives::approval::{
	AssignmentCert, BlockApprovalMeta, IndirectAssignmentCert, IndirectSignedApprovalVote,
};
use polkadot_node_subsystem_util::reputation::REPUTATION_CHANGE_INTERVAL;
use worker::state::MessageSource;

use polkadot_node_subsystem::{
	messages::{
		ApprovalCheckResult, ApprovalDistributionMessage, ApprovalVotingMessage,
		AssignmentCheckResult, NetworkBridgeEvent, NetworkBridgeTxMessage,
	},
	overseer, FromOrchestra, OverseerSignal, SpawnedSubsystem, SubsystemError,
};
use polkadot_primitives::{BlakeTwo256, HashT};
use rand::{CryptoRng, Rng, SeedableRng};
use std::time::Duration;
use worker::state::{MessageKind, MessageSubject};

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
		let mut state = State::default();

		// According to the docs of `rand`, this is a ChaCha12 RNG in practice
		// and will always be chosen for strong performance and security properties.
		let mut rng = rand::rngs::StdRng::from_entropy();
		self.run_inner(ctx, &mut state, REPUTATION_CHANGE_INTERVAL, &mut rng).await
	}

	/// Used for testing.
	async fn run_inner<Context>(
		self,
		mut ctx: Context,
		state: &mut State,
		reputation_interval: Duration,
		rng: &mut (impl CryptoRng + Rng),
	) {
		let new_reputation_delay = || futures_timer::Delay::new(reputation_interval).fuse();
		let mut reputation_delay = new_reputation_delay();

		loop {
			select! {
				_ = reputation_delay => {
					state.reputation.send(ctx.sender()).await;
					reputation_delay = new_reputation_delay();
				},
				message = ctx.recv().fuse() => {
					let message = match message {
						Ok(message) => message,
						Err(e) => {
							gum::debug!(target: LOG_TARGET, err = ?e, "Failed to receive a message from Overseer, exiting");
							return
						},
					};
					match message {
						FromOrchestra::Communication { msg } =>
							Self::handle_incoming(&mut ctx, state, msg, &self.metrics, rng).await,
						FromOrchestra::Signal(OverseerSignal::ActiveLeaves(update)) => {
							gum::trace!(target: LOG_TARGET, "active leaves signal (ignored)");
							// the relay chain blocks relevant to the approval subsystems
							// are those that are available, but not finalized yet
							// actived and deactivated heads hence are irrelevant to this subsystem, other than
							// for tracing purposes.
							if let Some(activated) = update.activated {
								let head = activated.hash;
								let approval_distribution_span =
									jaeger::PerLeafSpan::new(activated.span, "approval-distribution");
								state.spans.insert(head, approval_distribution_span);
							}
						},
						FromOrchestra::Signal(OverseerSignal::BlockFinalized(_hash, number)) => {
							gum::trace!(target: LOG_TARGET, number = %number, "finalized signal");
							state.handle_block_finalized(&mut ctx, &self.metrics, number).await;
						},
						FromOrchestra::Signal(OverseerSignal::Conclude) => return,
					}
				},
			}
		}
	}

	async fn handle_incoming<Context>(
		ctx: &mut Context,
		state: &mut State,
		msg: ApprovalDistributionMessage,
		metrics: &Metrics,
		rng: &mut (impl CryptoRng + Rng),
	) {
		match msg {
			ApprovalDistributionMessage::NetworkBridgeUpdate(event) => {
				state.handle_network_msg(ctx, metrics, event, rng).await;
			},
			ApprovalDistributionMessage::NewBlocks(metas) => {
				state.handle_new_blocks(ctx, metrics, metas, rng).await;
			},
			ApprovalDistributionMessage::DistributeAssignment(cert, candidate_index) => {
				gum::debug!(
					target: LOG_TARGET,
					"Distributing our assignment on candidate (block={}, index={})",
					cert.block_hash,
					candidate_index,
				);

				state
					.import_and_circulate_assignment(
						ctx,
						&metrics,
						MessageSource::Local,
						cert,
						candidate_index,
						rng,
					)
					.await;
			},
			ApprovalDistributionMessage::DistributeApproval(vote) => {
				gum::debug!(
					target: LOG_TARGET,
					"Distributing our approval vote on candidate (block={}, index={})",
					vote.block_hash,
					vote.candidate_index,
				);

				state
					.import_and_circulate_approval(ctx, metrics, MessageSource::Local, vote)
					.await;
			},
			ApprovalDistributionMessage::GetApprovalSignatures(indices, tx) => {
				let sigs = state.get_approval_signatures(indices);
				if let Err(_) = tx.send(sigs) {
					gum::debug!(
						target: LOG_TARGET,
						"Sending back approval signatures failed, oneshot got closed"
					);
				}
			},
			ApprovalDistributionMessage::ApprovalCheckingLagUpdate(lag) => {
				gum::debug!(target: LOG_TARGET, lag, "Received `ApprovalCheckingLagUpdate`");
				state.approval_checking_lag = lag;
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
