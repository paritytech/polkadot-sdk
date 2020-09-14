// Copyright 2019-2020 Parity Technologies (UK) Ltd.
// This file is part of Parity Bridges Common.

// Parity Bridges Common is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Parity Bridges Common is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

//! Loop that is serving single race within message lane. This could be
//! message delivery race, receiving confirmations race or processing
//! confirmations race.
//!
//! The idea of the race is simple - we have `nonce`-s on source and target
//! nodes. We're trying to prove that the source node has this nonce (and
//! associated data - like messages, lane state, etc) to the target node by
//! generating and submitting proof.

// Until there'll be actual message-lane in the runtime.
#![allow(dead_code)]

use crate::message_lane_loop::ClientState;
use crate::utils::{process_future_result, retry_backoff, FailedClient, MaybeConnectionError};

use async_trait::async_trait;
use futures::{
	future::FutureExt,
	stream::{FusedStream, StreamExt},
};
use std::{
	fmt::Debug,
	ops::RangeInclusive,
	time::{Duration, Instant},
};

/// One of races within lane.
pub trait MessageRace {
	/// Header id of the race source.
	type SourceHeaderId: Debug + Clone + PartialEq;
	/// Header id of the race source.
	type TargetHeaderId: Debug + Clone + PartialEq;

	/// Message nonce used in the race.
	type MessageNonce: Debug + Clone;
	/// Proof that is generated and delivered in this race.
	type Proof: Clone;

	/// Name of the race source.
	fn source_name() -> String;
	/// Name of the race target.
	fn target_name() -> String;
}

/// State of race source client.
type SourceClientState<P> = ClientState<<P as MessageRace>::SourceHeaderId, <P as MessageRace>::TargetHeaderId>;

/// State of race target client.
type TargetClientState<P> = ClientState<<P as MessageRace>::TargetHeaderId, <P as MessageRace>::SourceHeaderId>;

/// One of message lane clients, which is source client for the race.
#[async_trait(?Send)]
pub trait SourceClient<P: MessageRace> {
	/// Type of error this clients returns.
	type Error: std::fmt::Debug + MaybeConnectionError;

	/// Return latest nonce that is known to the source client.
	async fn latest_nonce(
		&self,
		at_block: P::SourceHeaderId,
	) -> Result<(P::SourceHeaderId, P::MessageNonce), Self::Error>;
	/// Generate proof for delivering to the target client.
	async fn generate_proof(
		&self,
		at_block: P::SourceHeaderId,
		nonces: RangeInclusive<P::MessageNonce>,
	) -> Result<(P::SourceHeaderId, RangeInclusive<P::MessageNonce>, P::Proof), Self::Error>;
}

/// One of message lane clients, which is target client for the race.
#[async_trait(?Send)]
pub trait TargetClient<P: MessageRace> {
	/// Type of error this clients returns.
	type Error: std::fmt::Debug + MaybeConnectionError;

	/// Return latest nonce that is known to the target client.
	async fn latest_nonce(
		&self,
		at_block: P::TargetHeaderId,
	) -> Result<(P::TargetHeaderId, P::MessageNonce), Self::Error>;
	/// Submit proof to the target client.
	async fn submit_proof(
		&self,
		generated_at_block: P::SourceHeaderId,
		nonces: RangeInclusive<P::MessageNonce>,
		proof: P::Proof,
	) -> Result<RangeInclusive<P::MessageNonce>, Self::Error>;
}

/// Race strategy.
pub trait RaceStrategy<SourceHeaderId, TargetHeaderId, MessageNonce, Proof> {
	/// Should return true if nothing has to be synced.
	fn is_empty(&self) -> bool;
	/// Called when latest nonce is updated at source node of the race.
	fn source_nonce_updated(&mut self, at_block: SourceHeaderId, nonce: MessageNonce);
	/// Called when latest nonce is updated at target node of the race.
	fn target_nonce_updated(
		&mut self,
		nonce: MessageNonce,
		race_state: &mut RaceState<SourceHeaderId, TargetHeaderId, MessageNonce, Proof>,
	);
	/// Should return `Some(nonces)` if we need to deliver proof of `nonces` (and associated
	/// data) from source to target node.
	fn select_nonces_to_deliver(
		&mut self,
		race_state: &RaceState<SourceHeaderId, TargetHeaderId, MessageNonce, Proof>,
	) -> Option<RangeInclusive<MessageNonce>>;
}

