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

use crate::{
	task::{RecoveryParams, RecoveryStrategy, State},
	ErasureTask, PostRecoveryCheck, LOG_TARGET,
};

use polkadot_node_network_protocol::request_response::{
	self as req_res, outgoing::RequestError, OutgoingRequest, Recipient, Requests,
};
use polkadot_node_primitives::AvailableData;
use polkadot_node_subsystem::{messages::NetworkBridgeTxMessage, overseer, RecoveryError};
use polkadot_primitives::ValidatorIndex;
use sc_network::{IfDisconnected, OutboundFailure, RequestFailure};

use futures::{channel::oneshot, SinkExt};
use rand::seq::SliceRandom;

/// Parameters specific to the `FetchFull` strategy.
pub struct FetchFullParams {
	/// Validators that will be used for fetching the data.
	pub validators: Vec<ValidatorIndex>,
}

/// `RecoveryStrategy` that sequentially tries to fetch the full `AvailableData` from
/// already-connected validators in the configured validator set.
pub struct FetchFull {
	params: FetchFullParams,
}

impl FetchFull {
	/// Create a new `FetchFull` recovery strategy.
	pub fn new(mut params: FetchFullParams) -> Self {
		params.validators.shuffle(&mut rand::thread_rng());
		Self { params }
	}
}

#[async_trait::async_trait]
impl<Sender: overseer::AvailabilityRecoverySenderTrait> RecoveryStrategy<Sender> for FetchFull {
	fn display_name(&self) -> &'static str {
		"Full recovery from backers"
	}

	fn strategy_type(&self) -> &'static str {
		"full_from_backers"
	}

	async fn run(
		mut self: Box<Self>,
		_: &mut State,
		sender: &mut Sender,
		common_params: &RecoveryParams,
	) -> Result<AvailableData, RecoveryError> {
		let strategy_type = RecoveryStrategy::<Sender>::strategy_type(&*self);

		loop {
			// Pop the next validator.
			let validator_index =
				self.params.validators.pop().ok_or_else(|| RecoveryError::Unavailable)?;

			// Request data.
			let (req, response) = OutgoingRequest::new(
				Recipient::Authority(
					common_params.validator_authority_keys[validator_index.0 as usize].clone(),
				),
				req_res::v1::AvailableDataFetchingRequest {
					candidate_hash: common_params.candidate_hash,
				},
			);

			sender
				.send_message(NetworkBridgeTxMessage::SendRequests(
					vec![Requests::AvailableDataFetchingV1(req)],
					IfDisconnected::ImmediateError,
				))
				.await;

			common_params.metrics.on_full_request_issued();

			match response.await {
				Ok(req_res::v1::AvailableDataFetchingResponse::AvailableData(data)) => {
					let recovery_duration =
						common_params.metrics.time_erasure_recovery(strategy_type);
					let maybe_data = match common_params.post_recovery_check {
						PostRecoveryCheck::Reencode => {
							let (reencode_tx, reencode_rx) = oneshot::channel();
							let mut erasure_task_tx = common_params.erasure_task_tx.clone();

							erasure_task_tx
								.send(ErasureTask::Reencode(
									common_params.n_validators,
									common_params.erasure_root,
									data,
									reencode_tx,
								))
								.await
								.map_err(|_| RecoveryError::ChannelClosed)?;

							reencode_rx.await.map_err(|_| RecoveryError::ChannelClosed)?
						},
						PostRecoveryCheck::PovHash =>
							(data.pov.hash() == common_params.pov_hash).then_some(data),
					};

					match maybe_data {
						Some(data) => {
							gum::trace!(
								target: LOG_TARGET,
								candidate_hash = ?common_params.candidate_hash,
								"Received full data",
							);

							common_params.metrics.on_full_request_succeeded();
							return Ok(data)
						},
						None => {
							common_params.metrics.on_full_request_invalid();
							recovery_duration.map(|rd| rd.stop_and_discard());

							gum::debug!(
								target: LOG_TARGET,
								candidate_hash = ?common_params.candidate_hash,
								?validator_index,
								"Invalid data response",
							);

							// it doesn't help to report the peer with req/res.
							// we'll try the next backer.
						},
					}
				},
				Ok(req_res::v1::AvailableDataFetchingResponse::NoSuchData) => {
					common_params.metrics.on_full_request_no_such_data();
				},
				Err(e) => {
					match &e {
						RequestError::Canceled(_) => common_params.metrics.on_full_request_error(),
						RequestError::InvalidResponse(_) =>
							common_params.metrics.on_full_request_invalid(),
						RequestError::NetworkError(req_failure) => {
							if let RequestFailure::Network(OutboundFailure::Timeout) = req_failure {
								common_params.metrics.on_full_request_timeout();
							} else {
								common_params.metrics.on_full_request_error();
							}
						},
					};
					gum::debug!(
						target: LOG_TARGET,
						candidate_hash = ?common_params.candidate_hash,
						?validator_index,
						err = ?e,
						"Error fetching full available data."
					);
				},
			}
		}
	}
}
