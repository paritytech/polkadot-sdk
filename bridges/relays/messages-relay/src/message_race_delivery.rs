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

//! Message delivery race delivers proof-of-messages from lane.source to lane.target.

use crate::message_lane::{MessageLane, SourceHeaderIdOf, TargetHeaderIdOf};
use crate::message_lane_loop::{
	SourceClient as MessageLaneSourceClient, SourceClientState, TargetClient as MessageLaneTargetClient,
	TargetClientState,
};
use crate::message_race_loop::{ClientNonces, MessageRace, RaceState, RaceStrategy, SourceClient, TargetClient};
use crate::message_race_strategy::BasicStrategy;
use crate::metrics::MessageLaneLoopMetrics;

use async_trait::async_trait;
use futures::stream::FusedStream;
use num_traits::CheckedSub;
use relay_utils::FailedClient;
use std::{marker::PhantomData, ops::RangeInclusive, time::Duration};

/// Maximal number of messages to relay in single transaction.
const MAX_MESSAGES_TO_RELAY_IN_SINGLE_TX: u32 = 4;

/// Run message delivery race.
pub async fn run<P: MessageLane>(
	source_client: impl MessageLaneSourceClient<P>,
	source_state_updates: impl FusedStream<Item = SourceClientState<P>>,
	target_client: impl MessageLaneTargetClient<P>,
	target_state_updates: impl FusedStream<Item = TargetClientState<P>>,
	stall_timeout: Duration,
	metrics_msg: Option<MessageLaneLoopMetrics>,
	max_unconfirmed_nonces_at_target: P::MessageNonce,
) -> Result<(), FailedClient> {
	crate::message_race_loop::run(
		MessageDeliveryRaceSource {
			client: source_client,
			metrics_msg: metrics_msg.clone(),
			_phantom: Default::default(),
		},
		source_state_updates,
		MessageDeliveryRaceTarget {
			client: target_client,
			metrics_msg,
			_phantom: Default::default(),
		},
		target_state_updates,
		stall_timeout,
		MessageDeliveryStrategy::<P> {
			max_unconfirmed_nonces_at_target,
			source_nonces: None,
			target_nonces: None,
			strategy: BasicStrategy::new(MAX_MESSAGES_TO_RELAY_IN_SINGLE_TX.into()),
		},
	)
	.await
}

/// Message delivery race.
struct MessageDeliveryRace<P>(std::marker::PhantomData<P>);

impl<P: MessageLane> MessageRace for MessageDeliveryRace<P> {
	type SourceHeaderId = SourceHeaderIdOf<P>;
	type TargetHeaderId = TargetHeaderIdOf<P>;

	type MessageNonce = P::MessageNonce;
	type Proof = P::MessagesProof;

	fn source_name() -> String {
		format!("{}::MessagesDelivery", P::SOURCE_NAME)
	}

	fn target_name() -> String {
		format!("{}::MessagesDelivery", P::TARGET_NAME)
	}
}

/// Message delivery race source, which is a source of the lane.
struct MessageDeliveryRaceSource<P: MessageLane, C> {
	client: C,
	metrics_msg: Option<MessageLaneLoopMetrics>,
	_phantom: PhantomData<P>,
}

