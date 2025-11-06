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

//! The Statement Distribution Subsystem.
//!
//! This is responsible for distributing signed statements about candidate
//! validity among validators.

#![warn(missing_docs)]

use error::FatalResult;
use std::time::Duration;

use polkadot_node_network_protocol::request_response::{
	v2::AttestedCandidateRequest, IncomingRequestReceiver,
};
use polkadot_node_subsystem::{
	messages::StatementDistributionMessage, overseer, ActiveLeavesUpdate, FromOrchestra,
	OverseerSignal, SpawnedSubsystem, SubsystemError,
};
use polkadot_node_subsystem_util::reputation::{ReputationAggregator, REPUTATION_CHANGE_INTERVAL};

use futures::{channel::mpsc, prelude::*};
use sp_keystore::KeystorePtr;

use fatality::Nested;

mod error;
pub use error::{Error, FatalError, JfyiError, Result};

/// Metrics for the statement distribution
pub(crate) mod metrics;
use metrics::Metrics;

mod v2;

const LOG_TARGET: &str = "parachain::statement-distribution";

/// The statement distribution subsystem.
pub struct StatementDistributionSubsystem {
	/// Pointer to a keystore, which is required for determining this node's validator index.
	keystore: KeystorePtr,
	/// Receiver for incoming candidate requests.
	req_receiver: Option<IncomingRequestReceiver<AttestedCandidateRequest>>,
	/// Prometheus metrics
	metrics: Metrics,
	/// Aggregated reputation change
	reputation: ReputationAggregator,
}

#[overseer::subsystem(StatementDistribution, error=SubsystemError, prefix=self::overseer)]
impl<Context> StatementDistributionSubsystem {
	fn start(self, ctx: Context) -> SpawnedSubsystem {
		// Swallow error because failure is fatal to the node and we log with more precision
		// within `run`.
		SpawnedSubsystem {
			name: "statement-distribution-subsystem",
			future: self
				.run(ctx)
				.map_err(|e| SubsystemError::with_origin("statement-distribution", e))
				.boxed(),
		}
	}
}

/// Messages to be handled in this subsystem.
enum MuxedMessage {
	/// Messages from other subsystems.
	Subsystem(FatalResult<FromOrchestra<StatementDistributionMessage>>),
	/// Messages from candidate responder background task.
	Responder(Option<v2::ResponderMessage>),
	/// Messages from answered requests.
	Response(v2::UnhandledResponse),
	/// Message that a request is ready to be retried. This just acts as a signal that we should
	/// dispatch all pending requests again.
	RetryRequest(()),
}

#[overseer::contextbounds(StatementDistribution, prefix = self::overseer)]
impl MuxedMessage {
	async fn receive<Context>(
		ctx: &mut Context,
		state: &mut v2::State,
		from_responder: &mut mpsc::Receiver<v2::ResponderMessage>,
	) -> MuxedMessage {
		let (request_manager, response_manager) = state.request_and_response_managers();
		// We are only fusing here to make `select` happy, in reality we will quit if one of those
		// streams end:
		let from_orchestra = ctx.recv().fuse();
		let from_responder = from_responder.next();
		let receive_response = v2::receive_response(response_manager).fuse();
		let retry_request = v2::next_retry(request_manager).fuse();
		futures::pin_mut!(from_orchestra, from_responder, receive_response, retry_request,);
		futures::select! {
			msg = from_orchestra => MuxedMessage::Subsystem(msg.map_err(FatalError::SubsystemReceive)),
			msg = from_responder => MuxedMessage::Responder(msg),
			msg = receive_response => MuxedMessage::Response(msg),
			msg = retry_request => MuxedMessage::RetryRequest(msg),
		}
	}
}

#[overseer::contextbounds(StatementDistribution, prefix = self::overseer)]
impl StatementDistributionSubsystem {
	/// Create a new Statement Distribution Subsystem
	pub fn new(
		keystore: KeystorePtr,
		req_receiver: IncomingRequestReceiver<AttestedCandidateRequest>,
		metrics: Metrics,
	) -> Self {
		Self { keystore, req_receiver: Some(req_receiver), metrics, reputation: Default::default() }
	}