/// State of the race.
pub struct RaceState<SourceHeaderId, TargetHeaderId, MessageNonce, Proof> {
	/// Source state, if known.
	pub source_state: Option<ClientState<SourceHeaderId, TargetHeaderId>>,
	/// Target state, if known.
	pub target_state: Option<ClientState<TargetHeaderId, SourceHeaderId>>,
	/// Range of nonces that we have selected to submit.
	pub nonces_to_submit: Option<(SourceHeaderId, RangeInclusive<MessageNonce>, Proof)>,
	/// Range of nonces that is currently submitted.
	pub nonces_submitted: Option<RangeInclusive<MessageNonce>>,
}

/// Run race loop until connection with target or source node is lost.
pub async fn run<P: MessageRace>(
	race_source: impl SourceClient<P>,
	race_source_updated: impl FusedStream<Item = SourceClientState<P>>,
	race_target: impl TargetClient<P>,
	race_target_updated: impl FusedStream<Item = TargetClientState<P>>,
	stall_timeout: Duration,
	mut strategy: impl RaceStrategy<P::SourceHeaderId, P::TargetHeaderId, P::MessageNonce, P::Proof>,
) -> Result<(), FailedClient> {
	let mut race_state = RaceState::default();
	let mut stall_countdown = Instant::now();

	let mut source_retry_backoff = retry_backoff();
	let mut source_client_is_online = true;
	let mut source_latest_nonce_required = false;
	let source_latest_nonce = futures::future::Fuse::terminated();
	let source_generate_proof = futures::future::Fuse::terminated();
	let source_go_offline_future = futures::future::Fuse::terminated();

	let mut target_retry_backoff = retry_backoff();
	let mut target_client_is_online = true;
	let mut target_latest_nonce_required = false;
	let target_latest_nonce = futures::future::Fuse::terminated();
	let target_submit_proof = futures::future::Fuse::terminated();
	let target_go_offline_future = futures::future::Fuse::terminated();

	futures::pin_mut!(
		race_source_updated,
		source_latest_nonce,
		source_generate_proof,
		source_go_offline_future,
		race_target_updated,
		target_latest_nonce,
		target_submit_proof,
		target_go_offline_future,
	);

	loop {
		futures::select! {
			// when headers ids are updated
			source_state = race_source_updated.next() => {
				if let Some(source_state) = source_state {
					if race_state.source_state.as_ref() != Some(&source_state) {
						source_latest_nonce_required = true;
						race_state.source_state = Some(source_state);
					}
				}
			},
			target_state = race_target_updated.next() => {
				if let Some(target_state) = target_state {
					if race_state.target_state.as_ref() != Some(&target_state) {
						target_latest_nonce_required = true;
						race_state.target_state = Some(target_state);
					}
				}
			},

			// when nonces are updated
			latest_nonce = source_latest_nonce => {
				source_latest_nonce_required = false;

				source_client_is_online = process_future_result(
					latest_nonce,
					&mut source_retry_backoff,
					|(at_block, latest_nonce)| {
						log::debug!(
							target: "bridge",
							"Received latest nonce from {}: {:?}",
							P::source_name(),
							latest_nonce,
						);

						strategy.source_nonce_updated(at_block, latest_nonce);
					},
					&mut source_go_offline_future,
					|delay| async_std::task::sleep(delay),
					|| format!("Error retrieving latest nonce from {}", P::source_name()),
				).fail_if_connection_error(FailedClient::Source)?;
			},
			latest_nonce = target_latest_nonce => {
				target_latest_nonce_required = false;

				target_client_is_online = process_future_result(
					latest_nonce,
					&mut target_retry_backoff,
					|(_, latest_nonce)| {
						log::debug!(
							target: "bridge",
							"Received latest nonce from {}: {:?}",
							P::target_name(),
							latest_nonce,
						);

						strategy.target_nonce_updated(latest_nonce, &mut race_state);
					},
					&mut target_go_offline_future,
					|delay| async_std::task::sleep(delay),
					|| format!("Error retrieving latest nonce from {}", P::target_name()),
				).fail_if_connection_error(FailedClient::Target)?;
			},

			// proof generation and submission
			proof = source_generate_proof => {
				source_client_is_online = process_future_result(
					proof,
					&mut source_retry_backoff,
					|(at_block, nonces_range, proof)| {
						log::debug!(
							target: "bridge",
							"Received proof for nonces in range {:?} from {}",
							nonces_range,
							P::source_name(),
						);

						race_state.nonces_to_submit = Some((at_block, nonces_range, proof));
					},
					&mut source_go_offline_future,
					|delay| async_std::task::sleep(delay),
					|| format!("Error generating proof at {}", P::source_name()),
				).fail_if_connection_error(FailedClient::Source)?;
			},
			proof_submit_result = target_submit_proof => {
				target_client_is_online = process_future_result(
					proof_submit_result,
					&mut target_retry_backoff,
					|nonces_range| {
						log::debug!(
							target: "bridge",
							"Successfully submitted proof of nonces {:?} to {}",
							nonces_range,
							P::target_name(),
						);

						race_state.nonces_to_submit = None;
						race_state.nonces_submitted = Some(nonces_range);
					},
					&mut target_go_offline_future,
					|delay| async_std::task::sleep(delay),
					|| format!("Error submitting proof {}", P::target_name()),
				).fail_if_connection_error(FailedClient::Target)?;
			}
		}

		if stall_countdown.elapsed() > stall_timeout {
			return Err(FailedClient::Both);
		} else if race_state.nonces_to_submit.is_none() && race_state.nonces_submitted.is_none() && strategy.is_empty()
		{
			stall_countdown = Instant::now();
		}

		if source_client_is_online {
			source_client_is_online = false;

			let nonces_to_deliver = race_state.source_state.as_ref().and_then(|source_state| {
				strategy
					.select_nonces_to_deliver(&race_state)
					.map(|nonces_range| (source_state.best_self.clone(), nonces_range))
			});

			if let Some((at_block, nonces_range)) = nonces_to_deliver {
				log::debug!(
					target: "bridge",
					"Asking {} to prove nonces in range {:?}",
					P::source_name(),
					nonces_range,
				);
				source_generate_proof.set(race_source.generate_proof(at_block, nonces_range).fuse());
			} else if source_latest_nonce_required {
				log::debug!(target: "bridge", "Asking {} about latest generated message nonce", P::source_name());
				let at_block = race_state
					.source_state
					.as_ref()
					.expect("source_latest_nonce_required is only true when source_state is Some; qed")
					.best_self
					.clone();
				source_latest_nonce.set(race_source.latest_nonce(at_block).fuse());
			} else {
				source_client_is_online = true;
			}
		}

		if target_client_is_online {
			target_client_is_online = false;

			if let Some((at_block, nonces_range, proof)) = race_state.nonces_to_submit.as_ref() {
				log::debug!(
					target: "bridge",
					"Going to submit proof of messages in range {:?} to {} node",
					nonces_range,
					P::target_name(),
				);
				target_submit_proof.set(
					race_target
						.submit_proof(at_block.clone(), nonces_range.clone(), proof.clone())
						.fuse(),
				);
			}
			if target_latest_nonce_required {
				log::debug!(target: "bridge", "Asking {} about latest nonce", P::target_name());
				let at_block = race_state
					.target_state
					.as_ref()
					.expect("target_latest_nonce_required is only true when target_state is Some; qed")
					.best_self
					.clone();
				target_latest_nonce.set(race_target.latest_nonce(at_block).fuse());
			} else {
				target_client_is_online = true;
			}
		}
	}
}

impl<SourceHeaderId, TargetHeaderId, MessageNonce, Proof> Default
	for RaceState<SourceHeaderId, TargetHeaderId, MessageNonce, Proof>
{
	fn default() -> Self {
		RaceState {
			source_state: None,
			target_state: None,
			nonces_to_submit: None,
			nonces_submitted: None,
		}
	}
}