#[async_trait]
impl<P, C> SourceClient<MessageDeliveryRace<P>> for MessageDeliveryRaceSource<P, C>
where
	P: MessageLane,
	C: MessageLaneSourceClient<P>,
{
	type Error = C::Error;
	type ProofParameters = bool;

	async fn nonces(
		&self,
		at_block: SourceHeaderIdOf<P>,
	) -> Result<(SourceHeaderIdOf<P>, ClientNonces<P::MessageNonce>), Self::Error> {
		let (at_block, latest_generated_nonce) = self.client.latest_generated_nonce(at_block).await?;
		let (at_block, latest_confirmed_nonce) = self.client.latest_confirmed_received_nonce(at_block).await?;

		if let Some(metrics_msg) = self.metrics_msg.as_ref() {
			metrics_msg.update_source_latest_generated_nonce::<P>(latest_generated_nonce);
			metrics_msg.update_source_latest_confirmed_nonce::<P>(latest_confirmed_nonce);
		}

		Ok((
			at_block,
			ClientNonces {
				latest_nonce: latest_generated_nonce,
				confirmed_nonce: Some(latest_confirmed_nonce),
			},
		))
	}

	async fn generate_proof(
		&self,
		at_block: SourceHeaderIdOf<P>,
		nonces: RangeInclusive<P::MessageNonce>,
		proof_parameters: Self::ProofParameters,
	) -> Result<(SourceHeaderIdOf<P>, RangeInclusive<P::MessageNonce>, P::MessagesProof), Self::Error> {
		let outbound_state_proof_required = proof_parameters;
		self.client
			.prove_messages(at_block, nonces, outbound_state_proof_required)
			.await
	}
}

/// Message delivery race target, which is a target of the lane.
struct MessageDeliveryRaceTarget<P: MessageLane, C> {
	client: C,
	metrics_msg: Option<MessageLaneLoopMetrics>,
	_phantom: PhantomData<P>,
}

#[async_trait]
impl<P, C> TargetClient<MessageDeliveryRace<P>> for MessageDeliveryRaceTarget<P, C>
where
	P: MessageLane,
	C: MessageLaneTargetClient<P>,
{
	type Error = C::Error;

	async fn nonces(
		&self,
		at_block: TargetHeaderIdOf<P>,
	) -> Result<(TargetHeaderIdOf<P>, ClientNonces<P::MessageNonce>), Self::Error> {
		let (at_block, latest_received_nonce) = self.client.latest_received_nonce(at_block).await?;
		let (at_block, latest_confirmed_nonce) = self.client.latest_confirmed_received_nonce(at_block).await?;

		if let Some(metrics_msg) = self.metrics_msg.as_ref() {
			metrics_msg.update_target_latest_received_nonce::<P>(latest_received_nonce);
			metrics_msg.update_target_latest_confirmed_nonce::<P>(latest_confirmed_nonce);
		}

		Ok((
			at_block,
			ClientNonces {
				latest_nonce: latest_received_nonce,
				confirmed_nonce: Some(latest_confirmed_nonce),
			},
		))
	}

	async fn submit_proof(
		&self,
		generated_at_block: SourceHeaderIdOf<P>,
		nonces: RangeInclusive<P::MessageNonce>,
		proof: P::MessagesProof,
	) -> Result<RangeInclusive<P::MessageNonce>, Self::Error> {
		self.client
			.submit_messages_proof(generated_at_block, nonces, proof)
			.await
	}
}

/// Messages delivery strategy.
struct MessageDeliveryStrategy<P: MessageLane> {
	/// Maximal unconfirmed nonces at target client.
	max_unconfirmed_nonces_at_target: P::MessageNonce,
	/// Latest nonces from the source client.
	source_nonces: Option<ClientNonces<P::MessageNonce>>,
	/// Target nonces from the source client.
	target_nonces: Option<ClientNonces<P::MessageNonce>>,
	/// Basic delivery strategy.
	strategy: MessageDeliveryStrategyBase<P>,
}

type MessageDeliveryStrategyBase<P> = BasicStrategy<
	<P as MessageLane>::SourceHeaderNumber,
	<P as MessageLane>::SourceHeaderHash,
	<P as MessageLane>::TargetHeaderNumber,
	<P as MessageLane>::TargetHeaderHash,
	<P as MessageLane>::MessageNonce,
	<P as MessageLane>::MessagesProof,
>;