	async fn run<Context>(self, ctx: Context) -> std::result::Result<(), FatalError> {
		self.run_inner(ctx, REPUTATION_CHANGE_INTERVAL).await
	}

	async fn run_inner<Context>(
		mut self,
		mut ctx: Context,
		reputation_interval: Duration,
	) -> std::result::Result<(), FatalError> {
		let new_reputation_delay = || futures_timer::Delay::new(reputation_interval).fuse();
		let mut reputation_delay = new_reputation_delay();

		let mut state = crate::v2::State::new(self.keystore.clone());

		// Sender/receiver for getting news from our candidate responder task.
		let (res_sender, mut res_receiver) = mpsc::channel(1);

		ctx.spawn(
			"candidate-responder",
			v2::respond_task(
				self.req_receiver.take().expect("Mandatory argument to new. qed"),
				res_sender.clone(),
				self.metrics.clone(),
			)
			.boxed(),
		)
		.map_err(FatalError::SpawnTask)?;

		loop {
			// Wait for the next message.
			let message = futures::select! {
				_ = reputation_delay => {
					self.reputation.send(ctx.sender()).await;
					reputation_delay = new_reputation_delay();
					continue
				},
				message = MuxedMessage::receive(
					&mut ctx,
					&mut state,
					&mut res_receiver,
				).fuse() => {
					message
				}
			};

			match message {
				MuxedMessage::Subsystem(result) => {
					let result = self.handle_subsystem_message(&mut ctx, &mut state, result?).await;
					match result.into_nested()? {
						Ok(true) => break,
						Ok(false) => {},
						Err(jfyi) => gum::debug!(target: LOG_TARGET, error = ?jfyi),
					}
				},
				MuxedMessage::Responder(result) => {
					v2::answer_request(
						&mut state,
						result.ok_or(FatalError::RequesterReceiverFinished)?,
					);
				},
				MuxedMessage::Response(result) => {
					v2::handle_response(
						&mut ctx,
						&mut state,
						result,
						&mut self.reputation,
						&self.metrics,
					)
					.await;
				},
				MuxedMessage::RetryRequest(()) => {
					// A pending request is ready to retry. This is only a signal to call
					// `dispatch_requests` again.
					()
				},
			};

			v2::dispatch_requests(&mut ctx, &mut state).await;
		}
		Ok(())
	}

	async fn handle_subsystem_message<Context>(
		&mut self,
		ctx: &mut Context,
		state: &mut v2::State,
		message: FromOrchestra<StatementDistributionMessage>,
	) -> Result<bool> {
		let metrics = &self.metrics;

		match message {
			FromOrchestra::Signal(OverseerSignal::ActiveLeaves(ActiveLeavesUpdate {
				activated,
				deactivated,
			})) => {
				let _timer = metrics.time_active_leaves_update();

				if let Some(ref activated) = activated {
					let res =
						v2::handle_active_leaves_update(ctx, state, activated, &metrics).await;
					// Regardless of the result of leaf activation, we always prune before
					// handling it to avoid leaks.
					v2::handle_deactivate_leaves(state, &deactivated);
					res?;
				} else {
					v2::handle_deactivate_leaves(state, &deactivated);
				}
			},
			FromOrchestra::Signal(OverseerSignal::BlockFinalized(..)) => {
				// do nothing
			},
			FromOrchestra::Signal(OverseerSignal::Conclude) => return Ok(true),
			FromOrchestra::Communication { msg } => match msg {
				StatementDistributionMessage::Share(relay_parent, statement) => {
					let _timer = metrics.time_share();

					v2::share_local_statement(
						ctx,
						state,
						relay_parent,
						statement,
						&mut self.reputation,
						&self.metrics,
					)
					.await?;
				},
				StatementDistributionMessage::NetworkBridgeUpdate(event) => {
					v2::handle_network_update(
						ctx,
						state,
						event,
						&mut self.reputation,
						&self.metrics,
					)
					.await;
				},
				StatementDistributionMessage::Backed(candidate_hash) => {
					crate::v2::handle_backed_candidate_message(
						ctx,
						state,
						candidate_hash,
						&self.metrics,
					)
					.await;
				},
			},
		}
		Ok(false)
	}
}