impl<P: MessageLane> RaceStrategy<SourceHeaderIdOf<P>, TargetHeaderIdOf<P>, P::MessageNonce, P::MessagesProof>
	for MessageDeliveryStrategy<P>
{
	type ProofParameters = bool;

	fn is_empty(&self) -> bool {
		self.strategy.is_empty()
	}

	fn source_nonces_updated(&mut self, at_block: SourceHeaderIdOf<P>, nonces: ClientNonces<P::MessageNonce>) {
		self.source_nonces = Some(nonces.clone());
		self.strategy.source_nonces_updated(at_block, nonces)
	}

	fn target_nonces_updated(
		&mut self,
		nonces: ClientNonces<P::MessageNonce>,
		race_state: &mut RaceState<SourceHeaderIdOf<P>, TargetHeaderIdOf<P>, P::MessageNonce, P::MessagesProof>,
	) {
		self.target_nonces = Some(nonces.clone());
		self.strategy.target_nonces_updated(nonces, race_state)
	}

	fn select_nonces_to_deliver(
		&mut self,
		race_state: &RaceState<SourceHeaderIdOf<P>, TargetHeaderIdOf<P>, P::MessageNonce, P::MessagesProof>,
	) -> Option<(RangeInclusive<P::MessageNonce>, Self::ProofParameters)> {
		const CONFIRMED_NONCE_PROOF: &str = "\
			ClientNonces are crafted by MessageDeliveryRace(Source|Target);\
			MessageDeliveryRace(Source|Target) always fills confirmed_nonce field;\
			qed";

		let source_nonces = self.source_nonces.as_ref()?;
		let target_nonces = self.target_nonces.as_ref()?;

		// There's additional condition in the message delivery race: target would reject messages
		// if there are too much unconfirmed messages at the inbound lane.

		// https://github.com/paritytech/parity-bridges-common/issues/432
		// TODO: message lane loop works with finalized blocks only, but we're submitting transactions that
		// are updating best block (which may not be finalized yet). So all decisions that are made below
		// may be outdated. This needs to be changed - all logic here must be built on top of best blocks.

		// The receiving race is responsible to deliver confirmations back to the source chain. So if
		// there's a lot of unconfirmed messages, let's wait until it'll be able to do its job.
		let latest_received_nonce_at_target = target_nonces.latest_nonce;
		let latest_confirmed_nonce_at_source = source_nonces.confirmed_nonce.expect(CONFIRMED_NONCE_PROOF);
		let confirmations_missing = latest_received_nonce_at_target.checked_sub(&latest_confirmed_nonce_at_source);
		match confirmations_missing {
			Some(confirmations_missing) if confirmations_missing > self.max_unconfirmed_nonces_at_target => {
				log::debug!(
					target: "bridge",
					"Cannot deliver any more messages from {} to {}. Too many unconfirmed nonces \
					at target: target.latest_received={:?}, source.latest_confirmed={:?}, max={:?}",
					MessageDeliveryRace::<P>::source_name(),
					MessageDeliveryRace::<P>::target_name(),
					latest_received_nonce_at_target,
					latest_confirmed_nonce_at_source,
					self.max_unconfirmed_nonces_at_target,
				);

				return None;
			}
			_ => (),
		}

		// If we're here, then the confirmations race did it job && sending side now knows that messages
		// have been delivered. Now let's select nonces that we want to deliver.
		let selected_nonces = self.strategy.select_nonces_to_deliver(race_state)?.0;

		// Ok - we have new nonces to deliver. But target may still reject new messages, because we haven't
		// notified it that (some) messages have been confirmed. So we may want to include updated
		// `source.latest_confirmed` in the proof.
		//
		// Important note: we're including outbound state lane proof whenever there are unconfirmed nonces
		// on the target chain. Other strategy is to include it only if it's absolutely necessary.
		let latest_confirmed_nonce_at_target = target_nonces.confirmed_nonce.expect(CONFIRMED_NONCE_PROOF);
		let outbound_state_proof_required = latest_confirmed_nonce_at_target < latest_confirmed_nonce_at_source;

		// https://github.com/paritytech/parity-bridges-common/issues/432
		// https://github.com/paritytech/parity-bridges-common/issues/433
		// TODO: number of messages must be no larger than:
		// `max_unconfirmed_nonces_at_target - (latest_received_nonce_at_target - latest_confirmed_nonce_at_target)`

		Some((selected_nonces, outbound_state_proof_required))
	}
}
